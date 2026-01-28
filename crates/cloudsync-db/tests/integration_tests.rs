//! Integration tests for the database layer.
//!
//! These tests verify the complete workflows and interactions between
//! different database modules using in-memory SQLite databases.

use chrono::Utc;
use cloudsync_core::file_state::FileState;
use cloudsync_core::types::{AccountId, CloudPath, FileId, ProviderId};
use cloudsync_db::{
    create_account, create_file, delete_account, delete_file, delete_files_by_account,
    get_account_by_id, get_file_by_id, get_file_by_provider_id, list_accounts, list_files,
    update_account, update_file, Account, Database, File, Migration, ACCOUNTS_MIGRATION,
    FILES_MIGRATION,
};

/// Creates a test database with all migrations applied.
fn setup_test_database() -> Database {
    Database::in_memory_with_migrations(vec![
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
    ])
    .expect("Failed to create test database")
}

/// Helper to create a test account.
fn create_test_account_helper(db: &Database, provider: ProviderId, email: &str) -> AccountId {
    let account = Account::new(
        provider,
        email.to_string(),
        format!("access_token_{}", email),
        Some(format!("refresh_token_{}", email)),
        Some(Utc::now() + chrono::Duration::hours(1)),
    );

    db.with_conn(|conn| create_account(conn, &account))
        .expect("Failed to create test account");

    account.id
}

/// Helper to create a test file.
fn create_test_file_helper(
    db: &Database,
    account_id: AccountId,
    provider_file_id: &str,
    path: &str,
    name: &str,
) -> i64 {
    let file = File::new(
        account_id,
        FileId::new(provider_file_id),
        CloudPath::new(path),
        name.to_string(),
        Some(1024),
        Some(format!("hash_{}", name)),
        false,
        Utc::now(),
    );

    db.with_conn(|conn| create_file(conn, &file))
        .expect("Failed to create test file")
}

// ============================================================================
// Account Lifecycle Tests
// ============================================================================

#[test]
fn test_complete_account_lifecycle() {
    let db = setup_test_database();

    // Create account
    let account = Account::new(
        ProviderId::GoogleDrive,
        "user@example.com".to_string(),
        "access_123".to_string(),
        Some("refresh_456".to_string()),
        Some(Utc::now() + chrono::Duration::hours(1)),
    );

    let account_id = db
        .with_conn(|conn| create_account(conn, &account))
        .expect("Failed to create account");

    // Retrieve account
    let retrieved = db
        .with_conn(|conn| get_account_by_id(conn, &account_id))
        .expect("Failed to get account")
        .expect("Account not found");

    assert_eq!(retrieved.id, account_id);
    assert_eq!(retrieved.email, "user@example.com");
    assert!(retrieved.is_active);

    // Update account
    let mut updated_account = retrieved.clone();
    updated_account.email = "newemail@example.com".to_string();
    updated_account.is_active = false;

    let update_result = db
        .with_conn(|conn| update_account(conn, &updated_account))
        .expect("Failed to update account");
    assert!(update_result);

    // Verify update
    let after_update = db
        .with_conn(|conn| get_account_by_id(conn, &account_id))
        .expect("Failed to get account")
        .expect("Account not found");

    assert_eq!(after_update.email, "newemail@example.com");
    assert!(!after_update.is_active);

    // List accounts
    let all_accounts = db
        .with_conn(|conn| list_accounts(conn, false))
        .expect("Failed to list accounts");
    assert_eq!(all_accounts.len(), 1);

    let active_accounts = db
        .with_conn(|conn| list_accounts(conn, true))
        .expect("Failed to list active accounts");
    assert_eq!(active_accounts.len(), 0);

    // Delete account
    let delete_result = db
        .with_conn(|conn| delete_account(conn, &account_id))
        .expect("Failed to delete account");
    assert!(delete_result);

    // Verify deletion
    let after_delete = db
        .with_conn(|conn| get_account_by_id(conn, &account_id))
        .expect("Failed to get account");
    assert!(after_delete.is_none());
}

// ============================================================================
// File Lifecycle Tests
// ============================================================================

