//! Transport and security settings.

use serde_json::{json, Map, Value};

use crate::model::{Network, Profile, Security};

pub(super) fn settings(profile: &Profile) -> Value {
    let mut stream = Map::new();
    stream.insert("network".into(), json!(profile.network.as_str()));

    let path = profile.path.as_deref().filter(|p| !p.is_empty()).unwrap_or("/");
    let host = profile
        .host
        .as_deref()
        .unwrap_or_else(|| profile.server_name());

    match profile.network {
        // `headerType=http` is old but still handed out by some panels: the
        // stream is framed as an HTTP/1.1 request, which hides it from a
        // protocol sniffer that only trusts TLS.
        Network::Tcp => {
            if profile.header_type.as_deref() == Some("http") {
                stream.insert(
                    "tcpSettings".into(),
                    json!({
                        "header": {
                            "type": "http",
                            "request": {
                                "path": [path],
                                "headers": { "Host": [host] }
                            }
                        }
                    }),
                );
            }
        }
        Network::Ws => {
            let mut ws = Map::new();
            ws.insert("path".into(), json!(path));
            ws.insert("headers".into(), json!({ "Host": host }));
            // Early data lets the first frames leave with the handshake, so the
            // connection looks like one short burst instead of a quiet open.
            if let Some(size) = profile.early_data {
                ws.insert("maxEarlyData".into(), json!(size));
                ws.insert(
                    "earlyDataHeaderName".into(),
                    json!(profile
                        .early_data_header
                        .as_deref()
                        .unwrap_or("Sec-WebSocket-Protocol")),
                );
            }
            stream.insert("wsSettings".into(), Value::Object(ws));
        }
        Network::Grpc => {
            let mut grpc = Map::new();
            grpc.insert(
                "serviceName".into(),
                json!(profile
                    .service_name
                    .as_deref()
                    .or(profile.path.as_deref())
                    .unwrap_or("")),
            );
            if profile.mode.as_deref() == Some("multi") {
                grpc.insert("multiMode".into(), json!(true));
            }
            stream.insert("grpcSettings".into(), Value::Object(grpc));
        }
        Network::Http => {
            stream.insert(
                "httpSettings".into(),
                json!({ "path": path, "host": [host] }),
            );
        }
        Network::Xhttp => {
            let mut xhttp = Map::new();
            xhttp.insert("path".into(), json!(path));
            xhttp.insert("host".into(), json!(host));
            // Only the modes the bundled core understands; anything else falls
            // back to the auto-negotiating default instead of failing to start.
            let mode = profile
                .mode
                .as_deref()
                .filter(|mode| matches!(*mode, "packet-up" | "stream-up" | "stream-one"))
                .unwrap_or("auto");
            xhttp.insert("mode".into(), json!(mode));
            if let Some(extra) = profile.extra.clone() {
                xhttp.insert("extra".into(), extra);
            }
            stream.insert("xhttpSettings".into(), Value::Object(xhttp));
        }
        // HTTPUpgrade is a bare HTTP/1.1 upgrade: to a middlebox it is ordinary
        // web traffic, and it survives CDNs that drop WebSocket.
        Network::HttpUpgrade => {
            stream.insert(
                "httpupgradeSettings".into(),
                json!({ "path": path, "host": host }),
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
