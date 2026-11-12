//! Latency probes for saved servers: TCP, ICMP and HTTP through the server.

use std::{
    fs,
    net::{TcpListener, TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use serde_json::{json, Value};
use tauri::{AppHandle, State};

use rumurule_core::{config, Entry, PingKind, PingMode};

use super::{profiles::Profiles, settings::SettingsStore};

const TIMEOUT: Duration = Duration::from_millis(1500);
const BUDGET: Duration = Duration::from_millis(2500);
/// How long a throwaway Xray instance gets to bind its SOCKS inbound.
const STARTUP: Duration = Duration::from_millis(1200);

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

        let mut child = Command::new(binary)
            .arg("run")
            .arg("-c")
            .arg(&config)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
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
