//! OAuth 2.0 authentication for Google Drive.
//!
//! This module implements the OAuth 2.0 authorization code flow with PKCE
//! for Google Drive API access.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;

/// OAuth configuration for Google Drive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthConfig {
    /// OAuth client ID from Google Cloud Console.
    pub client_id: String,
    /// Redirect URI registered with the OAuth application.
    pub redirect_uri: String,
    /// OAuth scopes to request.
    pub scopes: Vec<String>,
}

impl OAuthConfig {
    /// Creates a new OAuth configuration.
    pub fn new(client_id: String, redirect_uri: String, scopes: Vec<String>) -> Self {
        Self {
            client_id,
            redirect_uri,
            scopes,
        }
    }

    /// Creates a default configuration for Google Drive with read/write access.
    pub fn default_gdrive(client_id: String, redirect_uri: String) -> Self {
        Self {
            client_id,
            redirect_uri,
            scopes: vec![
                "https://www.googleapis.com/auth/drive.file".to_string(),
                "https://www.googleapis.com/auth/drive.metadata.readonly".to_string(),
            ],
        }
    }
}

/// PKCE (Proof Key for Code Exchange) challenge for enhanced OAuth security.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PkceChallenge {
    /// The code verifier (random string).
    pub verifier: String,
    /// The code challenge (SHA-256 hash of verifier, base64url encoded).
    pub challenge: String,
}

impl PkceChallenge {
    /// Generates a new PKCE challenge.
    ///
    /// Creates a random code verifier and derives the challenge using SHA-256.
    pub fn generate() -> Self {
        // Generate a random 32-byte verifier
        let verifier = Self::generate_verifier();
        let challenge = Self::generate_challenge(&verifier);

        Self {
            verifier,
            challenge,
        }
    }

    /// Generates a random code verifier.
    fn generate_verifier() -> String {
        use rand::Rng;
        const CHARSET: &[u8] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
        const VERIFIER_LENGTH: usize = 128;

        let mut rng = rand::thread_rng();
        (0..VERIFIER_LENGTH)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }

    /// Generates the code challenge from a verifier.
    fn generate_challenge(verifier: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let hash = hasher.finalize();
        base64_url::encode(&hash)
    }
}

/// Builder for creating OAuth authorization URLs.
pub struct AuthorizationUrlBuilder {
    config: OAuthConfig,
    state: Option<String>,
    pkce: Option<PkceChallenge>,
    access_type: String,
    prompt: Option<String>,
}

impl AuthorizationUrlBuilder {
    /// Google OAuth 2.0 authorization endpoint.
    const AUTHORIZATION_ENDPOINT: &'static str = "https://accounts.google.com/o/oauth2/v2/auth";

    /// Creates a new authorization URL builder with the given configuration.
    pub fn new(config: OAuthConfig) -> Self {
        Self {
            config,
            state: None,
            pkce: None,
            access_type: "offline".to_string(),
            prompt: None,
        }
    }

    /// Sets the state parameter for CSRF protection.
    ///
    /// The state should be a unique, random string that the client can verify
    /// when receiving the authorization code callback.
    pub fn with_state(mut self, state: String) -> Self {
        self.state = Some(state);
        self
    }

    /// Enables PKCE (Proof Key for Code Exchange) with the given challenge.
    pub fn with_pkce(mut self, pkce: PkceChallenge) -> Self {
        self.pkce = Some(pkce);
        self
    }

    /// Sets the access type (online or offline).
    ///
    /// Use "offline" to receive a refresh token for long-lived access.
    pub fn with_access_type(mut self, access_type: String) -> Self {
        self.access_type = access_type;
        self
    }

    /// Sets the prompt parameter.
    ///
    /// Common values: "consent" (force consent screen), "select_account" (force account selection).
    pub fn with_prompt(mut self, prompt: String) -> Self {
        self.prompt = Some(prompt);
        self
    }

