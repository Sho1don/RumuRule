//! GNOME proxy settings shared through `gsettings`.

use std::process::Command;

use serde::{Deserialize, Serialize};

use rumurule_core::{proxy, Endpoints};

/// Everything changed by [`apply`], so it can be put back exactly.
const KEYS: [(&str, &str); 9] = [
    ("org.gnome.system.proxy", "mode"),
    ("org.gnome.system.proxy", "ignore-hosts"),
    ("org.gnome.system.proxy", "use-same-proxy"),
    ("org.gnome.system.proxy.http", "host"),
    ("org.gnome.system.proxy.http", "port"),
    ("org.gnome.system.proxy.https", "host"),
    ("org.gnome.system.proxy.https", "port"),
    ("org.gnome.system.proxy.socks", "host"),
    ("org.gnome.system.proxy.socks", "port"),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub schema: String,
    pub key: String,
    pub value: String,
}

pub fn available() -> bool {
    Command::new("gsettings")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Raw values (`'none'`, `10809`, `['localhost']`) before any change.
pub fn snapshot() -> Option<Vec<Entry>> {
    if !available() {
        return None;
    }

    let mut entries = Vec::with_capacity(KEYS.len());
    for (schema, key) in KEYS {
        let output = Command::new("gsettings")
            .args(["get", schema, key])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        entries.push(Entry {
            schema: schema.into(),
            key: key.into(),
            value: String::from_utf8_lossy(&output.stdout).trim().to_string(),
        });
    }
    Some(entries)
}

pub fn apply(endpoints: Endpoints) -> Result<(), String> {
    if !available() {
        return Ok(());
    }

    for (schema, key, value) in proxy::gnome_settings(endpoints) {
        set(schema, key, &value)?;
    }
    Ok(())
}

pub fn restore(entries: &[Entry]) -> Result<(), String> {
    if !available() {
        return Ok(());
    }

    for entry in entries {
        set(&entry.schema, &entry.key, &entry.value)?;
    }
    Ok(())
}

fn set(schema: &str, key: &str, value: &str) -> Result<(), String> {
    let output = Command::new("gsettings")
        .args(["set", schema, key, value])
        .output()
        .map_err(|e| format!("cannot run gsettings: {e}"))?;
    if output.status.success() {
        return Ok(());
    }

    let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(format!("gsettings failed for {schema} {key}: {error}"))
}
