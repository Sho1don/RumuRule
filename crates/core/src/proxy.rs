//! Local proxy endpoints and the settings desktop environments expect.

/// Hosts and networks that must bypass the proxy.
pub const NO_PROXY: &str =
    "localhost,127.0.0.1,::1,10.0.0.0/8,172.16.0.0/12,192.168.0.0/16";

/// Ports of the local Xray inbounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Endpoints {
    pub socks: u16,
    pub http: u16,
}

impl Endpoints {
    pub fn new(socks: u16, http: u16) -> Self {
        Self { socks, http }
    }
}

/// Keys KDE reads from `kioslaverc` under `[Proxy Settings]`.
///
/// KDE stores the HTTP family as `scheme://host port` (a space before the
/// port) while SOCKS keeps the usual `scheme://host:port` form.
pub fn kde_settings(endpoints: Endpoints) -> [(&'static str, String); 7] {
    [
        ("ProxyType", "1".to_string()),
        (
            "httpProxy",
            format!("http://127.0.0.1 {}", endpoints.http),
        ),
        (
            "httpsProxy",
            format!("http://127.0.0.1 {}", endpoints.http),
        ),
        (
            "ftpProxy",
            format!("http://127.0.0.1 {}", endpoints.http),
        ),
        (
            "socksProxy",
            format!("socks://127.0.0.1:{}", endpoints.socks),
        ),
        ("NoProxyFor", NO_PROXY.to_string()),
        ("ReversedException", "false".to_string()),
    ]
}

/// Schema, key and GVariant value for the GNOME proxy settings.
pub fn gnome_settings(endpoints: Endpoints) -> [(&'static str, &'static str, String); 9] {
    let ignore = NO_PROXY
        .split(',')
        .map(|host| format!("'{host}'"))
        .collect::<Vec<_>>()
        .join(",");

    [
        ("org.gnome.system.proxy", "mode", "'manual'".to_string()),
        (
            "org.gnome.system.proxy.http",
            "host",
            "'127.0.0.1'".to_string(),
        ),
        (
            "org.gnome.system.proxy.http",
            "port",
            endpoints.http.to_string(),
        ),
        (
            "org.gnome.system.proxy.https",
            "host",
            "'127.0.0.1'".to_string(),
        ),
        (
            "org.gnome.system.proxy.https",
            "port",
            endpoints.http.to_string(),
        ),
        (
            "org.gnome.system.proxy.socks",
            "host",
            "'127.0.0.1'".to_string(),
        ),
        (
            "org.gnome.system.proxy.socks",
            "port",
            endpoints.socks.to_string(),
        ),
        ("org.gnome.system.proxy", "ignore-hosts", format!("[{ignore}]")),
        (
            "org.gnome.system.proxy",
            "use-same-proxy",
            "true".to_string(),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoints() -> Endpoints {
        Endpoints::new(10808, 10809)
    }

    #[test]
    fn kde_uses_space_before_the_http_port() {
        let settings = kde_settings(endpoints());
        let value = |key: &str| {
            settings
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| value.as_str())
                .unwrap()
        };
        assert_eq!(value("ProxyType"), "1");
        assert_eq!(value("httpProxy"), "http://127.0.0.1 10809");
        assert_eq!(value("httpsProxy"), "http://127.0.0.1 10809");
        assert_eq!(value("socksProxy"), "socks://127.0.0.1:10808");
    }

    #[test]
    fn gnome_switches_to_manual_with_both_ports() {
        let settings = gnome_settings(endpoints());
        let value = |schema: &str, key: &str| {
            settings
                .iter()
                .find(|(s, k, _)| *s == schema && *k == key)
                .map(|(_, _, value)| value.as_str())
                .unwrap()
        };
        assert_eq!(value("org.gnome.system.proxy", "mode"), "'manual'");
        assert_eq!(value("org.gnome.system.proxy.http", "port"), "10809");
        assert_eq!(value("org.gnome.system.proxy.socks", "port"), "10808");
    }

    #[test]
    fn ignore_list_covers_loopback_and_private_networks() {
        assert!(NO_PROXY.contains("127.0.0.1"));
        assert!(NO_PROXY.contains("192.168.0.0/16"));
    }
}
