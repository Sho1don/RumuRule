//! `ss://` links, both the modern `method:password@host` and the legacy
//! fully base64-encoded form.

use super::{base64_decode, decode, Error, Result};
use crate::model::{Network, Profile, Security};

pub(super) fn parse(rest: &str) -> Result<Profile> {
    let (body, name) = match rest.split_once('#') {
        Some((body, fragment)) => (body, decode(fragment)),
        None => (rest, String::new()),
    };
    let body = body.split('?').next().unwrap_or(body);

    let (credentials, host_port) = match body.rsplit_once('@') {
        Some((userinfo, host_port)) => {
            let credentials = base64_decode(userinfo).unwrap_or_else(|_| decode(userinfo));
            (credentials, host_port.to_string())
        }
        None => {
            let decoded = base64_decode(body)?;
            let (credentials, host_port) = decoded
                .rsplit_once('@')
                .ok_or(Error::MissingField("server"))?;
            (credentials.to_string(), host_port.to_string())
        }
    };

    let (method, password) = credentials
        .split_once(':')
        .ok_or(Error::MissingField("method"))?;
    let (host, port) = host_port
        .rsplit_once(':')
        .ok_or(Error::MissingField("port"))?;

    let server = host.trim_matches(['[', ']']).to_string();
    let port: u16 = port
        .trim_end_matches('/')
        .parse()
        .map_err(|_| Error::InvalidPort(port.to_string()))?;

    Ok(Profile {
        name,
        protocol: "shadowsocks".into(),
        server,
        port,
        id: password.to_string(),
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
        method: Some(method.to_string()),
        obfs: None,
        obfs_password: None,
        pin_sha256: None,
        insecure: false,
    })
}

#[cfg(test)]
mod tests {
    use super::super::parse_link;

    #[test]
    fn parses_modern_form() {
        // base64("aes-256-gcm:pass")
        let link = "ss://YWVzLTI1Ni1nY206cGFzcw@example.net:8388#SS";
        let p = parse_link(link).expect("ss");
        assert_eq!(p.protocol, "shadowsocks");
        assert_eq!(p.method.as_deref(), Some("aes-256-gcm"));
        assert_eq!(p.id, "pass");
        assert_eq!(p.port, 8388);
    }
}
