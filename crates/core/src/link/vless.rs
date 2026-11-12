//! `vless://` links (plain TCP, WS, gRPC, REALITY).

use url::Url;

use super::{
    alpn_list, decode, fragment_name, network_from, query, security_from, split_host_port, Error,
    Result,
};
use crate::model::Profile;

pub(super) fn parse(rest: &str) -> Result<Profile> {
    let url = Url::parse(&format!("vless://{rest}"))
        .map_err(|e| Error::Malformed(e.to_string()))?;
    let (server, port) = split_host_port(url.host_str(), url.port())?;
    if url.username().is_empty() {
        return Err(Error::MissingField("uuid"));
    }

    let id = decode(url.username());
    let q = query(&url);

    Ok(Profile {
        name: fragment_name(&url),
        protocol: "vless".into(),
        server,
        port,
        id,
        network: network_from(q("type")),
        security: security_from(q("security")),
        sni: q("sni").or_else(|| q("peer")),
        alpn: alpn_list(q("alpn")),
        fingerprint: q("fp"),
        flow: q("flow"),
        path: q("path"),
        host: q("host"),
        service_name: q("serviceName"),
        public_key: q("pbk"),
        short_id: q("sid"),
        encryption: Some(q("encryption").unwrap_or_else(|| "none".into())),
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

    #[test]
    fn parses_reality() {
        let link = "vless://11111111-2222-3333-4444-555555555555@example.com:443?security=reality&sni=cdn.example.com&fp=chrome&pbk=PUBKEY&sid=ab12&type=grpc&serviceName=svc&flow=xtls-rprx-vision#Node%20A";
        let p = parse_link(link).expect("vless");
        assert_eq!(p.protocol, "vless");
        assert_eq!(p.server, "example.com");
        assert_eq!(p.port, 443);
        assert_eq!(p.security, Security::Reality);
        assert_eq!(p.network, Network::Grpc);
        assert_eq!(p.name, "Node A");
        assert_eq!(p.service_name.as_deref(), Some("svc"));
        assert_eq!(p.public_key.as_deref(), Some("PUBKEY"));
    }

    #[test]
    fn parses_websocket() {
        let link = "vless://aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee@1.2.3.4:8080?type=ws&path=%2Fws&host=cdn.test&security=tls&sni=cdn.test";
        let p = parse_link(link).expect("vless ws");
        assert_eq!(p.network, Network::Ws);
        assert_eq!(p.path.as_deref(), Some("/ws"));
        assert_eq!(p.host.as_deref(), Some("cdn.test"));
        assert_eq!(p.security, Security::Tls);
    }
}
