//! CloudSync Core Library
//!
//! This crate provides the core types, traits, and abstractions used throughout
//! the CloudSync application. It defines the provider trait that all cloud
//! storage backends must implement, as well as common types for file metadata,
//! sync states, and error handling.

pub mod browser;
pub mod error;
pub mod file_state;
pub mod provider;
pub mod types;

pub use browser::{CloudNativeFile, CloudNativeUrlBuilder, GoogleDriveUrlBuilder};
pub use error::{Error, Result};
pub use file_state::FileState;
pub use provider::CloudProvider;
pub use types::*;
