use std::{
    future::Future,
    io::{self, BufRead, Write},
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Instant, SystemTime, UNIX_EPOCH},
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
use chrono::{DateTime, SecondsFormat, Utc};
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
const BRIDGE_RUNTIME_FILE: &str = "mcp-bridge.json";

#[derive(Debug, Clone, Default)]
struct ClientIdentity {
    name: Option<String>,
    version: Option<String>,
}

enum RuntimeMode {
    Stdio,
    Http(HttpConfig),
}

pub struct HttpConfig {
    pub bind_addr: SocketAddr,
    pub path: String,
    pub token: String,
    pub allow_remote: bool,
    pub bridge: Option<BridgeTarget>,
}

#[derive(Clone)]
struct HttpState {
    token: Arc<str>,
    allow_remote: bool,
    bridge: Option<BridgeTarget>,
}

pub async fn run_cli() {
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
        let response = handle_line(&line, &mut client, None).await;
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
                bridge: None,
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
    if let Some(path) = token_file {
        return read_token_file(&path);
    }
    let data_dir = app_data_dir().ok_or_else(|| "HTTP token is not configured".to_string())?;
    if let Some(target) = read_bridge_runtime_file(&data_dir.join(BRIDGE_RUNTIME_FILE)) {
        return Ok(target.token);
    }
    read_token_file(&data_dir.join("mcp-bridge-token"))
}

fn read_token_file(path: &Path) -> Result<String, String> {
    let token = std::fs::read_to_string(path)
        .map_err(|error| format!("could not read HTTP token from {}: {error}", path.display()))?;
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err("HTTP token file is empty".to_string());
    }
    Ok(token)
}

async fn run_http(config: HttpConfig) -> Result<(), String> {
    let bind_addr = config.bind_addr;
    let listener = TcpListener::bind(bind_addr)
        .await
        .map_err(|error| format!("could not bind HTTP service at {bind_addr}: {error}"))?;
    serve_http_on_listener(listener, config, async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await
}

/// Serve Streamable HTTP on a pre-bound listener. The desktop app uses this
/// entry point to run the MCP transport inside the Tauri process.
pub async fn serve_http_on_listener(
    listener: TcpListener,
    config: HttpConfig,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<(), String> {
    let local_addr = listener.local_addr().map_err(|error| format!("could not read HTTP address: {error}"))?;
    let path = config.path.clone();
    let state = HttpState { token: Arc::from(config.token), allow_remote: config.allow_remote, bridge: config.bridge };
    let router = Router::new()
        .route("/healthz", get(http_health))
        .route(&path, post(http_message).get(http_stream).delete(http_delete).options(http_options))
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024))
        .with_state(state);
    eprintln!("cirrus-mcp Streamable HTTP listening on http://{local_addr}{path}");
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown)
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
    match handle_message(payload, &mut client, state.bridge.as_ref()).await {
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

async fn handle_line(line: &str, client: &mut ClientIdentity, bridge: Option<&BridgeTarget>) -> Option<Value> {
    let payload: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(error) => {
            return Some(error_response(Value::Null, -32700, format!("Parse error: {error}")));
        }
    };

    if let Value::Array(items) = payload {
        let mut responses = Vec::new();
        for item in items {
            if let Some(response) = handle_message(item, client, bridge).await {
                responses.push(response);
            }
        }
        return if responses.is_empty() { None } else { Some(Value::Array(responses)) };
    }

    handle_message(payload, client, bridge).await
}

