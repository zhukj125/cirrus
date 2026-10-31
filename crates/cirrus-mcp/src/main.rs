use std::{
    io::{self, BufRead, Write},
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    sync::Arc,
};

use axum::{
    extract::{DefaultBodyLimit, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use reqwest::Url;
use serde_json::{json, Value};
use tokio::net::TcpListener;

const LATEST_PROTOCOL_VERSION: &str = "2025-06-18";
const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
const LOGIN_TOOL: &str = "cirrus_login";
const LIST_ACCOUNTS_TOOL: &str = "cirrus_list_accounts";
const CLIENT_NAME: &str = "cirrus-mcp";
const MAX_DEVICE_IMAGE_BYTES: usize = 2 * 1024 * 1024;
const DEFAULT_HTTP_HOST: &str = "127.0.0.1";
const DEFAULT_HTTP_PORT: u16 = 5226;
const DEFAULT_HTTP_PATH: &str = "/mcp";

#[derive(Debug, Clone, Default)]
struct ClientIdentity {
    name: Option<String>,
    version: Option<String>,
}

enum RuntimeMode {
    Stdio,
    Http(HttpConfig),
}

struct HttpConfig {
    bind_addr: SocketAddr,
    path: String,
    token: String,
    allow_remote: bool,
}

#[derive(Clone)]
struct HttpState {
    token: Arc<str>,
    allow_remote: bool,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("cirrus-mcp");
        println!();
        println!("MCP bridge for Cirrus.");
        println!();
        println!("Usage:");
        println!("  cirrus-mcp                         Start stdio transport");
        println!("  cirrus-mcp --http                  Start Streamable HTTP transport");
        println!("  cirrus-mcp --http --http-port 5226 Start HTTP on a custom port");
        println!();
        println!("HTTP authentication uses CIRRUS_MCP_HTTP_TOKEN or CIRRUS_MCP_HTTP_TOKEN_FILE.");
        return;
    }
    if args.iter().any(|arg| arg == "--version" || arg == "-V") {
        println!("cirrus-mcp {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let mode = match runtime_mode(args) {
        Ok(mode) => mode,
        Err(error) => {
            eprintln!("cirrus-mcp: {error}");
            std::process::exit(2);
        }
    };
    let result = match mode {
        RuntimeMode::Stdio => run_stdio().await,
        RuntimeMode::Http(config) => run_http(config).await,
    };
    if let Err(error) = result {
        eprintln!("cirrus-mcp: {error}");
        std::process::exit(1);
    }
}

async fn run_stdio() -> Result<(), String> {
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    let mut client = ClientIdentity::default();

    for line in stdin.lock().lines() {
        let line = line.map_err(|error| format!("could not read stdin: {error}"))?;
        if line.trim().is_empty() {
            continue;
        }
        let response = handle_line(&line, &mut client).await;
        let Some(response) = response else {
            continue;
        };
        let encoded =
            serde_json::to_string(&response).map_err(|error| format!("could not encode response: {error}"))?;
        stdout.write_all(encoded.as_bytes()).map_err(|error| format!("could not write stdout: {error}"))?;
        stdout.write_all(b"\n").map_err(|error| format!("could not write stdout: {error}"))?;
        stdout.flush().map_err(|error| format!("could not flush stdout: {error}"))?;
    }
    Ok(())
}

fn runtime_mode(args: Vec<String>) -> Result<RuntimeMode, String> {
    let mut transport = std::env::var("CIRRUS_MCP_TRANSPORT").ok();
    let mut host = std::env::var("CIRRUS_MCP_HTTP_HOST").ok();
    let mut port = std::env::var("CIRRUS_MCP_HTTP_PORT").ok();
    let mut path = std::env::var("CIRRUS_MCP_HTTP_PATH").ok();
    let mut token = std::env::var("CIRRUS_MCP_HTTP_TOKEN").ok();
    let mut token_file = std::env::var_os("CIRRUS_MCP_HTTP_TOKEN_FILE").map(PathBuf::from);
    let mut allow_remote = env_flag("CIRRUS_MCP_HTTP_ALLOW_REMOTE")?;
    let mut explicit_allow_remote = false;

    let mut arguments = args.into_iter();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--stdio" => transport = Some("stdio".into()),
            "--http" => transport = Some("streamable-http".into()),
            "--http-allow-remote" => {
                allow_remote = true;
                explicit_allow_remote = true;
            }
            "--transport" => transport = Some(required_argument(&mut arguments, "--transport")?),
            "--http-host" => host = Some(required_argument(&mut arguments, "--http-host")?),
            "--http-port" => port = Some(required_argument(&mut arguments, "--http-port")?),
            "--http-path" => path = Some(required_argument(&mut arguments, "--http-path")?),
            "--http-token" => token = Some(required_argument(&mut arguments, "--http-token")?),
            "--http-token-file" => {
                token_file = Some(PathBuf::from(required_argument(&mut arguments, "--http-token-file")?));
            }
            _ => {
                if let Some(value) = argument.strip_prefix("--transport=") {
                    transport = Some(value.into());
                } else if let Some(value) = argument.strip_prefix("--http-host=") {
                    host = Some(value.into());
                } else if let Some(value) = argument.strip_prefix("--http-port=") {
                    port = Some(value.into());
                } else if let Some(value) = argument.strip_prefix("--http-path=") {
                    path = Some(value.into());
                } else if let Some(value) = argument.strip_prefix("--http-token=") {
                    token = Some(value.into());
                } else if let Some(value) = argument.strip_prefix("--http-token-file=") {
                    token_file = Some(PathBuf::from(value));
                } else {
                    return Err(format!("unknown argument: {argument}"));
                }
            }
        }
    }

    let transport = transport.unwrap_or_else(|| "stdio".into());
    match transport.trim().to_ascii_lowercase().as_str() {
        "stdio" => Ok(RuntimeMode::Stdio),
        "http" | "streamable-http" | "streamable_http" => {
            let host = host.unwrap_or_else(|| DEFAULT_HTTP_HOST.into());
            let ip = host.parse::<IpAddr>().map_err(|_| "HTTP host must be an IP address".to_string())?;
            let port = port
                .unwrap_or_else(|| DEFAULT_HTTP_PORT.to_string())
                .parse::<u16>()
                .map_err(|_| "HTTP port must be a valid TCP port".to_string())?;
            if port == 0 {
                return Err("HTTP port must be between 1 and 65535".to_string());
            }
            let path = path.unwrap_or_else(|| DEFAULT_HTTP_PATH.into());
            validate_http_path(&path)?;
            if !ip.is_loopback() && !(allow_remote && explicit_allow_remote) {
                return Err(
                    "non-loopback HTTP binding requires CIRRUS_MCP_HTTP_ALLOW_REMOTE=1 and --http-allow-remote".into(),
                );
            }
            if token.is_some() && token_file.is_some() {
                return Err("set only one of HTTP token and token file".into());
            }
            let token = http_token(token, token_file)?;
            Ok(RuntimeMode::Http(HttpConfig {
                bind_addr: SocketAddr::new(ip, port),
                path,
                token,
                allow_remote: !ip.is_loopback(),
            }))
        }
        _ => Err("CIRRUS_MCP_TRANSPORT must be stdio or streamable-http".into()),
    }
}

