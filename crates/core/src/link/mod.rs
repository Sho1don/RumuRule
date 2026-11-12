//! Share-link parsing: one module per protocol, shared helpers here.

mod hysteria;
mod shadowsocks;
mod trojan;
mod vless;
mod vmess;

use base64::Engine;
use percent_encoding::percent_decode_str;
use url::Url;

use crate::model::{Network, Profile, Security};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unsupported link scheme: {0}")]
    UnsupportedScheme(String),
    #[error("malformed link: {0}")]
    Malformed(String),
    #[error("missing required field: {0}")]
    MissingField(&'static str),
    #[error("invalid port: {0}")]
    InvalidPort(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Parse a share link (`vless://`, `vmess://`, `trojan://`, `ss://`,
/// `hysteria2://`).
pub fn parse_link(raw: &str) -> Result<Profile> {
    let raw = raw.trim();
    let (scheme, rest) = raw
        .split_once("://")
        .ok_or_else(|| Error::Malformed("no scheme separator".into()))?;
    let scheme = scheme.to_ascii_lowercase();

    match scheme.as_str() {
        "vless" => vless::parse(rest),
        "trojan" => trojan::parse(rest),
        "vmess" => vmess::parse(rest),
        "ss" => shadowsocks::parse(rest),
        "hysteria2" | "hy2" => hysteria::parse(rest),
        other => Err(Error::UnsupportedScheme(other.to_string())),
    }
}

pub(crate) fn decode(value: &str) -> String {
    percent_decode_str(value).decode_utf8_lossy().into_owned()
}

pub(crate) fn base64_decode(value: &str) -> Result<String> {
    let normalized: String = value
        .trim()
        .trim_end_matches('=')
        .chars()
        .map(|c| match c {
            '-' => '+',
            '_' => '/',
            other => other,
        })
        .collect();
    let engine = base64::engine::general_purpose::STANDARD_NO_PAD;
    engine
        .decode(normalized)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .ok_or_else(|| Error::Malformed("invalid base64 payload".into()))
}

pub(crate) fn query<'a>(url: &'a Url) -> impl Fn(&str) -> Option<String> + 'a {
    move |key: &str| {
        url.query_pairs()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.into_owned())
            .filter(|v| !v.is_empty())
    }
}

pub(crate) fn split_host_port(host: Option<&str>, port: Option<u16>) -> Result<(String, u16)> {
    let host = host.ok_or(Error::MissingField("server"))?;
    let port = port.ok_or(Error::MissingField("port"))?;
    Ok((host.to_string(), port))
}

pub(crate) fn fragment_name(url: &Url) -> String {
    url.fragment().map(decode).unwrap_or_default()
}

pub(crate) fn security_from(value: Option<String>) -> Security {
    Security::parse(value.as_deref().unwrap_or("none"))
}

pub(crate) fn alpn_list(value: Option<String>) -> Vec<String> {
    value
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn network_from(value: Option<String>) -> Network {
    Network::parse(value.as_deref().unwrap_or("tcp"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_scheme() {
        assert!(parse_link("http://example.com").is_err());
    }
}