#[test]
fn test_complete_file_lifecycle() {
    let db = setup_test_database();
    let account_id = create_test_account_helper(&db, ProviderId::GoogleDrive, "test@example.com");

    // Create file
    let file = File::new(
        account_id.clone(),
        FileId::new("provider_123"),
        CloudPath::new("/docs/report.pdf"),
        "report.pdf".to_string(),
        Some(2048),
        Some("hash_abc123".to_string()),
        false,
        Utc::now(),
    );

    let file_id = db
        .with_conn(|conn| create_file(conn, &file))
        .expect("Failed to create file");

    assert!(file_id > 0);

    // Retrieve by ID
    let retrieved_by_id = db
        .with_conn(|conn| get_file_by_id(conn, file_id))
        .expect("Failed to get file")
        .expect("File not found");

    assert_eq!(retrieved_by_id.name, "report.pdf");
    assert_eq!(retrieved_by_id.state, FileState::Pending);

    // Retrieve by provider ID
    let retrieved_by_provider = db
        .with_conn(|conn| get_file_by_provider_id(conn, &account_id, &FileId::new("provider_123")))
        .expect("Failed to get file by provider ID")
        .expect("File not found");

    assert_eq!(retrieved_by_provider.id, file_id);

    // Update file state
    let mut updated_file = retrieved_by_id.clone();
    updated_file.state = FileState::Synced;
    updated_file.synced_at = Some(Utc::now());
    updated_file.content_hash = Some("new_hash_def456".to_string());

    let update_result = db
        .with_conn(|conn| update_file(conn, &updated_file))
        .expect("Failed to update file");
    assert!(update_result);

    // Verify update
    let after_update = db
        .with_conn(|conn| get_file_by_id(conn, file_id))
        .expect("Failed to get file")
        .expect("File not found");

    assert_eq!(after_update.state, FileState::Synced);
    assert!(after_update.synced_at.is_some());
    assert_eq!(after_update.content_hash.unwrap(), "new_hash_def456");

    // List files
    let all_files = db
        .with_conn(|conn| list_files(conn, &account_id, None))
        .expect("Failed to list files");
    assert_eq!(all_files.len(), 1);

    let synced_files = db
        .with_conn(|conn| list_files(conn, &account_id, Some(FileState::Synced)))
        .expect("Failed to list synced files");
    assert_eq!(synced_files.len(), 1);

    // Delete file
    let delete_result = db
        .with_conn(|conn| delete_file(conn, file_id))
        .expect("Failed to delete file");
    assert!(delete_result);

    // Verify deletion
    let after_delete = db
        .with_conn(|conn| get_file_by_id(conn, file_id))
        .expect("Failed to get file");
    assert!(after_delete.is_none());
}

// ============================================================================
// Multi-Account Workflows
// ============================================================================

#[test]
fn test_multiple_accounts_with_files() {
    let db = setup_test_database();

    // Create multiple accounts
    let gdrive_id = create_test_account_helper(&db, ProviderId::GoogleDrive, "gdrive@example.com");
    let dropbox_id = create_test_account_helper(&db, ProviderId::Dropbox, "dropbox@example.com");
    let onedrive_id = create_test_account_helper(&db, ProviderId::OneDrive, "onedrive@example.com");

    // Add files to each account
    create_test_file_helper(&db, gdrive_id.clone(), "gdrive_1", "/doc1.pdf", "doc1.pdf");
    create_test_file_helper(&db, gdrive_id.clone(), "gdrive_2", "/doc2.pdf", "doc2.pdf");

    create_test_file_helper(
        &db,
        dropbox_id.clone(),
        "dropbox_1",
        "/file1.txt",
        "file1.txt",
    );

    create_test_file_helper(
        &db,
        onedrive_id.clone(),
        "onedrive_1",
        "/sheet.xlsx",
        "sheet.xlsx",
    );
    create_test_file_helper(
        &db,
        onedrive_id.clone(),
        "onedrive_2",
        "/preso.pptx",
        "preso.pptx",
    );
    create_test_file_helper(
        &db,
        onedrive_id.clone(),
        "onedrive_3",
        "/notes.docx",
        "notes.docx",
    );

    // Verify file counts per account
    let gdrive_files = db
        .with_conn(|conn| list_files(conn, &gdrive_id, None))
        .expect("Failed to list Google Drive files");
    assert_eq!(gdrive_files.len(), 2);

    let dropbox_files = db
        .with_conn(|conn| list_files(conn, &dropbox_id, None))
        .expect("Failed to list Dropbox files");
    assert_eq!(dropbox_files.len(), 1);

    let onedrive_files = db
        .with_conn(|conn| list_files(conn, &onedrive_id, None))
        .expect("Failed to list OneDrive files");
    assert_eq!(onedrive_files.len(), 3);

    // Verify all accounts exist
    let all_accounts = db
        .with_conn(|conn| list_accounts(conn, false))
        .expect("Failed to list accounts");
    assert_eq!(all_accounts.len(), 3);
}