async fn handle_message(payload: Value, client: &mut ClientIdentity, bridge: Option<&BridgeTarget>) -> Option<Value> {
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
                    "instructions": "Cirrus exposes iCloud sign-in and authorized-account listing. Never ask the user to put an Apple ID password in chat or tool arguments. Call cirrus_login, ask the user to approve the request in Cirrus, and let them enter credentials directly in the Cirrus sign-in window that opens. If the result reports mcpEnabled=false, tell the user to enable that account in Cirrus Settings > MCP before requesting further iCloud operations. Use cirrus_list_accounts after sign-in to discover the accounts currently allowed for MCP. Tool results include a concise Markdown summary and structuredContent; use structuredContent when exact field values or identifiers are needed. Preserve the first Markdown summary, including artwork URLs when provided. Render native MCP image content blocks when present, and include each device's last location update timestamp when presenting device lists."
                }),
            ))
        }
        "ping" | "logging/setLevel" => Some(success_response(id, json!({}))),
        "tools/list" => Some(success_response(id, tools_list())),
        "tools/call" => Some(handle_tool_call(id, params, client, bridge).await),
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
                "outputSchema": tool_output_schema(LOGIN_TOOL),
                "annotations": { "title": "Sign in to iCloud with Cirrus", "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": true }
            },
            {
                "name": LIST_ACCOUNTS_TOOL,
                "title": "List MCP-enabled iCloud accounts",
                "description": "List iCloud accounts that are allowed for MCP operations. Accounts disabled in Cirrus Settings > MCP are omitted.",
                "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
                "outputSchema": tool_output_schema(LIST_ACCOUNTS_TOOL),
                "annotations": { "title": "List MCP-enabled iCloud accounts", "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
            },
            {
                "name": "cirrus_account_status",
                "title": "Get iCloud account status",
                "description": "Return verification and sign-in status for an MCP-enabled Cirrus account.",
                "inputSchema": account_schema(json!({})),
                "outputSchema": tool_output_schema("cirrus_account_status"),
                "annotations": { "title": "Get iCloud account status", "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
            },
            {
                "name": "cirrus_logout",
                "title": "Sign out of an iCloud account",
                "description": "Remove the local Cirrus session and mark the selected account as signed out. This does not change the Apple account.",
                "inputSchema": account_schema(json!({})),
                "outputSchema": tool_output_schema("cirrus_logout"),
                "annotations": { "title": "Sign out of an iCloud account", "readOnlyHint": false, "destructiveHint": true, "idempotentHint": false, "openWorldHint": false }
            },
            {
                "name": "cirrus_list_devices",
                "title": "List Find My devices",
                "description": "List Apple devices for an MCP-enabled account. Call cirrus_list_accounts first and pass accounts[].accountKey verbatim. Returns retrievedAt, deviceCount, and devices[] with name, model, online status, activation lock state, battery, location, artwork, and deviceId. Each device includes lastLocationUpdatedAt and an artwork object. includeArtwork defaults to url, which returns stable Apple-hosted artwork URLs without fetching image bytes. dataUri additionally returns native MCP image content blocks with inline base64 data and structuredContent.artwork.dataUri; none omits artwork. The first text block is a device summary, followed by a detailed table. Include the last location update in user-facing output. Use the exact returned deviceId with cirrus_play_sound.",
                "inputSchema": account_schema(json!({
                    "includeArtwork": {
                        "type": "string",
                        "enum": ["url", "dataUri", "none"],
                        "default": "url",
                        "description": "Artwork payload mode. url returns stable Apple-hosted artwork URLs, dataUri additionally returns native MCP image content blocks with inline base64 data, and none omits artwork."
                    }
                })),
                "outputSchema": tool_output_schema("cirrus_list_devices"),
                "annotations": { "title": "List Find My devices", "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true }
            },
            {
                "name": "cirrus_play_sound",
                "title": "Play sound on an Apple device",
                "description": "Ask Apple Find My to play a sound. Call cirrus_list_devices first and pass the exact deviceId for the target device.",
                "inputSchema": account_schema_required(json!({
                    "deviceId": { "type": "string", "description": "Find My deviceId returned by cirrus_list_devices." }
                }), &["deviceId"]),
                "outputSchema": tool_output_schema("cirrus_play_sound"),
                "annotations": { "title": "Play sound on an Apple device", "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": true }
            },
            {
                "name": "cirrus_list_photos",
                "title": "List iCloud Photos",
                "description": "List iCloud Photos assets with metadata, thumbnail URLs, and original media URLs.",
                "inputSchema": account_schema(json!({
                    "cursor": { "type": "string", "description": "Optional opaque pagination cursor from a previous nextCursor value." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 200, "default": 20, "description": "Maximum number of assets to return, from 1 to 200." },
                    "startRank": { "type": "integer", "minimum": 0, "description": "Optional zero-based rank into the timeline. Prefer cursor for normal pagination." }
                })),
                "outputSchema": tool_output_schema("cirrus_list_photos"),
                "annotations": { "title": "List iCloud Photos", "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true }
            },
            {
                "name": "cirrus_photos_total",
                "title": "Get iCloud Photos totals",
                "description": "Return total photo, video, and asset counts for an iCloud Photo Library.",
                "inputSchema": account_schema(json!({})),
                "outputSchema": tool_output_schema("cirrus_photos_total"),
                "annotations": { "title": "Get iCloud Photos totals", "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true }
            },
            {
                "name": "cirrus_photos_timeline",
                "title": "Get iCloud Photos timeline",
                "description": "Return the library timeline distribution grouped by year and month.",
                "inputSchema": account_schema(json!({})),
                "outputSchema": tool_output_schema("cirrus_photos_timeline"),
                "annotations": { "title": "Get iCloud Photos timeline", "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true }
            },
            {
                "name": "cirrus_photos_month_exact",
                "title": "Get exact iCloud Photos month count",
                "description": "Return the exact photo count and rank range for a year and month.",
                "inputSchema": account_schema_required(json!({
                    "year": { "type": "integer", "minimum": 1, "description": "Four-digit calendar year, for example 2026." },
                    "month": { "type": "integer", "minimum": 1, "maximum": 12, "description": "Calendar month from 1 to 12." }
                }), &["year", "month"]),
                "outputSchema": tool_output_schema("cirrus_photos_month_exact"),
                "annotations": { "title": "Get exact iCloud Photos month count", "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true }
            },
            {
                "name": "cirrus_download_photo",
                "title": "Download an iCloud photo or video",
                "description": "Download an original iCloud media URL to an absolute local path. Only Apple media hosts are accepted.",
                "inputSchema": account_schema_required(json!({
                    "url": { "type": "string", "description": "Original media URL returned by cirrus_list_photos." },
                    "photoId": { "type": "string", "description": "Optional photo ID used in download history." },
                    "name": { "type": "string", "description": "Optional display file name. The final extension may be corrected from the downloaded content type." },
                    "targetPath": { "type": "string", "description": "Absolute destination file path." },
                    "transferId": { "type": "string", "description": "Optional caller-generated ID used for progress and cancellation." },
                    "partialKey": { "type": "string", "description": "Optional stable key used for resumable partial files." }
                }), &["url", "targetPath"]),
                "outputSchema": tool_output_schema("cirrus_download_photo"),
                "annotations": { "title": "Download an iCloud photo or video", "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true }
            },
            {
                "name": "cirrus_cancel_download",
                "title": "Cancel an MCP download",
                "description": "Cancel a download started with cirrus_download_photo by its transferId.",
                "inputSchema": account_schema_required(json!({
                    "transferId": { "type": "string", "description": "Transfer ID supplied to cirrus_download_photo." }
                }), &["transferId"]),
                "outputSchema": tool_output_schema("cirrus_cancel_download"),
                "annotations": { "title": "Cancel an MCP download", "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
            },
            {
                "name": "cirrus_list_download_tasks",
                "title": "List MCP download history",
                "description": "List persisted download tasks for the selected iCloud account.",
                "inputSchema": account_schema(json!({})),
                "outputSchema": tool_output_schema("cirrus_list_download_tasks"),
                "annotations": { "title": "List MCP download history", "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
            },
            {
                "name": "cirrus_get_account_avatar",
                "title": "Get iCloud account avatar",
                "description": "Return the Apple account avatar as an MCP image content block. The text and structuredContent only describe the image and never include the Base64 payload.",
                "inputSchema": account_schema(json!({})),
                "outputSchema": tool_output_schema("cirrus_get_account_avatar"),
                "annotations": { "title": "Get iCloud account avatar", "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true }
            },
            {
                "name": "cirrus_get_contact_photo",
                "title": "Get an iCloud contact photo",
                "description": "Fetch an Apple-hosted contact photo and return it as an MCP image content block. The text and structuredContent only describe the image and never include the Base64 payload.",
                "inputSchema": account_schema_required(json!({
                    "url": { "type": "string", "description": "Contact photoUrl returned by cirrus_list_contacts." }
                }), &["url"]),
                "outputSchema": tool_output_schema("cirrus_get_contact_photo"),
                "annotations": { "title": "Get an iCloud contact photo", "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true }
            },
            {
                "name": "cirrus_list_notes",
                "title": "List iCloud Notes",
                "description": "List folders and notes visible to the selected iCloud account.",
                "inputSchema": account_schema(json!({ "cursor": { "type": "string", "description": "Optional opaque pagination cursor from a previous nextCursor value." } })),
                "outputSchema": tool_output_schema("cirrus_list_notes"),
                "annotations": { "title": "List iCloud Notes", "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true }
            },
            {
                "name": "cirrus_get_note",
                "title": "Read an iCloud Note",
                "description": "Return a note with inline image attachments in document order as native MCP text and image content blocks. Set includeImages=false for text and attachment metadata only.",
                "inputSchema": account_schema_required(json!({
                    "noteId": { "type": "string", "description": "Note ID returned by cirrus_list_notes." },
                    "includeImages": { "type": "boolean", "default": true, "description": "Return inline image content blocks in body order. Defaults to true." },
                    "original": { "type": "boolean", "default": false, "description": "Use original images instead of previews when includeImages is true." }
                }), &["noteId"]),
                "outputSchema": tool_output_schema("cirrus_get_note"),
                "annotations": { "title": "Read an iCloud Note", "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true }
            },
            {
                "name": "cirrus_get_note_image",
                "title": "Read an iCloud Note image",
                "description": "Fetch an image attachment from an iCloud Note and return it as a native MCP image content block. Use attachments[].id from cirrus_get_note. Returns a compact preview by default; set original=true to request the original image.",
                "inputSchema": account_schema_required(json!({
                    "noteId": { "type": "string", "description": "Note ID returned by cirrus_list_notes." },
                    "attachmentId": { "type": "string", "description": "Image attachment ID returned by cirrus_get_note." },
                    "original": { "type": "boolean", "description": "Return the original image instead of the preview. Defaults to false." }
                }), &["noteId", "attachmentId"]),
                "outputSchema": tool_output_schema("cirrus_get_note_image"),
                "annotations": { "title": "Read an iCloud Note image", "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true }
            },
            {
                "name": "cirrus_create_note",
                "title": "Create an iCloud Note",
                "description": "Create a plain-text note in iCloud Notes. Omit folderId to use the default Notes folder.",
                "inputSchema": account_schema_required(json!({
                    "title": { "type": "string", "description": "Note title." },
                    "body": { "type": "string", "description": "Plain-text note body. Omit for an empty note." },
                    "folderId": { "type": "string", "description": "Optional folder ID returned by cirrus_list_notes." }
                }), &["title"]),
                "outputSchema": tool_output_schema("cirrus_create_note"),
                "annotations": { "title": "Create an iCloud Note", "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": true }
            },
            {
                "name": "cirrus_update_note",
                "title": "Update an iCloud Note",
                "description": "Replace the title and plain-text body of an existing iCloud Note. Notes with attachments cannot be edited by this tool yet.",
                "inputSchema": account_schema_required(json!({
                    "noteId": { "type": "string", "description": "Note ID returned by cirrus_list_notes." },
                    "title": { "type": "string", "description": "New note title." },
                    "body": { "type": "string", "description": "New plain-text note body." }
                }), &["noteId", "title", "body"]),
                "outputSchema": tool_output_schema("cirrus_update_note"),
                "annotations": { "title": "Update an iCloud Note", "readOnlyHint": false, "destructiveHint": true, "idempotentHint": true, "openWorldHint": true }
            },
            {
                "name": "cirrus_delete_note",
                "title": "Delete an iCloud Note",
                "description": "Move an iCloud Note to the Trash folder.",
                "inputSchema": account_schema_required(json!({
                    "noteId": { "type": "string", "description": "Note ID returned by cirrus_list_notes." }
                }), &["noteId"]),
                "outputSchema": tool_output_schema("cirrus_delete_note"),
                "annotations": { "title": "Delete an iCloud Note", "readOnlyHint": false, "destructiveHint": true, "idempotentHint": true, "openWorldHint": true }
            },
            {
                "name": "cirrus_list_drive_items",
                "title": "List iCloud Drive items",
                "description": "List files, folders, and app containers in an iCloud Drive folder. Use folderId from a previous result to browse deeper; omit it to list the iCloud Drive root.",
                "inputSchema": account_schema(json!({ "folderId": { "type": "string", "description": "Optional item id returned by cirrus_list_drive_items. Omit for the root folder." } })),
                "outputSchema": tool_output_schema("cirrus_list_drive_items"),
                "annotations": { "title": "List iCloud Drive items", "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true }
            },
            {
                "name": "cirrus_download_drive_file",
                "title": "Download an iCloud Drive file",
                "description": "Download a file returned by cirrus_list_drive_items to an absolute local targetPath. Pass the item's downloadUrl and name.",
                "inputSchema": account_schema_required(json!({
                    "url": { "type": "string", "description": "downloadUrl returned by cirrus_list_drive_items." },
                    "targetPath": { "type": "string", "description": "Absolute local file path to save." },
                    "name": { "type": "string", "description": "Optional display file name for extension fallback." }
                }), &["url", "targetPath"]),
                "outputSchema": tool_output_schema("cirrus_download_drive_file"),
                "annotations": { "title": "Download an iCloud Drive file", "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true }
            },
            {
                "name": "cirrus_list_contacts",
                "title": "List iCloud Contacts",
                "description": "List contacts, phone numbers, email addresses, postal addresses, and other contact fields.",
                "inputSchema": account_schema(json!({ "locale": { "type": "string", "default": "zh-CN", "description": "Language for Apple contact formatting. Use zh-CN or en-US." } })),
                "outputSchema": tool_output_schema("cirrus_list_contacts"),
                "annotations": { "title": "List iCloud Contacts", "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true }
            },
            {
                "name": "cirrus_load_calendar",
                "title": "Load iCloud Calendar events",
                "description": "Load calendars and events in a date range.",
                "inputSchema": account_schema_required(json!({
                    "startDate": { "type": "string", "description": "Inclusive start date, YYYY-MM-DD." },
                    "endDate": { "type": "string", "description": "Inclusive end date, YYYY-MM-DD." },
                    "timezone": { "type": "string", "default": "Asia/Shanghai", "description": "IANA timezone such as Asia/Shanghai or America/Los_Angeles." },
                    "locale": { "type": "string", "default": "zh-CN", "description": "Language for Apple calendar formatting. Use zh-CN or en-US." }
                }), &["startDate", "endDate"]),
                "outputSchema": tool_output_schema("cirrus_load_calendar"),
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
            "description": "Required. Call cirrus_list_accounts first and pass accounts[].accountKey verbatim. Do not guess or construct this value."
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArtworkMode {
    Url,
    DataUri,
    None,
}

fn artwork_mode(arguments: &Value) -> ArtworkMode {
    match arguments.get("includeArtwork").and_then(Value::as_str) {
        Some(value) if value.eq_ignore_ascii_case("dataUri") => ArtworkMode::DataUri,
        Some(value) if value.eq_ignore_ascii_case("none") => ArtworkMode::None,
        _ => ArtworkMode::Url,
    }
}

async fn handle_tool_call(id: Value, params: Value, client: &ClientIdentity, bridge: Option<&BridgeTarget>) -> Value {
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return error_response(id, -32602, "Missing tool name");
    };
    let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
    let started_at = now_millis();
    let started = Instant::now();
    if (name == LOGIN_TOOL || name == LIST_ACCOUNTS_TOOL)
        && arguments.as_object().map(|object| !object.is_empty()).unwrap_or(!arguments.is_null())
    {
        let result = json!({
            "content": [{ "type": "text", "text": format!("{name} does not accept arguments.") }],
            "isError": true
        });
        record_mcp_history(name, &arguments, client, started_at, started, &result, bridge).await;
        return success_response(id, result);
    }

    let artwork_mode = artwork_mode(&arguments);
    let mut bridge_arguments = arguments.clone();
    if name == "cirrus_list_devices" {
        if let Some(object) = bridge_arguments.as_object_mut() {
            object.remove("includeArtwork");
        }
    }
    match call_bridge(name, &bridge_arguments, client, bridge).await {
        Ok(mut result) => {
            if name == "cirrus_list_devices" {
                annotate_device_list_timestamps(&mut result, now_millis());
            }
            let inline_note = name == "cirrus_get_note" && result.get("contentSegments").is_some();
            let mut content = result_image_content(name, &mut result, artwork_mode).await;
            let mut text_result = result_without_inline_artwork(&result);
            if inline_note {
                if let Some(object) = text_result.as_object_mut() {
                    object.remove("contentSegments");
                }
            }
            if name == "cirrus_list_devices" {
                content.insert(
                    0,
                    json!({
                        "type": "text",
                        "text": devices_summary_markdown(&result, artwork_mode)
                    }),
                );
            }
            let structured_result = result_for_structured_content(name, &result, &text_result, artwork_mode);
            if !inline_note {
                content.push(json!({
                    "type": "text",
                    "text": tool_result_text(name, &text_result)
                }));
            }
            let result = json!({
                "content": content,
                "structuredContent": structured_result,
                "isError": false
            });
            record_mcp_history(name, &arguments, client, started_at, started, &result, bridge).await;
            success_response(id, result)
        }
        Err(message) => {
            let result = json!({
                "content": [{ "type": "text", "text": tool_error_text(&message) }],
                "isError": true
            });
            record_mcp_history(name, &arguments, client, started_at, started, &result, bridge).await;
            success_response(id, result)
        }
    }
}

async fn record_mcp_history(
    tool: &str,
    arguments: &Value,
    client: &ClientIdentity,
    started_at: i64,
    started: Instant,
    response: &Value,
    bridge: Option<&BridgeTarget>,
) {
    let target = match bridge.cloned().or_else(|| discover_bridge().ok()) {
        Some(target) => target,
        None => return,
    };
    let error = response.get("isError").and_then(Value::as_bool).unwrap_or(false).then(|| {
        response
            .get("content")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(|item| item.get("text"))
            .and_then(Value::as_str)
            .unwrap_or("MCP tool call failed")
            .to_string()
    });
    let body = json!({
        "tool": tool,
        "arguments": arguments,
        "clientName": client.name.as_deref().unwrap_or("MCP client"),
        "clientVersion": client.version,
        "startedAt": started_at,
        "durationMs": started.elapsed().as_millis() as i64,
        "success": response.get("isError").and_then(Value::as_bool) != Some(true),
        "error": error,
        "response": response
    });
    let Ok(http) = reqwest::Client::builder().timeout(std::time::Duration::from_secs(5)).no_proxy().build() else {
        return;
    };
    let _ = http.post(format!("{}/v1/history", target.base_url)).bearer_auth(target.token).json(&body).send().await;
}

fn now_millis() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_millis() as i64).unwrap_or(0)
}

fn tool_output_schema(tool: &str) -> Value {
    match tool {
        LOGIN_TOOL => object_schema(
            json!({
                "authenticated": { "type": "boolean", "description": "Whether sign-in completed successfully." },
                "approved": { "type": "boolean", "description": "Whether the user approved the Cirrus sign-in request." },
                "message": { "type": ["string", "null"], "description": "Optional status or error message." },
                "account": {
                    "type": ["object", "null"],
                    "description": "Signed-in account summary when authentication succeeds.",
                    "properties": {
                        "accountKey": { "type": "string" },
                        "appleId": { "type": "string" },
                        "displayName": { "type": "string" },
                        "mcpEnabled": { "type": "boolean" }
                    }
                }
            }),
            &["authenticated", "approved"],
        ),
        LIST_ACCOUNTS_TOOL => object_schema(
            json!({
                "accounts": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "accountKey": { "type": "string", "description": "Pass this value to account-scoped tools." },
                            "appleId": { "type": "string" },
                            "firstName": { "type": ["string", "null"] },
                            "lastName": { "type": ["string", "null"] },
                            "signedIn": { "type": "boolean" },
                            "active": { "type": "boolean" },
                            "lastUsedAt": { "type": "integer", "description": "Unix timestamp in milliseconds." }
                        }
                    }
                },
                "count": { "type": "integer" }
            }),
            &["accounts", "count"],
        ),
        "cirrus_account_status" => object_schema(
            json!({
                "accountKey": { "type": "string" },
                "appleId": { "type": "string" },
                "dsid": { "type": ["string", "null"] },
                "firstName": { "type": ["string", "null"] },
                "lastName": { "type": ["string", "null"] },
                "signedIn": { "type": "boolean" },
                "verified": { "type": "boolean" },
                "lastUsedAt": { "type": "integer", "description": "Unix timestamp in milliseconds." }
            }),
            &["accountKey", "appleId", "signedIn", "verified"],
        ),
        "cirrus_logout" => object_schema(
            json!({ "accountKey": { "type": "string" }, "signedOut": { "type": "boolean" } }),
            &["accountKey", "signedOut"],
        ),
        "cirrus_list_devices" => device_output_schema(),
        "cirrus_play_sound" => object_schema(
            json!({ "success": { "type": "boolean" }, "message": { "type": ["string", "null"] } }),
            &["success"],
        ),
        "cirrus_list_photos" => object_schema(
            json!({
                "status": { "type": "string" },
                "message": { "type": ["string", "null"] },
                "photos": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string" },
                            "fileName": { "type": ["string", "null"] },
                            "takenAt": { "type": ["integer", "null"], "description": "Capture time as Unix milliseconds." },
                            "isVideo": { "type": ["boolean", "null"] },
                            "isLivePhoto": { "type": ["boolean", "null"] },
                            "width": { "type": ["integer", "null"] },
                            "height": { "type": ["integer", "null"] },
                            "sizeBytes": { "type": ["integer", "null"] },
                            "thumbUrl": { "type": "string" },
                            "displayUrl": { "type": ["string", "null"] },
                            "originalUrl": { "type": ["string", "null"] },
                            "videoUrl": { "type": ["string", "null"] }
                        }
                    }
                },
                "nextCursor": { "type": ["string", "null"], "description": "Pass this value as cursor to fetch the next page." }
            }),
            &["status", "photos"],
        ),
        "cirrus_photos_total" => object_schema(
            json!({
                "photos": { "type": "integer" },
                "videos": { "type": "integer" },
                "assets": { "type": "integer" }
            }),
            &["photos", "videos", "assets"],
        ),
        "cirrus_photos_timeline" => object_schema(
            json!({
                "distribution": {
                    "type": "object",
                    "properties": {
                        "bucketWidthMs": { "type": "integer" },
                        "total": { "type": "integer" },
                        "buckets": { "type": "array", "items": { "type": "object" } }
                    }
                },
                "timeline": {
                    "type": "object",
                    "properties": {
                        "total": { "type": "integer" },
                        "years": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "year": { "type": "integer" },
                                    "count": { "type": "integer" },
                                    "months": { "type": "array", "items": { "type": "object" } }
                                }
                            }
                        }
                    }
                }
            }),
            &["distribution", "timeline"],
        ),
        "cirrus_photos_month_exact" => object_schema(
            json!({
                "year": { "type": "integer" },
                "month": { "type": "integer" },
                "count": { "type": "integer" },
                "newestRank": { "type": "integer" },
                "oldestRank": { "type": "integer" }
            }),
            &["year", "month", "count", "newestRank", "oldestRank"],
        ),
        "cirrus_download_photo" => object_schema(
            json!({
                "transferId": { "type": "string" },
                "accountKey": { "type": "string" },
                "path": { "type": "string", "description": "Absolute path of the saved file." },
                "sizeBytes": { "type": "integer" },
                "fileName": { "type": "string" },
                "persisted": { "type": "boolean", "description": "Whether the download task was saved to history." }
            }),
            &["transferId", "path", "sizeBytes", "fileName", "persisted"],
        ),
        "cirrus_cancel_download" => object_schema(
            json!({
                "transferId": { "type": "string" },
                "cancelled": { "type": "boolean", "description": "Whether an active transfer was found and cancelled." }
            }),
            &["transferId", "cancelled"],
        ),
        "cirrus_list_download_tasks" => object_schema(
            json!({
                "tasks": { "type": "array", "items": { "type": "object", "description": "Persisted download task." } },
                "persistenceEnabled": { "type": "boolean" },
                "writable": { "type": "boolean" },
                "warning": { "type": ["string", "null"] }
            }),
            &["tasks", "persistenceEnabled", "writable"],
        ),
        "cirrus_get_account_avatar" | "cirrus_get_contact_photo" => object_schema(
            json!({
                "imageIncluded": { "type": "boolean", "description": "Whether an image content block was returned." },
                "mimeType": { "type": ["string", "null"], "description": "MIME type of the image content block." },
                "byteLength": { "type": ["integer", "null"], "description": "Decoded image size in bytes." }
            }),
            &["imageIncluded"],
        ),
        "cirrus_list_notes" => object_schema(
            json!({
                "status": { "type": "string" },
                "message": { "type": ["string", "null"] },
                "folders": { "type": "array", "items": { "type": "object", "properties": { "id": { "type": "string" }, "name": { "type": "string" } } } },
                "notes": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string" },
                            "title": { "type": "string" },
                            "snippet": { "type": "string" },
                            "folderId": { "type": ["string", "null"] },
                            "createdAt": { "type": ["integer", "null"], "description": "Unix milliseconds." },
                            "modifiedAt": { "type": ["integer", "null"], "description": "Unix milliseconds." },
                            "pinned": { "type": "boolean" },
                            "deleted": { "type": "boolean" },
                            "firstAttachmentUti": { "type": ["string", "null"] }
                        }
                    }
                },
                "nextCursor": { "type": ["string", "null"] }
            }),
            &["status", "folders", "notes"],
        ),
        "cirrus_get_note" => object_schema(
            json!({
                "status": { "type": "string" },
                "message": { "type": ["string", "null"] },
                "inlineImageCount": { "type": "integer", "description": "Number of native image content blocks returned inline." },
                "note": {
                    "type": ["object", "null"],
                    "properties": {
                        "id": { "type": "string" },
                        "title": { "type": "string" },
                        "body": { "type": "string", "description": "Plain-text note body." },
                        "bodyHtml": { "type": "string", "description": "Rich HTML note body." },
                        "folderId": { "type": ["string", "null"] },
                        "createdAt": { "type": ["integer", "null"] },
                        "modifiedAt": { "type": ["integer", "null"] },
                        "attachments": { "type": "array", "items": { "type": "object" } }
                    }
                }
            }),
            &["status"],
        ),
        "cirrus_get_note_image" => object_schema(
            json!({
                "imageIncluded": { "type": "boolean", "description": "Whether an image content block was returned." },
                "mimeType": { "type": ["string", "null"], "description": "MIME type of the image content block." },
                "byteLength": { "type": ["integer", "null"], "description": "Decoded image size in bytes." },
                "attachmentId": { "type": "string" },
                "name": { "type": "string" },
                "source": { "type": "string", "description": "preview, original, or rendered." }
            }),
            &["imageIncluded", "attachmentId"],
        ),
        "cirrus_create_note" | "cirrus_update_note" | "cirrus_delete_note" => object_schema(
            json!({
                "status": { "type": "string" },
                "message": { "type": ["string", "null"] },
                "noteId": { "type": ["string", "null"] }
            }),
            &["status"],
        ),
        "cirrus_list_drive_items" => object_schema(
            json!({
                "status": { "type": "string" },
                "message": { "type": ["string", "null"] },
                "folderId": { "type": "string" },
                "items": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string" },
                            "drivewsid": { "type": "string" },
                            "parentId": { "type": ["string", "null"] },
                            "name": { "type": "string" },
                            "kind": { "type": "string", "enum": ["folder", "file", "app-library"] },
                            "zone": { "type": ["string", "null"] },
                            "extension": { "type": ["string", "null"] },
                            "sizeBytes": { "type": ["integer", "null"] },
                            "dateModified": { "type": ["string", "null"] },
                            "downloadUrl": { "type": ["string", "null"] },
                            "childCount": { "type": ["integer", "null"] },
                            "favorite": { "type": "boolean" }
                        }
                    }
                },
                "nextCursor": { "type": ["string", "null"] }
            }),
            &["status", "folderId", "items"],
        ),
        "cirrus_download_drive_file" => object_schema(
            json!({
                "path": { "type": "string", "description": "Absolute path of the saved file." },
                "fileName": { "type": "string" },
                "sizeBytes": { "type": "integer" }
            }),
            &["path", "fileName", "sizeBytes"],
        ),
        "cirrus_list_contacts" => object_schema(
            json!({
                "status": { "type": "string" },
                "message": { "type": ["string", "null"] },
                "contacts": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string" },
                            "displayName": { "type": "string" },
                            "firstName": { "type": "string" },
                            "lastName": { "type": "string" },
                            "phones": { "type": "array", "items": { "type": "object" } },
                            "emails": { "type": "array", "items": { "type": "object" } },
                            "addresses": { "type": "array", "items": { "type": "object" } },
                            "photoUrl": { "type": ["string", "null"], "description": "Pass to cirrus_get_contact_photo to fetch the image." }
                        }
                    }
                }
            }),
            &["status", "contacts"],
        ),
        "cirrus_load_calendar" => object_schema(
            json!({
                "status": { "type": "string" },
                "message": { "type": ["string", "null"] },
                "calendars": { "type": "array", "items": { "type": "object" } },
                "events": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string" },
                            "title": { "type": "string" },
                            "calendarId": { "type": "string" },
                            "start": { "type": "string" },
                            "end": { "type": "string" },
                            "allDay": { "type": "boolean" },
                            "recurring": { "type": "boolean" },
                            "location": { "type": "string" },
                            "notes": { "type": "string" }
                        }
                    }
                }
            }),
            &["status", "calendars", "events"],
        ),
        _ => json!({ "type": "object" }),
    }
}

