//! Locating the Xray binary and the generated configuration file.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

/// Where a packaged install keeps the core, next to the helper.
const SYSTEM_XRAY: &str = "/usr/lib/rumurule/xray";

/// Resolution order: `RUMURULE_XRAY`, the copy `fetch-xray.sh` installed for
/// this user, the package copy, then `xray` from `PATH`.
pub(super) fn find_binary(app: &AppHandle) -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("RUMURULE_XRAY") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
        return Err(format!(
            "RUMURULE_XRAY points to a missing file: {}",
            path.display()
        ));
    }

    if let Ok(dir) = app.path().app_data_dir() {
        let bundled = dir.join("bin").join("xray");
        if bundled.is_file() {
            return Ok(bundled);
        }
    }

    if Path::new(SYSTEM_XRAY).is_file() {
        return Ok(PathBuf::from(SYSTEM_XRAY));
    }

    find_on_path("xray").ok_or_else(|| {
        "xray binary not found: install the package, set RUMURULE_XRAY or run scripts/fetch-xray.sh"
            .into()
    })
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
        .and_then(|candidate| candidate.canonicalize().ok())
}

pub(super) fn config_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("cannot resolve config dir: {e}"))?;
    Ok(dir.join("xray.json"))
}

/// Ask the binary for its release, e.g. `26.11.9`.
pub(super) fn version(binary: &Path) -> Result<String, String> {
    let output = std::process::Command::new(binary)
        .arg("version")
        .output()
        .map_err(|e| format!("cannot run {}: {e}", binary.display()))?;
    if !output.status.success() {
        return Err(format!("{} version exited with {}", binary.display(), output.status));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_version(&stdout).ok_or_else(|| "unexpected version output".to_string())
}

/// `Xray 26.11.9 (Xray, Penetrates Everything.) …` → `26.11.9`.
fn parse_version(output: &str) -> Option<String> {
    output
        .lines()
        .next()?
        .split_whitespace()
        .nth(1)
        .filter(|token| token.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::parse_version;

    #[test]
    fn reads_the_release_from_version_output() {
        let output = "Xray 26.11.9 (Xray, Penetrates Everything.) 676eaf6 (go1.27.3 linux/amd64)\n";
        assert_eq!(parse_version(output).as_deref(), Some("26.11.9"));
        assert_eq!(parse_version("garbage"), None);
    }
}
