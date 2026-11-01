use std::{
    env,
    path::PathBuf,
    process::{Command, ExitCode, Stdio},
};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn native_binary_path() -> Result<PathBuf, String> {
    let current = env::current_exe().map_err(|error| format!("could not locate cirrus-mcp-server: {error}"))?;
    let directory = current.parent().ok_or_else(|| "cirrus-mcp-server has no installation directory".to_string())?;
    let name = if cfg!(windows) { "cirrus-mcp.exe" } else { "cirrus-mcp" };
    let binary = directory.join(name);
    if binary.is_file() {
        Ok(binary)
    } else {
        Err(format!("native MCP executable was not found at {}", binary.display()))
    }
}

fn run() -> Result<ExitCode, String> {
    let binary = native_binary_path()?;
    let mut command = Command::new(&binary);
    command.args(env::args_os().skip(1)).stdin(Stdio::inherit()).stdout(Stdio::inherit()).stderr(Stdio::inherit());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let error = command.exec();
        return Err(format!("could not start {}: {error}", binary.display()));
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
        let status = command.status().map_err(|error| format!("could not start {}: {error}", binary.display()))?;
        return Ok(ExitCode::from(status.code().unwrap_or(1) as u8));
    }

    #[allow(unreachable_code)]
    Err("unsupported platform".to_string())
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(message) => {
            eprintln!("cirrus-mcp-server: {message}");
            ExitCode::FAILURE
        }
    }
}
