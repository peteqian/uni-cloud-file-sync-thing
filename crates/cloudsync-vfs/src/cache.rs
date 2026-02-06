//! Metadata cache for the virtual filesystem
//!
//! Provides file metadata (names, sizes, timestamps) by querying the SQLite
//! `files` table directly. No in-memory caching — SQLite WAL handles concurrent
//! reads efficiently.

use anyhow::Result;
use chrono::{DateTime, Utc};
use cloudsync_core::file_state::FileState;
use cloudsync_core::types::{AccountId, FileId};
use cloudsync_db::Database;

/// File state in the VFS layer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheState {
    /// File metadata exists but content not downloaded
    CloudOnly,
    /// File is currently being downloaded
    Downloading,
    /// File content is cached locally
    Cached,
    /// File has been modified locally
    Modified,
}

/// Cached file entry — the VFS layer's view of file data
#[derive(Debug, Clone)]
pub struct CachedFile {
    pub file_id: FileId,
    pub name: String,
    pub path: String,
    pub size: u64,
    pub is_directory: bool,
    pub modified_time: DateTime<Utc>,
    pub state: CacheState,
}

/// Metadata cache backed by the `files` table in SQLite
#[derive(Clone)]
pub struct MetadataCache {
    db: Database,
    account_id: AccountId,
}

impl MetadataCache {
    /// Create a new metadata cache for the given account
    pub fn new(db: Database, account_id: AccountId) -> Self {
        Self { db, account_id }
    }

    /// Get a file by its provider file ID
    pub fn get(&self, file_id: &FileId) -> Result<Option<CachedFile>> {
        Ok(self.db.with_conn(|conn| {
            let file = cloudsync_db::get_file_by_provider_id(conn, &self.account_id, file_id)?;
            Ok(file.map(|f| db_file_to_cached_file(&f)))
        })?)
    }

    /// Get children of a directory by its path.
    ///
    /// Children are files whose path starts with `parent_path/` (one level deep).
    pub fn get_children(&self, parent_path: &str) -> Result<Vec<CachedFile>> {
        let account_id = self.account_id.clone();
        let parent = parent_path.to_string();

        Ok(self.db.with_conn(move |conn| {
            let all_files = cloudsync_db::list_files(conn, &account_id, None)?;

            let prefix = if parent == "/" {
                "/".to_string()
            } else {
                format!("{}/", parent.trim_end_matches('/'))
            };

            let children: Vec<CachedFile> = all_files
                .into_iter()
                .filter(|f| {
                    let path = f.path.as_str();
                    if !path.starts_with(&prefix) {
                        return false;
                    }
                    // Only direct children (no further '/' after prefix)
                    let remainder = &path[prefix.len()..];
                    !remainder.contains('/')
                })
                .map(|f| db_file_to_cached_file(&f))
                .collect();

            Ok(children)
        })?)
    }
}

/// Convert a database File to a VFS CachedFile
fn db_file_to_cached_file(file: &cloudsync_db::File) -> CachedFile {
    CachedFile {
        file_id: file.provider_file_id.clone(),
        name: file.name.clone(),
        path: file.path.as_str().to_string(),
        size: file.size.unwrap_or(0),
        is_directory: file.is_folder,
        modified_time: file.modified_at,
        state: file_state_to_cache_state(&file.state),
    }
}