fn object_schema(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required
    })
}

fn device_output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "retrievedAt": { "type": "integer", "description": "Unix timestamp in milliseconds when the device list was fetched from Cirrus." },
            "deviceCount": { "type": "integer", "description": "Number of devices returned." },
            "devices": {
                "type": "array",
                "description": "Find My devices for the selected account.",
                "items": {
                    "type": "object",
                    "properties": {
                        "deviceId": { "type": "string", "description": "Opaque Find My identifier. Pass verbatim to cirrus_play_sound." },
                        "name": { "type": "string", "description": "Device name shown by Find My." },
                        "deviceDisplayName": { "type": "string", "description": "Human-readable Apple model name." },
                        "deviceStatus": { "type": "string", "description": "Raw Find My status code; 200 means online." },
                        "activationLocked": { "type": ["boolean", "null"], "description": "Whether Activation Lock is enabled on the device." },
                        "batteryLevel": { "type": ["number", "null"], "description": "Battery fraction from 0 to 1. Use batteryStatus when it is Unknown." },
                        "batteryStatus": { "type": ["string", "null"], "description": "Find My battery status label." },
                        "location": { "type": ["object", "null"], "description": "Last known coordinates, accuracy, and raw Find My timestamp. Null when unavailable." },
                        "lastLocationUpdatedAt": { "type": ["integer", "null"], "description": "Unix timestamp in milliseconds of the latest Find My location report." },
                        "locationEnabled": { "type": ["boolean", "null"], "description": "Whether Location Services is enabled on the device." },
                        "deviceImageMimeType": { "type": ["string", "null"], "description": "MIME type of the image block returned alongside this result." },
                        "deviceImageUrl": { "type": ["string", "null"], "description": "Apple-hosted artwork URL suitable for Markdown image syntax." },
                        "deviceImageError": { "type": ["string", "null"], "description": "Why artwork could not be included, when applicable." },
                        "artwork": {
                            "type": ["object", "null"],
                            "description": "Normalized artwork metadata. dataUri is present only when includeArtwork=dataUri.",
                            "properties": {
                                "mimeType": { "type": ["string", "null"], "description": "Image MIME type." },
                                "url": { "type": ["string", "null"], "description": "Stable Apple-hosted artwork URL for Markdown rendering." },
                                "dataUri": { "type": ["string", "null"], "description": "Inline base64 data URI when includeArtwork=dataUri." },
                                "alt": { "type": "string", "description": "Accessible image label." }
                            }
                        }
                    },
                    "required": ["deviceId", "name"]
                }
            }
        },
        "required": ["deviceCount", "devices"]
    })
}

