//! XDG-compliant directory paths for CloudSync.

use std::path::PathBuf;

use directories::ProjectDirs;

/// Manages XDG-compliant directory paths for CloudSync.
///
/// On Linux, follows the XDG Base Directory specification:
/// - Config: `~/.config/cloudsync/`
/// - Data: `~/.local/share/cloudsync/`
/// - Cache: `~/.cache/cloudsync/`
/// - Runtime: `/run/user/$UID/cloudsync/`
pub struct CloudSyncPaths {
    project_dirs: ProjectDirs,
}

impl CloudSyncPaths {
    /// Creates a new CloudSyncPaths instance.
    ///
    /// Returns None if home directory cannot be determined.
    pub fn new() -> Option<Self> {
        ProjectDirs::from("", "", "cloudsync").map(|project_dirs| Self { project_dirs })
    }

    /// Returns the configuration directory path.
    ///
    /// Example: `~/.config/cloudsync/`
    pub fn config_dir(&self) -> PathBuf {
        self.project_dirs.config_dir().to_path_buf()
    }

    /// Returns the data directory path.
    ///
    /// Example: `~/.local/share/cloudsync/`
    pub fn data_dir(&self) -> PathBuf {
        self.project_dirs.data_dir().to_path_buf()
    }

    /// Returns the cache directory path.
    ///
    /// Example: `~/.cache/cloudsync/`
    pub fn cache_dir(&self) -> PathBuf {
        self.project_dirs.cache_dir().to_path_buf()
    }

    /// Returns the config file path.
    ///
    /// Example: `~/.config/cloudsync/config.toml`
    pub fn config_file(&self) -> PathBuf {
        self.config_dir().join("config.toml")
    }

    /// Returns the database file path.
    ///
    /// Example: `~/.local/share/cloudsync/cloudsync.db`
    pub fn database_file(&self) -> PathBuf {
        self.data_dir().join("cloudsync.db")
    }

    /// Returns the IPC socket path.
    ///
    /// On Linux: `/run/user/$UID/cloudsync.sock` or `/tmp/cloudsync-$UID.sock`
    pub fn ipc_socket(&self) -> PathBuf {
        #[cfg(unix)]
        {
            if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
                return PathBuf::from(runtime_dir).join("cloudsync.sock");
            }
            // Fallback to /tmp if XDG_RUNTIME_DIR is not set
            let uid = unsafe { libc::getuid() };
            PathBuf::from(format!("/tmp/cloudsync-{}.sock", uid))
        }

        #[cfg(not(unix))]
        {
            // On Windows, use named pipe path
            PathBuf::from("\\\\.\\pipe\\cloudsync")
        }
    }

    /// Returns the log directory path.
    ///
    /// Example: `~/.cache/cloudsync/logs/`
    pub fn log_dir(&self) -> PathBuf {
        self.cache_dir().join("logs")
    }

    /// Returns the file cache directory path.
    ///
    /// Example: `~/.cache/cloudsync/files/`
    pub fn file_cache_dir(&self) -> PathBuf {
        self.cache_dir().join("files")
    }

    /// Creates all necessary directories.
    pub fn create_directories(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(self.config_dir())?;
        std::fs::create_dir_all(self.data_dir())?;
        std::fs::create_dir_all(self.cache_dir())?;
        std::fs::create_dir_all(self.log_dir())?;
        std::fs::create_dir_all(self.file_cache_dir())?;
        Ok(())
    }
}

impl Default for CloudSyncPaths {
    fn default() -> Self {
        Self::new().expect("Failed to determine CloudSync directory paths")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_can_be_created() {
        let paths = CloudSyncPaths::new();
        assert!(paths.is_some());
    }

    #[test]
    fn config_dir_is_non_empty() {
        let paths = CloudSyncPaths::new().unwrap();
        let config_dir = paths.config_dir();
        assert!(!config_dir.as_os_str().is_empty());
        assert!(config_dir.to_str().unwrap().contains("cloudsync"));
    }

    #[test]
    fn data_dir_is_non_empty() {
        let paths = CloudSyncPaths::new().unwrap();
        let data_dir = paths.data_dir();
        assert!(!data_dir.as_os_str().is_empty());
        assert!(data_dir.to_str().unwrap().contains("cloudsync"));
    }

    #[test]
    fn cache_dir_is_non_empty() {
        let paths = CloudSyncPaths::new().unwrap();
        let cache_dir = paths.cache_dir();
        assert!(!cache_dir.as_os_str().is_empty());
        assert!(cache_dir.to_str().unwrap().contains("cloudsync"));
    }

    #[test]
    fn config_file_has_toml_extension() {
        let paths = CloudSyncPaths::new().unwrap();
        let config_file = paths.config_file();
        assert_eq!(config_file.extension().unwrap(), "toml");
        assert!(config_file.to_str().unwrap().ends_with("config.toml"));
    }

    #[test]
    fn database_file_has_db_extension() {
        let paths = CloudSyncPaths::new().unwrap();
        let db_file = paths.database_file();
        assert_eq!(db_file.extension().unwrap(), "db");
        assert!(db_file.to_str().unwrap().ends_with("cloudsync.db"));
    }

    #[test]
    fn ipc_socket_path_is_valid() {
        let paths = CloudSyncPaths::new().unwrap();
        let socket = paths.ipc_socket();
        assert!(!socket.as_os_str().is_empty());

        #[cfg(unix)]
        {
            let socket_str = socket.to_str().unwrap();
            assert!(socket_str.ends_with(".sock"));
        }
    }

    #[test]
    fn log_dir_is_under_cache() {
        let paths = CloudSyncPaths::new().unwrap();
        let log_dir = paths.log_dir();
        assert!(log_dir.starts_with(paths.cache_dir()));
        assert!(log_dir.to_str().unwrap().ends_with("logs"));
    }

    #[test]
    fn file_cache_dir_is_under_cache() {
        let paths = CloudSyncPaths::new().unwrap();
        let file_cache = paths.file_cache_dir();
        assert!(file_cache.starts_with(paths.cache_dir()));
        assert!(file_cache.to_str().unwrap().ends_with("files"));
    }

    #[test]
    fn directories_are_distinct() {
        let paths = CloudSyncPaths::new().unwrap();
        let config = paths.config_dir();
        let data = paths.data_dir();
        let cache = paths.cache_dir();

        // Config and data should be different
        assert_ne!(config, data);
        // Config and cache should be different
        assert_ne!(config, cache);
        // Data and cache should be different (on most systems)
        // Note: On some systems they might be the same, so we don't assert here
    }
}
