//! Demonstrates automatic token refresh functionality.
//!
//! This example shows that yup-oauth2 automatically refreshes tokens
//! when they expire. The `get_token()` method handles this transparently.
//!
//! ## How it works
//!
//! 1. First call to `get_token()` returns a valid token
//! 2. If token is expired on subsequent calls, yup-oauth2 automatically:
//!    - Uses the refresh token
//!    - Requests a new access token from Google
//!    - Updates the cached tokens
//!    - Returns the new access token
//! 3. All of this happens transparently - no code changes needed
//!
//! ## Running
//!
//! ```bash
//! export GOOGLE_CLIENT_ID="your-client-id"
//! export GOOGLE_CLIENT_SECRET="your-client-secret"
//! cargo run --example token_refresh_demo
//! ```

use cloudsync_providers::gdrive::{GoogleDriveClient, OAuthConfig};
use std::env;
use std::path::PathBuf;
use yup_oauth2::InstalledFlowReturnMethod;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client_id =
        env::var("GOOGLE_CLIENT_ID").expect("GOOGLE_CLIENT_ID environment variable not set");
    let client_secret = env::var("GOOGLE_CLIENT_SECRET")
        .expect("GOOGLE_CLIENT_SECRET environment variable not set");

    println!("🔄 Token Refresh Demo\n");

    let config = OAuthConfig::default_gdrive(client_id, client_secret);
    let token_cache = PathBuf::from("./refresh_demo_tokens.json");

    let client = GoogleDriveClient::new(
        config,
        Some(token_cache.clone()),
        InstalledFlowReturnMethod::HTTPRedirect,
    )
    .await?;

    println!("✅ Client created\n");

    // Get token multiple times - refresh happens automatically if needed
    for i in 1..=3 {
        println!("📥 Request #{}: Getting access token...", i);

        let token = client.get_token().await?;

        if let Some(token_str) = token.token() {
            println!("   ✓ Token: {}...", &token_str[..40.min(token_str.len())]);
        }

        if let Some(expiry) = token.expiration_time() {
            println!("   ⏰ Expires: {}", expiry);

            // Calculate time until expiry
            let now = time::OffsetDateTime::now_utc();
            let duration = expiry - now;
            if duration.is_positive() {
                println!("   ⏳ Valid for: {} seconds", duration.whole_seconds());
            }
        }

        println!();

        // Small delay between requests
        if i < 3 {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
    }

    println!("🎉 Demo complete!");
    println!("\n📝 Key points:");
    println!("   • get_token() always returns a valid token");
    println!("   • Expired tokens are automatically refreshed");
    println!("   • No manual refresh logic needed");
    println!("   • Tokens are cached and reused when valid");
    println!("\n💡 To test actual refresh:");
    println!(
        "   1. Edit {} and set expiry to the past",
        token_cache.display()
    );
    println!("   2. Run this example again");
    println!("   3. Watch it automatically refresh the token");

    Ok(())
}
