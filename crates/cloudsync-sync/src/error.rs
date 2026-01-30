//! Error types for the sync engine.

use thiserror::Error;

/// Result type for sync operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur in the sync engine.
#[derive(Debug, Error)]
pub enum Error {
    /// The queue is empty.
    #[error("Sync queue is empty")]
    QueueEmpty,

    /// Operation not found in queue.
    #[error("Operation with ID {0} not found in queue")]
    OperationNotFound(String),

    /// Sync orchestration error.
    #[error("Sync error: {0}")]
    Sync(String),

    /// Other errors.
    #[error("{0}")]
    Other(String),
}
