use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use cirrus_core::{database_path, now_millis, open_database, AppError, AppResult};

const ACTIVE_ACCOUNT_KEY: &str = "active_account_key";
const MAX_AVATARS: usize = 8;
const MAX_AVATAR_DATA_URL_LENGTH: usize = 256 * 1024;
const AVATAR_PREFIX: &str = "data:image/";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredAccount {
    pub key: String,
    pub apple_id: String,
    pub dsid: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub photos_session_id: Option<String>,
    pub devices_session_id: Option<String>,
    pub last_used_at: i64,
    pub last_known_status: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountStoreSnapshot {
    #[serde(default)]
    pub accounts: Vec<StoredAccount>,
    #[serde(default)]
    pub active_account_key: Option<String>,
    #[serde(default)]
    pub avatars: BTreeMap<String, String>,
}

pub struct AccountStoreRepository {
    database: Option<PathBuf>,
}

impl AccountStoreRepository {
    pub fn new(data_dir: Option<&Path>) -> Self {
        Self { database: data_dir.map(database_path) }
    }

    pub fn load(&self) -> AppResult<AccountStoreSnapshot> {
        let Some(database) = &self.database else {
            return Ok(AccountStoreSnapshot::default());
        };
        let connection = open_database(database)?;
        let mut statement = connection.prepare(
            "SELECT key, apple_id, dsid, first_name, last_name, photos_session_id, devices_session_id, last_used_at, last_known_status
             FROM accounts
             ORDER BY last_used_at DESC, lower(apple_id) ASC",
        )?;
        let accounts = statement
            .query_map([], |row| {
                Ok(StoredAccount {
                    key: row.get(0)?,
                    apple_id: row.get(1)?,
                    dsid: row.get(2)?,
                    first_name: row.get(3)?,
                    last_name: row.get(4)?,
                    photos_session_id: row.get(5)?,
                    devices_session_id: row.get(6)?,
                    last_used_at: row.get(7)?,
                    last_known_status: row.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut avatars = BTreeMap::new();
        let mut avatar_statement = connection.prepare("SELECT account_key, data_url FROM account_avatars")?;
        for row in avatar_statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))? {
            let (key, data_url) = row?;
            if valid_avatar(&data_url) {
                avatars.insert(key, data_url);
            }
        }

        let active_account_key = connection
            .query_row("SELECT value_json FROM app_state WHERE key = ?1", [ACTIVE_ACCOUNT_KEY], |row| {
                row.get::<_, String>(0)
            })
            .optional()?
            .and_then(|value| serde_json::from_str::<String>(&value).ok())
            .filter(|key| accounts.iter().any(|account| account.key == *key));

        Ok(AccountStoreSnapshot { accounts, active_account_key, avatars })
    }

    pub fn save(&self, snapshot: &AccountStoreSnapshot) -> AppResult<()> {
        let Some(database) = &self.database else {
            return Err(AppError::other("App data directory is unavailable; accounts cannot be saved"));
        };

        validate_accounts(&snapshot.accounts)?;
        let mut connection = open_database(database)?;
        let transaction = connection.transaction()?;
        transaction.execute("DELETE FROM account_avatars", [])?;
        transaction.execute("DELETE FROM accounts", [])?;

        for account in &snapshot.accounts {
            transaction.execute(
                "INSERT INTO accounts (key, apple_id, dsid, first_name, last_name, photos_session_id, devices_session_id, last_used_at, last_known_status)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    account.key,
                    account.apple_id,
                    account.dsid,
                    account.first_name,
                    account.last_name,
                    account.photos_session_id,
                    account.devices_session_id,
                    account.last_used_at,
                    account.last_known_status
                ],
            )?;
        }

        for (account_key, data_url) in snapshot.avatars.iter().take(MAX_AVATARS) {
            if !snapshot.accounts.iter().any(|account| account.key == *account_key) || !valid_avatar(data_url) {
                continue;
            }
            transaction.execute(
                "INSERT INTO account_avatars (account_key, data_url, updated_at) VALUES (?1, ?2, ?3)",
                params![account_key, data_url, now_millis()],
            )?;
        }

        let active_key = snapshot
            .active_account_key
            .as_ref()
            .filter(|key| snapshot.accounts.iter().any(|account| account.key == **key));
        if let Some(key) = active_key {
            let value = serde_json::to_string(key)?;
            transaction.execute(
                "INSERT OR REPLACE INTO app_state (key, value_json) VALUES (?1, ?2)",
                params![ACTIVE_ACCOUNT_KEY, value],
            )?;
        } else {
            transaction.execute("DELETE FROM app_state WHERE key = ?1", [ACTIVE_ACCOUNT_KEY])?;
        }
        transaction.commit()?;
        Ok(())
    }
}

fn validate_accounts(accounts: &[StoredAccount]) -> AppResult<()> {
    let mut keys = HashSet::with_capacity(accounts.len());
    for account in accounts {
        if account.key.trim().is_empty() || account.apple_id.trim().is_empty() {
            return Err(AppError::other("Account directory contains an invalid account"));
        }
        if !keys.insert(account.key.as_str()) {
            return Err(AppError::other("Account directory contains duplicate keys"));
        }
    }
    Ok(())
}

fn valid_avatar(data_url: &str) -> bool {
    data_url.len() <= MAX_AVATAR_DATA_URL_LENGTH && data_url.starts_with(AVATAR_PREFIX) && data_url.contains(";base64,")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!("cirrus-account-store-{}-{}", std::process::id(), now_millis()))
    }

    fn account(key: &str) -> StoredAccount {
        StoredAccount {
            key: key.to_string(),
            apple_id: "person@example.com".to_string(),
            dsid: Some("123".to_string()),
            first_name: Some("三".to_string()),
            last_name: Some("张".to_string()),
            photos_session_id: Some("session-a".to_string()),
            devices_session_id: Some("session-a".to_string()),
            last_used_at: 10,
            last_known_status: "valid".to_string(),
        }
    }

    #[test]
    fn account_avatar_and_active_key_roundtrip() {
        let dir = temp_dir();
        let repository = AccountStoreRepository::new(Some(&dir));
        let snapshot = AccountStoreSnapshot {
            accounts: vec![account("dsid:123")],
            active_account_key: Some("dsid:123".to_string()),
            avatars: BTreeMap::from([("dsid:123".to_string(), "data:image/jpeg;base64,avatar".to_string())]),
        };
        repository.save(&snapshot).expect("save snapshot");
        let loaded = repository.load().expect("load snapshot");
        assert_eq!(loaded.accounts, snapshot.accounts);
        assert_eq!(loaded.active_account_key.as_deref(), Some("dsid:123"));
        assert_eq!(loaded.avatars, snapshot.avatars);
        let _ = std::fs::remove_dir_all(dir);
    }
}
