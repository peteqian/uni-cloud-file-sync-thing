//! Database error types.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("Database not found: {0}")]
    NotFound(String),

    #[error("Query failed: {0}")]
    Query(String),

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("Migration failed: {0}")]
    Migration(String),
}

pub type DbResult<T> = Result<T, DbError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_error_display() {
        let err = DbError::NotFound("/path/to/db".to_string());
        assert!(err.to_string().contains("/path/to/db"));
    }
}
