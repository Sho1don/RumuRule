//! `trojan://` links.

use url::Url;

use super::{
    alpn_list, decode, fragment_name, network_from, query, split_host_port, Error, Result,
};
use crate::model::{Profile, Security};

pub(super) fn parse(rest: &str) -> Result<Profile> {
    let url = Url::parse(&format!("trojan://{rest}"))
        .map_err(|e| Error::Malformed(e.to_string()))?;
    let (server, port) = split_host_port(url.host_str(), url.port())?;
    if url.username().is_empty() {
        return Err(Error::MissingField("password"));
    }

    let id = decode(url.username());
    let q = query(&url);

    Ok(Profile {
        name: fragment_name(&url),
        protocol: "trojan".into(),
        server,
        port,
        id,
        network: network_from(q("type")),
        security: Security::Tls,
        sni: q("sni").or_else(|| q("peer")),
        alpn: alpn_list(q("alpn")),
        fingerprint: q("fp"),
        flow: None,
        path: q("path"),
        host: q("host"),
        service_name: q("serviceName"),
        public_key: None,
        short_id: None,
        encryption: None,
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
    use crate::model::Security;

    #[test]
    fn parses_trojan() {
        let p = parse_link("trojan://secret@example.org:8443?sni=example.org#Trojan").expect("trojan");
        assert_eq!(p.protocol, "trojan");
        assert_eq!(p.id, "secret");
        assert_eq!(p.security, Security::Tls);
    }
}