fn tool_result_text(tool: &str, result: &Value) -> String {
    match tool {
        LOGIN_TOOL => login_markdown(result),
        LIST_ACCOUNTS_TOOL => accounts_markdown(result),
        "cirrus_account_status" => account_status_markdown(result),
        "cirrus_logout" => logout_markdown(result),
        "cirrus_list_devices" => devices_markdown(result),
        "cirrus_play_sound" => play_sound_markdown(result),
        "cirrus_list_photos" => photos_markdown(result),
        "cirrus_photos_total" => photos_total_markdown(result),
        "cirrus_photos_timeline" => photos_timeline_markdown(result),
        "cirrus_photos_month_exact" => month_exact_markdown(result),
        "cirrus_download_photo" => download_photo_markdown(result),
        "cirrus_cancel_download" => cancel_download_markdown(result),
        "cirrus_list_download_tasks" => download_tasks_markdown(result),
        "cirrus_get_account_avatar" | "cirrus_get_contact_photo" | "cirrus_get_note_image" => image_markdown(result),
        "cirrus_list_notes" => notes_list_markdown(result),
        "cirrus_get_note" => note_detail_markdown(result),
        "cirrus_create_note" | "cirrus_update_note" | "cirrus_delete_note" => note_mutation_markdown(result),
        "cirrus_list_drive_items" => drive_items_markdown(result),
        "cirrus_download_drive_file" => drive_download_markdown(result),
        "cirrus_list_contacts" => contacts_markdown(result),
        "cirrus_load_calendar" => calendar_markdown(result),
        _ => serde_json::to_string_pretty(result).unwrap_or_else(|_| result.to_string()),
    }
}

fn login_markdown(result: &Value) -> String {
    let mut rows = vec![
        ("Authenticated", yes_no(result.get("authenticated")).to_string()),
        ("Approved", yes_no(result.get("approved")).to_string()),
    ];
    if let Some(account) = result.get("account").filter(|value| !value.is_null()) {
        rows.push(("Account", string_field(account, "displayName")));
        rows.push(("Apple ID", string_field(account, "appleId")));
        rows.push(("Account key", code_field(account, "accountKey")));
        rows.push(("MCP enabled", yes_no(account.get("mcpEnabled")).to_string()));
    }
    if let Some(message) = result.get("message").and_then(Value::as_str).filter(|value| !value.is_empty()) {
        rows.push(("Message", message.to_string()));
    }
    key_value_markdown("Cirrus sign-in", &rows)
}

fn account_status_markdown(result: &Value) -> String {
    let rows = vec![
        ("Account key", code_field(result, "accountKey")),
        ("Apple ID", string_field(result, "appleId")),
        ("First name", string_field(result, "firstName")),
        ("Last name", string_field(result, "lastName")),
        ("Signed in", yes_no(result.get("signedIn")).to_string()),
        ("Verified", yes_no(result.get("verified")).to_string()),
        ("Last used", number_field(result, "lastUsedAt")),
    ];
    key_value_markdown("Account status", &rows)
}

fn logout_markdown(result: &Value) -> String {
    let rows = vec![
        ("Account key", code_field(result, "accountKey")),
        ("Signed out", yes_no(result.get("signedOut")).to_string()),
    ];
    key_value_markdown("Account sign-out", &rows)
}

fn play_sound_markdown(result: &Value) -> String {
    let mut rows = vec![("Success", yes_no(result.get("success")).to_string())];
    if let Some(message) = result.get("message").and_then(Value::as_str).filter(|value| !value.is_empty()) {
        rows.push(("Message", message.to_string()));
    }
    key_value_markdown("Play sound", &rows)
}

fn photos_markdown(result: &Value) -> String {
    let photos = result.get("photos").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut output = format!("# iCloud Photos ({})\n\n", photos.len());
    if let Some(message) = result.get("message").and_then(Value::as_str).filter(|value| !value.is_empty()) {
        output.push_str(&format!("Status: {message}\n\n"));
    }
    if photos.is_empty() {
        output.push_str("No photos were returned for this page.");
    } else {
        output.push_str("| # | ID | File | Taken at | Type | Dimensions | Size | Favorite |\n");
        output.push_str("| --- | --- | --- | --- | --- | --- | --- | --- |\n");
        for (index, photo) in photos.iter().enumerate() {
            let kind = if photo.get("isVideo").and_then(Value::as_bool).unwrap_or(false) { "Video" } else { "Photo" };
            let dimensions =
                match (photo.get("width").and_then(Value::as_i64), photo.get("height").and_then(Value::as_i64)) {
                    (Some(width), Some(height)) => format!("{width}×{height}"),
                    _ => "Unknown".to_string(),
                };
            output.push_str(&format!(
                "| {} | `{}` | {} | {} | {} | {} | {} | {} |\n",
                index + 1,
                markdown_cell(string_field(photo, "id")),
                markdown_cell(string_field(photo, "fileName")),
                number_field(photo, "takenAt"),
                kind,
                dimensions,
                byte_size_field(photo, "sizeBytes"),
                yes_no(photo.get("isFavorite")),
            ));
        }
        output.push_str("\nUse `structuredContent.photos` for exact thumbnail and original media URLs.");
    }
    if let Some(cursor) = result.get("nextCursor").and_then(Value::as_str).filter(|value| !value.is_empty()) {
        output.push_str(&format!("\n\nNext cursor: `{cursor}`"));
    }
    output
}

fn photos_total_markdown(result: &Value) -> String {
    let rows = vec![
        ("Photos", number_field(result, "photos")),
        ("Videos", number_field(result, "videos")),
        ("Assets", number_field(result, "assets")),
    ];
    key_value_markdown("iCloud Photos totals", &rows)
}

fn photos_timeline_markdown(result: &Value) -> String {
    let timeline = result.get("timeline").unwrap_or(result);
    let total = timeline.get("total").and_then(Value::as_i64).unwrap_or(0);
    let mut output = format!("# iCloud Photos timeline ({total} assets)\n\n");
    let years = timeline.get("years").and_then(Value::as_array).cloned().unwrap_or_default();
    if years.is_empty() {
        output.push_str("No timeline buckets were returned.");
        return output;
    }
    output.push_str("| Year | Year count | Month | Month count | Start rank |\n");
    output.push_str("| --- | --- | --- | --- | --- |\n");
    for year in years {
        let year_number = year.get("year").and_then(Value::as_i64).unwrap_or(0);
        let year_count = year.get("count").and_then(Value::as_i64).unwrap_or(0);
        let months = year.get("months").and_then(Value::as_array).cloned().unwrap_or_default();
        if months.is_empty() {
            output.push_str(&format!("| {year_number} | {year_count} | - | - | - |\n"));
            continue;
        }
        for month in months {
            output.push_str(&format!(
                "| {year_number} | {year_count} | {} | {} | {} |\n",
                number_field(&month, "month"),
                number_field(&month, "count"),
                number_field(&month, "startRank"),
            ));
        }
    }
    output
}

fn month_exact_markdown(result: &Value) -> String {
    let rows = vec![
        ("Year", number_field(result, "year")),
        ("Month", number_field(result, "month")),
        ("Count", number_field(result, "count")),
        ("Newest rank", number_field(result, "newestRank")),
        ("Oldest rank", number_field(result, "oldestRank")),
    ];
    key_value_markdown("Exact monthly photo count", &rows)
}