fn required_argument(arguments: &mut impl Iterator<Item = String>, name: &str) -> Result<String, String> {
    arguments.next().filter(|value| !value.starts_with("--")).ok_or_else(|| format!("{name} requires a value"))
}

fn env_flag(name: &str) -> Result<bool, String> {
    match std::env::var(name) {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" => Ok(true),
            "0" | "false" | "no" | "" => Ok(false),
            _ => Err(format!("{name} must be true or false")),
        },
        Err(std::env::VarError::NotPresent) => Ok(false),
        Err(std::env::VarError::NotUnicode(_)) => Err(format!("{name} must be valid UTF-8")),
    }
}

fn validate_http_path(path: &str) -> Result<(), String> {
    if path == "/" || !path.starts_with('/') || path.ends_with('/') || path.contains('?') || path.contains('#') {
        return Err("HTTP path must be an absolute path such as /mcp".to_string());
    }
    Ok(())
}

fn http_token(token: Option<String>, token_file: Option<PathBuf>) -> Result<String, String> {
    if let Some(token) = token {
        let token = token.trim().to_string();
        if token.is_empty() {
            return Err("HTTP token must not be empty".to_string());
        }
        return Ok(token);
    }
    let path = token_file
        .or_else(|| app_data_dir().map(|dir| dir.join("mcp-bridge-token")))
        .ok_or_else(|| "HTTP token is not configured".to_string())?;
    let token = std::fs::read_to_string(&path)
        .map_err(|error| format!("could not read HTTP token from {}: {error}", path.display()))?;
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err("HTTP token file is empty".to_string());
    }
    Ok(token)
}

async fn run_http(config: HttpConfig) -> Result<(), String> {
    let listener = TcpListener::bind(config.bind_addr)
        .await
        .map_err(|error| format!("could not bind HTTP service at {}: {error}", config.bind_addr))?;
    let local_addr = listener.local_addr().map_err(|error| format!("could not read HTTP address: {error}"))?;
    let state = HttpState { token: Arc::from(config.token), allow_remote: config.allow_remote };
    let router = Router::new()
        .route("/healthz", get(http_health))
        .route(&config.path, post(http_message).get(http_stream).delete(http_delete).options(http_options))
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024))
        .with_state(state);
    eprintln!("cirrus-mcp Streamable HTTP listening on http://{local_addr}{}", config.path);
    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .map_err(|error| format!("HTTP service failed: {error}"))
}

