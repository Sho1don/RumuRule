//! Per-protocol outbound settings.

use serde_json::{json, Map, Value};

use crate::model::Profile;

pub(super) fn settings(profile: &Profile) -> Value {
    match profile.protocol.as_str() {
        "vless" => {
            let mut user = Map::new();
            user.insert("id".into(), json!(profile.id));
            user.insert(
                "encryption".into(),
                json!(profile.encryption.as_deref().unwrap_or("none")),
            );
            if let Some(flow) = profile.flow.as_deref().filter(|f| !f.is_empty()) {
                user.insert("flow".into(), json!(flow));
            }
            json!({
                "vnext": [{
                    "address": profile.server,
                    "port": profile.port,
                    "users": [Value::Object(user)]
                }]
            })
        }
        "vmess" => json!({
            "vnext": [{
                "address": profile.server,
                "port": profile.port,
                "users": [{
                    "id": profile.id,
                    "alterId": 0,
                    "security": profile.encryption.as_deref().unwrap_or("auto")
                }]
            }]
        }),
        "trojan" => json!({
            "servers": [{
                "address": profile.server,
                "port": profile.port,
                "password": profile.id
            }]
        }),
        "shadowsocks" => json!({
            "servers": [{
                "address": profile.server,
                "port": profile.port,
                "method": profile.method.as_deref().unwrap_or("2022-blake3-aes-128-gcm"),
                "password": profile.id
            }]
        }),
        _ => json!({}),
    }
}

/// A complete Hysteria 2 outbound. Xray 26 speaks the protocol natively, but
/// its shape differs from the other proxies: auth and QUIC tuning live under
/// `streamSettings`, and the endpoint is a plain `address`/`port` pair.
pub(super) fn hysteria(profile: &Profile) -> Value {
    let mut tls = Map::new();
    tls.insert("serverName".into(), json!(profile.server_name()));
    let alpn = if profile.alpn.is_empty() {
        vec!["h3".to_string()]
    } else {
        profile.alpn.clone()
    };
    tls.insert("alpn".into(), json!(alpn));
    tls.insert(
        "fingerprint".into(),
        json!(profile.fingerprint.as_deref().unwrap_or("chrome")),
    );
    // Xray 26 removed `allowInsecure`: a self-signed Hysteria server is
    // accepted only through a certificate pin, which Hysteria links carry as
    // `pinSHA256`. `insecure=1` without a pin cannot be expressed any more.
    if let Some(pin) = profile.pin_sha256.as_deref().filter(|pin| !pin.is_empty()) {
        tls.insert("pinnedPeerCertSha256".into(), json!(pin));
    }

    let mut stream = Map::new();
    stream.insert("network".into(), json!("hysteria"));
    stream.insert("security".into(), json!("tls"));
    stream.insert("tlsSettings".into(), Value::Object(tls));
    stream.insert(
        "hysteriaSettings".into(),
        json!({ "auth": profile.id, "version": 2 }),
    );

    // Salamander obfuscation is a packet mask around QUIC, not a transport.
    if let (Some(kind), Some(password)) = (
        profile.obfs.as_deref().filter(|obfs| !obfs.is_empty()),
        profile
            .obfs_password
            .as_deref()
            .filter(|password| !password.is_empty()),
    ) {
        stream.insert(
            "finalmask".into(),
            json!({
                "quicParams": {
                    "congestion": "bbr",
                    "keepAlivePeriod": 60,
                    "maxIdleTimeout": 120
                },
                "udp": [{ "type": kind, "settings": { "password": password } }]
            }),
        );
    }

    json!({
        "tag": "proxy",
        "protocol": "hysteria",
        "settings": {
            "address": profile.server,
            "port": profile.port,
            "version": 2
        },
        "streamSettings": Value::Object(stream)
    })
}
