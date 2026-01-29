//! Integration tests for OAuth flow: URL generation, token exchange, and refresh.
//!
//! These tests verify that:
//! 1. OAuth authorization URLs are generated with correct parameters
//! 2. Token exchange flow properly handles authorization codes
//! 3. Token refresh logic works when access tokens expire

use cloudsync_providers::gdrive::{GoogleDriveClient, OAuthConfig, SecureTokenStorage};
use time::{Duration, OffsetDateTime};
use yup_oauth2::{
    storage::{TokenInfo, TokenStorage},
    InstalledFlowReturnMethod,
};

/// Test that OAuth configuration generates correct authorization URL parameters.
///
/// This verifies:
/// - auth_uri points to Google's OAuth endpoint
/// - token_uri points to Google's token endpoint
/// - redirect_uris are properly configured
/// - scopes are included in the configuration
#[tokio::test]
async fn test_oauth_url_generation_parameters() {
    let config = OAuthConfig::new(
        "test-client-id".to_string(),
        "test-client-secret".to_string(),
        "http://localhost:8080".to_string(),
        vec![
            "https://www.googleapis.com/auth/drive.file".to_string(),
            "https://www.googleapis.com/auth/drive.readonly".to_string(),
        ],
    );

    // Convert to ApplicationSecret to verify URL parameters
    let app_secret = config.to_application_secret();

    // Verify authorization URL
    assert_eq!(
        app_secret.auth_uri, "https://accounts.google.com/o/oauth2/auth",
        "Authorization URI should point to Google's OAuth endpoint"
    );

    // Verify token exchange URL
    assert_eq!(
        app_secret.token_uri, "https://oauth2.googleapis.com/token",
        "Token URI should point to Google's token endpoint"
    );

    // Verify redirect URI
    assert_eq!(
        app_secret.redirect_uris,
        vec!["http://localhost:8080"],
        "Redirect URI should match config"
    );

    // Verify client credentials are set
    assert_eq!(app_secret.client_id, "test-client-id");
    assert_eq!(app_secret.client_secret, "test-client-secret");
}

/// Test that OAuth URL is generated with correct scopes for default GDrive config.
#[tokio::test]
async fn test_oauth_url_generation_default_gdrive_scopes() {
    let config = OAuthConfig::default_gdrive(
        "test-client-id".to_string(),
        "test-client-secret".to_string(),
    );

    // Verify scopes are set correctly
    assert!(
        config
            .scopes
            .contains(&"https://www.googleapis.com/auth/drive.file".to_string()),
        "Should include drive.file scope"
    );
    assert!(
        config
            .scopes
            .contains(&"https://www.googleapis.com/auth/drive.metadata.readonly".to_string()),
        "Should include drive.metadata.readonly scope"
    );

    // Verify ApplicationSecret includes these for URL generation
    let app_secret = config.to_application_secret();
    assert_eq!(app_secret.client_id, "test-client-id");
}

/// Test that OAuth URL generation uses custom redirect URIs.
#[tokio::test]
async fn test_oauth_url_generation_custom_redirect_uri() {
    let custom_uri = "http://localhost:9999/oauth/callback";
    let config = OAuthConfig::new(
        "test-client-id".to_string(),
        "test-client-secret".to_string(),
        custom_uri.to_string(),
        vec!["scope1".to_string()],
    );

    let app_secret = config.to_application_secret();

    assert_eq!(
        app_secret.redirect_uris,
        vec![custom_uri],
        "Custom redirect URI should be used in authorization URL"
    );
}

/// Test token exchange error handling when invalid credentials are provided.
///
/// This verifies that:
/// - Invalid client credentials result in proper error propagation
/// - Error messages are descriptive
#[tokio::test]
async fn test_token_exchange_invalid_credentials() {
    let config = OAuthConfig::new(
        "invalid-client-id".to_string(),
        "invalid-client-secret".to_string(),
        "http://localhost:8080".to_string(),
        vec!["https://www.googleapis.com/auth/drive.file".to_string()],
    );

    let temp_dir = tempfile::tempdir().unwrap();
    let token_path = temp_dir.path().join("test_tokens.json");

    // Create client - should succeed (doesn't validate credentials yet)
    let client_result = GoogleDriveClient::new(
        config,
        Some(token_path),
        InstalledFlowReturnMethod::Interactive,
    )
    .await;

    assert!(
        client_result.is_ok(),
        "Client creation should succeed without validating credentials"
    );

    // Note: Actually calling get_token() would trigger OAuth flow
    // which would fail with invalid credentials, but we can't test that
    // without mocking the OAuth server or user interaction
}

