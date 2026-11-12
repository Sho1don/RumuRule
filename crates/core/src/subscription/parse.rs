//! Parsing subscription bodies: plain lists or base64-encoded ones.

use base64::Engine;
use serde_json::Value;

use crate::{
    model::{Network, Security},
    parse_link, Profile,
};

/// One server imported from a subscription, either as a share link or as a
/// ready-made Xray configuration.
#[derive(Debug, Clone)]
pub struct Imported {
    pub link: String,
    pub profile: Profile,
    pub config: Option<Value>,
}

/// Parse a subscription body: plain text with one link per line, or the same
/// list encoded as base64 (the format used by most providers).
pub fn parse_subscription(body: &str) -> Vec<Imported> {
    decode_body(body)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter(|line| line.contains("://"))
        .filter_map(|line| {
            parse_link(line)
                .ok()
                .map(|profile| Imported {
                    link: line.to_string(),
                    profile,
                    config: None,
                })
        })
        .collect()
}

/// Panels may answer with a JSON array of complete Xray configs instead of
/// share links. Returns `None` when the body is not such a document.
pub fn parse_configs(body: &str) -> Option<Vec<Imported>> {
    let trimmed = body.trim();
    if !(trimmed.starts_with('[') || trimmed.starts_with('{')) {
        return None;
    }
    let value: Value = serde_json::from_str(trimmed).ok()?;
    let items: Vec<&Value> = match &value {
        Value::Array(items) => items.iter().collect(),
        Value::Object(_) => vec![&value],
        _ => return None,
    };

    let configs: Vec<Imported> = items
        .iter()
        .filter(|item| item.get("outbounds").is_some())
        .enumerate()
        .map(|(index, item)| {
            let name = item
                .get("remarks")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| format!("Config {}", index + 1));
            Imported {
                link: String::new(),
                profile: summarize(&name, item),
                config: Some((*item).clone()),
            }
        })
        .collect();

    (!configs.is_empty()).then_some(configs)
}

/// Build a display-only profile from the first real outbound of a config.
fn summarize(name: &str, config: &Value) -> Profile {
    let mut profile = Profile {
        name: name.to_string(),
        protocol: "config".into(),
        server: String::new(),
        port: 0,
        id: String::new(),
        network: Network::Tcp,
        security: Security::None,
        sni: None,
        alpn: Vec::new(),
        fingerprint: None,
        flow: None,
        path: None,
        host: None,
        service_name: None,
        public_key: None,
        short_id: None,
        encryption: None,
        method: None,
        obfs: None,
        obfs_password: None,
        pin_sha256: None,
        insecure: false,
    };

    let Some(outbounds) = config.get("outbounds").and_then(Value::as_array) else {
        return profile;
    };

    for outbound in outbounds {
        let protocol = outbound
            .get("protocol")
            .and_then(Value::as_str)
            .unwrap_or("");
        if protocol.is_empty() || protocol == "freedom" || protocol == "blackhole" {
            continue;
        }

        profile.protocol = protocol.to_string();
        let settings = &outbound["settings"];

        if let Some(target) = settings
            .get("vnext")
            .and_then(Value::as_array)
            .and_then(|list| list.first())
        {
            profile.server = target["address"].as_str().unwrap_or("").to_string();
            profile.port = target["port"].as_u64().unwrap_or(0) as u16;
            profile.id = target["id"]
                .as_str()
                .or_else(|| {
                    target["users"]
                        .as_array()
                        .and_then(|users| users.first())
                        .and_then(|user| user["id"].as_str())
                })
                .unwrap_or("")
                .to_string();
        } else if let Some(target) = settings
            .get("servers")
            .and_then(Value::as_array)
            .and_then(|list| list.first())
        {
            profile.server = target["address"].as_str().unwrap_or("").to_string();
            profile.port = target["port"].as_u64().unwrap_or(0) as u16;
            profile.id = target["password"].as_str().unwrap_or("").to_string();
            profile.method = target["method"].as_str().map(str::to_string);
        } else {
            profile.server = settings["address"].as_str().unwrap_or("").to_string();
            profile.port = settings["port"].as_u64().unwrap_or(0) as u16;
            profile.id = settings["password"].as_str().unwrap_or("").to_string();
        }

        let stream = &outbound["streamSettings"];
        profile.network = Network::parse(stream["network"].as_str().unwrap_or("tcp"));
        profile.security = Security::parse(stream["security"].as_str().unwrap_or("none"));
        profile.sni = stream["tlsSettings"]["serverName"]
            .as_str()
            .or_else(|| stream["realitySettings"]["serverName"].as_str())
            .map(str::to_string);
        break;
    }

    profile
}

