//! Privileged TUN helper.
//!
//! The GUI never runs as root. It starts this binary through `pkexec`, which
//! asks polkit for the `slop.rumurule.tunnel` action, and the helper then owns
//! the root Xray process. It validates the configuration it is given, keeps a
//! private copy, forwards signals and removes whatever Xray leaves behind.

use std::{
    fs,
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpStream},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

use serde_json::Value;

/// Runtime directory owned by the helper.
const RUN_DIR: &str = "/run/rumurule";
/// Copy of the validated configuration handed to Xray.
const CONFIG_FILE: &str = "/run/rumurule/xray-tun.json";
/// Session description the GUI polls while it waits for the tunnel.
const STATE_FILE: &str = "/run/rumurule/tun.state";

static STOP: AtomicBool = AtomicBool::new(false);
/// Target used to prove that the tunnel carries traffic.
const DEFAULT_HEALTH: &str = "1.1.1.1:80";
/// How often the health check runs while the tunnel is up.
const HEALTH_INTERVAL: Duration = Duration::from_secs(10);
/// Failed checks in a row before the helper takes the tunnel down.
const HEALTH_FAILURES: u8 = 3;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("run") => run(&args[1..]),
        Some("probe") => probe(),
        Some("install") => install(),
        Some("cleanup") => {
            let interface = args.get(1).map(String::as_str).unwrap_or("rumurule0");
            cleanup(interface, None);
            Ok(())
        }
        _ => Err(usage()),
    };

    if let Err(error) = result {
        eprintln!("rumurule-helper: {error}");
        std::process::exit(1);
    }
}

fn usage() -> String {
    "usage: rumurule-helper run --config FILE --xray FILE --owner PID [--token ID] [--stop-file FILE] [--max-runtime SECONDS] [--health HOST:PORT]\n       \
     rumurule-helper probe\n       \
     rumurule-helper install\n       \
     rumurule-helper cleanup INTERFACE"
        .into()
}

fn require_root() -> Result<(), String> {
    if unsafe { libc::geteuid() } != 0 {
        return Err("root privileges are required".into());
    }
    Ok(())
}

/// Checks the helper can do its job; run through `pkexec` to pre-authorize.
fn probe() -> Result<(), String> {
    require_root()?;
    let report = serde_json::json!({
        "root": true,
        "ip": exists("/usr/bin/ip") || exists("/usr/sbin/ip") || exists("/bin/ip"),
        "tun": exists("/dev/net/tun"),
        "resolvectl": Command::new("resolvectl")
            .arg("--version")
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false),
    });
    println!("{report}");
    Ok(())
}

fn exists(path: &str) -> bool {
    Path::new(path).exists()
}

/// Where a packaged install keeps the privileged pieces.
const SYSTEM_HELPER: &str = "/usr/lib/rumurule/rumurule-helper";
const SYSTEM_POLICY: &str = "/usr/share/polkit-1/actions/slop.rumurule.tunnel.policy";
const SYSTEM_RULE: &str = "/etc/polkit-1/rules.d/50-rumurule.rules";
const SYSTEM_MARKER: &str = "/usr/lib/rumurule/setup.json";

const POLICY: &str = include_str!("../../packaging/slop.rumurule.tunnel.policy");

/// Grants the user who ran the setup passwordless access to this one action.
/// The uid placeholder is replaced at install time, so other accounts still
/// get the regular polkit prompt.
const RULE: &str = r#"// Installed by RumuRule. Allows the user who set up the tunnel to start it
// without a password; the helper still validates every configuration file.
polkit.addRule(function (action, subject) {
  if (action.id != "slop.rumurule.tunnel") {
    return null;
  }
  if (!subject.local || !subject.active || subject.uid != @UID@) {
    return null;
  }
  return polkit.Result.YES;
});
"#;

