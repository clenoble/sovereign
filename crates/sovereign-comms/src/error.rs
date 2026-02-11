use thiserror::Error;

#[derive(Debug, Error)]
pub enum CommsError {
    #[error("Channel not connected: {0}")]
    NotConnected(String),

    #[error("Authentication failed: {0}")]
    AuthFailed(String),

    #[error("Send failed: {0}")]
    SendFailed(String),

    #[error("Fetch failed: {0}")]
    FetchFailed(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Database error: {0}")]
    DbError(String),

    #[error("{0}")]
    Other(String),
}

impl From<sovereign_db::DbError> for CommsError {
    fn from(e: sovereign_db::DbError) -> Self {
        CommsError::DbError(e.to_string())
    }
}

impl From<anyhow::Error> for CommsError {
    fn from(e: anyhow::Error) -> Self {
        CommsError::Other(e.to_string())
    }
}
