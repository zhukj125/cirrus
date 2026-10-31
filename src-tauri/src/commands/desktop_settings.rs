use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use cirrus_core::{load_app_state_json, save_app_state_json};
use serde::Deserialize;
use tauri::State;

use crate::state::AppState;

const DESKTOP_SETTINGS_KEY: &str = "desktop_settings";
const LEGACY_MAP_CONFIG_FILE: &str = "map-config.json";
const AMAP_KEY: &str = "cirrus.amapKey";
const AMAP_SECURITY_CODE: &str = "cirrus.amapSecurityCode";
const LEGACY_AMAP_WEB_SERVICE_KEY: &str = "cirrus.amapWebServiceKey";
static DESKTOP_SETTINGS_WRITE_LOCK: Mutex<()> = Mutex::new(());

type SharedState<'a> = State<'a, Arc<AppState>>;
type DesktopSettings = BTreeMap<String, String>;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSettingChange {
    pub key: String,
    pub value: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyMapConfig {
    amap_key: Option<String>,
    amap_security_code: Option<String>,
}

fn legacy_config_path(data_dir: &Path) -> PathBuf {
    data_dir.join(LEGACY_MAP_CONFIG_FILE)
}

fn insert_non_empty(settings: &mut DesktopSettings, key: &str, value: Option<String>) {
    if let Some(value) = value.map(|value| value.trim().to_string()).filter(|value| !value.is_empty()) {
        settings.insert(key.to_string(), value);
    }
}

fn migrate_legacy_map_config(data_dir: &Path) -> DesktopSettings {
    let path = legacy_config_path(data_dir);
    let Some(legacy) =
        std::fs::read_to_string(&path).ok().and_then(|value| serde_json::from_str::<LegacyMapConfig>(&value).ok())
    else {
        return DesktopSettings::new();
    };
    let mut settings = DesktopSettings::new();
    insert_non_empty(&mut settings, AMAP_KEY, legacy.amap_key);
    insert_non_empty(&mut settings, AMAP_SECURITY_CODE, legacy.amap_security_code);
    if !settings.is_empty() && save_settings(data_dir, &settings).is_ok() {
        let _ = std::fs::remove_file(path);
    }
    settings
}

fn load_settings(data_dir: &Path) -> Result<DesktopSettings, String> {
    if let Some(mut settings) =
        load_app_state_json::<DesktopSettings>(data_dir, DESKTOP_SETTINGS_KEY).map_err(|error| error.to_string())?
    {
        if settings.remove(LEGACY_AMAP_WEB_SERVICE_KEY).is_some() {
            save_settings(data_dir, &settings)?;
        }
        return Ok(settings);
    }
    Ok(migrate_legacy_map_config(data_dir))
}

fn save_settings(data_dir: &Path, settings: &DesktopSettings) -> Result<(), String> {
    save_app_state_json(data_dir, DESKTOP_SETTINGS_KEY, settings).map_err(|error| error.to_string())
}

fn validate_key(key: &str) -> Result<(), String> {
    if !key.starts_with("cirrus.") || key.len() > 128 {
        return Err("Invalid desktop setting key".to_string());
    }
    Ok(())
}

#[tauri::command]
pub fn desktop_settings_load(state: SharedState<'_>) -> Result<DesktopSettings, String> {
    let data_dir = state.data_dir().ok_or_else(|| "App data directory is unavailable".to_string())?;
    load_settings(data_dir)
}

#[tauri::command]
pub fn desktop_setting_set(payload: DesktopSettingChange, state: SharedState<'_>) -> Result<(), String> {
    validate_key(&payload.key)?;
    let data_dir = state.data_dir().ok_or_else(|| "App data directory is unavailable".to_string())?;
    let _guard = DESKTOP_SETTINGS_WRITE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let mut settings = load_settings(data_dir)?;
    match payload.value.map(|value| value.trim().to_string()).filter(|value| !value.is_empty()) {
        Some(value) => {
            settings.insert(payload.key, value);
        }
        None => {
            settings.remove(&payload.key);
        }
    }
    save_settings(data_dir, &settings)
}

#[tauri::command]
pub fn desktop_settings_merge(payload: DesktopSettings, state: SharedState<'_>) -> Result<(), String> {
    let data_dir = state.data_dir().ok_or_else(|| "App data directory is unavailable".to_string())?;
    let _guard = DESKTOP_SETTINGS_WRITE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let mut settings = load_settings(data_dir)?;
    for (key, value) in payload {
        validate_key(&key)?;
        settings.entry(key).or_insert(value);
    }
    save_settings(data_dir, &settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_roundtrip_in_sqlite_and_migrate_legacy_map_config() {
        let dir = std::env::temp_dir().join(format!("cirrus-desktop-settings-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        std::fs::write(
            legacy_config_path(&dir),
            serde_json::to_vec(&serde_json::json!({
                "amapKey": "js-key",
                "amapSecurityCode": "security"
            }))
            .unwrap(),
        )
        .expect("write legacy");

        let settings = load_settings(&dir).expect("load");
        assert_eq!(settings.get(AMAP_KEY).map(String::as_str), Some("js-key"));
        assert!(!legacy_config_path(&dir).exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
