use std::sync::Arc;

use tauri::State;

use cirrus_core::models::{NoteDetailResult, NoteMutationResult, NotesListResult};
use cirrus_core::services::notes;
use cirrus_core::{AppResult, SessionManager};

use crate::state::AppState;

type SharedState<'a> = State<'a, Arc<AppState>>;

fn sessions<'a, 'r>(state: &'a State<'r, Arc<AppState>>) -> &'a SessionManager {
    &state.inner().sessions
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotesListRequest {
    pub session_id: String,
    #[serde(default)]
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteDetailRequest {
    pub session_id: String,
    pub note_id: String,
}

#[tauri::command]
pub async fn notes_list(state: SharedState<'_>, payload: NotesListRequest) -> AppResult<NotesListResult> {
    Ok(notes::list(sessions(&state), &payload.session_id, payload.cursor).await)
}

#[tauri::command]
pub async fn notes_get(state: SharedState<'_>, payload: NoteDetailRequest) -> AppResult<NoteDetailResult> {
    Ok(notes::detail(sessions(&state), &payload.session_id, &payload.note_id).await)
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteCreateRequest {
    pub session_id: String,
    #[serde(default)]
    pub folder_id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub body: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteUpdateRequest {
    pub session_id: String,
    pub note_id: String,
    pub title: String,
    #[serde(default)]
    pub body: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteDeleteRequest {
    pub session_id: String,
    pub note_id: String,
}

#[tauri::command]
pub async fn notes_create(state: SharedState<'_>, payload: NoteCreateRequest) -> AppResult<NoteMutationResult> {
    Ok(notes::create(sessions(&state), &payload.session_id, payload.folder_id, &payload.title, &payload.body).await)
}

#[tauri::command]
pub async fn notes_update(state: SharedState<'_>, payload: NoteUpdateRequest) -> AppResult<NoteMutationResult> {
    Ok(notes::update(sessions(&state), &payload.session_id, &payload.note_id, &payload.title, &payload.body).await)
}

#[tauri::command]
pub async fn notes_delete(state: SharedState<'_>, payload: NoteDeleteRequest) -> AppResult<NoteMutationResult> {
    Ok(notes::delete(sessions(&state), &payload.session_id, &payload.note_id).await)
}
