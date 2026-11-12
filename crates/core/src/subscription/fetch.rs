//! HTTP client used to download subscriptions.

use std::{env, time::Duration};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

/// Some subscription panels only answer requests from known clients, so the
/// user agent is configurable (`RUMURULE_SUB_UA`) and accepts a common default.
pub const DEFAULT_USER_AGENT: &str = "Happ/1.0";

/// Device identity sent to panels that bind a subscription to one device.
#[derive(Debug, Clone)]
pub struct Device {
    pub hwid: String,
    pub os: String,
    pub model: String,
    pub app_version: String,
}

impl Device {
    fn headers(&self) -> Result<HeaderMap, String> {
        let mut headers = HeaderMap::new();
        let mut insert = |name: &'static str, value: &str| -> Result<(), String> {
            let value = HeaderValue::from_str(value).map_err(|e| e.to_string())?;
            headers.insert(HeaderName::from_static(name), value);
            Ok(())
        };

        insert("x-hwid", &self.hwid)?;
        insert("x-device-os", &self.os)?;
        insert("x-device-model", &self.model)?;
        insert("x-device-name", &self.model)?;
        insert("x-ver-os", "1.0")?;
        insert("x-ver-app", &self.app_version)?;
        Ok(headers)
    }
}

/// Body plus the human readable name the panel advertises for it.
#[derive(Debug, Clone)]
pub struct Fetched {
    pub body: String,
    pub title: Option<String>,
}

/// Fetch a subscription body over HTTP(S). When `device` is set, HWID headers
/// are attached so the panel can bind the subscription to this device.
pub async fn fetch(url: &str, device: Option<&Device>) -> Result<Fetched, String> {
    let user_agent = env::var("RUMURULE_SUB_UA").unwrap_or_else(|_| DEFAULT_USER_AGENT.to_string());

    let mut builder = reqwest::Client::builder()
        .user_agent(user_agent)
        .timeout(Duration::from_secs(20));

    if let Some(device) = device {
        builder = builder.default_headers(device.headers()?);
    }

    let client = builder.build().map_err(|e| e.to_string())?;

    let response = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("subscription returned HTTP {}", response.status()));
    }

    // Panels name their subscription in `profile-title`, often base64 encoded.
    let title = response
        .headers()
        .get("profile-title")
        .and_then(|value| value.to_str().ok())
        .and_then(decode_title);

    let body = response.text().await.map_err(|e| e.to_string())?;
    Ok(Fetched { body, title })
}

fn decode_title(raw: &str) -> Option<String> {
    let text = raw.trim();
    let decoded = match text.strip_prefix("base64:") {
        Some(encoded) => {
            let bytes = STANDARD.decode(encoded.trim()).ok()?;
            String::from_utf8(bytes).ok()?
        }
        None => text.to_string(),
    };

    let decoded = decoded.trim().to_string();
    (!decoded.is_empty()).then_some(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_headers_cover_the_panel_contract() {
        let device = Device {
            hwid: "11111111-2222-3333-4444-555555555555".into(),
            os: "Linux".into(),
            model: "RumuRule".into(),
            app_version: "0.1.0".into(),
        };
        let headers = device.headers().unwrap();
        assert_eq!(headers["x-hwid"], device.hwid);
        assert_eq!(headers["x-device-os"], "Linux");
        assert_eq!(headers["x-ver-app"], "0.1.0");
    }

    #[test]
    fn title_accepts_base64_and_plain_text() {
        assert_eq!(decode_title("base64:RXhhbXBsZSBWUE4=").as_deref(), Some("Example VPN"));
        assert_eq!(decode_title(" Example VPN ").as_deref(), Some("Example VPN"));
        assert_eq!(decode_title("   "), None);
    }
}