// ============================================================================
// Cascade Delete Tests
// ============================================================================

#[test]
fn test_cascade_delete_files_on_account_deletion() {
    let db = setup_test_database();
    let account_id = create_test_account_helper(&db, ProviderId::GoogleDrive, "test@example.com");

    // Create multiple files
    create_test_file_helper(&db, account_id.clone(), "file1", "/a.txt", "a.txt");
    create_test_file_helper(&db, account_id.clone(), "file2", "/b.txt", "b.txt");
    create_test_file_helper(&db, account_id.clone(), "file3", "/c.txt", "c.txt");

    // Verify files exist
    let files_before = db
        .with_conn(|conn| list_files(conn, &account_id, None))
        .expect("Failed to list files");
    assert_eq!(files_before.len(), 3);

    // Delete account
    let delete_result = db
        .with_conn(|conn| delete_account(conn, &account_id))
        .expect("Failed to delete account");
    assert!(delete_result);

    // Verify files were cascade deleted
    let files_after = db
        .with_conn(|conn| list_files(conn, &account_id, None))
        .expect("Failed to list files");
    assert_eq!(files_after.len(), 0);
}

#[test]
fn test_delete_files_by_account() {
    let db = setup_test_database();
    let account_id = create_test_account_helper(&db, ProviderId::Dropbox, "user@example.com");

    // Create files
    create_test_file_helper(&db, account_id.clone(), "f1", "/file1.pdf", "file1.pdf");
    create_test_file_helper(&db, account_id.clone(), "f2", "/file2.pdf", "file2.pdf");
    create_test_file_helper(&db, account_id.clone(), "f3", "/file3.pdf", "file3.pdf");

    // Delete all files for account
    let deleted_count = db
        .with_conn(|conn| delete_files_by_account(conn, &account_id))
        .expect("Failed to delete files");
    assert_eq!(deleted_count, 3);

    // Verify files are gone but account still exists
    let files = db
        .with_conn(|conn| list_files(conn, &account_id, None))
        .expect("Failed to list files");
    assert_eq!(files.len(), 0);

    let account = db
        .with_conn(|conn| get_account_by_id(conn, &account_id))
        .expect("Failed to get account");
    assert!(account.is_some());
}

// ============================================================================
// File State Management Tests
// ============================================================================

#[test]
fn test_file_state_transitions() {
    let db = setup_test_database();
    let account_id = create_test_account_helper(&db, ProviderId::GoogleDrive, "test@example.com");

    let file_id = create_test_file_helper(
        &db,
        account_id.clone(),
        "state_test",
        "/test.txt",
        "test.txt",
    );

    // Transition: Pending -> Syncing
    let mut file = db
        .with_conn(|conn| get_file_by_id(conn, file_id))
        .unwrap()
        .unwrap();
    assert_eq!(file.state, FileState::Pending);

    file.state = FileState::Syncing;
    db.with_conn(|conn| update_file(conn, &file)).unwrap();

    // Transition: Syncing -> Synced
    file = db
        .with_conn(|conn| get_file_by_id(conn, file_id))
        .unwrap()
        .unwrap();
    assert_eq!(file.state, FileState::Syncing);

    file.state = FileState::Synced;
    file.synced_at = Some(Utc::now());
    db.with_conn(|conn| update_file(conn, &file)).unwrap();

    // Verify final state
    file = db
        .with_conn(|conn| get_file_by_id(conn, file_id))
        .unwrap()
        .unwrap();
    assert_eq!(file.state, FileState::Synced);
    assert!(file.synced_at.is_some());

    // Test all states
    let all_states = [
        FileState::Synced,
        FileState::CloudOnly,
        FileState::Syncing,
        FileState::Pending,
        FileState::Error,
        FileState::Excluded,
        FileState::OfflineModified,
        FileState::Conflict,
    ];

    for state in all_states {
        file.state = state;
        db.with_conn(|conn| update_file(conn, &file)).unwrap();

        let retrieved = db
            .with_conn(|conn| get_file_by_id(conn, file_id))
            .unwrap()
            .unwrap();
        assert_eq!(retrieved.state, state);
    }
}

