//! Connection lifecycle commands.

use tauri::{AppHandle, Emitter, State};

use rumurule_core::{parse_link, Profile, TunnelMode};

use crate::commands::settings::{apply_proxy, SettingsStore};
use crate::system_proxy::SystemProxy;
use crate::tun::{self, Prereqs};
use crate::xray::{emit_progress, Status, XrayManager, LOG_EVENT};

#[tauri::command]
pub fn core_name() -> &'static str {
    rumurule_core::AppInfo::NAME
}

#[tauri::command]
pub fn profile_from_link(link: String) -> Result<Profile, String> {
    parse_link(&link).map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn connect(
    app: AppHandle,
    state: State<'_, XrayManager>,
    settings: State<'_, SettingsStore>,
    proxy: State<'_, SystemProxy>,
    link: String,
) -> Result<Status, String> {
    let profile = parse_link(&link).map_err(|e| e.to_string())?;
    let mode = settings.snapshot().tunnel_mode;
    let status = match mode {
        TunnelMode::Tun => state.start_tun(&app, &rumurule_core::config::build_config(&profile))?,
        TunnelMode::Proxy => state.start(&app, &profile)?,
    };

    apply_proxy(&app, &settings, &proxy, &status);
    emit_progress(&app, 1.0, "connected");
    Ok(status)
}

#[tauri::command(async)]
pub fn disconnect(
    app: AppHandle,
    state: State<'_, XrayManager>,
    proxy: State<'_, SystemProxy>,
) -> Result<Status, String> {
    if let Err(error) = proxy.disable() {
        let _ = app.emit(LOG_EVENT, format!("system proxy: {error}"));
    }
    state.stop()
}

/// Runs off the main thread: polling must never freeze the window.
#[tauri::command(async)]
pub fn tunnel_status(state: State<'_, XrayManager>) -> Result<Status, String> {
    Ok(state.status())
}

/// What TUN mode needs: the helper, Xray, polkit and IPv6 routing.
#[tauri::command]
pub fn tun_prereqs(app: AppHandle) -> Prereqs {
    tun::prereqs(&app)
}

/// Ask polkit for the tunnel action once and report the helper environment.
#[tauri::command(async)]
pub fn tun_authorize(app: AppHandle) -> Result<String, String> {
    tun::authorize(&app)
}

/// One-time privileged setup for passwordless tunnel starts.
#[tauri::command(async)]
pub fn helper_install(app: AppHandle) -> Result<String, String> {
    tun::install(&app)
}

/// Whether that setup already covers the current user.
#[tauri::command]
pub fn setup_state(app: AppHandle) -> tun::Setup {
    tun::setup_state(&app)
}
