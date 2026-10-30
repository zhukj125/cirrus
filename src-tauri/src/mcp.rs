use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

use http_body_util::{BodyExt, Full};
use hyper::body::{Body, Bytes};
use hyper::header::{self, HeaderValue};
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, Manager};
use tokio::net::TcpListener;
use tokio::sync::{oneshot, Mutex};
use uuid::Uuid;

use crate::account_store::AccountStoreRepository;
use crate::state::AppState;

pub const LOGIN_TOOL: &str = "cirrus_login";
pub const LIST_ACCOUNTS_TOOL: &str = "cirrus_list_accounts";
const PORT_FILE: &str = "mcp-bridge-port";
const TOKEN_FILE: &str = "mcp-bridge-token";
const SETTINGS_FILE: &str = "mcp-settings.json";
const ACCOUNT_ACCESS_FILE: &str = "mcp-account-access.json";
const TOOL_ACCESS_FILE: &str = "mcp-tool-access.json";
const AUTHORIZATION_TIMEOUT: Duration = Duration::from_secs(120);
const APP_LOGIN_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const MAX_HTTP_BODY: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpInfo {
    pub enabled: bool,
    pub running: bool,
    pub port: Option<u16>,
    pub endpoint: Option<String>,
    pub command: Option<String>,
    pub config_toml: Option<String>,
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
struct McpAuthorizationFinishedEvent {
    request_id: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpSettings {
    enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct McpAccountAccessSettings {
    #[serde(default)]
    disabled_account_keys: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct McpToolAccessSettings {
    #[serde(default)]
    pub disabled_tools: BTreeSet<String>,
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

struct ServerRuntime {
    task: JoinHandle<()>,
}

#[derive(Default)]
struct Inner {
    runtime: Option<ServerRuntime>,
    pending_authorization: Option<PendingAuthorization>,
    active_login: Option<ActiveLogin>,
}

pub struct McpService {
    data_dir: Option<PathBuf>,
    enabled: AtomicBool,
    running_port: AtomicU16,
    starting: AtomicBool,
    inner: Mutex<Inner>,
}

impl McpService {
    pub fn new(data_dir: Option<&Path>) -> Self {
        let enabled = data_dir.map(load_enabled_setting).unwrap_or(false);
        Self {
            data_dir: data_dir.map(Path::to_path_buf),
            enabled: AtomicBool::new(enabled),
            running_port: AtomicU16::new(0),
            starting: AtomicBool::new(false),
            inner: Mutex::new(Inner::default()),
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
        let config_toml =
            command.as_ref().map(|path| format!("[mcp_servers.cirrus]\ncommand = \"{}\"\n", escape_toml_string(path)));
        McpInfo {
            enabled: self.enabled(),
            running: port.is_some(),
            port,
            endpoint: port.map(|value| format!("http://127.0.0.1:{value}")),
            command,
            config_toml,
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    pub fn binary_path(&self) -> Option<PathBuf> {
        mcp_binary_path().filter(|path| path.is_file())
    }

    pub fn disabled_account_keys(&self) -> BTreeSet<String> {
        self.data_dir
            .as_deref()
            .map(load_account_access)
            .map(|settings| settings.disabled_account_keys)
            .unwrap_or_default()
    }

    pub fn account_enabled(&self, account_key: &str) -> bool {
        !self.disabled_account_keys().contains(account_key)
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
        let value = serde_json::to_vec_pretty(&settings).map_err(|error| error.to_string())?;
        write_private_file(&data_dir.join(ACCOUNT_ACCESS_FILE), &value)
            .map_err(|error| format!("Could not save MCP account access: {error}"))
    }

    pub fn tool_access(&self) -> McpToolAccessSettings {
        self.data_dir.as_deref().map(load_tool_access).unwrap_or_default()
    }

    pub fn tool_enabled(&self, tool: &str) -> bool {
        !self.tool_access().disabled_tools.contains(tool)
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
        let value = serde_json::to_vec_pretty(&settings).map_err(|error| error.to_string())?;
        write_private_file(&data_dir.join(TOOL_ACCESS_FILE), &value)
            .map_err(|error| format!("Could not save MCP tool access: {error}"))?;
        Ok(settings)
    }

    pub fn visible_accounts(&self) -> Result<McpAccountList, String> {
        let snapshot =
            AccountStoreRepository::new(self.data_dir.as_deref()).load().map_err(|error| error.to_string())?;
        let disabled = self.disabled_account_keys();
        let mut accounts = snapshot
            .accounts
            .into_iter()
            .filter(|account| !disabled.contains(&account.key))
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
                log::warn!("[MCP] could not start the local bridge: {error}");
            }
        });
    }

    pub async fn start(self: &Arc<Self>, app: AppHandle, state: Arc<AppState>) -> Result<(), String> {
        if self.running_port.load(Ordering::Relaxed) != 0 {
            return Ok(());
        }
        if self.starting.swap(true, Ordering::AcqRel) {
            return Err("The MCP bridge is already starting".to_string());
        }
        let result = self.start_locked(app, state).await;
        self.starting.store(false, Ordering::Release);
        result
    }

    async fn start_locked(self: &Arc<Self>, app: AppHandle, state: Arc<AppState>) -> Result<(), String> {
        let data_dir =
            self.data_dir.as_ref().ok_or_else(|| "App data directory is unavailable; MCP cannot start".to_string())?;
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(|error| format!("Could not bind the Cirrus MCP bridge: {error}"))?;
        let port =
            listener.local_addr().map_err(|error| format!("Could not read the MCP bridge address: {error}"))?.port();
        let token = Uuid::new_v4().simple().to_string();

        write_private_file(&data_dir.join(PORT_FILE), port.to_string().as_bytes())
            .map_err(|error| format!("Could not write the MCP port file: {error}"))?;
        write_private_file(&data_dir.join(TOKEN_FILE), token.as_bytes())
            .map_err(|error| format!("Could not write the MCP token file: {error}"))?;

        let service = Arc::downgrade(self);
        let weak_state = Arc::downgrade(&state);
        let task_app = app.clone();
        let task_token = token.clone();
        let task = tauri::async_runtime::spawn(async move {
            accept_loop(listener, service, weak_state, task_app, task_token).await;
        });

        let mut inner = self.inner.lock().await;
        if inner.runtime.is_some() {
            task.abort();
            return Ok(());
        }
        inner.runtime = Some(ServerRuntime { task });
        self.enabled.store(true, Ordering::Relaxed);
        self.running_port.store(port, Ordering::Release);
        save_enabled_setting(self.data_dir.as_deref(), true);
        log::info!("[MCP] local bridge listening on http://127.0.0.1:{port}");
        Ok(())
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
        let (runtime, pending, active) = {
            let mut inner = self.inner.lock().await;
            (inner.runtime.take(), inner.pending_authorization.take(), inner.active_login.take())
        };
        if let Some(runtime) = runtime {
            runtime.task.abort();
            log::info!("[MCP] local bridge stopped");
        }
        if let Some(pending) = pending {
            let _ = pending.approval_tx.send(false);
        }
        if let Some(active) = active {
            let _ = active.login_tx.send(LoginOutcome::denied("MCP was disabled before sign-in completed"));
        }
        self.running_port.store(0, Ordering::Release);
        self.enabled.store(false, Ordering::Relaxed);
        self.starting.store(false, Ordering::Release);
        if let Some(data_dir) = &self.data_dir {
            let _ = std::fs::remove_file(data_dir.join(PORT_FILE));
            let _ = std::fs::remove_file(data_dir.join(TOKEN_FILE));
        }
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

    async fn cancel_pending(&self, request_id: &str) {
        let mut inner = self.inner.lock().await;
        if inner.pending_authorization.as_ref().map(|pending| pending.id == request_id).unwrap_or(false) {
            inner.pending_authorization = None;
        }
    }
}

async fn accept_loop(
    listener: TcpListener,
    service: Weak<McpService>,
    app_state: Weak<AppState>,
    app: AppHandle,
    bridge_token: String,
) {
    loop {
        let (stream, _) = match listener.accept().await {
            Ok(value) => value,
            Err(error) => {
                log::warn!("[MCP] bridge accept failed: {error}");
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }
        };
        let service = service.clone();
        let app_state = app_state.clone();
        let app = app.clone();
        let bridge_token = bridge_token.clone();
        tokio::spawn(async move {
            let io = TokioIo::new(stream);
            let service = service_fn(move |request| {
                let service = service.clone();
                let app_state = app_state.clone();
                let app = app.clone();
                let bridge_token = bridge_token.clone();
                async move {
                    Ok::<_, std::convert::Infallible>(
                        handle_request(request, service, app_state, app, bridge_token).await,
                    )
                }
            });
            if let Err(error) = hyper::server::conn::http1::Builder::new().serve_connection(io, service).await {
                log::debug!("[MCP] bridge connection ended: {error}");
            }
        });
    }
}

async fn handle_request(
    request: Request<hyper::body::Incoming>,
    service: Weak<McpService>,
    app_state: Weak<AppState>,
    app: AppHandle,
    bridge_token: String,
) -> Response<Full<Bytes>> {
    let path = request.uri().path();
    if request.method() == Method::GET && path == "/healthz" {
        return json_response(
            StatusCode::OK,
            json!({ "ok": true, "app": "cirrus", "version": env!("CARGO_PKG_VERSION") }),
        );
    }
    if request.method() != Method::POST || path != "/v1/call" {
        return json_response(StatusCode::NOT_FOUND, json!({ "ok": false, "error": "Not found" }));
    }
    if !authorized_bearer(&request, &bridge_token) {
        return json_response(StatusCode::UNAUTHORIZED, json!({ "ok": false, "error": "Unauthorized" }));
    }
    let Some(service) = service.upgrade() else {
        return json_response(
            StatusCode::SERVICE_UNAVAILABLE,
            json!({ "ok": false, "error": "Cirrus is shutting down" }),
        );
    };
    let Some(app_state) = app_state.upgrade() else {
        return json_response(
            StatusCode::SERVICE_UNAVAILABLE,
            json!({ "ok": false, "error": "Cirrus is shutting down" }),
        );
    };
    handle_call(request, service, app_state, app).await
}

async fn handle_call(
    request: Request<hyper::body::Incoming>,
    service: Arc<McpService>,
    app_state: Arc<AppState>,
    app: AppHandle,
) -> Response<Full<Bytes>> {
    let body = match read_body(request).await {
        Ok(body) => body,
        Err(error) => return json_response(StatusCode::BAD_REQUEST, json!({ "ok": false, "error": error })),
    };
    let payload: Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(error) => {
            return json_response(
                StatusCode::BAD_REQUEST,
                json!({ "ok": false, "error": format!("Invalid JSON: {error}") }),
            )
        }
    };
    let tool = payload.get("tool").and_then(Value::as_str).unwrap_or_default();
    if !tool.starts_with("cirrus_") {
        return json_response(
            StatusCode::BAD_REQUEST,
            json!({ "ok": false, "error": format!("Unknown Cirrus tool: {tool}") }),
        );
    }
    if !service.tool_enabled(tool) {
        return json_response(
            StatusCode::FORBIDDEN,
            json!({ "ok": false, "error": format!("MCP tool is disabled in Cirrus: {tool}") }),
        );
    }
    let arguments = payload.get("arguments").cloned().unwrap_or_else(|| json!({}));
    if (tool == LOGIN_TOOL || tool == LIST_ACCOUNTS_TOOL)
        && arguments.as_object().map(|object| !object.is_empty()).unwrap_or(!arguments.is_null())
    {
        return json_response(
            StatusCode::BAD_REQUEST,
            json!({ "ok": false, "error": format!("{tool} does not accept arguments") }),
        );
    }
    if tool == LIST_ACCOUNTS_TOOL {
        return match service.visible_accounts() {
            Ok(result) => json_response(StatusCode::OK, json!({ "ok": true, "result": result })),
            Err(error) => json_response(StatusCode::INTERNAL_SERVER_ERROR, json!({ "ok": false, "error": error })),
        };
    }
    if tool != LOGIN_TOOL {
        return match crate::mcp_operations::call_tool(tool, &arguments, &app_state, &service, &app).await {
            Ok(result) => json_response(StatusCode::OK, json!({ "ok": true, "result": result })),
            Err(error) => json_response(StatusCode::BAD_REQUEST, json!({ "ok": false, "error": error })),
        };
    }

    let client_name = payload
        .get("client")
        .and_then(|value| value.get("name"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("MCP client")
        .to_string();
    let client_version =
        payload.get("client").and_then(|value| value.get("version")).and_then(Value::as_str).map(ToOwned::to_owned);

    let request_id = Uuid::new_v4().to_string();
    let (approval_tx, approval_rx) = oneshot::channel();
    let (login_tx, login_rx) = oneshot::channel();
    {
        let mut inner = service.inner.lock().await;
        if inner.pending_authorization.is_some() || inner.active_login.is_some() {
            return json_response(
                StatusCode::CONFLICT,
                json!({ "ok": false, "error": "Another Cirrus authorization or sign-in is already in progress" }),
            );
        }
        inner.pending_authorization = Some(PendingAuthorization {
            id: request_id.clone(),
            client_name: client_name.clone(),
            client_version: client_version.clone(),
            approval_tx,
            login_tx,
        });
    }

    show_main_window(&app);
    if let Err(error) = app.emit(
        "mcp-authorization-request",
        McpAuthorizationEvent {
            request_id: request_id.clone(),
            client_name: client_name.clone(),
            client_version,
            tool: LOGIN_TOOL.to_string(),
            expires_in_seconds: AUTHORIZATION_TIMEOUT.as_secs(),
        },
    ) {
        service.cancel_pending(&request_id).await;
        return json_response(
            StatusCode::SERVICE_UNAVAILABLE,
            json!({ "ok": false, "error": format!("Could not ask the Cirrus window for authorization: {error}") }),
        );
    }
    log::info!("[MCP] sign-in authorization requested by {client_name}");

    let approval = match tokio::time::timeout(AUTHORIZATION_TIMEOUT, approval_rx).await {
        Ok(Ok(value)) => value,
        Ok(Err(_)) => {
            service.cancel_pending(&request_id).await;
            let _ = app
                .emit("mcp-authorization-finished", McpAuthorizationFinishedEvent { request_id: request_id.clone() });
            return json_response(
                StatusCode::CONFLICT,
                json!({ "ok": false, "error": "The Cirrus authorization request was cancelled" }),
            );
        }
        Err(_) => {
            service.cancel_pending(&request_id).await;
            let _ = app
                .emit("mcp-authorization-finished", McpAuthorizationFinishedEvent { request_id: request_id.clone() });
            return json_response(
                StatusCode::REQUEST_TIMEOUT,
                json!({ "ok": false, "error": "Cirrus authorization timed out" }),
            );
        }
    };

    if !approval {
        return json_response(
            StatusCode::OK,
            json!({ "ok": true, "result": LoginOutcome::denied("The user denied the Cirrus sign-in request") }),
        );
    }

    let outcome = match tokio::time::timeout(APP_LOGIN_TIMEOUT, login_rx).await {
        Ok(Ok(value)) => value,
        Ok(Err(_)) => LoginOutcome::error("The Cirrus sign-in request ended unexpectedly"),
        Err(_) => {
            service.cancel_active(&request_id, "Cirrus sign-in timed out").await;
            let _ = app.emit("mcp-login-finished", McpLoginFinishedEvent { request_id: request_id.clone() });
            LoginOutcome::error("Cirrus sign-in timed out")
        }
    };
    if let Some(message) = &outcome.message {
        log::info!("[MCP] sign-in request finished: {message}");
    } else {
        log::info!("[MCP] sign-in request completed successfully");
    }
    let _ = app_state;
    json_response(StatusCode::OK, json!({ "ok": true, "result": outcome }))
}

fn show_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}

fn authorized_bearer(request: &Request<hyper::body::Incoming>, expected: &str) -> bool {
    let Some(value) = request.headers().get(header::AUTHORIZATION).and_then(|value| value.to_str().ok()) else {
        return false;
    };
    let Some(token) = value.strip_prefix("Bearer ") else {
        return false;
    };
    token == expected
}

async fn read_body(request: Request<hyper::body::Incoming>) -> Result<Vec<u8>, String> {
    let size_hint = request.body().size_hint().upper().unwrap_or(0);
    if size_hint > MAX_HTTP_BODY as u64 {
        return Err("Request body is too large".to_string());
    }
    let body = request
        .into_body()
        .collect()
        .await
        .map_err(|error| format!("Could not read request body: {error}"))?
        .to_bytes();
    if body.len() > MAX_HTTP_BODY {
        return Err("Request body is too large".to_string());
    }
    Ok(body.to_vec())
}

fn json_response(status: StatusCode, body: Value) -> Response<Full<Bytes>> {
    let bytes = serde_json::to_vec(&body)
        .unwrap_or_else(|_| b"{\"ok\":false,\"error\":\"Could not encode response\"}".to_vec());
    let mut response = Response::new(Full::new(Bytes::from(bytes)));
    *response.status_mut() = status;
    response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json; charset=utf-8"));
    response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
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

fn load_tool_access(data_dir: &Path) -> McpToolAccessSettings {
    std::fs::read_to_string(data_dir.join(TOOL_ACCESS_FILE))
        .ok()
        .and_then(|value| serde_json::from_str(&value).ok())
        .unwrap_or_default()
}

fn load_account_access(data_dir: &Path) -> McpAccountAccessSettings {
    std::fs::read_to_string(data_dir.join(ACCOUNT_ACCESS_FILE))
        .ok()
        .and_then(|value| serde_json::from_str(&value).ok())
        .unwrap_or_default()
}

fn mask_apple_id(apple_id: &str) -> String {
    let trimmed = apple_id.trim();
    let Some((local, domain)) = trimmed.split_once('@') else {
        return if trimmed.len() <= 2 { "***".to_string() } else { format!("{}***", &trimmed[..1]) };
    };
    let first = local.chars().next().unwrap_or('*');
    format!("{first}***@{domain}")
}

fn escape_toml_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

pub(crate) fn mcp_binary_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let file_name = if cfg!(windows) { "cirrus-mcp.exe" } else { "cirrus-mcp" };
    Some(dir.join(file_name))
}

fn settings_path(data_dir: &Path) -> PathBuf {
    data_dir.join(SETTINGS_FILE)
}

fn load_enabled_setting(data_dir: &Path) -> bool {
    let path = settings_path(data_dir);
    std::fs::read_to_string(path)
        .ok()
        .and_then(|value| serde_json::from_str::<McpSettings>(&value).ok())
        .map(|settings| settings.enabled)
        .unwrap_or(true)
}

fn save_enabled_setting(data_dir: Option<&Path>, enabled: bool) {
    let Some(data_dir) = data_dir else {
        return;
    };
    if let Err(error) = std::fs::create_dir_all(data_dir) {
        log::warn!("[MCP] could not create the app data directory: {error}");
        return;
    }
    let value = serde_json::to_vec_pretty(&McpSettings { enabled }).unwrap_or_default();
    if let Err(error) = write_private_file(&settings_path(data_dir), &value) {
        log::warn!("[MCP] could not save the MCP setting: {error}");
    }
}

fn write_private_file(path: &Path, value: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new().create(true).truncate(true).write(true).mode(0o600).open(path)?;
        file.write_all(value)?;
        file.flush()?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, value)
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
    fn toml_escaping_handles_windows_paths() {
        assert_eq!(escape_toml_string(r#"C:\Users\me\cirrus-mcp.exe"#), r#"C:\\Users\\me\\cirrus-mcp.exe"#);
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
    fn bridge_is_enabled_by_default_for_new_installs() {
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
    fn mcp_binary_uses_the_app_sibling_name() {
        let path = mcp_binary_path().expect("binary path");
        let name = path.file_name().and_then(|value| value.to_str()).unwrap_or_default();
        assert_eq!(name, if cfg!(windows) { "cirrus-mcp.exe" } else { "cirrus-mcp" });
    }
}
