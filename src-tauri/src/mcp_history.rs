use std::path::{Path, PathBuf};
use std::sync::Mutex;

use cirrus_core::{database_path, open_database, AppError, AppResult};
use serde::Serialize;
use serde_json::{json, Value};

const MAX_HISTORY_ENTRIES: i64 = 1_000;
const MAX_REQUEST_BYTES: usize = 8 * 1024;
const MAX_RESPONSE_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpCallHistoryEntry {
    pub id: String,
    pub tool_name: String,
    pub account_key: Option<String>,
    pub client_name: Option<String>,
    pub client_version: Option<String>,
    pub started_at: i64,
    pub duration_ms: i64,
    pub success: bool,
    pub error: Option<String>,
    pub request_json: Option<String>,
    pub response_json: Option<String>,
}

pub struct McpHistoryRepository {
    database: Option<PathBuf>,
    write_lock: Mutex<()>,
}

impl McpHistoryRepository {
    pub fn new(data_dir: Option<&Path>) -> Self {
        Self { database: data_dir.map(database_path), write_lock: Mutex::new(()) }
    }

    pub fn record(&self, mut entry: McpCallHistoryEntry) -> AppResult<()> {
        let Some(path) = &self.database else {
            return Ok(());
        };
        let _guard = self.write_lock.lock().unwrap_or_else(|error| error.into_inner());
        let connection = open_database(path)?;
        if let Some(error) = entry.error.as_mut() {
            *error = truncate_chars(error, 2_000);
        }
        entry.request_json = bounded_history_json(
            entry.request_json.as_deref().and_then(|value| serde_json::from_str(value).ok()),
            MAX_REQUEST_BYTES,
        );
        entry.response_json = bounded_history_json(
            entry.response_json.as_deref().and_then(|value| serde_json::from_str(value).ok()),
            MAX_RESPONSE_BYTES,
        );
        connection.execute(
            "INSERT INTO mcp_call_history (id, tool_name, account_key, client_name, client_version, started_at, duration_ms, success, error, request_json, response_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                entry.id,
                entry.tool_name,
                entry.account_key,
                entry.client_name,
                entry.client_version,
                entry.started_at,
                entry.duration_ms,
                i64::from(entry.success),
                entry.error,
                entry.request_json,
                entry.response_json,
            ],
        )?;
        connection.execute(
            "DELETE FROM mcp_call_history WHERE id NOT IN (SELECT id FROM mcp_call_history ORDER BY started_at DESC, id DESC LIMIT ?1)",
            [MAX_HISTORY_ENTRIES],
        )?;
        Ok(())
    }

    pub fn list(&self, limit: u32) -> AppResult<Vec<McpCallHistoryEntry>> {
        let Some(path) = &self.database else {
            return Ok(Vec::new());
        };
        let connection = open_database(path)?;
        let limit = i64::from(limit.clamp(1, 500));
        let mut statement = connection.prepare(
            "SELECT id, tool_name, account_key, client_name, client_version, started_at, duration_ms, success, error, request_json, response_json FROM mcp_call_history ORDER BY started_at DESC, id DESC LIMIT ?1",
        )?;
        let rows = statement.query_map([limit], |row| {
            Ok(McpCallHistoryEntry {
                id: row.get(0)?,
                tool_name: row.get(1)?,
                account_key: row.get(2)?,
                client_name: row.get(3)?,
                client_version: row.get(4)?,
                started_at: row.get(5)?,
                duration_ms: row.get(6)?,
                success: row.get::<_, i64>(7)? != 0,
                error: row.get(8)?,
                request_json: row.get(9)?,
                response_json: row.get(10)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
    }

    pub fn clear(&self) -> AppResult<()> {
        let Some(path) = &self.database else {
            return Ok(());
        };
        let _guard = self.write_lock.lock().unwrap_or_else(|error| error.into_inner());
        open_database(path)?.execute("DELETE FROM mcp_call_history", [])?;
        Ok(())
    }
}

pub fn bounded_history_json(value: Option<Value>, limit: usize) -> Option<String> {
    let value = value?;
    let sanitized = sanitize_history_value(&value, 0).to_string();
    if sanitized.len() > limit {
        Some(json!({ "truncated": true, "summary": "Payload exceeded the history size limit" }).to_string())
    } else {
        Some(sanitized)
    }
}

fn sanitize_history_value(value: &Value, depth: usize) -> Value {
    if depth > 12 {
        return json!("[depth limit]");
    }
    match value {
        Value::Object(fields) => {
            let media_block =
                fields.get("type").and_then(Value::as_str).is_some_and(|kind| kind == "image" || kind == "audio");
            Value::Object(
                fields
                    .iter()
                    .take(64)
                    .map(|(key, value)| {
                        let lower = key.to_ascii_lowercase();
                        let redacted = (media_block && key == "data")
                            || [
                                "password",
                                "passwd",
                                "secret",
                                "token",
                                "credential",
                                "authorization",
                                "private_key",
                                "apikey",
                                "api_key",
                                "deviceimagedata",
                                "base64",
                            ]
                            .iter()
                            .any(|part| lower.contains(part));
                        (
                            truncate_chars(key, 128),
                            if redacted { json!("[redacted]") } else { sanitize_history_value(value, depth + 1) },
                        )
                    })
                    .collect(),
            )
        }
        Value::Array(values) => {
            Value::Array(values.iter().take(32).map(|value| sanitize_history_value(value, depth + 1)).collect())
        }
        Value::String(value) => json!(truncate_chars(value, 1_024)),
        value => value.clone(),
    }
}

fn truncate_chars(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_is_persisted_and_sensitive_payloads_are_redacted() {
        let dir = std::env::temp_dir().join(format!("cirrus-mcp-history-{}", uuid::Uuid::new_v4()));
        let repository = McpHistoryRepository::new(Some(&dir));
        repository
            .record(McpCallHistoryEntry {
                id: "one".to_string(),
                tool_name: "cirrus_list_devices".to_string(),
                account_key: Some("dsid:1".to_string()),
                client_name: Some("Test client".to_string()),
                client_version: Some("1.0".to_string()),
                started_at: 1,
                duration_ms: 12,
                success: true,
                error: None,
                request_json: Some(json!({ "accountKey": "dsid:1", "token": "secret" }).to_string()),
                response_json: Some(
                    json!({
                        "content": [{ "type": "image", "data": "large-image-base64", "mimeType": "image/png" }],
                        "structuredContent": { "deviceImageDataUrl": "data:image/png;base64,large", "name": "Phone" }
                    })
                    .to_string(),
                ),
            })
            .expect("record");

        let entries = repository.list(20).expect("list");
        assert_eq!(entries.len(), 1);
        assert!(!entries[0].request_json.as_deref().unwrap().contains("secret"));
        assert!(!entries[0].response_json.as_deref().unwrap().contains("base64"));
        assert!(!entries[0].response_json.as_deref().unwrap().contains("large-image"));
        repository.clear().expect("clear");
        assert!(repository.list(20).expect("list").is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
