//! Status-area icon: session state at a glance and quick actions.

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

use crate::system_proxy::SystemProxy;
use crate::xray::{Status, XrayManager};

/// Menu handles the background watcher keeps in sync with the session.
pub struct Tray {
    status: MenuItem<tauri::Wry>,
    disconnect: MenuItem<tauri::Wry>,
}

impl Tray {
    /// Reflects the current session in the menu.
    pub fn sync(&self, status: &Status) {
        let label = if !status.running {
            "Disconnected".to_string()
        } else if status.mode == "tun" {
            "Connected — TUN".to_string()
        } else {
            "Connected — proxy".to_string()
        };
        let _ = self.status.set_text(label);
        let _ = self.disconnect.set_enabled(status.running);
    }
}

/// Builds the tray icon. A desktop without a status area is not an error.
pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let status = MenuItem::with_id(app, "status", "Disconnected", false, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Open RumuRule", true, None::<&str>)?;
    let disconnect = MenuItem::with_id(app, "disconnect", "Disconnect", false, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&status, &separator, &open, &disconnect, &quit])?;

    let mut builder = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("RumuRule")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show(app),
            "disconnect" => {
                let proxy = app.state::<SystemProxy>();
                let _ = proxy.disable();
                let _ = app.state::<XrayManager>().stop();
            }
            "quit" => {
                let proxy = app.state::<SystemProxy>();
                let _ = proxy.disable();
                let _ = app.state::<XrayManager>().stop();
                app.state::<crate::Quitting>().0.store(true, std::sync::atomic::Ordering::Relaxed);
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show(tray.app_handle());
            }
        });

    // Feed the tray a real PNG: without one the panel shows a generic symbol.
    let icon = match tauri::image::Image::from_bytes(include_bytes!(
        "../icons/128x128.png"
    )) {
        Ok(icon) => Some(icon),
        Err(_) => app.default_window_icon().cloned(),
    };
    if let Some(icon) = icon {
        builder = builder.icon(icon);
    }

    builder.build(app)?;
    app.manage(Tray { status, disconnect });
    Ok(())
}

/// Brings the main window back, rebuilding it when the tray destroyed it.
fn show(app: &AppHandle) {
    crate::window::open(app);
}