fn download_photo_markdown(result: &Value) -> String {
    let rows = vec![
        ("Transfer ID", code_field(result, "transferId")),
        ("File name", string_field(result, "fileName")),
        ("Saved path", code_field(result, "path")),
        ("Size", byte_size_field(result, "sizeBytes")),
        ("History persisted", yes_no(result.get("persisted")).to_string()),
    ];
    key_value_markdown("Download completed", &rows)
}

fn cancel_download_markdown(result: &Value) -> String {
    let rows = vec![
        ("Transfer ID", code_field(result, "transferId")),
        ("Cancelled", yes_no(result.get("cancelled")).to_string()),
    ];
    key_value_markdown("Download cancellation", &rows)
}

fn download_tasks_markdown(result: &Value) -> String {
    let tasks = result.get("tasks").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut output = format!("# Download tasks ({})\n\n", tasks.len());
    if tasks.is_empty() {
        output.push_str("No persisted download tasks were found.");
    } else {
        output.push_str("| # | Title | Status | Completed | Failed | Directory | Created at |\n");
        output.push_str("| --- | --- | --- | --- | --- | --- | --- |\n");
        for (index, task) in tasks.iter().enumerate() {
            let failed = task.get("failed").and_then(Value::as_array).map(Vec::len).unwrap_or(0);
            output.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} |\n",
                index + 1,
                markdown_cell(string_field(task, "title")),
                markdown_cell(string_field(task, "status")),
                number_field(task, "done"),
                failed,
                markdown_cell(string_field(task, "directory")),
                number_field(task, "createdAt"),
            ));
        }
    }
    if let Some(warning) = result.get("warning").and_then(Value::as_str).filter(|value| !value.is_empty()) {
        output.push_str(&format!("\n\nWarning: {warning}"));
    }
    output
}

fn note_inline_header(note: &Value) -> String {
    let title = string_field(note, "title");
    let folder = string_field(note, "folderId");
    let modified = number_field(note, "modifiedAt");
    let mut header = format!("# {title}\n\n");
    if !folder.is_empty() {
        header.push_str(&format!("Folder: `{folder}`\n\n"));
    }
    if !modified.is_empty() {
        header.push_str(&format!("Modified at: {modified}\n"));
    }
    header
}
fn image_markdown(result: &Value) -> String {
    if result.get("imageIncluded").and_then(Value::as_bool) == Some(true) {
        let mime = string_field(result, "mimeType");
        let bytes = result.get("byteLength").and_then(Value::as_u64).unwrap_or(0);
        format!("Image included as MCP image content.\n\nMIME type: {mime}\nSize: {} bytes", human_bytes(bytes))
    } else {
        "No image data was returned.".to_string()
    }
}

fn notes_list_markdown(result: &Value) -> String {
    let folders = result.get("folders").and_then(Value::as_array).cloned().unwrap_or_default();
    let notes = result.get("notes").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut output = format!("# iCloud Notes ({})\n\n", notes.len());
    if !folders.is_empty() {
        let names = folders.iter().map(|folder| string_field(folder, "name")).collect::<Vec<_>>().join(", ");
        output.push_str(&format!("Folders: {names}\n\n"));
    }
    if notes.is_empty() {
        output.push_str("No notes were returned for this page.");
    } else {
        output.push_str("| # | ID | Title | Folder | Pinned | Modified at | Snippet |\n");
        output.push_str("| --- | --- | --- | --- | --- | --- | --- |\n");
        for (index, note) in notes.iter().enumerate() {
            output.push_str(&format!(
                "| {} | `{}` | {} | {} | {} | {} | {} |\n",
                index + 1,
                markdown_cell(string_field(note, "id")),
                markdown_cell(string_field(note, "title")),
                markdown_cell(string_field(note, "folderId")),
                yes_no(note.get("pinned")),
                number_field(note, "modifiedAt"),
                markdown_cell(truncate_text(&string_field(note, "snippet"), 240)),
            ));
        }
    }
    if let Some(cursor) = result.get("nextCursor").and_then(Value::as_str).filter(|value| !value.is_empty()) {
        output.push_str(&format!("\n\nNext cursor: `{cursor}`"));
    }
    output
}

fn note_detail_markdown(result: &Value) -> String {
    let Some(note) = result.get("note").filter(|value| !value.is_null()) else {
        return format!("# iCloud Note\n\n{}", string_field(result, "message"));
    };
    let attachments = note.get("attachments").and_then(Value::as_array).map(Vec::len).unwrap_or(0);
    let mut output = format!(
        "# {}\n\n| Field | Value |\n| --- | --- |\n| Note ID | `{}` |\n| Folder ID | {} |\n| Created at | {} |\n| Modified at | {} |\n| Attachments | {} |\n\n",
        string_field(note, "title"),
        markdown_cell(string_field(note, "id")),
        markdown_cell(string_field(note, "folderId")),
        number_field(note, "createdAt"),
        number_field(note, "modifiedAt"),
        attachments,
    );
    output.push_str("## Body\n\n");
    output.push_str(&truncate_text(&string_field(note, "body"), 4_000));
    if string_field(note, "body").chars().count() > 4_000 {
        output.push_str("\n\n[Body summary truncated. Read structuredContent.note.body for the full text.]");
    }
    output
}

fn note_mutation_markdown(result: &Value) -> String {
    if let Some(message) = result.get("message").and_then(Value::as_str).filter(|value| !value.is_empty()) {
        return format!("# iCloud Note\n\n{message}");
    }
    let rows = vec![("Status", string_field(result, "status")), ("Note ID", code_field(result, "noteId"))];
    key_value_markdown("iCloud Note", &rows)
}
fn drive_items_markdown(result: &Value) -> String {
    if let Some(message) = result.get("message").and_then(Value::as_str).filter(|value| !value.is_empty()) {
        return format!("# iCloud Drive\n\n{message}");
    }
    let items = result.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut output = format!("# iCloud Drive ({})\n\n", items.len());
    if items.is_empty() {
        output.push_str("No items were returned for this folder.");
        return output;
    }
    output.push_str("| Name | Kind | Size | Modified | Item ID | Download URL |\n");
    output.push_str("| --- | --- | --- | --- | --- | --- |\n");
    for item in items {
        let size = item.get("sizeBytes").and_then(Value::as_u64).map(human_bytes).unwrap_or_else(|| "-".to_string());
        let url = string_field(&item, "downloadUrl");
        output.push_str(&format!(
            "| {} | {} | {} | {} | `{}` | {} |\n",
            markdown_cell(string_field(&item, "name")),
            markdown_cell(string_field(&item, "kind")),
            size,
            markdown_cell(string_field(&item, "dateModified")),
            markdown_cell(string_field(&item, "id")),
            if url.is_empty() { "-".to_string() } else { format!("[download]({url})") },
        ));
    }
    output
}

fn drive_download_markdown(result: &Value) -> String {
    let rows = vec![
        ("File name", string_field(result, "fileName")),
        ("Size", human_bytes(result.get("sizeBytes").and_then(Value::as_u64).unwrap_or(0))),
        ("Path", code_field(result, "path")),
    ];
    key_value_markdown("iCloud Drive download", &rows)
}

fn contacts_markdown(result: &Value) -> String {
    let contacts = result.get("contacts").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut output = format!("# iCloud Contacts ({})\n\n", contacts.len());
    if contacts.is_empty() {
        output.push_str("No contacts were returned.");
    } else {
        output.push_str("| Name | Company | Phones | Emails | Birthday | Contact ID |\n");
        output.push_str("| --- | --- | --- | --- | --- | --- |\n");
        for contact in contacts {
            let phones = field_values(contact.get("phones"));
            let emails = field_values(contact.get("emails"));
            output.push_str(&format!(
                "| {} | {} | {} | {} | {} | `{}` |\n",
                markdown_cell(string_field(&contact, "displayName")),
                markdown_cell(string_field(&contact, "companyName")),
                markdown_cell(&phones),
                markdown_cell(&emails),
                markdown_cell(string_field(&contact, "birthday")),
                markdown_cell(string_field(&contact, "id")),
            ));
        }
        output.push_str(
            "\nUse `structuredContent.contacts[].photoUrl` with `cirrus_get_contact_photo` when a photo is available.",
        );
    }
    output
}

fn calendar_markdown(result: &Value) -> String {
    let calendars = result.get("calendars").and_then(Value::as_array).cloned().unwrap_or_default();
    let events = result.get("events").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut output = format!("# iCloud Calendar ({} events)\n\n", events.len());
    if !calendars.is_empty() {
        let names = calendars.iter().map(|calendar| string_field(calendar, "title")).collect::<Vec<_>>().join(", ");
        output.push_str(&format!("Calendars: {names}\n\n"));
    }
    if events.is_empty() {
        output.push_str("No events were returned for this date range.");
    } else {
        output.push_str("| Title | Start | End | Calendar ID | Location | All day | Recurring |\n");
        output.push_str("| --- | --- | --- | --- | --- | --- | --- |\n");
        for event in events {
            output.push_str(&format!(
                "| {} | {} | {} | `{}` | {} | {} | {} |\n",
                markdown_cell(string_field(&event, "title")),
                markdown_cell(string_field(&event, "start")),
                markdown_cell(string_field(&event, "end")),
                markdown_cell(string_field(&event, "calendarId")),
                markdown_cell(string_field(&event, "location")),
                yes_no(event.get("allDay")),
                yes_no(event.get("recurring")),
            ));
        }
    }
    output
}

fn key_value_markdown(title: &str, rows: &[(&str, String)]) -> String {
    let mut output = format!("# {title}\n\n| Field | Value |\n| --- | --- |\n");
    for (label, value) in rows {
        output.push_str(&format!("| {label} | {} |\n", markdown_cell(value)));
    }
    output
}

fn string_field(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

fn code_field(value: &Value, key: &str) -> String {
    let text = string_field(value, key);
    if text.is_empty() {
        "Unknown".to_string()
    } else {
        format!("`{text}`")
    }
}

fn number_field(value: &Value, key: &str) -> String {
    value.get(key).map(|value| value.to_string()).unwrap_or_else(|| "Unknown".to_string())
}

fn byte_size_field(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_u64).map(human_bytes).unwrap_or_else(|| "Unknown".to_string())
}

fn human_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let value = bytes as f64;
    if value >= GIB {
        format!("{:.2} GiB", value / GIB)
    } else if value >= MIB {
        format!("{:.2} MiB", value / MIB)
    } else if value >= KIB {
        format!("{:.1} KiB", value / KIB)
    } else {
        format!("{bytes} bytes")
    }
}

fn field_values(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| string_field(item, "value"))
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}

