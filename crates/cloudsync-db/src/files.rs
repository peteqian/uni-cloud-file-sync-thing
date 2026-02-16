//! File CRUD operations for cloud-synced files.
//!
//! This module provides database operations for managing file metadata,
//! including sync states, content hashes, and cloud provider references.

use crate::{DbError, DbResult};
use chrono::{DateTime, Utc};
use cloudsync_core::file_state::FileState;
use cloudsync_core::types::{AccountId, CloudPath, FileId};
use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};

/// Database migration for the files table.
///
/// This creates the files table with fields for:
/// - Unique local identifier (INTEGER PRIMARY KEY)
/// - Account reference (foreign key)
/// - Provider-specific file ID
/// - File path and name
/// - Size, hash, and folder flag
/// - Sync state tracking
/// - Timestamps for modification and sync
pub const FILES_MIGRATION: &str = r#"
CREATE TABLE IF NOT EXISTS files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id TEXT NOT NULL,
    provider_file_id TEXT NOT NULL,
    path TEXT NOT NULL,
    name TEXT NOT NULL,
    size INTEGER,
    content_hash TEXT,
    is_folder INTEGER NOT NULL DEFAULT 0,
    state TEXT NOT NULL CHECK(state IN (
        'synced', 'cloud_only', 'syncing', 'pending',
        'error', 'excluded', 'offline_modified', 'conflict'
    )),
    modified_at INTEGER NOT NULL,
    synced_at INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(account_id, provider_file_id),
    FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE
);

CREATE INDEX idx_files_account_id ON files(account_id);
CREATE INDEX idx_files_path ON files(path);
CREATE INDEX idx_files_state ON files(state);
CREATE INDEX idx_files_is_folder ON files(is_folder);
"#;

/// Represents a cloud-synced file stored in the database.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct File {
    /// Unique local database identifier.
    pub id: i64,

    /// The account this file belongs to.
    pub account_id: AccountId,

    /// Provider-specific file identifier from the cloud API.
    pub provider_file_id: FileId,

    /// Full path of the file in the cloud filesystem.
    pub path: CloudPath,

    /// Name of the file or folder.
    pub name: String,

    /// Size in bytes (None for folders).
    pub size: Option<u64>,

    /// Content hash for change detection.
    pub content_hash: Option<String>,

    /// Whether this is a folder.
    pub is_folder: bool,

    /// Current synchronization state.
    pub state: FileState,

    /// Last modification time from the provider.
    pub modified_at: DateTime<Utc>,

    /// When this file was last successfully synced.
    pub synced_at: Option<DateTime<Utc>>,

    /// When this record was created in the database.
    pub created_at: DateTime<Utc>,

    /// When this record was last updated.
    pub updated_at: DateTime<Utc>,
}

impl File {
    /// Creates a new file record.
    ///
    /// The local ID is set to 0 (will be assigned by the database),
    /// and timestamps are set to now.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        account_id: AccountId,
        provider_file_id: FileId,
        path: CloudPath,
        name: String,
        size: Option<u64>,
        content_hash: Option<String>,
        is_folder: bool,
        modified_at: DateTime<Utc>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: 0, // Will be assigned by database
            account_id,
            provider_file_id,
            path,
            name,
            size,
            content_hash,
            is_folder,
            state: FileState::Pending,
            modified_at,
            synced_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Maps a database row to a File.
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        // Parse state from string
        let state_str: String = row.get(8)?;
        let state = match state_str.as_str() {
            "synced" => FileState::Synced,
            "cloud_only" => FileState::CloudOnly,
            "syncing" => FileState::Syncing,
            "pending" => FileState::Pending,
            "error" => FileState::Error,
            "excluded" => FileState::Excluded,
            "offline_modified" => FileState::OfflineModified,
            "conflict" => FileState::Conflict,
            _ => return Err(rusqlite::Error::InvalidQuery),
        };

        // Parse timestamps
        let modified_at: i64 = row.get(9)?;
        let synced_at: Option<i64> = row.get(10)?;
        let created_at: i64 = row.get(11)?;
        let updated_at: i64 = row.get(12)?;