async fn http_health() -> Response {
    with_http_headers(
        Json(json!({
            "status": "ok",
            "transport": "streamable-http",
            "version": env!("CARGO_PKG_VERSION")
        }))
        .into_response(),
    )
}

async fn http_message(State(state): State<HttpState>, headers: HeaderMap, Json(payload): Json<Value>) -> Response {
    if let Err(response) = authorize_http(&state, &headers) {
        return with_http_headers(*response);
    }
    let mut client = ClientIdentity::default();
    match handle_message(payload, &mut client).await {
        Some(response) => http_json(response),
        None => with_http_headers(StatusCode::ACCEPTED.into_response()),
    }
}

async fn http_stream(State(state): State<HttpState>, headers: HeaderMap) -> Response {
    if let Err(response) = authorize_http(&state, &headers) {
        return with_http_headers(*response);
    }
    with_http_headers(
        (
            StatusCode::METHOD_NOT_ALLOWED,
            [(header::ALLOW, HeaderValue::from_static("POST, DELETE, OPTIONS"))],
            "Server-initiated SSE streams are not used by Cirrus MCP.",
        )
            .into_response(),
    )
}

async fn http_delete(State(state): State<HttpState>, headers: HeaderMap) -> Response {
    if let Err(response) = authorize_http(&state, &headers) {
        return with_http_headers(*response);
    }
    with_http_headers(StatusCode::NO_CONTENT.into_response())
}

async fn http_options() -> Response {
    with_http_headers(StatusCode::NO_CONTENT.into_response())
}

fn authorize_http(state: &HttpState, headers: &HeaderMap) -> Result<(), Box<Response>> {
    if !state.allow_remote && !host_is_loopback(headers) {
        return Err(Box::new(with_http_headers(
            (StatusCode::FORBIDDEN, "MCP HTTP Host header is not allowed").into_response(),
        )));
    }
    let candidate = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or_default();
    if token_matches(state.token.as_bytes(), candidate) {
        Ok(())
    } else {
        Err(Box::new(with_http_headers(
            (
                StatusCode::UNAUTHORIZED,
                [(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"))],
                "Unauthorized",
            )
                .into_response(),
        )))
    }
}

fn token_matches(expected: &[u8], candidate: &str) -> bool {
    let candidate = candidate.as_bytes();
    if expected.len() != candidate.len() {
        return false;
    }
    expected.iter().zip(candidate).fold(0_u8, |difference, (left, right)| difference | (left ^ right)) == 0
}

fn host_is_loopback(headers: &HeaderMap) -> bool {
    let Some(host) = headers.get(header::HOST).and_then(|value| value.to_str().ok()) else {
        return true;
    };
    let host = host.trim().to_ascii_lowercase();
    host == "localhost"
        || host == "127.0.0.1"
        || host == "[::1]"
        || host == "::1"
        || host.starts_with("localhost:")
        || host.starts_with("127.0.0.1:")
        || host.starts_with("[::1]:")
}

fn http_json(value: Value) -> Response {
    let mut response = Json(value).into_response();
    response.headers_mut().insert("MCP-Protocol-Version", HeaderValue::from_static("2025-06-18"));
    with_http_headers(response)
}

fn with_http_headers(mut response: Response) -> Response {
    let headers = response.headers_mut();
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("Authorization, Content-Type, Accept, MCP-Protocol-Version"),
    );
    headers.insert(header::ACCESS_CONTROL_ALLOW_METHODS, HeaderValue::from_static("POST, GET, DELETE, OPTIONS"));
    response
}

async fn handle_line(line: &str, client: &mut ClientIdentity) -> Option<Value> {
    let payload: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(error) => {
            return Some(error_response(Value::Null, -32700, format!("Parse error: {error}")));
        }
    };

    if let Value::Array(items) = payload {
        let mut responses = Vec::new();
        for item in items {
            if let Some(response) = handle_message(item, client).await {
                responses.push(response);
            }
        }
        return if responses.is_empty() { None } else { Some(Value::Array(responses)) };
    }

    handle_message(payload, client).await
}

