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

    #[test]
    fn test_get_nonexistent_file() {
        let cache = MetadataCache::new();
        assert!(cache.get(&FileId::new("does-not-exist")).is_none());
    }

    #[test]
    fn test_get_children_of_empty_directory() {
        let cache = MetadataCache::new();
        let children = cache.get_children(&FileId::new("empty-dir"));
        assert!(children.is_empty());
    }

    #[test]
    fn test_remove_nonexistent_file_is_noop() {
        let cache = MetadataCache::new();
        cache.remove(&FileId::new("does-not-exist"));
        // Should not panic
    }

    #[test]
    fn test_remove_child_updates_parent_children() {
        let cache = MetadataCache::new();

        let parent = create_test_file("parent", "folder", None);
        let child1 = create_test_file("child1", "file1.txt", Some("parent"));
        let child2 = create_test_file("child2", "file2.txt", Some("parent"));

        cache.insert(parent);
        cache.insert(child1);
        cache.insert(child2);

        assert_eq!(cache.get_children(&FileId::new("parent")).len(), 2);

        cache.remove(&FileId::new("child1"));

        let remaining = cache.get_children(&FileId::new("parent"));
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].name, "file2.txt");
    }

    #[test]
    fn test_insert_overwrites_existing_file() {
        let cache = MetadataCache::new();

        let file = CachedFile {
            file_id: FileId::new("file1"),
            name: "original.txt".to_string(),
            parent_id: None,
            size: 100,
            mime_type: "text/plain".to_string(),
            modified_time: Utc::now(),
            is_directory: false,
            state: CacheState::CloudOnly,
        };
        cache.insert(file);

        let updated = CachedFile {
            file_id: FileId::new("file1"),
            name: "renamed.txt".to_string(),
            parent_id: None,
            size: 200,
            mime_type: "text/plain".to_string(),
            modified_time: Utc::now(),
            is_directory: false,
            state: CacheState::Cached,
        };
        cache.insert(updated);

        let retrieved = cache.get(&FileId::new("file1")).unwrap();
        assert_eq!(retrieved.name, "renamed.txt");
        assert_eq!(retrieved.size, 200);
        assert_eq!(retrieved.state, CacheState::Cached);
    }

    #[test]
    fn test_update_state_nonexistent_file_is_noop() {
        let cache = MetadataCache::new();
        cache.update_state(&FileId::new("ghost"), CacheState::Cached);
        // Should not panic, and file should still not exist
        assert!(cache.get(&FileId::new("ghost")).is_none());
    }

    #[test]
    fn test_populate_from_items() {
        use cloudsync_core::{CloudItem, CloudPath};

        let cache = MetadataCache::new();
        let root_id = FileId::new("root");

        let items = vec![
            CloudItem::file(
                FileId::new("f1"),
                "document.pdf".to_string(),
                CloudPath::new("/document.pdf"),
                4096,
                Utc::now(),
            ),
            CloudItem::folder(
                FileId::new("d1"),
                "photos".to_string(),
                CloudPath::new("/photos"),
                Utc::now(),
            ),
        ];

        cache.populate_from_items(items, &root_id);

        let file = cache.get(&FileId::new("f1")).unwrap();
        assert_eq!(file.name, "document.pdf");
        assert_eq!(file.size, 4096);
        assert!(!file.is_directory);
        assert_eq!(file.state, CacheState::CloudOnly);

        let folder = cache.get(&FileId::new("d1")).unwrap();
        assert_eq!(folder.name, "photos");
        assert!(folder.is_directory);
        assert_eq!(folder.size, 0); // folders have no size

        // Both should be children of root
        let children = cache.get_children(&root_id);
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn test_nested_directory_hierarchy() {
        let cache = MetadataCache::new();

        let mut root = create_test_file("root", "root", None);
        root.is_directory = true;
        cache.insert(root);

        let mut subdir = create_test_file("subdir", "docs", Some("root"));
        subdir.is_directory = true;
        cache.insert(subdir);

        let nested_file = create_test_file("nested", "readme.md", Some("subdir"));
        cache.insert(nested_file);

        let root_children = cache.get_children(&FileId::new("root"));
        assert_eq!(root_children.len(), 1);
        assert_eq!(root_children[0].name, "docs");

        let subdir_children = cache.get_children(&FileId::new("subdir"));
        assert_eq!(subdir_children.len(), 1);
        assert_eq!(subdir_children[0].name, "readme.md");
    }

    #[test]
    fn test_state_transitions() {
        let cache = MetadataCache::new();
        let file = create_test_file("file1", "test.txt", None);
        cache.insert(file);

        // CloudOnly -> Downloading -> Cached -> Modified
        let transitions = [
            CacheState::Downloading,
            CacheState::Cached,
            CacheState::Modified,
        ];

        for expected_state in transitions {
            cache.update_state(&FileId::new("file1"), expected_state);
            let file = cache.get(&FileId::new("file1")).unwrap();
            assert_eq!(file.state, expected_state);
        }
    }

    #[test]
    fn test_default_creates_empty_cache() {
        let cache = MetadataCache::default();
        assert!(cache.get(&FileId::new("anything")).is_none());
        assert!(cache.get_children(&FileId::new("anything")).is_empty());
    }
}
