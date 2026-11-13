#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod session;
mod system_proxy;
mod theme;
mod tray;
mod tun;
mod window;
mod xray;

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{Manager, RunEvent};

use commands::profiles::Profiles;
use commands::settings::SettingsStore;
use system_proxy::SystemProxy;
use xray::XrayManager;

/// Set when the user asked to quit from the tray, so the "keep running while
/// the tray exists" rule does not hold the process back.
#[derive(Default)]
pub struct Quitting(pub AtomicBool);

/// `libayatana-appindicator3` announces its own deprecation through `g_log`
/// when the tray loads it. That advice belongs to application authors, not to
/// the people running the app, and it lands in the terminal of every user, so
/// the one line is dropped. Every other GLib message still goes through the
/// default writer unchanged.
fn quiet_appindicator_notice() {
    glib::log_set_writer_func(|level, fields| {
        let notice = fields.iter().any(|field| {
            field.key() == "MESSAGE"
                && field
                    .value_str()
                    .is_some_and(|text| text.contains("libayatana-appindicator-glib"))
        });
        if notice {
            glib::LogWriterOutput::Handled
        } else {
            glib::log_writer_default(level, fields)
        }
    });
}

fn main() {
    quiet_appindicator_notice();

    tauri::Builder::default()
        .manage(XrayManager::default())
        .manage(Quitting::default())
        .manage(window::WindowState::default())
        .setup(|app| {
            let profiles = Profiles::open(app.handle()).map_err(std::io::Error::other)?;
            app.manage(profiles);
            let settings = SettingsStore::open(app.handle()).map_err(std::io::Error::other)?;
            app.manage(settings);
            let proxy = SystemProxy::open(app.handle()).map_err(std::io::Error::other)?;
            app.manage(proxy);
            theme::spawn_watcher(app.handle().clone());
            if let Err(error) = tray::build(app.handle()) {
                eprintln!("[tray] {error}");
            }
            session::spawn(app.handle().clone());
            window::apply_override(app);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::tunnel::core_name,
            commands::tunnel::profile_from_link,
            commands::tunnel::connect,
            commands::tunnel::disconnect,
            commands::tunnel::tunnel_status,
            commands::tunnel::tun_prereqs,
            commands::tunnel::helper_install,
            commands::tunnel::setup_state,
            commands::profiles::profiles_list,
            commands::profiles::profile_add,
            commands::profiles::profile_rename,
            commands::profiles::profile_pin,
            commands::profiles::profile_remove,
            commands::profiles::subscriptions_list,
            commands::profiles::subscription_add,
            commands::profiles::subscription_refresh,
            commands::profiles::subscription_rename,
            commands::profiles::subscription_remove,
            commands::profiles::connect_entry,
            commands::ping::ping_entry,
            commands::settings::settings_get,
            commands::settings::settings_set_hwid,
            commands::settings::settings_set_show_address,
            commands::settings::settings_set_system_proxy,
            commands::settings::settings_set_ping,
            commands::settings::settings_set_split,
            commands::settings::settings_set_tunnel_mode,
            commands::settings::settings_reset_hwid,
            theme::system_theme
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // With a tray the window is destroyed rather than hidden: a
                // hidden webview keeps WebKit alive and costs more memory than
                // the whole Rust side. The tray rebuilds it on demand.
                // Without a tray, closing still quits the app.
                if window.app_handle().try_state::<tray::Tray>().is_some() {
                    api.prevent_close();
                    if let Some(webview) = window.app_handle().get_webview_window(window.label()) {
                        window::remember(&webview);
                    }
                    let app = window.app_handle().clone();
                    let _ = window.destroy();
                    // WebKit unwinds the page asynchronously and writes its
                    // storage on the way out; the process is stopped after
                    // that, so the last tab and server survive a close.
                    std::thread::spawn(move || {
                        std::thread::sleep(window::FLUSH);
                        window::release_web_processes(&app);
                    });
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("failed to build RumuRule")
        .run(|app, event| match event {
            // Destroying the last window asks the app to exit; the tray keeps
            // it alive so the session and its fuse survive.
            RunEvent::ExitRequested { api, .. } => {
                if app.try_state::<tray::Tray>().is_some() && !app.state::<Quitting>().0.load(Ordering::Relaxed) {
                    api.prevent_exit();
                }
            }
            RunEvent::Exit => {
                let proxy = app.state::<SystemProxy>();
                let _ = proxy.disable();
                let manager = app.state::<XrayManager>();
                let _ = manager.stop();
            }
            _ => {}
        });
}
