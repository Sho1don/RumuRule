//! Saved profiles and subscription commands.

use std::{path::PathBuf, sync::Mutex};

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use rumurule_core::{
    fetch, panel_error, parse_configs, parse_link, parse_subscription, Entry, Imported, Profile,
    Store, Subscription, TunnelMode,
};

use crate::commands::settings::{apply_proxy, SettingsStore};
use crate::system_proxy::SystemProxy;
use crate::xray::{emit_progress, Status};

pub struct Profiles {
    path: PathBuf,
    store: Mutex<Store>,
}

impl Profiles {
    /// Copy of a saved entry, including any ready-made subscription config.
    pub fn entry(&self, id: &str) -> Option<Entry> {
        let store = self.store.lock().expect("profiles poisoned");
        store
            .entries
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
    }

    pub fn open(app: &AppHandle) -> Result<Self, String> {
        let dir = app
            .path()
            .app_config_dir()
            .map_err(|e| format!("cannot resolve config dir: {e}"))?;
        let path = dir.join("profiles.json");
        let store = Store::load(&path).map_err(|e| e.to_string())?;
        Ok(Self {
            path,
            store: Mutex::new(store),
        })
    }

    fn persist(&self, store: &Store) -> Result<(), String> {
        store.save(&self.path).map_err(|e| e.to_string())
    }
}

/// Entry as exposed to the UI: the full Xray config stays on the backend.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryView {
    pub id: String,
    pub link: String,
    pub profile: Profile,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscription: Option<String>,
    pub pinned: bool,
}

impl From<&Entry> for EntryView {
    fn from(entry: &Entry) -> Self {
        Self {
            id: entry.id.clone(),
            link: entry.link.clone(),
            profile: entry.profile.clone(),
            subscription: entry.subscription.clone(),
            pinned: entry.pinned,
        }
    }
}

fn views(entries: &[Entry]) -> Vec<EntryView> {
    entries.iter().map(EntryView::from).collect()
}

/// Validate a subscription body and return its servers (JSON configs or links).
fn prepare(body: &str) -> Result<Vec<Imported>, String> {
    if let Some(configs) = parse_configs(body) {
        return Ok(configs);
    }

    let imported = parse_subscription(body);
    if let Some(message) = panel_error(&imported) {
        return Err(format!("panel returned an error instead of servers: {message}"));
    }
    if imported.is_empty() {
        return Err("subscription contains no supported links".into());
    }
    Ok(imported)
}

fn insert(store: &mut Store, items: Vec<Imported>, subscription_id: &str) -> usize {
    let count = items.len();
    for item in items {
        let owner = Some(subscription_id.to_string());
        match item.config {
            Some(config) => store.upsert_config(config, item.profile, owner),
            None => store.upsert(&item.link, item.profile, owner),
        };
    }
    count
}

#[tauri::command]
pub fn profiles_list(state: State<'_, Profiles>) -> Vec<EntryView> {
    views(&state.store.lock().expect("profiles poisoned").entries)
}

#[tauri::command]
pub fn profile_add(state: State<'_, Profiles>, link: String) -> Result<EntryView, String> {
    let link = link.trim().to_string();
    let profile = parse_link(&link).map_err(|e| e.to_string())?;

    let mut store = state.store.lock().expect("profiles poisoned");
    let entry = store.upsert(&link, profile, None);
    state.persist(&store)?;
    Ok(EntryView::from(&entry))
}

#[tauri::command]
pub fn profile_rename(
    state: State<'_, Profiles>,
    id: String,
    name: String,
) -> Result<Vec<EntryView>, String> {
    let mut store = state.store.lock().expect("profiles poisoned");
    if !store.rename_entry(&id, &name) {
        return Err(format!("unknown profile {id}"));
    }
    state.persist(&store)?;
    Ok(views(&store.entries))
}

#[tauri::command]
pub fn profile_pin(
    state: State<'_, Profiles>,
    id: String,
    pinned: bool,
) -> Result<Vec<EntryView>, String> {
    let mut store = state.store.lock().expect("profiles poisoned");
    if !store.set_entry_pinned(&id, pinned) {
        return Err(format!("unknown profile {id}"));
    }
    state.persist(&store)?;
    Ok(views(&store.entries))
}

#[tauri::command]
pub fn profile_remove(state: State<'_, Profiles>, id: String) -> Result<Vec<EntryView>, String> {
    let mut store = state.store.lock().expect("profiles poisoned");
    store.remove_entry(&id);
    state.persist(&store)?;
    Ok(views(&store.entries))
}

