//! Shared logic: link parsing, Xray config building and persistent storage.
//!
//! This crate has no Tauri dependency, so every module can be tested on its
//! own with `cargo test -p rumurule-core`.

pub mod config;
pub mod link;
pub mod model;
pub mod proxy;
pub mod settings;
pub mod store;
pub mod subscription;

pub use link::{parse_link, Error};
pub use model::{Entry, Network, Profile, Security, Subscription};
pub use proxy::Endpoints;
pub use settings::{PingKind, PingMode, Settings, SplitMode, SplitTunnel, TunnelMode};
pub use store::Store;
pub use subscription::{fetch, panel_error, parse_configs, parse_subscription, Device, Imported};

/// Build-wide identity shared by the UI, the core bridge and the packaging scripts.
pub struct AppInfo;

impl AppInfo {
    pub const NAME: &'static str = "RumuRule";
    pub const VERSION: &'static str = "0.1.0";
}
