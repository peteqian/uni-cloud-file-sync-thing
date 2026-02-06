//! Inode management for the virtual filesystem
//!
//! Maps cloud file IDs to filesystem inodes (u64). FUSE requires stable inode
//! numbers that persist across mounts.

use cloudsync_core::FileId;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// FUSE root inode (required to be 1)
pub const FUSE_ROOT_INODE: u64 = 1;

/// Manages allocation and mapping of inodes
#[derive(Clone)]
pub struct InodeManager {
    inner: Arc<Mutex<InodeManagerInner>>,
}

struct InodeManagerInner {
    /// Next inode to allocate
    next_inode: u64,

    /// Map from cloud FileId to inode
    file_to_inode: HashMap<FileId, u64>,

    /// Map from inode to FileId
    inode_to_file: HashMap<u64, FileId>,
}

impl InodeManager {
    /// Create a new inode manager
    pub fn new() -> Self {
        let mut inner = InodeManagerInner {
            next_inode: FUSE_ROOT_INODE + 1, // Start after root
            file_to_inode: HashMap::new(),
            inode_to_file: HashMap::new(),
        };

        // Reserve root inode (it represents the mount point root directory)
        // We'll use a special FileId for root
        let root_file_id = FileId::new("root");
        inner
            .file_to_inode
            .insert(root_file_id.clone(), FUSE_ROOT_INODE);
        inner.inode_to_file.insert(FUSE_ROOT_INODE, root_file_id);

        Self {
            inner: Arc::new(Mutex::new(inner)),
        }
    }

    /// Get or allocate an inode for a file ID
    pub fn get_or_allocate(&self, file_id: &FileId) -> u64 {
        let mut inner = self.inner.lock().unwrap();

        if let Some(&inode) = inner.file_to_inode.get(file_id) {
            return inode;
        }

        // Allocate new inode
        let inode = inner.next_inode;
        inner.next_inode += 1;

        inner.file_to_inode.insert(file_id.clone(), inode);
        inner.inode_to_file.insert(inode, file_id.clone());

        inode
    }

    /// Look up file ID by inode
    pub fn get_file_id(&self, inode: u64) -> Option<FileId> {
        let inner = self.inner.lock().unwrap();
        inner.inode_to_file.get(&inode).cloned()
    }

    /// Look up inode by file ID
    #[allow(dead_code)] // Will be used in future issues
    pub fn get_inode(&self, file_id: &FileId) -> Option<u64> {
        let inner = self.inner.lock().unwrap();
        inner.file_to_inode.get(file_id).copied()
    }

    /// Remove a file from the inode map (when deleted)
    #[allow(dead_code)] // Will be used in future issues
    pub fn remove(&self, file_id: &FileId) {
        let mut inner = self.inner.lock().unwrap();

        if let Some(inode) = inner.file_to_inode.remove(file_id) {
            inner.inode_to_file.remove(&inode);
        }
    }

    /// Get the root inode
    #[allow(dead_code)] // Will be used in future issues
    pub fn root_inode() -> u64 {
        FUSE_ROOT_INODE
    }
}

impl Default for InodeManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_root_inode() {
        let manager = InodeManager::new();
        let root_id = FileId::new("root");

        assert_eq!(manager.get_inode(&root_id), Some(FUSE_ROOT_INODE));
        assert_eq!(manager.get_file_id(FUSE_ROOT_INODE), Some(root_id));
    }

    #[test]
    fn test_inode_allocation() {
        let manager = InodeManager::new();

        let file1 = FileId::new("file1");
        let file2 = FileId::new("file2");

        let inode1 = manager.get_or_allocate(&file1);
        let inode2 = manager.get_or_allocate(&file2);

        // Should get different inodes
        assert_ne!(inode1, inode2);
        assert!(inode1 > FUSE_ROOT_INODE);
        assert!(inode2 > FUSE_ROOT_INODE);

        // Should be able to look up both ways
        assert_eq!(manager.get_file_id(inode1), Some(file1.clone()));
        assert_eq!(manager.get_inode(&file1), Some(inode1));
    }

    #[test]
    fn test_inode_stability() {
        let manager = InodeManager::new();
        let file = FileId::new("stable");

        let inode1 = manager.get_or_allocate(&file);
        let inode2 = manager.get_or_allocate(&file);

        // Should get same inode on repeated calls
        assert_eq!(inode1, inode2);
    }

    #[test]
    fn test_remove_inode() {
        let manager = InodeManager::new();
        let file = FileId::new("to-remove");

        let inode = manager.get_or_allocate(&file);
        assert_eq!(manager.get_inode(&file), Some(inode));

        manager.remove(&file);

        assert_eq!(manager.get_inode(&file), None);
        assert_eq!(manager.get_file_id(inode), None);
    }

    #[test]
    fn test_remove_nonexistent_is_noop() {
        let manager = InodeManager::new();
        manager.remove(&FileId::new("ghost"));
        // Should not panic, root should still be intact
        assert_eq!(
            manager.get_file_id(FUSE_ROOT_INODE),
            Some(FileId::new("root"))
        );
    }

    #[test]
    fn test_get_file_id_nonexistent_inode() {
        let manager = InodeManager::new();
        assert!(manager.get_file_id(9999).is_none());
    }

    #[test]
    fn test_inodes_are_sequential() {
        let manager = InodeManager::new();

        let inode_a = manager.get_or_allocate(&FileId::new("a"));
        let inode_b = manager.get_or_allocate(&FileId::new("b"));
        let inode_c = manager.get_or_allocate(&FileId::new("c"));

        assert_eq!(inode_b, inode_a + 1);
        assert_eq!(inode_c, inode_b + 1);
    }

    #[test]
    fn test_removed_inode_not_reused() {
        let manager = InodeManager::new();

        let inode_a = manager.get_or_allocate(&FileId::new("a"));
        manager.remove(&FileId::new("a"));

        // New allocation should get a new inode, not reuse the old one
        let inode_b = manager.get_or_allocate(&FileId::new("b"));
        assert_ne!(inode_a, inode_b);
        assert!(inode_b > inode_a);
    }

    #[test]
    fn test_re_allocate_after_remove() {
        let manager = InodeManager::new();

        let original_inode = manager.get_or_allocate(&FileId::new("file"));
        manager.remove(&FileId::new("file"));

        // Re-allocating the same file_id should get a different inode
        let new_inode = manager.get_or_allocate(&FileId::new("file"));
        assert_ne!(original_inode, new_inode);
    }

    #[test]
    fn test_root_inode_constant() {
        assert_eq!(InodeManager::root_inode(), 1);
        assert_eq!(FUSE_ROOT_INODE, 1);
    }

    #[test]
    fn test_default_creates_valid_manager() {
        let manager = InodeManager::default();
        assert_eq!(
            manager.get_file_id(FUSE_ROOT_INODE),
            Some(FileId::new("root"))
        );
    }
}
