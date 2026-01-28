//! Google Drive provider implementation.
//!
//! This module implements OAuth 2.0 authentication and API communication
//! for Google Drive.

pub mod oauth;

pub use oauth::{AuthorizationUrlBuilder, OAuthConfig, PkceChallenge};