        // Parse optional size as u64
        let size: Option<i64> = row.get(5)?;
        let size = size.map(|s| s as u64);

        Ok(Self {
            id: row.get(0)?,
            account_id: AccountId::from_string(row.get::<_, String>(1)?),
            provider_file_id: FileId::new(row.get::<_, String>(2)?),
            path: CloudPath::new(row.get::<_, String>(3)?),
            name: row.get(4)?,
            size,
            content_hash: row.get(6)?,
            is_folder: row.get::<_, i32>(7)? != 0,
            state,
            modified_at: DateTime::from_timestamp(modified_at, 0).unwrap(),
            synced_at: synced_at.and_then(|ts| DateTime::from_timestamp(ts, 0)),
            created_at: DateTime::from_timestamp(created_at, 0).unwrap(),
            updated_at: DateTime::from_timestamp(updated_at, 0).unwrap(),
        })
    }
}

/// Creates a new file record in the database.
///
/// # Arguments
///
/// * `conn` - Database connection
/// * `file` - The file to create
///
/// # Returns
///
/// * `Ok(i64)` - The ID of the created file
/// * `Err(DbError::Conflict)` - If a file with the same account_id and provider_file_id already exists
pub fn create_file(conn: &Connection, file: &File) -> DbResult<i64> {
    conn.execute(
        "INSERT INTO files (
            account_id, provider_file_id, path, name, size,
            content_hash, is_folder, state, modified_at, synced_at,
            created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            file.account_id.to_string(),
            file.provider_file_id.to_string(),
            file.path.as_str(),
            file.name,
            file.size.map(|s| s as i64),
            file.content_hash,
            file.is_folder as i32,
            match file.state {
                FileState::Synced => "synced",
                FileState::CloudOnly => "cloud_only",
                FileState::Syncing => "syncing",
                FileState::Pending => "pending",
                FileState::Error => "error",
                FileState::Excluded => "excluded",
                FileState::OfflineModified => "offline_modified",
                FileState::Conflict => "conflict",
            },
            file.modified_at.timestamp(),
            file.synced_at.map(|dt| dt.timestamp()),
            file.created_at.timestamp(),
            file.updated_at.timestamp(),
        ],
    )
    .map_err(|e| match e {
        rusqlite::Error::SqliteFailure(err, _)
            if err.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            DbError::Conflict(
                "File with this account and provider_file_id already exists".to_string(),
            )
        }
        _ => DbError::from(e),
    })?;

    Ok(conn.last_insert_rowid())
}

/// Retrieves a file by its local database ID.
///
/// # Returns
///
/// * `Ok(Some(File))` - The file if found
/// * `Ok(None)` - If no file with this ID exists
pub fn get_file_by_id(conn: &Connection, id: i64) -> DbResult<Option<File>> {
    conn.query_row(
        "SELECT id, account_id, provider_file_id, path, name, size,
                content_hash, is_folder, state, modified_at, synced_at,
                created_at, updated_at
         FROM files WHERE id = ?1",
        params![id],
        File::from_row,
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        _ => Err(DbError::from(e)),
    })
}

/// Retrieves a file by account and provider file ID.
///
/// # Returns
///
/// * `Ok(Some(File))` - The file if found
/// * `Ok(None)` - If no file with this combination exists
pub fn get_file_by_provider_id(
    conn: &Connection,
    account_id: &AccountId,
    provider_file_id: &FileId,
) -> DbResult<Option<File>> {
    conn.query_row(
        "SELECT id, account_id, provider_file_id, path, name, size,
                content_hash, is_folder, state, modified_at, synced_at,
                created_at, updated_at
         FROM files WHERE account_id = ?1 AND provider_file_id = ?2",
        params![account_id.to_string(), provider_file_id.to_string()],
        File::from_row,
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        _ => Err(DbError::from(e)),
    })
}

