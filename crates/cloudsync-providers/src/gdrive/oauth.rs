//! OAuth 2.0 authentication for Google Drive using yup-oauth2.
//!
//! This module provides a wrapper around yup-oauth2's InstalledFlowAuthenticator
//! for Google Drive API access.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;
use yup_oauth2::{
    authenticator::Authenticator, ApplicationSecret, InstalledFlowAuthenticator,
    InstalledFlowReturnMethod,
};

/// Errors that can occur during OAuth authentication.
#[derive(Debug, Error)]
pub enum OAuthError {
    #[error("Failed to build authenticator: {0}")]
    AuthenticatorBuild(String),

    #[error("Failed to read application secret: {0}")]
    SecretRead(#[from] std::io::Error),

    #[error("Invalid application secret format: {0}")]
    InvalidSecret(#[from] serde_json::Error),

    #[error("OAuth flow error: {0}")]
    FlowError(String),
}

/// OAuth configuration for Google Drive.
///
/// This wraps yup-oauth2's ApplicationSecret with our application-specific
/// configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthConfig {
    /// OAuth client ID from Google Cloud Console.
    pub client_id: String,
    /// OAuth client secret from Google Cloud Console.
    pub client_secret: String,
    /// Redirect URI registered with the OAuth application.
    /// For installed apps, typically "http://localhost" with a port.
    pub redirect_uri: String,
    /// OAuth scopes to request.
    pub scopes: Vec<String>,
}

impl OAuthConfig {
    /// Creates a new OAuth configuration.
    pub fn new(
        client_id: String,
        client_secret: String,
        redirect_uri: String,
        scopes: Vec<String>,
    ) -> Self {
        Self {
            client_id,
            client_secret,
            redirect_uri,
            scopes,
        }
    }

    /// Creates a default configuration for Google Drive with full access.
    pub fn default_gdrive(client_id: String, client_secret: String) -> Self {
        Self {
            client_id,
            client_secret,
            redirect_uri: "http://localhost:8080".to_string(),
            scopes: vec![
                "https://www.googleapis.com/auth/drive.file".to_string(),
                "https://www.googleapis.com/auth/drive.metadata.readonly".to_string(),
            ],
        }
    }

    /// Creates an ApplicationSecret for use with yup-oauth2.
    pub fn to_application_secret(&self) -> ApplicationSecret {
        ApplicationSecret {
            client_id: self.client_id.clone(),
            client_secret: self.client_secret.clone(),
            token_uri: "https://oauth2.googleapis.com/token".to_string(),
            auth_uri: "https://accounts.google.com/o/oauth2/auth".to_string(),
            redirect_uris: vec![self.redirect_uri.clone()],
            ..Default::default()
        }
    }

    /// Reads OAuth configuration from a Google client secrets JSON file.
    pub async fn from_secret_file(path: PathBuf) -> Result<Self, OAuthError> {
        let secret = yup_oauth2::read_application_secret(path).await?;

        let client_id = secret.client_id.clone();
        let client_secret = secret.client_secret.clone();
        let redirect_uri = secret
            .redirect_uris
            .first()
            .cloned()
            .unwrap_or_else(|| "http://localhost:8080".to_string());

        Ok(Self {
            client_id,
            client_secret,
            redirect_uri,
            scopes: vec![
                "https://www.googleapis.com/auth/drive.file".to_string(),
                "https://www.googleapis.com/auth/drive.metadata.readonly".to_string(),
            ],
        })
    }
}

/// OAuth authenticator builder for Google Drive.
///
/// This provides a high-level API for setting up OAuth authentication
/// using yup-oauth2's InstalledFlowAuthenticator.
pub struct OAuthAuthenticatorBuilder {
    config: OAuthConfig,
    token_cache_path: Option<PathBuf>,
    return_method: InstalledFlowReturnMethod,
}

impl OAuthAuthenticatorBuilder {
    /// Creates a new authenticator builder with the given configuration.
    pub fn new(config: OAuthConfig) -> Self {
        Self {
            config,
            token_cache_path: None,
            return_method: InstalledFlowReturnMethod::HTTPRedirect,
        }
    }

    /// Sets the path where tokens should be cached.
    ///
    /// If set, tokens will be persisted to disk and reused across sessions.
    pub fn with_token_cache(mut self, path: PathBuf) -> Self {
        self.token_cache_path = Some(path);
        self
    }

    /// Sets the OAuth return method.
    ///
    /// Default is HTTPRedirect. For CLI applications, Interactive might be preferable.
    pub fn with_return_method(mut self, method: InstalledFlowReturnMethod) -> Self {
        self.return_method = method;
        self
    }

