use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

const LATEST_PROTOCOL_VERSION: &str = "2025-06-18";
const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
const LOGIN_TOOL: &str = "cirrus_login";
const LIST_ACCOUNTS_TOOL: &str = "cirrus_list_accounts";
const CLIENT_NAME: &str = "cirrus-mcp";

#[derive(Debug, Clone, Default)]
struct ClientIdentity {
    name: Option<String>,
    version: Option<String>,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("cirrus-mcp");
        println!();
        println!("MCP stdio bridge for Cirrus. Configure it as a stdio MCP server, not a browser URL.");
        return;
    }
    if args.iter().any(|arg| arg == "--version" || arg == "-V") {
        println!("cirrus-mcp {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if !args.is_empty() {
        eprintln!("cirrus-mcp: unknown argument: {}", args[0]);
        std::process::exit(2);
    }

    if let Err(error) = run_stdio().await {
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
                "description": "List Apple devices, locations, battery status, and Find My state for an account.",
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
    if name != LOGIN_TOOL && name != LIST_ACCOUNTS_TOOL {
        return success_response(
            id,
            json!({
                "content": [{ "type": "text", "text": format!("Unknown Cirrus tool: {name}") }],
                "isError": true
            }),
        );
    }

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
        Ok(result) => success_response(
            id,
            json!({
                "content": [{ "type": "text", "text": serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string()) }],
                "structuredContent": result,
                "isError": false
            }),
        ),
        Err(message) => success_response(
            id,
            json!({
                "content": [{ "type": "text", "text": message }],
                "isError": true
            }),
        ),
    }
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
    fn errors_are_json_rpc_compatible() {
        let response = error_response(json!("request-1"), -32601, "Method not found");
        assert_eq!(response.get("jsonrpc").and_then(Value::as_str), Some("2.0"));
        assert_eq!(response.get("error").and_then(|value| value.get("code")).and_then(Value::as_i64), Some(-32601));
    }
}