/// One-time privileged setup: install the helper, the polkit action and a
/// per-user rule, so later connects never ask for a password again.
fn install() -> Result<(), String> {
    require_root()?;
    let uid = std::env::var("PKEXEC_UID")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .or_else(|| {
            std::env::var("SUDO_UID")
                .ok()
                .and_then(|value| value.parse::<u32>().ok())
        })
        .unwrap_or_else(|| unsafe { libc::getuid() });

    let exe = std::env::current_exe()
        .map_err(|e| format!("cannot locate the helper: {e}"))?;
    if exe != Path::new(SYSTEM_HELPER) {
        if let Some(dir) = Path::new(SYSTEM_HELPER).parent() {
            fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        }
        fs::copy(&exe, SYSTEM_HELPER).map_err(|e| format!("cannot install helper: {e}"))?;
    }
    fs::set_permissions(SYSTEM_HELPER, fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("cannot set helper permissions: {e}"))?;

    if let Some(dir) = Path::new(SYSTEM_POLICY).parent() {
        fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    fs::write(SYSTEM_POLICY, POLICY).map_err(|e| format!("cannot write policy: {e}"))?;
    fs::set_permissions(SYSTEM_POLICY, fs::Permissions::from_mode(0o644))
        .map_err(|e| format!("cannot set policy permissions: {e}"))?;

    if let Some(dir) = Path::new(SYSTEM_RULE).parent() {
        fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    let rule = RULE.replace("@UID@", &uid.to_string());
    fs::write(SYSTEM_RULE, rule).map_err(|e| format!("cannot write polkit rule: {e}"))?;
    fs::set_permissions(SYSTEM_RULE, fs::Permissions::from_mode(0o644))
        .map_err(|e| format!("cannot set rule permissions: {e}"))?;

    let installed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let marker = serde_json::json!({
        "uid": uid,
        "version": env!("CARGO_PKG_VERSION"),
        "installed": installed,
    });
    fs::write(SYSTEM_MARKER, marker.to_string())
        .map_err(|e| format!("cannot write setup marker: {e}"))?;

    println!(
        "{}",
        serde_json::json!({
            "helper": SYSTEM_HELPER,
            "policy": SYSTEM_POLICY,
            "rule": SYSTEM_RULE,
            "uid": uid,
        })
    );
    Ok(())
}

struct Options {
    config: PathBuf,
    xray: PathBuf,
    owner: i32,
    stop_file: Option<PathBuf>,
    /// Identifies the GUI request, so a stale session file is never trusted.
    token: String,
    /// Hard limit for the session; a dead-man's switch for unattended runs.
    max_runtime: Option<u64>,
    /// Endpoint probed through the tunnel; an empty value disables the check.
    health: String,
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut config = None;
    let mut xray = None;
    let mut owner = None;
    let mut stop_file = None;
    let mut max_runtime = None;
    let mut health = DEFAULT_HEALTH.to_string();
    let mut token = String::new();
    let mut iter = args.iter();
    while let Some(flag) = iter.next() {
        let value = iter.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--config" => config = Some(PathBuf::from(value)),
            "--xray" => xray = Some(PathBuf::from(value)),
            "--owner" => {
                owner = Some(
                    value
                        .parse::<i32>()
                        .map_err(|_| "--owner must be a process id".to_string())?,
                )
            }
            "--stop-file" => stop_file = Some(PathBuf::from(value)),
            "--token" => token = value.clone(),
            "--health" => health = value.clone(),
            "--max-runtime" => {
                max_runtime = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| "--max-runtime must be seconds".to_string())?,
                )
            }
            other => return Err(format!("unknown flag {other}")),
        }
    }
    Ok(Options {
        config: config.ok_or("--config is required")?,
        xray: xray.ok_or("--xray is required")?,
        owner: owner.ok_or("--owner is required")?,
        stop_file,
        token,
        max_runtime,
        health,
    })
}

