//! Saved server entries.

use serde_json::Value;

use super::{id_for, Store};
use crate::model::{Entry, Profile};

impl Store {
    /// Insert a profile, replacing an existing entry with the same link.
    pub fn upsert(
        &mut self,
        link: &str,
        profile: Profile,
        subscription: Option<String>,
    ) -> Entry {
        let id = id_for(link);
        let pinned = self
            .entries
            .iter()
            .find(|entry| entry.id == id)
            .is_some_and(|entry| entry.pinned);
        self.entries.retain(|entry| entry.id != id);
        let entry = Entry {
            id,
            link: link.to_string(),
            profile,
            subscription,
            config: None,
            pinned,
        };
        self.entries.insert(0, entry.clone());
        entry
    }

    pub fn remove_entry(&mut self, id: &str) {
        self.entries.retain(|entry| entry.id != id);
    }

    /// Give a server a custom name without touching its link.
    pub fn rename_entry(&mut self, id: &str, name: &str) -> bool {
        let name = name.trim();
        match self.entries.iter_mut().find(|entry| entry.id == id) {
            Some(entry) => {
                entry.profile.name = name.to_string();
                true
            }
            None => false,
        }
    }

    /// Keep a server on top of its group.
    pub fn set_entry_pinned(&mut self, id: &str, pinned: bool) -> bool {
        match self.entries.iter_mut().find(|entry| entry.id == id) {
            Some(entry) => {
                entry.pinned = pinned;
                true
            }
            None => false,
        }
    }

    /// Drop every server that came from the given subscription.
    pub fn clear_subscription_entries(&mut self, subscription_id: &str) {
        self.entries
            .retain(|entry| entry.subscription.as_deref() != Some(subscription_id));
    }
}

impl Store {
    /// Insert a ready-made Xray config, replacing the identical one.
    pub fn upsert_config(
        &mut self,
        config: Value,
        profile: Profile,
        subscription: Option<String>,
    ) -> Entry {
        let key = serde_json::to_string(&config).unwrap_or_default();
        let id = id_for(&key);
        let pinned = self
            .entries
            .iter()
            .find(|entry| entry.id == id)
            .is_some_and(|entry| entry.pinned);
        self.entries.retain(|entry| entry.id != id);
        let entry = Entry {
            id,
            link: String::new(),
            profile,
            subscription,
            config: Some(config),
            pinned,
        };
        self.entries.insert(0, entry.clone());
        entry
    }
}
