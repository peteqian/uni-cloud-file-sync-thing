//! Inode management for the virtual filesystem
//!
//! Maps cloud file IDs to filesystem inodes (u64). FUSE requires stable inode
//! numbers that persist across mounts. Backed by SQLite for persistence.

use anyhow::Result;
use cloudsync_core::FileId;
use cloudsync_db::Database;
use std::sync::{Arc, Mutex};

/// FUSE root inode (required to be 1)
pub const FUSE_ROOT_INODE: u64 = 1;

/// Special FileId for the root directory
const ROOT_FILE_ID: &str = "root";

/// Manages allocation and mapping of inodes backed by SQLite.
///
/// The only in-memory state is the `next_inode` counter, which is restored
/// from `MAX(inode)` on startup. All mappings live in the database.
#[derive(Clone)]
pub struct InodeManager {
    db: Database,
    next_inode: Arc<Mutex<u64>>,
}

impl InodeManager {
    /// Create a new inode manager backed by the given database.
    ///
    /// Restores the next_inode counter from the database and ensures
    /// the root inode mapping exists.
    pub fn new(db: Database) -> Result<Self> {
        let max_inode = db.with_conn(cloudsync_db::vfs_inodes::get_max_inode)?;

        let next_inode = if max_inode == 0 {
            FUSE_ROOT_INODE + 1
        } else {
            max_inode + 1
        };

        let manager = Self {
            db,
            next_inode: Arc::new(Mutex::new(next_inode)),
        };

        // Ensure root inode exists in DB
        manager.db.with_conn(|conn| {
            cloudsync_db::vfs_inodes::get_or_insert_inode(conn, ROOT_FILE_ID, || FUSE_ROOT_INODE)
        })?;

        Ok(manager)
    }

    /// Get or allocate an inode for a file ID
    pub fn get_or_allocate(&self, file_id: &FileId) -> Result<u64> {
        let file_id_str = file_id.to_string();
        let next_inode = Arc::clone(&self.next_inode);

        Ok(self.db.with_conn(|conn| {
            let inode = cloudsync_db::vfs_inodes::get_or_insert_inode(conn, &file_id_str, || {
                let mut counter = next_inode.lock().unwrap();
                let inode = *counter;
                *counter += 1;
                inode
            })?;
            Ok(inode)
        })?)
    }

    /// Look up file ID by inode
    pub fn get_file_id(&self, inode: u64) -> Result<Option<FileId>> {
        Ok(self.db.with_conn(|conn| {
            let file_id = cloudsync_db::vfs_inodes::get_file_id_by_inode(conn, inode)?;
            Ok(file_id.map(FileId::new))
        })?)
    }

    /// Look up inode by file ID
    pub fn get_inode(&self, file_id: &FileId) -> Result<Option<u64>> {
        Ok(self.db.with_conn(|conn| {
            cloudsync_db::vfs_inodes::get_inode_by_file_id(conn, &file_id.to_string())
        })?)
    }

    /// Remove a file from the inode map (when deleted)
    pub fn remove(&self, file_id: &FileId) -> Result<()> {
        Ok(self.db.with_conn(|conn| {
            cloudsync_db::vfs_inodes::remove_inode(conn, &file_id.to_string())?;
            Ok(())
        })?)
    }