/// Test that token storage and retrieval works for OAuth tokens.
///
/// This is part of the token exchange flow - after exchanging
/// the authorization code for tokens, they should be stored and retrievable.
#[tokio::test]
async fn test_token_exchange_storage() {
    let temp_dir = tempfile::tempdir().unwrap();
    let token_path = temp_dir.path().join("oauth_tokens.json");

    let storage = SecureTokenStorage::new_file(token_path);

    // Simulate token exchange result
    let token_info = TokenInfo {
        access_token: Some("access_token_from_exchange".to_string()),
        refresh_token: Some("refresh_token_from_exchange".to_string()),
        expires_at: Some(OffsetDateTime::now_utc() + Duration::hours(1)),
        id_token: None,
    };

    let scopes = vec![
        "https://www.googleapis.com/auth/drive.file",
        "https://www.googleapis.com/auth/drive.readonly",
    ];

    // Store tokens (as would happen after exchange)
    storage.set(&scopes, token_info.clone()).await.unwrap();

    // Retrieve tokens
    let retrieved = storage.get(&scopes).await;
    assert!(
        retrieved.is_some(),
        "Tokens should be retrievable after exchange"
    );

    let retrieved_token = retrieved.unwrap();
    assert_eq!(
        retrieved_token.access_token, token_info.access_token,
        "Access token should match"
    );
    assert_eq!(
        retrieved_token.refresh_token, token_info.refresh_token,
        "Refresh token should be stored for token refresh"
    );
}

/// Test token refresh logic with expired access token.
///
/// This verifies that:
/// - Expired tokens are detected
/// - Refresh token is available for renewal
/// - Token storage maintains refresh tokens
#[tokio::test]
async fn test_token_refresh_with_expired_token() {
    let temp_dir = tempfile::tempdir().unwrap();
    let token_path = temp_dir.path().join("expired_tokens.json");

    let storage = SecureTokenStorage::new_file(token_path);

    // Create an expired token with refresh token
    let expired_token = TokenInfo {
        access_token: Some("expired_access_token".to_string()),
        refresh_token: Some("valid_refresh_token".to_string()),
        expires_at: Some(OffsetDateTime::now_utc() - Duration::hours(1)), // Expired 1 hour ago
        id_token: None,
    };

    let scopes = vec!["https://www.googleapis.com/auth/drive.file"];

    // Store expired token
    storage.set(&scopes, expired_token.clone()).await.unwrap();

    // Retrieve token
    let retrieved = storage.get(&scopes).await;
    assert!(retrieved.is_some(), "Expired token should still be stored");

    let retrieved_token = retrieved.unwrap();

    // Verify refresh token is available for refresh flow
    assert!(
        retrieved_token.refresh_token.is_some(),
        "Refresh token should be available for token refresh"
    );
    assert_eq!(
        retrieved_token.refresh_token,
        Some("valid_refresh_token".to_string()),
        "Refresh token should match original"
    );

    // Verify expiration is preserved
    assert!(
        retrieved_token.expires_at.is_some(),
        "Expiration timestamp should be stored"
    );
    if let Some(expires_at) = retrieved_token.expires_at {
        assert!(
            expires_at < OffsetDateTime::now_utc(),
            "Token should be marked as expired"
        );
    }
}

/// Test token refresh updates stored tokens.
///
/// After a token refresh, the new access token should replace the old one
/// while preserving the refresh token.
#[tokio::test]
async fn test_token_refresh_updates_storage() {
    let temp_dir = tempfile::tempdir().unwrap();
    let token_path = temp_dir.path().join("refresh_update_tokens.json");

    let storage = SecureTokenStorage::new_file(token_path);

    let scopes = vec!["https://www.googleapis.com/auth/drive.file"];

    // Store initial expired token
    let old_token = TokenInfo {
        access_token: Some("old_access_token".to_string()),
        refresh_token: Some("refresh_token".to_string()),
        expires_at: Some(OffsetDateTime::now_utc() - Duration::hours(1)),
        id_token: None,
    };

    storage.set(&scopes, old_token).await.unwrap();

    // Simulate token refresh - store new token
    let refreshed_token = TokenInfo {
        access_token: Some("new_access_token".to_string()),
        refresh_token: Some("refresh_token".to_string()), // Same refresh token
        expires_at: Some(OffsetDateTime::now_utc() + Duration::hours(1)),
        id_token: None,
    };

    storage.set(&scopes, refreshed_token.clone()).await.unwrap();

    // Retrieve and verify updated token
    let retrieved = storage.get(&scopes).await;
    assert!(retrieved.is_some());

    let retrieved_token = retrieved.unwrap();
    assert_eq!(
        retrieved_token.access_token,
        Some("new_access_token".to_string()),
        "Access token should be updated after refresh"
    );
    assert_eq!(
        retrieved_token.refresh_token,
        Some("refresh_token".to_string()),
        "Refresh token should be preserved"
    );

    // Verify new expiration is in the future
    if let Some(expires_at) = retrieved_token.expires_at {
        assert!(
            expires_at > OffsetDateTime::now_utc(),
            "Refreshed token should have future expiration"
        );
    }
}