fn run(args: &[String]) -> Result<(), String> {
    require_root()?;
    let options = parse(args)?;
    let config = validate(&options.config)?;
    let interface = config["inbounds"][0]["settings"]["name"]
        .as_str()
        .unwrap_or("rumurule0")
        .to_string();
    let socks_port = config["inbounds"]
        .as_array()
        .and_then(|list| {
            list.iter().find_map(|inbound| {
                (inbound["protocol"] == "socks")
                    .then(|| inbound["port"].as_u64())
                    .flatten()
            })
        })
        .unwrap_or(10808) as u16;

    if !options.xray.is_absolute() || !options.xray.is_file() {
        return Err(format!(
            "xray binary is missing: {}",
            options.xray.display()
        ));
    }

    install_signal_handlers();
    fs::create_dir_all(RUN_DIR).map_err(|e| format!("cannot create {RUN_DIR}: {e}"))?;
    let _ = fs::set_permissions(RUN_DIR, fs::Permissions::from_mode(0o755));

    let body = serde_json::to_vec_pretty(&config).map_err(|e| e.to_string())?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(CONFIG_FILE)
        .map_err(|e| format!("cannot write {CONFIG_FILE}: {e}"))?;
    file.write_all(&body).map_err(|e| e.to_string())?;
    drop(file);

    let write_state = || -> Result<(), String> {
        let state = serde_json::json!({
            "interface": interface,
            "owner": options.owner,
            "helper": std::process::id(),
            "config": CONFIG_FILE,
            "token": options.token,
        });
        fs::write(STATE_FILE, state.to_string()).map_err(|e| e.to_string())?;
        let _ = fs::set_permissions(STATE_FILE, fs::Permissions::from_mode(0o644));
        Ok(())
    };
    let mut child = match Command::new(&options.xray)
        .arg("run")
        .arg("-c")
        .arg(CONFIG_FILE)
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            let _ = fs::remove_file(STATE_FILE);
            return Err(format!("cannot start {}: {error}", options.xray.display()));
        }
    };

    eprintln!("[tun] {} started on {interface}", options.xray.display());

    let health = Health {
        target: parse_target(&options.health),
        socks_port,
    };
    if wait_for_link(&interface, &mut child).is_none() {
        eprintln!("[tun] {interface} did not appear; stopping");
        cleanup(&interface, options.stop_file.as_deref());
        return exit_with(1);
    }
    if !startup_check(&health) {
        eprintln!("[tun] the tunnel does not pass traffic; stopping");
        shutdown(&mut child);
        cleanup(&interface, options.stop_file.as_deref());
        return exit_with(1);
    }
    eprintln!("[tun] the tunnel passes traffic");
    write_state()?;

    let code = supervise(
        &mut child,
        options.owner,
        options.stop_file.as_deref(),
        options.max_runtime,
        &interface,
        &health,
    );
    cleanup(&interface, options.stop_file.as_deref());
    exit_with(code)
}

fn exit_with(code: i32) -> Result<(), String> {
    if code == 0 {
        Ok(())
    } else {
        Err(format!("xray exited with status {code}"))
    }
}

