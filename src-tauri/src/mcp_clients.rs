use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Map, Value};
use toml_edit::{value, DocumentMut, Item, Table};

const SERVER_NAME: &str = "cirrus";
const CODEX_HTTP_ONLY_FIELDS: [&str; 8] = [
    "url",
    "bearer_token_env_var",
    "http_headers",
    "env_http_headers",
    "oauth_client_id",
    "oauth_client_secret",
    "oauth_client_registration",
    "oauth_resource",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClientKind {
    Claude,
    Cursor,
    CodeBuddy,
    ZCode,
    Trae,
    VsCode,
    Windsurf,
    Codex,
    DeepSeekHarness,
    OpenCode,
    Pi,
    CherryStudio,
    Qoder,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpClientInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub installed: bool,
    pub configured: bool,
    pub can_install: bool,
    pub supports_write: bool,
    pub config_path: Option<String>,
    pub config_snippet: Option<String>,
    pub blocked_reason: Option<String>,
}

struct ClientDefinition {
    kind: ClientKind,
    id: &'static str,
    name: &'static str,
}

const CLIENTS: [ClientDefinition; 13] = [
    ClientDefinition { kind: ClientKind::Claude, id: "claude", name: "Claude Code" },
    ClientDefinition { kind: ClientKind::Cursor, id: "cursor", name: "Cursor" },
    ClientDefinition { kind: ClientKind::CodeBuddy, id: "codebuddy", name: "CodeBuddy Code" },
    ClientDefinition { kind: ClientKind::ZCode, id: "zcode", name: "ZCode" },
    ClientDefinition { kind: ClientKind::Trae, id: "trae", name: "TRAE" },
    ClientDefinition { kind: ClientKind::VsCode, id: "vscode", name: "VS Code" },
    ClientDefinition { kind: ClientKind::Windsurf, id: "windsurf", name: "Windsurf" },
    ClientDefinition { kind: ClientKind::Codex, id: "codex", name: "Codex" },
    ClientDefinition { kind: ClientKind::DeepSeekHarness, id: "deepseek-harness", name: "DeepSeek Harness" },
    ClientDefinition { kind: ClientKind::OpenCode, id: "opencode", name: "OpenCode" },
    ClientDefinition { kind: ClientKind::Pi, id: "pi", name: "Pi" },
    ClientDefinition { kind: ClientKind::CherryStudio, id: "cherry-studio", name: "Cherry Studio" },
    ClientDefinition { kind: ClientKind::Qoder, id: "qoder", name: "Qoder" },
];

pub fn list_clients(binary: Option<&Path>, data_dir: Option<&Path>) -> Vec<McpClientInfo> {
    CLIENTS.iter().map(|definition| client_info(definition, binary, data_dir)).collect()
}

pub(crate) fn client_data_dir(data_dir: Option<&Path>) -> Option<&Path> {
    let custom_data_dir = std::env::var_os("CIRRUS_DATA_DIR").is_some_and(|value| !value.is_empty());
    env_data_dir_for_client(data_dir, custom_data_dir)
}

fn env_data_dir_for_client(data_dir: Option<&Path>, custom_data_dir: bool) -> Option<&Path> {
    custom_data_dir.then_some(data_dir).flatten()
}

pub fn install_client(client_id: &str, binary: &Path, data_dir: Option<&Path>) -> Result<McpClientInfo, String> {
    if !binary.is_file() {
        return Err("The Cirrus MCP executable is not available. Build or reinstall Cirrus MCP first.".to_string());
    }
    let definition = definition(client_id).ok_or_else(|| format!("Unknown MCP client: {client_id}"))?;
    let info = client_info(definition, Some(binary), data_dir);
    if !info.installed {
        return Err(format!("{} was not detected on this computer", definition.name));
    }
    let path = client_config_path(definition.kind)
        .ok_or_else(|| "Could not resolve the client configuration path".to_string())?;
    if !supports_write(definition.kind) {
        return Err(format!("{} uses copy-and-paste configuration", definition.name));
    }
    match definition.kind {
        ClientKind::Codex => write_codex_config(&path, binary, data_dir, true)?,
        _ => write_json_config(&path, definition.kind, binary, data_dir, true)?,
    }
    log::info!("[MCP] configured {} at {}", definition.name, path.display());
    Ok(client_info(definition, Some(binary), data_dir))
}

pub fn remove_client(client_id: &str, binary: Option<&Path>, data_dir: Option<&Path>) -> Result<McpClientInfo, String> {
    let definition = definition(client_id).ok_or_else(|| format!("Unknown MCP client: {client_id}"))?;
    let path = client_config_path(definition.kind)
        .ok_or_else(|| "Could not resolve the client configuration path".to_string())?;
    if !supports_write(definition.kind) {
        return Err(format!("{} uses copy-and-paste configuration", definition.name));
    }
    match definition.kind {
        ClientKind::Codex => write_codex_config(&path, binary.unwrap_or(Path::new("")), data_dir, false)?,
        _ => write_json_config(&path, definition.kind, binary.unwrap_or(Path::new("")), data_dir, false)?,
    }
    log::info!("[MCP] removed {} configuration from {}", definition.name, path.display());
    Ok(client_info(definition, binary, data_dir))
}

fn definition(id: &str) -> Option<&'static ClientDefinition> {
    CLIENTS.iter().find(|definition| definition.id == id)
}