/// Panels answer a blocked or expired client with one repeated server link
/// whose fragment carries a human-readable message (for example a device limit
/// warning). Detect that instead of importing the same server several times.
pub fn panel_error(imported: &[Imported]) -> Option<String> {
    if imported.len() < 2 {
        return None;
    }

    let mut bases = imported
        .iter()
        .map(|item| item.link.split('#').next().unwrap_or(&item.link));
    let first = bases.next()?;
    if !bases.all(|base| base == first) {
        return None;
    }

    let mut messages: Vec<&str> = imported
        .iter()
        .map(|item| item.profile.name.trim())
        .filter(|name| !name.is_empty())
        .collect();
    messages.sort_unstable();
    messages.dedup();

    Some(if messages.is_empty() {
        "the panel returned a repeated server entry".to_string()
    } else {
        messages.join(" / ")
    })
}

fn decode_body(body: &str) -> String {
    let trimmed = body.trim();

    if trimmed.contains("://") {
        return trimmed.to_string();
    }

    let compact: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();

    for engine in [
        &base64::engine::general_purpose::STANDARD,
        &base64::engine::general_purpose::STANDARD_NO_PAD,
        &base64::engine::general_purpose::URL_SAFE,
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
    ] {
        if let Ok(bytes) = engine.decode(&compact) {
            if let Ok(text) = String::from_utf8(bytes) {
                if text.contains("://") {
                    return text;
                }
            }
        }
    }

    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD;

    const LINKS: &str = "vless://uuid-a@a.example.com:443?security=tls#Alpha\n\
trojan://secret@b.example.com:8443?sni=b.example.com#Beta\n\
ss://YWVzLTI1Ni1nY206cGFzcw@c.example.com:8388#Gamma\n";

    #[test]
    fn parses_plain_list() {
        let imported = parse_subscription(LINKS);
        assert_eq!(imported.len(), 3);
        assert_eq!(imported[0].profile.name, "Alpha");
        assert_eq!(imported[2].profile.protocol, "shadowsocks");
    }

    #[test]
    fn parses_base64_list() {
        let encoded = STANDARD.encode(LINKS.as_bytes());
        let imported = parse_subscription(&encoded);
        assert_eq!(imported.len(), 3);
        assert_eq!(imported[1].profile.server, "b.example.com");
    }

    #[test]
    fn wraps_long_base64_lines() {
        let encoded = STANDARD.encode(LINKS.as_bytes());
        let wrapped = encoded
            .as_bytes()
            .chunks(48)
            .map(String::from_utf8_lossy)
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(parse_subscription(&wrapped).len(), 3);
    }

    #[test]
    fn skips_garbage_and_comments() {
        let body = "# comment\nnot a link\nvless://uuid-a@a.example.com:443#A\n";
        let imported = parse_subscription(body);
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].profile.name, "A");
    }

    #[test]
    fn detects_panel_error_page() {
        let link = "vless://uuid-a@a.example.com:443?security=reality";
        let body = format!(
            "{link}#\u{1f7e2} Access\n{link}#\u{1f4f1} Only 3 devices\n{link}#\u{1f6ab} Limit exceeded\n"
        );
        let imported = parse_subscription(&body);
        assert_eq!(imported.len(), 3);
        let error = panel_error(&imported).expect("panel error");
        assert!(error.contains("Limit exceeded"));
    }

    #[test]
    fn accepts_subscriptions_with_unique_servers() {
        let imported = parse_subscription(LINKS);
        assert!(panel_error(&imported).is_none());
    }

    #[test]
    fn parses_config_subscription() {
        let body = r#"[{"remarks":"EU Auto","outbounds":[{"protocol":"vless","settings":{"vnext":[{"address":"1.2.3.4","port":443,"users":[{"id":"uuid-1"}]}]},"streamSettings":{"network":"ws","security":"tls","tlsSettings":{"serverName":"cdn.test"}}}]}]"#;
        let configs = parse_configs(body).expect("configs");
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].profile.name, "EU Auto");
        assert_eq!(configs[0].profile.server, "1.2.3.4");
        assert_eq!(configs[0].profile.network, Network::Ws);
        assert!(configs[0].config.is_some());
    }

    #[test]
    fn config_parser_ignores_link_lists() {
        assert!(parse_configs(LINKS).is_none());
    }
}
