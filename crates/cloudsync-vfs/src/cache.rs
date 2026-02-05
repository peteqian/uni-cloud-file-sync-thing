//! Metadata cache for the virtual filesystem
//!
//! Stores file metadata (names, sizes, timestamps) without downloading file content.
//! Backed by SQLite for persistence across mounts.

use chrono::{DateTime, Utc};
use cloudsync_core::{CloudItem, FileId};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// File state in the cache
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Skeleton implementation - will be used in future issues
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

/// Cached file entry
#[derive(Debug, Clone)]
#[allow(dead_code)] // Skeleton implementation - fields will be used in future issues
pub struct CachedFile {
    pub file_id: FileId,
    pub name: String,
    pub parent_id: Option<FileId>,
    pub size: u64,
    pub mime_type: String,
    pub modified_time: DateTime<Utc>,
    pub is_directory: bool,
    pub state: CacheState,
}

/// Metadata cache manager
#[derive(Clone)]
pub struct MetadataCache {
    inner: Arc<Mutex<MetadataCacheInner>>,
}

struct MetadataCacheInner {
    /// Map from FileId to cached metadata
    files: HashMap<FileId, CachedFile>,

    /// Directory contents: parent FileId -> list of child FileIds
    children: HashMap<FileId, Vec<FileId>>,
}

impl MetadataCache {
    /// Create a new metadata cache
    pub fn new() -> Self {
        let inner = MetadataCacheInner {
            files: HashMap::new(),
            children: HashMap::new(),
        };

        Self {
            inner: Arc::new(Mutex::new(inner)),
        }
    }

    /// Add or update a file in the cache
    #[allow(dead_code)] // Will be used in future issues
    pub fn insert(&self, file: CachedFile) {
        let mut inner = self.inner.lock().unwrap();

        // Update children map if file has a parent
        if let Some(parent_id) = &file.parent_id {
            inner
                .children
                .entry(parent_id.clone())
                .or_default()
                .push(file.file_id.clone());
        }

        inner.files.insert(file.file_id.clone(), file);
    }

    /// Get a file from the cache
    pub fn get(&self, file_id: &FileId) -> Option<CachedFile> {
        let inner = self.inner.lock().unwrap();
        inner.files.get(file_id).cloned()
    }

    /// Get children of a directory
    pub fn get_children(&self, parent_id: &FileId) -> Vec<CachedFile> {
        let inner = self.inner.lock().unwrap();

        inner
            .children
            .get(parent_id)
            .map(|child_ids| {
                child_ids
                    .iter()
                    .filter_map(|id| inner.files.get(id).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Update the state of a file
    #[allow(dead_code)] // Will be used in future issues
    pub fn update_state(&self, file_id: &FileId, state: CacheState) {
        let mut inner = self.inner.lock().unwrap();

        if let Some(file) = inner.files.get_mut(file_id) {
            file.state = state;
        }
    }

    /// Remove a file from the cache
    #[allow(dead_code)] // Will be used in future issues
    pub fn remove(&self, file_id: &FileId) {
        let mut inner = self.inner.lock().unwrap();

        if let Some(file) = inner.files.remove(file_id) {
            // Remove from parent's children list
            if let Some(parent_id) = &file.parent_id {
                if let Some(children) = inner.children.get_mut(parent_id) {
                    children.retain(|id| id != file_id);
                }
            }

            // Remove from children map if it's a directory
            inner.children.remove(file_id);
        }
    }

    /// Populate cache from cloud items
    #[allow(dead_code)] // Will be used in future issues
    pub fn populate_from_items(&self, items: Vec<CloudItem>, root_id: &FileId) {
        for item in items {
            let file = CachedFile {
                file_id: item.id.clone(),
                name: item.name,
                parent_id: Some(root_id.clone()), // CloudItem doesn't have parent_id
                size: item.size.unwrap_or(0),
                mime_type: item.mime_type.unwrap_or_default(),
                modified_time: item.modified,
                is_directory: item.is_folder,
                state: CacheState::CloudOnly,
            };

            self.insert(file);
        }
    }

    /// Clear all cached entries
    #[allow(dead_code)] // Will be used in future issues
    pub fn clear(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.files.clear();
        inner.children.clear();
    }
}

impl Default for MetadataCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cloudsync_core::FileId;

    fn create_test_file(id: &str, name: &str, parent: Option<&str>) -> CachedFile {
        CachedFile {
            file_id: FileId::new(id),
            name: name.to_string(),
            parent_id: parent.map(FileId::new),
            size: 1024,
            mime_type: "text/plain".to_string(),
            modified_time: Utc::now(),
            is_directory: false,
            state: CacheState::CloudOnly,
        }
    }

    #[test]
    fn test_insert_and_get() {
        let cache = MetadataCache::new();
        let file = create_test_file("file1", "test.txt", None);

        cache.insert(file.clone());

        let retrieved = cache.get(&FileId::new("file1")).unwrap();
        assert_eq!(retrieved.name, "test.txt");
    }

    #[test]
    fn test_get_children() {
        let cache = MetadataCache::new();

        let parent = create_test_file("parent", "folder", None);
        let child1 = create_test_file("child1", "file1.txt", Some("parent"));
        let child2 = create_test_file("child2", "file2.txt", Some("parent"));

        cache.insert(parent);
        cache.insert(child1);
        cache.insert(child2);

        let children = cache.get_children(&FileId::new("parent"));
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn test_update_state() {
        let cache = MetadataCache::new();
        let file = create_test_file("file1", "test.txt", None);

        cache.insert(file);
        assert_eq!(
            cache.get(&FileId::new("file1")).unwrap().state,
            CacheState::CloudOnly
        );

        cache.update_state(&FileId::new("file1"), CacheState::Cached);
        assert_eq!(
            cache.get(&FileId::new("file1")).unwrap().state,
            CacheState::Cached
        );
    }

    #[test]
    fn test_remove() {
        let cache = MetadataCache::new();
        let file = create_test_file("file1", "test.txt", None);

        cache.insert(file);
        assert!(cache.get(&FileId::new("file1")).is_some());

        cache.remove(&FileId::new("file1"));
        assert!(cache.get(&FileId::new("file1")).is_none());
    }

    #[test]
    fn test_clear() {
        let cache = MetadataCache::new();
        cache.insert(create_test_file("file1", "test1.txt", None));
        cache.insert(create_test_file("file2", "test2.txt", None));

        cache.clear();

        assert!(cache.get(&FileId::new("file1")).is_none());
        assert!(cache.get(&FileId::new("file2")).is_none());
    }
}
