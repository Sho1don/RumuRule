//! `hysteria2://` links (also shared as `hy2://`).

use url::Url;

use super::{alpn_list, decode, fragment_name, query, split_host_port, Error, Result};
use crate::model::{Network, Profile, Security};

pub(super) fn parse(rest: &str) -> Result<Profile> {
    let url =
        Url::parse(&format!("hysteria2://{rest}")).map_err(|e| Error::Malformed(e.to_string()))?;
    let (server, port) = split_host_port(url.host_str(), url.port())?;

    // The userinfo is the Hysteria auth string: either a password or
    // `user:password`, both sent verbatim.
    let mut auth = decode(url.username());
    if let Some(password) = url.password().filter(|value| !value.is_empty()) {
        auth = format!("{auth}:{}", decode(password));
    }
    if auth.is_empty() {
        return Err(Error::MissingField("auth"));
    }

    let q = query(&url);
    let insecure = matches!(
        q("insecure").as_deref(),
        Some("1" | "true" | "yes" | "on")
    );

    Ok(Profile {
        name: fragment_name(&url),
        protocol: "hysteria".into(),
        server,
        port,
        id: auth,
        // Hysteria 2 always runs over QUIC; the transport fields below only
        // matter for the TLS layer.
        network: Network::Tcp,
        security: Security::Tls,
        sni: q("sni").or_else(|| q("peer")),
        alpn: alpn_list(q("alpn")),
        fingerprint: q("fp"),
        flow: None,
        path: None,
        host: None,
        service_name: None,
        public_key: None,
        short_id: None,
        encryption: None,
        method: None,
        obfs: q("obfs"),
        obfs_password: q("obfs-password").or_else(|| q("obfsPassword")),
        pin_sha256: q("pinSHA256").or_else(|| q("pin-sha256")),
        insecure,
        mode: None,
        header_type: None,
        early_data: None,
        early_data_header: None,
        extra: None,
    })
}

#[cfg(test)]
mod tests {
    use super::super::parse_link;

    #[test]
    fn parses_plain_link() {
        let p = parse_link("hysteria2://secret@example.org:8443/?sni=cdn.example&insecure=1#HY2")
            .expect("hysteria2");
        assert_eq!(p.protocol, "hysteria");
        assert_eq!(p.id, "secret");
        assert_eq!(p.server, "example.org");
        assert_eq!(p.port, 8443);
        assert_eq!(p.sni.as_deref(), Some("cdn.example"));
        assert!(p.insecure);
    }

    #[test]
    fn parses_user_and_obfuscation() {
        let p = parse_link(
            "hy2://user:pass@1.2.3.4:443/?obfs=salamander&obfs-password=s3cret&alpn=h3",
        )
        .expect("hy2");
        assert_eq!(p.id, "user:pass");
        assert_eq!(p.obfs.as_deref(), Some("salamander"));
        assert_eq!(p.obfs_password.as_deref(), Some("s3cret"));
        assert_eq!(p.alpn, vec!["h3".to_string()]);
    }
}
