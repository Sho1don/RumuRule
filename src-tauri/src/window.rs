//! Main window: creation, geometry memory and the `RUMURULE_WINDOW` override.

use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};

/// Window size from the last close, so a rebuilt window comes back where the
/// user left it. The webview itself is gone at that point, so this is the only
/// place the size can live.
#[derive(Default)]
pub struct WindowState(Mutex<Option<(f64, f64)>>);

const DEFAULT_SIZE: (f64, f64) = (900.0, 820.0);
const MIN_SIZE: (f64, f64) = (360.0, 480.0);

/// How long a destroyed page gets before its process is stopped: WebKit writes
/// the web storage (last tab, chosen server, scroll) asynchronously, and
/// killing early throws those writes away.
pub const FLUSH: Duration = Duration::from_millis(1500);

/// Shows the window, building it again when the close button destroyed it.
pub fn open(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        return;
    }
    build(app);
}

/// Creates the window at the remembered size.
fn build(app: &AppHandle) {
    let size = app
        .try_state::<WindowState>()
        .and_then(|state| state.0.lock().ok().and_then(|size| *size))
        .unwrap_or(DEFAULT_SIZE);

    let built = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("RumuRule")
        .inner_size(size.0, size.1)
        .min_inner_size(MIN_SIZE.0, MIN_SIZE.1)
        .build();
    match built {
        Ok(window) => {
            lock_zoom(&window);
            let _ = window.set_focus();
        }
        Err(error) => eprintln!("[window] {error}"),
    }
}

/// Keep touchscreens from zooming the page. WebKitGTK runs pinch-to-zoom in
/// its own event controller on the web view, below the page, so CSS and JS
/// cannot cancel it; the controller itself has to be switched off. The window
/// is rebuilt after a tray close, hence the call from `build` as well.
pub fn lock_zoom(window: &tauri::WebviewWindow) {
    #[cfg(target_os = "linux")]
    linux::lock_zoom(window);
    #[cfg(not(target_os = "linux"))]
    let _ = window;
}

#[cfg(target_os = "linux")]
mod linux {
    use std::ffi::CString;

    use gtk::prelude::*;
    use webkit2gtk::WebViewExt;

    /// WebKit stores its pinch gesture on the view under this name.
    const ZOOM_GESTURE: &str = "wk-view-zoom-gesture";

    pub fn lock_zoom(window: &tauri::WebviewWindow) {
        let Ok(gtk_window) = window.gtk_window() else {
            return;
        };
        let Some(view) = find_webview(gtk_window.upcast_ref::<gtk::Widget>()) else {
            eprintln!("[window] web view not found for the zoom lock");
            return;
        };

        let key = CString::new(ZOOM_GESTURE).expect("static key");
        let gesture =
            unsafe { glib::gobject_ffi::g_object_get_data(view.as_ptr().cast(), key.as_ptr()) };
        if gesture.is_null() {
            eprintln!("[window] pinch gesture not found; zoom is reset instead");
        } else {
            // GTK_PHASE_NONE takes the controller out of event delivery.
            unsafe {
                gtk::ffi::gtk_event_controller_set_propagation_phase(
                    gesture.cast(),
                    gtk::ffi::GTK_PHASE_NONE,
                );
            }
        }

        // Belt and braces: if a WebKit update renames that key, the page still
        // stays at 100%. Nothing else in the app sets a zoom level.
        view.connect_zoom_level_notify(|view| {
            if (view.zoom_level() - 1.0).abs() > f64::EPSILON {
                view.set_zoom_level(1.0);
            }
        });
    }

    fn find_webview(widget: &gtk::Widget) -> Option<webkit2gtk::WebView> {
        if let Some(view) = widget.downcast_ref::<webkit2gtk::WebView>() {
            return Some(view.clone());
        }
        let container = widget.downcast_ref::<gtk::Container>()?;
        container
            .children()
            .into_iter()
            .find_map(|child| find_webview(&child))
    }
}

/// Remembers the size before the window is destroyed.
pub fn remember(window: &tauri::WebviewWindow) {
    let (Ok(size), Ok(scale)) = (window.inner_size(), window.scale_factor()) else {
        return;
    };
    let logical = size.to_logical::<f64>(scale);
    if let Some(state) = window.app_handle().try_state::<WindowState>() {
        if let Ok(mut slot) = state.0.lock() {
            *slot = Some((logical.width, logical.height));
        }
    }
}

/// Stops the WebKit web process left behind after the window is gone.
///
/// Tauri keeps the shared `WebContext` alive for the whole app on Linux so the
/// network process can be reused, and WebKit leaves the web process running
/// with it. That process is the expensive half of the UI (~140 MB) and has
/// nothing left to draw once the window is destroyed, so it is stopped here.
/// WebKit starts a fresh one when the window is built again.
pub fn release_web_processes(app: &AppHandle) {
    // A live view still needs its process.
    if app.get_webview_window("main").is_some() {
        return;
    }
    for pid in web_processes() {
        // The view is gone, so there is nothing to flush or close.
        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
    }
}

/// WebKit processes belonging to this app.
fn web_processes() -> Vec<u32> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    let own = std::process::id();
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|name| name.parse::<u32>().ok()) else {
            continue;
        };
        let Ok(status) = std::fs::read_to_string(entry.path().join("status")) else {
            continue;
        };
        // `Name` is cut to 15 characters, so the executable path is used
        // instead: "WebKitWebProcess" and "WebKitNetworkProcess" share a prefix.
        let Ok(cmdline) = std::fs::read(entry.path().join("cmdline")) else {
            continue;
        };
        let program = cmdline
            .split(|byte| *byte == 0)
            .next()
            .map(|arg| String::from_utf8_lossy(arg).into_owned())
            .unwrap_or_default();
        let is_web_process = program.ends_with("/WebKitWebProcess");
        let parent = status
            .lines()
            .find_map(|line| line.strip_prefix("PPid:"))
            .map(str::trim);
        let is_ours = parent.and_then(|ppid| ppid.parse::<u32>().ok()) == Some(own);
        if is_web_process && is_ours {
            found.push(pid);
        }
    }
    found
}

/// `RUMURULE_WINDOW=412x915` resizes the main window, handy for layout checks.
pub fn apply_override(app: &tauri::App) {
    let Ok(value) = std::env::var("RUMURULE_WINDOW") else {
        return;
    };
    let Some((width, height)) = value.split_once('x') else {
        return;
    };
    let (Ok(width), Ok(height)) = (width.trim().parse::<f64>(), height.trim().parse::<f64>()) else {
        return;
    };
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_size(LogicalSize::new(width, height));
    }
}
