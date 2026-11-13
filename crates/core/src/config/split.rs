//! Split tunneling: rules that keep chosen destinations out of the tunnel, or
//! inside it.
//!
//! The list is plain text, one entry per line, and every entry is either a
//! domain, an IP address or a CIDR subnet. Xray treats `domain` and `ip` as
//! separate matchers that are ANDed inside one rule, so the two groups always
//! become two rules.

use std::net::IpAddr;

use serde_json::{json, Value};

use crate::settings::{SplitMode, SplitTunnel};

/// Prepend the split rules to an already built configuration.
pub fn apply(config: &mut Value, split: &SplitTunnel) {
    if split.mode == SplitMode::Off {
        return;
    }

    let mut domains = Vec::new();
    let mut addresses = Vec::new();
    for entry in &split.entries {
        match classify(entry) {
            Some(Target::Domain(domain)) => domains.push(Value::String(domain)),
            Some(Target::Address(address)) => addresses.push(Value::String(address)),
            None => {}
        }
    }
    if domains.is_empty() && addresses.is_empty() && split.mode == SplitMode::Bypass {
        return;
    }

    let tag = match split.mode {
        SplitMode::Bypass => "direct".to_string(),
        _ => proxy_tag(config),
    };
    let mut rules = Vec::new();
    if !domains.is_empty() {
        rules.push(json!({
            "type": "field",
            "domain": domains,
            "outboundTag": tag
        }));
    }
    if !addresses.is_empty() {
        rules.push(json!({
            "type": "field",
            "ip": addresses,
            "outboundTag": tag
        }));
    }
    let existing = config["routing"]["rules"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    rules.extend(existing);
    // "Only" needs the inverse: whatever the list did not match leaves through
    // the physical link. It goes after every other rule, otherwise a
    // subscription rule could still pull traffic into the tunnel.
    if split.mode == SplitMode::Only {
        rules.push(json!({
            "type": "field",
            "network": "tcp,udp",
            "outboundTag": "direct"
        }));
    }
    if !config["routing"].is_object() {
        config["routing"] = json!({ "domainStrategy": "IPIfNonMatch" });
    }
    config["routing"]["rules"] = Value::Array(rules);
}

/// The outbound that carries tunneled traffic: `proxy` in our own configs, or
/// the first real outbound of a ready-made subscription config.
fn proxy_tag(config: &Value) -> String {
    let Some(outbounds) = config["outbounds"].as_array() else {
        return "proxy".into();
    };
    if outbounds.iter().any(|outbound| outbound["tag"] == "proxy") {
        return "proxy".into();
    }
    outbounds
        .iter()
        .find(|outbound| {
            !matches!(
                outbound["protocol"].as_str().unwrap_or(""),
                "freedom" | "blackhole" | "dns"
            )
        })
        .and_then(|outbound| outbound["tag"].as_str())
        .unwrap_or("proxy")
        .to_string()
}

enum Target {
    Domain(String),
    Address(String),
}

/// One list entry to a matcher. Anything that is not a domain, an address or a
/// subnet is dropped instead of failing the whole configuration.
fn classify(raw: &str) -> Option<Target> {
    let entry = raw.trim();
    if entry.is_empty() {
        return None;
    }
    let lower = entry.to_ascii_lowercase();
    // `geosite:` and `geoip:` need the data files next to the core, and the
    // package does not ship them: the config would fail to start. They are
    // dropped here instead of breaking the whole connection.
    if lower.starts_with("geosite:") || lower.starts_with("geoip:") {
        return None;
    }
    if lower.starts_with("domain:") || lower.starts_with("regexp:") || lower.starts_with("full:") {
        return Some(Target::Domain(entry.to_string()));
    }
    if is_address(entry) {
        return Some(Target::Address(entry.to_string()));
    }
    // A dotted run of digits is a mistyped address, not a host name.
    let digits_only = entry
        .chars()
        .all(|c| c.is_ascii_digit() || c == '.');
    if digits_only {
        return None;
    }
    if entry
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '*'))
    {
        return Some(Target::Domain(entry.to_string()));
    }
    None
}