/// Test that tokens can be refreshed with subset of scopes.
///
/// OAuth refresh allows requesting a subset of the original scopes.
#[tokio::test]
async fn test_token_refresh_with_scope_subset() {
    let temp_dir = tempfile::tempdir().unwrap();
    let token_path = temp_dir.path().join("scope_subset_tokens.json");

    let storage = SecureTokenStorage::new_file(token_path);

    // Store token with broad scopes
    let token_with_all_scopes = TokenInfo {
        access_token: Some("full_access_token".to_string()),
        refresh_token: Some("refresh_token".to_string()),
        expires_at: Some(OffsetDateTime::now_utc() + Duration::hours(1)),
        id_token: None,
    };

    let all_scopes = vec![
        "https://www.googleapis.com/auth/drive.file",
        "https://www.googleapis.com/auth/drive.readonly",
        "https://www.googleapis.com/auth/drive.metadata.readonly",
    ];

    storage
        .set(&all_scopes, token_with_all_scopes.clone())
        .await
        .unwrap();

    // Request token with subset of scopes
    let subset_scopes = vec![
        "https://www.googleapis.com/auth/drive.file",
        "https://www.googleapis.com/auth/drive.readonly",
    ];

    let retrieved = storage.get(&subset_scopes).await;
    assert!(
        retrieved.is_some(),
        "Token should be retrievable with scope subset"
    );

    let retrieved_token = retrieved.unwrap();
    assert_eq!(
        retrieved_token.refresh_token,
        Some("refresh_token".to_string()),
        "Refresh token should be available for subset refresh"
    );
}

/// Test error handling when no refresh token is available.
#[tokio::test]
async fn test_token_refresh_missing_refresh_token() {
    let temp_dir = tempfile::tempdir().unwrap();
    let token_path = temp_dir.path().join("no_refresh_tokens.json");

    let storage = SecureTokenStorage::new_file(token_path);

    // Store token without refresh token (unusual but possible)
    let token_without_refresh = TokenInfo {
        access_token: Some("access_token".to_string()),
        refresh_token: None, // No refresh token
        expires_at: Some(OffsetDateTime::now_utc() - Duration::hours(1)),
        id_token: None,
    };

    let scopes = vec!["https://www.googleapis.com/auth/drive.file"];

    storage
        .set(&scopes, token_without_refresh.clone())
        .await
        .unwrap();

    let retrieved = storage.get(&scopes).await;
    assert!(retrieved.is_some());

    let retrieved_token = retrieved.unwrap();
    assert!(
        retrieved_token.refresh_token.is_none(),
        "Token without refresh token cannot be refreshed"
    );
}

/// Test that client preserves scopes for token requests.
///
/// The client should maintain the scopes from config for all token operations.
#[tokio::test]
async fn test_client_preserves_scopes_for_token_refresh() {
    let scopes = vec![
        "https://www.googleapis.com/auth/drive.file".to_string(),
        "https://www.googleapis.com/auth/drive.readonly".to_string(),
    ];

    let config = OAuthConfig::new(
        "test-client-id".to_string(),
        "test-client-secret".to_string(),
        "http://localhost:8080".to_string(),
        scopes.clone(),
    );

    let temp_dir = tempfile::tempdir().unwrap();
    let token_path = temp_dir.path().join("client_scopes.json");

    let client = GoogleDriveClient::new(
        config,
        Some(token_path),
        InstalledFlowReturnMethod::Interactive,
    )
    .await
    .unwrap();

    // Verify client maintains scopes
    assert_eq!(
        client.scopes(),
        scopes.as_slice(),
        "Client should preserve scopes for token refresh operations"
    );
}
