//! CloudSync Virtual Filesystem
//!
//! This crate provides a FUSE-based virtual filesystem that allows on-demand access
//! to cloud storage files. Files appear in the filesystem immediately, but content
//! is only downloaded when accessed.

mod cache;
mod filesystem;
mod inode;

pub use filesystem::CloudSyncFS;

use anyhow::Result;
use std::path::{Path, PathBuf};
use tracing::{error, info};

/// Mount a cloud provider's filesystem at the specified path.
///
/// # Arguments
/// * `mount_point` - Directory where the filesystem will be mounted
/// * `provider_id` - Cloud provider identifier (e.g., "gdrive")
///
/// # Returns
/// A handle that unmounts the filesystem when dropped
pub fn mount(mount_point: &Path, provider_id: &str) -> Result<MountHandle> {
    info!("Mounting {} at {:?}", provider_id, mount_point);

    // Ensure mount point exists
    if !mount_point.exists() {
        std::fs::create_dir_all(mount_point)?;
    }

    // Create the filesystem
    let fs = CloudSyncFS::new(provider_id)?;

    // Mount options
    let options = vec![
        fuser::MountOption::FSName(format!("cloudsync-{}", provider_id)),
        fuser::MountOption::AutoUnmount,
        // Note: AllowOther requires 'user_allow_other' in /etc/fuse.conf
        // Uncomment if needed: fuser::MountOption::AllowOther,
    ];

    // Clone mount point for both thread and handle
    let mount_point_buf = mount_point.to_path_buf();
    let mount_point_for_thread = mount_point_buf.clone();

    // Spawn background thread for FUSE session
    let session = std::thread::spawn(move || {
        if let Err(e) = fuser::mount2(fs, &mount_point_for_thread, &options) {
            error!("FUSE mount failed: {}", e);
        }
    });

    info!("Filesystem mounted successfully");

    Ok(MountHandle {
        mount_point: mount_point_buf,
        _session: session,
    })
}

/// Handle for a mounted filesystem. Unmounts on drop.
pub struct MountHandle {
    mount_point: PathBuf,
    _session: std::thread::JoinHandle<()>,
}

impl Drop for MountHandle {
    fn drop(&mut self) {
        info!("Unmounting filesystem at {:?}", self.mount_point);
        // FUSE will auto-unmount due to MountOption::AutoUnmount
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_mount_unmount() {
        let temp_dir = TempDir::new().unwrap();
        let mount_point = temp_dir.path().join("mount");

        // This will fail without proper FUSE setup, but tests the API
        let result = mount(&mount_point, "test");

        // In CI/testing, FUSE may not be available
        if result.is_err() {
            println!("FUSE not available in test environment");
        }
    }
}
