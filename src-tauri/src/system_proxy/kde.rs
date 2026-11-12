//! KDE proxy settings stored in `kioslaverc`.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use rumurule_core::{proxy, Endpoints};

/// KDE apps read this file from the user's config directory.
pub const FILE_NAME: &str = "kioslaverc";
const GROUP: &str = "Proxy Settings";

pub fn available() -> bool {
    Command::new("kwriteconfig6")
        .arg("--help")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Copy the current file aside. Returns whether there was anything to keep.
pub fn snapshot(path: &Path, backup: &Path) -> Result<bool, String> {
    if !path.exists() {
        return Ok(false);
    }
    fs::copy(path, backup)
        .map_err(|e| format!("cannot back up {}: {e}", path.display()))?;
    Ok(true)
}

/// Write every proxy key and ask KDE to reload the configuration.
pub fn apply(endpoints: Endpoints) -> Result<(), String> {
    for (key, value) in proxy::kde_settings(endpoints) {
        let mut command = Command::new("kwriteconfig6");
        command.args([
            "--file",
            FILE_NAME,
            "--group",
            GROUP,
            "--key",
            key,
            "--notify",
        ]);
        if key == "ReversedException" {
            command.args(["--type", "bool"]);
        }
        let output = command
            .arg(&value)
            .output()
            .map_err(|e| format!("cannot run kwriteconfig6: {e}"))?;
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(format!("kwriteconfig6 failed for {key}: {error}"));
        }
    }
    Ok(())
}

/// Put the file back exactly as it was, or remove it when it did not exist.
pub fn restore(path: &Path, backup: &Path, existed: bool) -> Result<(), String> {
    if existed {
        return fs::copy(backup, path)
            .map(|_| ())
            .map_err(|e| format!("cannot restore {}: {e}", path.display()));
    }

    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("cannot remove {}: {e}", path.display())),
    }
}

/// Default location inside a config directory.
pub fn config_path(config_dir: &Path) -> PathBuf {
    config_dir.join(FILE_NAME)
}
