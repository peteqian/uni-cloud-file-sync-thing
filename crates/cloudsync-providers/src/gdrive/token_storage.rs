//! Secure token storage using system keyring.
//!
//! This module provides a TokenStorage implementation that stores OAuth tokens
//! in the system keyring (libsecret on Linux, Keychain on macOS, Credential Manager on Windows)
//! with fallback to file-based storage if keyring is unavailable.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;
use yup_oauth2::storage::{TokenInfo, TokenStorage};

/// Errors that can occur during token storage operations.
#[derive(Debug, Error)]
pub enum TokenStorageError {
    #[error("Keyring error: {0}")]
    Keyring(#[from] keyring::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Stored token with associated scopes.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredToken {
    scopes: Vec<String>,
    token_info: String, // JSON-serialized TokenInfo
}

/// Token storage backend selection.
#[derive(Debug, Clone)]
pub enum StorageBackend {
    /// Use system keyring (libsecret on Linux)
    Keyring {
        service_name: String,
        username: String,
    },
    /// Use file-based storage
    File { path: PathBuf },
    /// Try keyring, fallback to file if unavailable
    KeyringWithFallback {
        service_name: String,
        username: String,
        fallback_path: PathBuf,
    },
}

/// Secure token storage using system keyring with file fallback.
///
/// This implementation stores tokens in the system keyring for security.
/// If keyring is unavailable (e.g., headless systems, unsupported platforms),
/// it falls back to file-based storage.
pub struct SecureTokenStorage {
    backend: StorageBackend,
    /// Cache of stored tokens in memory
    cache: tokio::sync::RwLock<Vec<StoredToken>>,
}

impl SecureTokenStorage {
    /// Creates a new secure token storage with keyring backend.
    ///
    /// # Arguments
    ///
    /// * `service_name` - Service identifier for keyring (e.g., "cloudsync-gdrive")
    /// * `username` - Username for keyring (e.g., user's email or account ID)
    pub fn new_keyring(service_name: String, username: String) -> Self {
        Self {
            backend: StorageBackend::Keyring {
                service_name,
                username,
            },
            cache: tokio::sync::RwLock::new(Vec::new()),
        }
    }

    /// Creates a new token storage with file backend.
    pub fn new_file(path: PathBuf) -> Self {
        Self {
            backend: StorageBackend::File { path },
            cache: tokio::sync::RwLock::new(Vec::new()),
        }
    }

    /// Creates a new token storage that tries keyring first, falls back to file.
    ///
    /// This is the recommended default for desktop applications.
    pub fn new_with_fallback(
        service_name: String,
        username: String,
        fallback_path: PathBuf,
    ) -> Self {
        Self {
            backend: StorageBackend::KeyringWithFallback {
                service_name,
                username,
                fallback_path,
            },
            cache: tokio::sync::RwLock::new(Vec::new()),
        }
    }

    /// Stores token in the configured backend.
    async fn store_token(&self, key: &str, value: &str) -> Result<(), TokenStorageError> {
        match &self.backend {
            StorageBackend::Keyring {
                service_name,
                username,
            } => self.store_in_keyring(service_name, username, key, value),
            StorageBackend::File { path } => self.store_in_file(path, value).await,
            StorageBackend::KeyringWithFallback {
                service_name,
                username,
                fallback_path,
            } => {
                // Try keyring first
                match self.store_in_keyring(service_name, username, key, value) {
                    Ok(()) => Ok(()),
                    Err(_) => {
                        // Fall back to file storage
                        self.store_in_file(fallback_path, value).await
                    }
                }
            }
        }
    }

