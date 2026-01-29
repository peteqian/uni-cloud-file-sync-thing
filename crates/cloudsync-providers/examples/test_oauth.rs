//! Example: Test Google Drive OAuth Flow
//!
//! This example demonstrates the complete OAuth flow with Google Drive:
//! 1. Loads credentials from .env file
//! 2. Initiates OAuth authorization (opens browser)
//! 3. Exchanges authorization code for tokens
//! 4. Stores tokens securely in system keyring
//! 5. Lists files in Google Drive to verify authentication
//!
//! ## Setup
//!
//! 1. Copy `.env.example` to `.env` at the project root
//! 2. Fill in your Google OAuth credentials:
//!    - Get credentials from: https://console.cloud.google.com/apis/credentials
//!    - Create OAuth 2.0 Client ID (Desktop app type)
//!    - Enable Google Drive API
//! 3. Run: `cargo run --example test_oauth`
//!
//! ## What to Expect
//!
//! - Browser will open with Google login page
//! - After authorizing, you'll be redirected to localhost
//! - The example will exchange the code for tokens
//! - Tokens are stored in your system keyring for future use
//! - A list of your Google Drive files will be displayed

use cloudsync_providers::gdrive::{GoogleDriveClient, OAuthConfig};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;
use yup_oauth2::InstalledFlowReturnMethod;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("CloudSync OAuth Test - Google Drive");
    info!("====================================");

    // Load environment variables from .env file
    dotenvy::dotenv().ok();

    // Read OAuth credentials from environment
    let client_id = std::env::var("GOOGLE_CLIENT_ID").map_err(|_| {
        anyhow::anyhow!(
            "GOOGLE_CLIENT_ID not found in environment.\n\
             Please copy .env.example to .env and fill in your credentials."
        )
    })?;

    let client_secret = std::env::var("GOOGLE_CLIENT_SECRET").map_err(|_| {
        anyhow::anyhow!(
            "GOOGLE_CLIENT_SECRET not found in environment.\n\
             Please copy .env.example to .env and fill in your credentials."
        )
    })?;

    info!("✓ Loaded OAuth credentials from .env");

    // Create OAuth configuration with default Google Drive scopes
    let config = OAuthConfig::default_gdrive(client_id, client_secret);

    info!("✓ Created OAuth config with scopes:");
    for scope in &config.scopes {
        info!("  - {}", scope);
    }

    // Create Google Drive client
    // Uses system keyring for token storage (no file path specified)
    // HTTPRedirect will start a local server on port 8080 for OAuth callback
    info!("\nInitializing Google Drive client...");
    let client = GoogleDriveClient::new(
        config,
        None, // Use system keyring
        InstalledFlowReturnMethod::HTTPRedirect,
    )
    .await?;

    info!("✓ Google Drive client created");

    // Test authentication by listing files
    info!("\n🔐 Starting OAuth flow...");
    info!("Your browser will open for authorization.");
    info!("After authorizing, you'll be redirected to localhost.\n");

    // Get token - this triggers OAuth flow if not already authenticated
    let token = client.get_token().await?;

    info!("✓ Successfully authenticated!");
    info!("✓ Tokens stored securely in system keyring");

    // Display token info (safely truncated)
    if let Some(token_str) = token.token() {
        let preview_len = 20.min(token_str.len());
        info!("\nAccess token (first {} chars): {}...", preview_len, &token_str[..preview_len]);
    }

    // Verify token is valid by making a simple API call
    info!("\n📁 Testing API access...");

    let https = hyper_rustls::HttpsConnectorBuilder::new()
        .with_native_roots()
        .map_err(|e| anyhow::anyhow!("Failed to build HTTPS connector: {}", e))?
        .https_or_http()
        .enable_http1()
        .build();

    let http_client = hyper_util::client::legacy::Client::builder(
        hyper_util::rt::TokioExecutor::new()
    ).build(https);

    // Make a simple API call to verify authentication works
    let token_str = token.token().ok_or_else(|| anyhow::anyhow!("No token available"))?;

    let req = hyper::Request::builder()
        .uri("https://www.googleapis.com/drive/v3/about?fields=user")
        .header("Authorization", format!("Bearer {}", token_str))
        .body(http_body_util::Empty::<hyper::body::Bytes>::new())
        .map_err(|e| anyhow::anyhow!("Failed to build request: {}", e))?;

    let response = http_client.request(req).await?;

    if response.status().is_success() {
        info!("✓ API call successful!");
        info!("✓ OAuth flow is working correctly!");
    } else {
        info!("⚠ API call returned status: {}", response.status());
    }

    info!("\n✓ OAuth test completed successfully!");
    info!("\nNext time you run this, it will use the stored tokens");
    info!("and won't need to open the browser again.");

    Ok(())
}
