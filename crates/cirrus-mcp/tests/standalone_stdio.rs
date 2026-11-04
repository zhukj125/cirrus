use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use cirrus_core::{AccountStoreRepository, AccountStoreSnapshot, McpHistoryRepository, StoredAccount};
use serde_json::{json, Value};

fn temp_dir(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("cirrus-mcp-standalone-{label}-{}", uuid::Uuid::new_v4()))
}

fn run_child(mut child: std::process::Child) -> Output {
    let requests = [
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "integration-test", "version": "1" }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": { "name": "cirrus_list_accounts", "arguments": {} }
        }),
    ];
    {
        let stdin = child.stdin.as_mut().expect("stdin");
        for request in requests {
            writeln!(stdin, "{request}").expect("write request");
        }
    }
    child.wait_with_output().expect("wait for cirrus-mcp")
}

fn run_stdio(data_dir: &std::path::Path) -> Output {
    let child = Command::new(env!("CARGO_BIN_EXE_cirrus-mcp"))
        .env("CIRRUS_DATA_DIR", data_dir)
        .env("CIRRUS_SESSION_KEY", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn cirrus-mcp");
    run_child(child)
}

fn run_stdio_default_backend(data_dir: &std::path::Path) -> Output {
    let child = Command::new(env!("CARGO_BIN_EXE_cirrus-mcp"))
        .env("CIRRUS_DATA_DIR", data_dir)
        .env("CIRRUS_SESSION_KEY", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn cirrus-mcp");
    run_child(child)
}

fn responses(output: &Output) -> Vec<Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("JSON-RPC response"))
        .collect()
}

fn response_for(responses: &[Value], id: i64) -> &Value {
    responses.iter().find(|response| response.get("id").and_then(Value::as_i64) == Some(id)).expect("response id")
}

#[test]
fn authorize_keychain_command_reads_an_existing_key_without_creating_one() {
    let output = Command::new(env!("CARGO_BIN_EXE_cirrus-mcp"))
        .arg("--authorize-keychain")
        .env("CIRRUS_SESSION_KEY", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")
        .output()
        .expect("run authorization command");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("session vault access authorized"));
}

#[test]
fn stdio_lists_accounts_from_the_local_vault() {
    let dir = temp_dir("accounts");
    AccountStoreRepository::new(Some(&dir))
        .save(&AccountStoreSnapshot {
            accounts: vec![StoredAccount {
                key: "dsid:123".to_string(),
                apple_id: "person@example.com".to_string(),
                dsid: Some("123".to_string()),
                first_name: Some("Test".to_string()),
                last_name: Some("User".to_string()),
                photos_session_id: Some("session-1".to_string()),
                devices_session_id: Some("session-1".to_string()),
                last_used_at: 10,
                last_known_status: "valid".to_string(),
            }],
            active_account_key: Some("dsid:123".to_string()),
            avatars: Default::default(),
            revision: 0,
        })
        .expect("seed account");

    let output = run_stdio(&dir);
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("local backend ready"), "stderr: {stderr}");
    let responses = responses(&output);
    assert_eq!(response_for(&responses, 1)["result"]["serverInfo"]["name"], "cirrus-mcp");
    let result = &response_for(&responses, 2)["result"];
    assert_eq!(result["isError"], false);
    assert_eq!(result["structuredContent"]["count"], 1);
    assert_eq!(result["structuredContent"]["accounts"][0]["accountKey"], "dsid:123");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn stdio_uses_the_local_backend_only() {
    let dir = temp_dir("default-local");
    AccountStoreRepository::new(Some(&dir))
        .save(&AccountStoreSnapshot {
            accounts: vec![StoredAccount {
                key: "dsid:123".to_string(),
                apple_id: "person@example.com".to_string(),
                dsid: Some("123".to_string()),
                first_name: None,
                last_name: None,
                photos_session_id: None,
                devices_session_id: None,
                last_used_at: 10,
                last_known_status: "valid".to_string(),
            }],
            active_account_key: Some("dsid:123".to_string()),
            avatars: Default::default(),
            revision: 0,
        })
        .expect("seed account");

    let output = run_stdio_default_backend(&dir);
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stderr).contains("local backend ready"));
    let responses = responses(&output);
    assert_eq!(response_for(&responses, 2)["result"]["structuredContent"]["count"], 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn concurrent_stdio_processes_share_the_database_without_losing_writes() {
    let dir = temp_dir("concurrent");
    AccountStoreRepository::new(Some(&dir))
        .save(&AccountStoreSnapshot {
            accounts: vec![StoredAccount {
                key: "dsid:123".to_string(),
                apple_id: "person@example.com".to_string(),
                dsid: Some("123".to_string()),
                first_name: None,
                last_name: None,
                photos_session_id: None,
                devices_session_id: None,
                last_used_at: 10,
                last_known_status: "valid".to_string(),
            }],
            active_account_key: Some("dsid:123".to_string()),
            avatars: Default::default(),
            revision: 0,
        })
        .expect("seed account");

    let first_dir = dir.clone();
    let second_dir = dir.clone();
    let first = std::thread::spawn(move || run_stdio(&first_dir));
    let second = std::thread::spawn(move || run_stdio(&second_dir));
    let first = first.join().expect("first process");
    let second = second.join().expect("second process");
    assert!(first.status.success(), "stderr: {}", String::from_utf8_lossy(&first.stderr));
    assert!(second.status.success(), "stderr: {}", String::from_utf8_lossy(&second.stderr));

    let history = McpHistoryRepository::new(Some(&dir)).list(20).expect("list history");
    assert_eq!(history.len(), 2);
    assert_eq!(AccountStoreRepository::new(Some(&dir)).load().expect("load accounts").accounts.len(), 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn stdio_stays_answerable_when_the_local_backend_cannot_open() {
    let dir = temp_dir("unavailable");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let blocked = dir.join("not-a-directory");
    std::fs::write(&blocked, b"blocked").expect("write blocker");

    let output = run_stdio(&blocked);
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let responses = responses(&output);
    assert_eq!(response_for(&responses, 1)["result"]["serverInfo"]["name"], "cirrus-mcp");
    let result = &response_for(&responses, 2)["result"];
    assert_eq!(result["isError"], true);
    assert!(!result["content"][0]["text"].as_str().expect("error text").trim().is_empty());

    let _ = std::fs::remove_dir_all(dir);
}
