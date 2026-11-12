//! Xray configuration generation.

mod outbound;
mod routing;
mod stream;

use std::net::{IpAddr, ToSocketAddrs};

use serde_json::{json, Value};

use crate::model::Profile;

/// Local SOCKS5 port used by tun2socks and by the system proxy mode.
pub const SOCKS_PORT: u16 = 10808;
/// Local HTTP proxy port for applications that only speak HTTP proxies.
pub const HTTP_PORT: u16 = 10809;
/// TUN interface created by the privileged helper path.
pub const TUN_INTERFACE: &str = "rumurule0";
/// Address applied to the interface; the system DNS endpoint is the next one.
pub const TUN_GATEWAY: &str = "198.18.0.1/15";
/// Upstream resolvers used by the tunnel's own DNS module.
pub const TUN_DNS: [&str; 2] = ["1.1.1.1", "1.0.0.1"];

/// Inbound tag used by the routing rules.
const PROXY_TAG: &str = "proxy";

/// Build a complete Xray configuration for a single profile.
pub fn build_config(profile: &Profile) -> Value {
    let proxy = if profile.protocol == "hysteria" {
        outbound::hysteria(profile)
    } else {
        json!({
            "tag": PROXY_TAG,
            "protocol": profile.protocol,
            "settings": outbound::settings(profile),
            "streamSettings": stream::settings(profile)
        })
    };

    json!({
        "log": { "loglevel": "warning", "access": "none" },
        "inbounds": [
            {
                "tag": "socks",
                "listen": "127.0.0.1",
                "port": SOCKS_PORT,
                "protocol": "socks",
                "settings": { "auth": "noauth", "udp": true }
            },
            {
                "tag": "http",
                "listen": "127.0.0.1",
                "port": HTTP_PORT,
                "protocol": "http",
                "settings": {}
            }
        ],
        "outbounds": [
            proxy,
            { "tag": "direct", "protocol": "freedom" },
            { "tag": "block", "protocol": "blackhole" }
        ],
        "routing": {
            "domainStrategy": "IPIfNonMatch",
            "rules": routing::rules()
        }
    })
}

