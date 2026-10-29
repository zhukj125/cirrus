use std::sync::Arc;

use tauri::{AppHandle, State};

use crate::account_store::{AccountStoreRepository, AccountStoreSnapshot};
use crate::mcp::{McpAccountSummary, McpAuthorizationEvent, McpAuthorizeResponse, McpInfo, McpToolAccessSettings};
use crate::mcp_clients::{self, McpClientInfo};
use crate::state::AppState;

type SharedState<'a> = State<'a, Arc<AppState>>;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpEnabledRequest {
    pub enabled: bool,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAuthorizeRequest {
    pub request_id: String,
    pub approved: bool,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpLoginRequest {
    pub request_id: String,
    pub session_id: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpCancelRequest {
    pub request_id: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpClientRequest {
    pub client_id: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolAccessRequest {
    pub tool: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAccountAccessRequest {
    pub account_key: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAccountInfo {
    pub key: String,
    pub apple_id: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub last_used_at: i64,
    pub enabled: bool,
    pub signed_in: bool,
    pub avatar_data_url: Option<String>,
}

#[tauri::command]
pub fn mcp_info(state: SharedState<'_>) -> McpInfo {
    state.inner().mcp.info()
}

#[tauri::command]
pub async fn mcp_pending_authorization(state: SharedState<'_>) -> Result<Option<McpAuthorizationEvent>, String> {
    Ok(state.inner().mcp.pending_authorization().await)
}

#[tauri::command]
pub fn mcp_tool_access(state: SharedState<'_>) -> McpToolAccessSettings {
    state.inner().mcp.tool_access()
}

#[tauri::command]
pub fn mcp_set_tool_enabled(
    state: SharedState<'_>,
    payload: McpToolAccessRequest,
) -> Result<McpToolAccessSettings, String> {
    state.inner().mcp.set_tool_enabled(&payload.tool, payload.enabled)
}

#[tauri::command]
pub fn mcp_clients(state: SharedState<'_>) -> Vec<McpClientInfo> {
    mcp_clients::list_clients(state.inner().mcp.binary_path().as_deref())
}

#[tauri::command]
pub fn mcp_install_client(state: SharedState<'_>, payload: McpClientRequest) -> Result<McpClientInfo, String> {
    let binary =
        state.inner().mcp.binary_path().ok_or_else(|| "The Cirrus MCP executable is not available".to_string())?;
    mcp_clients::install_client(&payload.client_id, &binary)
}

#[tauri::command]
pub fn mcp_remove_client(state: SharedState<'_>, payload: McpClientRequest) -> Result<McpClientInfo, String> {
    mcp_clients::remove_client(&payload.client_id, state.inner().mcp.binary_path().as_deref())
}

#[tauri::command]
pub fn mcp_accounts(state: SharedState<'_>) -> Result<Vec<McpAccountInfo>, String> {
    list_mcp_accounts(state.inner())
}

#[tauri::command]
pub fn mcp_set_account_enabled(
    state: SharedState<'_>,
    payload: McpAccountAccessRequest,
) -> Result<Vec<McpAccountInfo>, String> {
    state.inner().mcp.set_account_enabled(&payload.account_key, payload.enabled)?;
    list_mcp_accounts(state.inner())
}

fn list_mcp_accounts(state: &AppState) -> Result<Vec<McpAccountInfo>, String> {
    let snapshot = AccountStoreRepository::new(state.data_dir()).load().map_err(|error| error.to_string())?;
    let AccountStoreSnapshot { accounts, avatars, .. } = snapshot;
    let disabled = state.mcp.disabled_account_keys();
    Ok(accounts
        .into_iter()
        .map(|account| McpAccountInfo {
            key: account.key.clone(),
            apple_id: account.apple_id.clone(),
            first_name: account.first_name,
            last_name: account.last_name,
            last_used_at: account.last_used_at,
            enabled: !disabled.contains(&account.key),
            signed_in: account.photos_session_id.is_some() || account.devices_session_id.is_some(),
            avatar_data_url: avatars.get(&account.key).cloned(),
        })
        .collect())
}

#[tauri::command]
pub async fn mcp_set_enabled(
    app: AppHandle,
    state: SharedState<'_>,
    payload: McpEnabledRequest,
) -> Result<McpInfo, String> {
    if payload.enabled {
        let state_ref = Arc::clone(state.inner());
        state_ref.mcp.start(app, Arc::clone(&state_ref)).await?;
    } else {
        state.inner().mcp.stop().await;
    }
    Ok(state.inner().mcp.info())
}

#[tauri::command]
pub async fn mcp_authorize(
    app: AppHandle,
    state: SharedState<'_>,
    payload: McpAuthorizeRequest,
) -> Result<McpAuthorizeResponse, String> {
    state.inner().mcp.authorize(&payload.request_id, payload.approved, &app).await
}

#[tauri::command]
pub async fn mcp_complete_login(
    app: AppHandle,
    state: SharedState<'_>,
    payload: McpLoginRequest,
) -> Result<McpAccountSummary, String> {
    state.inner().mcp.complete_login(&payload.request_id, &payload.session_id, &app, state.inner()).await
}

#[tauri::command]
pub async fn mcp_cancel_login(app: AppHandle, state: SharedState<'_>, payload: McpCancelRequest) -> Result<(), String> {
    state.inner().mcp.cancel_login(&payload.request_id, &app).await
}