#[test]
fn test_filtering_files_by_state() {
    let db = setup_test_database();
    let account_id = create_test_account_helper(&db, ProviderId::GoogleDrive, "test@example.com");

    // Create files with different states
    let pending_id = create_test_file_helper(
        &db,
        account_id.clone(),
        "pending",
        "/pending.txt",
        "pending.txt",
    );
    let syncing_id = create_test_file_helper(
        &db,
        account_id.clone(),
        "syncing",
        "/syncing.txt",
        "syncing.txt",
    );
    let synced_id = create_test_file_helper(
        &db,
        account_id.clone(),
        "synced",
        "/synced.txt",
        "synced.txt",
    );
    let error_id =
        create_test_file_helper(&db, account_id.clone(), "error", "/error.txt", "error.txt");

    // Update states
    db.with_conn(|conn| {
        let mut file = get_file_by_id(conn, syncing_id)?.unwrap();
        file.state = FileState::Syncing;
        update_file(conn, &file)?;

        let mut file = get_file_by_id(conn, synced_id)?.unwrap();
        file.state = FileState::Synced;
        file.synced_at = Some(Utc::now());
        update_file(conn, &file)?;

        let mut file = get_file_by_id(conn, error_id)?.unwrap();
        file.state = FileState::Error;
        update_file(conn, &file)?;

        Ok::<_, cloudsync_db::DbError>(())
    })
    .unwrap();

    // Test filtering
    let pending_files = db
        .with_conn(|conn| list_files(conn, &account_id, Some(FileState::Pending)))
        .unwrap();
    assert_eq!(pending_files.len(), 1);
    assert_eq!(pending_files[0].id, pending_id);

    let syncing_files = db
        .with_conn(|conn| list_files(conn, &account_id, Some(FileState::Syncing)))
        .unwrap();
    assert_eq!(syncing_files.len(), 1);
    assert_eq!(syncing_files[0].id, syncing_id);

    let synced_files = db
        .with_conn(|conn| list_files(conn, &account_id, Some(FileState::Synced)))
        .unwrap();
    assert_eq!(synced_files.len(), 1);
    assert_eq!(synced_files[0].id, synced_id);

    let error_files = db
        .with_conn(|conn| list_files(conn, &account_id, Some(FileState::Error)))
        .unwrap();
    assert_eq!(error_files.len(), 1);
    assert_eq!(error_files[0].id, error_id);

    // Verify all files
    let all_files = db
        .with_conn(|conn| list_files(conn, &account_id, None))
        .unwrap();
    assert_eq!(all_files.len(), 4);
}

// ============================================================================
// Constraint Tests
// ============================================================================

#[test]
fn test_unique_constraint_account_provider_email() {
    let db = setup_test_database();

    let account1 = Account::new(
        ProviderId::GoogleDrive,
        "same@example.com".to_string(),
        "token1".to_string(),
        None,
        None,
    );

    db.with_conn(|conn| create_account(conn, &account1))
        .expect("First account creation should succeed");

    // Try to create duplicate
    let account2 = Account::new(
        ProviderId::GoogleDrive,
        "same@example.com".to_string(),
        "token2".to_string(),
        None,
        None,
    );

    let result = db.with_conn(|conn| create_account(conn, &account2));
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        cloudsync_db::DbError::Conflict(_)
    ));

    // Different provider, same email should succeed
    let account3 = Account::new(
        ProviderId::Dropbox,
        "same@example.com".to_string(),
        "token3".to_string(),
        None,
        None,
    );

    let result = db.with_conn(|conn| create_account(conn, &account3));
    assert!(result.is_ok());
}

#[test]
fn test_unique_constraint_file_account_provider_id() {
    let db = setup_test_database();
    let account_id = create_test_account_helper(&db, ProviderId::GoogleDrive, "test@example.com");

    let file1 = File::new(
        account_id.clone(),
        FileId::new("same_provider_id"),
        CloudPath::new("/path1.txt"),
        "file1.txt".to_string(),
        Some(100),
        None,
        false,
        Utc::now(),
    );

    db.with_conn(|conn| create_file(conn, &file1))
        .expect("First file creation should succeed");

    // Try to create duplicate
    let file2 = File::new(
        account_id.clone(),
        FileId::new("same_provider_id"),
        CloudPath::new("/path2.txt"),
        "file2.txt".to_string(),
        Some(200),
        None,
        false,
        Utc::now(),
    );

    let result = db.with_conn(|conn| create_file(conn, &file2));
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        cloudsync_db::DbError::Conflict(_)
    ));
}

