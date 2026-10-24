use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::Mutex;

use cirrus_core::{database_path, now_millis, open_database, AppError, AppResult};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedDownloadTasks {
    pub tasks: Vec<Value>,
    pub persistence_enabled: bool,
    pub writable: bool,
    pub warning: Option<String>,
}

pub struct DownloadTaskRepository {
    database: Option<PathBuf>,
    lock: Arc<Mutex<()>>,
}

impl DownloadTaskRepository {
    pub fn new(data_dir: Option<&Path>) -> Self {
        Self { database: data_dir.map(database_path), lock: Arc::new(Mutex::new(())) }
    }

    pub fn persistence_enabled(&self) -> bool {
        self.database.is_some()
    }

    pub async fn load(&self, account_key: &str) -> AppResult<LoadedDownloadTasks> {
        validate_account_key(account_key)?;
        let Some(database) = &self.database else {
            return Ok(LoadedDownloadTasks {
                tasks: Vec::new(),
                persistence_enabled: false,
                writable: false,
                warning: Some(
                    "App data directory is unavailable. Download tasks will only be kept for this run.".to_string(),
                ),
            });
        };

        let _guard = self.lock.lock().await;
        let connection = open_database(database)?;
        let tasks_json = connection
            .query_row("SELECT tasks_json FROM download_tasks WHERE account_key = ?1", [account_key], |row| {
                row.get::<_, String>(0)
            })
            .optional()?;

        let Some(tasks_json) = tasks_json else {
            return Ok(LoadedDownloadTasks {
                tasks: Vec::new(),
                persistence_enabled: true,
                writable: true,
                warning: None,
            });
        };

        match serde_json::from_str::<Vec<Value>>(&tasks_json) {
            Ok(tasks) => Ok(LoadedDownloadTasks { tasks, persistence_enabled: true, writable: true, warning: None }),
            Err(error) => {
                log::warn!("[DOWNLOADS] stored task list is invalid: {error}");
                Ok(LoadedDownloadTasks {
                    tasks: Vec::new(),
                    persistence_enabled: true,
                    writable: true,
                    warning: Some("Stored download tasks are invalid and were not loaded.".to_string()),
                })
            }
        }
    }

    pub async fn save(&self, account_key: &str, tasks: Vec<Value>) -> AppResult<()> {
        validate_account_key(account_key)?;
        let Some(database) = &self.database else {
            return Err(AppError::other("App data directory is unavailable; download tasks cannot be saved"));
        };

        let _guard = self.lock.lock().await;
        let connection = open_database(database)?;
        save_tasks(&connection, account_key, &tasks)
    }

    pub async fn rekey(&self, from_key: &str, to_key: &str) -> AppResult<()> {
        validate_account_key(from_key)?;
        validate_account_key(to_key)?;
        if from_key == to_key {
            return Ok(());
        }
        let Some(database) = &self.database else {
            return Ok(());
        };

        let _guard = self.lock.lock().await;
        let mut connection = open_database(database)?;
        let source_json = connection
            .query_row("SELECT tasks_json FROM download_tasks WHERE account_key = ?1", [from_key], |row| {
                row.get::<_, String>(0)
            })
            .optional()?;
        let Some(source_json) = source_json else {
            return Ok(());
        };

        let mut tasks = serde_json::from_str::<Vec<Value>>(&source_json)?;
        let target_json = connection
            .query_row("SELECT tasks_json FROM download_tasks WHERE account_key = ?1", [to_key], |row| {
                row.get::<_, String>(0)
            })
            .optional()?;
        if let Some(target_json) = target_json {
            tasks.extend(serde_json::from_str::<Vec<Value>>(&target_json)?);
        }

        let transaction = connection.transaction()?;
        transaction.execute("DELETE FROM download_tasks WHERE account_key = ?1", [from_key])?;
        let tasks_json = serde_json::to_string(&tasks)?;
        transaction.execute(
            "INSERT OR REPLACE INTO download_tasks (account_key, tasks_json, updated_at) VALUES (?1, ?2, ?3)",
            params![to_key, tasks_json, now_millis()],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub async fn delete_account(&self, account_key: &str) -> AppResult<()> {
        validate_account_key(account_key)?;
        let Some(database) = &self.database else {
            return Ok(());
        };

        let _guard = self.lock.lock().await;
        let connection = open_database(database)?;
        connection.execute("DELETE FROM download_tasks WHERE account_key = ?1", [account_key])?;
        Ok(())
    }
}

fn validate_account_key(account_key: &str) -> AppResult<()> {
    if account_key.trim().is_empty() {
        return Err(AppError::other("Download task is missing an account identifier"));
    }
    Ok(())
}

fn save_tasks(connection: &Connection, account_key: &str, tasks: &[Value]) -> AppResult<()> {
    let tasks_json = serde_json::to_string(tasks)?;
    connection.execute(
        "INSERT OR REPLACE INTO download_tasks (account_key, tasks_json, updated_at) VALUES (?1, ?2, ?3)",
        params![account_key, tasks_json, now_millis()],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIR: AtomicU64 = AtomicU64::new(1);

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "cirrus-download-tasks-{label}-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn repo(dir: &Path) -> DownloadTaskRepository {
        DownloadTaskRepository::new(Some(dir))
    }

    #[tokio::test]
    async fn missing_database_loads_empty_and_save_roundtrips_by_account() {
        let dir = temp_dir("roundtrip");
        let store = repo(&dir);
        let empty = store.load("dsid:1").await.expect("load empty");
        assert!(empty.persistence_enabled && empty.writable);
        assert!(empty.tasks.is_empty());
        assert!(empty.warning.is_none());

        let a = vec![json!({"id": "a", "title": "A"})];
        let b = vec![json!({"id": "b", "title": "B"})];
        store.save("dsid:1", a.clone()).await.expect("save a");
        store.save("dsid:2", b.clone()).await.expect("save b");
        let reopened = repo(&dir);
        assert_eq!(reopened.load("dsid:1").await.expect("load a").tasks, a);
        assert_eq!(reopened.load("dsid:2").await.expect("load b").tasks, b);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn rekey_merges_tasks_and_delete_only_touches_target_partition() {
        let dir = temp_dir("keys");
        let store = repo(&dir);
        store.save("apple:test@example.com", vec![json!({"id": "a"})]).await.expect("save old");
        store.save("dsid:1", vec![json!({"id": "b"})]).await.expect("save target");
        store.save("dsid:2", vec![json!({"id": "c"})]).await.expect("save other");

        store.rekey("apple:test@example.com", "dsid:1").await.expect("rekey");
        assert!(store.load("apple:test@example.com").await.expect("load old").tasks.is_empty());
        assert_eq!(store.load("dsid:1").await.expect("load target").tasks.len(), 2);

        store.delete_account("dsid:1").await.expect("delete one");
        assert!(store.load("dsid:1").await.expect("load deleted").tasks.is_empty());
        assert_eq!(store.load("dsid:2").await.expect("load other").tasks.len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn disabled_repository_never_writes() {
        let store = DownloadTaskRepository::new(None);
        assert!(!store.persistence_enabled());
    }
}
