use std::sync::Arc;

use tauri::State;

use cirrus_core::models::{
    CalendarEventDeleteResult, CalendarEventInput, CalendarEventMutationResult, CalendarLoadResult,
};
use cirrus_core::services::calendar;
use cirrus_core::{AppResult, SessionManager};

use crate::state::AppState;

type SharedState<'a> = State<'a, Arc<AppState>>;

fn sessions<'a, 'r>(state: &'a State<'r, Arc<AppState>>) -> &'a SessionManager {
    &state.inner().sessions
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarLoadRequest {
    pub session_id: String,
    pub start_date: String,
    pub end_date: String,
    pub timezone: String,
    pub locale: String,
}

#[tauri::command]
pub async fn calendar_load(state: SharedState<'_>, payload: CalendarLoadRequest) -> AppResult<CalendarLoadResult> {
    Ok(calendar::load(
        sessions(&state),
        &payload.session_id,
        &payload.start_date,
        &payload.end_date,
        &payload.timezone,
        &payload.locale,
    )
    .await)
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarSaveRequest {
    pub session_id: String,
    pub event: CalendarEventInput,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarDeleteRequest {
    pub session_id: String,
    pub event_id: String,
    pub calendar_id: String,
    pub start: String,
    pub end: String,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub locale: Option<String>,
}

#[tauri::command]
pub async fn calendar_save(
    state: SharedState<'_>,
    payload: CalendarSaveRequest,
) -> AppResult<CalendarEventMutationResult> {
    Ok(calendar::save_event(sessions(&state), &payload.session_id, payload.event).await)
}

#[tauri::command]
pub async fn calendar_delete(
    state: SharedState<'_>,
    payload: CalendarDeleteRequest,
) -> AppResult<CalendarEventDeleteResult> {
    Ok(calendar::delete_event(
        sessions(&state),
        &payload.session_id,
        &payload.event_id,
        &payload.calendar_id,
        &payload.start,
        &payload.end,
        payload.timezone.as_deref(),
        payload.locale.as_deref(),
    )
    .await)
}