// ============================================================================
// Folder Support Tests
// ============================================================================

#[test]
fn test_folder_and_file_hierarchy() {
    let db = setup_test_database();
    let account_id = create_test_account_helper(&db, ProviderId::OneDrive, "test@example.com");

    // Create folder
    let folder = File::new(
        account_id.clone(),
        FileId::new("folder_root"),
        CloudPath::new("/Documents"),
        "Documents".to_string(),
        None,
        None,
        true,
        Utc::now(),
    );

    let folder_id = db.with_conn(|conn| create_file(conn, &folder)).unwrap();

    // Create files in folder
    let file1 = File::new(
        account_id.clone(),
        FileId::new("file_1"),
        CloudPath::new("/Documents/report.pdf"),
        "report.pdf".to_string(),
        Some(5120),
        Some("hash1".to_string()),
        false,
        Utc::now(),
    );

    let file2 = File::new(
        account_id.clone(),
        FileId::new("file_2"),
        CloudPath::new("/Documents/notes.txt"),
        "notes.txt".to_string(),
        Some(1024),
        Some("hash2".to_string()),
        false,
        Utc::now(),
    );

    db.with_conn(|conn| create_file(conn, &file1)).unwrap();
    db.with_conn(|conn| create_file(conn, &file2)).unwrap();

    // Verify folder properties
    let retrieved_folder = db
        .with_conn(|conn| get_file_by_id(conn, folder_id))
        .unwrap()
        .unwrap();

    assert!(retrieved_folder.is_folder);
    assert!(retrieved_folder.size.is_none());
    assert!(retrieved_folder.content_hash.is_none());

    // Verify all items
    let all_items = db
        .with_conn(|conn| list_files(conn, &account_id, None))
        .unwrap();
    assert_eq!(all_items.len(), 3);

    let folders: Vec<_> = all_items.iter().filter(|f| f.is_folder).collect();
    let files: Vec<_> = all_items.iter().filter(|f| !f.is_folder).collect();

    assert_eq!(folders.len(), 1);
    assert_eq!(files.len(), 2);
}

// ============================================================================
// Migration Tests
// ============================================================================

#[test]
fn test_database_initialization_with_migrations() {
    // Create database with migrations
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
    ])
    .expect("Failed to create database with migrations");

    // Verify tables exist by querying them
    let accounts_count: i64 = db
        .with_conn(|conn| {
            conn.query_row("SELECT COUNT(*) FROM accounts", [], |row| row.get(0))
                .map_err(Into::into)
        })
        .expect("Accounts table should exist");
    assert_eq!(accounts_count, 0);

    let files_count: i64 = db
        .with_conn(|conn| {
            conn.query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))
                .map_err(Into::into)
        })
        .expect("Files table should exist");
    assert_eq!(files_count, 0);

    // Verify migration version is tracked
    let migration_version: i32 = db
        .with_conn(|conn| {
            conn.pragma_query_value(None, "user_version", |row| row.get(0))
                .map_err(Into::into)
        })
        .expect("Should be able to query user_version");
    assert_eq!(migration_version, 2);
}

// ============================================================================
// Real-world Sync Workflow Tests
// ============================================================================

