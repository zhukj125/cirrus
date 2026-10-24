//! Shared iCloud authentication, device, photo, and persistence primitives.

pub mod apple;
pub mod cookies;
pub mod crypto;
pub mod database;
pub mod error;
pub mod models;
pub mod persistence;
pub mod services;
pub mod session;
pub mod session_manager;

pub use cookies::SharedCookieJar;
pub use database::{database_path, open_database, DATABASE_FILE_NAME};
pub use error::{AppError, AppResult};
pub use models::*;
pub use persistence::{SessionSnapshot, SessionVault};
pub use session::{now_millis, HttpResponse, SessionHandle, SrpSession};
pub use session_manager::{SessionManager, MAX_SESSIONS};
