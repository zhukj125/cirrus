use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{load_app_state_json, save_app_state_json, AppResult};

const SETTINGS_KEY: &str = "mcp_settings";
const ACCOUNT_ACCESS_KEY: &str = "mcp_account_access";
const TOOL_ACCESS_KEY: &str = "mcp_tool_access";
const LEGACY_SETTINGS_FILE: &str = "mcp-settings.json";
const LEGACY_ACCOUNT_ACCESS_FILE: &str = "mcp-account-access.json";
const LEGACY_TOOL_ACCESS_FILE: &str = "mcp-tool-access.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpSettings {
    pub enabled: bool,
}

impl Default for McpSettings {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAccountAccessSettings {
    #[serde(default)]
    pub disabled_account_keys: BTreeSet<String>,
    #[serde(default)]
    pub deny_all: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolAccessSettings {
    #[serde(default)]
    pub disabled_tools: BTreeSet<String>,
    #[serde(default)]
    pub deny_all: bool,
}

pub fn load_mcp_settings(data_dir: &Path) -> McpSettings {
    load_or_migrate(
        data_dir,
        SETTINGS_KEY,
        LEGACY_SETTINGS_FILE,
        McpSettings::default(),
        McpSettings { enabled: false },
    )
}

pub fn save_mcp_settings(data_dir: &Path, enabled: bool) -> AppResult<()> {
    save_and_remove_legacy(data_dir, SETTINGS_KEY, LEGACY_SETTINGS_FILE, &McpSettings { enabled })
}

pub fn load_account_access(data_dir: &Path) -> McpAccountAccessSettings {
    load_or_migrate(
        data_dir,
        ACCOUNT_ACCESS_KEY,
        LEGACY_ACCOUNT_ACCESS_FILE,
        McpAccountAccessSettings::default(),
        McpAccountAccessSettings { deny_all: true, ..Default::default() },
    )
}

pub fn save_account_access(data_dir: &Path, settings: &McpAccountAccessSettings) -> AppResult<()> {
    save_and_remove_legacy(data_dir, ACCOUNT_ACCESS_KEY, LEGACY_ACCOUNT_ACCESS_FILE, settings)
}

pub fn load_tool_access(data_dir: &Path) -> McpToolAccessSettings {
    load_or_migrate(
        data_dir,
        TOOL_ACCESS_KEY,
        LEGACY_TOOL_ACCESS_FILE,
        McpToolAccessSettings::default(),
        McpToolAccessSettings { deny_all: true, ..Default::default() },
    )
}

pub fn save_tool_access(data_dir: &Path, settings: &McpToolAccessSettings) -> AppResult<()> {
    save_and_remove_legacy(data_dir, TOOL_ACCESS_KEY, LEGACY_TOOL_ACCESS_FILE, settings)
}

fn load_or_migrate<T>(data_dir: &Path, key: &str, legacy_file: &str, default: T, fail_closed: T) -> T
where
    T: for<'de> Deserialize<'de> + Serialize + Clone,
{
    match load_app_state_json::<T>(data_dir, key) {
        Ok(Some(settings)) => return settings,
        Err(error) => {
            log::error!("[MCP] could not read access settings for {key}; denying access: {error}");
            return fail_closed;
        }
        Ok(None) => {}
    }

    let legacy_path = data_dir.join(legacy_file);
    let settings = match std::fs::read_to_string(&legacy_path) {
        Ok(value) => match serde_json::from_str::<T>(&value) {
            Ok(settings) => settings,
            Err(error) => {
                log::error!("[MCP] could not parse legacy access settings {}: {error}", legacy_path.display());
                return fail_closed;
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => default,
        Err(error) => {
            log::error!("[MCP] could not read legacy access settings {}: {error}", legacy_path.display());
            return fail_closed;
        }
    };
    if let Err(error) = save_app_state_json(data_dir, key, &settings) {
        log::warn!("[MCP] could not persist migrated access settings for {key}: {error}");
    } else {
        let _ = std::fs::remove_file(legacy_path);
    }
    settings
}

fn save_and_remove_legacy<T: Serialize>(data_dir: &Path, key: &str, legacy_file: &str, settings: &T) -> AppResult<()> {
    save_app_state_json(data_dir, key, settings)?;
    let _ = std::fs::remove_file(data_dir.join(legacy_file));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("cirrus-mcp-access-{name}-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn defaults_to_enabled_and_empty_access_lists() {
        let dir = temp_dir("defaults");
        assert!(load_mcp_settings(&dir).enabled);
        assert!(load_account_access(&dir).disabled_account_keys.is_empty());
        assert!(load_tool_access(&dir).disabled_tools.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn read_or_parse_failures_fail_closed() {
        let dir = temp_dir("fail-closed");
        let connection = crate::open_database(&crate::database_path(&dir)).expect("open database");
        for key in [SETTINGS_KEY, ACCOUNT_ACCESS_KEY, TOOL_ACCESS_KEY] {
            connection
                .execute(
                    "INSERT OR REPLACE INTO app_state (key, value_json) VALUES (?1, ?2)",
                    rusqlite::params![key, "not-json"],
                )
                .expect("seed invalid setting");
        }
        assert!(!load_mcp_settings(&dir).enabled);
        assert!(load_account_access(&dir).deny_all);
        assert!(load_tool_access(&dir).deny_all);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn migrates_legacy_files_and_roundtrips_settings() {
        let dir = temp_dir("legacy");
        std::fs::create_dir_all(&dir).expect("create dir");
        std::fs::write(dir.join(LEGACY_SETTINGS_FILE), r#"{"enabled":false}"#).expect("settings");
        std::fs::write(dir.join(LEGACY_ACCOUNT_ACCESS_FILE), r#"{"disabledAccountKeys":["dsid:1"]}"#)
            .expect("account access");
        std::fs::write(dir.join(LEGACY_TOOL_ACCESS_FILE), r#"{"disabledTools":["cirrus_delete_photos"]}"#)
            .expect("tool access");

        assert!(!load_mcp_settings(&dir).enabled);
        assert!(load_account_access(&dir).disabled_account_keys.contains("dsid:1"));
        assert!(load_tool_access(&dir).disabled_tools.contains("cirrus_delete_photos"));

        save_mcp_settings(&dir, true).expect("save mcp");
        let mut account_access = load_account_access(&dir);
        account_access.disabled_account_keys.remove("dsid:1");
        save_account_access(&dir, &account_access).expect("save accounts");
        let mut tool_access = load_tool_access(&dir);
        tool_access.disabled_tools.clear();
        save_tool_access(&dir, &tool_access).expect("save tools");

        assert!(load_mcp_settings(&dir).enabled);
        assert!(load_account_access(&dir).disabled_account_keys.is_empty());
        assert!(load_tool_access(&dir).disabled_tools.is_empty());
        assert!(!dir.join(LEGACY_SETTINGS_FILE).exists());
        assert!(!dir.join(LEGACY_ACCOUNT_ACCESS_FILE).exists());
        assert!(!dir.join(LEGACY_TOOL_ACCESS_FILE).exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