/// Build a system-wide TUN configuration on top of a working proxy config.
///
/// The base may come from a subscription (it already has outbounds, DNS and
/// routing) or from [`build_config`]. FakeDNS and the local resolver are
/// dropped: through a TUN every destination is an address, so DNS has to return
/// real addresses, and a local resolver would point back into the tunnel.
///
/// Xray creates the interface, applies the default routes and keeps its own
/// upstream sockets bound to the physical interface, so traffic to the server
/// never re-enters the tunnel. `ipv6` follows the host's default route.
pub fn tun_config(base: &Value, ipv6: bool) -> Value {
    let mut config = base.clone();
    pin_server_addresses(&mut config);
    let Some(object) = config.as_object_mut() else {
        return base.clone();
    };

    object.insert("log".into(), json!({ "loglevel": "warning", "access": "none" }));
    // FakeDNS answers with a fake address, which a TUN cannot route anywhere.
    object.remove("fakedns");

    let mut outbounds = object
        .get("outbounds")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let direct_tag = outbounds
        .iter()
        .find(|outbound| outbound["protocol"] == "freedom")
        .and_then(|outbound| outbound["tag"].as_str())
        .unwrap_or("direct")
        .to_string();
    if !outbounds
        .iter()
        .any(|outbound| outbound["tag"].as_str() == Some(direct_tag.as_str()))
    {
        outbounds.push(json!({ "tag": direct_tag, "protocol": "freedom" }));
    }

    let dns_tag = if outbounds.iter().any(|outbound| outbound["tag"] == "dns") {
        "dns-tun"
    } else {
        "dns"
    };
    outbounds.push(json!({ "tag": dns_tag, "protocol": "dns" }));
    object.insert("outbounds".into(), Value::Array(outbounds));

    object.insert("dns".into(), tunnel_dns(base, ipv6));

    let mut rules = vec![
        json!({
            "type": "field",
            "inboundTag": ["tun"],
            "ip": routing::private_ranges(),
            "outboundTag": direct_tag
        }),
        json!({
            "type": "field",
            "inboundTag": ["tun"],
            "port": 53,
            "outboundTag": dns_tag
        }),
    ];
    if let Some(existing) = base["routing"]["rules"].as_array() {
        rules.extend(existing.iter().cloned());
    }
    let strategy = base["routing"]["domainStrategy"]
        .as_str()
        .unwrap_or("IPIfNonMatch");
    // Keep the rest of the subscription routing intact: balancers, observatory
    // references and matcher options are just as load-bearing as the rules.
    // Rules that point at a balancer fail the whole start if it goes missing.
    let mut routing = base["routing"].clone();
    if !routing.is_object() {
        routing = json!({});
    }
    if let Some(map) = routing.as_object_mut() {
        map.insert("domainStrategy".into(), strategy.into());
        map.insert("rules".into(), Value::Array(rules));
    }
    object.insert("routing".into(), routing);

    let mut routes = vec!["0.0.0.0/0"];
    if ipv6 {
        routes.push("::/0");
    }
    // Local endpoints stay available next to the TUN, so apps can pick either
    // path and the system proxy keeps working.
    object.insert(
        "inbounds".into(),
        json!([
        {
            "tag": "tun",
            "protocol": "tun",
            "sniffing": {
                "enabled": true,
                "destOverride": ["http", "tls", "quic"],
                // Sniffed names only feed routing; the address is kept.
                "routeOnly": true
            },
            "settings": {
                "name": TUN_INTERFACE,
                // Smaller than the usual 1500: a tunneled packet must still fit
                // the path MTU, or large TLS handshakes stall.
                "mtu": 1400,
                "gateway": [TUN_GATEWAY],
                "autoSystemRoutingTable": routes,
                "autoOutboundsInterface": "auto",
                "autoSystemDnsToGateway": true
            }
        },
        {
            "tag": "socks",
            "listen": "127.0.0.1",
            "port": SOCKS_PORT,
            "protocol": "socks",
            "settings": { "auth": "noauth", "udp": true }
        },
        {
            "tag": "http",
            "listen": "127.0.0.1",
            "port": HTTP_PORT,
            "protocol": "http",
            "settings": {}
        }
        ]),
    );

    config
}

/// DNS section for the tunnel: real resolvers only, no localhost.
fn tunnel_dns(base: &Value, ipv6: bool) -> Value {
    let mut servers: Vec<Value> = Vec::new();
    if let Some(list) = base["dns"]["servers"].as_array() {
        for server in list {
            let address = match server {
                Value::String(address) => address.clone(),
                Value::Object(object) => object
                    .get("address")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                _ => continue,
            };
            let address = address.to_ascii_lowercase();
            if address.is_empty()
                || address.starts_with("fakedns")
                || address.starts_with("localhost")
                || address.starts_with("127.")
                || address == "::1"
            {
                continue;
            }
            servers.push(server.clone());
        }
    }
    if servers.is_empty() {
        servers = TUN_DNS.iter().map(|server| json!(server)).collect();
        if ipv6 {
            servers.push(json!("2606:4700:4700::1111"));
        }
    }

    let mut dns = base["dns"].clone();
    dns["servers"] = Value::Array(servers);
    // `UseIPv4` would answer every AAAA query with an empty result.
    dns["queryStrategy"] = json!(if ipv6 { "UseIP" } else { "UseIPv4" });
    dns
}