    /// Retrieves token from the configured backend.
    async fn retrieve_token(&self, key: &str) -> Result<Option<String>, TokenStorageError> {
        match &self.backend {
            StorageBackend::Keyring {
                service_name,
                username,
            } => self.retrieve_from_keyring(service_name, username, key),
            StorageBackend::File { path } => self.retrieve_from_file(path).await,
            StorageBackend::KeyringWithFallback {
                service_name,
                username,
                fallback_path,
            } => {
                // Try keyring first
                match self.retrieve_from_keyring(service_name, username, key) {
                    Ok(Some(value)) => Ok(Some(value)),
                    Ok(None) | Err(_) => {
                        // Fall back to file storage
                        self.retrieve_from_file(fallback_path).await
                    }
                }
            }
        }
    }

    /// Stores a value in the system keyring.
    fn store_in_keyring(
        &self,
        service_name: &str,
        username: &str,
        _key: &str,
        value: &str,
    ) -> Result<(), TokenStorageError> {
        let entry = keyring::Entry::new(service_name, username)?;
        entry.set_password(value)?;
        Ok(())
    }

    /// Retrieves a value from the system keyring.
    fn retrieve_from_keyring(
        &self,
        service_name: &str,
        username: &str,
        _key: &str,
    ) -> Result<Option<String>, TokenStorageError> {
        let entry = keyring::Entry::new(service_name, username)?;
        match entry.get_password() {
            Ok(password) => Ok(Some(password)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(TokenStorageError::Keyring(e)),
        }
    }

    /// Stores a value in a file.
    async fn store_in_file(&self, path: &PathBuf, value: &str) -> Result<(), TokenStorageError> {
        tokio::fs::write(path, value).await?;
        Ok(())
    }

    /// Retrieves a value from a file.
    async fn retrieve_from_file(
        &self,
        path: &PathBuf,
    ) -> Result<Option<String>, TokenStorageError> {
        match tokio::fs::read_to_string(path).await {
            Ok(content) => Ok(Some(content)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(TokenStorageError::Io(e)),
        }
    }

    /// Generates a storage key for the given scopes.
    fn scopes_key(scopes: &[&str]) -> String {
        let mut sorted_scopes = scopes.to_vec();
        sorted_scopes.sort();
        format!("token:{}", sorted_scopes.join(","))
    }
}

#[async_trait]
impl TokenStorage for SecureTokenStorage {
    async fn set(
        &self,
        scopes: &[&str],
        token: TokenInfo,
    ) -> Result<(), yup_oauth2::error::TokenStorageError> {
        // Serialize token info
        let token_json = serde_json::to_string(&token).map_err(|e| {
            yup_oauth2::error::TokenStorageError::Io(std::io::Error::other(e.to_string()))
        })?;

        let stored_token = StoredToken {
            scopes: scopes.iter().map(|s| s.to_string()).collect(),
            token_info: token_json.clone(),
        };

        // Update cache
        let mut cache = self.cache.write().await;
        // Remove existing token for these scopes
        cache.retain(|t| {
            !scopes_covered_by(
                &t.scopes.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                scopes,
            )
        });
        cache.push(stored_token);

        // Serialize all tokens
        let all_tokens_json = serde_json::to_string(&*cache).map_err(|e| {
            yup_oauth2::error::TokenStorageError::Io(std::io::Error::other(e.to_string()))
        })?;

        // Store in backend
        let key = Self::scopes_key(scopes);
        self.store_token(&key, &all_tokens_json)
            .await
            .map_err(|e| {
                yup_oauth2::error::TokenStorageError::Io(std::io::Error::other(e.to_string()))
            })?;

        Ok(())
    }

    async fn get(&self, scopes: &[&str]) -> Option<TokenInfo> {
        // Try cache first
        {
            let cache = self.cache.read().await;
            for stored in cache.iter() {
                let stored_scopes: Vec<&str> = stored.scopes.iter().map(|s| s.as_str()).collect();
                if scopes_covered_by(&stored_scopes, scopes) {
                    if let Ok(token_info) = serde_json::from_str(&stored.token_info) {
                        return Some(token_info);
                    }
                }
            }
        }

        // Try backend
        let key = Self::scopes_key(scopes);
        if let Ok(Some(json)) = self.retrieve_token(&key).await {
            if let Ok(stored_tokens) = serde_json::from_str::<Vec<StoredToken>>(&json) {
                // Update cache
                let mut cache = self.cache.write().await;
                *cache = stored_tokens.clone();

                // Find matching token
                for stored in stored_tokens.iter() {
                    let stored_scopes: Vec<&str> =
                        stored.scopes.iter().map(|s| s.as_str()).collect();
                    if scopes_covered_by(&stored_scopes, scopes) {
                        if let Ok(token_info) = serde_json::from_str(&stored.token_info) {
                            return Some(token_info);
                        }
                    }
                }
            }
        }

        None
    }
}

/// Checks if `available_scopes` covers all `required_scopes`.
///
/// Returns true if every scope in `required_scopes` is present in `available_scopes`.
fn scopes_covered_by(available_scopes: &[&str], required_scopes: &[&str]) -> bool {
    required_scopes
        .iter()
        .all(|req| available_scopes.contains(req))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scopes_covered_by_exact_match() {
        let available = vec!["scope1", "scope2"];
        let required = vec!["scope1", "scope2"];
        assert!(scopes_covered_by(&available, &required));
    }

    #[test]
    fn scopes_covered_by_subset() {
        let available = vec!["scope1", "scope2", "scope3"];
        let required = vec!["scope1", "scope2"];
        assert!(scopes_covered_by(&available, &required));
    }

    #[test]
    fn scopes_covered_by_missing_scope() {
        let available = vec!["scope1"];
        let required = vec!["scope1", "scope2"];
        assert!(!scopes_covered_by(&available, &required));
    }

    #[test]
    fn scopes_covered_by_empty() {
        let available = vec!["scope1", "scope2"];
        let required: Vec<&str> = vec![];
        assert!(scopes_covered_by(&available, &required));
    }

    #[test]
    fn scopes_key_generation() {
        let scopes = vec!["scope2", "scope1"];
        let key = SecureTokenStorage::scopes_key(&scopes);
        // Should be sorted
        assert_eq!(key, "token:scope1,scope2");
    }

    #[tokio::test]
    async fn file_storage_round_trip() {
        let temp_dir = tempfile::tempdir().unwrap();
        let token_path = temp_dir.path().join("test_tokens.json");

        let storage = SecureTokenStorage::new_file(token_path.clone());

        // Create a mock token
        let token_info = TokenInfo {
            access_token: Some("test_access_token".to_string()),
            refresh_token: Some("test_refresh_token".to_string()),
            expires_at: None,
            id_token: None,
        };

        let scopes = vec!["scope1", "scope2"];

        // Store token
        storage.set(&scopes, token_info.clone()).await.unwrap();

        // Retrieve token
        let retrieved = storage.get(&scopes).await;
        assert!(retrieved.is_some());

        let retrieved_token = retrieved.unwrap();
        assert_eq!(retrieved_token.access_token, token_info.access_token);
        assert_eq!(retrieved_token.refresh_token, token_info.refresh_token);
    }

    #[tokio::test]
    async fn file_storage_scope_matching() {
        let temp_dir = tempfile::tempdir().unwrap();
        let token_path = temp_dir.path().join("test_tokens.json");

        let storage = SecureTokenStorage::new_file(token_path);

        let token_info = TokenInfo {
            access_token: Some("test_token".to_string()),
            refresh_token: Some("test_refresh".to_string()),
            expires_at: None,
            id_token: None,
        };

        // Store with broader scopes
        storage
            .set(&vec!["scope1", "scope2", "scope3"], token_info.clone())
            .await
            .unwrap();

        // Should match subset
        let retrieved = storage.get(&vec!["scope1", "scope2"]).await;
        assert!(retrieved.is_some());

        // Should not match superset
        let not_found = storage
            .get(&vec!["scope1", "scope2", "scope3", "scope4"])
            .await;
        assert!(not_found.is_none());
    }
}
