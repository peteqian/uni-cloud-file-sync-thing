//! Real Google Drive file listing and download example.
//!
//! This example authenticates with your Google Drive and lets you:
//! - List files in your root folder
//! - Download a file by selecting its number
//! - Automatically open cloud-native files (Google Docs) in browser
//!
//! Run with:
//! ```
//! cargo run --example list_and_download
//! ```

use cloudsync_core::{
    browser::CloudNativeFile,
    provider::CloudProvider,
    types::CloudPath,
    Error,
};
use cloudsync_providers::gdrive::{GoogleDriveClient, GoogleDriveProvider, OAuthConfig};
use std::io::{self, Write};
use std::path::PathBuf;
use url::Url;
use yup_oauth2::InstalledFlowReturnMethod;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load environment variables
    dotenvy::dotenv().ok();

    println!("=== Google Drive File Viewer & Downloader ===\n");

    // Get OAuth credentials from environment
    let client_id = std::env::var("GOOGLE_CLIENT_ID")
        .expect("GOOGLE_CLIENT_ID must be set in .env file");
    let client_secret = std::env::var("GOOGLE_CLIENT_SECRET")
        .expect("GOOGLE_CLIENT_SECRET must be set in .env file");

    println!("Authenticating with Google Drive...");

    // Create OAuth config
    let config = OAuthConfig::default_gdrive(client_id, client_secret);

    // Create client with token caching
    let token_cache = directories::ProjectDirs::from("com", "cloudsync", "app")
        .map(|dirs| {
            let token_path = dirs.data_dir().join("gdrive_tokens.json");
            // Ensure parent directory exists
            if let Some(parent) = token_path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            token_path
        });

    let client = GoogleDriveClient::new(
        config,
        token_cache,
        InstalledFlowReturnMethod::HTTPRedirect,
    )
    .await?;

    // Create provider
    let provider = GoogleDriveProvider::new(client).await?;

    println!("✓ Authenticated as: {}\n", provider.display_name());

    // List files in root folder
    println!("Fetching files from your Google Drive root folder...\n");
    let items = provider.list_folder(&CloudPath::root()).await?;

    if items.is_empty() {
        println!("No files found in root folder.");
        return Ok(());
    }

    println!("Found {} items:\n", items.len());

    // Display files
    for (i, item) in items.iter().enumerate() {
        let icon = if item.is_folder { "📁" } else { "📄" };
        let size_str = if let Some(size) = item.size {
            format!("{} bytes", size)
        } else {
            "N/A".to_string()
        };

        println!(
            "{}. {} {} ({})",
            i + 1,
            icon,
            item.name,
            size_str
        );
        if let Some(mime) = &item.mime_type {
            println!("   Type: {}", mime);
        }
    }

    // Prompt user to select a file
    println!("\nEnter a number to download/view a file (or 'q' to quit): ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim();

    if input == "q" {
        println!("Goodbye!");
        return Ok(());
    }

    let selection: usize = input.parse()?;
    if selection == 0 || selection > items.len() {
        println!("Invalid selection.");
        return Ok(());
    }

    let selected_item = &items[selection - 1];
    println!("\nSelected: {}", selected_item.name);

    if selected_item.is_folder {
        println!("This is a folder. Folder download not yet supported.");
        return Ok(());
    }

    // Attempt to download
    let dest = PathBuf::from(format!("/tmp/{}", selected_item.name));
    println!("Downloading to: {:?}", dest);

    match provider
        .download(&selected_item.id, &dest, None)
        .await
    {
        Ok(()) => {
            println!("✓ Downloaded successfully to {:?}", dest);
        }
        Err(Error::CloudNativeFile { url }) => {
            println!("\n⚠️  This is a cloud-native file (Google Docs/Sheets/Slides)");
            println!("These files live in the cloud and should be opened in a browser.\n");
            println!("Opening in your browser: {}", url);

            // Parse URL and open in browser
            let web_url = Url::parse(&url)?;
            let cloud_file = CloudNativeFile::new(
                selected_item.id.clone(),
                "gdrive".to_string(),
                selected_item.mime_type.clone().unwrap_or_default(),
                web_url,
            );

            if let Err(e) = cloud_file.open_in_browser() {
                println!("Failed to open browser: {}", e);
                println!("Please open this URL manually: {}", url);
            } else {
                println!("✓ Opened in browser!");
            }
        }
        Err(e) => {
            println!("✗ Error downloading: {}", e);
        }
    }

    Ok(())
}
