//! System proxy: point desktop apps at the local Xray ports and put the
//! previous settings back when the tunnel stops.

mod gnome;
mod kde;

use std::{
    fs,
    path::PathBuf,
    sync::Mutex,
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use rumurule_core::Endpoints;

const STATE_FILE: &str = "state.json";
const BACKUP_FILE: &str = "kioslaverc.bak";
const DIR_NAME: &str = "proxy";

/// Snapshot needed to undo an enable.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct State {
    enabled: bool,
    had_kioslaverc: bool,
    #[serde(default)]
    gnome: Option<Vec<gnome::Entry>>,
}

pub struct SystemProxy {
    dir: PathBuf,
    state_path: PathBuf,
    kioslaverc: PathBuf,
    backup: PathBuf,
    state: Mutex<Option<State>>,
}

impl SystemProxy {
    /// Open the manager and restore settings left behind by a crash.
    pub fn open(app: &AppHandle) -> Result<Self, String> {
        let config_dir = app
            .path()
            .config_dir()
            .map_err(|e| format!("cannot resolve config dir: {e}"))?;
        let app_dir = app
            .path()
            .app_config_dir()
            .map_err(|e| format!("cannot resolve app config dir: {e}"))?;
        let dir = app_dir.join(DIR_NAME);
        fs::create_dir_all(&dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;

        let proxy = Self {
            state_path: dir.join(STATE_FILE),
            kioslaverc: kde::config_path(&config_dir),
            backup: dir.join(BACKUP_FILE),
            dir,
            state: Mutex::new(None),
        };

        if let Some(state) = proxy.load_state() {
            if state.enabled {
                if let Err(error) = proxy.restore_with(&state) {
                    eprintln!("[proxy] leftover settings not restored: {error}");
                } else {
                    eprintln!("[proxy] restored settings from a previous session");
                }
            } else {
                let _ = fs::remove_file(&proxy.state_path);
            }
        }

        Ok(proxy)
    }

    /// Apply the desktop proxy settings, saving the previous values once.
    pub fn enable(&self, endpoints: Endpoints) -> Result<(), String> {
        if !kde::available() && !gnome::available() {
            return Err("no supported desktop proxy settings found".into());
        }

        let mut slot = self.state.lock().expect("proxy state poisoned");
        if slot.is_none() {
            *slot = Some(self.snapshot()?);
        }
        let state = slot.as_ref().expect("snapshot just stored").clone();

        if let Err(error) = self.apply(endpoints) {
            if let Err(rollback) = self.restore_with(&state) {
                eprintln!("[proxy] rollback failed: {rollback}");
            }
            *slot = None;
            return Err(error);
        }

        Ok(())
    }

    /// Put the desktop settings back. Safe to call when nothing was applied.
    pub fn disable(&self) -> Result<(), String> {
        let mut slot = self.state.lock().expect("proxy state poisoned");
        let Some(state) = slot.take() else {
            return Ok(());
        };
        self.restore_with(&state)
    }

    fn apply(&self, endpoints: Endpoints) -> Result<(), String> {
        if kde::available() {
            kde::apply(endpoints)?;
        }
        if gnome::available() {
            gnome::apply(endpoints)?;
        }
        Ok(())
    }

    fn snapshot(&self) -> Result<State, String> {
        fs::create_dir_all(&self.dir)
            .map_err(|e| format!("cannot create {}: {e}", self.dir.display()))?;
        let had_kioslaverc = kde::snapshot(&self.kioslaverc, &self.backup)?;
        let state = State {
            enabled: true,
            had_kioslaverc,
            gnome: gnome::snapshot(),
        };

        let body = serde_json::to_vec_pretty(&state).map_err(|e| e.to_string())?;
        fs::write(&self.state_path, body)
            .map_err(|e| format!("cannot write {}: {e}", self.state_path.display()))?;
        Ok(state)
    }

    fn restore_with(&self, state: &State) -> Result<(), String> {
        let mut errors = Vec::new();

        if let Err(error) = kde::restore(&self.kioslaverc, &self.backup, state.had_kioslaverc) {
            errors.push(error);
        }
        if let Some(entries) = &state.gnome {
            if let Err(error) = gnome::restore(entries) {
                errors.push(error);
            }
        }

        match fs::remove_file(&self.state_path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => errors.push(format!(
                "cannot remove {}: {error}",
                self.state_path.display()
            )),
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    fn load_state(&self) -> Option<State> {
        let raw = fs::read_to_string(&self.state_path).ok()?;
        serde_json::from_str(&raw).ok()
    }
}
