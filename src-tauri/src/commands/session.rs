use std::sync::Arc;

use tauri::{AppHandle, State};

use cirrus_core::models::session_keys::PHOTOS_VERIFIED;
use cirrus_core::models::*;
use cirrus_core::services::avatar;
use cirrus_core::AppResult;

use crate::state::{AppState, RuntimeInfo};

type SharedState<'a> = State<'a, Arc<AppState>>;

#[tauri::command]
pub async fn runtime_info(state: SharedState<'_>) -> AppResult<RuntimeInfo> {
    Ok(state.runtime_info())
}

#[tauri::command]
pub fn tray_available(state: SharedState<'_>) -> bool {
    state.tray_available()
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub async fn session_state(state: SharedState<'_>, payload: SessionRequest) -> AppResult<SessionStateResponse> {
    let ttl_minutes = state.sessions.ttl_minutes();
    let invalid = SessionStateResponse {
        valid: false,
        verified: false,
        first_name: None,
        last_name: None,
        apple_id: None,
        dsid: None,
        ttl_minutes,
    };

    let Some(session_id) = payload.session_id.filter(|id| !id.trim().is_empty()) else {
        return Ok(invalid);
    };
    if !state.sessions.is_valid(&session_id).await {
        return Ok(invalid);
    }
    let Ok(handle) = state.sessions.get(&session_id).await else {
        return Ok(invalid);
    };
    let session = handle.lock().await;
    Ok(SessionStateResponse {
        valid: true,
        verified: session.is_flag_set(PHOTOS_VERIFIED),
        first_name: session.first_name().map(str::to_string),
        last_name: session.last_name().map(str::to_string),
        apple_id: Some(session.apple_id().to_string()),
        dsid: session.dsid().map(str::to_string),
        ttl_minutes,
    })
}

#[tauri::command]
pub async fn session_avatar(state: SharedState<'_>, payload: SessionRequest) -> AppResult<AccountAvatar> {
    let session_id = payload
        .session_id
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| cirrus_core::AppError::auth("Missing session ID. Please sign in again."))?;
    let handle = state.sessions.get(&session_id).await?;
    let result = avatar::fetch(&handle).await;
    if let Err(error) = &result {
        log::warn!("[AVATAR] avatar read failed: {error}");
    }
    result
}

#[tauri::command]
pub async fn session_logout(state: SharedState<'_>, payload: SessionRequest) -> AppResult<String> {
    if let Some(session_id) = payload.session_id.filter(|id| !id.trim().is_empty()) {
        state.sessions.remove(&session_id).await;
    }
    Ok(statuses::OK.to_string())
}
