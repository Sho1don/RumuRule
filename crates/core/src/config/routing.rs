//! Routing rules: keep private networks and BitTorrent off the tunnel.

use serde_json::{json, Value};

/// Address ranges that must never leave through the proxy.
const PRIVATE_RANGES: [&str; 8] = [
    "127.0.0.0/8",
    "10.0.0.0/8",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "169.254.0.0/16",
    "fc00::/7",
    "fe80::/10",
    "::1/128",
];

/// Private and loopback ranges that keep using the physical link.
pub(super) fn private_ranges() -> Value {
    json!(PRIVATE_RANGES)
}

pub(super) fn rules() -> Value {
    json!([
        {
            "type": "field",
            "ip": PRIVATE_RANGES,
            "outboundTag": "direct"
        },
        { "type": "field", "protocol": ["bittorrent"], "outboundTag": "direct" }
    ])
}
