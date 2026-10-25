use std::path::{Path, PathBuf};
use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;

use cirrus_core::models::*;
use cirrus_core::services::{media, photos};
use cirrus_core::{AppResult, SessionManager};

use crate::state::AppState;

type SharedState<'a> = State<'a, Arc<AppState>>;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenDownloadFolderRequest {
    pub path: String,
}

#[tauri::command]
pub fn open_download_folder(app: AppHandle, payload: OpenDownloadFolderRequest) -> AppResult<()> {
    let path = Path::new(payload.path.trim());
    if !path.is_absolute() || !path.is_dir() {
        return Err(cirrus_core::AppError::other("Download folder does not exist or is not accessible"));
    }
    app.opener()
        .open_path(path.to_string_lossy().into_owned(), None::<&str>)
        .map_err(|error| cirrus_core::AppError::other(format!("Could not open folder: {error}")))
}

fn sessions<'a, 'r>(state: &'a State<'r, Arc<AppState>>) -> &'a SessionManager {
    &state.inner().sessions
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePhotoRequest {
    pub url: String,

    #[serde(default)]
    pub name: Option<String>,

    pub target_path: String,

    #[serde(default)]
    pub transfer_id: Option<String>,

    #[serde(default)]
    pub partial_key: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadSpeedLimitRequest {
    pub bytes_per_sec: u64,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadCancelRequest {
    pub transfer_id: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedPhotoResult {
    pub path: String,
    pub size_bytes: u64,
    pub file_name: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgressEvent {
    pub transfer_id: String,
    pub file_name: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
}

#[tauri::command]
pub async fn photos_login(state: SharedState<'_>, payload: CredentialsRequest) -> AppResult<PhotosLoginResult> {
    Ok(photos::login(sessions(&state), &payload.apple_id, &payload.password, payload.china_mainland).await)
}

#[tauri::command]
pub async fn photos_verify_2fa(state: SharedState<'_>, payload: PhotosVerifyRequest) -> AppResult<PhotosLoginResult> {
    let result = photos::verify_2fa(sessions(&state), &payload.session_id, &payload.code).await;

    if result.status == statuses::OK {
        match state.sessions.get(&payload.session_id).await {
            Ok(handle) => {
                if let Err(error) = state.sessions.persist(&handle).await {
                    log::warn!("[PHOTOS] failed to persist session after 2FA; current sign-in remains valid: {error}");
                }
            }
            Err(error) => log::warn!("[PHOTOS] failed to read session after 2FA; skipping persistence: {error}"),
        }
    }
    Ok(result)
}

#[tauri::command]
pub async fn photos_send_code(state: SharedState<'_>, payload: SessionRequest) -> AppResult<PhotosLoginResult> {
    let session_id = payload.session_id.unwrap_or_default();
    Ok(photos::request_device_code(sessions(&state), &session_id).await)
}

#[tauri::command]
pub async fn photos_status(state: SharedState<'_>, payload: SessionRequest) -> AppResult<serde_json::Value> {
    let session_id = payload.session_id.unwrap_or_default();
    Ok(photos::status(sessions(&state), &session_id).await)
}

#[tauri::command]
pub async fn photos_check_approval(state: SharedState<'_>, payload: SessionRequest) -> AppResult<PhotosLoginResult> {
    let session_id = payload.session_id.unwrap_or_default();
    Ok(photos::check_approval(sessions(&state), &session_id).await)
}

#[tauri::command]
pub async fn photos_2fa_options(state: SharedState<'_>, payload: SessionRequest) -> AppResult<Photos2faOptions> {
    let session_id = payload.session_id.unwrap_or_default();
    Ok(photos::options_2fa(sessions(&state), &session_id).await)
}

#[tauri::command]
pub async fn photos_2fa_sms(state: SharedState<'_>, payload: PhotosPhoneRequest) -> AppResult<PhotosLoginResult> {
    Ok(photos::send_phone_code(sessions(&state), &payload.session_id, payload.phone_number_id).await)
}

#[tauri::command]
pub async fn photos_verify_2fa_phone(
    state: SharedState<'_>,
    payload: PhotosPhoneVerifyRequest,
) -> AppResult<PhotosLoginResult> {
    Ok(photos::verify_phone_2fa(sessions(&state), &payload.session_id, payload.phone_number_id, &payload.code).await)
}

#[tauri::command]
pub async fn photos_list(state: SharedState<'_>, payload: PhotosListRequest) -> AppResult<PhotosPage> {
    Ok(photos::list(sessions(&state), &payload.session_id, payload.cursor, payload.limit, payload.start_rank).await)
}

#[tauri::command]
pub async fn photos_total(state: SharedState<'_>, payload: SessionRequest) -> AppResult<LibraryTotal> {
    let session_id = payload.session_id.unwrap_or_default();
    let total = photos::library_total(sessions(&state), &session_id).await?;
    log::info!("[PHOTOS] library total: photos {} videos {} total {}", total.photos, total.videos, total.assets);
    Ok(total)
}

#[tauri::command]
pub async fn photos_asset_distribution(
    state: SharedState<'_>,
    payload: PhotosTimelineRequest,
) -> AppResult<PhotosTimelineDto> {
    let dto = photos::timeline(sessions(&state), &payload.session_id, photos::DEFAULT_TZ_OFFSET_SECONDS).await?;
    log::info!(
        "[PHOTOS] timeline histogram: buckets {} photos {} years {}",
        dto.distribution.buckets.len(),
        dto.distribution.total,
        dto.timeline.years.len()
    );
    Ok(dto)
}

#[tauri::command]
pub async fn photos_month_exact(state: SharedState<'_>, payload: PhotosMonthExactRequest) -> AppResult<MonthExactDto> {
    let exact = photos::month_exact(
        sessions(&state),
        &payload.session_id,
        payload.year,
        payload.month,
        photos::DEFAULT_TZ_OFFSET_SECONDS,
    )
    .await?;
    log::info!("[PHOTOS] exact monthly count: {} {} = {} photos", exact.year, exact.month, exact.count);
    Ok(MonthExactDto::from(&exact))
}

#[tauri::command]
pub async fn download_set_speed_limit(state: SharedState<'_>, payload: DownloadSpeedLimitRequest) -> AppResult<()> {
    state.inner().download_limiter.set_limit(payload.bytes_per_sec).await;
    log::info!("[MEDIA] global speed limit updated: {} bytes/sec", payload.bytes_per_sec);
    Ok(())
}

#[tauri::command]
pub async fn download_cancel_transfer(state: SharedState<'_>, payload: DownloadCancelRequest) -> AppResult<()> {
    state.inner().download_transfers.cancel(&payload.transfer_id).await;
    Ok(())
}

#[tauri::command]
pub async fn save_photo(
    state: SharedState<'_>,
    app: AppHandle,
    payload: SavePhotoRequest,
) -> AppResult<SavedPhotoResult> {
    let file_name = payload.name.clone().unwrap_or_default();
    let transfer_id = payload.transfer_id.clone();
    let cancel_flag = match transfer_id.as_ref() {
        Some(id) => Some(state.inner().download_transfers.register(id).await),
        None => None,
    };
    let limiter = &state.inner().download_limiter;
    let target = PathBuf::from(payload.target_path.trim());
    let absolute_target = if target.is_absolute() { target } else { std::env::current_dir()?.join(target) };
    let fetch_result = media::fetch_media_to_file_with_progress(
        &payload.url,
        payload.name.as_deref(),
        &absolute_target,
        payload.partial_key.as_deref(),
        Some(limiter),
        cancel_flag.as_deref(),
        |downloaded_bytes, total_bytes| {
            let Some(transfer_id) = transfer_id.as_ref() else {
                return;
            };
            let event = DownloadProgressEvent {
                transfer_id: transfer_id.clone(),
                file_name: file_name.clone(),
                downloaded_bytes,
                total_bytes,
            };
            if let Err(error) = app.emit("download-progress", event) {
                log::debug!("[MEDIA] failed to emit download progress: {error}");
            }
        },
    )
    .await;
    if let Some(id) = transfer_id.as_ref() {
        state.inner().download_transfers.finish(id).await;
    }
    let downloaded = fetch_result?;
    let destination = media::target_with_content_extension(&absolute_target, &downloaded.content_type);
    let saved_path = media::persist_downloaded_file(&downloaded.path, &destination)?;
    let saved = saved_path.to_string_lossy().to_string();
    let saved_name = saved_path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| downloaded.file_name.clone());
    log::info!(
        "[MEDIA] saved {} ({} bytes, {}) -> {}",
        downloaded.file_name,
        downloaded.size,
        downloaded.content_type,
        saved
    );
    Ok(SavedPhotoResult { path: saved, size_bytes: downloaded.size, file_name: saved_name })
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_event_serializes_without_sensitive_fields() {
        let value = serde_json::to_value(DownloadProgressEvent {
            transfer_id: "t1".to_string(),
            file_name: "IMG_0001.HEIC".to_string(),
            downloaded_bytes: 1024,
            total_bytes: Some(2048),
        })
        .expect("serialize progress");
        assert_eq!(value["transferId"], "t1");
        assert_eq!(value["downloadedBytes"], 1024);
        assert!(value.get("url").is_none());
        assert!(value.get("cookie").is_none());
        assert!(value.get("sessionToken").is_none());
    }
}