#[test]
fn test_realistic_sync_workflow() {
    let db = setup_test_database();
    let account_id = create_test_account_helper(&db, ProviderId::GoogleDrive, "user@example.com");

    // Step 1: Discover new files from cloud (create as CloudOnly)
    let mut file1 = File::new(
        account_id.clone(),
        FileId::new("cloud_file_1"),
        CloudPath::new("/photo.jpg"),
        "photo.jpg".to_string(),
        Some(2048000),
        Some("remote_hash_1".to_string()),
        false,
        Utc::now(),
    );
    file1.state = FileState::CloudOnly;

    let file1_id = db.with_conn(|conn| create_file(conn, &file1)).unwrap();

    // Step 2: Start downloading file (transition to Syncing)
    file1 = db
        .with_conn(|conn| get_file_by_id(conn, file1_id))
        .unwrap()
        .unwrap();
    file1.state = FileState::Syncing;
    db.with_conn(|conn| update_file(conn, &file1)).unwrap();

    // Step 3: Download complete (transition to Synced)
    file1 = db
        .with_conn(|conn| get_file_by_id(conn, file1_id))
        .unwrap()
        .unwrap();
    file1.state = FileState::Synced;
    file1.synced_at = Some(Utc::now());
    db.with_conn(|conn| update_file(conn, &file1)).unwrap();

    // Step 4: Local modification detected (transition to OfflineModified)
    file1 = db
        .with_conn(|conn| get_file_by_id(conn, file1_id))
        .unwrap()
        .unwrap();
    file1.state = FileState::OfflineModified;
    file1.content_hash = Some("local_hash_modified".to_string());
    file1.modified_at = Utc::now();
    db.with_conn(|conn| update_file(conn, &file1)).unwrap();

    // Step 5: Conflict detected (remote also changed)
    file1 = db
        .with_conn(|conn| get_file_by_id(conn, file1_id))
        .unwrap()
        .unwrap();
    file1.state = FileState::Conflict;
    db.with_conn(|conn| update_file(conn, &file1)).unwrap();

    // Step 6: User resolves conflict, upload to cloud
    file1 = db
        .with_conn(|conn| get_file_by_id(conn, file1_id))
        .unwrap()
        .unwrap();
    file1.state = FileState::Syncing;
    db.with_conn(|conn| update_file(conn, &file1)).unwrap();

    // Step 7: Upload complete
    file1 = db
        .with_conn(|conn| get_file_by_id(conn, file1_id))
        .unwrap()
        .unwrap();
    file1.state = FileState::Synced;
    file1.synced_at = Some(Utc::now());
    db.with_conn(|conn| update_file(conn, &file1)).unwrap();

    // Verify final state
    let final_file = db
        .with_conn(|conn| get_file_by_id(conn, file1_id))
        .unwrap()
        .unwrap();
    assert_eq!(final_file.state, FileState::Synced);
    assert!(final_file.synced_at.is_some());
}

#[test]
fn test_multi_account_sync_workflow() {
    let db = setup_test_database();

    // Setup multiple accounts
    let gdrive = create_test_account_helper(&db, ProviderId::GoogleDrive, "gdrive@example.com");
    let dropbox = create_test_account_helper(&db, ProviderId::Dropbox, "dropbox@example.com");

    // Create files for each account with different states
    let mut gdrive_file1 = File::new(
        gdrive.clone(),
        FileId::new("gd1"),
        CloudPath::new("/work/doc.pdf"),
        "doc.pdf".to_string(),
        Some(1024),
        Some("hash_gd1".to_string()),
        false,
        Utc::now(),
    );
    gdrive_file1.state = FileState::Synced;
    gdrive_file1.synced_at = Some(Utc::now());

    let mut gdrive_file2 = File::new(
        gdrive.clone(),
        FileId::new("gd2"),
        CloudPath::new("/work/sheet.xlsx"),
        "sheet.xlsx".to_string(),
        Some(2048),
        Some("hash_gd2".to_string()),
        false,
        Utc::now(),
    );
    gdrive_file2.state = FileState::Pending;

    let mut dropbox_file1 = File::new(
        dropbox.clone(),
        FileId::new("db1"),
        CloudPath::new("/photos/img1.jpg"),
        "img1.jpg".to_string(),
        Some(512000),
        Some("hash_db1".to_string()),
        false,
        Utc::now(),
    );
    dropbox_file1.state = FileState::CloudOnly;

    let mut dropbox_file2 = File::new(
        dropbox.clone(),
        FileId::new("db2"),
        CloudPath::new("/photos/img2.jpg"),
        "img2.jpg".to_string(),
        Some(498000),
        Some("hash_db2".to_string()),
        false,
        Utc::now(),
    );
    dropbox_file2.state = FileState::Error;

    db.with_conn(|conn| create_file(conn, &gdrive_file1))
        .unwrap();
    db.with_conn(|conn| create_file(conn, &gdrive_file2))
        .unwrap();
    db.with_conn(|conn| create_file(conn, &dropbox_file1))
        .unwrap();
    db.with_conn(|conn| create_file(conn, &dropbox_file2))
        .unwrap();

    // Query files by account and state
    let gdrive_synced = db
        .with_conn(|conn| list_files(conn, &gdrive, Some(FileState::Synced)))
        .unwrap();
    assert_eq!(gdrive_synced.len(), 1);

    let gdrive_pending = db
        .with_conn(|conn| list_files(conn, &gdrive, Some(FileState::Pending)))
        .unwrap();
    assert_eq!(gdrive_pending.len(), 1);

    let dropbox_cloud_only = db
        .with_conn(|conn| list_files(conn, &dropbox, Some(FileState::CloudOnly)))
        .unwrap();
    assert_eq!(dropbox_cloud_only.len(), 1);

    let dropbox_error = db
        .with_conn(|conn| list_files(conn, &dropbox, Some(FileState::Error)))
        .unwrap();
    assert_eq!(dropbox_error.len(), 1);

    // Verify total file counts
    let total_gdrive = db
        .with_conn(|conn| list_files(conn, &gdrive, None))
        .unwrap();
    assert_eq!(total_gdrive.len(), 2);

    let total_dropbox = db
        .with_conn(|conn| list_files(conn, &dropbox, None))
        .unwrap();
    assert_eq!(total_dropbox.len(), 2);
}

