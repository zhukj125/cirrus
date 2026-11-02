use std::sync::Arc;

use tauri::State;

use cirrus_core::models::{SessionRequest, StorageUsageResult};
use cirrus_core::services::storage;
use cirrus_core::AppResult;

use crate::state::AppState;

type SharedState<'a> = State<'a, Arc<AppState>>;

#[tauri::command]
pub async fn storage_usage(state: SharedState<'_>, payload: SessionRequest) -> AppResult<StorageUsageResult> {
    let Some(session_id) = payload.session_id.filter(|id| !id.trim().is_empty()) else {
        return Ok(StorageUsageResult::error("Missing session ID. Please sign in again."));
    };
    match state.sessions.get(&session_id).await {
        Ok(handle) => Ok(storage::get_usage(&handle).await),
        Err(error) => Ok(StorageUsageResult::error(error.to_string())),
    }
}
