//! User settings and the device identity used for subscription requests.

use std::{path::PathBuf, sync::Mutex};

use tauri::{AppHandle, Emitter, Manager, State};

use rumurule_core::{subscription::Device, Endpoints, PingKind, PingMode, Settings, TunnelMode};

use crate::system_proxy::SystemProxy;
use crate::xray::{Status, XrayManager, LOG_EVENT};

pub struct SettingsStore {
    path: PathBuf,
    settings: Mutex<Settings>,
}

impl SettingsStore {
    pub fn open(app: &AppHandle) -> Result<Self, String> {
        let dir = app
            .path()
            .app_config_dir()
            .map_err(|e| format!("cannot resolve config dir: {e}"))?;
        let path = Settings::path_in(&dir);
        let settings = Settings::load(&path)?;
        settings.save(&path)?;
        Ok(Self {
            path,
            settings: Mutex::new(settings),
        })
    }

    pub fn snapshot(&self) -> Settings {
        self.settings
            .lock()
            .expect("settings poisoned")
            .clone()
    }

    /// Device identity for subscription requests, when HWID is enabled.
    pub fn device(&self) -> Option<Device> {
        let settings = self.snapshot();
        settings.hwid_enabled.then(|| Device {
            hwid: settings.hwid,
            os: "Linux".into(),
            model: "RumuRule".into(),
            app_version: rumurule_core::AppInfo::VERSION.into(),
        })
    }

    fn persist(&self, settings: &Settings) -> Result<(), String> {
        settings.save(&self.path)
    }
}

#[tauri::command]
pub fn settings_get(state: State<'_, SettingsStore>) -> Settings {
    state.snapshot()
}

#[tauri::command]
pub fn settings_set_hwid(
    state: State<'_, SettingsStore>,
    enabled: bool,
) -> Result<Settings, String> {
    let mut settings = state.settings.lock().expect("settings poisoned");
    settings.hwid_enabled = enabled;
    state.persist(&settings)?;
    Ok(settings.clone())
}

/// Apply the desktop proxy for a fresh tunnel, unless the user turned it off.
pub fn apply_proxy(
    app: &AppHandle,
    settings: &SettingsStore,
    proxy: &SystemProxy,
    status: &Status,
) {
    if !status.running || !settings.snapshot().system_proxy {
        return;
    }
    if let Err(error) = proxy.enable(Endpoints::new(status.socks_port, status.http_port)) {
        let _ = app.emit(LOG_EVENT, format!("system proxy: {error}"));
    }
}

/// Turn the desktop proxy on or off, applying it right away while connected.
#[tauri::command]
pub fn settings_set_system_proxy(
    app: AppHandle,
    state: State<'_, SettingsStore>,
    proxy: State<'_, SystemProxy>,
    xray: State<'_, XrayManager>,
    enabled: bool,
) -> Result<Settings, String> {
    let snapshot = {
        let mut settings = state.settings.lock().expect("settings poisoned");
        settings.system_proxy = enabled;
        state.persist(&settings)?;
        settings.clone()
    };

    let status = xray.status();
    if status.running {
        let result = if enabled {
            proxy.enable(Endpoints::new(status.socks_port, status.http_port))
        } else {
            proxy.disable()
        };
        if let Err(error) = result {
            let _ = app.emit(LOG_EVENT, format!("system proxy: {error}"));
        }
    }

    Ok(snapshot)
}

/// Choose how traffic is captured; a reconnect applies the new mode.
#[tauri::command]
pub fn settings_set_tunnel_mode(
    xray: State<'_, XrayManager>,
    state: State<'_, SettingsStore>,
    mode: TunnelMode,
) -> Result<Settings, String> {
    if xray.status().running {
        return Err("disconnect before changing the tunnel mode".into());
    }
    let mut settings = state.settings.lock().expect("settings poisoned");
    settings.tunnel_mode = mode;
    state.persist(&settings)?;
    Ok(settings.clone())
}

/// Reveal or hide server endpoints in the list and on the status tab.
#[tauri::command]
pub fn settings_set_show_address(
    state: State<'_, SettingsStore>,
    enabled: bool,
) -> Result<Settings, String> {
    let mut settings = state.settings.lock().expect("settings poisoned");
    settings.show_address = enabled;
    state.persist(&settings)?;
    Ok(settings.clone())
}

/// Choose how the server list measures latency.
#[tauri::command]
pub fn settings_set_ping(
    state: State<'_, SettingsStore>,
    kind: PingKind,
    mode: PingMode,
    url: String,
) -> Result<Settings, String> {
    let url = url.trim();
    let parsed = url::Url::parse(url).map_err(|_| "test URL is invalid".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("test URL must use http or https".into());
    }

    let mut settings = state.settings.lock().expect("settings poisoned");
    settings.ping_kind = kind;
    settings.ping_mode = mode;
    settings.ping_url = url.to_string();
    state.persist(&settings)?;
    Ok(settings.clone())
}

#[tauri::command]
pub fn settings_reset_hwid(state: State<'_, SettingsStore>) -> Result<Settings, String> {
    let mut settings = state.settings.lock().expect("settings poisoned");
    settings.reset_hwid();
    state.persist(&settings)?;
    Ok(settings.clone())
}
