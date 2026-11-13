//! User settings persisted next to the profile store.

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

/// Settings file name inside the application config directory.
pub const FILE_NAME: &str = "settings.json";

fn default_true() -> bool {
    true
}

fn new_hwid() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn default_ping_url() -> String {
    "https://www.gstatic.com/generate_204".into()
}

/// How server latency is measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PingKind {
    /// HTTP GET through a temporary Xray instance for that server.
    #[serde(alias = "proxy-get", alias = "proxy-head")]
    Proxy,
    /// TCP handshake straight to the server address.
    Tcp,
    /// ICMP echo to the server address.
    Icmp,
}

impl Default for PingKind {
    fn default() -> Self {
        Self::Tcp
    }
}

/// How traffic is captured while a tunnel is connected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TunnelMode {
    /// Only applications pointed at the local SOCKS5/HTTP endpoints.
    Proxy,
    /// A system-wide TUN interface, which needs the privileged helper.
    Tun,
}

impl Default for TunnelMode {
    fn default() -> Self {
        Self::Proxy
    }
}

/// Request strategy for proxy probes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PingMode {
    /// One request, measured as is.
    Default,
    /// Warm-up request with keep-alive disabled, then the measured one.
    Double,
    /// Warm-up request, then measure the reused TLS connection.
    Keepalive,
}

impl Default for PingMode {
    fn default() -> Self {
        Self::Default
    }
}

/// Which destinations the split tunnel sends around the tunnel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SplitMode {
    /// Everything goes through the server.
    Off,
    /// The listed domains and addresses go around the tunnel.
    Bypass,
    /// Only the listed domains and addresses go through the tunnel.
    Only,
}

impl Default for SplitMode {
    fn default() -> Self {
        Self::Off
    }
}

/// Split tunneling rules, applied to the generated configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitTunnel {
    #[serde(default)]
    pub mode: SplitMode,
    /// Domains, IP addresses and CIDR subnets, one per line.
    #[serde(default)]
    pub entries: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Send the device id headers panels use to bind a subscription to a device.
    #[serde(default = "default_true")]
    pub hwid_enabled: bool,
    /// Stable, randomly generated device id.
    #[serde(default = "new_hwid")]
    pub hwid: String,
    /// Point desktop apps at the local proxy while a tunnel is connected.
    #[serde(default = "default_true")]
    pub system_proxy: bool,
    /// Latency probe used by the server list.
    #[serde(default)]
    pub ping_kind: PingKind,
    /// Request strategy for proxy probes.
    #[serde(default)]
    pub ping_mode: PingMode,
    /// How traffic is captured while a tunnel is connected.
    #[serde(default)]
    pub tunnel_mode: TunnelMode,
    /// Reveal the IP endpoints of servers in the list and on the status tab.
    #[serde(default)]
    pub show_address: bool,
    /// URL requested when a proxy probe is selected.
    #[serde(default = "default_ping_url")]
    pub ping_url: String,
    /// Destinations that bypass the tunnel, or the only ones allowed into it.
    #[serde(default)]
    pub split: SplitTunnel,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hwid_enabled: true,
            hwid: new_hwid(),
            system_proxy: true,
            ping_kind: PingKind::default(),
            ping_mode: PingMode::default(),
            tunnel_mode: TunnelMode::default(),
            show_address: false,
            ping_url: default_ping_url(),
            split: SplitTunnel::default(),
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Result<Self, String> {
        match fs::read_to_string(path) {
            Ok(raw) => {
                let mut settings: Self = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
                if settings.hwid.trim().is_empty() {
                    settings.hwid = new_hwid();
                }
                if settings.ping_url.trim().is_empty() {
                    settings.ping_url = default_ping_url();
                }
                Ok(settings)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let body = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, body).map_err(|e| e.to_string())
    }

    /// Fresh device id; the user can reset it when a panel keeps the old binding.
    pub fn reset_hwid(&mut self) {
        self.hwid = new_hwid();
    }

    pub fn path_in(dir: &Path) -> PathBuf {
        dir.join(FILE_NAME)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_enabled_with_an_id() {
        let settings = Settings::default();
        assert!(settings.hwid_enabled);
        assert!(settings.system_proxy);
        assert_eq!(settings.hwid.len(), 36);
        assert_eq!(settings.ping_kind, PingKind::Tcp);
        assert_eq!(settings.ping_mode, PingMode::Default);
        assert_eq!(settings.tunnel_mode, TunnelMode::Proxy);
        assert!(!settings.show_address);
        assert!(settings.ping_url.starts_with("https://"));
    }

    #[test]
    fn roundtrip_keeps_the_id() {
        let dir = std::env::temp_dir().join("rumurule-settings-test");
        let path = Settings::path_in(&dir);
        let _ = fs::remove_dir_all(&dir);

        let settings = Settings::default();
        settings.save(&path).unwrap();
        let loaded = Settings::load(&path).unwrap();

        assert_eq!(loaded.hwid, settings.hwid);
        assert!(loaded.hwid_enabled);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let settings: Settings = serde_json::from_str("{}").unwrap();
        assert!(settings.hwid_enabled);
        assert!(settings.system_proxy);
        assert!(!settings.hwid.is_empty());
        assert_eq!(settings.ping_kind, PingKind::Tcp);
        assert_eq!(settings.tunnel_mode, TunnelMode::Proxy);
        assert!(!settings.show_address);
    }

    #[test]
    fn old_proxy_probe_names_migrate() {
        assert_eq!(
            serde_json::from_str::<PingKind>("\"proxy-get\"").unwrap(),
            PingKind::Proxy
        );
        assert_eq!(
            serde_json::from_str::<PingKind>("\"proxy-head\"").unwrap(),
            PingKind::Proxy
        );
    }
}
