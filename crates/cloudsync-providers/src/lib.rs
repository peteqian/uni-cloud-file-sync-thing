//! CloudSync Provider Implementations
//!
//! This crate provides implementations of the `CloudProvider` trait for:
//! - Google Drive (Phase 2)
//! - Dropbox (Phase 5)
//! - OneDrive (Phase 6)
//!
//! Each provider module handles authentication, API communication, and
//! mapping provider-specific concepts to CloudSync's unified model.

pub mod gdrive;

// Provider modules will be added in later phases
// pub mod dropbox;
// pub mod onedrive;

/// Re-export the CloudProvider trait for convenience.
pub use cloudsync_core::CloudProvider;
