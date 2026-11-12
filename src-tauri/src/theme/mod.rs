//! Desktop color integration: read the KDE scheme and stream changes to the UI.

mod kdeglobals;

use std::{
    fs,
    path::PathBuf,
    thread,
    time::{Duration, SystemTime},
};

use tauri::{AppHandle, Emitter};

use kdeglobals::parse_kdeglobals;

pub use kdeglobals::SystemTheme;

/// Event name used to push desktop color changes to the UI.
pub const THEME_EVENT: &str = "system-theme";

/// Read the KDE color scheme. Returns `None` when the file is unavailable.
#[tauri::command]
pub fn system_theme() -> Option<SystemTheme> {
    read_theme()
}

/// Watch `kdeglobals` and re-emit the palette whenever the desktop theme changes.
pub fn spawn_watcher(app: AppHandle) {
    thread::spawn(move || {
        let mut last = modified_at();
        loop {
            thread::sleep(Duration::from_millis(1200));
            let current = modified_at();
            if current == last {
                continue;
            }
            last = current;
            if let Some(theme) = read_theme() {
                let _ = app.emit(THEME_EVENT, theme);
            }
        }
    });
}

fn read_theme() -> Option<SystemTheme> {
    let path = config_dir()?.join("kdeglobals");
    let text = fs::read_to_string(path).ok()?;
    parse_kdeglobals(&text)
}

fn modified_at() -> Option<SystemTime> {
    let path = config_dir()?.join("kdeglobals");
    fs::metadata(path).and_then(|meta| meta.modified()).ok()
}

fn config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
}
