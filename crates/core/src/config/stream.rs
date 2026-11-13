//! Transport and security settings.

use serde_json::{json, Map, Value};

use crate::model::{Network, Profile, Security};

pub(super) fn settings(profile: &Profile) -> Value {
    let mut stream = Map::new();
    stream.insert("network".into(), json!(profile.network.as_str()));

    match profile.network {
        Network::Tcp => {}
        Network::Ws => {
            let mut headers = Map::new();
            headers.insert(
                "Host".into(),
                json!(profile
                    .host
                    .as_deref()
                    .unwrap_or_else(|| profile.server_name())),
            );
            stream.insert(
                "wsSettings".into(),
                json!({
                    "path": profile.path.as_deref().filter(|p| !p.is_empty()).unwrap_or("/"),
                    "headers": Value::Object(headers)
                }),
            );
        }
        Network::Grpc => {
            stream.insert(
                "grpcSettings".into(),
                json!({
                    "serviceName": profile
                        .service_name
                        .as_deref()
                        .or(profile.path.as_deref())
                        .unwrap_or("")
                }),
            );
        }
        Network::Http => {
            stream.insert(
                "httpSettings".into(),
                json!({
                    "path": profile.path.as_deref().filter(|p| !p.is_empty()).unwrap_or("/"),
                    "host": [profile.host.as_deref().unwrap_or(profile.server_name())]
                }),
            );
        }
        Network::Xhttp => {
            stream.insert(
                "xhttpSettings".into(),
                json!({
                    "path": profile.path.as_deref().filter(|p| !p.is_empty()).unwrap_or("/"),
                    "host": profile.host.as_deref().unwrap_or(profile.server_name()),
                    "mode": "auto"
                }),
            );
        }
    }

    match profile.security {
        Security::None => {}
        Security::Tls => {
            let mut tls = Map::new();
            tls.insert("serverName".into(), json!(profile.server_name()));
            if !profile.alpn.is_empty() {
                tls.insert("alpn".into(), json!(profile.alpn));
            }
            // An empty fingerprint leaves Go's own TLS client hello, which
            // stands out next to a browser. Chrome is the safest default.
            let fp = profile
                .fingerprint
                .as_deref()
                .filter(|f| !f.is_empty())
                .unwrap_or("chrome");
            tls.insert("fingerprint".into(), json!(fp));
            stream.insert("security".into(), json!("tls"));
            stream.insert("tlsSettings".into(), Value::Object(tls));
        }
        Security::Reality => {
            let mut reality = Map::new();
            reality.insert("serverName".into(), json!(profile.server_name()));
            reality.insert(
                "fingerprint".into(),
                json!(profile.fingerprint.as_deref().unwrap_or("chrome")),
            );
            reality.insert(
                "publicKey".into(),
                json!(profile.public_key.as_deref().unwrap_or_default()),
            );
            reality.insert(
                "shortId".into(),
                json!(profile.short_id.as_deref().unwrap_or_default()),
            );
            reality.insert("spiderX".into(), json!("/"));
            stream.insert("security".into(), json!("reality"));
            stream.insert("realitySettings".into(), Value::Object(reality));
        }
    }

    Value::Object(stream)
}