fn truncate_text(value: &str, limit: usize) -> String {
    let mut chars = value.chars();
    let truncated = chars.by_ref().take(limit).collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

fn tool_error_text(message: &str) -> String {
    let lower = message.to_ascii_lowercase();
    if lower.contains("accountkey") {
        format!(
            "{message}\n\nCall cirrus_list_accounts first and pass the returned accounts[].accountKey as accountKey."
        )
    } else if lower.contains("deviceid") {
        format!("{message}\n\nCall cirrus_list_devices first and pass the exact deviceId for the target device.")
    } else {
        message.to_string()
    }
}

fn devices_summary_markdown(result: &Value, mode: ArtworkMode) -> String {
    let devices = result.get("devices").and_then(Value::as_array).cloned().unwrap_or_default();
    let online =
        devices.iter().filter(|device| device.get("deviceStatus").and_then(Value::as_str) == Some("200")).count();
    let offline = devices.len().saturating_sub(online);
    let mut output = format!("# Find My devices ({})\n\n{} online, {} offline.", devices.len(), online, offline);
    for device in devices {
        let name = device.get("name").and_then(Value::as_str).unwrap_or("Apple device");
        let model = device
            .get("deviceDisplayName")
            .and_then(Value::as_str)
            .or_else(|| device.get("modelDisplayName").and_then(Value::as_str))
            .unwrap_or("Unknown");
        output.push_str(&format!(
            "\n- **{}** ({}) — {} · battery {} · activation lock {} · last location {}",
            name,
            model,
            device_status_text(&device),
            device_battery_text(&device),
            device_activation_lock_text(&device),
            device_last_location_updated_text(&device),
        ));
        if mode == ArtworkMode::Url {
            if let Some(url) = device_artwork_url(&device) {
                output.push_str(&format!("\n  ![Device artwork]({url})"));
            }
        } else if mode == ArtworkMode::DataUri {
            output.push_str("\n  Artwork embedded as native image content.");
        }
    }
    output
}

fn devices_markdown(result: &Value) -> String {
    let devices = result.get("devices").and_then(Value::as_array).cloned().unwrap_or_default();
    let count = result.get("deviceCount").and_then(Value::as_u64).unwrap_or(devices.len() as u64);
    let mut output = format!("# Find My devices ({count})\n\n");
    if devices.is_empty() {
        output.push_str("No devices were returned for this account.");
        return output;
    }
    output.push_str("| Name | Model | Status | Activation lock | Battery | Last location update | Location | Artwork | Device ID |\n");
    output.push_str("| --- | --- | --- | --- | --- | --- | --- | --- | --- |\n");
    for device in devices {
        let name = device.get("name").and_then(Value::as_str).unwrap_or("Apple device");
        let model = device
            .get("deviceDisplayName")
            .and_then(Value::as_str)
            .or_else(|| device.get("modelDisplayName").and_then(Value::as_str))
            .unwrap_or("Unknown");
        let id = device.get("deviceId").and_then(Value::as_str).unwrap_or("Unknown");
        output.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | `{}` |\n",
            markdown_cell(name),
            markdown_cell(model),
            device_status_text(&device),
            device_activation_lock_text(&device),
            device_battery_text(&device),
            device_last_location_updated_text(&device),
            markdown_cell(device_location_text(&device)),
            device_artwork_text(&device),
            markdown_cell(id),
        ));
    }
    if let Some(retrieved_at) = result.get("retrievedAt").and_then(Value::as_i64) {
        output.push_str(&format!("\n\nData retrieved: {}. The per-device update time is the latest location report from Find My, not a general device-online timestamp.", format_timestamp_millis(retrieved_at)));
    }
    output.push_str("\n\nUse the exact `deviceId` with `cirrus_play_sound`.");
    output
}

fn accounts_markdown(result: &Value) -> String {
    let accounts = result.get("accounts").and_then(Value::as_array).cloned().unwrap_or_default();
    let count = result.get("count").and_then(Value::as_u64).unwrap_or(accounts.len() as u64);
    let mut output = format!("# MCP-enabled iCloud accounts ({count})\n\n");
    if accounts.is_empty() {
        output.push_str("No MCP-enabled accounts are available.");
        return output;
    }
    output.push_str("| Name | Apple ID | Account key | Signed in | Active |\n");
    output.push_str("| --- | --- | --- | --- | --- |\n");
    for account in accounts {
        let first = account.get("firstName").and_then(Value::as_str).unwrap_or("");
        let last = account.get("lastName").and_then(Value::as_str).unwrap_or("");
        let display_name = format!("{first} {last}").trim().to_string();
        let name = if display_name.is_empty() {
            account.get("appleId").and_then(Value::as_str).unwrap_or("Unknown")
        } else {
            &display_name
        };
        output.push_str(&format!(
            "| {} | {} | `{}` | {} | {} |\n",
            markdown_cell(name),
            markdown_cell(account.get("appleId").and_then(Value::as_str).unwrap_or("Unknown")),
            markdown_cell(account.get("accountKey").and_then(Value::as_str).unwrap_or("Unknown")),
            yes_no(account.get("signedIn")),
            yes_no(account.get("active")),
        ));
    }
    output.push_str("\nUse the exact `accountKey` for account-scoped tools.");
    output
}

fn markdown_cell(value: impl AsRef<str>) -> String {
    value.as_ref().replace('\\', "\\\\").replace('|', "\\|").replace(['\r', '\n'], " ").trim().to_string()
}

fn yes_no(value: Option<&Value>) -> &'static str {
    match value.and_then(Value::as_bool) {
        Some(true) => "yes",
        Some(false) => "no",
        None => "unknown",
    }
}

fn device_status_text(device: &Value) -> &'static str {
    match device.get("deviceStatus").and_then(Value::as_str) {
        Some("200") => "Online",
        _ => "Offline",
    }
}

fn device_activation_lock_text(device: &Value) -> &'static str {
    match device.get("activationLocked").and_then(Value::as_bool) {
        Some(true) => "Locked",
        Some(false) => "Unlocked",
        None => "Unknown",
    }
}

fn device_battery_text(device: &Value) -> String {
    if device.get("batteryStatus").and_then(Value::as_str).is_some_and(|value| value.eq_ignore_ascii_case("unknown")) {
        return "Unknown".to_string();
    }
    device
        .get("batteryLevel")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .map(|value| format!("{:.0}%", value.clamp(0.0, 1.0) * 100.0))
        .unwrap_or_else(|| "Unknown".to_string())
}

fn device_location_text(device: &Value) -> String {
    let Some(location) = device.get("location").filter(|value| !value.is_null()) else {
        return if device.get("locationEnabled").and_then(Value::as_bool) == Some(false) {
            "Services off".to_string()
        } else {
            "Unavailable".to_string()
        };
    };
    let (Some(latitude), Some(longitude)) =
        (location.get("latitude").and_then(Value::as_f64), location.get("longitude").and_then(Value::as_f64))
    else {
        return "Unavailable".to_string();
    };
    let accuracy = location
        .get("horizontalAccuracy")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| format!(" ±{value:.0} m"))
        .unwrap_or_default();
    let stale = location.get("isOld").and_then(Value::as_bool).unwrap_or(false);
    format!("{latitude:.6}, {longitude:.6}{accuracy}{}", if stale { " · stale" } else { "" })
}

fn device_last_location_updated_at(device: &Value) -> Option<i64> {
    device
        .get("lastLocationUpdatedAt")
        .and_then(Value::as_i64)
        .or_else(|| device.get("location").and_then(|location| location.get("timeStamp")).and_then(Value::as_i64))
}

fn device_last_location_updated_text(device: &Value) -> String {
    device_last_location_updated_at(device).map(format_timestamp_millis).unwrap_or_else(|| "Unknown".to_string())
}

fn format_timestamp_millis(value: i64) -> String {
    DateTime::<Utc>::from_timestamp_millis(value)
        .map(|timestamp| timestamp.to_rfc3339_opts(SecondsFormat::Secs, true))
        .unwrap_or_else(|| value.to_string())
}

fn device_artwork_url(device: &Value) -> Option<&str> {
    device
        .pointer("/artwork/url")
        .and_then(Value::as_str)
        .or_else(|| device.get("deviceImageUrl").and_then(Value::as_str))
        .filter(|url| is_apple_device_image_url(url))
}

fn annotate_device_artwork_url(device: &mut Value) {
    let url = device
        .get("deviceImageUrl")
        .and_then(Value::as_str)
        .filter(|url| is_apple_device_image_url(url))
        .map(str::to_string);
    let alt = device_label(device);
    let Some(device_object) = device.as_object_mut() else { return };
    match url {
        Some(url) => {
            device_object.insert("artwork".to_string(), json!({ "mimeType": "image/png", "url": url, "alt": alt }));
        }
        None => {
            device_object.insert("artwork".to_string(), Value::Null);
        }
    }
}

fn remove_device_artwork(device: &mut Value) {
    let Some(device) = device.as_object_mut() else { return };
    for key in ["artwork", "deviceImageUrl", "deviceImageMimeType", "deviceImageDataUrl", "deviceImageError"] {
        device.remove(key);
    }
}

fn device_artwork_text(device: &Value) -> &'static str {
    if device.pointer("/artwork/dataUri").and_then(Value::as_str).is_some() {
        "Embedded"
    } else if device.pointer("/artwork/url").and_then(Value::as_str).is_some()
        || device.get("deviceImageUrl").and_then(Value::as_str).is_some()
    {
        "URL"
    } else if device.get("deviceImageMimeType").and_then(Value::as_str).is_some() {
        "Included"
    } else if device.get("deviceImageError").and_then(Value::as_str).is_some() {
        "Unavailable"
    } else {
        "Not provided"
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
    let status = match device.get("deviceStatus").and_then(Value::as_str) {
        Some("200") => "Status: online",
        _ => "Status: offline",
    };
    let battery = device
        .get("batteryLevel")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .map(|value| format!("Battery: {:.0}%", value.clamp(0.0, 1.0) * 100.0));
    let activation = match device.get("activationLocked").and_then(Value::as_bool) {
        Some(true) => Some("Activation lock: enabled".to_string()),
        Some(false) => Some("Activation lock: disabled".to_string()),
        None => None,
    };
    let updated = device_last_location_updated_at(device)
        .map(|value| format!("Last location update: {}", format_timestamp_millis(value)));
    [Some(device_label(device)), Some(location), Some(status.to_string()), activation, battery, updated]
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

async fn result_image_content(tool: &str, result: &mut Value, mode: ArtworkMode) -> Vec<Value> {
    if tool == "cirrus_get_note" {
        if let Some(segments) = result.get("contentSegments").and_then(Value::as_array) {
            let note = result.get("note").unwrap_or(&Value::Null);
            let mut content = vec![json!({ "type": "text", "text": note_inline_header(note) })];
            let mut image_count = 0_u64;
            for segment in segments {
                match segment.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        if let Some(text) =
                            segment.get("text").and_then(Value::as_str).filter(|text| !text.trim().is_empty())
                        {
                            content.push(json!({ "type": "text", "text": text }));
                        }
                    }
                    Some("image") => {
                        let data_url = segment.get("dataUrl").and_then(Value::as_str).unwrap_or_default();
                        if let Some((mime_type, encoded, _)) = parse_image_data_url(data_url) {
                            image_count += 1;
                            content.push(json!({ "type": "image", "data": encoded, "mimeType": mime_type }));
                        } else {
                            let name = string_field(segment, "name");
                            content.push(
                                json!({ "type": "text", "text": format!("[Image could not be decoded: {name}]") }),
                            );
                        }
                    }
                    _ => {}
                }
            }
            if let Some(object) = result.as_object_mut() {
                object.insert("inlineImageCount".to_string(), json!(image_count));
            }
            return content;
        }
    }
    if tool == "cirrus_get_account_avatar" || tool == "cirrus_get_contact_photo" || tool == "cirrus_get_note_image" {
        let data_url = result.get("dataUrl").and_then(Value::as_str).unwrap_or_default();
        let image = parse_image_data_url(data_url);
        if let Some(object) = result.as_object_mut() {
            object.insert("imageIncluded".to_string(), json!(image.is_some()));
            if let Some((mime_type, _, byte_length)) = &image {
                object.insert("mimeType".to_string(), json!(mime_type));
                object.insert("byteLength".to_string(), json!(byte_length));
            }
        }
        return image
            .map(|(mime_type, encoded, _)| vec![json!({ "type": "image", "data": encoded, "mimeType": mime_type })])
            .unwrap_or_default();
    }
    if tool != "cirrus_list_devices" {
        return Vec::new();
    }
    let Some(devices) = result.get_mut("devices").and_then(Value::as_array_mut) else {
        return Vec::new();
    };
    if mode != ArtworkMode::DataUri {
        for device in devices {
            if mode == ArtworkMode::Url {
                annotate_device_artwork_url(device);
            } else {
                remove_device_artwork(device);
            }
        }
        return Vec::new();
    }
    let Ok(client) = reqwest::Client::builder().timeout(std::time::Duration::from_secs(15)).build() else {
        return Vec::new();
    };
    let mut content = Vec::new();
    for device in devices {
        annotate_device_artwork_url(device);
        let alt = device_label(device);
        let (items, artwork, error) = device_image_content(&client, device).await;
        content.extend(items);
        if let Some(device) = device.as_object_mut() {
            if let Some((mime_type, data_url)) = artwork {
                device.insert("deviceImageMimeType".to_string(), json!(mime_type));
                device.insert("deviceImageDataUrl".to_string(), json!(data_url.clone()));
                let artwork = device.entry("artwork").or_insert_with(|| json!({ "alt": alt }));
                if let Some(artwork) = artwork.as_object_mut() {
                    artwork.insert("mimeType".to_string(), json!(mime_type));
                    artwork.insert("dataUri".to_string(), json!(data_url));
                }
            }
            if let Some(error) = error {
                device.insert("deviceImageError".to_string(), json!(error));
            }
        }
    }
    content
}

