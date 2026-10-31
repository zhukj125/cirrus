use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

use crate::AppResult;

pub const DATABASE_FILE_NAME: &str = "cirrus.db";
const SCHEMA_VERSION: i64 = 2;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS accounts (
    key TEXT PRIMARY KEY NOT NULL,
    apple_id TEXT NOT NULL,
    dsid TEXT,
    first_name TEXT,
    last_name TEXT,
    photos_session_id TEXT,
    devices_session_id TEXT,
    last_used_at INTEGER NOT NULL DEFAULT 0,
    last_known_status TEXT NOT NULL DEFAULT 'unknown'
);

CREATE TABLE IF NOT EXISTS account_avatars (
    account_key TEXT PRIMARY KEY NOT NULL REFERENCES accounts(key) ON DELETE CASCADE,
    data_url TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS app_state (
    key TEXT PRIMARY KEY NOT NULL,
    value_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS session_secrets (
    session_id TEXT PRIMARY KEY NOT NULL,
    encrypted BLOB NOT NULL,
    last_access INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS session_secrets_last_access ON session_secrets(last_access);

CREATE TABLE IF NOT EXISTS trusted_browsers (
    apple_id_key TEXT PRIMARY KEY NOT NULL,
    encrypted BLOB NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS download_tasks (
    account_key TEXT PRIMARY KEY NOT NULL,
    tasks_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
"#;

pub fn database_path(data_dir: &Path) -> PathBuf {
    data_dir.join(DATABASE_FILE_NAME)
}

pub fn open_database(path: &Path) -> AppResult<Connection> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let connection = Connection::open(path)?;
    connection.busy_timeout(Duration::from_secs(5))?;
    connection.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA foreign_keys = ON;
        "#,
    )?;
    connection.execute_batch(SCHEMA)?;
    connection.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    Ok(connection)
}

pub fn load_app_state_json<T: DeserializeOwned>(data_dir: &Path, key: &str) -> AppResult<Option<T>> {
    let connection = open_database(&database_path(data_dir))?;
    let value = connection
        .query_row("SELECT value_json FROM app_state WHERE key = ?1", [key], |row| row.get::<_, String>(0))
        .optional()?;
    value
        .map(|value| serde_json::from_str::<T>(&value).map_err(|error| crate::AppError::other(error.to_string())))
        .transpose()
}

pub fn load_app_state_value(data_dir: &Path, key: &str) -> AppResult<Option<Value>> {
    load_app_state_json::<Value>(data_dir, key)
}

pub fn save_app_state_json<T: Serialize>(data_dir: &Path, key: &str, value: &T) -> AppResult<()> {
    let connection = open_database(&database_path(data_dir))?;
    let value = serde_json::to_string(value).map_err(|error| crate::AppError::other(error.to_string()))?;
    connection.execute(
        "INSERT INTO app_state (key, value_json) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json",
        [key, value.as_str()],
    )?;
    Ok(())
}

pub fn delete_app_state_value(data_dir: &Path, key: &str) -> AppResult<()> {
    let connection = open_database(&database_path(data_dir))?;
    connection.execute("DELETE FROM app_state WHERE key = ?1", [key])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn schema_initializes_idempotently() {
        let dir = std::env::temp_dir().join(format!("cirrus-database-{}", Uuid::new_v4()));
        let path = database_path(&dir);
        let connection = open_database(&path).expect("open database");
        drop(connection);

        let connection = open_database(&path).expect("reopen database");
        let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0)).expect("read version");
        assert_eq!(version, SCHEMA_VERSION);
        let tables: i64 = connection
            .query_row("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('accounts', 'account_avatars', 'app_state', 'session_secrets', 'trusted_browsers', 'download_tasks')", [], |row| row.get(0))
            .expect("count tables");
        assert_eq!(tables, 6);
        drop(connection);
        let _ = std::fs::remove_dir_all(dir);
    }
}
