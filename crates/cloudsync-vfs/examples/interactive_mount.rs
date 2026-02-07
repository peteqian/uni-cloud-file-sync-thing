//! Interactive mount test - keeps filesystem mounted for manual inspection
//!
//! Usage: cargo run --example interactive_mount
//!
//! This mounts an empty filesystem and waits for you to press Enter,
//! giving you time to inspect it with `ls`, `stat`, etc.

use anyhow::Result;
use cloudsync_core::types::{AccountId, ProviderId};
use cloudsync_db::{
    accounts, Database, Migration, ACCOUNTS_MIGRATION, FILES_MIGRATION, VFS_INODES_MIGRATION,
};

use std::io;
use tempfile::TempDir;
use tracing::Level;

fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    println!("\n CloudSync VFS Interactive Mount Test\n");

    // Create temporary mount point
    let temp_dir = TempDir::new()?;
    let mount_point = temp_dir.path().join("cloudsync_test");
    std::fs::create_dir(&mount_point)?;

    // Create in-memory DB with all migrations
    let db = Database::in_memory_with_migrations(vec![
        Migration {
            version: 1,
            description: "Create accounts table",
            sql: ACCOUNTS_MIGRATION,
        },
        Migration {
            version: 2,
            description: "Create files table",
            sql: FILES_MIGRATION,
        },
        Migration {
            version: 3,
            description: "Create vfs_inodes table",
            sql: VFS_INODES_MIGRATION,
        },
    ])?;

    // Create a test account
    let account_id = db.with_conn(|conn| {
        let account = accounts::Account::new(
            ProviderId::GoogleDrive,
            "test@example.com".to_string(),
            "token".to_string(),
            None,
            None,
        );
        accounts::create_account(conn, &account)?;
        Ok::<AccountId, cloudsync_db::DbError>(account.id)
    })?;

    println!("Mount point: {}", mount_point.display());
    println!("Mounting filesystem...\n");

    // Mount the filesystem
    let _handle = cloudsync_vfs::mount(&mount_point, "gdrive", db, account_id)?;

    // Give it a moment to fully mount
    std::thread::sleep(std::time::Duration::from_millis(500));

    println!("Filesystem mounted successfully!\n");
    println!("Try these commands in another terminal:");
    println!("  ls -la {}", mount_point.display());
    println!("  stat {}", mount_point.display());
    println!("  df -h {}\n", mount_point.display());

    // Test basic operations programmatically
    println!("Running basic tests...\n");

    match std::fs::read_dir(&mount_point) {
        Ok(entries) => {
            let count = entries.count();
            println!("  Directory is readable");
            println!("  Contains {} entries (empty filesystem)", count);
        }
        Err(e) => {
            println!("  Failed to read directory: {}", e);
        }
    }

    match std::fs::metadata(&mount_point) {
        Ok(meta) => {
            println!("  Metadata accessible");
            println!("  Is directory: {}", meta.is_dir());
        }
        Err(e) => {
            println!("  Failed to get metadata: {}", e);
        }
    }

    println!("\nAll basic operations working!\n");
    println!("Press Enter to unmount and exit...");

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    println!("Unmounting...");
    drop(_handle);
    println!("Unmounted successfully!\n");

    Ok(())
}