async fn handle_message(payload: Value, client: &mut ClientIdentity) -> Option<Value> {
    let id = payload.get("id").cloned();
    let Some(method) = payload.get("method").and_then(Value::as_str) else {
        return id.map(|id| error_response(id, -32600, "Invalid Request"));
    };

    if method.starts_with("notifications/") {
        return None;
    }

    let id = id?;
    let params = payload.get("params").cloned().unwrap_or(Value::Null);

    match method {
        "initialize" => {
            if let Some(info) = params.get("clientInfo") {
                client.name = info.get("name").and_then(Value::as_str).map(ToOwned::to_owned);
                client.version = info.get("version").and_then(Value::as_str).map(ToOwned::to_owned);
            }
            let requested = params.get("protocolVersion").and_then(Value::as_str);
            let protocol = requested
                .filter(|version| SUPPORTED_PROTOCOL_VERSIONS.contains(version))
                .unwrap_or(LATEST_PROTOCOL_VERSION);
            Some(success_response(
                id,
                json!({
                    "protocolVersion": protocol,
                    "capabilities": {
                        "tools": { "listChanged": false }
                    },
                    "serverInfo": {
                        "name": CLIENT_NAME,
                        "version": env!("CARGO_PKG_VERSION")
                    },
                    "instructions": "Cirrus exposes iCloud sign-in and authorized-account listing. Never ask the user to put an Apple ID password in chat or tool arguments. Call cirrus_login, ask the user to approve the request in Cirrus, and let them enter credentials directly in the Cirrus sign-in window that opens. If the result reports mcpEnabled=false, tell the user to enable that account in Cirrus Settings > MCP before requesting further iCloud operations. Use cirrus_list_accounts after sign-in to discover the accounts currently allowed for MCP."
                }),
            ))
        }
        "ping" | "logging/setLevel" => Some(success_response(id, json!({}))),
        "tools/list" => Some(success_response(id, tools_list())),
        "tools/call" => Some(handle_tool_call(id, params, client).await),
        _ => Some(error_response(id, -32601, format!("Method not found: {method}"))),
    }
}

