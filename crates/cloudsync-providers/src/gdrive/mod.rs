//! Google Drive provider implementation.
//!
//! This module implements OAuth 2.0 authentication and API communication
//! for Google Drive using google-drive3 and yup-oauth2.

pub mod client;
pub mod oauth;

pub use client::GoogleDriveClient;
pub use oauth::{OAuthAuthenticatorBuilder, OAuthConfig, OAuthError};
