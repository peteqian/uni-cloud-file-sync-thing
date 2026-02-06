//! FUSE filesystem implementation
//!
//! Implements the `fuser::Filesystem` trait to provide a virtual filesystem
//! that exposes cloud files without downloading content until accessed.

use crate::cache::{CachedFile, MetadataCache};
use crate::inode::{InodeManager, FUSE_ROOT_INODE};
use anyhow::Result;
use cloudsync_core::types::AccountId;
use cloudsync_db::Database;
use fuser::{
    FileAttr, FileType, Filesystem, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, Request,
};
use std::ffi::OsStr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

const TTL: Duration = Duration::from_secs(1);

/// CloudSync FUSE filesystem
#[allow(dead_code)] // provider_id will be used in future issues
pub struct CloudSyncFS {
    provider_id: String,
    inode_manager: InodeManager,
    metadata_cache: MetadataCache,
}

impl CloudSyncFS {
    /// Create a new CloudSync filesystem backed by SQLite
    pub fn new(provider_id: &str, db: Database, account_id: AccountId) -> Result<Self> {
        info!("Initializing CloudSyncFS for provider: {}", provider_id);

        let inode_manager = InodeManager::new(db.clone())?;
        let metadata_cache = MetadataCache::new(db, account_id);

        Ok(Self {
            provider_id: provider_id.to_string(),
            inode_manager,
            metadata_cache,
        })
    }

    /// Convert cached file to FUSE file attributes
    fn cached_file_to_attr(&self, file: &CachedFile, inode: u64) -> FileAttr {
        let kind = if file.is_directory {
            FileType::Directory
        } else {
            FileType::RegularFile
        };

        // Convert chrono DateTime to SystemTime
        let mtime = UNIX_EPOCH + Duration::from_secs(file.modified_time.timestamp() as u64);

        FileAttr {
            ino: inode,
            size: file.size,
            blocks: file.size.div_ceil(512), // Standard 512-byte blocks
            atime: mtime,
            mtime,
            ctime: mtime,
            crtime: mtime,
            kind,
            perm: if file.is_directory { 0o755 } else { 0o644 },
            nlink: 1,
            uid: 1000, // TODO: use actual user ID
            gid: 1000, // TODO: use actual group ID
            rdev: 0,
            blksize: 4096,
            flags: 0,
        }
    }

    /// Get root directory attributes
    fn get_root_attr(&self) -> FileAttr {
        FileAttr {
            ino: FUSE_ROOT_INODE,
            size: 0,
            blocks: 0,
            atime: SystemTime::now(),
            mtime: SystemTime::now(),
            ctime: SystemTime::now(),
            crtime: SystemTime::now(),
            kind: FileType::Directory,
            perm: 0o755,
            nlink: 2,
            uid: 1000,
            gid: 1000,
            rdev: 0,
            blksize: 4096,
            flags: 0,
        }
    }
}

impl Filesystem for CloudSyncFS {
    /// Look up a file by name in a directory
    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        debug!("lookup(parent={}, name={:?})", parent, name);

        let name_str = match name.to_str() {
            Some(s) => s,
            None => {
                reply.error(libc::EINVAL);
                return;
            }
        };

        // Get parent file ID
        let parent_file_id = match self.inode_manager.get_file_id(parent) {
            Ok(Some(id)) => id,
            Ok(None) | Err(_) => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        // Get parent's path to find children
        let parent_path = match self.metadata_cache.get(&parent_file_id) {
            Ok(Some(f)) => f.path,
            _ => {
                // Root directory has no entry in files table
                if parent == FUSE_ROOT_INODE {
                    "/".to_string()
                } else {
                    reply.error(libc::ENOENT);
                    return;
                }
            }
        };

        // Search for file in parent's children
        let children = match self.metadata_cache.get_children(&parent_path) {
            Ok(c) => c,
            Err(_) => {
                reply.error(libc::EIO);
                return;
            }
        };

        for child in children {
            if child.name == name_str {
                let inode = match self.inode_manager.get_or_allocate(&child.file_id) {
                    Ok(i) => i,
                    Err(_) => {
                        reply.error(libc::EIO);
                        return;
                    }
                };
                let attr = self.cached_file_to_attr(&child, inode);
                reply.entry(&TTL, &attr, 0);
                return;
            }
        }

        reply.error(libc::ENOENT);
    }

