//! CloudSync Sync Engine
//!
//! This crate provides the sync orchestration layer for CloudSync,
//! including the priority queue for managing sync operations (upload,
//! download, delete) and coordination logic for the sync engine.

pub mod engine;
pub mod error;
pub mod operation;
pub mod queue;

pub use engine::SyncEngine;
pub use error::{Error, Result};
pub use operation::{SyncOperation, SyncOperationType, SyncPriority};
pub use queue::SyncQueue;
