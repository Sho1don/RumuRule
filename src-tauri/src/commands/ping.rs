//! Latency probes for saved servers: TCP, ICMP and HTTP through the server.

use std::{
    collections::VecDeque,
    fs,
    net::{TcpListener, TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, State};

use rumurule_core::{config, Entry, PingKind, PingMode, Settings, SplitMode};

use super::{profiles::Profiles, settings::SettingsStore};

const TIMEOUT: Duration = Duration::from_millis(1500);
const BUDGET: Duration = Duration::from_millis(2500);
/// How long a throwaway Xray instance gets to bind its SOCKS inbound.
const STARTUP: Duration = Duration::from_millis(1200);

/// Health check: one small page through the tunnel, twice at most.
const CHECK_URL: &str = "https://www.gstatic.com/generate_204";
const CHECK_TIMEOUT: Duration = Duration::from_millis(4000);
/// Answers above this are alive but barely, so the list asks before trusting.
const CHECK_SLOW_MS: u32 = 3000;
/// A temporary Xray per server costs memory, so a few run at a time.
const CHECK_WORKERS: usize = 3;

/// Event name for per-server health results.
pub const CHECK_EVENT: &str = "server-check";

/// How a server answered through the tunnel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckState {
    Ok,
    Barely,
    Dead,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerCheck {
    pub id: String,
    pub state: CheckState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ms: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Last results, kept so a rebuilt webview still shows the marks.
#[derive(Default)]
pub struct ServerChecks(pub Mutex<Vec<ServerCheck>>);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckProgress {
    done: usize,
    total: usize,
    check: ServerCheck,
}

/// Test every saved server through the tunnel with the current split rules.
#[tauri::command]
pub async fn check_servers(
    app: AppHandle,
    profiles: State<'_, Profiles>,
    settings: State<'_, SettingsStore>,
    stored: State<'_, ServerChecks>,
) -> Result<Vec<ServerCheck>, String> {
    let settings = settings.snapshot();
    let entries = profiles.entries();
    let total = entries.len();
    if total == 0 {
        stored.0.lock().expect("checks poisoned").clear();
        return Ok(Vec::new());
    }

    let binary = crate::xray::binary_path(&app)?;
    let queue = Arc::new(Mutex::new(entries.into_iter().collect::<VecDeque<_>>()));
    let done = Arc::new(Mutex::new(Vec::<ServerCheck>::new()));

    let workers = CHECK_WORKERS.min(total);
    let mut handles = Vec::with_capacity(workers);
    for _ in 0..workers {
        let app = app.clone();
        let queue = Arc::clone(&queue);
        let done = Arc::clone(&done);
        let binary = binary.clone();
        let settings = settings.clone();
        handles.push(std::thread::spawn(move || loop {
            let entry = queue.lock().expect("queue poisoned").pop_front();
            let Some(entry) = entry else { break };
            let check = check_entry(&entry, &binary, &settings);
            let mut results = done.lock().expect("checks poisoned");
            results.push(check.clone());
            let _ = app.emit(
                CHECK_EVENT,
                CheckProgress {
                    done: results.len(),
                    total,
                    check,
                },
            );
        }));
    }

    let results = tauri::async_runtime::spawn_blocking(move || {
        for handle in handles {
            let _ = handle.join();
        }
        done.lock().expect("checks poisoned").clone()
    })
    .await
    .map_err(|error| format!("check task failed: {error}"))?;

    *stored.0.lock().expect("checks poisoned") = results.clone();
    Ok(results)
}

/// Results of the last full check, for a window that was re-created.
#[tauri::command]
pub fn check_results(stored: State<'_, ServerChecks>) -> Vec<ServerCheck> {
    stored.0.lock().expect("checks poisoned").clone()
}

/// Probe one entry and classify the answer.
fn check_entry(entry: &Entry, binary: &Path, settings: &Settings) -> ServerCheck {
    let result = |state, ms, reason: Option<String>| ServerCheck {
        id: entry.id.clone(),
        state,
        ms,
        reason,
    };
    if entry.config.is_none() && entry.profile.server.is_empty() {
        return result(CheckState::Dead, None, Some("no address".into()));
    }

    let mut base = entry
        .config
        .clone()
        .unwrap_or_else(|| config::build_config(&entry.profile));
    // Bypass is the case this check exists for: the real list decides what
    // leaves the tunnel. Only would send a foreign probe direct, which says
    // nothing about the server, so it is left alone.
    if settings.split.mode == SplitMode::Bypass {
        config::apply_split(&mut base, &settings.split);
    }

    let port = match free_port() {
        Ok(port) => port,
        Err(error) => return result(CheckState::Dead, None, Some(error)),
    };
    let _probe = match Probe::start(binary, &base, port) {
        Ok(probe) => probe,
        Err(error) => return result(CheckState::Dead, None, Some(trim_reason(&error))),
    };

    let proxy = match reqwest::Proxy::all(format!("socks5h://127.0.0.1:{port}")) {
        Ok(proxy) => proxy,
        Err(error) => return result(CheckState::Dead, None, Some(error.to_string())),
    };
    let client = match reqwest::blocking::Client::builder()
        .proxy(proxy)
        .timeout(CHECK_TIMEOUT)
        .connect_timeout(CHECK_TIMEOUT)
        .build()
    {
        Ok(client) => client,
        Err(error) => return result(CheckState::Dead, None, Some(error.to_string())),
    };

    match check_request(&client) {
        Ok(ms) if ms <= CHECK_SLOW_MS => result(CheckState::Ok, Some(ms), None),
        Ok(ms) => result(
            CheckState::Barely,
            Some(ms),
            Some("answers, but slowly".into()),
        ),
        Err(first) => {
            std::thread::sleep(Duration::from_millis(300));
            match check_request(&client) {
                Ok(ms) => result(
                    CheckState::Barely,
                    Some(ms),
                    Some("answered only on the second try".into()),
                ),
                Err(second) => result(
                    CheckState::Dead,
                    None,
                    Some(trim_reason(&format!("{first}; retry: {second}"))),
                ),
            }
        }
    }
}

fn check_request(client: &reqwest::blocking::Client) -> Result<u32, String> {
    let started = Instant::now();
    let response = client.get(CHECK_URL).send().map_err(describe)?;
    let elapsed = elapsed_ms(started);
    // The body is drained so the connection closes cleanly and a failed read
    // still counts as a failure.
    response.bytes().map_err(describe)?;
    Ok(elapsed)
}

/// Keep tooltips readable: reqwest chains its causes with long URLs.
fn trim_reason(text: &str) -> String {
    let lower = text.to_lowercase();
    if lower.contains("timed out") || lower.contains("timeout") {
        return "timed out".into();
    }
    if lower.contains("connection refused") {
        return "connection refused".into();
    }
    if lower.contains("connection reset") || lower.contains("unexpected eof") {
        return "connection reset".into();
    }
    if lower.contains("dns") || lower.contains("lookup") {
        return "cannot resolve the test address".into();
    }

    let mut flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > 160 {
        flat = flat.chars().take(157).collect::<String>();
        flat.push_str("...");
    }
    flat
}

/// Measure latency to a saved server with the configured probe.
#[tauri::command]
pub async fn ping_entry(
    app: AppHandle,
    profiles: State<'_, Profiles>,
    settings: State<'_, SettingsStore>,
    id: String,
) -> Result<u32, String> {
    let entry = profiles
        .entry(&id)
        .ok_or_else(|| format!("unknown profile {id}"))?;
    let settings = settings.snapshot();

    if entry.profile.server.is_empty() {
        return Err("server has no address".into());
    }
    if settings.ping_kind != PingKind::Icmp && entry.profile.port == 0 {
        return Err("server has no port".into());
    }

    let binary = match settings.ping_kind {
        PingKind::Proxy => Some(crate::xray::binary_path(&app)?),
        PingKind::Tcp | PingKind::Icmp => None,
    };

    tauri::async_runtime::spawn_blocking(move || match settings.ping_kind {
        PingKind::Tcp => probe_tcp(&entry.profile.server, entry.profile.port),
        PingKind::Icmp => probe_icmp(&entry.profile.server),
        PingKind::Proxy => probe_proxy(
            &entry,
            binary.expect("proxy probe needs the xray binary"),
            settings.ping_mode,
            &settings.ping_url,
        ),
    })
    .await
    .map_err(|error| format!("ping task failed: {error}"))?
}

fn probe_tcp(host: &str, port: u16) -> Result<u32, String> {
    let started = Instant::now();
    let addresses = (host, port)
        .to_socket_addrs()
        .map_err(|error| error.to_string())?
        .collect::<Vec<_>>();
    if addresses.is_empty() {
        return Err("cannot resolve server".into());
    }

    let mut last_error = String::from("cannot connect");
    for address in addresses {
        let remaining = BUDGET.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return Err("timeout".into());
        }
        match TcpStream::connect_timeout(&address, remaining.min(TIMEOUT)) {
            Ok(stream) => {
                drop(stream);
                return Ok(elapsed_ms(started));
            }
            Err(error) => last_error = error.to_string(),
        }
    }
    Err(last_error)
}

fn probe_icmp(host: &str) -> Result<u32, String> {
    let output = Command::new("ping")
        .args(["-n", "-c", "1", "-W", "2", host])
        .output()
        .map_err(|error| format!("cannot run ping: {error}"))?;
    if !output.status.success() {
        return Err("no ICMP reply".into());
    }

    let text = String::from_utf8_lossy(&output.stdout);
    if let Some(ms) = parse_ping_time(&text) {
        return Ok(ms);
    }
    if text.contains("time<1") {
        return Ok(1);
    }
    Err("unexpected ping output".into())
}

/// Pull the millisecond value out of `time=12.3 ms`.
fn parse_ping_time(text: &str) -> Option<u32> {
    let rest = &text[text.find("time=")? + 5..];
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(rest.len());
    rest[..end].parse::<f64>().ok().map(|ms| ms.round() as u32)
}

fn probe_proxy(
    entry: &Entry,
    binary: PathBuf,
    mode: PingMode,
    url: &str,
) -> Result<u32, String> {
    let base = entry
        .config
        .clone()
        .unwrap_or_else(|| config::build_config(&entry.profile));
    let port = free_port()?;
    let _guard = Probe::start(&binary, &base, port)?;

    let proxy = reqwest::Proxy::all(format!("socks5h://127.0.0.1:{port}"))
        .map_err(|error| format!("cannot set probe proxy: {error}"))?;
    let client = reqwest::blocking::Client::builder()
        .proxy(proxy)
        .timeout(BUDGET)
        .connect_timeout(TIMEOUT)
        .build()
        .map_err(|error| error.to_string())?;

    let method = reqwest::Method::GET;

    // Warm-up: complete one request so the measured one reuses the tunnel
    // (keep-alive) or starts from an idle connection (double).
    if mode != PingMode::Default {
        let _ = probe_request(&client, method.clone(), url, mode, true);
    }

    probe_request(&client, method, url, mode, false)
}

fn probe_request(
    client: &reqwest::blocking::Client,
    method: reqwest::Method,
    url: &str,
    mode: PingMode,
    drain: bool,
) -> Result<u32, String> {
    let mut request = client.request(method, url);
    if mode != PingMode::Default {
        let value = if mode == PingMode::Keepalive {
            "keep-alive"
        } else {
            "close"
        };
        request = request.header(reqwest::header::CONNECTION, value);
    }

    let started = Instant::now();
    let response = request.send().map_err(describe)?;
    let elapsed = elapsed_ms(started);
    if drain {
        let _ = response.bytes();
    }
    Ok(elapsed)
}

/// Flatten a reqwest error and its causes into one line.
fn describe(error: reqwest::Error) -> String {
    use std::error::Error;

    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

fn elapsed_ms(started: Instant) -> u32 {
    started.elapsed().as_millis().clamp(1, u32::MAX as u128) as u32
}

fn free_port() -> Result<u16, String> {
    let listener =
        TcpListener::bind(("127.0.0.1", 0)).map_err(|error| format!("no free port: {error}"))?;
    listener
        .local_addr()
        .map(|address| address.port())
        .map_err(|error| error.to_string())
}

/// Throwaway Xray instance serving one SOCKS port for a single probe.
struct Probe {
    child: Child,
    config: PathBuf,
}

impl Probe {
    fn start(binary: &Path, base: &Value, port: u16) -> Result<Self, String> {
        let dir = std::env::temp_dir().join("rumurule").join("ping");
        fs::create_dir_all(&dir).map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
        let config = dir.join(format!("ping-{}-{port}.json", std::process::id()));
        let body = serde_json::to_vec(&probe_config(base, port)).map_err(|e| e.to_string())?;
        fs::write(&config, body).map_err(|error| format!("cannot write {}: {error}", config.display()))?;

        let mut command = Command::new(binary);
        command
            .arg("run")
            .arg("-c")
            .arg(&config)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        crate::xray::apply_asset_dir(&mut command, binary);
        let mut child = command
            .spawn()
            .map_err(|error| format!("cannot start {}: {error}", binary.display()))?;

        let started = Instant::now();
        while started.elapsed() < STARTUP {
            if let Ok(Some(code)) = child.try_wait() {
                let _ = fs::remove_file(&config);
                return Err(format!("probe xray exited ({code})"));
            }
            if TcpStream::connect(("127.0.0.1", port)).is_ok() {
                return Ok(Self { child, config });
            }
            std::thread::sleep(Duration::from_millis(25));
        }

        let _ = child.kill();
        let _ = child.wait();
        let _ = fs::remove_file(&config);
        Err("probe xray did not start".into())
    }
}

impl Drop for Probe {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_file(&self.config);
    }
}

/// Clone the entry config but expose a single ephemeral SOCKS inbound.
fn probe_config(base: &Value, port: u16) -> Value {
    let mut config = base.clone();
    if let Some(object) = config.as_object_mut() {
        object.insert(
            "inbounds".into(),
            json!([
                {
                    "tag": "socks",
                    "listen": "127.0.0.1",
                    "port": port,
                    "protocol": "socks",
                    "settings": { "auth": "noauth", "udp": false }
                }
            ]),
        );
        object.remove("api");
        object.remove("stats");
        object.remove("policy");
    }
    config
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_iputils_time() {
        assert_eq!(
            parse_ping_time("64 bytes from 1.1.1.1: icmp_seq=1 ttl=57 time=12.3 ms"),
            Some(12)
        );
        assert_eq!(parse_ping_time("no timing here"), None);
    }

    #[test]
    fn probe_config_has_one_loopback_socks_inbound() {
        let base = config::build_config(
            &rumurule_core::parse_link("trojan://pass@example.org:443").unwrap(),
        );
        let probe = probe_config(&base, 45678);
        let inbounds = probe["inbounds"].as_array().unwrap();
        assert_eq!(inbounds.len(), 1);
        assert_eq!(inbounds[0]["port"], 45678);
        assert_eq!(inbounds[0]["protocol"], "socks");
        assert_eq!(probe["outbounds"][0]["protocol"], "trojan");
    }
}