#[tauri::command]
pub fn subscription_rename(
    state: State<'_, Profiles>,
    id: String,
    title: String,
) -> Result<Vec<Subscription>, String> {
    let mut store = state.store.lock().expect("profiles poisoned");
    if !store.rename_subscription(&id, title.trim()) {
        return Err(format!("unknown subscription {id}"));
    }
    state.persist(&store)?;
    Ok(store.subscriptions.clone())
}

#[tauri::command]
pub fn subscriptions_list(state: State<'_, Profiles>) -> Vec<Subscription> {
    state
        .store
        .lock()
        .expect("profiles poisoned")
        .subscriptions
        .clone()
}

#[tauri::command]
pub async fn subscription_add(
    state: State<'_, Profiles>,
    settings: State<'_, SettingsStore>,
    url: String,
) -> Result<Vec<EntryView>, String> {
    let url = url.trim().to_string();
    let device = settings.device();
    eprintln!("[profiles] importing {url} (hwid: {})", device.is_some());
    let fetched = match fetch(&url, device.as_ref()).await {
        Ok(fetched) => fetched,
        Err(error) => {
            eprintln!("[profiles] fetch failed: {error}");
            return Err(error);
        }
    };
    eprintln!(
        "[profiles] received {} bytes (title: {:?})",
        fetched.body.len(),
        fetched.title
    );

    let items = prepare(&fetched.body)?;
    eprintln!("[profiles] parsed {} servers", items.len());

    let mut store = state.store.lock().expect("profiles poisoned");
    let title = fetched.title.unwrap_or_else(|| title_for(&url));
    let subscription = store.add_subscription(&url, title);
    insert(&mut store, items, &subscription.id);
    state.persist(&store)?;
    Ok(views(&store.entries))
}

#[tauri::command]
pub async fn subscription_refresh(
    state: State<'_, Profiles>,
    settings: State<'_, SettingsStore>,
    id: String,
) -> Result<Vec<EntryView>, String> {
    let url = {
        let store = state.store.lock().expect("profiles poisoned");
        let subscription = store
            .find_subscription(&id)
            .ok_or_else(|| format!("unknown subscription {id}"))?;
        subscription.url.clone()
    };

    let device = settings.device();
    let fetched = fetch(&url, device.as_ref()).await?;
    let items = prepare(&fetched.body)?;

    let mut store = state.store.lock().expect("profiles poisoned");
    let pinned: Vec<String> = store
        .entries
        .iter()
        .filter(|entry| entry.subscription.as_deref() == Some(id.as_str()) && entry.pinned)
        .map(|entry| entry.id.clone())
        .collect();

    if let Some(title) = fetched.title {
        store.set_subscription_title(&id, title);
    }
    store.clear_subscription_entries(&id);
    insert(&mut store, items, &id);
    for id in pinned {
        store.set_entry_pinned(&id, true);
    }
    state.persist(&store)?;
    Ok(views(&store.entries))
}

#[tauri::command]
pub fn subscription_remove(
    state: State<'_, Profiles>,
    id: String,
) -> Result<Vec<EntryView>, String> {
    let mut store = state.store.lock().expect("profiles poisoned");
    store.remove_subscription(&id);
    state.persist(&store)?;
    Ok(views(&store.entries))
}

/// Connect using a stored profile or a ready-made subscription config.
#[tauri::command(async)]
pub fn connect_entry(
    app: AppHandle,
    profiles: State<'_, Profiles>,
    settings: State<'_, SettingsStore>,
    proxy: State<'_, SystemProxy>,
    xray: State<'_, crate::xray::XrayManager>,
    id: String,
) -> Result<Status, String> {
    let entry = {
        let store = profiles.store.lock().expect("profiles poisoned");
        store
            .entries
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
            .ok_or_else(|| format!("unknown profile {id}"))?
    };

    let mode = settings.snapshot().tunnel_mode;
    let status = if mode == TunnelMode::Tun {
        let base = match entry.config {
            Some(config) => config,
            None => {
                let profile = parse_link(&entry.link).map_err(|e| e.to_string())?;
                rumurule_core::config::build_config(&profile)
            }
        };
        xray.start_tun(&app, &base)?
    } else {
        match entry.config {
            Some(config) => xray.start_config(&app, &config)?,
            None => {
                let profile = parse_link(&entry.link).map_err(|e| e.to_string())?;
                xray.start(&app, &profile)?
            }
        }
    };

    apply_proxy(&app, &settings, &proxy, &status);
    emit_progress(&app, 1.0, "connected");
    Ok(status)
}

fn title_for(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|parsed| {
            parsed
                .path_segments()
                .and_then(|mut segments| segments.next_back().map(str::to_string))
                .filter(|segment| !segment.is_empty())
        })
        .or_else(|| url::Url::parse(url).ok().and_then(|p| p.host_str().map(str::to_string)))
        .unwrap_or_else(|| "subscription".into())
}
