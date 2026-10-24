use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Auth(String),

    #[error("HTTP request failed ({url}): {source}")]
    Http {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("Response parsing failed: {0}")]
    Json(String),

    #[error("File operation failed: {0}")]
    Io(#[from] std::io::Error),

    #[error("Database operation failed: {0}")]
    Database(String),

    #[error("{0}")]
    Other(String),
}

impl AppError {
    pub fn auth(message: impl Into<String>) -> Self {
        AppError::Auth(message.into())
    }

    pub fn other(message: impl Into<String>) -> Self {
        AppError::Other(message.into())
    }

    pub fn json(error: impl std::fmt::Display) -> Self {
        AppError::Json(error.to_string())
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(error: rusqlite::Error) -> Self {
        AppError::Database(error.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(error: serde_json::Error) -> Self {
        AppError::Json(error.to_string())
    }
}

impl From<reqwest::Error> for AppError {
    fn from(error: reqwest::Error) -> Self {
        AppError::Http { url: error.url().map(|u| u.to_string()).unwrap_or_default(), source: error }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
