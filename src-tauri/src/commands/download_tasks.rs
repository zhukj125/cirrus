use std::sync::Arc;

use serde_json::Value;
use tauri::State;

use cirrus_core::AppResult;

use crate::download_tasks::LoadedDownloadTasks;
use crate::state::AppState;

type SharedState<'a> = State<'a, Arc<AppState>>;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadTasksLoadRequest {
    pub account_key: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadTasksSaveRequest {
    pub account_key: String,
    pub tasks: Vec<Value>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadTasksRekeyRequest {
    pub from_key: String,
    pub to_key: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadTasksDeleteRequest {
    pub account_key: String,
}

#[tauri::command]
pub async fn download_tasks_load(
    state: SharedState<'_>,
    payload: DownloadTasksLoadRequest,
) -> AppResult<LoadedDownloadTasks> {
    state.inner().download_tasks.load(&payload.account_key).await
}

#[tauri::command]
pub async fn download_tasks_save(state: SharedState<'_>, payload: DownloadTasksSaveRequest) -> AppResult<()> {
    state.inner().download_tasks.save(&payload.account_key, payload.tasks).await
}

#[tauri::command]
pub async fn download_tasks_rekey(state: SharedState<'_>, payload: DownloadTasksRekeyRequest) -> AppResult<()> {
    state.inner().download_tasks.rekey(&payload.from_key, &payload.to_key).await
}

#[tauri::command]
pub async fn download_tasks_delete_account(
    state: SharedState<'_>,
    payload: DownloadTasksDeleteRequest,
) -> AppResult<()> {
    state.inner().download_tasks.delete_account(&payload.account_key).await
}