/// Retrieves a file by account and cloud path.
///
/// Uses the `idx_files_path` index for efficient lookups.
///
/// # Returns
///
/// * `Ok(Some(File))` - The file if found
/// * `Ok(None)` - If no file with this account and path exists
pub fn get_file_by_cloud_path(
    conn: &Connection,
    account_id: &AccountId,
    cloud_path: &CloudPath,
) -> DbResult<Option<File>> {
    conn.query_row(
        "SELECT id, account_id, provider_file_id, path, name, size,
                content_hash, is_folder, state, modified_at, synced_at,
                created_at, updated_at
         FROM files WHERE account_id = ?1 AND path = ?2",
        params![account_id.to_string(), cloud_path.as_str()],
        File::from_row,
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        _ => Err(DbError::from(e)),
    })
}

/// Lists all files for a given account.
///
/// # Arguments
///
/// * `account_id` - The account to list files for
/// * `state_filter` - Optional state to filter by (e.g., only pending files)
pub fn list_files(
    conn: &Connection,
    account_id: &AccountId,
    state_filter: Option<FileState>,
) -> DbResult<Vec<File>> {
    let sql = if let Some(state) = state_filter {
        format!(
            "SELECT id, account_id, provider_file_id, path, name, size,
                    content_hash, is_folder, state, modified_at, synced_at,
                    created_at, updated_at
             FROM files WHERE account_id = ?1 AND state = '{}'
             ORDER BY path ASC",
            match state {
                FileState::Synced => "synced",
                FileState::CloudOnly => "cloud_only",
                FileState::Syncing => "syncing",
                FileState::Pending => "pending",
                FileState::Error => "error",
                FileState::Excluded => "excluded",
                FileState::OfflineModified => "offline_modified",
                FileState::Conflict => "conflict",
            }
        )
    } else {
        "SELECT id, account_id, provider_file_id, path, name, size,
                content_hash, is_folder, state, modified_at, synced_at,
                created_at, updated_at
         FROM files WHERE account_id = ?1
         ORDER BY path ASC"
            .to_string()
    };

    let mut stmt = conn.prepare(&sql)?;
    let files = stmt
        .query_map([account_id.to_string()], File::from_row)?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(files)
}

/// Updates an existing file record.
///
/// # Returns
///
/// * `Ok(true)` - If the file was updated
/// * `Ok(false)` - If no file with this ID exists
pub fn update_file(conn: &Connection, file: &File) -> DbResult<bool> {
    let updated_file = File {
        updated_at: Utc::now(),
        ..file.clone()
    };

    let rows_affected = conn.execute(
        "UPDATE files
         SET account_id = ?2, provider_file_id = ?3, path = ?4, name = ?5,
             size = ?6, content_hash = ?7, is_folder = ?8, state = ?9,
             modified_at = ?10, synced_at = ?11, updated_at = ?12
         WHERE id = ?1",
        params![
            updated_file.id,
            updated_file.account_id.to_string(),
            updated_file.provider_file_id.to_string(),
            updated_file.path.as_str(),
            updated_file.name,
            updated_file.size.map(|s| s as i64),
            updated_file.content_hash,
            updated_file.is_folder as i32,
            match updated_file.state {
                FileState::Synced => "synced",
                FileState::CloudOnly => "cloud_only",
                FileState::Syncing => "syncing",
                FileState::Pending => "pending",
                FileState::Error => "error",
                FileState::Excluded => "excluded",
                FileState::OfflineModified => "offline_modified",
                FileState::Conflict => "conflict",
            },
            updated_file.modified_at.timestamp(),
            updated_file.synced_at.map(|dt| dt.timestamp()),
            updated_file.updated_at.timestamp(),
        ],
    )?;

    Ok(rows_affected > 0)
}

/// Deletes a file by its local database ID.
///
/// # Returns
///
/// * `Ok(true)` - If the file was deleted
/// * `Ok(false)` - If no file with this ID exists
pub fn delete_file(conn: &Connection, id: i64) -> DbResult<bool> {
    let rows_affected = conn.execute("DELETE FROM files WHERE id = ?1", params![id])?;
    Ok(rows_affected > 0)
}