/// Waits for Xray, the GUI owner or a termination signal, whichever comes first.
fn supervise(
    child: &mut Child,
    owner: i32,
    stop_file: Option<&Path>,
    max_runtime: Option<u64>,
    interface: &str,
    health: &Health,
) -> i32 {
    let started = Instant::now();
    let link = Path::new("/sys/class/net").join(interface);
    let mut seen = false;
    let mut checked = Instant::now();
    let mut failures = 0u8;
    loop {
        // Something outside can remove the link (a manual `ip link delete`, a
        // network manager). Xray then keeps running with a dead fd.
        let up = link.exists();
        if up {
            seen = true;
        } else if seen {
            eprintln!("[tun] {interface} disappeared");
            return shutdown(child);
        }
        if let Some(limit) = max_runtime {
            if started.elapsed() >= Duration::from_secs(limit) {
                eprintln!("[tun] stopping after the {limit}s limit");
                return shutdown(child);
            }
        }
        if STOP.load(Ordering::Relaxed) {
            eprintln!("[tun] stopping on signal");
            return shutdown(child);
        }
        if stop_file.is_some_and(Path::exists) {
            eprintln!("[tun] stopping on request");
            return shutdown(child);
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                eprintln!("[tun] xray exited: {status}");
                return status_code(status);
            }
            Ok(None) => {}
            Err(error) => {
                eprintln!("[tun] cannot poll xray: {error}");
                return shutdown(child);
            }
        }
        if !owner_alive(owner) {
            eprintln!("[tun] GUI process {owner} is gone");
            return shutdown(child);
        }
        // A live tunnel can break later: the network changes, the server goes
        // away. Never leave the machine routing everything into a dead tunnel.
        if health.target.is_some() && checked.elapsed() >= HEALTH_INTERVAL {
            checked = Instant::now();
            if healthy(health, Duration::from_secs(4)) {
                failures = 0;
            } else {
                failures += 1;
                eprintln!("[tun] health check failed ({failures}/{HEALTH_FAILURES})");
                if failures >= HEALTH_FAILURES {
                    eprintln!("[tun] the tunnel stopped passing traffic; stopping");
                    return shutdown(child);
                }
            }
        }
        thread::sleep(Duration::from_millis(200));
    }
}

struct Health {
    /// `None` when the check is disabled.
    target: Option<(Ipv4Addr, u16)>,
    socks_port: u16,
}

fn parse_target(value: &str) -> Option<(Ipv4Addr, u16)> {
    let (host, port) = value.rsplit_once(':')?;
    Some((host.parse().ok()?, port.parse().ok()?))
}

/// Waits for the interface Xray creates, or for Xray itself to give up.
fn wait_for_link(interface: &str, child: &mut Child) -> Option<()> {
    let link = Path::new("/sys/class/net").join(interface);
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if link.exists() {
            return Some(());
        }
        if let Ok(Some(_)) = child.try_wait() {
            return None;
        }
        thread::sleep(Duration::from_millis(150));
    }
    None
}

/// Proves the tunnel works before the session is reported as running.
fn startup_check(health: &Health) -> bool {
    if health.target.is_none() {
        return true;
    }
    // A dead server must fail fast: the window waits at ~68% while this runs,
    // so the whole probe keeps a tight budget instead of stalling on retries.
    let deadline = Instant::now() + Duration::from_secs(12);
    for attempt in 0..4 {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        // One dial gets at most four seconds; the rest of the budget is left
        // for a retry, because the first one through a fresh tunnel is slow.
        if healthy(health, left.min(Duration::from_secs(4))) {
            return true;
        }
        eprintln!("[tun] waiting for traffic ({}/4)", attempt + 1);
        thread::sleep(Duration::from_millis(300));
    }
    false
}

/// Sends an HTTP request through the tunnel's own SOCKS port and waits for a
/// byte back: a black-holed upstream answers nothing. The budget caps the
/// whole exchange, so a silent server cannot hold the connect screen hostage.
fn healthy(health: &Health, budget: Duration) -> bool {
    let Some((host, port)) = health.target else {
        return true;
    };
    let deadline = Instant::now() + budget;
    let left = || deadline.saturating_duration_since(Instant::now());
    let step = left();
    if step.is_zero() {
        return false;
    }
    let Ok(mut stream) = TcpStream::connect_timeout(
        &SocketAddr::from(([127, 0, 0, 1], health.socks_port)),
        step,
    ) else {
        return false;
    };
    if !arm(&stream, left()) {
        return false;
    }

    if stream.write_all(&[5, 1, 0]).is_err() {
        return false;
    }
    if !arm(&stream, left()) || !greeting(&mut stream) {
        return false;
    }

    let mut request = vec![5, 1, 0, 1];
    request.extend_from_slice(&host.octets());
    request.extend_from_slice(&port.to_be_bytes());
    if stream.write_all(&request).is_err() {
        return false;
    }
    if !arm(&stream, left()) || handshake(&mut stream).is_none() {
        return false;
    }

    let get = format!("GET / HTTP/1.0\r\nHost: {host}\r\n\r\n");
    if stream.write_all(get.as_bytes()).is_err() {
        return false;
    }
    if !arm(&stream, left()) {
        return false;
    }
    let mut byte = [0u8; 1];
    stream.read_exact(&mut byte).is_ok()
}

