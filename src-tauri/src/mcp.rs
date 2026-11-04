use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub use cirrus_core::mcp_access::McpToolAccessSettings;
use cirrus_core::mcp_access::{
    load_account_access, load_mcp_settings, load_tool_access, save_account_access, save_mcp_settings, save_tool_access,
    McpAccountAccessSettings,
};
use cirrus_mcp::{serve_http_on_listener, HttpConfig as McpHttpConfig};
use serde::Serialize;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{oneshot, Mutex};
use uuid::Uuid;

use crate::account_store::AccountStoreRepository;
use crate::state::AppState;

pub const LOGIN_TOOL: &str = "cirrus_login";
pub const LIST_ACCOUNTS_TOOL: &str = "cirrus_list_accounts";
const HTTP_PORT: u16 = 5226;
const HTTP_PATH: &str = "/mcp";
const AUTHORIZATION_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpInfo {
    pub enabled: bool,
    pub running: bool,
    pub port: Option<u16>,
    pub endpoint: Option<String>,
    pub command: Option<String>,
    pub config_toml: Option<String>,
    pub http_running: bool,
    pub http_port: Option<u16>,
    pub http_endpoint: Option<String>,
    pub http_access_token: Option<String>,
    pub http_error: Option<String>,
    pub version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAuthorizeResponse {
    pub approved: bool,
    pub request_id: String,
    pub client_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpVisibleAccount {
    pub account_key: String,
    pub apple_id: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub signed_in: bool,
    pub active: bool,
    pub last_used_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAccountList {
    pub accounts: Vec<McpVisibleAccount>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAccountSummary {
    pub account_key: String,
    pub apple_id: String,
    pub dsid: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub display_name: String,
    pub mcp_enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAuthorizationEvent {
    request_id: String,
    client_name: String,
    client_version: Option<String>,
    tool: String,
    expires_in_seconds: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct McpLoginRequestEvent {
    request_id: String,
    client_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct McpLoginFinishedEvent {
    request_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LoginOutcome {
    authenticated: bool,
    approved: bool,
    message: Option<String>,
    account: Option<McpAccountSummary>,
}

impl LoginOutcome {
    fn success(account: McpAccountSummary) -> Self {
        Self { authenticated: true, approved: true, message: None, account: Some(account) }
    }

    fn error(message: impl Into<String>) -> Self {
        Self { authenticated: false, approved: true, message: Some(message.into()), account: None }
    }

    fn denied(message: impl Into<String>) -> Self {
        Self { authenticated: false, approved: false, message: Some(message.into()), account: None }
    }
}

struct PendingAuthorization {
    id: String,
    client_name: String,
    client_version: Option<String>,
    approval_tx: oneshot::Sender<bool>,
    login_tx: oneshot::Sender<LoginOutcome>,
}

struct ActiveLogin {
    id: String,
    login_tx: oneshot::Sender<LoginOutcome>,
}

struct HttpRuntime {
    port: u16,
    stop: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
}
#[derive(Default)]
struct Inner {
    pending_authorization: Option<PendingAuthorization>,
    active_login: Option<ActiveLogin>,
}

pub struct McpService {
    data_dir: Option<PathBuf>,
    enabled: AtomicBool,
    running_port: AtomicU16,
    starting: AtomicBool,
    inner: Mutex<Inner>,
    http: std::sync::Mutex<Option<HttpRuntime>>,
    http_token: std::sync::Mutex<Option<String>>,
    http_error: std::sync::Mutex<Option<String>>,
}

impl McpService {
    pub fn new(data_dir: Option<&Path>) -> Self {
        let enabled = data_dir.map(load_enabled_setting).unwrap_or(false);
        if let Some(data_dir) = data_dir {
            let _ = load_account_access(data_dir);
            let _ = load_tool_access(data_dir);
        }
        Self {
            data_dir: data_dir.map(Path::to_path_buf),
            enabled: AtomicBool::new(enabled),
            running_port: AtomicU16::new(0),
            starting: AtomicBool::new(false),
            inner: Mutex::new(Inner::default()),
            http: std::sync::Mutex::new(None),
            http_token: std::sync::Mutex::new(None),
            http_error: std::sync::Mutex::new(None),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn info(&self) -> McpInfo {
        let port = match self.running_port.load(Ordering::Relaxed) {
            0 => None,
            value => Some(value),
        };
        let command = mcp_binary_path().filter(|path| path.is_file()).map(|path| path.display().to_string());
        let client_data_dir = crate::mcp_clients::client_data_dir(self.data_dir.as_deref());
        let config_toml =
            command.as_ref().map(|path| crate::mcp_clients::codex_config_snippet(Path::new(path), client_data_dir));
        let (http_running, http_port) = self.http_status();
        let http_access_token =
            if http_running { self.http_token.lock().unwrap_or_else(|error| error.into_inner()).clone() } else { None };
        let http_error = self.http_error.lock().unwrap_or_else(|error| error.into_inner()).clone();
        McpInfo {
            enabled: self.enabled(),
            running: port.is_some(),
            port,
            endpoint: port.map(|value| format!("http://127.0.0.1:{value}")),
            command,
            config_toml,
            http_running,
            http_port,
            http_endpoint: http_port.map(|value| format!("http://127.0.0.1:{value}{HTTP_PATH}")),
            http_access_token,
            http_error,
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    pub fn binary_path(&self) -> Option<PathBuf> {
        mcp_binary_path().filter(|path| path.is_file())
    }

    fn http_status(&self) -> (bool, Option<u16>) {
        let mut exited = None;
        let result = {
            let mut http = self.http.lock().unwrap_or_else(|error| error.into_inner());
            if let Some(runtime) = http.as_ref() {
                if runtime.task.inner().is_finished() {
                    let port = runtime.port;
                    *http = None;
                    exited = Some("MCP HTTP service stopped unexpectedly".to_string());
                    (false, Some(port))
                } else {
                    (true, Some(runtime.port))
                }
            } else {
                (false, None)
            }
        };
        if let Some(error) = exited {
            *self.http_error.lock().unwrap_or_else(|error| error.into_inner()) = Some(error);
        }
        result
    }

    async fn start_http(&self, data_dir: PathBuf) -> Result<(), String> {
        self.stop_http().await;
        let bind_addr = std::net::SocketAddr::from(([127, 0, 0, 1], HTTP_PORT));
        let listener = tokio::net::TcpListener::bind(bind_addr)
            .await
            .map_err(|error| format!("Could not bind the MCP HTTP service at {bind_addr}: {error}"))?;
        let token = Uuid::new_v4().simple().to_string();
        let config = McpHttpConfig {
            bind_addr,
            path: HTTP_PATH.to_string(),
            token: token.clone(),
            allow_remote: false,
            data_dir: Some(data_dir),
        };
        let (stop_tx, stop_rx) = oneshot::channel::<()>();
        let task = tauri::async_runtime::spawn(async move {
            if let Err(error) = serve_http_on_listener(listener, config, async move {
                let _ = stop_rx.await;
            })
            .await
            {
                log::error!("[MCP] Streamable HTTP stopped: {error}");
            }
        });

        *self.http.lock().unwrap_or_else(|error| error.into_inner()) =
            Some(HttpRuntime { port: HTTP_PORT, stop: Some(stop_tx), task });
        *self.http_token.lock().unwrap_or_else(|error| error.into_inner()) = Some(token);
        *self.http_error.lock().unwrap_or_else(|error| error.into_inner()) = None;
        log::info!("[MCP] Streamable HTTP listening on http://127.0.0.1:{HTTP_PORT}{HTTP_PATH}");
        Ok(())
    }

    async fn stop_http(&self) {
        let runtime = self.http.lock().unwrap_or_else(|error| error.into_inner()).take();
        if let Some(mut runtime) = runtime {
            if let Some(stop) = runtime.stop.take() {
                let _ = stop.send(());
            }
            let _ = runtime.task.await;
            log::info!("[MCP] Streamable HTTP stopped");
        }
        *self.http_token.lock().unwrap_or_else(|error| error.into_inner()) = None;
        *self.http_error.lock().unwrap_or_else(|error| error.into_inner()) = None;
    }

    pub fn disabled_account_keys(&self) -> BTreeSet<String> {
        let Some(data_dir) = self.data_dir.as_deref() else {
            return BTreeSet::new();
        };
        let settings = load_account_access(data_dir);
        if !settings.deny_all {
            return settings.disabled_account_keys;
        }
        AccountStoreRepository::new(Some(data_dir))
            .load()
            .map(|snapshot| snapshot.accounts.into_iter().map(|account| account.key).collect())
            .unwrap_or_default()
    }

    pub fn account_enabled(&self, account_key: &str) -> bool {
        let settings = self.data_dir.as_deref().map(load_account_access);
        settings
            .map(|settings| !settings.deny_all && !settings.disabled_account_keys.contains(account_key))
            .unwrap_or(false)
    }

    pub fn ensure_account_enabled(&self, account_key: &str) -> Result<(), String> {
        if self.account_enabled(account_key) {
            Ok(())
        } else {
            Err("This iCloud account is disabled for MCP operations".to_string())
        }
    }

    pub fn set_account_enabled(&self, account_key: &str, enabled: bool) -> Result<(), String> {
        let data_dir = self.data_dir.as_deref().ok_or_else(|| "App data directory is unavailable".to_string())?;
        if account_key.trim().is_empty() || (!account_key.starts_with("dsid:") && !account_key.starts_with("apple:")) {
            return Err("Invalid account key".to_string());
        }
        let mut settings = load_account_access(data_dir);
        if enabled {
            settings.disabled_account_keys.remove(account_key);
        } else {
            settings.disabled_account_keys.insert(account_key.to_string());
        }
        save_account_access(data_dir, &settings).map_err(|error| format!("Could not save MCP account access: {error}"))
    }

    pub fn tool_access(&self) -> McpToolAccessSettings {
        self.data_dir.as_deref().map(load_tool_access).unwrap_or_default()
    }

    pub fn tool_enabled(&self, tool: &str) -> bool {
        let access = self.tool_access();
        !access.deny_all && !access.disabled_tools.contains(tool)
    }

    pub fn set_tool_enabled(&self, tool: &str, enabled: bool) -> Result<McpToolAccessSettings, String> {
        let data_dir = self.data_dir.as_deref().ok_or_else(|| "App data directory is unavailable".to_string())?;
        if !tool.starts_with("cirrus_") {
            return Err("Invalid MCP tool name".to_string());
        }
        let mut settings = load_tool_access(data_dir);
        if enabled {
            settings.disabled_tools.remove(tool);
        } else {
            settings.disabled_tools.insert(tool.to_string());
        }
        save_tool_access(data_dir, &settings).map_err(|error| format!("Could not save MCP tool access: {error}"))?;
        Ok(settings)
    }

    pub fn visible_accounts(&self) -> Result<McpAccountList, String> {
        let snapshot =
            AccountStoreRepository::new(self.data_dir.as_deref()).load().map_err(|error| error.to_string())?;
        let account_access = self
            .data_dir
            .as_deref()
            .map(load_account_access)
            .unwrap_or_else(|| McpAccountAccessSettings { deny_all: true, ..Default::default() });
        let mut accounts = snapshot
            .accounts
            .into_iter()
            .filter(|account| !account_access.deny_all && !account_access.disabled_account_keys.contains(&account.key))
            .map(|account| McpVisibleAccount {
                account_key: account.key.clone(),
                apple_id: account.apple_id,
                first_name: account.first_name,
                last_name: account.last_name,
                signed_in: account.photos_session_id.is_some() || account.devices_session_id.is_some(),
                active: snapshot.active_account_key.as_deref() == Some(account.key.as_str()),
                last_used_at: account.last_used_at,
            })
            .collect::<Vec<_>>();
        accounts.sort_by_key(|account| std::cmp::Reverse(account.last_used_at));
        let count = accounts.len();
        Ok(McpAccountList { accounts, count })
    }

    pub fn start_in_background(self: &Arc<Self>, app: AppHandle, state: Arc<AppState>) {
        if !self.enabled() || self.running_port.load(Ordering::Relaxed) != 0 {
            return;
        }
        let service = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            if let Err(error) = service.start(app, state).await {
                log::warn!("[MCP] could not start the direct MCP service: {error}");
            }
        });
    }

    pub async fn start(self: &Arc<Self>, app: AppHandle, state: Arc<AppState>) -> Result<(), String> {
        if self.running_port.load(Ordering::Relaxed) != 0 {
            return Ok(());
        }
        if self.starting.swap(true, Ordering::AcqRel) {
            return Err("The MCP service is already starting".to_string());
        }
        let result = self.start_locked(app, state).await;
        self.starting.store(false, Ordering::Release);
        result
    }

    async fn start_locked(self: &Arc<Self>, _app: AppHandle, _state: Arc<AppState>) -> Result<(), String> {
        let data_dir =
            self.data_dir.as_ref().ok_or_else(|| "App data directory is unavailable; MCP cannot start".to_string())?;
        self.enabled.store(true, Ordering::Relaxed);
        save_enabled_setting(self.data_dir.as_deref(), true);
        match self.start_http(data_dir.clone()).await {
            Ok(()) => {
                self.running_port.store(HTTP_PORT, Ordering::Release);
                log::info!("[MCP] direct Streamable HTTP listening on http://127.0.0.1:{HTTP_PORT}{HTTP_PATH}");
                Ok(())
            }
            Err(error) => {
                self.enabled.store(false, Ordering::Relaxed);
                self.running_port.store(0, Ordering::Release);
                save_enabled_setting(self.data_dir.as_deref(), false);
                *self.http_error.lock().unwrap_or_else(|error| error.into_inner()) = Some(error.clone());
                Err(error)
            }
        }
    }
    pub async fn pending_authorization(&self) -> Option<McpAuthorizationEvent> {
        let inner = self.inner.lock().await;
        inner.pending_authorization.as_ref().map(|pending| McpAuthorizationEvent {
            request_id: pending.id.clone(),
            client_name: pending.client_name.clone(),
            client_version: pending.client_version.clone(),
            tool: LOGIN_TOOL.to_string(),
            expires_in_seconds: AUTHORIZATION_TIMEOUT.as_secs(),
        })
    }

    pub async fn stop(&self) {
        let (pending, active) = {
            let mut inner = self.inner.lock().await;
            (inner.pending_authorization.take(), inner.active_login.take())
        };
        if let Some(pending) = pending {
            let _ = pending.approval_tx.send(false);
        }
        if let Some(active) = active {
            let _ = active.login_tx.send(LoginOutcome::denied("MCP was disabled before sign-in completed"));
        }
        self.stop_http().await;
        self.running_port.store(0, Ordering::Release);
        self.enabled.store(false, Ordering::Relaxed);
        self.starting.store(false, Ordering::Release);
        save_enabled_setting(self.data_dir.as_deref(), false);
    }

    pub async fn authorize(
        &self,
        request_id: &str,
        approved: bool,
        app: &AppHandle,
    ) -> Result<McpAuthorizeResponse, String> {
        if !approved {
            let pending = {
                let mut inner = self.inner.lock().await;
                match inner.pending_authorization.take() {
                    Some(pending) if pending.id == request_id => Some(pending),
                    other => {
                        inner.pending_authorization = other;
                        None
                    }
                }
            };
            let pending = pending.ok_or_else(|| "This MCP authorization request is no longer active".to_string())?;
            let client_name = pending.client_name.clone();
            let _ = pending.approval_tx.send(false);
            return Ok(McpAuthorizeResponse { approved: false, request_id: request_id.to_string(), client_name });
        }

        let client_name = {
            let mut inner = self.inner.lock().await;
            if inner.active_login.is_some() {
                return Err("Another Cirrus MCP sign-in is already in progress".to_string());
            }
            let pending = match inner.pending_authorization.take() {
                Some(pending) if pending.id == request_id => pending,
                other => {
                    inner.pending_authorization = other;
                    return Err("This MCP authorization request is no longer active".to_string());
                }
            };
            let client_name = pending.client_name.clone();
            let _ = pending.approval_tx.send(true);
            let login_tx = pending.login_tx;
            inner.active_login = Some(ActiveLogin { id: pending.id, login_tx });
            client_name
        };

        show_main_window(app);
        if let Err(error) = app.emit(
            "mcp-login-request",
            McpLoginRequestEvent { request_id: request_id.to_string(), client_name: client_name.clone() },
        ) {
            self.cancel_active(request_id, "Could not show the Cirrus sign-in window").await;
            return Err(format!("Could not open the Cirrus sign-in view: {error}"));
        }
        log::info!("[MCP] user authorized sign-in for {client_name}");
        Ok(McpAuthorizeResponse { approved: true, request_id: request_id.to_string(), client_name })
    }

    pub async fn complete_login(
        &self,
        request_id: &str,
        session_id: &str,
        app: &AppHandle,
        state: &AppState,
    ) -> Result<McpAccountSummary, String> {
        let handle = state.sessions.get(session_id).await.map_err(|error| error.to_string())?;
        let mut account = {
            let session = handle.lock().await;
            if !session.is_flag_set(cirrus_core::models::session_keys::PHOTOS_VERIFIED) {
                return Err("The iCloud session is not fully verified yet".to_string());
            }
            account_summary(&session)?
        };
        account.mcp_enabled = self.account_enabled(&account.account_key);
        if let Err(error) = state.sessions.persist(&handle).await {
            log::warn!("[MCP] could not persist the authenticated session; it remains valid for this run: {error}");
        }

        let active = {
            let mut inner = self.inner.lock().await;
            match inner.active_login.as_ref() {
                Some(active) if active.id == request_id => inner.active_login.take(),
                _ => None,
            }
        };
        let Some(active) = active else {
            return Err("This Cirrus MCP sign-in request is no longer active".to_string());
        };
        let _ = active.login_tx.send(LoginOutcome::success(account.clone()));
        let _ = app.emit("mcp-login-finished", McpLoginFinishedEvent { request_id: request_id.to_string() });
        log::info!("[MCP] MCP sign-in completed for {}", account.display_name);
        Ok(account)
    }

    pub async fn cancel_login(&self, request_id: &str, app: &AppHandle) -> Result<(), String> {
        let active = self
            .take_active(request_id)
            .await
            .ok_or_else(|| "This Cirrus MCP sign-in request is no longer active".to_string())?;
        let _ = active.login_tx.send(LoginOutcome::denied("The user cancelled Cirrus sign-in"));
        let _ = app.emit("mcp-login-finished", McpLoginFinishedEvent { request_id: request_id.to_string() });
        Ok(())
    }

    async fn take_active(&self, request_id: &str) -> Option<ActiveLogin> {
        let mut inner = self.inner.lock().await;
        if inner.active_login.as_ref().map(|active| active.id == request_id).unwrap_or(false) {
            inner.active_login.take()
        } else {
            None
        }
    }

    async fn cancel_active(&self, request_id: &str, message: impl Into<String>) {
        if let Some(active) = self.take_active(request_id).await {
            let _ = active.login_tx.send(LoginOutcome::error(message));
        }
    }
}

impl Drop for McpService {
    fn drop(&mut self) {
        if let Ok(http) = self.http.get_mut() {
            if let Some(mut runtime) = http.take() {
                if let Some(stop) = runtime.stop.take() {
                    let _ = stop.send(());
                }
                runtime.task.abort();
            }
        }
    }
}

fn show_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}

fn account_summary(session: &cirrus_core::SrpSession) -> Result<McpAccountSummary, String> {
    if session.apple_id().trim().is_empty() {
        return Err("Apple did not return an account identifier. Please sign in again.".to_string());
    }
    let first_name = session.first_name().map(ToOwned::to_owned);
    let last_name = session.last_name().map(ToOwned::to_owned);
    let display_name = [first_name.as_deref(), last_name.as_deref()]
        .into_iter()
        .flatten()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let dsid = session.dsid().map(ToOwned::to_owned);
    Ok(McpAccountSummary {
        account_key: account_key(session.apple_id(), dsid.as_deref()),
        apple_id: session.apple_id().to_string(),
        dsid,
        first_name,
        last_name,
        display_name: if display_name.is_empty() { mask_apple_id(session.apple_id()) } else { display_name },
        mcp_enabled: false,
    })
}

pub fn account_key(apple_id: &str, dsid: Option<&str>) -> String {
    if let Some(dsid) = dsid.map(str::trim).filter(|value| !value.is_empty()) {
        return format!("dsid:{dsid}");
    }
    format!("apple:{}", apple_id.trim().to_lowercase())
}

fn mask_apple_id(apple_id: &str) -> String {
    let trimmed = apple_id.trim();
    let Some((local, domain)) = trimmed.split_once('@') else {
        return if trimmed.len() <= 2 { "***".to_string() } else { format!("{}***", &trimmed[..1]) };
    };
    let first = local.chars().next().unwrap_or('*');
    format!("{first}***@{domain}")
}

fn mcp_sibling_path(file_name: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.join(if cfg!(windows) { format!("{file_name}.exe") } else { file_name.to_string() }))
}

pub(crate) fn bundled_mcp_binary_path() -> Option<PathBuf> {
    mcp_sibling_path("cirrus-mcp-server")
}

pub(crate) fn bundled_mcp_native_binary_path() -> Option<PathBuf> {
    mcp_sibling_path("cirrus-mcp")
}

pub(crate) fn mcp_binary_path() -> Option<PathBuf> {
    // Client configurations must always launch the native server directly.
    // The legacy cirrus-mcp-server launcher remains shipped only for existing
    // external configurations and forwards to this same native binary.
    crate::mcp_installation::installed_native_path()
        .ok()
        .filter(|path| path.is_file())
        .or_else(bundled_mcp_native_binary_path)
}

fn load_enabled_setting(data_dir: &Path) -> bool {
    load_mcp_settings(data_dir).enabled
}

fn save_enabled_setting(data_dir: Option<&Path>, enabled: bool) {
    let Some(data_dir) = data_dir else {
        return;
    };
    if let Err(error) = save_mcp_settings(data_dir, enabled) {
        log::warn!("[MCP] could not save the MCP setting: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apple_id_mask_keeps_only_the_first_local_character() {
        assert_eq!(mask_apple_id("person@example.com"), "p***@example.com");
        assert_eq!(mask_apple_id("x"), "***");
    }

    #[test]
    fn account_keys_prefer_dsid_and_normalize_apple_ids() {
        assert_eq!(account_key(" Person@Example.com ", Some(" 123 ")), "dsid:123");
        assert_eq!(account_key(" Person@Example.com ", None), "apple:person@example.com");
    }

    #[test]
    fn accounts_are_enabled_by_default_and_can_be_disabled() {
        let dir = std::env::temp_dir().join(format!("cirrus-mcp-account-access-{}", Uuid::new_v4()));
        let service = McpService::new(Some(&dir));
        assert!(service.account_enabled("dsid:123"));
        service.set_account_enabled("dsid:123", false).expect("disable account");
        assert!(!service.account_enabled("dsid:123"));
        service.set_account_enabled("dsid:123", true).expect("enable account");
        assert!(service.account_enabled("dsid:123"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn mcp_is_enabled_by_default_for_new_installs() {
        let dir = std::env::temp_dir().join(format!("cirrus-mcp-default-enabled-{}", Uuid::new_v4()));
        let service = McpService::new(Some(&dir));
        assert!(service.enabled());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn settings_roundtrip() {
        let dir = std::env::temp_dir().join(format!("cirrus-mcp-settings-{}", Uuid::new_v4()));
        save_enabled_setting(Some(&dir), true);
        assert!(load_enabled_setting(&dir));
        save_enabled_setting(Some(&dir), false);
        assert!(!load_enabled_setting(&dir));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn bundled_mcp_native_binary_uses_the_app_sibling_name() {
        let path = bundled_mcp_native_binary_path().expect("native binary path");
        let name = path.file_name().and_then(|value| value.to_str()).unwrap_or_default();
        assert_eq!(name, if cfg!(windows) { "cirrus-mcp.exe" } else { "cirrus-mcp" });
    }
}
