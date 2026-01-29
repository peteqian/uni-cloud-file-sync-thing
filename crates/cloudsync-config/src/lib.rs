//! CloudSync Configuration Management
//!
//! This crate handles loading, saving, and validating configuration
//! for CloudSync. It follows XDG Base Directory specification on Linux.

pub mod error;
pub mod paths;

use std::path::{Path, PathBuf};

pub use error::{ConfigError, ConfigResult};
pub use paths::CloudSyncPaths;

use serde::{Deserialize, Serialize};

/// Main configuration structure for CloudSync.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Config {
    /// General application settings.
    #[serde(default)]
    pub general: GeneralConfig,

    /// Synchronization settings.
    #[serde(default)]
    pub sync: SyncConfig,

    /// Conflict resolution settings.
    #[serde(default)]
    pub conflicts: ConflictsConfig,
}

impl Config {
    /// Creates a new configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads configuration from a file, using defaults if file doesn't exist.
    pub fn load_from_file(path: impl AsRef<Path>) -> ConfigResult<Self> {
        let path = path.as_ref();

        if !path.exists() {
            return Ok(Self::default());
        }

        let contents = std::fs::read_to_string(path).map_err(ConfigError::Io)?;

        let config: Config = toml::from_str(&contents)?;
        config.validate()?;

        Ok(config)
    }

    /// Saves configuration to a file.
    pub fn save_to_file(&self, path: impl AsRef<Path>) -> ConfigResult<()> {
        self.validate()?;

        let path = path.as_ref();

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let contents = toml::to_string_pretty(self)
            .map_err(|e| ConfigError::Invalid(format!("Failed to serialize config: {}", e)))?;

        std::fs::write(path, contents)?;

        Ok(())
    }

    /// Validates the configuration.
    pub fn validate(&self) -> ConfigResult<()> {
        self.sync.validate()?;
        self.conflicts.validate()?;
        Ok(())
    }
}

/// General application settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneralConfig {
    /// Log level (trace, debug, info, warn, error).
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// Whether to launch CloudSync on system startup.
    #[serde(default)]
    pub launch_on_startup: bool,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            log_level: default_log_level(),
            launch_on_startup: false,
        }
    }
}

fn default_log_level() -> String {
    "info".to_string()
}

/// Synchronization settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Root folder where cloud files are synchronized.
    #[serde(default = "default_root_folder")]
    pub root_folder: PathBuf,

    /// Upload bandwidth limit in KB/s (0 = unlimited).
    #[serde(default)]
    pub bandwidth_limit_up: u32,

    /// Download bandwidth limit in KB/s (0 = unlimited).
    #[serde(default)]
    pub bandwidth_limit_down: u32,

    /// Maximum cache size in MB.
    #[serde(default = "default_cache_size_mb")]
    pub cache_size_mb: u32,
}

impl SyncConfig {
    /// Returns the sync folder for a specific provider (e.g., "gdrive").
    pub fn provider_root(&self, provider_slug: &str) -> PathBuf {
        self.root_folder.join(provider_slug)
    }

    fn validate(&self) -> ConfigResult<()> {
        if self.cache_size_mb == 0 {
            return Err(ConfigError::Invalid(
                "cache_size_mb must be greater than 0".to_string(),
            ));
        }
        Ok(())
    }
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            root_folder: default_root_folder(),
            bandwidth_limit_up: 0,
            bandwidth_limit_down: 0,
            cache_size_mb: default_cache_size_mb(),
        }
    }
}

fn default_root_folder() -> PathBuf {
    directories::UserDirs::new()
        .and_then(|dirs| dirs.home_dir().join("UniCloudST").into())
        .unwrap_or_else(|| PathBuf::from("~/UniCloudST"))
}

fn default_cache_size_mb() -> u32 {
    1024 // 1 GB default
}

/// Conflict resolution settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConflictsConfig {
    /// Default conflict resolution strategy.
    #[serde(default)]
    pub resolution: ConflictResolution,
}

