use std::sync::Arc;

use tauri::State;

use cirrus_core::models::CalendarLoadResult;
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
