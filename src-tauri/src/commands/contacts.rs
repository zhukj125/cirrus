use std::sync::Arc;

use tauri::State;

use cirrus_core::models::{AccountAvatar, ContactsListResult};
use cirrus_core::services::{avatar, contacts};
use cirrus_core::{AppResult, SessionManager};

use crate::state::AppState;

type SharedState<'a> = State<'a, Arc<AppState>>;

fn sessions<'a, 'r>(state: &'a State<'r, Arc<AppState>>) -> &'a SessionManager {
    &state.inner().sessions
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactsListRequest {
    pub session_id: String,
    #[serde(default)]
    pub locale: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactPhotoRequest {
    pub session_id: String,
    pub url: String,
}

#[tauri::command]
pub async fn contact_photo(state: SharedState<'_>, payload: ContactPhotoRequest) -> AppResult<AccountAvatar> {
    avatar::fetch_url(sessions(&state), &payload.session_id, &payload.url).await
}
#[tauri::command]
pub async fn contacts_list(state: SharedState<'_>, payload: ContactsListRequest) -> AppResult<ContactsListResult> {
    Ok(contacts::list(sessions(&state), &payload.session_id, &payload.locale).await)
}
