//! CloudSync Database Layer
//!
//! This crate provides SQLite-based storage for CloudSync metadata,
//! including accounts, file states, sync queues, and conflict history.
//!
//! Full implementation in Phase 1.4.

pub mod error;

pub use error::{DbError, DbResult};

/// Placeholder for database connection.
/// Will be fully implemented in Phase 1.4.
pub struct Database {
    _placeholder: (),
}

impl Database {
    /// Opens an in-memory database (for testing).
    pub fn in_memory() -> DbResult<Self> {
        Ok(Self { _placeholder: () })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_create_in_memory_database() {
        let db = Database::in_memory();
        assert!(db.is_ok());
    }
}
