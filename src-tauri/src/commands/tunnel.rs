//! Connection lifecycle commands.

use tauri::{AppHandle, Emitter, State};

use rumurule_core::{parse_link, Profile};

use crate::system_proxy::SystemProxy;
use crate::tun::{self, Prereqs};
use crate::xray::{Status, XrayManager, LOG_EVENT};

#[tauri::command]
pub fn core_name() -> &'static str {
    rumurule_core::AppInfo::NAME
}

#[tauri::command]
pub fn profile_from_link(link: String) -> Result<Profile, String> {
    parse_link(&link).map_err(|e| e.to_string())
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
