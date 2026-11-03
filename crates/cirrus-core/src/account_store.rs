use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};

use crate::{database_path, now_millis, open_database, AppError, AppResult};

const ACTIVE_ACCOUNT_KEY: &str = "active_account_key";
const ACCOUNT_STORE_REVISION_KEY: &str = "account_store_revision";
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
    #[serde(default)]
    pub revision: i64,
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
        let mut connection = open_database(database)?;
        let transaction = connection.transaction()?;
        let mut statement = transaction.prepare(
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
        let mut avatar_statement = transaction.prepare("SELECT account_key, data_url FROM account_avatars")?;
        for row in avatar_statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))? {
            let (key, data_url) = row?;
            if valid_avatar(&data_url) {
                avatars.insert(key, data_url);
            }
        }

        let active_account_key = transaction
            .query_row("SELECT value_json FROM app_state WHERE key = ?1", [ACTIVE_ACCOUNT_KEY], |row| {
                row.get::<_, String>(0)
            })
            .optional()?
            .and_then(|value| serde_json::from_str::<String>(&value).ok())
            .filter(|key| accounts.iter().any(|account| account.key == *key));
        let revision = transaction
            .query_row("SELECT value_json FROM app_state WHERE key = ?1", [ACCOUNT_STORE_REVISION_KEY], |row| {
                row.get::<_, String>(0)
            })
            .optional()?
            .and_then(|value| serde_json::from_str::<i64>(&value).ok())
            .unwrap_or_default();
        drop(avatar_statement);
        drop(statement);
        transaction.commit()?;

        Ok(AccountStoreSnapshot { accounts, active_account_key, avatars, revision })
    }

    pub fn clear_sessions(&self, account_key: &str) -> AppResult<()> {
        let Some(database) = &self.database else {
            return Err(AppError::other("App data directory is unavailable; account sessions cannot be updated"));
        };
        let mut connection = open_database(database)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "UPDATE accounts SET photos_session_id = NULL, devices_session_id = NULL, last_known_status = 'expired', last_used_at = ?2 WHERE key = ?1",
            params![account_key, now_millis()],
        )?;
        bump_revision(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn save(&self, snapshot: &AccountStoreSnapshot) -> AppResult<i64> {
        let Some(database) = &self.database else {
            return Err(AppError::other("App data directory is unavailable; accounts cannot be saved"));
        };

        validate_accounts(&snapshot.accounts)?;
        let mut connection = open_database(database)?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current_revision = revision(&transaction)?;
        if snapshot.revision != current_revision {
            return Err(AppError::other(format!(
                "ACCOUNT_STORE_CONFLICT: expected revision {}, current revision {}",
                snapshot.revision, current_revision
            )));
        }
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
        let next_revision = bump_revision(&transaction)?;
        transaction.commit()?;
        Ok(next_revision)
    }
}

fn revision(connection: &rusqlite::Connection) -> AppResult<i64> {
    Ok(connection
        .query_row("SELECT value_json FROM app_state WHERE key = ?1", [ACCOUNT_STORE_REVISION_KEY], |row| {
            row.get::<_, String>(0)
        })
        .optional()?
        .and_then(|value| serde_json::from_str::<i64>(&value).ok())
        .unwrap_or_default())
}