impl ConflictsConfig {
    fn validate(&self) -> ConfigResult<()> {
        Ok(())
    }
}

impl Default for ConflictsConfig {
    fn default() -> Self {
        Self {
            resolution: ConflictResolution::Prompt,
        }
    }
}

/// Conflict resolution strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ConflictResolution {
    /// Prompt the user to choose.
    #[default]
    Prompt,
    /// Keep the local version.
    KeepLocal,
    /// Keep the remote version.
    KeepRemote,
    /// Keep both versions (rename).
    KeepBoth,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn config_has_sensible_defaults() {
        let config = Config::new();
        assert_eq!(config.general.log_level, "info");
        assert!(!config.general.launch_on_startup);
        assert_eq!(config.sync.bandwidth_limit_up, 0);
        assert_eq!(config.sync.bandwidth_limit_down, 0);
        assert_eq!(config.sync.cache_size_mb, 1024);
        assert_eq!(config.conflicts.resolution, ConflictResolution::Prompt);
    }

    #[test]
    fn default_root_folder_is_unicloudst() {
        let config = Config::new();
        let root_folder = config.sync.root_folder;
        assert_eq!(root_folder.file_name().unwrap(), "UniCloudST");
    }

    #[test]
    fn provider_root_is_under_root_folder() {
        let config = Config::new();
        let provider_root = config.sync.provider_root("gdrive");
        assert_eq!(provider_root.file_name().unwrap(), "gdrive");
        assert_eq!(provider_root.parent().unwrap(), config.sync.root_folder);
    }

    #[test]
    fn config_validates_cache_size() {
        let mut config = Config::new();
        config.sync.cache_size_mb = 0;
        assert!(config.validate().is_err());

        config.sync.cache_size_mb = 100;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn config_serializes_to_toml() {
        let config = Config::new();
        let toml = toml::to_string(&config).unwrap();
        assert!(toml.contains("log_level"));
        assert!(toml.contains("resolution"));
    }

    #[test]
    fn config_deserializes_from_toml() {
        let toml = r#"
            [general]
            log_level = "debug"
            launch_on_startup = true

            [sync]
            bandwidth_limit_up = 512
            bandwidth_limit_down = 1024
            cache_size_mb = 2048

            [conflicts]
            resolution = "keep_local"
        "#;

        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.general.log_level, "debug");
        assert!(config.general.launch_on_startup);
        assert_eq!(config.sync.bandwidth_limit_up, 512);
        assert_eq!(config.sync.bandwidth_limit_down, 1024);
        assert_eq!(config.sync.cache_size_mb, 2048);
        assert_eq!(config.conflicts.resolution, ConflictResolution::KeepLocal);
    }

    #[test]
    fn config_saves_and_loads_from_file() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        let mut config = Config::new();
        config.general.log_level = "debug".to_string();
        config.sync.bandwidth_limit_up = 256;

        config.save_to_file(&config_path).unwrap();
        assert!(config_path.exists());

        let loaded = Config::load_from_file(&config_path).unwrap();
        assert_eq!(loaded.general.log_level, "debug");
        assert_eq!(loaded.sync.bandwidth_limit_up, 256);
    }

    #[test]
    fn config_load_returns_defaults_if_file_missing() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("nonexistent.toml");

        let config = Config::load_from_file(&config_path).unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn config_creates_parent_directory_on_save() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("nested").join("config.toml");

        let config = Config::new();
        config.save_to_file(&config_path).unwrap();

        assert!(config_path.exists());
    }

    #[test]
    fn conflict_resolution_serializes_to_snake_case() {
        assert_eq!(
            serde_json::to_string(&ConflictResolution::KeepLocal).unwrap(),
            "\"keep_local\""
        );
        assert_eq!(
            serde_json::to_string(&ConflictResolution::KeepBoth).unwrap(),
            "\"keep_both\""
        );
    }
}