fn parse_image_data_url(value: &str) -> Option<(String, String, usize)> {
    let rest = value.strip_prefix("data:")?;
    let (metadata, encoded) = rest.split_once(',')?;
    let mime_type = metadata.split(';').next().filter(|value| value.starts_with("image/"))?;
    if !metadata.split(';').any(|part| part.eq_ignore_ascii_case("base64")) {
        return None;
    }
    let bytes = BASE64.decode(encoded).ok()?;
    Some((mime_type.to_string(), encoded.to_string(), bytes.len()))
}

fn annotate_device_list_timestamps(result: &mut Value, retrieved_at: i64) {
    let Some(result) = result.as_object_mut() else { return };
    result.insert("retrievedAt".to_string(), json!(retrieved_at));
    let Some(devices) = result.get_mut("devices").and_then(Value::as_array_mut) else { return };
    for device in devices {
        let updated_at = device.pointer("/location/timeStamp").and_then(Value::as_i64);
        if let Some(device) = device.as_object_mut() {
            device.insert("lastLocationUpdatedAt".to_string(), json!(updated_at));
        }
    }
}

fn result_for_structured_content(tool: &str, result: &Value, text_result: &Value, mode: ArtworkMode) -> Value {
    if tool == "cirrus_list_devices" && mode == ArtworkMode::DataUri {
        let mut structured = result.clone();
        strip_flat_device_image_data_urls(&mut structured);
        structured
    } else {
        text_result.clone()
    }
}

fn strip_flat_device_image_data_urls(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            fields.remove("deviceImageDataUrl");
            for value in fields.values_mut() {
                strip_flat_device_image_data_urls(value);
            }
        }
        Value::Array(values) => {
            for value in values {
                strip_flat_device_image_data_urls(value);
            }
        }
        _ => {}
    }
}

fn result_without_inline_artwork(result: &Value) -> Value {
    let mut visible = result.clone();
    strip_inline_artwork(&mut visible);
    visible
}

fn strip_inline_artwork(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            fields.remove("deviceImageDataUrl");
            fields.remove("dataUrl");
            fields.remove("dataUri");
            for value in fields.values_mut() {
                strip_inline_artwork(value);
            }
        }
        Value::Array(values) => {
            for value in values {
                strip_inline_artwork(value);
            }
        }
        _ => {}
    }
}

#[derive(Clone)]
pub struct BridgeTarget {
    pub base_url: String,
    pub token: String,
}