fn client_info(definition: &ClientDefinition, binary: Option<&Path>, data_dir: Option<&Path>) -> McpClientInfo {
    let config_path = client_config_path(definition.kind);
    let installed = config_path.as_ref().map(|path| client_installed(definition.kind, path)).unwrap_or(false);
    let binary_ready = binary.map(Path::is_file).unwrap_or(false);
    let supports_write = supports_write(definition.kind);
    let configured = config_path
        .as_ref()
        .zip(binary)
        .filter(|_| supports_write)
        .map(|(path, binary)| is_configured(definition.kind, path, binary, data_dir))
        .unwrap_or(false);
    let blocked_reason = if !binary_ready {
        Some("binary-missing".to_string())
    } else if supports_write && !installed {
        Some("not-installed".to_string())
    } else {
        None
    };
    McpClientInfo {
        id: definition.id,
        name: definition.name,
        installed,
        configured,
        can_install: installed && binary_ready && supports_write,
        supports_write,
        config_path: config_path.map(|path| path.display().to_string()),
        config_snippet: binary.map(|binary| config_snippet(definition.kind, binary, data_dir)),
        blocked_reason,
    }
}

fn client_installed(kind: ClientKind, config_path: &Path) -> bool {
    match kind {
        ClientKind::Codex => config_path.parent().map(Path::exists).unwrap_or(false),
        ClientKind::Claude => {
            config_path.exists() || home_dir().map(|home| home.join(".claude").exists()).unwrap_or(false)
        }
        _ => config_path.parent().map(Path::exists).unwrap_or(false),
    }
}

fn is_configured(kind: ClientKind, path: &Path, binary: &Path, data_dir: Option<&Path>) -> bool {
    let Ok(content) = std::fs::read_to_string(path) else {
        return false;
    };
    match kind {
        ClientKind::Codex => codex_has_command(&content, binary, data_dir),
        _ => json_has_command(&content, kind, binary, data_dir),
    }
}

fn client_config_path(kind: ClientKind) -> Option<PathBuf> {
    let home = home_dir()?;
    Some(match kind {
        ClientKind::Claude => home.join(".claude.json"),
        ClientKind::Cursor => home.join(".cursor/mcp.json"),
        ClientKind::CodeBuddy => home.join(".codebuddy/.mcp.json"),
        ClientKind::ZCode
        | ClientKind::Trae
        | ClientKind::DeepSeekHarness
        | ClientKind::CherryStudio
        | ClientKind::Qoder => return None,
        ClientKind::VsCode => {
            if cfg!(target_os = "windows") {
                std::env::var_os("APPDATA")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| home.join("AppData/Roaming"))
                    .join("Code/User/mcp.json")
            } else if cfg!(target_os = "macos") {
                home.join("Library/Application Support/Code/User/mcp.json")
            } else {
                config_home(&home).join("Code/User/mcp.json")
            }
        }
        ClientKind::Windsurf => home.join(".codeium/windsurf/mcp_config.json"),
        ClientKind::Codex => {
            let root = std::env::var_os("CODEX_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".codex"));
            root.join("config.toml")
        }
        ClientKind::OpenCode => home.join(".config/opencode/opencode.json"),
        ClientKind::Pi => home.join(".pi/agent/mcp.json"),
    })
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
}

fn config_home(home: &Path) -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config"))
}

