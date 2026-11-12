//! Xray sidecar: process lifecycle and log streaming.

mod binary;
mod output;

use std::{
    fs,
    path::{Path, PathBuf},
    process::Child,
    sync::Mutex,
    thread,
    time::Duration,
};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use rumurule_core::{config, Profile};

use binary::{config_path, find_binary};
pub(crate) use output::{stream_output, stream_output_tail, Tail};

use crate::tun::{self, Session as TunSession};

/// Path to the Xray binary, reused by throwaway latency probe instances.
pub fn binary_path(app: &AppHandle) -> Result<PathBuf, String> {
    find_binary(app)
}

/// Release of the resolved binary, e.g. `26.11.9`, for the settings panel.
pub fn version(binary: &Path) -> Result<String, String> {
    binary::version(binary)
}

/// Reject a configuration before it produces a tunnel-shaped failure.
///
/// `-test` parses and builds every outbound and inbound without opening a
/// socket, so a schema change in a new core shows up as one readable line.
fn check_config(binary: &Path, path: &Path) -> Result<(), String> {
    let output = std::process::Command::new(binary)
        .arg("run")
        .arg("-test")
        .arg("-c")
        .arg(path)
        .output()
        .map_err(|e| format!("cannot check the config: {e}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let reason = stderr
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("no details");
    let reason: String = reason.chars().take(300).collect();
    Err(format!("Xray rejected the configuration: {reason}"))
}

/// Remove a sidecar left behind when a previous app process was killed.
fn stop_orphans(binary: &Path, config: &Path) {
    let binary = binary.to_string_lossy().to_string();
    let config = config.to_string_lossy().to_string();
    let Ok(entries) = fs::read_dir("/proc") else {
        return;
    };

    for entry in entries.flatten() {
        let pid = entry.file_name().to_string_lossy().to_string();
        if pid.is_empty() || !pid.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let Ok(raw) = fs::read(format!("/proc/{pid}/cmdline")) else {
            continue;
        };
        let args: Vec<String> = raw
            .split(|byte| *byte == 0)
            .map(|part| String::from_utf8_lossy(part).into_owned())
            .collect();
        let same_binary = args.first().is_some_and(|arg| *arg == binary);
        let same_config = args.iter().any(|arg| *arg == config);
        if same_binary && same_config {
            let _ = std::process::Command::new("kill")
                .arg("-TERM")
                .arg(&pid)
                .status();
        }
    }

    thread::sleep(Duration::from_millis(150));
}

/// Event name used to stream Xray output to the UI.
pub const LOG_EVENT: &str = "xray-log";
/// Event name for connect progress: determinate values the window can show.
pub const PROGRESS_EVENT: &str = "tunnel-progress";

/// One step of the connect sequence.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    /// Fraction between 0 and 1.
    pub value: f32,
    pub label: &'static str,
}

/// Tell the window how far the connect sequence got.
pub(crate) fn emit_progress(app: &AppHandle, value: f32, label: &'static str) {
    let _ = app.emit(PROGRESS_EVENT, Progress { value, label });
}

#[derive(Default)]
pub struct XrayManager {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    running: Option<Running>,
    binary: Option<PathBuf>,
    since: Option<u64>,
    /// Set when a session ended on its own, so the caller can clean up after it.
    died: bool,
}

enum Running {
    Proxy(Child),
    Tun(TunSession),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub running: bool,
    pub pid: Option<u32>,
    pub socks_port: u16,
    pub http_port: u16,
    pub binary: Option<String>,
    /// `proxy` or `tun`.
    pub mode: String,
    /// TUN interface name while the tunnel runs.
    pub interface: Option<String>,
    /// Unix time in milliseconds when the session started.
    pub since: Option<u64>,
}

impl Status {
    fn proxy(binary: Option<String>) -> Self {
        Self {
            running: false,
            pid: None,
            socks_port: config::SOCKS_PORT,
            http_port: config::HTTP_PORT,
            binary,
            mode: "proxy".into(),
            interface: None,
            since: None,
        }
    }
}

/// Wall clock in milliseconds, for the session timer.
fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

impl XrayManager {
    /// Start Xray with a freshly generated configuration.
    pub fn start(&self, app: &AppHandle, profile: &Profile) -> Result<Status, String> {
        self.start_config(app, &config::build_config(profile))
    }

    /// Start Xray with a ready-made configuration from a subscription.
    pub fn start_config(
        &self,
        app: &AppHandle,
        config: &serde_json::Value,
    ) -> Result<Status, String> {
        self.stop()?;
        emit_progress(app, 0.1, "preparing config");

        let binary = find_binary(app)?;
        let config_path = config_path(app)?;
        if let Some(dir) = config_path.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        }
        stop_orphans(&binary, &config_path);

        let body = serde_json::to_vec_pretty(config).map_err(|e| e.to_string())?;
        fs::write(&config_path, body)
            .map_err(|e| format!("cannot write {}: {e}", config_path.display()))?;
        emit_progress(app, 0.25, "checking the config");
        check_config(&binary, &config_path)?;
        emit_progress(app, 0.35, "starting Xray");

        let mut child = std::process::Command::new(&binary)
            .arg("run")
            .arg("-c")
            .arg(&config_path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("cannot start {}: {e}", binary.display()))?;

        if let Some(stdout) = child.stdout.take() {
            stream_output(app.clone(), stdout);
        }
        if let Some(stderr) = child.stderr.take() {
            stream_output(app.clone(), stderr);
        }

        // Give Xray time to bind its inbounds; a port conflict shows up here.
        for _ in 0..10 {
            thread::sleep(Duration::from_millis(80));
            if let Some(code) = child.try_wait().ok().flatten() {
                return Err(format!("xray exited immediately ({code})"));
            }
        }

        emit_progress(app, 0.9, "waiting for the ports");
        let status = Status {
            running: true,
            pid: Some(child.id()),
            socks_port: config::SOCKS_PORT,
            http_port: config::HTTP_PORT,
            binary: Some(binary.display().to_string()),
            mode: "proxy".into(),
            interface: None,
            since: Some(now_millis()),
        };

        let mut inner = self.inner.lock().expect("xray state poisoned");
        inner.running = Some(Running::Proxy(child));
        inner.binary = Some(binary);
        inner.since = status.since;
        Ok(status)
    }

    /// Start Xray as root with a TUN inbound, through the privileged helper.
    pub fn start_tun(&self, app: &AppHandle, base: &serde_json::Value) -> Result<Status, String> {
        self.stop()?;
        emit_progress(app, 0.1, "preparing config");

        let binary = find_binary(app)?;
        let path = tun::config_path(app)?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        }

        let config = config::tun_config(base, tun::has_ipv6_default_route());
        let body = serde_json::to_vec_pretty(&config).map_err(|e| e.to_string())?;
        fs::write(&path, body).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        emit_progress(app, 0.15, "checking the config");
        check_config(&binary, &path)?;
        emit_progress(app, 0.2, "starting the helper");

        let session = tun::start(app, &path, &binary)?;
        let status = Status {
            running: true,
            pid: Some(session.pid()),
            socks_port: config::SOCKS_PORT,
            http_port: config::HTTP_PORT,
            binary: Some(binary.display().to_string()),
            mode: "tun".into(),
            interface: Some(session.interface.clone()),
            since: Some(now_millis()),
        };

        let mut inner = self.inner.lock().expect("xray state poisoned");
        inner.running = Some(Running::Tun(session));
        inner.binary = Some(binary);
        inner.since = status.since;
        Ok(status)
    }

    /// Stop the running Xray process, if any.
    pub fn stop(&self) -> Result<Status, String> {
        let mut inner = self.inner.lock().expect("xray state poisoned");
        inner.since = None;
        match inner.running.take() {
            Some(Running::Proxy(mut child)) => {
                let _ = child.kill();
                let _ = child.wait();
            }
            Some(Running::Tun(mut session)) => session.stop(),
            None => {}
        }
        Ok(Status::proxy(
            inner.binary.as_ref().map(|p| p.display().to_string()),
        ))
    }

    /// Whether the last status poll found a session that died on its own.
    pub fn take_died(&self) -> bool {
        let mut inner = self.inner.lock().expect("xray state poisoned");
        std::mem::take(&mut inner.died)
    }

    /// Current state, reaping the child process when it died on its own.
    pub fn status(&self) -> Status {
        let mut inner = self.inner.lock().expect("xray state poisoned");
        let binary = inner.binary.as_ref().map(|p| p.display().to_string());
        let mut status = Status::proxy(binary);

        match inner.running.take() {
            Some(Running::Proxy(mut child)) => match child.try_wait() {
                Ok(Some(_)) => {
                    inner.died = true;
                    inner.since = None;
                }
                Ok(None) => {
                    status.running = true;
                    status.pid = Some(child.id());
                    status.since = inner.since;
                    inner.running = Some(Running::Proxy(child));
                }
                Err(_) => {
                    inner.died = true;
                    inner.since = None;
                }
            },
            Some(Running::Tun(mut session)) => {
                if session.alive() && session.interface_up() {
                    status.running = true;
                    status.pid = Some(session.pid());
                    status.mode = "tun".into();
                    status.interface = Some(session.interface.clone());
                    status.since = inner.since;
                    inner.running = Some(Running::Tun(session));
                } else {
                    // Stopping waits for the root helper; never block a caller
                    // (a status poll) on that.
                    inner.died = true;
                    inner.since = None;
                    thread::spawn(move || session.stop());
                }
            }
            None => {}
        }

        status
    }
}
