use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Map, Value};
use toml_edit::{value, DocumentMut, Item, Table};

const SERVER_NAME: &str = "cirrus";

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

pub fn list_clients(binary: Option<&Path>) -> Vec<McpClientInfo> {
    CLIENTS.iter().map(|definition| client_info(definition, binary)).collect()
}

pub fn install_client(client_id: &str, binary: &Path) -> Result<McpClientInfo, String> {
    if !binary.is_file() {
        return Err("The Cirrus MCP executable is not available. Build or reinstall Cirrus MCP first.".to_string());
    }
    let definition = definition(client_id).ok_or_else(|| format!("Unknown MCP client: {client_id}"))?;
    let info = client_info(definition, Some(binary));
    if !info.installed {
        return Err(format!("{} was not detected on this computer", definition.name));
    }
    let path = client_config_path(definition.kind)
        .ok_or_else(|| "Could not resolve the client configuration path".to_string())?;
    if !supports_write(definition.kind) {
        return Err(format!("{} uses copy-and-paste configuration", definition.name));
    }
    match definition.kind {
        ClientKind::Codex => write_codex_config(&path, binary, true)?,
        _ => write_json_config(&path, definition.kind, binary, true)?,
    }
    log::info!("[MCP] configured {} at {}", definition.name, path.display());
    Ok(client_info(definition, Some(binary)))
}

pub fn remove_client(client_id: &str, binary: Option<&Path>) -> Result<McpClientInfo, String> {
    let definition = definition(client_id).ok_or_else(|| format!("Unknown MCP client: {client_id}"))?;
    let path = client_config_path(definition.kind)
        .ok_or_else(|| "Could not resolve the client configuration path".to_string())?;
    if !supports_write(definition.kind) {
        return Err(format!("{} uses copy-and-paste configuration", definition.name));
    }
    match definition.kind {
        ClientKind::Codex => write_codex_config(&path, binary.unwrap_or(Path::new("")), false)?,
        _ => write_json_config(&path, definition.kind, binary.unwrap_or(Path::new("")), false)?,
    }
    log::info!("[MCP] removed {} configuration from {}", definition.name, path.display());
    Ok(client_info(definition, binary))
}

fn definition(id: &str) -> Option<&'static ClientDefinition> {
    CLIENTS.iter().find(|definition| definition.id == id)
}

