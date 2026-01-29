//! Example demonstrating download with automatic browser fallback for cloud-native files.
//!
//! This example shows how to handle file downloads that might be cloud-native files
//! (Google Docs, Sheets, etc.) which should be opened in a browser instead of downloaded.
//!
//! Run with:
//! ```
//! cargo run --example download_with_browser_fallback
//! ```

use cloudsync_core::{Error, GoogleDriveUrlBuilder};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Download with Browser Fallback Example ===\n");

    // For demonstration purposes, we'll show how to handle the error
    // In a real application, you would:
    // 1. Load OAuth credentials from environment
    // 2. Create the Google Drive provider
    // 3. Attempt to download a file
    // 4. If it's a cloud-native file, open it in a browser

    println!("Example: Handling cloud-native file downloads\n");

    // Simulating what happens when you try to download a Google Docs file
    let simulated_error = Error::CloudNativeFile {
        url: "https://docs.google.com/document/d/abc123xyz/edit".to_string(),
    };

    match simulated_error {
        Error::CloudNativeFile { url } => {
            println!("✓ Detected cloud-native file");
            println!("  URL: {}", url);
            println!("  This file should be opened in a browser instead of downloaded.");
            println!();

            // In a real application, you would parse the URL and open it:
            // let url = Url::parse(&url)?;
            // CloudNativeFile { ... }.open_in_browser()?;

            println!("  [In a real app, the browser would open now]");
        }
        _ => {
            println!("✗ This would be a different error type");
        }
    }

    println!("\n=== URL Builder Examples ===\n");

    // Demonstrate URL construction for different Google Apps file types
    use cloudsync_core::{browser::CloudNativeUrlBuilder, types::FileId};

    let builder = GoogleDriveUrlBuilder;
    let file_id = FileId::new("example123");

    let examples = vec![
        (
            "application/vnd.google-apps.document",
            "Google Docs Document",
        ),
        (
            "application/vnd.google-apps.spreadsheet",
            "Google Sheets Spreadsheet",
        ),
        (
            "application/vnd.google-apps.presentation",
            "Google Slides Presentation",
        ),
        ("application/vnd.google-apps.form", "Google Forms"),
        ("application/vnd.google-apps.drawing", "Google Drawings"),
        ("application/pdf", "Regular PDF file"),
    ];

    for (mime_type, description) in examples {
        if builder.is_cloud_native(mime_type) {
            if let Some(url) = builder.build_web_url(&file_id, mime_type) {
                println!("✓ {}", description);
                println!("  MIME: {}", mime_type);
                println!("  URL:  {}", url);
                println!();
            }
        } else {
            println!("✗ {} (not cloud-native, can be downloaded)", description);
            println!("  MIME: {}", mime_type);
            println!();
        }
    }

    println!("=== Integration Pattern ===\n");
    println!("In your application code:");
    println!();
    println!("```rust");
    println!("match provider.download(&file_id, &dest_path, None).await {{");
    println!("    Ok(()) => println!(\"Downloaded successfully\"),");
    println!("    Err(Error::CloudNativeFile {{ url }}) => {{");
    println!("        println!(\"Opening in browser: {{}}\", url);");
    println!("        // Parse URL and open browser");
    println!("        let url = Url::parse(&url)?;");
    println!("        let cloud_file = CloudNativeFile::new(");
    println!("            file_id, \"gdrive\".into(), mime_type, url");
    println!("        );");
    println!("        cloud_file.open_in_browser()?;");
    println!("    }}");
    println!("    Err(e) => eprintln!(\"Error: {{}}\", e),");
    println!("}}");
    println!("```");

    Ok(())
}
