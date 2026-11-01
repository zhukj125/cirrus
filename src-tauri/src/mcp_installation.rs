use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::Serialize;
use sysinfo::{ProcessesToUpdate, System};

const VERSION_FILE: &str = ".cirrus-mcp-version";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerStatus {
    pub installed: bool,
    pub installed_version: Option<String>,
    pub bundled_version: String,
    pub update_available: bool,
    pub active_source: String,
    pub install_dir: String,
    pub launcher_path: String,
    pub native_path: String,
}

fn home_dir() -> Result<PathBuf, String> {
    #[cfg(windows)]
    let value = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"));
    #[cfg(not(windows))]
    let value = std::env::var_os("HOME");
    value
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| "Could not resolve the user home directory".to_string())
}

pub fn install_dir() -> Result<PathBuf, String> {
    Ok(home_dir()?.join(".cirrus").join("bin"))
}

pub fn installed_launcher_path() -> Result<PathBuf, String> {
    Ok(install_dir()?.join(if cfg!(windows) { "cirrus-mcp-server.exe" } else { "cirrus-mcp-server" }))
}

pub fn installed_native_path() -> Result<PathBuf, String> {
    Ok(install_dir()?.join(if cfg!(windows) { "cirrus-mcp.exe" } else { "cirrus-mcp" }))
}

fn installed_version(install_dir: &Path) -> Option<String> {
    fs::read_to_string(install_dir.join(VERSION_FILE))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn status() -> Result<McpServerStatus, String> {
    let install_dir = install_dir()?;
    let launcher = installed_launcher_path()?;
    let native = installed_native_path()?;
    let installed = launcher.is_file() && native.is_file();
    let installed_version = installed.then(|| installed_version(&install_dir)).flatten();
    let bundled_launcher = crate::mcp::bundled_mcp_binary_path().is_some_and(|path| path.is_file());
    let bundled_native = crate::mcp::bundled_mcp_native_binary_path().is_some_and(|path| path.is_file());
    let active_source = if installed {
        "installed"
    } else if bundled_launcher && bundled_native {
        "bundled"
    } else {
        "missing"
    };
    let bundled_version = env!("CARGO_PKG_VERSION").to_string();
    Ok(McpServerStatus {
        installed,
        update_available: installed && installed_version.as_deref() != Some(bundled_version.as_str()),
        installed_version,
        bundled_version,
        active_source: active_source.to_string(),
        install_dir: install_dir.display().to_string(),
        launcher_path: launcher.display().to_string(),
        native_path: native.display().to_string(),
    })
}

fn same_path(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    let equal = left.to_string_lossy().eq_ignore_ascii_case(&right.to_string_lossy());
    #[cfg(not(windows))]
    let equal = left == right;
    equal
}

fn stop_installed_processes() -> Result<(), String> {
    let launcher = installed_launcher_path()?;
    let native = installed_native_path()?;
    let expected = [launcher, native]
        .into_iter()
        .filter(|path| path.is_file())
        .map(|path| fs::canonicalize(&path).unwrap_or(path))
        .collect::<Vec<_>>();
    if expected.is_empty() {
        return Ok(());
    }

    let mut system = System::new_all();
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        system.refresh_processes(ProcessesToUpdate::All, true);
        let mut remaining = Vec::new();
        for (pid, process) in system.processes() {
            let Some(actual) = process.exe() else { continue };
            let actual = fs::canonicalize(actual).unwrap_or_else(|_| actual.to_path_buf());
            if !expected.iter().any(|path| same_path(&actual, path)) {
                continue;
            }
            if process.kill() {
                log::info!("[MCP] stopped installed MCP process pid={pid}");
            } else {
                remaining.push(pid.as_u32());
            }
        }
        if remaining.is_empty() {
            std::thread::sleep(Duration::from_millis(250));
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!("Could not stop installed MCP processes: {remaining:?}"));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn copy_atomic(source: &Path, destination: &Path) -> Result<(), String> {
    let parent = destination.parent().ok_or_else(|| "MCP destination has no parent directory".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
    let temporary = parent.join(format!(
        ".{}.tmp-{}",
        destination.file_name().and_then(|name| name.to_str()).unwrap_or("cirrus-mcp"),
        uuid::Uuid::new_v4().simple()
    ));
    fs::copy(source, &temporary)
        .map_err(|error| format!("Could not copy {} to {}: {error}", source.display(), temporary.display()))?;
    if destination.exists() {
        fs::remove_file(destination)
            .map_err(|error| format!("Could not replace {}: {error}", destination.display()))?;
    }
    fs::rename(&temporary, destination)
        .map_err(|error| format!("Could not install {}: {error}", destination.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(destination, fs::Permissions::from_mode(0o755))
            .map_err(|error| format!("Could not mark {} executable: {error}", destination.display()))?;
    }
    Ok(())
}

pub fn install_or_update() -> Result<McpServerStatus, String> {
    let source_launcher = crate::mcp::bundled_mcp_binary_path()
        .filter(|path| path.is_file())
        .ok_or_else(|| "The bundled cirrus-mcp-server launcher is missing. Reinstall Cirrus.".to_string())?;
    let source_native = crate::mcp::bundled_mcp_native_binary_path()
        .filter(|path| path.is_file())
        .ok_or_else(|| "The bundled cirrus-mcp binary is missing. Reinstall Cirrus.".to_string())?;
    let install_dir = install_dir()?;
    let destination_launcher = installed_launcher_path()?;
    let destination_native = installed_native_path()?;

    stop_installed_processes()?;
    copy_atomic(&source_launcher, &destination_launcher)?;
    copy_atomic(&source_native, &destination_native)?;
    fs::write(install_dir.join(VERSION_FILE), env!("CARGO_PKG_VERSION"))
        .map_err(|error| format!("Could not write the MCP version marker: {error}"))?;
    log::info!("[MCP] installed standalone MCP engine at {}", install_dir.display());
    status()
}

pub fn uninstall() -> Result<McpServerStatus, String> {
    stop_installed_processes()?;
    let launcher = installed_launcher_path()?;
    let native = installed_native_path()?;
    for path in [&launcher, &native, &install_dir()?.join(VERSION_FILE)] {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("Could not remove {}: {error}", path.display())),
        }
    }
    log::info!("[MCP] removed standalone MCP engine");
    status()
}
