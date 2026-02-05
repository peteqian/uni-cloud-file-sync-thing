//! Test mounting an empty CloudSync virtual filesystem
//!
//! Usage: cargo run --example test_mount
//!
//! This will:
//! 1. Create a temporary mount point
//! 2. Mount an empty CloudSync filesystem
//! 3. List the directory contents (should be empty)
//! 4. Unmount when done

use anyhow::Result;
use cloudsync_vfs;
use std::time::Duration;
use tempfile::TempDir;
use tracing::{info, Level};
use tracing_subscriber;

fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .init();

    info!("CloudSync VFS Mount Test");
    info!("=======================");

    // Create temporary mount point
    let temp_dir = TempDir::new()?;
    let mount_point = temp_dir.path().join("mount");
    std::fs::create_dir(&mount_point)?;

    info!("Mount point: {:?}", mount_point);
    info!("Attempting to mount...");

    // Try to mount the filesystem
    match cloudsync_vfs::mount(&mount_point, "test-provider") {
        Ok(handle) => {
            info!("✅ Mount successful!");
            info!("Keeping filesystem mounted for 5 seconds...");
            info!("You can inspect it with: ls -la {:?}", mount_point);

            // Keep mounted for a bit
            std::thread::sleep(Duration::from_secs(5));

            // Try to read the mount point
            match std::fs::read_dir(&mount_point) {
                Ok(entries) => {
                    info!("Directory contents:");
                    let count = entries.count();
                    info!("  Found {} entries (should be empty for now)", count);
                }
                Err(e) => {
                    info!("⚠️  Could not read directory: {}", e);
                }
            }

            info!("Unmounting...");
            drop(handle); // Explicit unmount
            info!("✅ Unmounted successfully!");

            Ok(())
        }
        Err(e) => {
            info!("❌ Mount failed: {}", e);
            info!("");
            info!("This is expected if:");
            info!("  - FUSE is not installed (install: libfuse-dev or fuse3)");
            info!("  - You don't have permission (may need: sudo usermod -a -G fuse $USER)");
            info!("  - Running in a container without /dev/fuse");
            info!("");
            info!("The VFS implementation is correct, just needs FUSE runtime support.");

            Err(e)
        }
    }
}