    /// Get the root inode
    pub fn root_inode() -> u64 {
        FUSE_ROOT_INODE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cloudsync_db::{Migration, VFS_INODES_MIGRATION};

    fn create_test_db() -> Database {
        Database::in_memory_with_migrations(vec![Migration {
            version: 1,
            description: "Create vfs_inodes table",
            sql: VFS_INODES_MIGRATION,
        }])
        .unwrap()
    }

    #[test]
    fn test_root_inode_exists_after_init() {
        let db = create_test_db();
        let manager = InodeManager::new(db).unwrap();

        let root_id = FileId::new("root");
        assert_eq!(manager.get_inode(&root_id).unwrap(), Some(FUSE_ROOT_INODE));
        assert_eq!(manager.get_file_id(FUSE_ROOT_INODE).unwrap(), Some(root_id));
    }

    #[test]
    fn test_inode_allocation() {
        let db = create_test_db();
        let manager = InodeManager::new(db).unwrap();

        let file1 = FileId::new("file1");
        let file2 = FileId::new("file2");

        let inode1 = manager.get_or_allocate(&file1).unwrap();
        let inode2 = manager.get_or_allocate(&file2).unwrap();

        assert_ne!(inode1, inode2);
        assert!(inode1 > FUSE_ROOT_INODE);
        assert!(inode2 > FUSE_ROOT_INODE);

        assert_eq!(manager.get_file_id(inode1).unwrap(), Some(file1.clone()));
        assert_eq!(manager.get_inode(&file1).unwrap(), Some(inode1));
    }

    #[test]
    fn test_inode_stability() {
        let db = create_test_db();
        let manager = InodeManager::new(db).unwrap();
        let file = FileId::new("stable");

        let inode1 = manager.get_or_allocate(&file).unwrap();
        let inode2 = manager.get_or_allocate(&file).unwrap();

        assert_eq!(inode1, inode2);
    }

    #[test]
    fn test_stability_across_restarts() {
        let db = create_test_db();

        let file = FileId::new("persistent_file");
        let original_inode;

        // First "session"
        {
            let manager = InodeManager::new(db.clone()).unwrap();
            original_inode = manager.get_or_allocate(&file).unwrap();
        }

        // Second "session" — simulates daemon restart
        {
            let manager = InodeManager::new(db).unwrap();
            let restored_inode = manager.get_inode(&file).unwrap();
            assert_eq!(restored_inode, Some(original_inode));
        }
    }

    #[test]
    fn test_counter_restored_on_restart() {
        let db = create_test_db();

        // First session: allocate some inodes
        let last_inode;
        {
            let manager = InodeManager::new(db.clone()).unwrap();
            manager.get_or_allocate(&FileId::new("a")).unwrap();
            last_inode = manager.get_or_allocate(&FileId::new("b")).unwrap();
        }

        // Second session: new allocation should not collide
        {
            let manager = InodeManager::new(db).unwrap();
            let new_inode = manager.get_or_allocate(&FileId::new("c")).unwrap();
            assert!(new_inode > last_inode);
        }
    }

    #[test]
    fn test_remove_inode() {
        let db = create_test_db();
        let manager = InodeManager::new(db).unwrap();
        let file = FileId::new("to-remove");

        let inode = manager.get_or_allocate(&file).unwrap();
        assert_eq!(manager.get_inode(&file).unwrap(), Some(inode));

        manager.remove(&file).unwrap();

        assert_eq!(manager.get_inode(&file).unwrap(), None);
        assert_eq!(manager.get_file_id(inode).unwrap(), None);
    }

    #[test]
    fn test_remove_nonexistent_is_noop() {
        let db = create_test_db();
        let manager = InodeManager::new(db).unwrap();
        manager.remove(&FileId::new("ghost")).unwrap();

        // Root should still be intact
        assert_eq!(
            manager.get_file_id(FUSE_ROOT_INODE).unwrap(),
            Some(FileId::new("root"))
        );
    }

    #[test]
    fn test_get_file_id_nonexistent_inode() {
        let db = create_test_db();
        let manager = InodeManager::new(db).unwrap();
        assert!(manager.get_file_id(9999).unwrap().is_none());
    }

    #[test]
    fn test_inodes_are_sequential() {
        let db = create_test_db();
        let manager = InodeManager::new(db).unwrap();

        let inode_a = manager.get_or_allocate(&FileId::new("a")).unwrap();
        let inode_b = manager.get_or_allocate(&FileId::new("b")).unwrap();
        let inode_c = manager.get_or_allocate(&FileId::new("c")).unwrap();

        assert_eq!(inode_b, inode_a + 1);
        assert_eq!(inode_c, inode_b + 1);
    }

    #[test]
    fn test_removed_inode_not_reused() {
        let db = create_test_db();
        let manager = InodeManager::new(db).unwrap();

        let inode_a = manager.get_or_allocate(&FileId::new("a")).unwrap();
        manager.remove(&FileId::new("a")).unwrap();

        let inode_b = manager.get_or_allocate(&FileId::new("b")).unwrap();
        assert_ne!(inode_a, inode_b);
        assert!(inode_b > inode_a);
    }

    #[test]
    fn test_re_allocate_after_remove() {
        let db = create_test_db();
        let manager = InodeManager::new(db).unwrap();

        let original_inode = manager.get_or_allocate(&FileId::new("file")).unwrap();
        manager.remove(&FileId::new("file")).unwrap();

        let new_inode = manager.get_or_allocate(&FileId::new("file")).unwrap();
        assert_ne!(original_inode, new_inode);
    }

    #[test]
    fn test_root_inode_constant() {
        assert_eq!(InodeManager::root_inode(), 1);
        assert_eq!(FUSE_ROOT_INODE, 1);
    }
}