    /// Get file attributes
    fn getattr(&mut self, _req: &Request, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
        debug!("getattr(ino={})", ino);

        // Handle root directory
        if ino == FUSE_ROOT_INODE {
            reply.attr(&TTL, &self.get_root_attr());
            return;
        }

        // Get file from cache
        let file_id = match self.inode_manager.get_file_id(ino) {
            Ok(Some(id)) => id,
            Ok(None) | Err(_) => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let file = match self.metadata_cache.get(&file_id) {
            Ok(Some(f)) => f,
            Ok(None) | Err(_) => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let attr = self.cached_file_to_attr(&file, ino);
        reply.attr(&TTL, &attr);
    }

    /// Read directory contents
    fn readdir(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        debug!("readdir(ino={}, offset={})", ino, offset);

        // Get directory file ID and path
        let dir_path = if ino == FUSE_ROOT_INODE {
            "/".to_string()
        } else {
            let dir_file_id = match self.inode_manager.get_file_id(ino) {
                Ok(Some(id)) => id,
                Ok(None) | Err(_) => {
                    reply.error(libc::ENOENT);
                    return;
                }
            };

            match self.metadata_cache.get(&dir_file_id) {
                Ok(Some(f)) => f.path,
                Ok(None) | Err(_) => {
                    reply.error(libc::ENOENT);
                    return;
                }
            }
        };

        // Add . and .. entries
        let mut entries = vec![
            (ino, FileType::Directory, ".".to_string()),
            (FUSE_ROOT_INODE, FileType::Directory, "..".to_string()),
        ];

        // Add children
        let children = match self.metadata_cache.get_children(&dir_path) {
            Ok(c) => c,
            Err(_) => {
                reply.error(libc::EIO);
                return;
            }
        };

        for child in children {
            let child_ino = match self.inode_manager.get_or_allocate(&child.file_id) {
                Ok(i) => i,
                Err(_) => continue,
            };
            let kind = if child.is_directory {
                FileType::Directory
            } else {
                FileType::RegularFile
            };
            entries.push((child_ino, kind, child.name));
        }

        // Return entries starting from offset
        for (i, (ino, kind, name)) in entries.into_iter().enumerate().skip(offset as usize) {
            if reply.add(ino, (i + 1) as i64, kind, name) {
                break;
            }
        }

        reply.ok();
    }

    /// Read file data
    fn read(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock: Option<u64>,
        reply: ReplyData,
    ) {
        debug!("read(ino={}, offset={}, size={})", ino, offset, size);

        // Get file from cache
        let file_id = match self.inode_manager.get_file_id(ino) {
            Ok(Some(id)) => id,
            Ok(None) | Err(_) => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let file = match self.metadata_cache.get(&file_id) {
            Ok(Some(f)) => f,
            Ok(None) | Err(_) => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        // Check if file is a directory
        if file.is_directory {
            reply.error(libc::EISDIR);
            return;
        }

        // TODO: Implement actual file download and caching
        warn!(
            "read() called but download not implemented yet for file: {}",
            file.name
        );

        reply.error(libc::ENOSYS); // Function not implemented
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::CacheState;
    use chrono::Utc;
    use cloudsync_core::types::{CloudPath, FileId, ProviderId};
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
    fn test_filesystem_creation() {
        let (db, account_id) = setup_test_db();
        let fs = CloudSyncFS::new("test-provider", db, account_id);
        assert!(fs.is_ok());
    }

    #[test]
    fn test_root_attr() {
        let (db, account_id) = setup_test_db();
        let fs = CloudSyncFS::new("test-provider", db, account_id).unwrap();
        let attr = fs.get_root_attr();

        assert_eq!(attr.ino, FUSE_ROOT_INODE);
        assert_eq!(attr.kind, FileType::Directory);
        assert_eq!(attr.perm, 0o755);
    }

    #[test]
    fn test_cached_file_to_attr() {
        let (db, account_id) = setup_test_db();
        let fs = CloudSyncFS::new("test-provider", db, account_id).unwrap();

        let file = CachedFile {
            file_id: FileId::new("test-file"),
            name: "test.txt".to_string(),
            path: "/test.txt".to_string(),
            size: 1024,
            modified_time: chrono::Utc::now(),
            is_directory: false,
            state: CacheState::CloudOnly,
        };

        let attr = fs.cached_file_to_attr(&file, 42);

        assert_eq!(attr.ino, 42);
        assert_eq!(attr.size, 1024);
        assert_eq!(attr.kind, FileType::RegularFile);
        assert_eq!(attr.perm, 0o644);
    }

    #[test]
    fn test_directory_to_attr() {
        let (db, account_id) = setup_test_db();
        let fs = CloudSyncFS::new("test-provider", db, account_id).unwrap();

        let dir = CachedFile {
            file_id: FileId::new("dir1"),
            name: "documents".to_string(),
            path: "/documents".to_string(),
            size: 0,
            modified_time: chrono::Utc::now(),
            is_directory: true,
            state: CacheState::CloudOnly,
        };

        let attr = fs.cached_file_to_attr(&dir, 10);

        assert_eq!(attr.ino, 10);
        assert_eq!(attr.kind, FileType::Directory);
        assert_eq!(attr.perm, 0o755);
        assert_eq!(attr.size, 0);
    }

    #[test]
    fn test_blocks_calculation() {
        let (db, account_id) = setup_test_db();
        let fs = CloudSyncFS::new("test-provider", db, account_id).unwrap();

        let file = CachedFile {
            file_id: FileId::new("f1"),
            name: "data.bin".to_string(),
            path: "/data.bin".to_string(),
            size: 1025, // Just over 2 blocks
            modified_time: chrono::Utc::now(),
            is_directory: false,
            state: CacheState::CloudOnly,
        };

        let attr = fs.cached_file_to_attr(&file, 5);
        // 1025 bytes / 512 = 2.002 -> ceil -> 3 blocks
        assert_eq!(attr.blocks, 3);
    }

    #[test]
    fn test_zero_size_file_blocks() {
        let (db, account_id) = setup_test_db();
        let fs = CloudSyncFS::new("test-provider", db, account_id).unwrap();

        let file = CachedFile {
            file_id: FileId::new("empty"),
            name: "empty.txt".to_string(),
            path: "/empty.txt".to_string(),
            size: 0,
            modified_time: chrono::Utc::now(),
            is_directory: false,
            state: CacheState::CloudOnly,
        };

        let attr = fs.cached_file_to_attr(&file, 6);
        assert_eq!(attr.blocks, 0);
        assert_eq!(attr.size, 0);
    }

    #[test]
    fn test_getattr_root_returns_directory() {
        let (db, account_id) = setup_test_db();
        let fs = CloudSyncFS::new("test-provider", db, account_id).unwrap();
        let attr = fs.get_root_attr();

        assert_eq!(attr.ino, FUSE_ROOT_INODE);
        assert_eq!(attr.kind, FileType::Directory);
        assert_eq!(attr.perm, 0o755);
        assert_eq!(attr.nlink, 2);
    }

    #[test]
    fn test_inode_and_cache_consistency() {
        let (db, account_id) = setup_test_db();

        insert_file(
            &db,
            &account_id,
            "file1",
            "/hello.txt",
            "hello.txt",
            false,
            Some(512),
        );
        insert_file(&db, &account_id, "dir1", "/photos", "photos", true, None);

        let fs = CloudSyncFS::new("test-provider", db, account_id).unwrap();

        // Root should be in inode manager
        let root_file_id = fs.inode_manager.get_file_id(FUSE_ROOT_INODE).unwrap();
        assert_eq!(root_file_id, Some(FileId::new("root")));

        // Allocate an inode for file1 and verify cache agrees
        let inode = fs
            .inode_manager
            .get_or_allocate(&FileId::new("file1"))
            .unwrap();
        let file_id = fs.inode_manager.get_file_id(inode).unwrap().unwrap();
        let cached = fs.metadata_cache.get(&file_id).unwrap().unwrap();
        assert_eq!(cached.name, "hello.txt");
        assert_eq!(cached.size, 512);
    }

    #[test]
    fn test_children_from_db() {
        let (db, account_id) = setup_test_db();

        insert_file(
            &db,
            &account_id,
            "file1",
            "/hello.txt",
            "hello.txt",
            false,
            Some(512),
        );
        insert_file(&db, &account_id, "dir1", "/photos", "photos", true, None);
        insert_file(
            &db,
            &account_id,
            "nested1",
            "/photos/pic.jpg",
            "pic.jpg",
            false,
            Some(2048),
        );

        let fs = CloudSyncFS::new("test-provider", db, account_id).unwrap();

        // Root children
        let root_children = fs.metadata_cache.get_children("/").unwrap();
        assert_eq!(root_children.len(), 2);
        let names: Vec<&str> = root_children.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"hello.txt"));
        assert!(names.contains(&"photos"));

        // Nested children
        let dir_children = fs.metadata_cache.get_children("/photos").unwrap();
        assert_eq!(dir_children.len(), 1);
        assert_eq!(dir_children[0].name, "pic.jpg");
    }

    #[test]
    fn test_inode_persistence_across_fs_instances() {
        let (db, account_id) = setup_test_db();
        insert_file(
            &db,
            &account_id,
            "file1",
            "/hello.txt",
            "hello.txt",
            false,
            Some(512),
        );

        let original_inode;
        {
            let fs = CloudSyncFS::new("test-provider", db.clone(), account_id.clone()).unwrap();
            original_inode = fs
                .inode_manager
                .get_or_allocate(&FileId::new("file1"))
                .unwrap();
        }

        // New FS instance should see the same inode
        {
            let fs = CloudSyncFS::new("test-provider", db, account_id).unwrap();
            let restored_inode = fs.inode_manager.get_inode(&FileId::new("file1")).unwrap();
            assert_eq!(restored_inode, Some(original_inode));
        }
    }

    #[test]
    fn test_attr_timestamps_from_modified_time() {
        let (db, account_id) = setup_test_db();
        let fs = CloudSyncFS::new("test-provider", db, account_id).unwrap();

        let fixed_time = chrono::DateTime::parse_from_rfc3339("2025-06-15T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        let file = CachedFile {
            file_id: FileId::new("f1"),
            name: "test.txt".to_string(),
            path: "/test.txt".to_string(),
            size: 100,
            modified_time: fixed_time,
            is_directory: false,
            state: CacheState::CloudOnly,
        };

        let attr = fs.cached_file_to_attr(&file, 7);

        let expected_system_time = UNIX_EPOCH + Duration::from_secs(fixed_time.timestamp() as u64);

        assert_eq!(attr.mtime, expected_system_time);
        assert_eq!(attr.atime, expected_system_time);
        assert_eq!(attr.ctime, expected_system_time);
        assert_eq!(attr.crtime, expected_system_time);
    }
}