    /// Builds the authenticator.
    ///
    /// This creates an Authenticator that handles the complete
    /// OAuth flow including:
    /// - Generating authorization URLs
    /// - Handling the redirect callback
    /// - Exchanging authorization codes for tokens
    /// - Refreshing expired tokens
    /// - Persisting tokens to disk (if token_cache_path is set)
    pub async fn build(
        self,
    ) -> Result<
        Authenticator<
            hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>,
        >,
        OAuthError,
    > {
        let secret = self.config.to_application_secret();

        let mut auth_builder = InstalledFlowAuthenticator::builder(secret, self.return_method);

        if let Some(cache_path) = self.token_cache_path {
            auth_builder = auth_builder.persist_tokens_to_disk(cache_path);
        }

        auth_builder
            .build()
            .await
            .map_err(|e| OAuthError::AuthenticatorBuild(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oauth_config_creation() {
        let config = OAuthConfig::new(
            "test-client-id".to_string(),
            "test-client-secret".to_string(),
            "http://localhost:8080".to_string(),
            vec!["scope1".to_string(), "scope2".to_string()],
        );

        assert_eq!(config.client_id, "test-client-id");
        assert_eq!(config.client_secret, "test-client-secret");
        assert_eq!(config.redirect_uri, "http://localhost:8080");
        assert_eq!(config.scopes, vec!["scope1", "scope2"]);
    }

    #[test]
    fn oauth_config_default_gdrive() {
        let config = OAuthConfig::default_gdrive(
            "test-client-id".to_string(),
            "test-client-secret".to_string(),
        );

        assert_eq!(config.client_id, "test-client-id");
        assert_eq!(config.client_secret, "test-client-secret");
        assert_eq!(config.redirect_uri, "http://localhost:8080");
        assert!(config
            .scopes
            .contains(&"https://www.googleapis.com/auth/drive.file".to_string()));
        assert!(config
            .scopes
            .contains(&"https://www.googleapis.com/auth/drive.metadata.readonly".to_string()));
    }

    #[test]
    fn oauth_config_to_application_secret() {
        let config = OAuthConfig::new(
            "test-client-id".to_string(),
            "test-client-secret".to_string(),
            "http://localhost:9000".to_string(),
            vec!["scope1".to_string()],
        );

        let secret = config.to_application_secret();

        assert_eq!(secret.client_id, "test-client-id");
        assert_eq!(secret.client_secret, "test-client-secret");
        assert_eq!(secret.token_uri, "https://oauth2.googleapis.com/token");
        assert_eq!(secret.auth_uri, "https://accounts.google.com/o/oauth2/auth");
        assert_eq!(secret.redirect_uris, vec!["http://localhost:9000"]);
    }

    #[test]
    fn authenticator_builder_creation() {
        let config = OAuthConfig::default_gdrive(
            "test-client-id".to_string(),
            "test-client-secret".to_string(),
        );

        let builder = OAuthAuthenticatorBuilder::new(config.clone());

        assert_eq!(builder.config.client_id, config.client_id);
        assert!(builder.token_cache_path.is_none());
    }

    #[test]
    fn authenticator_builder_with_token_cache() {
        let config = OAuthConfig::default_gdrive(
            "test-client-id".to_string(),
            "test-client-secret".to_string(),
        );

        let cache_path = PathBuf::from("/tmp/tokens.json");
        let builder = OAuthAuthenticatorBuilder::new(config).with_token_cache(cache_path.clone());

        assert_eq!(builder.token_cache_path, Some(cache_path));
    }

    #[test]
    fn authenticator_builder_with_return_method() {
        let config = OAuthConfig::default_gdrive(
            "test-client-id".to_string(),
            "test-client-secret".to_string(),
        );

        let builder = OAuthAuthenticatorBuilder::new(config)
            .with_return_method(InstalledFlowReturnMethod::Interactive);

        assert!(matches!(
            builder.return_method,
            InstalledFlowReturnMethod::Interactive
        ));
    }

    #[tokio::test]
    async fn authenticator_builder_builds() {
        let config = OAuthConfig::default_gdrive(
            "test-client-id".to_string(),
            "test-client-secret".to_string(),
        );

        let builder = OAuthAuthenticatorBuilder::new(config);

        // Building should succeed even without valid credentials
        // (actual OAuth flow will fail, but builder construction shouldn't)
        let result = builder.build().await;
        assert!(result.is_ok());
    }
}