fn client_info(definition: &ClientDefinition, binary: Option<&Path>) -> McpClientInfo {
    let config_path = client_config_path(definition.kind);
    let installed = config_path.as_ref().map(|path| client_installed(definition.kind, path)).unwrap_or(false);
    let binary_ready = binary.map(Path::is_file).unwrap_or(false);
    let supports_write = supports_write(definition.kind);
    let configured = config_path
        .as_ref()
        .zip(binary)
        .filter(|_| supports_write)
        .map(|(path, binary)| is_configured(definition.kind, path, binary))
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
        config_snippet: binary.map(|binary| config_snippet(definition.kind, binary)),
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

fn is_configured(kind: ClientKind, path: &Path, binary: &Path) -> bool {
    let Ok(content) = std::fs::read_to_string(path) else {
        return false;
    };
    match kind {
        ClientKind::Codex => codex_has_command(&content, binary),
        _ => json_has_command(&content, kind, binary),
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

fn json_server_entry(kind: ClientKind, binary: &Path) -> Value {
    match kind {
        ClientKind::VsCode => json!({ "type": "stdio", "command": binary, "args": [] }),
        _ => json!({ "command": binary, "args": [] }),
    }
}

fn json_has_command(content: &str, kind: ClientKind, binary: &Path) -> bool {
    let Ok(root) = serde_json::from_str::<Value>(content) else {
        return false;
    };
    root.get(json_root_key(kind))
        .and_then(|servers| servers.get(SERVER_NAME))
        .and_then(|server| server.get("command"))
        .and_then(Value::as_str)
        .map(|value| value == binary.to_string_lossy())
        .unwrap_or(false)
}

fn write_json_config(path: &Path, kind: ClientKind, binary: &Path, install: bool) -> Result<(), String> {
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
        servers.insert(SERVER_NAME.to_string(), json_server_entry(kind, binary));
    } else if let Some(servers) = root.get_mut(json_root_key(kind)).and_then(Value::as_object_mut) {
        servers.remove(SERVER_NAME);
        if servers.is_empty() {
            root.remove(json_root_key(kind));
        }
    }

    let serialized = serde_json::to_string_pretty(&Value::Object(root.clone())).map_err(|error| error.to_string())?;
    atomic_write(path, format!("{serialized}\n").as_bytes())
}

fn codex_has_command(content: &str, binary: &Path) -> bool {
    let Ok(document) = content.parse::<DocumentMut>() else {
        return false;
    };
    document
        .get("mcp_servers")
        .and_then(Item::as_table)
        .and_then(|servers| servers.get(SERVER_NAME))
        .and_then(Item::as_table)
        .and_then(|server| server.get("command"))
        .and_then(Item::as_str)
        .map(|value| value == binary.to_string_lossy())
        .unwrap_or(false)
}

fn write_codex_config(path: &Path, binary: &Path, install: bool) -> Result<(), String> {
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
        server["command"] = value(binary.to_string_lossy().to_string());
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

fn config_snippet(kind: ClientKind, binary: &Path) -> String {
    let command = binary.to_string_lossy().to_string();
    match kind {
        ClientKind::Codex => format!(
            "[mcp_servers.cirrus]\ncommand = {}\n",
            serde_json::to_string(&command).unwrap_or_else(|_| "\"cirrus-mcp\"".to_string())
        ),
        ClientKind::VsCode => serde_json::to_string_pretty(&json!({
            "servers": { SERVER_NAME: json_server_entry(kind, binary) }
        }))
        .unwrap_or_default(),
        ClientKind::DeepSeekHarness => format!(
            "- insert:\n    - id: mcp-cirrus\n      name: '@deepseek-ai/dsh-mcp-client'\n      config:\n        serverName: cirrus\n        transport: stdio\n        command: {}\n",
            serde_json::to_string(&command).unwrap_or_else(|_| "\"cirrus-mcp\"".to_string())
        ),
        ClientKind::OpenCode => serde_json::to_string_pretty(&json!({
            "mcp": { SERVER_NAME: { "type": "local", "command": [command] } }
        }))
        .unwrap_or_default(),
        ClientKind::CherryStudio => serde_json::to_string_pretty(&json!({
            "mcpServers": { SERVER_NAME: {
                "name": SERVER_NAME,
                "description": "Cirrus iCloud",
                "baseUrl": "",
                "command": command,
                "args": [],
                "env": {},
                "isActive": true,
                "type": "stdio"
            } }
        }))
        .unwrap_or_default(),
        _ => serde_json::to_string_pretty(&json!({
            "mcpServers": { SERVER_NAME: json_server_entry(kind, binary) }
        }))
        .unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_config_preserves_other_servers_and_removes_only_cirrus() {
        let dir = std::env::temp_dir().join(format!("cirrus-mcp-client-{}", uuid::Uuid::new_v4()));
        let path = dir.join("mcp.json");
        std::fs::create_dir_all(&dir).expect("create dir");
        std::fs::write(&path, r#"{"mcpServers":{"other":{"command":"other.exe"}}}"#).expect("seed");
        let binary = Path::new(r"C:\Program Files\Cirrus\cirrus-mcp.exe");

        write_json_config(&path, ClientKind::Claude, binary, true).expect("install");
        let value: Value = serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        assert!(value["mcpServers"]["other"].is_object());
        assert_eq!(value["mcpServers"]["cirrus"]["command"], binary.to_string_lossy().as_ref());

        write_json_config(&path, ClientKind::Claude, binary, false).expect("remove");
        let value: Value = serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        assert!(value["mcpServers"]["other"].is_object());
        assert!(value["mcpServers"]["cirrus"].is_null());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn codex_config_preserves_existing_tables() {
        let dir = std::env::temp_dir().join(format!("cirrus-mcp-codex-{}", uuid::Uuid::new_v4()));
        let path = dir.join("config.toml");
        std::fs::create_dir_all(&dir).expect("create dir");
        std::fs::write(&path, "model = \"test\"\n\n[mcp_servers.other]\ncommand = \"other\"\n").expect("seed");
        let binary = Path::new(r"C:\Program Files\Cirrus\cirrus-mcp.exe");

        write_codex_config(&path, binary, true).expect("install");
        let content = std::fs::read_to_string(&path).expect("read");
        assert!(content.contains("model = \"test\""));
        assert!(content.contains("[mcp_servers.other]"));
        assert!(codex_has_command(&content, binary));

        write_codex_config(&path, binary, false).expect("remove");
        let content = std::fs::read_to_string(&path).expect("read");
        assert!(content.contains("[mcp_servers.other]"));
        assert!(!content.contains("[mcp_servers.cirrus]"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn invalid_json_is_never_overwritten() {
        let dir = std::env::temp_dir().join(format!("cirrus-mcp-invalid-{}", uuid::Uuid::new_v4()));
        let path = dir.join("mcp.json");
        std::fs::create_dir_all(&dir).expect("create dir");
        std::fs::write(&path, "not json").expect("seed");
        let result = write_json_config(&path, ClientKind::Cursor, Path::new("cirrus-mcp"), true);
        assert!(result.is_err());
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "not json");
        let _ = std::fs::remove_dir_all(dir);
    }
}