/// Convenience wrapper for link-imported profiles without a stored config.
/// Resolves the domains of outbound servers before the tunnel starts.
///
/// Inside the tunnel the system resolver answers from the tunnel itself, so an
/// outbound that still holds a domain can never come up: resolving the server
/// address would need the tunnel the server provides. The domain is kept as the
/// TLS/REALITY server name, which is what the server checks.
fn pin_server_addresses(config: &mut Value) {
    let Some(outbounds) = config["outbounds"].as_array_mut() else {
        return;
    };
    for outbound in outbounds {
        // Hysteria keeps its endpoint in plain settings, not in vnext/servers.
        if outbound["protocol"] == "hysteria" {
            let domain = outbound["settings"]["address"]
                .as_str()
                .unwrap_or("")
                .to_string();
            if !domain.is_empty() && domain.parse::<IpAddr>().is_err() {
                if let Some(ip) = resolve_first(&domain) {
                    outbound["settings"]["address"] = Value::String(ip.to_string());
                    keep_server_name(outbound, &domain);
                }
            }
            continue;
        }
        let mut names = Vec::new();
        for key in ["vnext", "servers"] {
            let Some(servers) = outbound["settings"][key].as_array_mut() else {
                continue;
            };
            for server in servers {
                let Some(address) = server["address"].as_str() else {
                    continue;
                };
                if address.parse::<IpAddr>().is_ok() {
                    continue;
                }
                let Some(ip) = resolve_first(address) else {
                    continue;
                };
                names.push(address.to_string());
                server["address"] = Value::String(ip.to_string());
            }
        }
        for name in names {
            keep_server_name(outbound, &name);
        }
    }
}

fn resolve_first(domain: &str) -> Option<IpAddr> {
    let addrs: Vec<IpAddr> = (domain, 0)
        .to_socket_addrs()
        .ok()?
        .map(|addr| addr.ip())
        .collect();
    addrs
        .iter()
        .copied()
        .find(IpAddr::is_ipv4)
        .or_else(|| addrs.first().copied())
}

/// TLS and REALITY still need the domain, otherwise the server rejects us.
fn keep_server_name(outbound: &mut Value, domain: &str) {
    let Some(stream) = outbound.get_mut("streamSettings") else {
        return;
    };
    for key in ["tlsSettings", "realitySettings"] {
        let Some(settings) = stream.get_mut(key) else {
            continue;
        };
        let empty = settings
            .get("serverName")
            .and_then(Value::as_str)
            .map_or(true, str::is_empty);
        if empty {
            settings["serverName"] = Value::String(domain.to_string());
        }
    }
}

