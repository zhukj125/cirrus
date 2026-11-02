use std::path::PathBuf;
use std::sync::Arc;

use tauri::State;

use cirrus_core::models::DriveListing;
use cirrus_core::services::drive;
use cirrus_core::{AppError, AppResult};

use crate::state::AppState;

type SharedState<'a> = State<'a, Arc<AppState>>;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveListRequest {
    pub session_id: String,
    #[serde(default)]
    pub folder_id: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveDownloadRequest {
    pub session_id: String,
    pub url: String,
    pub target_path: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedDriveFile {
    pub path: String,
    pub file_name: String,
    pub size_bytes: u64,
}

#[tauri::command]
pub async fn drive_list(state: SharedState<'_>, payload: DriveListRequest) -> AppResult<DriveListing> {
    Ok(drive::list(&state.inner().sessions, &payload.session_id, payload.folder_id).await)
}

#[tauri::command]
pub async fn drive_download(state: SharedState<'_>, payload: DriveDownloadRequest) -> AppResult<SavedDriveFile> {
    let target = PathBuf::from(payload.target_path.trim());
    if !target.is_absolute() {
        return Err(AppError::other("Choose an absolute download path"));
    }
    let downloaded =
        drive::download(&state.inner().sessions, &payload.session_id, &payload.url, payload.name.as_deref(), &target)
            .await?;
    let path = downloaded.path.to_string_lossy().to_string();
    log::info!("[DRIVE] downloaded {} ({} bytes) -> {}", downloaded.file_name, downloaded.size_bytes, path);
    Ok(SavedDriveFile { path, file_name: downloaded.file_name, size_bytes: downloaded.size_bytes })
}