async fn call_bridge(
    tool: &str,
    arguments: &Value,
    client: &ClientIdentity,
    bridge: Option<&BridgeTarget>,
) -> Result<Value, String> {
    let target = match bridge {
        Some(target) => target.clone(),
        None => discover_bridge()?,
    };
    let response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(13 * 60))
        // MCP clients can inherit a system proxy; the Cirrus bridge is loopback-only.
        .no_proxy()
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
    let runtime_file = std::env::var_os("CIRRUS_MCP_RUNTIME_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| port_file.with_file_name(BRIDGE_RUNTIME_FILE));
    let token_file = std::env::var_os("CIRRUS_MCP_TOKEN_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| data_dir.join("mcp-bridge-token"));
    discover_bridge_from_paths(&runtime_file, &port_file, &token_file)
}

fn discover_bridge_from_paths(
    runtime_file: &Path,
    port_file: &Path,
    token_file: &Path,
) -> Result<BridgeTarget, String> {
    if let Some(target) = read_bridge_runtime_file(runtime_file) {
        return Ok(target);
    }
    let port = read_trimmed(port_file)
        .ok_or_else(|| "Cirrus MCP bridge is not running. Open Cirrus and enable MCP in Settings.".to_string())?;
    let port: u16 = port.parse().map_err(|_| "Cirrus MCP bridge port file is invalid".to_string())?;
    let token = read_trimmed(token_file)
        .ok_or_else(|| "Cirrus MCP bridge token is unavailable. Re-enable MCP in Cirrus Settings.".to_string())?;
    if token.len() < 24 {
        return Err("Cirrus MCP bridge token is invalid. Re-enable MCP in Cirrus Settings.".to_string());
    }
    Ok(BridgeTarget { base_url: format!("http://127.0.0.1:{port}"), token })
}

fn read_bridge_runtime_file(path: &Path) -> Option<BridgeTarget> {
    let content = std::fs::read_to_string(path).ok()?;
    let value = serde_json::from_str::<Value>(&content).ok()?;
    if value.get("version").and_then(Value::as_u64) != Some(1) {
        return None;
    }
    let port = value.get("port").and_then(Value::as_u64)?;
    let port = u16::try_from(port).ok().filter(|port| *port != 0)?;
    let token = value.get("token").and_then(Value::as_str)?.trim();
    if token.len() < 24 {
        return None;
    }
    Some(BridgeTarget { base_url: format!("http://127.0.0.1:{port}"), token: token.to_string() })
}

fn read_trimmed(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok().map(|value| value.trim().to_string()).filter(|value| !value.is_empty())
}

fn app_data_dir() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("CIRRUS_DATA_DIR").filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(path));
    }
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
    fn bridge_runtime_file_is_a_single_discovery_document() {
        let dir = std::env::temp_dir().join(format!("cirrus-mcp-runtime-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create dir");
        let path = dir.join(BRIDGE_RUNTIME_FILE);
        std::fs::write(&path, r#"{"version":1,"port":60241,"token":"c8ef2f10b25f483cae6ca03e855e2629"}"#)
            .expect("write runtime");

        let target = read_bridge_runtime_file(&path).expect("runtime target");
        assert_eq!(target.base_url, "http://127.0.0.1:60241");
        assert_eq!(target.token, "c8ef2f10b25f483cae6ca03e855e2629");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn bridge_runtime_file_rejects_short_tokens() {
        let dir = std::env::temp_dir().join(format!("cirrus-mcp-runtime-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create dir");
        let path = dir.join(BRIDGE_RUNTIME_FILE);
        std::fs::write(&path, r#"{"version":1,"port":60241,"token":"short"}"#).expect("write runtime");

        assert!(read_bridge_runtime_file(&path).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn bridge_runtime_file_takes_precedence_over_legacy_files() {
        let dir = std::env::temp_dir().join(format!("cirrus-mcp-runtime-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create dir");
        let runtime = dir.join(BRIDGE_RUNTIME_FILE);
        let port = dir.join("mcp-bridge-port");
        let token = dir.join("mcp-bridge-token");
        std::fs::write(&runtime, r#"{"version":1,"port":60241,"token":"c8ef2f10b25f483cae6ca03e855e2629"}"#)
            .expect("runtime");
        std::fs::write(&port, "51997").expect("legacy port");
        std::fs::write(&token, "f2eefde5cd7b4f14933d261236dfd9c2").expect("legacy token");

        let target = discover_bridge_from_paths(&runtime, &port, &token).expect("preferred target");
        assert_eq!(target.base_url, "http://127.0.0.1:60241");
        assert_eq!(target.token, "c8ef2f10b25f483cae6ca03e855e2629");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn bridge_discovery_falls_back_to_legacy_files() {
        let dir = std::env::temp_dir().join(format!("cirrus-mcp-runtime-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create dir");
        let runtime = dir.join(BRIDGE_RUNTIME_FILE);
        let port = dir.join("mcp-bridge-port");
        let token = dir.join("mcp-bridge-token");
        std::fs::write(&runtime, "not json").expect("runtime");
        std::fs::write(&port, "60241").expect("legacy port");
        std::fs::write(&token, "c8ef2f10b25f483cae6ca03e855e2629").expect("legacy token");

        let target = discover_bridge_from_paths(&runtime, &port, &token).expect("legacy target");
        assert_eq!(target.base_url, "http://127.0.0.1:60241");
        assert_eq!(target.token, "c8ef2f10b25f483cae6ca03e855e2629");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn tools_list_exposes_login_and_account_list_without_arguments() {
        let tools = tools_list();
        let tools = tools.get("tools").and_then(Value::as_array).expect("tools");
        assert_eq!(tools.len(), 25);
        assert_eq!(tools[0].get("name").and_then(Value::as_str), Some(LOGIN_TOOL));
        assert_eq!(
            tools[0].get("inputSchema").and_then(|value| value.get("additionalProperties")).and_then(Value::as_bool),
            Some(false)
        );
        let devices = tools
            .iter()
            .find(|tool| tool.get("name").and_then(Value::as_str) == Some("cirrus_list_devices"))
            .expect("devices tool");
        assert!(devices.get("outputSchema").is_some());
    }

    #[test]
    fn every_tool_documents_inputs_and_structured_output() {
        let tools = tools_list();
        let tools = tools.get("tools").and_then(Value::as_array).expect("tools");
        for tool in tools {
            let name = tool.get("name").and_then(Value::as_str).expect("tool name");
            let schema = tool.get("outputSchema").unwrap_or_else(|| panic!("{name} outputSchema"));
            assert_eq!(schema.get("type").and_then(Value::as_str), Some("object"), "{name}");
            assert!(
                schema.get("properties").and_then(Value::as_object).is_some_and(|properties| !properties.is_empty()),
                "{name} output properties"
            );

            let properties = tool
                .get("inputSchema")
                .and_then(|value| value.get("properties"))
                .and_then(Value::as_object)
                .expect("input properties");
            for (property, definition) in properties {
                assert!(
                    definition.get("description").and_then(Value::as_str).is_some_and(|value| !value.trim().is_empty()),
                    "{name}.{property} must have a description"
                );
            }
        }
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
    fn device_text_is_a_readable_table_without_inline_artwork() {
        let result = json!({
            "deviceCount": 1,
            "devices": [{
                "name": "Test Phone",
                "deviceDisplayName": "iPhone",
                "deviceId": "device-id-123",
                "deviceStatus": "200",
                "activationLocked": false,
                "batteryLevel": 0.82,
                "deviceImageMimeType": "image/png",
                "deviceImageDataUrl": "data:image/png;base64,large",
                "location": {
                    "latitude": 30.1234567,
                    "longitude": 120.7654321,
                    "horizontalAccuracy": 9.6,
                    "timeStamp": 1_700_000_000_000i64,
                    "isOld": false
                }
            }]
        });
        let visible = result_without_inline_artwork(&result);
        let text = tool_result_text("cirrus_list_devices", &visible);
        assert!(text.contains("# Find My devices (1)"));
        assert!(text.contains(
            "| Test Phone | iPhone | Online | Unlocked | 82% | 2023-11-14T22:13:20Z | 30.123457, 120.765432 ±10 m | Included | `device-id-123` |"
        ));
        assert!(!text.contains("base64"));
        assert!(!visible.to_string().contains("base64"));
    }

    #[test]
    fn account_and_device_errors_explain_the_next_step() {
        assert!(tool_error_text("Missing required argument: accountKey").contains("Call cirrus_list_accounts first"));
        assert!(tool_error_text("missing deviceId").contains("Call cirrus_list_devices first"));
    }

    #[test]
    fn device_output_schema_exposes_identifiers_and_location() {
        let schema = device_output_schema();
        let properties = &schema["properties"]["devices"]["items"]["properties"];
        assert!(properties.get("deviceId").is_some());
        assert!(properties.get("location").is_some());
        assert!(properties.get("deviceImageDataUrl").is_none());
        assert!(properties.get("deviceImageMimeType").is_some());
        assert!(properties.get("activationLocked").is_some());
        assert!(properties.get("lastLocationUpdatedAt").is_some());
        assert!(schema["properties"].get("retrievedAt").is_some());
    }

    #[test]
    fn device_text_uses_native_image_block_without_external_markdown() {
        let result = json!({
            "retrievedAt": 1_700_000_000_000i64,
            "deviceCount": 1,
            "devices": [{
                "name": "Test Phone",
                "deviceDisplayName": "iPhone",
                "deviceId": "device-id-123",
                "deviceStatus": "200",
                "lastLocationUpdatedAt": 1_700_000_000_000i64,
                "deviceImageUrl": "https://statici.icloud.com/fmipmobile/deviceImages-9.0/iPhone/iPhone18,3/online-sourcelist.png",
                "deviceImageMimeType": "image/png",
                "location": {
                    "latitude": 30.1234567,
                    "longitude": 120.7654321,
                    "horizontalAccuracy": 9.6,
                    "timeStamp": 1_700_000_000_000i64,
                    "isOld": false
                }
            }]
        });
        let text = tool_result_text("cirrus_list_devices", &result);
        assert!(!text.contains("![Device artwork](https://"));
        assert!(text.contains("| URL |"));
        assert!(text.contains("2023-11-14T22:13:20Z"));
        assert!(text.contains("Data retrieved: 2023-11-14T22:13:20Z"));
    }

    #[test]
    fn device_list_timestamps_are_annotated() {
        let mut result = json!({
            "deviceCount": 1,
            "devices": [{ "location": { "timeStamp": 1_700_000_000_000i64 } }]
        });
        annotate_device_list_timestamps(&mut result, 1_700_000_100_000i64);
        assert_eq!(result["retrievedAt"], 1_700_000_100_000i64);
        assert_eq!(result["devices"][0]["lastLocationUpdatedAt"], 1_700_000_000_000i64);
    }

    #[test]
    fn image_results_are_moved_out_of_structured_content() {
        let result = json!({
            "dataUrl": "data:image/png;base64,YWJj",
            "imageIncluded": true,
            "mimeType": "image/png",
            "byteLength": 3
        });
        let visible = result_without_inline_artwork(&result);
        assert!(visible.get("dataUrl").is_none());
        assert_eq!(visible["imageIncluded"], true);
        let text = tool_result_text("cirrus_get_account_avatar", &visible);
        assert!(text.contains("image/png"));
        assert!(!text.contains("YWJj"));
        let parsed = parse_image_data_url("data:image/png;base64,YWJj").expect("data URL");
        assert_eq!(parsed.0, "image/png");
        assert_eq!(parsed.2, 3);
    }

    #[test]
    fn device_structured_content_does_not_duplicate_base64_artwork() {
        let result = json!({
            "devices": [{
                "name": "Phone",
                "deviceImageMimeType": "image/png",
                "deviceImageDataUrl": "data:image/png;base64,YWJj"
            }]
        });
        let text_result = result_without_inline_artwork(&result);
        let structured = result_for_structured_content("cirrus_list_devices", &result, &text_result, ArtworkMode::Url);
        assert!(structured["devices"][0].get("deviceImageDataUrl").is_none());
        assert_eq!(structured["devices"][0]["deviceImageMimeType"], "image/png");
        assert!(text_result["devices"][0].get("deviceImageDataUrl").is_none());
    }

    #[tokio::test]
    async fn image_tools_return_native_base64_image_content() {
        let mut result = json!({ "dataUrl": "data:image/png;base64,YWJj" });
        let content = result_image_content("cirrus_get_account_avatar", &mut result, ArtworkMode::Url).await;
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "image");
        assert_eq!(content[0]["data"], "YWJj");
        assert_eq!(content[0]["mimeType"], "image/png");
        assert_eq!(result["imageIncluded"], true);
        assert_eq!(result["byteLength"], 3);

        let mut note_image = json!({
            "dataUrl": "data:image/png;base64,YWJj",
            "attachmentId": "attachment-1",
            "name": "photo.png",
            "source": "preview"
        });
        let note_content = result_image_content("cirrus_get_note_image", &mut note_image, ArtworkMode::Url).await;
        assert_eq!(note_content.len(), 1);
        assert_eq!(note_content[0]["type"], "image");
        assert_eq!(note_content[0]["data"], "YWJj");
        assert_eq!(note_content[0]["mimeType"], "image/png");
        assert_eq!(note_image["imageIncluded"], true);
        assert_eq!(note_image["attachmentId"], "attachment-1");
    }

    #[tokio::test]
    async fn note_content_interleaves_text_and_images_in_document_order() {
        let mut result = json!({
            "note": {
                "title": "Photo note",
                "folderId": "DefaultFolder-CloudKit",
                "modifiedAt": 1700000000000_i64
            },
            "contentSegments": [
                { "type": "text", "text": "before" },
                {
                    "type": "image",
                    "attachmentId": "image-1",
                    "name": "photo.png",
                    "dataUrl": "data:image/png;base64,YWJj"
                },
                { "type": "text", "text": "after" }
            ]
        });
        let content = result_image_content("cirrus_get_note", &mut result, ArtworkMode::Url).await;
        assert_eq!(content.len(), 4);
        assert_eq!(content[0]["type"], "text");
        assert!(content[0]["text"].as_str().expect("header").contains("Photo note"));
        assert_eq!(content[1]["text"], "before");
        assert_eq!(content[2]["type"], "image");
        assert_eq!(content[2]["data"], "YWJj");
        assert_eq!(content[3]["text"], "after");
        assert_eq!(result["inlineImageCount"], 1);
    }

    #[test]
    fn artwork_mode_defaults_to_url_and_supports_explicit_modes() {
        assert_eq!(artwork_mode(&json!({})), ArtworkMode::Url);
        assert_eq!(artwork_mode(&json!({ "includeArtwork": "url" })), ArtworkMode::Url);
        assert_eq!(artwork_mode(&json!({ "includeArtwork": "dataUri" })), ArtworkMode::DataUri);
        assert_eq!(artwork_mode(&json!({ "includeArtwork": "none" })), ArtworkMode::None);
    }

    #[tokio::test]
    async fn url_artwork_mode_returns_text_fallback_without_fetching_images() {
        let url = "https://statici.icloud.com/fmipmobile/deviceImages-9.0/iPhone/iPhone18,3/online-sourcelist.png";
        let mut result = json!({
            "devices": [{
                "name": "Test Phone",
                "deviceDisplayName": "iPhone",
                "deviceImageUrl": url
            }]
        });
        let content = result_image_content("cirrus_list_devices", &mut result, ArtworkMode::Url).await;
        assert!(content.is_empty());
        assert_eq!(result["devices"][0]["artwork"]["url"], url);
        assert!(result["devices"][0]["artwork"].get("dataUri").is_none());
        let summary = devices_summary_markdown(&result, ArtworkMode::Url);
        assert!(summary.contains(&format!("![Device artwork]({url})")));
    }

    #[test]
    fn data_uri_structured_content_keeps_nested_data_uri_only() {
        let result = json!({
            "devices": [{
                "artwork": {
                    "url": "https://statici.icloud.com/example.png",
                    "dataUri": "data:image/png;base64,YWJj"
                },
                "deviceImageDataUrl": "data:image/png;base64,YWJj"
            }]
        });
        let text_result = result_without_inline_artwork(&result);
        let structured =
            result_for_structured_content("cirrus_list_devices", &result, &text_result, ArtworkMode::DataUri);
        assert!(structured["devices"][0].get("deviceImageDataUrl").is_none());
        assert_eq!(structured["devices"][0]["artwork"]["dataUri"], "data:image/png;base64,YWJj");
        assert!(text_result["devices"][0]["artwork"].get("dataUri").is_none());
    }

    #[tokio::test]
    async fn in_process_http_server_uses_the_bound_listener_and_shuts_down() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind test listener");
        let address = listener.local_addr().expect("listener address");
        let config = HttpConfig {
            bind_addr: address,
            path: "/mcp".to_string(),
            token: "test-token".to_string(),
            allow_remote: false,
            bridge: None,
        };
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(serve_http_on_listener(listener, config, async move {
            let _ = stop_rx.await;
        }));

        let response = reqwest::get(format!("http://{address}/healthz")).await.expect("health response");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.json::<Value>().await.expect("health JSON")["transport"], "streamable-http");

        stop_tx.send(()).expect("request shutdown");
        task.await.expect("server task").expect("server shutdown");
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
