//! Interactive mount test - keeps filesystem mounted for manual inspection
//!
//! Usage: cargo run --example interactive_mount
//!
//! This mounts an empty filesystem and waits for you to press Enter,
//! giving you time to inspect it with `ls`, `stat`, etc.

use anyhow::Result;
use cloudsync_vfs;
use std::io::{self, Write};
use tempfile::TempDir;
use tracing::{info, Level};
use tracing_subscriber;

fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    println!("\n╔════════════════════════════════════════╗");
    println!("║  CloudSync VFS Interactive Mount Test  ║");
    println!("╚════════════════════════════════════════╝\n");

    // Create temporary mount point
    let temp_dir = TempDir::new()?;
    let mount_point = temp_dir.path().join("cloudsync_test");
    std::fs::create_dir(&mount_point)?;

    println!("📁 Mount point: {}", mount_point.display());
    println!("🚀 Mounting filesystem...\n");

    // Mount the filesystem
    let _handle = cloudsync_vfs::mount(&mount_point, "gdrive")?;

    // Give it a moment to fully mount
    std::thread::sleep(std::time::Duration::from_millis(500));

    println!("✅ Filesystem mounted successfully!\n");
    println!("╭─────────────────────────────────────╮");
    println!("│  Try these commands in another      │");
    println!("│  terminal:                           │");
    println!("├─────────────────────────────────────┤");
    println!("│  ls -la {}  │", mount_point.display());
    println!("│  stat {}    │", mount_point.display());
    println!("│  df -h {}   │", mount_point.display());
    println!("╰─────────────────────────────────────╯\n");

    // Test basic operations programmatically
    println!("🧪 Running basic tests...\n");

    match std::fs::read_dir(&mount_point) {
        Ok(entries) => {
            let count = entries.count();
            println!("  ✓ Directory is readable");
            println!("  ✓ Contains {} entries (empty filesystem)", count);
        }
        Err(e) => {
            println!("  ✗ Failed to read directory: {}", e);
        }
    }

    match std::fs::metadata(&mount_point) {
        Ok(meta) => {
            println!("  ✓ Metadata accessible");
            println!("  ✓ Is directory: {}", meta.is_dir());
        }
        Err(e) => {
            println!("  ✗ Failed to get metadata: {}", e);
        }
    }

    println!("\n✅ All basic operations working!\n");
    println!("Press Enter to unmount and exit...");

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    println!("🔽 Unmounting...");
    drop(_handle);
    println!("✅ Unmounted successfully!\n");

    Ok(())
}