/// An IP address or a CIDR subnet, with the prefix length matching the family.
fn is_address(entry: &str) -> bool {
    let (address, prefix) = match entry.split_once('/') {
        Some((address, prefix)) => (address, Some(prefix)),
        None => (entry, None),
    };
    let Ok(address) = address.parse::<IpAddr>() else {
        return false;
    };
    match prefix {
        None => true,
        Some(prefix) => {
            let Ok(prefix) = prefix.parse::<u8>() else {
                return false;
            };
            match address {
                IpAddr::V4(_) => prefix <= 32,
                IpAddr::V6(_) => prefix <= 128,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn config() -> Value {
        json!({
            "outbounds": [
                { "tag": "proxy", "protocol": "vless" },
                { "tag": "direct", "protocol": "freedom" }
            ],
            "routing": { "domainStrategy": "IPIfNonMatch", "rules": [{ "type": "field", "port": 53 }] }
        })
    }

    fn split(mode: SplitMode, entries: &[&str]) -> SplitTunnel {
        SplitTunnel {
            mode,
            entries: entries.iter().map(|entry| entry.to_string()).collect(),
        }
    }

    #[test]
    fn off_adds_nothing() {
        let mut value = config();
        let before = value["routing"]["rules"].clone();
        apply(&mut value, &split(SplitMode::Off, &["example.com"]));
        assert_eq!(value["routing"]["rules"], before);
    }

    #[test]
    fn bypass_sends_listed_destinations_direct() {
        let mut value = config();
        apply(
            &mut value,
            &split(SplitMode::Bypass, &["bank.example", "10.0.0.5", "192.168.1.0/24"]),
        );
        let rules = value["routing"]["rules"].as_array().unwrap();
        assert_eq!(rules[0]["domain"][0], "bank.example");
        assert_eq!(rules[0]["outboundTag"], "direct");
        assert_eq!(rules[1]["ip"][0], "10.0.0.5");
        assert_eq!(rules[1]["ip"][1], "192.168.1.0/24");
        // The original rules survive behind the new ones.
        assert_eq!(rules.len(), 3);
        assert_eq!(rules[2]["port"], 53);
    }

    #[test]
    fn only_sends_unlisted_traffic_around_the_tunnel() {
        let mut value = config();
        apply(&mut value, &split(SplitMode::Only, &["work.example"]));
        let rules = value["routing"]["rules"].as_array().unwrap();
        assert_eq!(rules[0]["outboundTag"], "proxy");
        let last = rules.last().unwrap();
        assert_eq!(last["outboundTag"], "direct");
        assert_eq!(last["network"], "tcp,udp");
    }

    #[test]
    fn native_matcher_prefixes_survive() {
        let mut value = config();
        apply(
            &mut value,
            &split(SplitMode::Bypass, &["full:exact.example", "regexp:^ads\\."]),
        );
        let rules = value["routing"]["rules"].as_array().unwrap();
        assert_eq!(rules[0]["domain"][0], "full:exact.example");
        assert_eq!(rules[0]["domain"][1], "regexp:^ads\\.");
    }

    #[test]
    fn geo_lists_are_dropped_not_passed_through() {
        // The bundled core has no geosite.dat or geoip.dat: passing these on
        // would fail every connection.
        assert!(classify("geosite:private").is_none());
        assert!(classify("geoip:ru").is_none());
    }

    #[test]
    fn broken_entries_are_dropped() {
        assert!(classify("   ").is_none());
        assert!(classify("has space").is_none());
        assert!(classify("999.1.1.1").is_none());
        assert!(classify("10.0.0.0/99").is_none());
        assert!(classify("host/path").is_none());
    }

    #[test]
    fn proxy_tag_follows_a_ready_made_config() {
        let mut value = json!({
            "outbounds": [
                { "tag": "direct", "protocol": "freedom" },
                { "tag": "node", "protocol": "trojan" }
            ],
            "routing": { "rules": [] }
        });
        apply(&mut value, &split(SplitMode::Only, &["example.com"]));
        assert_eq!(value["routing"]["rules"][0]["outboundTag"], "node");
    }
}