fn bump_revision(connection: &rusqlite::Connection) -> AppResult<i64> {
    let revision = revision(connection)?.saturating_add(1);
    connection.execute(
        "INSERT OR REPLACE INTO app_state (key, value_json) VALUES (?1, ?2)",
        params![ACCOUNT_STORE_REVISION_KEY, serde_json::to_string(&revision)?],
    )?;
    Ok(revision)
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
        std::env::temp_dir().join(format!("cirrus-account-store-{}", uuid::Uuid::new_v4()))
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
    fn revision_conflict_rejects_a_stale_snapshot() {
        let dir = temp_dir();
        let repository = AccountStoreRepository::new(Some(&dir));
        let initial = AccountStoreSnapshot {
            accounts: vec![account("dsid:123")],
            active_account_key: Some("dsid:123".to_string()),
            avatars: Default::default(),
            revision: 0,
        };
        let revision = repository.save(&initial).expect("seed snapshot");
        assert_eq!(revision, 1);

        let mut first = repository.load().expect("load first snapshot");
        first.accounts[0].first_name = Some("Updated".to_string());
        assert_eq!(repository.save(&first).expect("first update"), 2);

        let mut stale = first.clone();
        stale.accounts[0].first_name = Some("Stale".to_string());
        let error = repository.save(&stale).expect_err("stale snapshot must be rejected");
        assert!(error.to_string().contains("ACCOUNT_STORE_CONFLICT"));
        assert_eq!(repository.load().expect("load final snapshot").accounts[0].first_name.as_deref(), Some("Updated"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn concurrent_session_clear_operations_are_serialized() {
        let dir = temp_dir();
        let initial = AccountStoreSnapshot {
            accounts: vec![account("dsid:123")],
            active_account_key: Some("dsid:123".to_string()),
            avatars: Default::default(),
            revision: 0,
        };
        AccountStoreRepository::new(Some(&dir)).save(&initial).expect("seed snapshot");

        let first = AccountStoreRepository::new(Some(&dir));
        let second = AccountStoreRepository::new(Some(&dir));
        let first_thread = std::thread::spawn(move || {
            for _ in 0..20 {
                first.clear_sessions("dsid:123").expect("first concurrent clear");
            }
        });
        let second_thread = std::thread::spawn(move || {
            for _ in 0..20 {
                second.clear_sessions("dsid:123").expect("second concurrent clear");
            }
        });
        first_thread.join().expect("first thread");
        second_thread.join().expect("second thread");
        let loaded = AccountStoreRepository::new(Some(&dir)).load().expect("load final snapshot");
        assert_eq!(loaded.accounts.len(), 1);
        assert_eq!(loaded.revision, 41);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn account_load_returns_rows_and_revision_from_the_same_snapshot() {
        let dir = temp_dir();
        let repository = AccountStoreRepository::new(Some(&dir));
        repository
            .save(&AccountStoreSnapshot {
                accounts: vec![account("dsid:one")],
                active_account_key: Some("dsid:one".to_string()),
                avatars: Default::default(),
                revision: 0,
            })
            .expect("seed account");

        let writer = AccountStoreRepository::new(Some(&dir));
        let write_thread = std::thread::spawn(move || {
            for _ in 0..40 {
                let mut snapshot = writer.load().expect("writer load");
                snapshot.accounts = if snapshot.accounts.len() == 1 {
                    vec![account("dsid:one"), account("dsid:two")]
                } else {
                    vec![account("dsid:one")]
                };
                snapshot.active_account_key = Some("dsid:one".to_string());
                writer.save(&snapshot).expect("writer save");
            }
        });
        let reader = AccountStoreRepository::new(Some(&dir));
        for _ in 0..200 {
            let snapshot = reader.load().expect("reader load");
            let expected_even_revision = snapshot.accounts.len() == 2;
            assert_eq!(
                snapshot.revision % 2 == 0,
                expected_even_revision,
                "accounts {:?} and revision {} came from different snapshots",
                snapshot.accounts.iter().map(|account| account.key.as_str()).collect::<Vec<_>>(),
                snapshot.revision
            );
        }
        write_thread.join().expect("writer thread");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn account_avatar_and_active_key_roundtrip() {
        let dir = temp_dir();
        let repository = AccountStoreRepository::new(Some(&dir));
        let snapshot = AccountStoreSnapshot {
            accounts: vec![account("dsid:123")],
            active_account_key: Some("dsid:123".to_string()),
            avatars: BTreeMap::from([("dsid:123".to_string(), "data:image/jpeg;base64,avatar".to_string())]),
            revision: 0,
        };
        let revision = repository.save(&snapshot).expect("save snapshot");
        assert_eq!(revision, 1);
        let loaded = repository.load().expect("load snapshot");
        assert_eq!(loaded.accounts, snapshot.accounts);
        assert_eq!(loaded.active_account_key.as_deref(), Some("dsid:123"));
        assert_eq!(loaded.avatars, snapshot.avatars);
        assert_eq!(loaded.revision, 1);

        repository.clear_sessions("dsid:123").expect("clear sessions");
        let signed_out = repository.load().expect("reload snapshot");
        assert!(signed_out.accounts[0].photos_session_id.is_none());
        assert!(signed_out.accounts[0].devices_session_id.is_none());
        assert_eq!(signed_out.accounts[0].last_known_status, "expired");
        assert_eq!(signed_out.revision, 2);
        let _ = std::fs::remove_dir_all(dir);
    }
}
