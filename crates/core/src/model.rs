//! Data structures shared by the parser, the config builder and the store.

use serde::{Deserialize, Serialize};

/// Transport used between the client and the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    Tcp,
    Ws,
    Grpc,
    Http,
}

impl Network {
    pub fn parse(value: &str) -> Self {
        match value {
            "ws" | "websocket" => Self::Ws,
            "grpc" | "gun" => Self::Grpc,
            "http" | "h2" => Self::Http,
            _ => Self::Tcp,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tcp => "tcp",
            Self::Ws => "ws",
            Self::Grpc => "grpc",
            Self::Http => "http",
        }
    }
}

/// Transport security layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Security {
    None,
    Tls,
    Reality,
}

impl Security {
    pub fn parse(value: &str) -> Self {
        match value {
            "tls" | "xtls" => Self::Tls,
            "reality" => Self::Reality,
            _ => Self::None,
        }
    }
}

/// A single server entry imported from a share link.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub name: String,
    /// `vless`, `vmess`, `trojan` or `shadowsocks`.
    pub protocol: String,
    pub server: String,
    pub port: u16,
    /// UUID, password or `user:password` pair depending on the protocol.
    pub id: String,
    pub network: Network,
    pub security: Security,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sni: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alpn: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub short_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encryption: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    /// Hysteria 2 obfuscation (`salamander`), empty when the server has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub obfs: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub obfs_password: Option<String>,
    /// Pinned certificate hash from `hysteria2://…?pinSHA256=…`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin_sha256: Option<String>,
    /// Hysteria 2 accepts any certificate when the link says `insecure=1`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub insecure: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl Profile {
    /// Human readable label used by the UI and the tray menu.
    pub fn label(&self) -> String {
        if self.name.is_empty() {
            format!("{}:{}", self.server, self.port)
        } else {
            self.name.clone()
        }
    }

    /// Server name used for the TLS handshake.
    pub fn server_name(&self) -> &str {
        self.sni.as_deref().unwrap_or(&self.server)
    }
}

/// One saved server, optionally imported from a subscription.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    pub link: String,
    pub profile: Profile,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subscription: Option<String>,
    /// Full Xray config for subscriptions that ship ready-made JSON.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<serde_json::Value>,
    /// Pinned entries stay on top of their group and survive a refresh.
    #[serde(default)]
    pub pinned: bool,
}

/// A remote list of servers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Subscription {
    pub id: String,
    pub url: String,
    #[serde(default)]
    pub title: String,
}
