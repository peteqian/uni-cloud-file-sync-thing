//! Google Drive client with OAuth authentication.
//!
//! This module provides a high-level client for Google Drive that handles
//! OAuth authentication and token management.

use super::oauth::{OAuthAuthenticatorBuilder, OAuthConfig, OAuthError};
use std::path::PathBuf;
use yup_oauth2::{authenticator::Authenticator, InstalledFlowReturnMethod};

/// Google Drive client with OAuth authentication.
///
/// This client wraps yup-oauth2's Authenticator and provides a high-level
/// API for OAuth flow and token management.
pub struct GoogleDriveClient {
    authenticator: Authenticator<
        hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>,
    >,
    scopes: Vec<String>,
}

impl GoogleDriveClient {
    /// Creates a new Google Drive client.
    ///
    /// This initiates the OAuth flow if no cached tokens are found.
    /// The authenticator will:
    /// 1. Check for cached tokens
    /// 2. If not found, generate an authorization URL
    /// 3. Start a local HTTP server (or prompt for manual code entry)
    /// 4. Exchange the authorization code for tokens
    /// 5. Cache the tokens for future use
    ///
    /// # Arguments
    ///
    /// * `config` - OAuth configuration with client credentials
    /// * `token_cache_path` - Optional path to cache tokens (recommended)
    /// * `return_method` - How to receive the OAuth callback (HTTPRedirect or Interactive)
    ///
    /// # Returns
    ///
    /// Returns a client ready to make authenticated API calls.
    pub async fn new(
        config: OAuthConfig,
        token_cache_path: Option<PathBuf>,
        return_method: InstalledFlowReturnMethod,
    ) -> Result<Self, OAuthError> {
        let scopes = config.scopes.clone();

        let mut builder = OAuthAuthenticatorBuilder::new(config).with_return_method(return_method);

        if let Some(cache_path) = token_cache_path {
            builder = builder.with_token_cache(cache_path);
        }

        let authenticator = builder.build().await?;

        Ok(Self {
            authenticator,
            scopes,
        })
    }

    /// Gets an access token for the configured scopes.
    ///
    /// This will:
    /// - Return cached token if valid
    /// - Refresh token if expired
    /// - Initiate OAuth flow if no token exists
    ///
    /// The token returned is ready to use in Authorization headers.
    pub async fn get_token(&self) -> Result<yup_oauth2::AccessToken, OAuthError> {
        self.authenticator
            .token(&self.scopes)
            .await
            .map_err(|e| OAuthError::FlowError(e.to_string()))
    }

    /// Gets a reference to the underlying authenticator.
    ///
    /// This can be used to create a google-drive3 DriveHub:
    /// ```ignore
    /// let hub = DriveHub::new(http_client, client.authenticator());
    /// ```
    pub fn authenticator(
        &self,
    ) -> &Authenticator<
        hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>,
    > {
        &self.authenticator
    }

    /// Gets the configured scopes.
    pub fn scopes(&self) -> &[String] {
        &self.scopes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gdrive::oauth::OAuthConfig;

    #[tokio::test]
    async fn client_creation_succeeds() {
        let config = OAuthConfig::default_gdrive(
            "test-client-id".to_string(),
            "test-client-secret".to_string(),
        );

        let result = GoogleDriveClient::new(
            config.clone(),
            Some(PathBuf::from("/tmp/test-tokens.json")),
            InstalledFlowReturnMethod::Interactive,
        )
        .await;

        assert!(result.is_ok());
        let client = result.unwrap();
        assert_eq!(client.scopes(), &config.scopes);
    }

    #[tokio::test]
    async fn client_creation_without_cache() {
        let config = OAuthConfig::default_gdrive(
            "test-client-id".to_string(),
            "test-client-secret".to_string(),
        );

        let result =
            GoogleDriveClient::new(config, None, InstalledFlowReturnMethod::HTTPRedirect).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn client_scopes_match_config() {
        let config = OAuthConfig::new(
            "test-client-id".to_string(),
            "test-client-secret".to_string(),
            "http://localhost:8080".to_string(),
            vec![
                "https://www.googleapis.com/auth/drive.file".to_string(),
                "https://www.googleapis.com/auth/drive.readonly".to_string(),
            ],
        );

        let client =
            GoogleDriveClient::new(config.clone(), None, InstalledFlowReturnMethod::Interactive)
                .await
                .unwrap();

        assert_eq!(client.scopes(), config.scopes.as_slice());
    }

    #[tokio::test]
    async fn client_authenticator_reference_works() {
        let config = OAuthConfig::default_gdrive(
            "test-client-id".to_string(),
            "test-client-secret".to_string(),
        );

        let client = GoogleDriveClient::new(config, None, InstalledFlowReturnMethod::Interactive)
            .await
            .unwrap();

        // Should be able to get reference without consuming client
        let _auth = client.authenticator();
        let _auth2 = client.authenticator();
    }
}
