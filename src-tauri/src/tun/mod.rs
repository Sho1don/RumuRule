//! Privileged TUN sessions.
//!
//! The GUI writes a TUN configuration and asks the helper to run Xray as root
//! through `pkexec`. The helper owns the Xray process, so the GUI only needs to
//! wait for the session file and can stop it with a signal.

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, SystemTime},
};

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::xray::emit_progress;

/// Session description written by the helper, root-owned and world-readable.
pub const STATE_FILE: &str = "/run/rumurule/tun.state";
/// Where packages install the privileged helper.
const SYSTEM_HELPER: &str = "/usr/lib/rumurule/rumurule-helper";
/// How long the GUI waits for polkit authorization and the interface.
const START_TIMEOUT: Duration = Duration::from_secs(120);
const INTERFACE_TIMEOUT: Duration = Duration::from_secs(10);

/// Root-owned helper that launches Xray, found next to the app in dev builds.
pub fn find_helper() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("RUMURULE_HELPER") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
        return Err(format!(
            "RUMURULE_HELPER points to a missing file: {}",
            path.display()
        ));
    }

    if Path::new(SYSTEM_HELPER).is_file() {
        return Ok(PathBuf::from(SYSTEM_HELPER));
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join("rumurule-helper");
            if sibling.is_file() {
                return Ok(sibling);
            }
        }
    }

    find_on_path("rumurule-helper")
        .ok_or_else(|| "rumurule-helper is not installed".to_string())
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// Settings-runtime prerequisites shown on the Settings screen.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prereqs {
    pub helper: Option<String>,
    pub xray: Option<String>,
    pub xray_version: Option<String>,
    pub polkit: bool,
    pub ipv6: bool,
    pub error: Option<String>,
}

pub fn prereqs(app: &AppHandle) -> Prereqs {
    let helper = find_helper();
    let xray = crate::xray::binary_path(app);
    Prereqs {
        helper: helper.as_ref().ok().map(|p| p.display().to_string()),
        xray: xray.as_ref().ok().map(|p| p.display().to_string()),
        xray_version: xray
            .as_ref()
            .ok()
            .and_then(|path| crate::xray::version(path).ok()),
        polkit: find_on_path("pkexec").is_some(),
        ipv6: has_ipv6_default_route(),
        error: helper.err().or_else(|| xray.err()),
    }
}

/// Whether the host routes IPv6 by default; the TUN follows the same answer.
pub fn has_ipv6_default_route() -> bool {
    Command::new("ip")
        .args(["-6", "route", "show", "default"])
        .output()
        .map(|out| out.status.success() && !out.stdout.is_empty())
        .unwrap_or(false)
}

fn service_environment() -> Command {
    let mut command = Command::new("pkexec");
    command.env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin");
    command
}

