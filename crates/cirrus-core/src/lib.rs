//! Shared iCloud authentication, device, photo, and persistence primitives.

pub mod account_store;
pub mod apple;
pub mod cookies;
pub mod crypto;
pub mod database;
pub mod download_tasks;
pub mod download_transfers;
pub mod error;
pub mod mcp_access;
pub mod mcp_history;
pub mod models;
pub mod persistence;
pub mod services;
pub mod session;
pub mod session_manager;

pub use account_store::{AccountStoreRepository, AccountStoreSnapshot, StoredAccount};
pub use cookies::SharedCookieJar;
pub use database::{
    database_path, delete_app_state_value, load_app_state_json, load_app_state_value, open_database,
    save_app_state_json, DATABASE_FILE_NAME,
};
pub use download_tasks::{DownloadTaskRepository, LoadedDownloadTasks};
pub use download_transfers::DownloadTransferRegistry;
pub use error::{AppError, AppResult};
pub use mcp_history::{McpCallHistoryEntry, McpHistoryRepository};
pub use models::*;
pub use persistence::{SessionSnapshot, SessionVault};
pub use session::{now_millis, HttpResponse, SessionHandle, SrpSession};
pub use session_manager::{SessionManager, MAX_SESSIONS};
