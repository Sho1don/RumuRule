//! `vmess://` links (base64-encoded JSON payload).

use serde_json::Value;

use super::{alpn_list, base64_decode, network_from, Error, Result};
use crate::model::{Profile, Security};

pub(super) fn parse(rest: &str) -> Result<Profile> {
    let payload = rest.split('#').next().unwrap_or(rest);
    let json = base64_decode(payload)?;
    let value: Value =
        serde_json::from_str(&json).map_err(|e| Error::Malformed(e.to_string()))?;

    let text = |key: &str| -> Option<String> {
        match value.get(key) {
            Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
            Some(Value::Number(n)) => Some(n.to_string()),
            _ => None,
        }
    };

    let server = text("add").ok_or(Error::MissingField("address"))?;
    let port: u16 = text("port")
        .ok_or(Error::MissingField("port"))?
        .parse()
        .map_err(|_| Error::InvalidPort(text("port").unwrap_or_default()))?;
    let id = text("id").ok_or(Error::MissingField("uuid"))?;
    let tls = text("tls").map(|v| v != "none").unwrap_or(false);
    let host = text("host");

    Ok(Profile {
        name: text("ps").unwrap_or_default(),
        protocol: "vmess".into(),
        server,
        port,
        id,
        network: network_from(text("net")),
        security: if tls { Security::Tls } else { Security::None },
        sni: text("sni").or_else(|| host.clone()),
        alpn: alpn_list(text("alpn")),
        fingerprint: text("fp"),
        flow: None,
        path: text("path"),
        host,
        service_name: text("path"),
        public_key: None,
        short_id: None,
        encryption: Some(text("scy").unwrap_or_else(|| "auto".into())),
        method: None,
        obfs: None,
        obfs_password: None,
        pin_sha256: None,
        insecure: false,
    })
}

#[cfg(test)]
mod tests {
    use super::super::parse_link;
    use crate::model::{Network, Security};
    use base64::Engine;

    #[test]
    fn parses_vmess() {
        let json = r#"{"v":"2","ps":"VM","add":"vm.example.com","port":"443","id":"uuid-1","net":"ws","path":"/ray","tls":"tls","sni":"vm.example.com"}"#;
        let encoded = base64::engine::general_purpose::STANDARD.encode(json);
        let p = parse_link(&format!("vmess://{encoded}")).expect("vmess");
        assert_eq!(p.protocol, "vmess");
        assert_eq!(p.server, "vm.example.com");
        assert_eq!(p.network, Network::Ws);
        assert_eq!(p.security, Security::Tls);
    }
}