    /// Builds the authorization URL.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Url)` with the complete authorization URL, or `Err` if URL construction fails.
    pub fn build(self) -> Result<Url, url::ParseError> {
        let mut url = Url::parse(Self::AUTHORIZATION_ENDPOINT)?;

        {
            let mut query = url.query_pairs_mut();

            // Required parameters
            query.append_pair("client_id", &self.config.client_id);
            query.append_pair("redirect_uri", &self.config.redirect_uri);
            query.append_pair("response_type", "code");
            query.append_pair("scope", &self.config.scopes.join(" "));
            query.append_pair("access_type", &self.access_type);

            // Optional parameters
            if let Some(state) = &self.state {
                query.append_pair("state", state);
            }

            if let Some(pkce) = &self.pkce {
                query.append_pair("code_challenge", &pkce.challenge);
                query.append_pair("code_challenge_method", "S256");
            }

            if let Some(prompt) = &self.prompt {
                query.append_pair("prompt", prompt);
            }
        }

        Ok(url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oauth_config_creation() {
        let config = OAuthConfig::new(
            "test-client-id".to_string(),
            "http://localhost:8080/callback".to_string(),
            vec!["scope1".to_string(), "scope2".to_string()],
        );

        assert_eq!(config.client_id, "test-client-id");
        assert_eq!(config.redirect_uri, "http://localhost:8080/callback");
        assert_eq!(config.scopes, vec!["scope1", "scope2"]);
    }

    #[test]
    fn oauth_config_default_gdrive() {
        let config = OAuthConfig::default_gdrive(
            "test-client-id".to_string(),
            "http://localhost:8080/callback".to_string(),
        );

        assert_eq!(config.client_id, "test-client-id");
        assert!(config
            .scopes
            .contains(&"https://www.googleapis.com/auth/drive.file".to_string()));
        assert!(config
            .scopes
            .contains(&"https://www.googleapis.com/auth/drive.metadata.readonly".to_string()));
    }

    #[test]
    fn pkce_challenge_generation() {
        let pkce = PkceChallenge::generate();

        // Verifier should be 128 characters
        assert_eq!(pkce.verifier.len(), 128);

        // Challenge should be base64url encoded (44 chars for SHA-256)
        assert!(!pkce.challenge.is_empty());

        // Verifier should only contain allowed characters
        for c in pkce.verifier.chars() {
            assert!(c.is_alphanumeric() || c == '-' || c == '.' || c == '_' || c == '~');
        }
    }

    #[test]
    fn pkce_challenges_are_unique() {
        let pkce1 = PkceChallenge::generate();
        let pkce2 = PkceChallenge::generate();

        assert_ne!(pkce1.verifier, pkce2.verifier);
        assert_ne!(pkce1.challenge, pkce2.challenge);
    }

    #[test]
    fn pkce_challenge_deterministic() {
        // Same verifier should produce same challenge
        let verifier = "test-verifier-12345";
        let challenge1 = PkceChallenge::generate_challenge(verifier);
        let challenge2 = PkceChallenge::generate_challenge(verifier);

        assert_eq!(challenge1, challenge2);
    }

    #[test]
    fn authorization_url_basic() {
        let config = OAuthConfig::new(
            "test-client-id".to_string(),
            "http://localhost:8080/callback".to_string(),
            vec!["scope1".to_string(), "scope2".to_string()],
        );

        let url = AuthorizationUrlBuilder::new(config)
            .build()
            .expect("Failed to build URL");

        // Verify base URL
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host_str(), Some("accounts.google.com"));
        assert_eq!(url.path(), "/o/oauth2/v2/auth");

        // Verify query parameters
        let query_pairs: Vec<_> = url.query_pairs().collect();
        assert!(query_pairs
            .iter()
            .any(|(k, v)| k == "client_id" && v == "test-client-id"));
        assert!(query_pairs
            .iter()
            .any(|(k, v)| k == "redirect_uri" && v == "http://localhost:8080/callback"));
        assert!(query_pairs
            .iter()
            .any(|(k, v)| k == "response_type" && v == "code"));
        assert!(query_pairs
            .iter()
            .any(|(k, v)| k == "scope" && v == "scope1 scope2"));
        assert!(query_pairs
            .iter()
            .any(|(k, v)| k == "access_type" && v == "offline"));
    }

    #[test]
    fn authorization_url_with_state() {
        let config = OAuthConfig::new(
            "test-client-id".to_string(),
            "http://localhost:8080/callback".to_string(),
            vec!["scope1".to_string()],
        );

        let url = AuthorizationUrlBuilder::new(config)
            .with_state("random-state-123".to_string())
            .build()
            .expect("Failed to build URL");

        let query_pairs: Vec<_> = url.query_pairs().collect();
        assert!(query_pairs
            .iter()
            .any(|(k, v)| k == "state" && v == "random-state-123"));
    }

    #[test]
    fn authorization_url_with_pkce() {
        let config = OAuthConfig::new(
            "test-client-id".to_string(),
            "http://localhost:8080/callback".to_string(),
            vec!["scope1".to_string()],
        );

        let pkce = PkceChallenge::generate();
        let challenge = pkce.challenge.clone();

        let url = AuthorizationUrlBuilder::new(config)
            .with_pkce(pkce)
            .build()
            .expect("Failed to build URL");

        let query_pairs: Vec<_> = url.query_pairs().collect();
        assert!(query_pairs
            .iter()
            .any(|(k, v)| k == "code_challenge" && v == &challenge));
        assert!(query_pairs
            .iter()
            .any(|(k, v)| k == "code_challenge_method" && v == "S256"));
    }

    #[test]
    fn authorization_url_with_prompt() {
        let config = OAuthConfig::new(
            "test-client-id".to_string(),
            "http://localhost:8080/callback".to_string(),
            vec!["scope1".to_string()],
        );

        let url = AuthorizationUrlBuilder::new(config)
            .with_prompt("consent".to_string())
            .build()
            .expect("Failed to build URL");

        let query_pairs: Vec<_> = url.query_pairs().collect();
        assert!(query_pairs
            .iter()
            .any(|(k, v)| k == "prompt" && v == "consent"));
    }

    #[test]
    fn authorization_url_with_all_options() {
        let config = OAuthConfig::default_gdrive(
            "my-client-id".to_string(),
            "http://localhost:8080/oauth/callback".to_string(),
        );

        let pkce = PkceChallenge::generate();
        let challenge = pkce.challenge.clone();

        let url = AuthorizationUrlBuilder::new(config)
            .with_state("csrf-token-xyz".to_string())
            .with_pkce(pkce)
            .with_access_type("offline".to_string())
            .with_prompt("consent".to_string())
            .build()
            .expect("Failed to build URL");

        let query_pairs: Vec<_> = url.query_pairs().collect();

        // Verify all parameters are present
        assert!(query_pairs
            .iter()
            .any(|(k, v)| k == "client_id" && v == "my-client-id"));
        assert!(query_pairs
            .iter()
            .any(|(k, v)| k == "state" && v == "csrf-token-xyz"));
        assert!(query_pairs
            .iter()
            .any(|(k, v)| k == "code_challenge" && v == &challenge));
        assert!(query_pairs
            .iter()
            .any(|(k, v)| k == "prompt" && v == "consent"));
        assert!(query_pairs
            .iter()
            .any(|(k, v)| k == "access_type" && v == "offline"));
    }

    #[test]
    fn authorization_url_custom_access_type() {
        let config = OAuthConfig::new(
            "test-client-id".to_string(),
            "http://localhost:8080/callback".to_string(),
            vec!["scope1".to_string()],
        );

        let url = AuthorizationUrlBuilder::new(config)
            .with_access_type("online".to_string())
            .build()
            .expect("Failed to build URL");

        let query_pairs: Vec<_> = url.query_pairs().collect();
        assert!(query_pairs
            .iter()
            .any(|(k, v)| k == "access_type" && v == "online"));
    }
}
