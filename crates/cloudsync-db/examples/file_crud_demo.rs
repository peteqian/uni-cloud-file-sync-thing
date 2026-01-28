//! Manual testing demo for file CRUD operations.
//!
//! Run with: cargo run --example file_crud_demo

use chrono::Utc;
use cloudsync_core::file_state::FileState;
use cloudsync_core::types::{CloudPath, FileId, ProviderId};
use cloudsync_db::{
    create_account, create_file, delete_account, delete_file, delete_files_by_account,
    get_file_by_id, get_file_by_provider_id, list_files, update_file, Account, Database, File,
    Migration, ACCOUNTS_MIGRATION, FILES_MIGRATION,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== CloudSync File CRUD Demo ===\n");

    // Create a file-based database for testing
    let db_path = "test_files.db";

    // Clean up old database if it exists
    let _ = std::fs::remove_file(db_path);
    let _ = std::fs::remove_file(format!("{}-shm", db_path));
    let _ = std::fs::remove_file(format!("{}-wal", db_path));

    println!("Creating database at: {}", db_path);

    let migrations = vec![
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
    ];

    let db = Database::open(db_path, migrations)?;
    println!("✓ Database created and migrations applied\n");

    // Create a test account first
    println!("--- Setting up test account ---");
    let account = Account::new(
        ProviderId::GoogleDrive,
        "demo@example.com".to_string(),
        "gdrive_token_123".to_string(),
        Some("refresh_token_456".to_string()),
        None,
    );
    let account_id = db.with_conn(|conn| create_account(conn, &account))?;
    println!("✓ Created test account: {} ({})\n", account.email, account_id);

    // CREATE: Add some test files
    println!("--- CREATE Operations ---");

    let file1 = File::new(
        account_id.clone(),
        FileId::new("gdrive_file_abc123"),
        CloudPath::new("/Documents/report.pdf"),
        "report.pdf".to_string(),
        Some(1024 * 1024), // 1 MB
        Some("sha256:abc123hash".to_string()),
        false,
        Utc::now(),
    );

    let file2 = File::new(
        account_id.clone(),
        FileId::new("gdrive_file_def456"),
        CloudPath::new("/Documents/presentation.pptx"),
        "presentation.pptx".to_string(),
        Some(5 * 1024 * 1024), // 5 MB
        Some("sha256:def456hash".to_string()),
        false,
        Utc::now(),
    );

    let folder1 = File::new(
        account_id.clone(),
        FileId::new("gdrive_folder_789"),
        CloudPath::new("/Photos"),
        "Photos".to_string(),
        None, // Folders don't have size
        None,
        true,
        Utc::now(),
    );

    let file3 = File::new(
        account_id.clone(),
        FileId::new("gdrive_file_xyz999"),
        CloudPath::new("/Photos/vacation.jpg"),
        "vacation.jpg".to_string(),
        Some(3 * 1024 * 1024), // 3 MB
        Some("sha256:xyz999hash".to_string()),
        false,
        Utc::now(),
    );

    let file1_id = db.with_conn(|conn| create_file(conn, &file1))?;
    println!("✓ Created file 1: {} (ID: {})", file1.name, file1_id);

    let file2_id = db.with_conn(|conn| create_file(conn, &file2))?;
    println!("✓ Created file 2: {} (ID: {})", file2.name, file2_id);

    let folder1_id = db.with_conn(|conn| create_file(conn, &folder1))?;
    println!("✓ Created folder: {} (ID: {})", folder1.name, folder1_id);

    let file3_id = db.with_conn(|conn| create_file(conn, &file3))?;
    println!("✓ Created file 3: {} (ID: {})", file3.name, file3_id);

    // Test duplicate detection
    println!("\nTesting duplicate detection...");
    let duplicate_result = db.with_conn(|conn| {
        let duplicate = File::new(
            account_id.clone(),
            FileId::new("gdrive_file_abc123"), // Same provider_file_id
            CloudPath::new("/Different/path.pdf"),
            "different.pdf".to_string(),
            Some(2048),
            None,
            false,
            Utc::now(),
        );
        create_file(conn, &duplicate)
    });

    match duplicate_result {
        Err(e) => println!("✓ Duplicate correctly rejected: {}", e),
        Ok(_) => println!("✗ Duplicate was not rejected (this shouldn't happen!)"),
    }

    // READ: List all files
    println!("\n--- READ Operations ---");

    let all_files = db.with_conn(|conn| list_files(conn, &account_id, None))?;
    println!("Total files for account: {}", all_files.len());
    for (i, file) in all_files.iter().enumerate() {
        let size_str = if let Some(size) = file.size {
            format!("{} bytes", size)
        } else {
            "N/A".to_string()
        };
        let type_str = if file.is_folder { "Folder" } else { "File" };
        println!(
            "  {}. {} - {} | {} | State: {} {}",
            i + 1,
            file.path.as_str(),
            type_str,
            size_str,
            file.state.icon(),
            file.state.description()
        );
    }

    // Read single file by database ID
    println!("\nFetching file by ID: {}", file1_id);
    let retrieved = db.with_conn(|conn| get_file_by_id(conn, file1_id))?;
    match retrieved {
        Some(file) => {
            println!("✓ Found file:");
            println!("  Name: {}", file.name);
            println!("  Path: {}", file.path.as_str());
            println!("  Size: {} bytes", file.size.unwrap_or(0));
            println!("  Hash: {}", file.content_hash.as_deref().unwrap_or("None"));
            println!("  State: {} {}", file.state.icon(), file.state);
        }
        None => println!("✗ File not found"),
    }

    // Read file by provider ID
    println!("\nFetching file by provider ID: gdrive_file_def456");
    let by_provider = db.with_conn(|conn| {
        get_file_by_provider_id(conn, &account_id, &FileId::new("gdrive_file_def456"))
    })?;
    match by_provider {
        Some(file) => println!("✓ Found file: {} ({})", file.name, file.path.as_str()),
        None => println!("✗ File not found"),
    }

    // UPDATE: Modify files and their sync states
    println!("\n--- UPDATE Operations ---");

    let mut file_to_sync = db.with_conn(|conn| get_file_by_id(conn, file1_id))?.unwrap();

    println!("Original state: {} {}", file_to_sync.state.icon(), file_to_sync.state);

    // Simulate syncing process
    file_to_sync.state = FileState::Syncing;
    db.with_conn(|conn| update_file(conn, &file_to_sync))?;
    println!("✓ Updated to Syncing state");

    // Simulate sync completion
    file_to_sync.state = FileState::Synced;
    file_to_sync.synced_at = Some(Utc::now());
    db.with_conn(|conn| update_file(conn, &file_to_sync))?;
    println!("✓ Updated to Synced state with synced_at timestamp");

    let synced_file = db.with_conn(|conn| get_file_by_id(conn, file1_id))?.unwrap();
    println!("New state: {} {}", synced_file.state.icon(), synced_file.state);
    println!(
        "Synced at: {}",
        synced_file
            .synced_at
            .map(|t| t.to_rfc3339())
            .unwrap_or("Never".to_string())
    );

    // Simulate a sync error on another file
    println!("\nSimulating sync error on file 2...");
    let mut file_with_error = db.with_conn(|conn| get_file_by_id(conn, file2_id))?.unwrap();
    file_with_error.state = FileState::Error;
    db.with_conn(|conn| update_file(conn, &file_with_error))?;
    println!("✓ File 2 marked as Error state");

    // List files by state
    println!("\n--- Filtering by State ---");

    println!("Files in Synced state:");
    let synced_files = db.with_conn(|conn| list_files(conn, &account_id, Some(FileState::Synced)))?;
    println!("  Count: {}", synced_files.len());
    for file in &synced_files {
        println!("    - {} {}", file.state.icon(), file.name);
    }

    println!("\nFiles in Error state:");
    let error_files = db.with_conn(|conn| list_files(conn, &account_id, Some(FileState::Error)))?;
    println!("  Count: {}", error_files.len());
    for file in &error_files {
        println!("    - {} {}", file.state.icon(), file.name);
    }

    println!("\nFiles in Pending state:");
    let pending_files =
        db.with_conn(|conn| list_files(conn, &account_id, Some(FileState::Pending)))?;
    println!("  Count: {}", pending_files.len());
    for file in &pending_files {
        println!("    - {} {}", file.state.icon(), file.name);
    }

    // DELETE: Remove individual files
    println!("\n--- DELETE Operations ---");

    println!("Deleting file: {} (ID: {})", file3.name, file3_id);
    let deleted = db.with_conn(|conn| delete_file(conn, file3_id))?;
    println!("✓ File deleted: {}", deleted);

    let deleted_check = db.with_conn(|conn| get_file_by_id(conn, file3_id))?;
    println!("✓ Verification - file exists: {}", deleted_check.is_some());

    // Delete all files for an account
    println!("\nDeleting all remaining files for the account...");
    let remaining_before = db.with_conn(|conn| list_files(conn, &account_id, None))?;
    println!("Files before deletion: {}", remaining_before.len());

    let deleted_count = db.with_conn(|conn| delete_files_by_account(conn, &account_id))?;
    println!("✓ Deleted {} files", deleted_count);

    let remaining_after = db.with_conn(|conn| list_files(conn, &account_id, None))?;
    println!("Files after deletion: {}", remaining_after.len());

    // Demonstrate cascade delete
    println!("\n--- CASCADE DELETE Demo ---");

    println!("Creating new files for cascade delete demonstration...");
    let file_a = File::new(
        account_id.clone(),
        FileId::new("cascade_file_1"),
        CloudPath::new("/test_a.txt"),
        "test_a.txt".to_string(),
        Some(100),
        None,
        false,
        Utc::now(),
    );
    let file_b = File::new(
        account_id.clone(),
        FileId::new("cascade_file_2"),
        CloudPath::new("/test_b.txt"),
        "test_b.txt".to_string(),
        Some(200),
        None,
        false,
        Utc::now(),
    );

    db.with_conn(|conn| create_file(conn, &file_a))?;
    db.with_conn(|conn| create_file(conn, &file_b))?;
    println!("✓ Created 2 files for cascade test");

    let files_before = db.with_conn(|conn| list_files(conn, &account_id, None))?;
    println!("Files before account deletion: {}", files_before.len());

    println!("\nDeleting account (should cascade delete all files)...");
    db.with_conn(|conn| delete_account(conn, &account_id))?;
    println!("✓ Account deleted");

    let files_after = db.with_conn(|conn| list_files(conn, &account_id, None))?;
    println!("✓ Files after account deletion: {}", files_after.len());

    println!("\n=== Demo Complete ===");
    println!("\nTo inspect the database directly, run:");
    println!("  sqlite3 {}", db_path);
    println!(
        "  sqlite> SELECT id, name, path, state, is_folder FROM files ORDER BY path;"
    );
    println!("\nTo clean up:");
    println!("  rm {}", db_path);

    Ok(())
}
