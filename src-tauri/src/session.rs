//! Background supervision of a running session.
//!
//! Two jobs: keep the tray menu in sync, and act as the fuse when a session
//! ends without the user asking for it. The fuse lives here rather than in a
//! command so it also works while the window is hidden in the tray.

use std::{thread, time::Duration};

use tauri::{AppHandle, Emitter, Manager};

use crate::system_proxy::SystemProxy;
use crate::tray::Tray;
use crate::xray::{Status, XrayManager, LOG_EVENT};

/// Emitted when a session ended on its own: the UI shows it to the user.
pub const ENDED_EVENT: &str = "tunnel-ended";
const INTERVAL: Duration = Duration::from_secs(2);

pub fn spawn(app: AppHandle) {
    thread::spawn(move || loop {
        thread::sleep(INTERVAL);
        let manager = app.state::<XrayManager>();
        let status = manager.status();
        if let Some(tray) = app.try_state::<Tray>() {
            tray.sync(&status);
        }
        if manager.take_died() {
            end_unexpectedly(&app, status);
        }
    });
}

/// The fuse: a dead session must not leave the desktop pointing at a dead port
/// and must not stay silent either.
fn end_unexpectedly(app: &AppHandle, status: Status) {
    let proxy = app.state::<SystemProxy>();
    if let Err(error) = proxy.disable() {
        let _ = app.emit(LOG_EVENT, format!("system proxy: {error}"));
    }
    let _ = app.emit(
        LOG_EVENT,
        "connection ended unexpectedly; system settings restored",
    );
    let _ = app.emit(ENDED_EVENT, status);
}
