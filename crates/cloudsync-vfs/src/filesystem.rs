//! FUSE filesystem implementation
//!
//! Implements the `fuser::Filesystem` trait to provide a virtual filesystem
//! that exposes cloud files without downloading content until accessed.

use crate::cache::{CachedFile, MetadataCache};
use crate::inode::{InodeManager, FUSE_ROOT_INODE};
use anyhow::Result;
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
    /// Create a new CloudSync filesystem
    pub fn new(provider_id: &str) -> Result<Self> {
        info!("Initializing CloudSyncFS for provider: {}", provider_id);

        Ok(Self {
            provider_id: provider_id.to_string(),
            inode_manager: InodeManager::new(),
            metadata_cache: MetadataCache::new(),
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
            Some(id) => id,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        // Search for file in parent's children
        let children = self.metadata_cache.get_children(&parent_file_id);

        for child in children {
            if child.name == name_str {
                let inode = self.inode_manager.get_or_allocate(&child.file_id);
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
            Some(id) => id,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let file = match self.metadata_cache.get(&file_id) {
            Some(f) => f,
            None => {
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

        // Get directory file ID
        let dir_file_id = match self.inode_manager.get_file_id(ino) {
            Some(id) => id,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        // Add . and .. entries
        let mut entries = vec![
            (ino, FileType::Directory, ".".to_string()),
            (FUSE_ROOT_INODE, FileType::Directory, "..".to_string()),
        ];

        // Add children
        let children = self.metadata_cache.get_children(&dir_file_id);
        for child in children {
            let child_ino = self.inode_manager.get_or_allocate(&child.file_id);
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
            Some(id) => id,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let file = match self.metadata_cache.get(&file_id) {
            Some(f) => f,
            None => {
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
        // For now, return empty data as this is just a skeleton
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
    use cloudsync_core::FileId;

    #[test]
    fn test_filesystem_creation() {
        let fs = CloudSyncFS::new("test-provider");
        assert!(fs.is_ok());
    }

    #[test]
    fn test_root_attr() {
        let fs = CloudSyncFS::new("test-provider").unwrap();
        let attr = fs.get_root_attr();

        assert_eq!(attr.ino, FUSE_ROOT_INODE);
        assert_eq!(attr.kind, FileType::Directory);
        assert_eq!(attr.perm, 0o755);
    }

    #[test]
    fn test_cached_file_to_attr() {
        let fs = CloudSyncFS::new("test-provider").unwrap();

        let file = CachedFile {
            file_id: FileId::new("test-file"),
            name: "test.txt".to_string(),
            parent_id: None,
            size: 1024,
            mime_type: "text/plain".to_string(),
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
}
