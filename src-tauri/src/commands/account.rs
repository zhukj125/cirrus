use std::sync::Arc;

use tauri::State;

use cirrus_core::AppResult;

use crate::account_store::{AccountStoreRepository, AccountStoreSnapshot};
use crate::state::AppState;

type SharedState<'a> = State<'a, Arc<AppState>>;

#[tauri::command]
pub async fn account_store_load(state: SharedState<'_>) -> AppResult<AccountStoreSnapshot> {
    AccountStoreRepository::new(state.data_dir()).load()
}

#[tauri::command]
pub async fn account_store_save(state: SharedState<'_>, payload: AccountStoreSnapshot) -> AppResult<()> {
    AccountStoreRepository::new(state.data_dir()).save(&payload)
}