fn tools_list() -> Value {
    json!({
        "tools": [
            {
                "name": LOGIN_TOOL,
                "title": "Sign in to iCloud with Cirrus",
                "description": "Ask the user to approve an iCloud sign-in in the Cirrus desktop app, then open the Cirrus sign-in window so the user can enter their Apple ID, password, and two-factor code privately. This tool does not accept credentials and waits for the Cirrus sign-in flow to finish.",
                "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
                "annotations": { "title": "Sign in to iCloud with Cirrus", "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": true }
            },
            {
                "name": LIST_ACCOUNTS_TOOL,
                "title": "List MCP-enabled iCloud accounts",
                "description": "List iCloud accounts that are allowed for MCP operations. Accounts disabled in Cirrus Settings > MCP are omitted.",
                "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
                "annotations": { "title": "List MCP-enabled iCloud accounts", "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
            },
            {
                "name": "cirrus_account_status",
                "title": "Get iCloud account status",
                "description": "Return verification and sign-in status for an MCP-enabled Cirrus account.",
                "inputSchema": account_schema(json!({})),
                "annotations": { "title": "Get iCloud account status", "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
            },
            {
                "name": "cirrus_logout",
                "title": "Sign out of an iCloud account",
                "description": "Remove the local Cirrus session and mark the selected account as signed out. This does not change the Apple account.",
                "inputSchema": account_schema(json!({})),
                "annotations": { "title": "Sign out of an iCloud account", "readOnlyHint": false, "destructiveHint": true, "idempotentHint": false, "openWorldHint": false }
            },
            {
                "name": "cirrus_list_devices",
                "title": "List Find My devices",
                "description": "List Apple devices, address-resolved locations, battery status, and Find My state for an account. Device artwork is returned as MCP image content when available.",
                "inputSchema": account_schema(json!({})),
                "annotations": { "title": "List Find My devices", "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true }
            },
            {
                "name": "cirrus_play_sound",
                "title": "Play sound on an Apple device",
                "description": "Ask Apple Find My to play a sound on the selected device.",
                "inputSchema": account_schema_required(json!({
                    "deviceId": { "type": "string", "description": "Find My deviceId returned by cirrus_list_devices." }
                }), &["deviceId"]),
                "annotations": { "title": "Play sound on an Apple device", "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": true }
            },
            {
                "name": "cirrus_list_photos",
                "title": "List iCloud Photos",
                "description": "List iCloud Photos assets with metadata, thumbnail URLs, and original media URLs.",
                "inputSchema": account_schema(json!({
                    "cursor": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 200, "default": 20 },
                    "startRank": { "type": "integer", "minimum": 0 }
                })),
                "annotations": { "title": "List iCloud Photos", "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true }
            },
            {
                "name": "cirrus_photos_total",
                "title": "Get iCloud Photos totals",
                "description": "Return total photo, video, and asset counts for an iCloud Photo Library.",
                "inputSchema": account_schema(json!({})),
                "annotations": { "title": "Get iCloud Photos totals", "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true }
            },
            {
                "name": "cirrus_photos_timeline",
                "title": "Get iCloud Photos timeline",
                "description": "Return the library timeline distribution grouped by year and month.",
                "inputSchema": account_schema(json!({})),
                "annotations": { "title": "Get iCloud Photos timeline", "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true }
            },
            {
                "name": "cirrus_photos_month_exact",
                "title": "Get exact iCloud Photos month count",
                "description": "Return the exact photo count and rank range for a year and month.",
                "inputSchema": account_schema_required(json!({
                    "year": { "type": "integer", "minimum": 1 },
                    "month": { "type": "integer", "minimum": 1, "maximum": 12 }
                }), &["year", "month"]),
                "annotations": { "title": "Get exact iCloud Photos month count", "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true }
            },
            {
                "name": "cirrus_download_photo",
                "title": "Download an iCloud photo or video",
                "description": "Download an original iCloud media URL to an absolute local path. Only Apple media hosts are accepted.",
                "inputSchema": account_schema_required(json!({
                    "url": { "type": "string", "description": "Original media URL returned by cirrus_list_photos." },
                    "photoId": { "type": "string", "description": "Optional photo ID used in download history." },
                    "name": { "type": "string" },
                    "targetPath": { "type": "string", "description": "Absolute destination file path." },
                    "transferId": { "type": "string", "description": "Optional caller-generated ID used for progress and cancellation." },
                    "partialKey": { "type": "string", "description": "Optional stable key used for resumable partial files." }
                }), &["url", "targetPath"]),
                "annotations": { "title": "Download an iCloud photo or video", "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true }
            },
            {
                "name": "cirrus_cancel_download",
                "title": "Cancel an MCP download",
                "description": "Cancel a download started with cirrus_download_photo by its transferId.",
                "inputSchema": account_schema_required(json!({
                    "transferId": { "type": "string", "description": "Transfer ID supplied to cirrus_download_photo." }
                }), &["transferId"]),
                "annotations": { "title": "Cancel an MCP download", "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
            },
            {
                "name": "cirrus_list_download_tasks",
                "title": "List MCP download history",
                "description": "List persisted download tasks for the selected iCloud account.",
                "inputSchema": account_schema(json!({})),
                "annotations": { "title": "List MCP download history", "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
            },
            {
                "name": "cirrus_get_account_avatar",
                "title": "Get iCloud account avatar",
                "description": "Return the cached or fetched Apple account avatar as a data URL.",
                "inputSchema": account_schema(json!({})),
                "annotations": { "title": "Get iCloud account avatar", "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true }
            },
            {
                "name": "cirrus_get_contact_photo",
                "title": "Get an iCloud contact photo",
                "description": "Fetch an Apple-hosted contact photo URL as a data URL.",
                "inputSchema": account_schema_required(json!({
                    "url": { "type": "string", "description": "Contact photoUrl returned by cirrus_list_contacts." }
                }), &["url"]),
                "annotations": { "title": "Get an iCloud contact photo", "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true }
            },
            {
                "name": "cirrus_list_notes",
                "title": "List iCloud Notes",
                "description": "List folders and notes visible to the selected iCloud account.",
                "inputSchema": account_schema(json!({ "cursor": { "type": "string" } })),
                "annotations": { "title": "List iCloud Notes", "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true }
            },
            {
                "name": "cirrus_get_note",
                "title": "Read an iCloud Note",
                "description": "Return the full title, body, HTML, and attachments for a note.",
                "inputSchema": account_schema_required(json!({
                    "noteId": { "type": "string", "description": "Note ID returned by cirrus_list_notes." }
                }), &["noteId"]),
                "annotations": { "title": "Read an iCloud Note", "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true }
            },
            {
                "name": "cirrus_list_contacts",
                "title": "List iCloud Contacts",
                "description": "List contacts, phone numbers, email addresses, postal addresses, and other contact fields.",
                "inputSchema": account_schema(json!({ "locale": { "type": "string", "default": "zh-CN" } })),
                "annotations": { "title": "List iCloud Contacts", "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true }
            },
            {
                "name": "cirrus_load_calendar",
                "title": "Load iCloud Calendar events",
                "description": "Load calendars and events in a date range.",
                "inputSchema": account_schema_required(json!({
                    "startDate": { "type": "string", "description": "Inclusive start date, YYYY-MM-DD." },
                    "endDate": { "type": "string", "description": "Inclusive end date, YYYY-MM-DD." },
                    "timezone": { "type": "string", "default": "Asia/Shanghai" },
                    "locale": { "type": "string", "default": "zh-CN" }
                }), &["startDate", "endDate"]),
                "annotations": { "title": "Load iCloud Calendar events", "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true }
            }
        ]
    })
}

fn account_schema(extra: Value) -> Value {
    account_schema_required(extra, &[])
}

fn account_schema_required(extra: Value, extra_required: &[&str]) -> Value {
    let mut properties = match extra {
        Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    properties.insert(
        "accountKey".to_string(),
        json!({
            "type": "string",
            "description": "Account key returned by cirrus_list_accounts."
        }),
    );
    let mut required = vec![json!("accountKey")];
    required.extend(extra_required.iter().map(|value| json!(value)));
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    })
}

async fn handle_tool_call(id: Value, params: Value, client: &ClientIdentity) -> Value {
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return error_response(id, -32602, "Missing tool name");
    };
    let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
    if (name == LOGIN_TOOL || name == LIST_ACCOUNTS_TOOL)
        && arguments.as_object().map(|object| !object.is_empty()).unwrap_or(!arguments.is_null())
    {
        return success_response(
            id,
            json!({
                "content": [{ "type": "text", "text": format!("{name} does not accept arguments.") }],
                "isError": true
            }),
        );
    }

    match call_bridge(name, &arguments, client).await {
        Ok(mut result) => {
            let mut content = result_image_content(name, &mut result).await;
            let text_result = result_without_inline_artwork(&result);
            content.push(json!({
                "type": "text",
                "text": serde_json::to_string_pretty(&text_result).unwrap_or_else(|_| text_result.to_string())
            }));
            success_response(
                id,
                json!({
                    "content": content,
                    "structuredContent": result,
                    "isError": false
                }),
            )
        }
        Err(message) => success_response(
            id,
            json!({
                "content": [{ "type": "text", "text": message }],
                "isError": true
            }),
        ),
    }
}

fn is_apple_device_image_url(value: &str) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    matches!(url.scheme(), "https")
        && url.host_str().is_some_and(|host| {
            host == "icloud.com"
                || host.ends_with(".icloud.com")
                || host == "icloud.com.cn"
                || host.ends_with(".icloud.com.cn")
        })
}

fn device_label(device: &Value) -> String {
    let name = device.get("name").and_then(Value::as_str).unwrap_or("Apple device");
    let model = device
        .get("deviceDisplayName")
        .and_then(Value::as_str)
        .or_else(|| device.get("modelDisplayName").and_then(Value::as_str))
        .unwrap_or("");
    if model.is_empty() {
        name.to_string()
    } else {
        format!("{name} · {model}")
    }
}

fn device_summary_text(device: &Value) -> String {
    let location = match device.get("location").filter(|value| !value.is_null()) {
        Some(location) => {
            let latitude = location.get("latitude").and_then(Value::as_f64);
            let longitude = location.get("longitude").and_then(Value::as_f64);
            match (latitude, longitude) {
                (Some(latitude), Some(longitude)) => {
                    let accuracy = location
                        .get("horizontalAccuracy")
                        .and_then(Value::as_f64)
                        .filter(|value| value.is_finite() && *value >= 0.0)
                        .map(|value| format!(" (±{value:.0} m)"))
                        .unwrap_or_default();
                    let stale = location.get("isOld").and_then(Value::as_bool).unwrap_or(false);
                    format!("Location: {latitude:.6}, {longitude:.6}{accuracy}{}", if stale { " · stale" } else { "" })
                }
                _ => "Location: unavailable".to_string(),
            }
        }
        None if device.get("locationEnabled").and_then(Value::as_bool) == Some(false) => {
            "Location: services off".to_string()
        }
        None => "Location: unavailable".to_string(),
    };
    let address = device
        .get("location")
        .and_then(|location| location.get("address"))
        .and_then(|address| address.get("short").or_else(|| address.get("full")))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(|value| format!("Address: {value}"));
    let status = match device.get("deviceStatus").and_then(Value::as_str) {
        Some("200") => "Status: online",
        _ => "Status: offline",
    };
    let battery = device
        .get("batteryLevel")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .map(|value| format!("Battery: {:.0}%", value.clamp(0.0, 1.0) * 100.0));
    [Some(device_label(device)), Some(location), address, Some(status.to_string()), battery]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n")
}
fn preferred_device_image_url(url: &str) -> String {
    if url.ends_with("online-sourcelist.png") {
        url.trim_end_matches("online-sourcelist.png").to_string() + "online-infobox__3x.png"
    } else if url.ends_with("offline-sourcelist.png") {
        url.trim_end_matches("offline-sourcelist.png").to_string() + "offline-infobox__3x.png"
    } else {
        url.to_string()
    }
}

async fn fetch_device_image(client: &reqwest::Client, url: &str) -> Result<(Vec<u8>, String), String> {
    if !is_apple_device_image_url(url) {
        return Err("device artwork URL was rejected".to_string());
    }
    let response = client
        .get(url)
        .header(reqwest::header::ACCEPT, "image/*")
        .timeout(std::time::Duration::from_secs(12))
        .send()
        .await
        .map_err(|error| format!("could not load device artwork: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("device artwork returned HTTP {}", response.status()));
    }
    if response.content_length().is_some_and(|length| length as usize > MAX_DEVICE_IMAGE_BYTES) {
        return Err("device artwork is too large".to_string());
    }
    let mime_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .filter(|value| value.starts_with("image/"))
        .unwrap_or("image/png")
        .to_string();
    let bytes = response.bytes().await.map_err(|error| format!("could not read device artwork: {error}"))?;
    if bytes.len() > MAX_DEVICE_IMAGE_BYTES {
        return Err("device artwork is too large".to_string());
    }
    Ok((bytes.to_vec(), mime_type))
}

async fn device_image_content(
    client: &reqwest::Client,
    device: &Value,
) -> (Vec<Value>, Option<(String, String)>, Option<String>) {
    let summary = device_summary_text(device);
    let Some(url) = device.get("deviceImageUrl").and_then(Value::as_str) else {
        return (
            vec![json!({ "type": "text", "text": format!("{summary}\nDevice artwork: unavailable") })],
            None,
            Some("unavailable".to_string()),
        );
    };
    let preferred = preferred_device_image_url(url);
    let image = match fetch_device_image(client, &preferred).await {
        Ok(image) => Ok(image),
        Err(preferred_error) if preferred != url => fetch_device_image(client, url)
            .await
            .map_err(|fallback_error| format!("{preferred_error}; fallback failed: {fallback_error}")),
        Err(error) => Err(error),
    };
    let (bytes, mime_type) = match image {
        Ok(image) => image,
        Err(error) => {
            return (
                vec![json!({ "type": "text", "text": format!("{summary}\nDevice artwork: {error}") })],
                None,
                Some(error),
            );
        }
    };
    let encoded = BASE64.encode(bytes);
    let data_url = format!("data:{mime_type};base64,{encoded}");
    (
        vec![
            json!({ "type": "text", "text": summary }),
            json!({ "type": "image", "data": encoded, "mimeType": mime_type }),
        ],
        Some((mime_type, data_url)),
        None,
    )
}

async fn result_image_content(tool: &str, result: &mut Value) -> Vec<Value> {
    if tool != "cirrus_list_devices" {
        return Vec::new();
    }
    let Some(devices) = result.get_mut("devices").and_then(Value::as_array_mut) else {
        return Vec::new();
    };
    let Ok(client) = reqwest::Client::builder().timeout(std::time::Duration::from_secs(15)).build() else {
        return Vec::new();
    };
    let mut content = Vec::new();
    for device in devices {
        let (items, artwork, error) = device_image_content(&client, device).await;
        content.extend(items);
        if let Some(device) = device.as_object_mut() {
            if let Some((mime_type, data_url)) = artwork {
                device.insert("deviceImageMimeType".to_string(), json!(mime_type));
                device.insert("deviceImageDataUrl".to_string(), json!(data_url));
            }
            if let Some(error) = error {
                device.insert("deviceImageError".to_string(), json!(error));
            }
        }
    }
    content
}

fn result_without_inline_artwork(result: &Value) -> Value {
    let mut visible = result.clone();
    if let Some(devices) = visible.get_mut("devices").and_then(Value::as_array_mut) {
        for device in devices {
            if let Some(device) = device.as_object_mut() {
                device.remove("deviceImageDataUrl");
            }
        }
    }
    visible
}

#[derive(Debug)]
struct BridgeTarget {
    base_url: String,
    token: String,
}

async fn call_bridge(tool: &str, arguments: &Value, client: &ClientIdentity) -> Result<Value, String> {
    let target = discover_bridge()?;
    let response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(13 * 60))
        .build()
        .map_err(|error| format!("Could not create the Cirrus MCP client: {error}"))?
        .post(format!("{}/v1/call", target.base_url))
        .bearer_auth(&target.token)
        .json(&json!({
            "tool": tool,
            "arguments": arguments,
            "client": {
                "name": client.name.clone().unwrap_or_else(|| "MCP client".to_string()),
                "version": client.version
            }
        }))
        .send()
        .await
        .map_err(|error| {
            if error.is_connect() {
                "Cirrus is not running or the MCP bridge is disabled. Open Cirrus and enable MCP in Settings."
                    .to_string()
            } else {
                format!("Could not reach Cirrus: {error}")
            }
        })?;

    let status = response.status();
    let body = response.text().await.map_err(|error| format!("Could not read the Cirrus response: {error}"))?;
    let value: Value =
        serde_json::from_str(&body).map_err(|error| format!("Cirrus returned an invalid response: {error}"))?;
    if !status.is_success() {
        let message = value
            .get("error")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("Cirrus returned HTTP {}", status.as_u16()));
        return Err(message);
    }
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("Cirrus did not return a login result")
            .to_string());
    }
    Ok(value.get("result").cloned().unwrap_or(Value::Null))
}

fn discover_bridge() -> Result<BridgeTarget, String> {
    if let Ok(url) = std::env::var("CIRRUS_MCP_URL") {
        let token =
            std::env::var("CIRRUS_MCP_TOKEN").map_err(|_| "CIRRUS_MCP_TOKEN is required when CIRRUS_MCP_URL is set")?;
        return Ok(BridgeTarget { base_url: url.trim_end_matches('/').to_string(), token });
    }

    let data_dir =
        app_data_dir().ok_or_else(|| "Could not resolve the Cirrus application data directory".to_string())?;
    let port_file =
        std::env::var_os("CIRRUS_MCP_PORT_FILE").map(PathBuf::from).unwrap_or_else(|| data_dir.join("mcp-bridge-port"));
    let token_file = std::env::var_os("CIRRUS_MCP_TOKEN_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| data_dir.join("mcp-bridge-token"));
    let port = read_trimmed(&port_file)
        .ok_or_else(|| "Cirrus MCP bridge is not running. Open Cirrus and enable MCP in Settings.".to_string())?;
    let port: u16 = port.parse().map_err(|_| "Cirrus MCP bridge port file is invalid".to_string())?;
    let token = read_trimmed(&token_file)
        .ok_or_else(|| "Cirrus MCP bridge token is unavailable. Re-enable MCP in Cirrus Settings.".to_string())?;
    if token.len() < 24 {
        return Err("Cirrus MCP bridge token is invalid. Re-enable MCP in Cirrus Settings.".to_string());
    }
    Ok(BridgeTarget { base_url: format!("http://127.0.0.1:{port}"), token })
}

fn read_trimmed(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok().map(|value| value.trim().to_string()).filter(|value| !value.is_empty())
}

fn app_data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        return std::env::var_os("APPDATA").map(|base| PathBuf::from(base).join("com.cirrus"));
    }
    #[cfg(target_os = "macos")]
    {
        return std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Application Support/com.cirrus"));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        return std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
            .map(|base| base.join("com.cirrus"));
    }
    #[allow(unreachable_code)]
    None
}

fn success_response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_response(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message.into() }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_protocol_versions_are_ordered_newest_first() {
        assert_eq!(SUPPORTED_PROTOCOL_VERSIONS[0], LATEST_PROTOCOL_VERSION);
    }

    #[test]
    fn tools_list_exposes_login_and_account_list_without_arguments() {
        let tools = tools_list();
        let tools = tools.get("tools").and_then(Value::as_array).expect("tools");
        assert_eq!(tools.len(), 19);
        assert_eq!(tools[0].get("name").and_then(Value::as_str), Some(LOGIN_TOOL));
        assert_eq!(
            tools[0].get("inputSchema").and_then(|value| value.get("additionalProperties")).and_then(Value::as_bool),
            Some(false)
        );
    }

    #[test]
    fn device_images_must_use_apple_https_hosts() {
        assert!(is_apple_device_image_url(
            "https://statici.icloud.com/fmipmobile/deviceImages-9.0/iPhone/iPhone18,3/online-sourcelist.png"
        ));
        assert!(is_apple_device_image_url(
            "https://statici.icloud.com.cn/fmipmobile/deviceImages-9.0/iPad/iPad13,1/offline-sourcelist.png"
        ));
        assert!(!is_apple_device_image_url("http://statici.icloud.com/image.png"));
        assert!(!is_apple_device_image_url("https://example.com/image.png"));
    }

    #[test]
    fn text_result_strips_inline_device_artwork() {
        let result = json!({
            "devices": [{
                "name": "Phone",
                "deviceImageMimeType": "image/png",
                "deviceImageDataUrl": "data:image/png;base64,large"
            }]
        });
        let visible = result_without_inline_artwork(&result);
        assert_eq!(visible["devices"][0]["deviceImageMimeType"], "image/png");
        assert!(visible["devices"][0].get("deviceImageDataUrl").is_none());
        assert_eq!(result["devices"][0]["deviceImageDataUrl"], "data:image/png;base64,large");
    }

    #[test]
    fn http_authentication_and_loopback_hosts_are_checked() {
        assert!(token_matches(b"secret", "secret"));
        assert!(!token_matches(b"secret", "secreu"));
        assert!(!token_matches(b"secret", "secret-longer"));

        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("127.0.0.1:5226"));
        assert!(host_is_loopback(&headers));
        headers.insert(header::HOST, HeaderValue::from_static("example.com"));
        assert!(!host_is_loopback(&headers));
    }

    #[test]
    fn device_summary_keeps_location_next_to_artwork() {
        let device = json!({
            "name": "Test Phone",
            "deviceDisplayName": "iPhone",
            "deviceStatus": "200",
            "batteryLevel": 0.82,
            "location": {
                "latitude": 30.1234567,
                "longitude": 120.7654321,
                "horizontalAccuracy": 9.6,
                "isOld": false
            }
        });
        let summary = device_summary_text(&device);
        assert!(summary.contains("Test Phone · iPhone"));
        assert!(summary.contains("Location: 30.123457, 120.765432 (±10 m)"));
        assert!(summary.contains("Status: online"));
        assert!(summary.contains("Battery: 82%"));
    }

    #[test]
    fn device_summary_explains_disabled_location_services() {
        let device = json!({
            "name": "Old Phone",
            "locationEnabled": false,
            "location": null,
            "deviceStatus": "201"
        });
        let summary = device_summary_text(&device);
        assert!(summary.contains("Location: services off"));
        assert!(summary.contains("Status: offline"));
    }
    #[test]
    fn errors_are_json_rpc_compatible() {
        let response = error_response(json!("request-1"), -32601, "Method not found");
        assert_eq!(response.get("jsonrpc").and_then(Value::as_str), Some("2.0"));
        assert_eq!(response.get("error").and_then(|value| value.get("code")).and_then(Value::as_i64), Some(-32601));
    }
}