fn json_root_key(kind: ClientKind) -> &'static str {
    match kind {
        ClientKind::VsCode => "servers",
        _ => "mcpServers",
    }
}

fn supports_write(kind: ClientKind) -> bool {
    matches!(
        kind,
        ClientKind::Claude
            | ClientKind::Cursor
            | ClientKind::CodeBuddy
            | ClientKind::VsCode
            | ClientKind::Windsurf
            | ClientKind::Codex
            | ClientKind::Pi
    )
}

fn json_server_entry(kind: ClientKind, binary: &Path, data_dir: Option<&Path>) -> Value {
    let mut entry = match kind {
        ClientKind::VsCode => json!({ "type": "stdio", "command": binary, "args": [] }),
        _ => json!({ "command": binary, "args": [] }),
    };
    if let Some(data_dir) = data_dir {
        if let Some(entry) = entry.as_object_mut() {
            entry.insert("env".to_string(), json!({ "CIRRUS_DATA_DIR": data_dir.to_string_lossy() }));
        }
    }
    entry
}

fn json_has_command(content: &str, kind: ClientKind, binary: &Path, data_dir: Option<&Path>) -> bool {
    let Ok(root) = serde_json::from_str::<Value>(content) else {
        return false;
    };
    let Some(server) = root.get(json_root_key(kind)).and_then(|servers| servers.get(SERVER_NAME)) else {
        return false;
    };
    let command_matches =
        server.get("command").and_then(Value::as_str).map(|value| value == binary.to_string_lossy()).unwrap_or(false);
    command_matches && json_env_matches(server, data_dir)
}