/// Applies the time left to the next read, refusing work past the deadline.
fn arm(stream: &TcpStream, budget: Duration) -> bool {
    if budget.is_zero() {
        return false;
    }
    stream.set_read_timeout(Some(budget)).is_ok()
        && stream.set_write_timeout(Some(budget)).is_ok()
}

/// Reads the two-byte answer to the method offer.
fn greeting(stream: &mut TcpStream) -> bool {
    let mut head = [0u8; 2];
    stream.read_exact(&mut head).is_ok() && head[0] == 5 && head[1] == 0
}

/// Reads a SOCKS5 reply and checks its status, skipping the bound address.
fn handshake(stream: &mut TcpStream) -> Option<()> {
    let mut head = [0u8; 4];
    stream.read_exact(&mut head).ok()?;
    if head[0] != 5 || head[1] != 0 {
        return None;
    }
    let skip = match head[3] {
        1 => 6,
        4 => 18,
        _ => return None,
    };
    let mut rest = vec![0u8; skip];
    stream.read_exact(&mut rest).ok()?;
    Some(())
}

fn status_code(status: ExitStatus) -> i32 {
    if status.success() {
        0
    } else {
        status.code().unwrap_or(1)
    }
}

fn owner_alive(pid: i32) -> bool {
    if pid <= 1 {
        return true;
    }
    let result = unsafe { libc::kill(pid, 0) };
    if result == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Asks Xray to stop, so it removes routes and DNS on its own.
fn shutdown(child: &mut Child) -> i32 {
    let _ = unsafe { libc::kill(child.id() as i32, libc::SIGTERM) };
    for _ in 0..25 {
        match child.try_wait() {
            Ok(Some(status)) => return status_code(status),
            Ok(None) => thread::sleep(Duration::from_millis(200)),
            Err(_) => break,
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    0
}

/// Best-effort removal of anything a killed Xray could leave behind.
fn cleanup(interface: &str, stop_file: Option<&Path>) {
    if let Some(path) = stop_file {
        let _ = fs::remove_file(path);
    }
    let _ = Command::new("ip")
        .args(["link", "del", interface])
        .status();
    let _ = Command::new("resolvectl")
        .args(["revert", interface])
        .status();
    let _ = fs::remove_file(STATE_FILE);
    let _ = fs::remove_file(CONFIG_FILE);
}

fn validate(path: &Path) -> Result<Value, String> {
    let metadata = fs::metadata(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    if !metadata.is_file() {
        return Err("configuration is not a regular file".into());
    }
    if let Some(uid) = std::env::var("PKEXEC_UID")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
    {
        if metadata.uid() != uid {
            return Err("configuration file belongs to another user".into());
        }
    }
    if metadata.mode() & 0o022 != 0 {
        return Err("configuration file is writable by other users".into());
    }

    let raw = fs::read(path).map_err(|e| e.to_string())?;
    let config: Value = serde_json::from_slice(&raw).map_err(|e| format!("invalid JSON: {e}"))?;
    let inbound = &config["inbounds"][0];
    if inbound["protocol"] != "tun" {
        return Err("configuration has no tun inbound".into());
    }
    if inbound["settings"]["name"].as_str().unwrap_or("").is_empty() {
        return Err("tun interface name is missing".into());
    }
    Ok(config)
}

extern "C" fn handle_signal(_: libc::c_int) {
    STOP.store(true, Ordering::Relaxed);
}

fn install_signal_handlers() {
    for signal in [
        libc::SIGTERM,
        libc::SIGINT,
        libc::SIGHUP,
        libc::SIGQUIT,
    ] {
        unsafe {
            libc::signal(signal, handle_signal as *const () as libc::sighandler_t);
        }
    }
}
