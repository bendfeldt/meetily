//! Fork-only settings (bendfeldt/meetily), kept out of upstream's
//! `RecordingPreferences` so its settings page cannot wipe them and upstream
//! syncs don't conflict. Stored in its own tauri-plugin-store file. The
//! Settings UI for these arrives with the Teams toggle (plan step 5).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};
use tauri_plugin_store::StoreExt;

const STORE_FILE: &str = "fork_settings.json";
const STORE_KEY: &str = "settings";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ForkSettings {
    /// Folder (usually inside an Obsidian vault) that meeting notes go to.
    #[serde(default)]
    pub obsidian_export_folder: Option<PathBuf>,
}

pub fn load<R: Runtime>(app: &AppHandle<R>) -> ForkSettings {
    let Ok(store) = app.store(STORE_FILE) else {
        log::warn!("fork_settings: store unavailable, using defaults");
        return ForkSettings::default();
    };
    match store.get(STORE_KEY).map(serde_json::from_value) {
        Some(Ok(settings)) => settings,
        Some(Err(e)) => {
            log::warn!("fork_settings: unreadable {STORE_FILE}, using defaults: {e}");
            ForkSettings::default()
        }
        None => ForkSettings::default(),
    }
}

pub fn save<R: Runtime>(app: &AppHandle<R>, settings: &ForkSettings) -> anyhow::Result<()> {
    let store = app
        .store(STORE_FILE)
        .map_err(|e| anyhow::anyhow!("Failed to open {STORE_FILE}: {e}"))?;
    store.set(STORE_KEY, serde_json::to_value(settings)?);
    store
        .save()
        .map_err(|e| anyhow::anyhow!("Failed to save {STORE_FILE}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_load_as_defaults() {
        let settings: ForkSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings, ForkSettings::default());
    }
}
