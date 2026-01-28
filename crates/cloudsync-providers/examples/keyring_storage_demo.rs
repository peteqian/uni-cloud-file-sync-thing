//! Demonstrates secure token storage using system keyring.
//!
//! This example shows how to use `SecureTokenStorage` to store OAuth tokens
//! in the system keyring instead of plain files.
//!
//! ## Benefits of Keyring Storage
//!
//! - **Encryption**: Tokens are encrypted by the OS
//! - **Access Control**: Protected by user authentication
//! - **Integration**: Works with system password managers
//! - **Security**: Better than plain JSON files
//!
//! ## How it Works
//!
//! - **Linux**: Uses libsecret (GNOME Keyring/KWallet)
//! - **macOS**: Uses macOS Keychain
//! - **Windows**: Uses Windows Credential Manager
//!
//! ## Setup
//!
//! ```bash
//! export GOOGLE_CLIENT_ID="your-client-id"
//! export GOOGLE_CLIENT_SECRET="your-client-secret"
//! cargo run --example keyring_storage_demo
//! ```
//!
//! ## Fallback Behavior
//!
//! This example uses `StorageBackend::KeyringWithFallback` which:
//! 1. Tries to store in system keyring
//! 2. Falls back to file storage if keyring unavailable
//! 3. Works on headless systems and in CI/CD

use cloudsync_providers::gdrive::{OAuthAuthenticatorBuilder, OAuthConfig, SecureTokenStorage};
use std::env;
use std::path::PathBuf;
use yup_oauth2::InstalledFlowReturnMethod;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client_id =
        env::var("GOOGLE_CLIENT_ID").expect("GOOGLE_CLIENT_ID environment variable not set");
    let client_secret = env::var("GOOGLE_CLIENT_SECRET")
        .expect("GOOGLE_CLIENT_SECRET environment variable not set");

    println!("🔐 Secure Token Storage Demo\n");

    // Create OAuth configuration
    let config = OAuthConfig::default_gdrive(client_id, client_secret);

    println!("📋 Storage backend options:\n");
    println!("1. Keyring-only (fails if keyring unavailable)");
    println!("2. File-only (less secure, always works)");
    println!("3. Keyring with fallback (recommended)\n");

    // Option 3: Keyring with fallback (recommended for most apps)
    println!("✅ Using: Keyring with fallback to file storage\n");

    let storage = SecureTokenStorage::new_with_fallback(
        "cloudsync-gdrive".to_string(),      // Service name in keyring
        "demo-user@example.com".to_string(), // Username/account ID
        PathBuf::from("./keyring_demo_fallback.json"),
    );

    println!("🔧 Building authenticator with secure storage...");

    let auth_builder = OAuthAuthenticatorBuilder::new(config)
        .with_custom_storage(Box::new(storage))
        .with_return_method(InstalledFlowReturnMethod::HTTPRedirect);

    let authenticator = auth_builder.build().await?;

    println!("✅ Authenticator created\n");

    // Trigger OAuth flow
    println!("🎫 Requesting token (will trigger OAuth if needed)...");

    let scopes = vec!["https://www.googleapis.com/auth/drive.file"];
    let token = authenticator.token(&scopes).await?;

    println!("✅ Token obtained!");
    if let Some(token_str) = token.token() {
        println!("   Token: {}...", &token_str[..40.min(token_str.len())]);
    }

    println!("\n🔒 Where are tokens stored?\n");
    println!("Primary: System keyring");
    println!(
        "   - Linux: secret-tool lookup service cloudsync-gdrive username demo-user@example.com"
    );
    println!("   - macOS: Keychain Access.app");
    println!("   - Windows: Credential Manager\n");
    println!("Fallback: ./keyring_demo_fallback.json");
    println!("   (only if keyring unavailable)\n");

    println!("💡 Production recommendation:");
    println!("   Use StorageBackend::KeyringWithFallback");
    println!("   - Secure on desktop systems");
    println!("   - Still works on headless servers");
    println!("   - Graceful degradation");

    Ok(())
}
