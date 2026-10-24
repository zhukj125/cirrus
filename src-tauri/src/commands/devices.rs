use std::sync::Arc;

use tauri::State;

use cirrus_core::models::*;
use cirrus_core::services::devices;
use cirrus_core::AppResult;

use crate::state::AppState;

type SharedState<'a> = State<'a, Arc<AppState>>;

#[tauri::command]
pub async fn refresh_devices(state: SharedState<'_>, payload: SessionRequest) -> AppResult<DeviceInfoResult> {
    let Some(session_id) = payload.session_id.filter(|id| !id.trim().is_empty()) else {
        return Ok(DeviceInfoResult::error("Missing session ID. Please sign in again."));
    };
    match state.sessions.get(&session_id).await {
        Ok(handle) => Ok(devices::list_devices(&handle).await),
        Err(error) => Ok(DeviceInfoResult::error(error.to_string())),
    }
}

#[tauri::command]
pub async fn play_sound(state: SharedState<'_>, payload: PlaySoundRequest) -> AppResult<PlaySoundResult> {
    match state.sessions.get(&payload.session_id).await {
        Ok(handle) => Ok(devices::play_sound(&handle, &payload.device_id).await),
        Err(error) => Ok(PlaySoundResult::error(error.to_string())),
    }
}