fn json_env_matches(server: &Value, data_dir: Option<&Path>) -> bool {
    let Some(data_dir) = data_dir else {
        return true;
    };
    server
        .get("env")
        .and_then(|env| env.get("CIRRUS_DATA_DIR"))
        .and_then(Value::as_str)
        .map(|value| paths_equal(Path::new(value), data_dir))
        .unwrap_or(false)
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    {
        left.to_string_lossy().eq_ignore_ascii_case(&right.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

fn write_json_config(
    path: &Path,
    kind: ClientKind,
    binary: &Path,
    data_dir: Option<&Path>,
    install: bool,
) -> Result<(), String> {
    if !install && !path.exists() {
        return Ok(());
    }
    let content = read_optional(path)?;
    let mut root = if content.trim().is_empty() {
        Value::Object(Map::new())
    } else {
        serde_json::from_str::<Value>(&content)
            .map_err(|error| format!("Invalid JSON in {}: {error}", path.display()))?
    };
    let root = root.as_object_mut().ok_or_else(|| format!("The root of {} must be a JSON object", path.display()))?;

    if install {
        let servers = root
            .entry(json_root_key(kind))
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .ok_or_else(|| format!("{} must be a JSON object", json_root_key(kind)))?;
        servers.insert(SERVER_NAME.to_string(), json_server_entry(kind, binary, data_dir));
    } else if let Some(servers) = root.get_mut(json_root_key(kind)).and_then(Value::as_object_mut) {
        servers.remove(SERVER_NAME);
        if servers.is_empty() {
            root.remove(json_root_key(kind));
        }
    }

    let serialized = serde_json::to_string_pretty(&Value::Object(root.clone())).map_err(|error| error.to_string())?;
    atomic_write(path, format!("{serialized}\n").as_bytes())
}

fn codex_has_command(content: &str, binary: &Path, data_dir: Option<&Path>) -> bool {
    let Ok(document) = content.parse::<DocumentMut>() else {
        return false;
    };
    let Some(server) = document
        .get("mcp_servers")
        .and_then(Item::as_table)
        .and_then(|servers| servers.get(SERVER_NAME))
        .and_then(Item::as_table)
    else {
        return false;
    };
    server.get("command").and_then(Item::as_str).map(|value| value == binary.to_string_lossy()).unwrap_or(false)
        && CODEX_HTTP_ONLY_FIELDS.iter().all(|key| server.get(key).is_none())
        && codex_env_matches(server, data_dir)
}

fn codex_env_matches(server: &Table, data_dir: Option<&Path>) -> bool {
    let Some(data_dir) = data_dir else {
        return true;
    };
    codex_env_value(server, "CIRRUS_DATA_DIR").map(|value| paths_equal(Path::new(value), data_dir)).unwrap_or(false)
}

fn codex_env_value<'a>(server: &'a Table, key: &str) -> Option<&'a str> {
    match server.get("env")? {
        Item::Table(env) => env.get(key).and_then(Item::as_str),
        Item::Value(toml_edit::Value::InlineTable(env)) => env.get(key).and_then(toml_edit::Value::as_str),
        _ => None,
    }
}

fn set_codex_data_dir(server: &mut Table, data_dir: Option<&Path>) -> Result<(), String> {
    if let Some(data_dir) = data_dir {
        match server.entry("env").or_insert(Item::Table(Table::new())) {
            Item::Table(env) => env["CIRRUS_DATA_DIR"] = value(data_dir.to_string_lossy().to_string()),
            Item::Value(toml_edit::Value::InlineTable(env)) => {
                env.insert("CIRRUS_DATA_DIR", toml_edit::Value::from(data_dir.to_string_lossy().to_string()));
            }
            _ => {}
        }
        return Ok(());
    }

    let mut remove_env = false;
    if let Some(env) = server.get_mut("env") {
        match env {
            Item::Table(env) => {
                env.remove("CIRRUS_DATA_DIR");
                remove_env = env.is_empty();
            }
            Item::Value(toml_edit::Value::InlineTable(env)) => {
                env.remove("CIRRUS_DATA_DIR");
                remove_env = env.is_empty();
            }
            _ => return Err("mcp_servers.cirrus.env must be a TOML table".to_string()),
        }
    }
    if remove_env {
        server.remove("env");
    }
    Ok(())
}

fn write_codex_config(path: &Path, binary: &Path, data_dir: Option<&Path>, install: bool) -> Result<(), String> {
    if !install && !path.exists() {
        return Ok(());
    }
    let content = read_optional(path)?;
    let mut document = if content.trim().is_empty() {
        DocumentMut::new()
    } else {
        content.parse::<DocumentMut>().map_err(|error| format!("Invalid TOML in {}: {error}", path.display()))?
    };
    if install {
        let servers = document
            .as_table_mut()
            .entry("mcp_servers")
            .or_insert(Item::Table(Table::new()))
            .as_table_mut()
            .ok_or_else(|| "mcp_servers must be a TOML table".to_string())?;
        let server = servers
            .entry(SERVER_NAME)
            .or_insert(Item::Table(Table::new()))
            .as_table_mut()
            .ok_or_else(|| "mcp_servers.cirrus must be a TOML table".to_string())?;
        // Codex rejects a server that mixes HTTP and stdio fields. Remove the
        // previous transport so switching from the HTTP snippet to stdio works.
        for key in CODEX_HTTP_ONLY_FIELDS {
            server.remove(key);
        }
        server["command"] = value(binary.to_string_lossy().to_string());
        set_codex_data_dir(server, data_dir)?;
    } else if let Some(servers) = document.get_mut("mcp_servers").and_then(Item::as_table_mut) {
        servers.remove(SERVER_NAME);
        if servers.is_empty() {
            document.as_table_mut().remove("mcp_servers");
        }
    }
    atomic_write(path, document.to_string().as_bytes())
}

fn read_optional(path: &Path) -> Result<String, String> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(format!("Could not read {}: {error}", path.display())),
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
    }
    if path.exists() {
        let backup = path.with_file_name(format!(
            "{}.cirrus.bak",
            path.file_name().and_then(|name| name.to_str()).unwrap_or("config")
        ));
        let _ = std::fs::copy(path, backup);
    }
    if path.exists() {
        return std::fs::write(path, bytes).map_err(|error| format!("Could not save {}: {error}", path.display()));
    }
    let temporary = path
        .with_file_name(format!(".{}.cirrus.tmp", path.file_name().and_then(|name| name.to_str()).unwrap_or("config")));
    std::fs::write(&temporary, bytes).map_err(|error| format!("Could not write {}: {error}", temporary.display()))?;
    std::fs::rename(&temporary, path).map_err(|error| format!("Could not save {}: {error}", path.display()))
}

