//! Persistent profile and subscription storage.

mod profiles;
mod subscriptions;

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub use crate::model::{Entry, Subscription};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid store: {0}")]
    Parse(#[source] serde_json::Error),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Store {
    pub entries: Vec<Entry>,
    pub subscriptions: Vec<Subscription>,
}

impl Store {
    pub fn load(path: &Path) -> Result<Self, Error> {
        match fs::read_to_string(path) {
            Ok(raw) => serde_json::from_str(&raw).map_err(Error::Parse),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(Error::Read {
                path: path.to_path_buf(),
                source,
            }),
        }
    }

    /// Persist the store, replacing the file atomically.
    pub fn save(&self, path: &Path) -> Result<(), Error> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|source| Error::Write {
                path: dir.to_path_buf(),
                source,
            })?;
        }

        let body = serde_json::to_vec_pretty(self).map_err(Error::Parse)?;
        let tmp = path.with_extension("json.tmp");
        {
            let mut file = fs::File::create(&tmp).map_err(|source| Error::Write {
                path: tmp.clone(),
                source,
            })?;
            file.write_all(&body).map_err(|source| Error::Write {
                path: tmp.clone(),
                source,
            })?;
            file.sync_all().map_err(|source| Error::Write {
                path: tmp.clone(),
                source,
            })?;
        }
        fs::rename(&tmp, path).map_err(|source| Error::Write {
            path: path.to_path_buf(),
            source,
        })
    }
}

/// Stable short identifier derived from a link or URL.
pub fn id_for(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    digest[..8].iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_link;
    use crate::Profile;

    fn profile(name: &str) -> Profile {
        parse_link(&format!(
            "vless://uuid-{name}@example.com:443?security=tls&sni=example.com#{name}"
        ))
        .unwrap()
    }

    #[test]
    fn upsert_replaces_same_link() {
        let mut store = Store::default();
        let link = "vless://uuid-a@example.com:443?security=tls#A";
        store.upsert(link, profile("a"), None);
        store.upsert(link, profile("a2"), None);
        assert_eq!(store.entries.len(), 1);
        assert_eq!(store.entries[0].profile.name, "a2");
    }

    #[test]
    fn removing_subscription_removes_its_entries() {
        let mut store = Store::default();
        let sub = store.add_subscription("https://example.com/sub", "Sub");
        store.upsert(
            "vless://uuid-a@example.com:443",
            profile("a"),
            Some(sub.id.clone()),
        );
        store.upsert("vless://uuid-b@example.com:443", profile("b"), None);

        store.remove_subscription(&sub.id);

        assert_eq!(store.entries.len(), 1);
        assert_eq!(store.entries[0].profile.name, "b");
        assert!(store.subscriptions.is_empty());
    }

    #[test]
    fn pin_survives_upsert_and_names_can_change() {
        let mut store = Store::default();
        let link = "vless://uuid-a@example.com:443";
        store.upsert(link, profile("a"), None);
        let id = store.entries[0].id.clone();

        assert!(store.set_entry_pinned(&id, true));
        store.upsert(link, profile("a"), None);

        assert!(store.entries[0].pinned);
        assert!(store.rename_entry(&id, "renamed"));
        assert_eq!(store.entries[0].profile.name, "renamed");
    }

    #[test]
    fn subscriptions_can_be_renamed() {
        let mut store = Store::default();
        let sub = store.add_subscription("https://example.com/sub", "Auto");

        assert!(store.rename_subscription(&sub.id, "Custom"));
        assert_eq!(store.find_subscription(&sub.id).unwrap().title, "Custom");
    }

    #[test]
    fn roundtrip_through_disk() {
        let dir = std::env::temp_dir().join(format!("rumurule-store-{}", id_for("test")));
        let path = dir.join("profiles.json");
        let _ = fs::remove_dir_all(&dir);

        let mut store = Store::default();
        store.upsert("vless://uuid-a@example.com:443", profile("a"), None);
        store.save(&path).unwrap();

        let loaded = Store::load(&path).unwrap();
        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(loaded.entries[0].profile.server, "example.com");

        let _ = fs::remove_dir_all(&dir);
    }
}