pub fn build_tun_config(profile: &Profile, ipv6: bool) -> Value {
    tun_config(&build_config(profile), ipv6)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_link;

    fn profile() -> crate::Profile {
        parse_link("vless://uuid-1@example.com:443?security=tls").unwrap()
    }

    #[test]
    fn tun_config_routes_everything_and_answers_dns() {
        let config = build_tun_config(&profile(), true);
        let inbound = &config["inbounds"][0];
        assert_eq!(inbound["protocol"], "tun");
        assert_eq!(inbound["settings"]["name"], TUN_INTERFACE);
        assert_eq!(
            inbound["settings"]["autoSystemRoutingTable"],
            serde_json::json!(["0.0.0.0/0", "::/0"])
        );
        assert_eq!(inbound["settings"]["autoOutboundsInterface"], "auto");
        assert_eq!(inbound["settings"]["autoSystemDnsToGateway"], true);
        assert_eq!(inline_dns_rule(&config), "dns");
        assert!(config["outbounds"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["protocol"] == "dns"));
    }

    #[test]
    fn tun_config_without_ipv6_omits_v6() {
        let config = build_tun_config(&profile(), false);
        let routes = &config["inbounds"][0]["settings"]["autoSystemRoutingTable"];
        assert_eq!(routes.as_array().unwrap().len(), 1);
        assert_eq!(routes[0].as_str(), Some("0.0.0.0/0"));
        assert_eq!(config["dns"]["servers"].as_array().unwrap().len(), 2);
    }

    fn inline_dns_rule(config: &Value) -> &str {
        config["routing"]["rules"]
            .as_array()
            .unwrap()
            .iter()
            .find(|rule| rule["port"] == 53)
            .and_then(|rule| rule["outboundTag"].as_str())
            .unwrap_or("")
    }

    #[test]
    fn tun_config_keeps_subscription_routing_but_drops_fakedns() {
        let base = serde_json::json!({
            "dns": {
                "servers": ["fakedns", "94.140.14.14", "localhost"],
                "hosts": { "example.com": ["1.2.3.4"] }
            },
            "fakedns": [{ "ipPool": "198.18.0.0/15" }],
            "inbounds": [{ "tag": "socks", "protocol": "socks" }],
            "outbounds": [
                { "tag": "proxy", "protocol": "vless" },
                { "tag": "DIRECT", "protocol": "freedom" }
            ],
            "routing": {
                "domainStrategy": "AsIs",
                "rules": [{ "type": "field", "domain": ["domain:ru"], "outboundTag": "DIRECT" }]
            }
        });
        let config = tun_config(&base, false);

        assert!(config.get("fakedns").is_none());
        assert_eq!(
            config["dns"]["servers"],
            serde_json::json!(["94.140.14.14"])
        );
        assert_eq!(config["dns"]["hosts"]["example.com"][0], "1.2.3.4");
        assert_eq!(config["dns"]["queryStrategy"], "UseIPv4");
        let inbounds = config["inbounds"].as_array().unwrap();
        assert_eq!(inbounds.len(), 3);
        assert_eq!(inbounds[0]["protocol"], "tun");
        assert!(inbounds.iter().any(|inbound| inbound["protocol"] == "socks"));
        assert!(inbounds.iter().any(|inbound| inbound["protocol"] == "http"));
        assert_eq!(config["routing"]["domainStrategy"], "AsIs");
        assert_eq!(config["routing"]["rules"].as_array().unwrap().len(), 3);
        assert_eq!(config["routing"]["rules"][0]["outboundTag"], "DIRECT");
        assert_eq!(inline_dns_rule(&config), "dns");
        assert!(config["outbounds"]
            .as_array()
            .unwrap()
            .iter()
            .any(|outbound| outbound["protocol"] == "dns"));
    }

    #[test]
    fn tun_config_keeps_balancers_referenced_by_rules() {
        // Subscriptions ship balancer-based routing; dropping the balancers
        // makes Xray refuse to start the whole configuration.
        let base = serde_json::json!({
            "outbounds": [
                { "tag": "proxy", "protocol": "vless" },
                { "tag": "DIRECT", "protocol": "freedom" }
            ],
            "routing": {
                "domainStrategy": "IPIfNonMatch",
                "balancers": [{ "tag": "balancer-whitelists", "selector": ["proxy"] }],
                "rules": [{
                    "type": "field",
                    "domain": ["domain:example.com"],
                    "balancerTag": "balancer-whitelists"
                }]
            }
        });
        let config = tun_config(&base, false);

        assert_eq!(
            config["routing"]["balancers"][0]["tag"],
            "balancer-whitelists"
        );
        let rules = config["routing"]["rules"].as_array().unwrap();
        assert!(rules
            .iter()
            .any(|rule| rule["balancerTag"] == "balancer-whitelists"));
    }

    #[test]
    fn vless_reality_config_has_reality_settings() {
        let profile = parse_link(
            "vless://uuid-1@example.com:443?security=reality&pbk=PUB&sid=abcd&sni=cdn.example.com&fp=chrome&flow=xtls-rprx-vision",
        )
        .unwrap();
        let config = build_config(&profile);
        let outbound = &config["outbounds"][0];
        assert_eq!(outbound["protocol"], "vless");
        assert_eq!(outbound["streamSettings"]["security"], "reality");
        assert_eq!(
            outbound["streamSettings"]["realitySettings"]["publicKey"],
            "PUB"
        );
        assert_eq!(
            outbound["settings"]["vnext"][0]["users"][0]["flow"],
            "xtls-rprx-vision"
        );
    }

    #[test]
    fn ws_config_sets_host_header() {
        let profile = parse_link(
            "vless://uuid-1@1.2.3.4:443?type=ws&path=%2Fws&host=cdn.test&security=tls&sni=cdn.test",
        )
        .unwrap();
        let config = build_config(&profile);
        let outbound = &config["outbounds"][0];
        assert_eq!(outbound["streamSettings"]["network"], "ws");
        assert_eq!(
            outbound["streamSettings"]["wsSettings"]["headers"]["Host"],
            "cdn.test"
        );
        assert_eq!(
            outbound["streamSettings"]["tlsSettings"]["serverName"],
            "cdn.test"
        );
    }

    #[test]
    fn inbounds_listen_on_loopback() {
        let profile = parse_link("trojan://pass@example.org:443").unwrap();
        let config = build_config(&profile);
        for inbound in config["inbounds"].as_array().unwrap() {
            assert_eq!(inbound["listen"], "127.0.0.1");
        }
    }

    #[test]
    fn hysteria2_link_becomes_a_quic_outbound() {
        let profile = parse_link(
            "hysteria2://secret@example.org:443/?sni=cdn.test&obfs=salamander&obfs-password=pass",
        )
        .unwrap();
        let config = build_config(&profile);
        let outbound = &config["outbounds"][0];
        assert_eq!(outbound["protocol"], "hysteria");
        assert_eq!(outbound["settings"]["address"], "example.org");
        assert_eq!(
            outbound["streamSettings"]["hysteriaSettings"]["auth"],
            "secret"
        );
        assert_eq!(
            outbound["streamSettings"]["tlsSettings"]["serverName"],
            "cdn.test"
        );
        assert_eq!(
            outbound["streamSettings"]["finalmask"]["udp"][0]["type"],
            "salamander"
        );
    }

    #[test]
    fn hysteria_self_signed_uses_a_certificate_pin() {
        // Xray 26 dropped allowInsecure; Hysteria's pinSHA256 is the way in.
        let profile = parse_link(
            "hysteria2://secret@example.org:443/?sni=example.org&insecure=1&pinSHA256=ABC123",
        )
        .unwrap();
        let config = build_config(&profile);
        let tls = &config["outbounds"][0]["streamSettings"]["tlsSettings"];
        assert_eq!(tls["pinnedPeerCertSha256"], "ABC123");
        assert!(tls.get("allowInsecure").is_none());

        let unpinned = parse_link("hysteria2://secret@example.org:443/?insecure=1").unwrap();
        let config = build_config(&unpinned);
        assert!(config["outbounds"][0]["streamSettings"]["tlsSettings"]
            .get("allowInsecure")
            .is_none());
    }

    #[test]
    fn tun_pins_hysteria_domains() {
        let profile = parse_link("hysteria2://secret@localhost:443/?sni=localhost").unwrap();
        let config = build_tun_config(&profile, false);
        let outbound = config["outbounds"]
            .as_array()
            .unwrap()
            .iter()
            .find(|outbound| outbound["protocol"] == "hysteria")
            .expect("hysteria outbound");
        assert_eq!(outbound["settings"]["address"], "127.0.0.1");
        assert_eq!(
            outbound["streamSettings"]["tlsSettings"]["serverName"],
            "localhost"
        );
    }

    #[test]
    fn tun_pins_outbound_domains() {
        // A domain outbound cannot resolve itself once the tunnel owns DNS.
        let mut config = serde_json::json!({
            "outbounds": [{
                "tag": "proxy",
                "protocol": "vless",
                "settings": { "vnext": [{ "address": "localhost", "port": 443 }] },
                "streamSettings": {
                    "security": "tls",
                    "tlsSettings": { "serverName": "" }
                }
            }]
        });
        pin_server_addresses(&mut config);
        let outbound = &config["outbounds"][0];
        assert_eq!(outbound["settings"]["vnext"][0]["address"], "127.0.0.1");
        assert_eq!(outbound["streamSettings"]["tlsSettings"]["serverName"], "localhost");
    }

    #[test]
    fn tun_keeps_outbound_ips() {
        let mut config = serde_json::json!({
            "outbounds": [{
                "tag": "proxy",
                "settings": { "vnext": [{ "address": "1.2.3.4", "port": 443 }] }
            }]
        });
        pin_server_addresses(&mut config);
        assert_eq!(config["outbounds"][0]["settings"]["vnext"][0]["address"], "1.2.3.4");
    }
}
