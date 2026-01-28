//! Example demonstrating the Google Drive OAuth flow.
//!
//! This example shows how to:
//! 1. Create an OAuth configuration
//! 2. Build a GoogleDriveClient
//! 3. Obtain an access token (triggers OAuth flow if needed)
//! 4. Use the token with Google Drive API
//!
//! ## Setup
//!
//! Before running this example:
//! 1. Create a project in Google Cloud Console
//! 2. Enable the Google Drive API
//! 3. Create OAuth 2.0 credentials (Desktop application)
//! 4. Download the client secret JSON
//! 5. Set environment variables:
//!    - GOOGLE_CLIENT_ID
//!    - GOOGLE_CLIENT_SECRET
//!
//! ## Running
//!
//! ```bash
//! export GOOGLE_CLIENT_ID="your-client-id"
//! export GOOGLE_CLIENT_SECRET="your-client-secret"
//! cargo run --example gdrive_oauth_flow
//! ```
//!
//! ## What happens
//!
//! 1. The client checks for cached tokens in `./gdrive_tokens.json`
//! 2. If no tokens exist, it starts an OAuth flow:
//!    - Generates an authorization URL
//!    - Opens your browser (or displays URL)
//!    - Starts local server on port 8080
//!    - Waits for OAuth callback
//!    - Exchanges authorization code for tokens
//!    - Caches tokens to disk
//! 3. Returns an access token ready for API calls
//! 4. On subsequent runs, uses cached tokens (refreshes if expired)

use cloudsync_providers::gdrive::{GoogleDriveClient, OAuthConfig};
use std::env;
use std::path::PathBuf;
use yup_oauth2::InstalledFlowReturnMethod;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Read credentials from environment
    let client_id = env::var("GOOGLE_CLIENT_ID")
        .expect("GOOGLE_CLIENT_ID environment variable not set");
    let client_secret = env::var("GOOGLE_CLIENT_SECRET")
        .expect("GOOGLE_CLIENT_SECRET environment variable not set");

    println!("🔐 Setting up Google Drive OAuth...\n");

    // Create OAuth configuration with default scopes
    let config = OAuthConfig::default_gdrive(client_id, client_secret);

    println!("📋 Scopes requested:");
    for scope in &config.scopes {
        println!("   - {}", scope);
    }
    println!();

    // Create client with token caching
    println!("🔧 Building OAuth client...");
    let token_cache = PathBuf::from("./gdrive_tokens.json");

    let client = GoogleDriveClient::new(
        config,
        Some(token_cache.clone()),
        InstalledFlowReturnMethod::HTTPRedirect,
    )
    .await?;

    println!("✅ Client created successfully\n");

    // Get access token (triggers OAuth flow if needed)
    println!("🎫 Obtaining access token...");
    println!("   (If this is your first time, your browser will open)");

    let token = client.get_token().await?;

    println!("✅ Access token obtained!");
    println!("   Token: {}...", &token.token().unwrap_or("")[..40.min(token.token().unwrap_or("").len())]);

    if let Some(expiry) = token.expiration_time() {
        println!("   Expires: {}", expiry);
    }

    println!("\n💾 Tokens cached to: {}", token_cache.display());

    println!("\n✨ OAuth flow complete! You can now use this token with the Google Drive API.");
    println!("   On your next run, cached tokens will be used automatically.");

    // Example: Using the authenticator with google-drive3
    println!("\n📝 Example usage with google-drive3:");
    println!("   let hub = DriveHub::new(http_client, client.authenticator());");
    println!("   let files = hub.files().list().doit().await?;");

    Ok(())
}