/// One-time privileged setup: install the helper and the polkit rule that
/// lets this user start the tunnel without a password prompt.
pub fn install(app: &AppHandle) -> Result<String, String> {
    let helper = find_helper()?;
    let output = service_environment()
        .arg(&helper)
        .arg("install")
        .output()
        .map_err(|e| format!("cannot run pkexec: {e}"))?;
    let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let _ = app;
    if !output.status.success() {
        return Err(if message.is_empty() {
            "setup was cancelled".into()
        } else {
            message
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Whether the passwordless setup is in place for the current user.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Setup {
    /// Helper, policy and marker are installed system-wide.
    pub installed: bool,
    pub helper: Option<String>,
    /// The helper resolved through the normal lookup is the system copy.
    pub system: bool,
    pub policy: bool,
    /// User the polkit rule was written for.
    pub uid: Option<u32>,
    /// That user is the one running the app.
    pub user_matches: bool,
    pub polkit: bool,
    pub xray: bool,
}

pub fn setup_state(app: &AppHandle) -> Setup {
    let marker = fs::read_to_string("/usr/lib/rumurule/setup.json")
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok());
    let uid = marker
        .as_ref()
        .and_then(|value| value["uid"].as_u64())
        .map(|value| value as u32);
    let helper = find_helper();
    let policy = Path::new("/usr/share/polkit-1/actions/slop.rumurule.tunnel.policy").is_file();
    let system = helper
        .as_ref()
        .map(|path| path == Path::new(SYSTEM_HELPER))
        .unwrap_or(false);
    let installed = system && policy && marker.is_some();
    Setup {
        installed,
        helper: helper.as_ref().ok().map(|path| path.display().to_string()),
        system,
        policy,
        uid,
        user_matches: uid == Some(unsafe { libc::geteuid() }),
        polkit: find_on_path("pkexec").is_some(),
        xray: crate::xray::binary_path(app).is_ok(),
    }
}

/// A running root Xray, owned by the helper.
pub struct Session {
    child: Child,
    pub interface: String,
    pub helper_pid: Option<u32>,
    stop_file: PathBuf,
}

impl Session {
    pub fn pid(&self) -> u32 {
        self.helper_pid.unwrap_or_else(|| self.child.id())
    }

    /// Whether the helper (and therefore Xray) is still alive.
    pub fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Whether the interface Xray created still exists. If something removes it
    /// (a manual `ip link delete`, a network manager) the session is dead even
    /// though the processes keep running.
    pub fn interface_up(&self) -> bool {
        Path::new("/sys/class/net").join(&self.interface).exists()
    }

    /// Asks the root helper to stop through a file in the user's runtime dir;
    /// the GUI cannot signal a root process itself.
    pub fn stop(&mut self) {
        let _ = fs::write(&self.stop_file, b"stop");
        for _ in 0..60 {
            if !self.alive() {
                let _ = fs::remove_file(&self.stop_file);
                return;
            }
            thread::sleep(Duration::from_millis(200));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_file(&self.stop_file);
    }
}

/// Private, user-owned directory the helper watches for a stop request.
fn runtime_dir() -> Result<PathBuf, String> {
    let uid = unsafe { libc::geteuid() };
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    let dir = base.join(format!("rumurule-{uid}"));
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
    Ok(dir)
}

/// Start the helper for a configuration that already sits on disk.
pub fn start(
    app: &AppHandle,
    config: &Path,
    xray: &Path,
) -> Result<Session, String> {
    let helper = find_helper()?;
    let stop_file = runtime_dir()?.join("stop");
    let _ = fs::remove_file(&stop_file);
    let token = format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis())
            .unwrap_or(0)
    );

    let mut command = if unsafe { libc::geteuid() } == 0 {
        Command::new(&helper)
    } else {
        let mut command = service_environment();
        command.arg(&helper);
        command
    };
    emit_progress(app, 0.28, "waiting for authorization");
    command
        .arg("run")
        .arg("--config")
        .arg(config)
        .arg("--xray")
        .arg(xray)
        .arg("--owner")
        .arg(std::process::id().to_string())
        .arg("--stop-file")
        .arg(&stop_file)
        .arg("--token")
        .arg(&token)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|e| format!("cannot start the helper: {e}"))?;

    let tail: crate::xray::Tail = Default::default();
    if let Some(stdout) = child.stdout.take() {
        crate::xray::stream_output_tail(app.clone(), stdout, tail.clone());
    }
    if let Some(stderr) = child.stderr.take() {
        crate::xray::stream_output_tail(app.clone(), stderr, tail.clone());
    }

    let state = wait_for_state(app, &mut child, &token, &tail)?;
    let interface = state
        .get("interface")
        .and_then(|value| value.as_str())
        .unwrap_or("rumurule0")
        .to_string();
    let helper_pid = state
        .get("helper")
        .and_then(|value| value.as_u64())
        .map(|value| value as u32);

    emit_progress(app, 0.75, "tunnel is up");
    wait_for_interface(app, &mut child, &interface)?;
    emit_progress(app, 0.9, "checking traffic");

    Ok(Session {
        child,
        interface,
        helper_pid,
        stop_file,
    })
}

fn wait_for_state(
    app: &AppHandle,
    child: &mut Child,
    token: &str,
    tail: &crate::xray::Tail,
) -> Result<serde_json::Value, String> {
    let deadline = SystemTime::now() + START_TIMEOUT;
    let started = SystemTime::now();
    let mut announced = started;
    while SystemTime::now() < deadline {
        if let Ok(raw) = fs::read_to_string(STATE_FILE) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) {
                // A previous run may have left a file; only this token counts.
                if value["token"].as_str() == Some(token) {
                    return Ok(value);
                }
            }
        }
        if let Ok(Some(status)) = child.try_wait() {
            return Err(match tail_reason(tail) {
                Some(reason) => format!("tunnel failed: {reason} ({status})"),
                None => format!(
                    "the tunnel helper exited before the tunnel was ready ({status}); see the log"
                ),
            });
        }
        // The wait includes the polkit dialog, so the bar keeps moving while
        // the user types. A linear ramp over the whole timeout looked frozen
        // for the first seconds; this approaches 65% with a 6 s half-life and
        // then creeps, never reaching the next stage on its own.
        if announced.elapsed().unwrap_or_default() > Duration::from_millis(250) {
            announced = SystemTime::now();
            let secs = started.elapsed().unwrap_or_default().as_secs_f32();
            emit_progress(
                app,
                0.3 + 0.4 * (1.0 - (-secs / 6.0).exp()),
                "waiting for authorization",
            );
        }
        thread::sleep(Duration::from_millis(150));
    }
    Err(match tail_reason(tail) {
        Some(reason) => format!("timed out waiting for the tunnel helper: {reason}"),
        None => "timed out waiting for the tunnel helper".into(),
    })
}