/// Map database FileState to VFS CacheState
fn file_state_to_cache_state(state: &FileState) -> CacheState {
    match state {
        FileState::Synced => CacheState::Cached,
        FileState::Syncing => CacheState::Downloading,
        FileState::OfflineModified => CacheState::Modified,
        _ => CacheState::CloudOnly,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use cloudsync_core::types::{CloudPath, ProviderId};
    use cloudsync_db::{
        accounts, create_file, Migration, ACCOUNTS_MIGRATION, FILES_MIGRATION, VFS_INODES_MIGRATION,
    };

    fn setup_test_db() -> (Database, AccountId) {
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
        ])
        .unwrap();

        let account_id = db
            .with_conn(|conn| {
                let account = accounts::Account::new(
                    ProviderId::GoogleDrive,
                    "test@example.com".to_string(),
                    "token".to_string(),
                    None,
                    None,
                );
                accounts::create_account(conn, &account)?;
                Ok(account.id)
            })
            .unwrap();

        (db, account_id)
    }

    fn insert_file(
        db: &Database,
        account_id: &AccountId,
        provider_id: &str,
        path: &str,
        name: &str,
        is_folder: bool,
        size: Option<u64>,
    ) {
        db.with_conn(|conn| {
            let file = cloudsync_db::File::new(
                account_id.clone(),
                FileId::new(provider_id),
                CloudPath::new(path),
                name.to_string(),
                size,
                None,
                is_folder,
                Utc::now(),
            );
            create_file(conn, &file)?;
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_get_returns_none_for_empty_db() {
        let (db, account_id) = setup_test_db();
        let cache = MetadataCache::new(db, account_id);

        let result = cache.get(&FileId::new("nonexistent")).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_returns_cached_file() {
        let (db, account_id) = setup_test_db();
        insert_file(
            &db,
            &account_id,
            "f1",
            "/docs/readme.md",
            "readme.md",
            false,
            Some(256),
        );

        let cache = MetadataCache::new(db, account_id);
        let file = cache.get(&FileId::new("f1")).unwrap().unwrap();

        assert_eq!(file.file_id, FileId::new("f1"));
        assert_eq!(file.name, "readme.md");
        assert_eq!(file.size, 256);
        assert!(!file.is_directory);
    }

    #[test]
    fn test_get_folder() {
        let (db, account_id) = setup_test_db();
        insert_file(&db, &account_id, "d1", "/photos", "photos", true, None);

        let cache = MetadataCache::new(db, account_id);
        let folder = cache.get(&FileId::new("d1")).unwrap().unwrap();

        assert!(folder.is_directory);
        assert_eq!(folder.size, 0);
        assert_eq!(folder.name, "photos");
    }

    #[test]
    fn test_get_children_of_root() {
        let (db, account_id) = setup_test_db();

        insert_file(
            &db,
            &account_id,
            "f1",
            "/hello.txt",
            "hello.txt",
            false,
            Some(100),
        );
        insert_file(&db, &account_id, "d1", "/photos", "photos", true, None);
        // Nested file should NOT appear as root child
        insert_file(
            &db,
            &account_id,
            "f2",
            "/photos/pic.jpg",
            "pic.jpg",
            false,
            Some(2048),
        );

        let cache = MetadataCache::new(db, account_id);
        let children = cache.get_children("/").unwrap();

        assert_eq!(children.len(), 2);
        let names: Vec<&str> = children.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"hello.txt"));
        assert!(names.contains(&"photos"));
    }

    #[test]
    fn test_get_children_of_subdirectory() {
        let (db, account_id) = setup_test_db();

        insert_file(&db, &account_id, "d1", "/photos", "photos", true, None);
        insert_file(
            &db,
            &account_id,
            "f1",
            "/photos/pic.jpg",
            "pic.jpg",
            false,
            Some(2048),
        );
        insert_file(
            &db,
            &account_id,
            "f2",
            "/photos/video.mp4",
            "video.mp4",
            false,
            Some(4096),
        );
        // Deeply nested should NOT appear
        insert_file(
            &db,
            &account_id,
            "f3",
            "/photos/vacation/beach.jpg",
            "beach.jpg",
            false,
            Some(1024),
        );

        let cache = MetadataCache::new(db, account_id);
        let children = cache.get_children("/photos").unwrap();

        assert_eq!(children.len(), 2);
        let names: Vec<&str> = children.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"pic.jpg"));
        assert!(names.contains(&"video.mp4"));
    }

    #[test]
    fn test_get_children_empty_directory() {
        let (db, account_id) = setup_test_db();
        insert_file(&db, &account_id, "d1", "/empty", "empty", true, None);

        let cache = MetadataCache::new(db, account_id);
        let children = cache.get_children("/empty").unwrap();

        assert!(children.is_empty());
    }

    #[test]
    fn test_state_mapping_synced() {
        let (db, account_id) = setup_test_db();

        db.with_conn(|conn| {
            let mut file = cloudsync_db::File::new(
                account_id.clone(),
                FileId::new("synced_file"),
                CloudPath::new("/synced.txt"),
                "synced.txt".to_string(),
                Some(100),
                None,
                false,
                Utc::now(),
            );
            file.state = FileState::Synced;
            create_file(conn, &file)?;
            Ok(())
        })
        .unwrap();

        let cache = MetadataCache::new(db, account_id);
        let file = cache.get(&FileId::new("synced_file")).unwrap().unwrap();
        assert_eq!(file.state, CacheState::Cached);
    }

    #[test]
    fn test_state_mapping_syncing() {
        let (db, account_id) = setup_test_db();

        db.with_conn(|conn| {
            let mut file = cloudsync_db::File::new(
                account_id.clone(),
                FileId::new("syncing_file"),
                CloudPath::new("/syncing.txt"),
                "syncing.txt".to_string(),
                Some(100),
                None,
                false,
                Utc::now(),
            );
            file.state = FileState::Syncing;
            create_file(conn, &file)?;
            Ok(())
        })
        .unwrap();

        let cache = MetadataCache::new(db, account_id);
        let file = cache.get(&FileId::new("syncing_file")).unwrap().unwrap();
        assert_eq!(file.state, CacheState::Downloading);
    }

    #[test]
    fn test_state_mapping_cloud_only() {
        let (db, account_id) = setup_test_db();

        db.with_conn(|conn| {
            let mut file = cloudsync_db::File::new(
                account_id.clone(),
                FileId::new("cloud_file"),
                CloudPath::new("/cloud.txt"),
                "cloud.txt".to_string(),
                Some(100),
                None,
                false,
                Utc::now(),
            );
            file.state = FileState::CloudOnly;
            create_file(conn, &file)?;
            Ok(())
        })
        .unwrap();

        let cache = MetadataCache::new(db, account_id);
        let file = cache.get(&FileId::new("cloud_file")).unwrap().unwrap();
        assert_eq!(file.state, CacheState::CloudOnly);
    }
}
