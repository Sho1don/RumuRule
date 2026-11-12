//! Remote subscription sources.

use super::{id_for, Store};
use crate::model::Subscription;

impl Store {
    pub fn add_subscription(&mut self, url: &str, title: impl Into<String>) -> Subscription {
        let id = id_for(url);
        let subscription = Subscription {
            id,
            url: url.to_string(),
            title: title.into(),
        };
        self.subscriptions.retain(|s| s.id != subscription.id);
        self.subscriptions.push(subscription.clone());
        subscription
    }

    pub fn remove_subscription(&mut self, id: &str) {
        self.subscriptions.retain(|s| s.id != id);
        self.clear_subscription_entries(id);
    }

    pub fn find_subscription(&self, id: &str) -> Option<&Subscription> {
        self.subscriptions.iter().find(|s| s.id == id)
    }

    /// Rename a subscription manually; a refresh may replace this name.
    pub fn rename_subscription(&mut self, id: &str, title: impl Into<String>) -> bool {
        match self.subscriptions.iter_mut().find(|s| s.id == id) {
            Some(subscription) => {
                subscription.title = title.into();
                true
            }
            None => false,
        }
    }

    /// Update the panel-provided name after a refresh.
    pub fn set_subscription_title(&mut self, id: &str, title: impl Into<String>) {
        if let Some(subscription) = self.subscriptions.iter_mut().find(|s| s.id == id) {
            subscription.title = title.into();
        }
    }
}