/// The most useful line the helper printed before it exited. Prefer its own
/// `[tun]` explanation: the raw tail also carries cleanup noise such as
/// "Failed to resolve interface 'rumurule0': No such device" from resolvectl.
fn tail_reason(tail: &crate::xray::Tail) -> Option<String> {
    let sink = tail.lock().ok()?;
    let lines: Vec<&str> = sink
        .iter()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect();
    let explanations = [
        "does not pass traffic",
        "stopped passing traffic",
        "did not appear",
        "disappeared",
        "cannot poll",
        "stopping after",
    ];
    if let Some(line) = lines.iter().rev().find(|line| {
        line.starts_with("[tun]") && explanations.iter().any(|hint| line.contains(hint))
    }) {
        return Some(line.trim_start_matches("[tun]").trim().to_string());
    }
    lines
        .iter()
        .rev()
        .find(|line| {
            !line.starts_with("rumurule-helper:")
                && !line.starts_with("[tun]")
                && !line.contains("Failed to resolve interface")
        })
        .or_else(|| {
            lines
                .iter()
                .rev()
                .find(|line| !line.starts_with("rumurule-helper:"))
        })
        .map(|line| line.to_string())
}

fn wait_for_interface(
    app: &AppHandle,
    child: &mut Child,
    interface: &str,
) -> Result<(), String> {
    let path = PathBuf::from("/sys/class/net").join(interface);
    let deadline = SystemTime::now() + INTERFACE_TIMEOUT;
    let started = SystemTime::now();
    let mut announced = started;
    while SystemTime::now() < deadline {
        if path.exists() {
            return Ok(());
        }
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!("Xray exited while bringing up {interface} ({status})"));
        }
        if announced.elapsed().unwrap_or_default() > Duration::from_millis(200) {
            announced = SystemTime::now();
            let share = started.elapsed().unwrap_or_default().as_secs_f32()
                / INTERFACE_TIMEOUT.as_secs_f32();
            emit_progress(
                app,
                0.75 + 0.13 * share.min(1.0),
                "bringing the interface up",
            );
        }
        thread::sleep(Duration::from_millis(120));
    }
    Err(format!("interface {interface} did not appear"))
}

/// Path of the TUN configuration inside the app config directory.
pub fn config_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("cannot resolve config dir: {e}"))?;
    Ok(dir.join("xray-tun.json"))
}
