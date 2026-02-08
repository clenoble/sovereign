use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("Record not found: {0}")]
    NotFound(String),

    #[error("Invalid ID format: {0}")]
    InvalidId(String),

    #[error("Schema initialization failed: {0}")]
    SchemaInit(String),

    #[error("Database connection failed: {0}")]
    Connection(String),

    #[error("Query failed: {0}")]
    Query(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("SurrealDB error: {0}")]
    Surreal(#[from] surrealdb::Error),
}

pub type DbResult<T> = Result<T, DbError>;
