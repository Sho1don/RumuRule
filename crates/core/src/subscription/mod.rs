//! Subscription fetching and parsing.

mod fetch;
mod parse;

pub use fetch::{fetch, Device, Fetched, DEFAULT_USER_AGENT};
pub use parse::{panel_error, parse_configs, parse_subscription, Imported};