pub(crate) fn codex_config_snippet(binary: &Path, data_dir: Option<&Path>) -> String {
    let command = binary.to_string_lossy().to_string();
    let command_json = serde_json::to_string(&command).unwrap_or_else(|_| "\"cirrus-mcp\"".to_string());
    let data_dir_json = data_dir.map(|path| serde_json::to_string(&path.to_string_lossy()).unwrap_or_default());
    let env = data_dir_json
        .as_ref()
        .map(|data_dir| format!("\nenv = {{ CIRRUS_DATA_DIR = {data_dir} }}"))
        .unwrap_or_default();
    format!("[mcp_servers.cirrus]\ncommand = {command_json}{env}\n")
}

fn config_snippet(kind: ClientKind, binary: &Path, data_dir: Option<&Path>) -> String {
    let command = binary.to_string_lossy().to_string();
    let command_json = serde_json::to_string(&command).unwrap_or_else(|_| "\"cirrus-mcp\"".to_string());
    let data_dir_json = data_dir.map(|path| serde_json::to_string(&path.to_string_lossy()).unwrap_or_default());
    match kind {
        ClientKind::Codex => codex_config_snippet(binary, data_dir),
        ClientKind::VsCode => serde_json::to_string_pretty(&json!({
            "servers": { SERVER_NAME: json_server_entry(kind, binary, data_dir) }
        }))
        .unwrap_or_default(),
        ClientKind::DeepSeekHarness => {
            let env = data_dir_json
                .as_ref()
                .map(|data_dir| format!("        env:\n          CIRRUS_DATA_DIR: {data_dir}\n"))
                .unwrap_or_default();
            format!(
                "- insert:\n    - id: mcp-cirrus\n      name: '@deepseek-ai/dsh-mcp-client'\n      config:\n        serverName: cirrus\n        transport: stdio\n        command: {command_json}\n{env}"
            )
        }
        ClientKind::OpenCode => {
            let mut entry = json!({ "type": "local", "command": [command] });
            if let (Some(entry), Some(data_dir)) = (entry.as_object_mut(), data_dir) {
                entry.insert("environment".to_string(), json!({ "CIRRUS_DATA_DIR": data_dir.to_string_lossy() }));
            }
            serde_json::to_string_pretty(&json!({ "mcp": { SERVER_NAME: entry } })).unwrap_or_default()
        }
        ClientKind::CherryStudio => {
            let mut env = Map::new();
            if let Some(data_dir) = data_dir {
                env.insert("CIRRUS_DATA_DIR".to_string(), json!(data_dir.to_string_lossy()));
            }
            serde_json::to_string_pretty(&json!({
                "mcpServers": { SERVER_NAME: {
                "name": SERVER_NAME,
                "description": "Cirrus iCloud",
                "baseUrl": "",
                "command": command,
                "args": [],
                "env": env,
                "isActive": true,
                "type": "stdio"
            } }
            }))
            .unwrap_or_default()
        }
        _ => serde_json::to_string_pretty(&json!({
            "mcpServers": { SERVER_NAME: json_server_entry(kind, binary, data_dir) }
        }))
        .unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_env_is_only_emitted_for_custom_data_directories() {
        let default_dir = Path::new(r"C:\Users\me\AppData\Roaming\com.cirrus");
        assert_eq!(env_data_dir_for_client(Some(default_dir), false), None);
        assert_eq!(env_data_dir_for_client(Some(default_dir), true), Some(default_dir));
        assert_eq!(env_data_dir_for_client(None, true), None);
    }

    #[test]
    fn json_config_preserves_other_servers_and_removes_only_cirrus() {
        let dir = std::env::temp_dir().join(format!("cirrus-mcp-client-{}", uuid::Uuid::new_v4()));
        let path = dir.join("mcp.json");
        std::fs::create_dir_all(&dir).expect("create dir");
        std::fs::write(&path, r#"{"mcpServers":{"other":{"command":"other.exe"}}}"#).expect("seed");
        let binary = Path::new(r"C:\Program Files\Cirrus\cirrus-mcp.exe");
        let data_dir = Path::new(r"D:\CirrusPortableData");

        write_json_config(&path, ClientKind::Claude, binary, Some(data_dir), true).expect("install");
        let value: Value = serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        assert!(value["mcpServers"]["other"].is_object());
        assert_eq!(value["mcpServers"]["cirrus"]["command"], binary.to_string_lossy().as_ref());
        assert_eq!(value["mcpServers"]["cirrus"]["env"]["CIRRUS_DATA_DIR"], data_dir.to_string_lossy().as_ref());

        write_json_config(&path, ClientKind::Claude, binary, Some(data_dir), false).expect("remove");
        let value: Value = serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        assert!(value["mcpServers"]["other"].is_object());
        assert!(value["mcpServers"]["cirrus"].is_null());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn json_default_install_replaces_existing_env() {
        let dir = std::env::temp_dir().join(format!("cirrus-mcp-json-env-{}", uuid::Uuid::new_v4()));
        let path = dir.join("mcp.json");
        std::fs::create_dir_all(&dir).expect("create dir");
        std::fs::write(
            &path,
            r#"{"mcpServers":{"cirrus":{"command":"old","env":{"CIRRUS_DATA_DIR":"old","KEEP":"1","HTTPS_PROXY":"http://proxy"}}}}"#,
        )
        .expect("seed");
        let binary = Path::new(r"C:\Program Files\Cirrus\cirrus-mcp.exe");

        write_json_config(&path, ClientKind::Claude, binary, None, true).expect("install default");
        let value: Value = serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        assert!(value["mcpServers"]["cirrus"].get("env").is_none());
        assert!(json_has_command(&value.to_string(), ClientKind::Claude, binary, None));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn codex_config_preserves_existing_tables() {
        let dir = std::env::temp_dir().join(format!("cirrus-mcp-codex-{}", uuid::Uuid::new_v4()));
        let path = dir.join("config.toml");
        std::fs::create_dir_all(&dir).expect("create dir");
        std::fs::write(&path, "model = \"test\"\n\n[mcp_servers.other]\ncommand = \"other\"\n").expect("seed");
        let binary = Path::new(r"C:\Program Files\Cirrus\cirrus-mcp.exe");
        let data_dir = Path::new(r"D:\CirrusPortableData");

        write_codex_config(&path, binary, Some(data_dir), true).expect("install");
        let content = std::fs::read_to_string(&path).expect("read");
        assert!(content.contains("model = \"test\""));
        assert!(content.contains("[mcp_servers.other]"));
        assert!(codex_has_command(&content, binary, Some(data_dir)));
        assert!(!codex_has_command(&content, binary, Some(Path::new(r"C:\Other"))));

        write_codex_config(&path, binary, Some(data_dir), false).expect("remove");
        let content = std::fs::read_to_string(&path).expect("read");
        assert!(content.contains("[mcp_servers.other]"));
        assert!(!content.contains("[mcp_servers.cirrus]"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn codex_stdio_install_removes_http_transport_fields() {
        let dir = std::env::temp_dir().join(format!("cirrus-mcp-codex-http-{}", uuid::Uuid::new_v4()));
        let path = dir.join("config.toml");
        std::fs::create_dir_all(&dir).expect("create dir");
        std::fs::write(
            &path,
            r#"model = "test"

[mcp_servers.other]
command = "other"

[mcp_servers.cirrus]
url = "http://127.0.0.1:5226/mcp"
bearer_token_env_var = "CIRRUS_MCP_TOKEN"

[mcp_servers.cirrus.http_headers]
Authorization = "Bearer test"
"#,
        )
        .expect("seed");
        let binary = Path::new(r"C:\Program Files\Cirrus\cirrus-mcp.exe");
        let data_dir = Path::new(r"D:\CirrusPortableData");

        assert!(!codex_has_command(&std::fs::read_to_string(&path).expect("read"), binary, Some(data_dir)));
        write_codex_config(&path, binary, Some(data_dir), true).expect("install");
        let content = std::fs::read_to_string(&path).expect("read");
        let document = content.parse::<DocumentMut>().expect("toml");
        let server = document
            .get("mcp_servers")
            .and_then(Item::as_table)
            .and_then(|servers| servers.get(SERVER_NAME))
            .and_then(Item::as_table)
            .expect("cirrus server");

        assert_eq!(server.get("command").and_then(Item::as_str), Some(binary.to_string_lossy().as_ref()));
        for key in CODEX_HTTP_ONLY_FIELDS {
            assert!(server.get(key).is_none(), "{key} should be removed");
        }
        assert!(content.contains("[mcp_servers.other]"));
        assert!(codex_has_command(&content, binary, Some(data_dir)));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn default_data_directory_omits_env_in_snippets() {
        let binary = Path::new(r"C:\Program Files\Cirrus\cirrus-mcp.exe");
        let json = config_snippet(ClientKind::Claude, binary, None);
        let value: Value = serde_json::from_str(&json).expect("JSON snippet");
        assert!(value["mcpServers"]["cirrus"].get("env").is_none());
        assert!(json_has_command(&json, ClientKind::Claude, binary, None));

        let codex = config_snippet(ClientKind::Codex, binary, None);
        assert!(!codex.contains("CIRRUS_DATA_DIR"));
        assert!(codex_has_command(&codex, binary, None));
    }

    #[test]
    fn default_data_directory_ignores_existing_env_when_detecting_config() {
        let binary = Path::new(r"C:\Program Files\Cirrus\cirrus-mcp.exe");
        let json = json!({
            "mcpServers": {
                "cirrus": {
                    "command": binary,
                    "args": [],
                    "env": { "CIRRUS_DATA_DIR": r"C:\Users\me\AppData\Roaming\com.cirrus" }
                }
            }
        })
        .to_string();
        assert!(json_has_command(&json, ClientKind::Claude, binary, None));

        let codex = format!(
            "[mcp_servers.cirrus]\ncommand = {:?}\nenv = {{ CIRRUS_DATA_DIR = \"C:\\\\Users\\\\me\\\\AppData\\\\Roaming\\\\com.cirrus\" }}\n",
            binary.to_string_lossy()
        );
        assert!(codex_has_command(&codex, binary, None));
    }

    #[test]
    fn codex_snippet_includes_the_shared_data_directory() {
        let binary = Path::new(r"C:\Program Files\Cirrus\cirrus-mcp.exe");
        let data_dir = Path::new(r"D:\CirrusPortableData");
        let snippet = config_snippet(ClientKind::Codex, binary, Some(data_dir));

        assert!(snippet.parse::<DocumentMut>().is_ok(), "{snippet}");
        assert!(codex_has_command(&snippet, binary, Some(data_dir)));
    }

    #[test]
    fn codex_install_updates_inline_env_without_dropping_other_values() {
        let dir = std::env::temp_dir().join(format!("cirrus-mcp-codex-env-{}", uuid::Uuid::new_v4()));
        let path = dir.join("config.toml");
        std::fs::create_dir_all(&dir).expect("create dir");
        std::fs::write(
            &path,
            "[mcp_servers.cirrus]\ncommand = \"old\"\nenv = { CIRRUS_DATA_DIR = \"old\", KEEP = \"1\" }\n",
        )
        .expect("seed");
        let binary = Path::new(r"C:\Program Files\Cirrus\cirrus-mcp.exe");
        let data_dir = Path::new(r"D:\CirrusPortableData");

        write_codex_config(&path, binary, Some(data_dir), true).expect("install");
        let content = std::fs::read_to_string(&path).expect("read");
        let document = content.parse::<DocumentMut>().expect("toml");
        let server = document
            .get("mcp_servers")
            .and_then(Item::as_table)
            .and_then(|servers| servers.get(SERVER_NAME))
            .and_then(Item::as_table)
            .expect("cirrus server");

        assert_eq!(codex_env_value(server, "CIRRUS_DATA_DIR"), Some(data_dir.to_string_lossy().as_ref()));
        assert_eq!(codex_env_value(server, "KEEP"), Some("1"));
        assert!(codex_has_command(&content, binary, Some(data_dir)));

        write_codex_config(&path, binary, None, true).expect("install default");
        let content = std::fs::read_to_string(&path).expect("read");
        let document = content.parse::<DocumentMut>().expect("toml");
        let server = document
            .get("mcp_servers")
            .and_then(Item::as_table)
            .and_then(|servers| servers.get(SERVER_NAME))
            .and_then(Item::as_table)
            .expect("cirrus server");
        assert!(codex_env_value(server, "CIRRUS_DATA_DIR").is_none());
        assert_eq!(codex_env_value(server, "KEEP"), Some("1"));
        assert!(codex_has_command(&content, binary, None));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn invalid_json_is_never_overwritten() {
        let dir = std::env::temp_dir().join(format!("cirrus-mcp-invalid-{}", uuid::Uuid::new_v4()));
        let path = dir.join("mcp.json");
        std::fs::create_dir_all(&dir).expect("create dir");
        std::fs::write(&path, "not json").expect("seed");
        let result = write_json_config(&path, ClientKind::Cursor, Path::new("cirrus-mcp"), None, true);
        assert!(result.is_err());
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "not json");
        let _ = std::fs::remove_dir_all(dir);
    }
}