// ============================================================================
// Edge Cases and Error Handling
// ============================================================================

#[test]
fn test_nonexistent_account_queries() {
    let db = setup_test_database();
    let fake_id = AccountId::new();

    // Get nonexistent account
    let result = db
        .with_conn(|conn| get_account_by_id(conn, &fake_id))
        .unwrap();
    assert!(result.is_none());

    // Update nonexistent account
    let fake_account = Account::new(
        ProviderId::GoogleDrive,
        "fake@example.com".to_string(),
        "token".to_string(),
        None,
        None,
    );
    let updated = db
        .with_conn(|conn| update_account(conn, &fake_account))
        .unwrap();
    assert!(!updated);

    // Delete nonexistent account
    let deleted = db.with_conn(|conn| delete_account(conn, &fake_id)).unwrap();
    assert!(!deleted);

    // List files for nonexistent account
    let files = db
        .with_conn(|conn| list_files(conn, &fake_id, None))
        .unwrap();
    assert_eq!(files.len(), 0);
}

#[test]
fn test_nonexistent_file_queries() {
    let db = setup_test_database();
    let account_id = create_test_account_helper(&db, ProviderId::GoogleDrive, "test@example.com");

    // Get nonexistent file
    let result = db.with_conn(|conn| get_file_by_id(conn, 99999)).unwrap();
    assert!(result.is_none());

    // Get by nonexistent provider ID
    let result = db
        .with_conn(|conn| get_file_by_provider_id(conn, &account_id, &FileId::new("fake")))
        .unwrap();
    assert!(result.is_none());

    // Update nonexistent file
    let mut fake_file = File::new(
        account_id,
        FileId::new("fake"),
        CloudPath::new("/fake.txt"),
        "fake.txt".to_string(),
        Some(100),
        None,
        false,
        Utc::now(),
    );
    fake_file.id = 99999;

    let updated = db.with_conn(|conn| update_file(conn, &fake_file)).unwrap();
    assert!(!updated);

    // Delete nonexistent file
    let deleted = db.with_conn(|conn| delete_file(conn, 99999)).unwrap();
    assert!(!deleted);
}

#[test]
fn test_empty_database_queries() {
    let db = setup_test_database();

    // List accounts in empty database
    let accounts = db.with_conn(|conn| list_accounts(conn, false)).unwrap();
    assert_eq!(accounts.len(), 0);

    let active_accounts = db.with_conn(|conn| list_accounts(conn, true)).unwrap();
    assert_eq!(active_accounts.len(), 0);
}

#[test]
fn test_timestamp_updates() {
    let db = setup_test_database();
    let account_id = create_test_account_helper(&db, ProviderId::GoogleDrive, "test@example.com");

    // Get original timestamps
    let original_account = db
        .with_conn(|conn| get_account_by_id(conn, &account_id))
        .unwrap()
        .unwrap();
    let original_updated_at = original_account.updated_at;

    // Wait and update (SQLite timestamps are in seconds, so we need to wait 1 second)
    std::thread::sleep(std::time::Duration::from_secs(1));

    let mut updated_account = original_account.clone();
    updated_account.access_token = "new_token".to_string();
    db.with_conn(|conn| update_account(conn, &updated_account))
        .unwrap();

    // Verify timestamp changed
    let final_account = db
        .with_conn(|conn| get_account_by_id(conn, &account_id))
        .unwrap()
        .unwrap();
    assert!(final_account.updated_at > original_updated_at);
}