/// Deletes all files for a given account.
///
/// # Returns
///
/// * `Ok(usize)` - The number of files deleted
pub fn delete_files_by_account(conn: &Connection, account_id: &AccountId) -> DbResult<usize> {
    let rows_affected = conn.execute(
        "DELETE FROM files WHERE account_id = ?1",
        params![account_id.to_string()],
    )?;
    Ok(rows_affected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{accounts, ConnectionManager, Migration, Migrator, ACCOUNTS_MIGRATION};
    use cloudsync_core::types::ProviderId;

    fn setup_test_db() -> Connection {
        let manager = ConnectionManager::new(Default::default());
        let conn = manager.open().unwrap();

        // Apply migrations
        let migrator = Migrator::new(vec![
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
        ]);
        migrator.migrate(&conn).unwrap();

        conn
    }

    fn create_test_account(conn: &Connection) -> AccountId {
        let account = accounts::Account::new(
            ProviderId::GoogleDrive,
            "test@example.com".to_string(),
            "access_token".to_string(),
            None,
            None,
        );
        accounts::create_account(conn, &account).unwrap();
        account.id
    }

    fn create_test_file(account_id: AccountId) -> File {
        File::new(
            account_id,
            FileId::new("provider_file_123"),
            CloudPath::new("/documents/test.pdf"),
            "test.pdf".to_string(),
            Some(1024),
            Some("abc123hash".to_string()),
            false,
            Utc::now(),
        )
    }

    #[test]
    fn test_create_file_success() {
        let conn = setup_test_db();
        let account_id = create_test_account(&conn);
        let file = create_test_file(account_id);

        let result = create_file(&conn, &file);
        assert!(result.is_ok());
        assert!(result.unwrap() > 0);
    }

    #[test]
    fn test_create_file_duplicate_provider_id() {
        let conn = setup_test_db();
        let account_id = create_test_account(&conn);
        let file = create_test_file(account_id.clone());

        // First insert should succeed
        create_file(&conn, &file).unwrap();

        // Second insert with same account and provider_file_id should fail
        let duplicate = File::new(
            account_id,
            FileId::new("provider_file_123"),
            CloudPath::new("/different/path.pdf"),
            "different.pdf".to_string(),
            Some(2048),
            None,
            false,
            Utc::now(),
        );

        let result = create_file(&conn, &duplicate);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), DbError::Conflict(_)));
    }

    #[test]
    fn test_create_file_different_account_same_provider_id() {
        let conn = setup_test_db();
        let account_id1 = create_test_account(&conn);
        let file1 = create_test_file(account_id1);

        create_file(&conn, &file1).unwrap();

        // Create second account
        let account2 = accounts::Account::new(
            ProviderId::Dropbox,
            "other@example.com".to_string(),
            "token2".to_string(),
            None,
            None,
        );
        let account_id2 = accounts::create_account(&conn, &account2).unwrap();

        // Same provider_file_id but different account should succeed
        let file2 = File::new(
            account_id2,
            FileId::new("provider_file_123"),
            CloudPath::new("/documents/test.pdf"),
            "test.pdf".to_string(),
            Some(1024),
            None,
            false,
            Utc::now(),
        );

        let result = create_file(&conn, &file2);
        assert!(result.is_ok());
    }

    #[test]
    fn test_get_file_by_id_found() {
        let conn = setup_test_db();
        let account_id = create_test_account(&conn);
        let file = create_test_file(account_id);

        let file_id = create_file(&conn, &file).unwrap();

        let result = get_file_by_id(&conn, file_id).unwrap();
        assert!(result.is_some());

        let retrieved = result.unwrap();
        assert_eq!(retrieved.id, file_id);
        assert_eq!(retrieved.account_id, file.account_id);
        assert_eq!(retrieved.provider_file_id, file.provider_file_id);
        assert_eq!(retrieved.path, file.path);
        assert_eq!(retrieved.name, file.name);
        assert_eq!(retrieved.size, file.size);
        assert_eq!(retrieved.content_hash, file.content_hash);
        assert_eq!(retrieved.is_folder, file.is_folder);
        assert_eq!(retrieved.state, FileState::Pending);
    }

    #[test]
    fn test_get_file_by_id_not_found() {
        let conn = setup_test_db();

        let result = get_file_by_id(&conn, 99999).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_file_by_provider_id_found() {
        let conn = setup_test_db();
        let account_id = create_test_account(&conn);
        let file = create_test_file(account_id.clone());

        create_file(&conn, &file).unwrap();

        let result =
            get_file_by_provider_id(&conn, &account_id, &FileId::new("provider_file_123")).unwrap();
        assert!(result.is_some());

        let retrieved = result.unwrap();
        assert_eq!(retrieved.provider_file_id, file.provider_file_id);
    }

    #[test]
    fn test_get_file_by_provider_id_not_found() {
        let conn = setup_test_db();
        let account_id = create_test_account(&conn);

        let result =
            get_file_by_provider_id(&conn, &account_id, &FileId::new("nonexistent")).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_file_by_cloud_path_found() {
        let conn = setup_test_db();
        let account_id = create_test_account(&conn);
        let file = create_test_file(account_id.clone());

        create_file(&conn, &file).unwrap();

        let result = get_file_by_cloud_path(
            &conn,
            &account_id,
            &CloudPath::new("/documents/test.pdf"),
        )
        .unwrap();
        assert!(result.is_some());

        let retrieved = result.unwrap();
        assert_eq!(retrieved.path, file.path);
        assert_eq!(retrieved.name, "test.pdf");
    }

    #[test]
    fn test_get_file_by_cloud_path_not_found() {
        let conn = setup_test_db();
        let account_id = create_test_account(&conn);

        let result = get_file_by_cloud_path(
            &conn,
            &account_id,
            &CloudPath::new("/nonexistent/file.txt"),
        )
        .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_file_by_cloud_path_wrong_account() {
        let conn = setup_test_db();
        let account_id = create_test_account(&conn);
        let file = create_test_file(account_id.clone());
        create_file(&conn, &file).unwrap();

        let other_account = AccountId::from_string("other-account-id");
        let result = get_file_by_cloud_path(
            &conn,
            &other_account,
            &CloudPath::new("/documents/test.pdf"),
        )
        .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_list_files_empty() {
        let conn = setup_test_db();
        let account_id = create_test_account(&conn);

        let files = list_files(&conn, &account_id, None).unwrap();
        assert_eq!(files.len(), 0);
    }

    #[test]
    fn test_list_files_multiple() {
        let conn = setup_test_db();
        let account_id = create_test_account(&conn);

        let file1 = create_test_file(account_id.clone());
        let file2 = File::new(
            account_id.clone(),
            FileId::new("provider_file_456"),
            CloudPath::new("/images/photo.jpg"),
            "photo.jpg".to_string(),
            Some(2048),
            None,
            false,
            Utc::now(),
        );

        create_file(&conn, &file1).unwrap();
        create_file(&conn, &file2).unwrap();

        let files = list_files(&conn, &account_id, None).unwrap();
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_list_files_with_state_filter() {
        let conn = setup_test_db();
        let account_id = create_test_account(&conn);

        let mut file1 = create_test_file(account_id.clone());
        file1.state = FileState::Synced;

        let file2 = File::new(
            account_id.clone(),
            FileId::new("provider_file_456"),
            CloudPath::new("/images/photo.jpg"),
            "photo.jpg".to_string(),
            Some(2048),
            None,
            false,
            Utc::now(),
        );
        // file2 defaults to Pending state

        let id1 = create_file(&conn, &file1).unwrap();
        create_file(&conn, &file2).unwrap();

        // Update file1 to synced state
        let mut file1_updated = get_file_by_id(&conn, id1).unwrap().unwrap();
        file1_updated.state = FileState::Synced;
        update_file(&conn, &file1_updated).unwrap();

        // Filter by Synced state
        let synced_files = list_files(&conn, &account_id, Some(FileState::Synced)).unwrap();
        assert_eq!(synced_files.len(), 1);
        assert_eq!(synced_files[0].state, FileState::Synced);

        // Filter by Pending state
        let pending_files = list_files(&conn, &account_id, Some(FileState::Pending)).unwrap();
        assert_eq!(pending_files.len(), 1);
        assert_eq!(pending_files[0].state, FileState::Pending);
    }

    #[test]
    fn test_list_files_sorted_by_path() {
        let conn = setup_test_db();
        let account_id = create_test_account(&conn);

        let file1 = File::new(
            account_id.clone(),
            FileId::new("file1"),
            CloudPath::new("/z_last.txt"),
            "z_last.txt".to_string(),
            Some(100),
            None,
            false,
            Utc::now(),
        );
        let file2 = File::new(
            account_id.clone(),
            FileId::new("file2"),
            CloudPath::new("/a_first.txt"),
            "a_first.txt".to_string(),
            Some(100),
            None,
            false,
            Utc::now(),
        );

        create_file(&conn, &file1).unwrap();
        create_file(&conn, &file2).unwrap();

        let files = list_files(&conn, &account_id, None).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path.as_str(), "/a_first.txt");
        assert_eq!(files[1].path.as_str(), "/z_last.txt");
    }

    #[test]
    fn test_update_file_success() {
        let conn = setup_test_db();
        let account_id = create_test_account(&conn);
        let file = create_test_file(account_id);

        let file_id = create_file(&conn, &file).unwrap();

        // Update the file
        let mut updated_file = get_file_by_id(&conn, file_id).unwrap().unwrap();
        updated_file.name = "updated.pdf".to_string();
        updated_file.size = Some(4096);
        updated_file.state = FileState::Synced;
        updated_file.synced_at = Some(Utc::now());

        let result = update_file(&conn, &updated_file).unwrap();
        assert!(result);

        // Verify the update
        let retrieved = get_file_by_id(&conn, file_id).unwrap().unwrap();
        assert_eq!(retrieved.name, "updated.pdf");
        assert_eq!(retrieved.size, Some(4096));
        assert_eq!(retrieved.state, FileState::Synced);
        assert!(retrieved.synced_at.is_some());
    }

    #[test]
    fn test_update_file_not_found() {
        let conn = setup_test_db();
        let account_id = create_test_account(&conn);
        let mut file = create_test_file(account_id);
        file.id = 99999;

        let result = update_file(&conn, &file).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_update_file_updates_timestamp() {
        let conn = setup_test_db();
        let account_id = create_test_account(&conn);
        let file = create_test_file(account_id);

        let file_id = create_file(&conn, &file).unwrap();
        let original = get_file_by_id(&conn, file_id).unwrap().unwrap();
        let original_updated_at = original.updated_at;

        // Wait to ensure timestamp changes
        std::thread::sleep(std::time::Duration::from_secs(1));

        let mut updated = original.clone();
        updated.name = "changed.pdf".to_string();
        update_file(&conn, &updated).unwrap();

        let retrieved = get_file_by_id(&conn, file_id).unwrap().unwrap();
        assert!(retrieved.updated_at > original_updated_at);
    }

    #[test]
    fn test_delete_file_success() {
        let conn = setup_test_db();
        let account_id = create_test_account(&conn);
        let file = create_test_file(account_id);

        let file_id = create_file(&conn, &file).unwrap();

        let result = delete_file(&conn, file_id).unwrap();
        assert!(result);

        // Verify deletion
        let retrieved = get_file_by_id(&conn, file_id).unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_delete_file_not_found() {
        let conn = setup_test_db();

        let result = delete_file(&conn, 99999).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_delete_files_by_account() {
        let conn = setup_test_db();
        let account_id = create_test_account(&conn);

        let file1 = create_test_file(account_id.clone());
        let file2 = File::new(
            account_id.clone(),
            FileId::new("file2"),
            CloudPath::new("/other.txt"),
            "other.txt".to_string(),
            Some(512),
            None,
            false,
            Utc::now(),
        );

        create_file(&conn, &file1).unwrap();
        create_file(&conn, &file2).unwrap();

        let deleted = delete_files_by_account(&conn, &account_id).unwrap();
        assert_eq!(deleted, 2);

        let files = list_files(&conn, &account_id, None).unwrap();
        assert_eq!(files.len(), 0);
    }

    #[test]
    fn test_file_new_sets_defaults() {
        let account_id = AccountId::new();
        let file = create_test_file(account_id);

        assert_eq!(file.id, 0);
        assert_eq!(file.state, FileState::Pending);
        assert!(file.synced_at.is_none());
        let now = Utc::now();
        assert!(file.created_at <= now);
        assert!(file.updated_at <= now);
    }

    #[test]
    fn test_folder_support() {
        let conn = setup_test_db();
        let account_id = create_test_account(&conn);

        let folder = File::new(
            account_id.clone(),
            FileId::new("folder_123"),
            CloudPath::new("/documents"),
            "documents".to_string(),
            None,
            None,
            true,
            Utc::now(),
        );

        let folder_id = create_file(&conn, &folder).unwrap();
        let retrieved = get_file_by_id(&conn, folder_id).unwrap().unwrap();

        assert!(retrieved.is_folder);
        assert!(retrieved.size.is_none());
    }

    #[test]
    fn test_all_file_states_supported() {
        let conn = setup_test_db();
        let account_id = create_test_account(&conn);

        let states = [
            FileState::Synced,
            FileState::CloudOnly,
            FileState::Syncing,
            FileState::Pending,
            FileState::Error,
            FileState::Excluded,
            FileState::OfflineModified,
            FileState::Conflict,
        ];

        for (i, state) in states.iter().enumerate() {
            let mut file = File::new(
                account_id.clone(),
                FileId::new(format!("file_{}", i)),
                CloudPath::new(format!("/file_{}.txt", i)),
                format!("file_{}.txt", i),
                Some(100),
                None,
                false,
                Utc::now(),
            );
            file.state = *state;

            let file_id = create_file(&conn, &file).unwrap();
            let retrieved = get_file_by_id(&conn, file_id).unwrap().unwrap();
            assert_eq!(retrieved.state, *state);
        }
    }

    #[test]
    fn test_cascade_delete_on_account_deletion() {
        let conn = setup_test_db();
        let account_id = create_test_account(&conn);
        let file = create_test_file(account_id.clone());

        create_file(&conn, &file).unwrap();

        // Verify file exists
        let files_before = list_files(&conn, &account_id, None).unwrap();
        assert_eq!(files_before.len(), 1);

        // Delete the account
        accounts::delete_account(&conn, &account_id).unwrap();

        // Verify files were cascade deleted
        let files_after = list_files(&conn, &account_id, None).unwrap();
        assert_eq!(files_after.len(), 0);
    }

    #[test]
    fn test_content_hash_optional() {
        let conn = setup_test_db();
        let account_id = create_test_account(&conn);

        let file_with_hash = File::new(
            account_id.clone(),
            FileId::new("file1"),
            CloudPath::new("/with_hash.txt"),
            "with_hash.txt".to_string(),
            Some(100),
            Some("sha256hash".to_string()),
            false,
            Utc::now(),
        );

        let file_without_hash = File::new(
            account_id.clone(),
            FileId::new("file2"),
            CloudPath::new("/without_hash.txt"),
            "without_hash.txt".to_string(),
            Some(100),
            None,
            false,
            Utc::now(),
        );

        let id1 = create_file(&conn, &file_with_hash).unwrap();
        let id2 = create_file(&conn, &file_without_hash).unwrap();

        let retrieved1 = get_file_by_id(&conn, id1).unwrap().unwrap();
        assert!(retrieved1.content_hash.is_some());

        let retrieved2 = get_file_by_id(&conn, id2).unwrap().unwrap();
        assert!(retrieved2.content_hash.is_none());
    }

    #[test]
    fn test_synced_at_optional() {
        let conn = setup_test_db();
        let account_id = create_test_account(&conn);
        let file = create_test_file(account_id);

        let file_id = create_file(&conn, &file).unwrap();
        let retrieved = get_file_by_id(&conn, file_id).unwrap().unwrap();

        // Initially, synced_at should be None
        assert!(retrieved.synced_at.is_none());

        // Update with synced_at
        let mut updated = retrieved.clone();
        updated.synced_at = Some(Utc::now());
        update_file(&conn, &updated).unwrap();

        let retrieved_again = get_file_by_id(&conn, file_id).unwrap().unwrap();
        assert!(retrieved_again.synced_at.is_some());
    }
}
