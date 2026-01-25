//! Database connection management with WAL mode.
//!
//! SQLite with WAL mode allows one writer and multiple concurrent readers.
//! This module provides connection management that:
//! - Configures WAL mode for concurrent read access
//! - Applies proper pragmas for performance and safety
//! - Supports both persistent and in-memory databases

use crate::DbResult;
use rusqlite::{Connection, OpenFlags};
use std::path::Path;

/// Configuration for database connections.
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    /// Path to the database file. Use ":memory:" for in-memory databases.
    pub path: String,
    /// Enable WAL (Write-Ahead Logging) mode for better concurrency.
    /// Note: WAL mode is not supported for in-memory databases.
    pub enable_wal: bool,
    /// Page cache size in KB (negative value) or pages (positive value).
    /// Default: -64000 (64MB)
    pub cache_size: i32,
    /// Enable read-only mode for this connection.
    pub read_only: bool,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            path: ":memory:".to_string(),
            enable_wal: false,  // WAL not supported for in-memory
            cache_size: -64000, // 64MB in KB
            read_only: false,
        }
    }
}

impl ConnectionConfig {
    /// Creates a configuration for a file-based database with WAL mode.
    pub fn file_with_wal<P: AsRef<Path>>(path: P) -> Self {
        Self {
            path: path.as_ref().to_string_lossy().to_string(),
            enable_wal: true,
            cache_size: -64000,
            read_only: false,
        }
    }

    /// Creates a configuration for a read-only connection.
    pub fn read_only<P: AsRef<Path>>(path: P) -> Self {
        Self {
            path: path.as_ref().to_string_lossy().to_string(),
            enable_wal: true, // WAL mode benefits readers
            cache_size: -64000,
            read_only: true,
        }
    }
}

/// Manages database connections with proper SQLite configuration.
///
/// This is NOT a connection pool in the traditional sense. SQLite only supports
/// one writer at a time, but with WAL mode, multiple readers can access the
/// database concurrently. This manager provides:
/// - Proper connection configuration (WAL, pragmas, etc.)
/// - Separation between writer and reader connections
/// - Consistent settings across all connections
pub struct ConnectionManager {
    config: ConnectionConfig,
}

impl ConnectionManager {
    /// Creates a new connection manager with the given configuration.
    pub fn new(config: ConnectionConfig) -> Self {
        Self { config }
    }

    /// Opens a new database connection and applies all configuration.
    ///
    /// For file-based databases with WAL enabled:
    /// - Sets journal_mode to WAL
    /// - Enables foreign keys
    /// - Applies cache size settings
    /// - Sets synchronous mode for durability
    pub fn open(&self) -> DbResult<Connection> {
        let conn = if self.config.read_only {
            // Open in read-only mode
            Connection::open_with_flags(&self.config.path, OpenFlags::SQLITE_OPEN_READ_ONLY)?
        } else if self.config.path == ":memory:" {
            // In-memory database
            Connection::open(&self.config.path)?
        } else {
            // File-based database with read-write access
            Connection::open_with_flags(
                &self.config.path,
                OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
            )?
        };

        Self::apply_pragmas(&conn, &self.config)?;

        Ok(conn)
    }

    /// Checks if WAL mode is enabled on a connection.
    pub fn is_wal_enabled(conn: &Connection) -> DbResult<bool> {
        let journal_mode: String =
            conn.pragma_query_value(None, "journal_mode", |row| row.get(0))?;

        Ok(journal_mode.eq_ignore_ascii_case("wal"))
    }

    /// Applies standard pragmas to a connection.
    fn apply_pragmas(conn: &Connection, config: &ConnectionConfig) -> DbResult<()> {
        // Enable foreign keys (must be set per connection)
        conn.pragma_update(None, "foreign_keys", "ON")?;

        // Set cache size
        conn.pragma_update(None, "cache_size", config.cache_size)?;

        // Enable WAL mode if requested (only for file-based databases)
        if config.enable_wal && config.path != ":memory:" && !config.read_only {
            conn.pragma_update(None, "journal_mode", "WAL")?;
            // Set synchronous to NORMAL for WAL (good balance of safety and performance)
            conn.pragma_update(None, "synchronous", "NORMAL")?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_default_config_is_in_memory() {
        let config = ConnectionConfig::default();
        assert_eq!(config.path, ":memory:");
        assert!(!config.enable_wal, "WAL not supported for in-memory");
        assert_eq!(config.cache_size, -64000);
        assert!(!config.read_only);
    }

    #[test]
    fn test_file_with_wal_config() {
        let config = ConnectionConfig::file_with_wal("/tmp/test.db");
        assert_eq!(config.path, "/tmp/test.db");
        assert!(
            config.enable_wal,
            "WAL should be enabled for file databases"
        );
        assert!(!config.read_only);
    }

    #[test]
    fn test_read_only_config() {
        let config = ConnectionConfig::read_only("/tmp/test.db");
        assert_eq!(config.path, "/tmp/test.db");
        assert!(config.enable_wal, "WAL benefits concurrent readers");
        assert!(config.read_only);
    }

    #[test]
    fn test_open_in_memory_connection() {
        let config = ConnectionConfig::default();
        let manager = ConnectionManager::new(config);
        let conn = manager.open();
        assert!(conn.is_ok(), "Should be able to open in-memory database");
    }

    #[test]
    fn test_open_file_connection_with_wal() {
        let temp_file = NamedTempFile::new().unwrap();
        let config = ConnectionConfig::file_with_wal(temp_file.path());
        let manager = ConnectionManager::new(config);
        let conn = manager.open();
        assert!(conn.is_ok(), "Should be able to open file-based database");
    }

    #[test]
    fn test_wal_mode_enabled_for_file_db() {
        let temp_file = NamedTempFile::new().unwrap();
        let config = ConnectionConfig::file_with_wal(temp_file.path());
        let manager = ConnectionManager::new(config);
        let conn = manager.open().unwrap();

        let is_wal = ConnectionManager::is_wal_enabled(&conn).unwrap();
        assert!(is_wal, "WAL mode should be enabled for file databases");
    }

    #[test]
    fn test_wal_mode_not_set_for_in_memory() {
        let config = ConnectionConfig::default();
        let manager = ConnectionManager::new(config);
        let conn = manager.open().unwrap();

        let is_wal = ConnectionManager::is_wal_enabled(&conn).unwrap();
        assert!(!is_wal, "WAL mode cannot be used with in-memory databases");
    }

    #[test]
    fn test_cache_size_applied() {
        let config = ConnectionConfig {
            cache_size: -32000, // 32MB
            ..Default::default()
        };
        let manager = ConnectionManager::new(config);
        let conn = manager.open().unwrap();

        let cache_size: i32 = conn
            .pragma_query_value(None, "cache_size", |row| row.get(0))
            .unwrap();
        assert_eq!(cache_size, -32000, "Cache size should be applied");
    }

    #[test]
    fn test_foreign_keys_enabled() {
        let config = ConnectionConfig::default();
        let manager = ConnectionManager::new(config);
        let conn = manager.open().unwrap();

        let fk_enabled: bool = conn
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .unwrap();
        assert!(
            fk_enabled,
            "Foreign keys should be enabled for data integrity"
        );
    }

    #[test]
    fn test_concurrent_readers_with_wal() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();

        // Open writer connection
        let writer_config = ConnectionConfig::file_with_wal(path);
        let writer_manager = ConnectionManager::new(writer_config);
        let writer_conn = writer_manager.open().unwrap();

        // Create a test table
        writer_conn
            .execute("CREATE TABLE test (id INTEGER PRIMARY KEY, value TEXT)", [])
            .unwrap();
        writer_conn
            .execute("INSERT INTO test (value) VALUES ('hello')", [])
            .unwrap();

        // Open multiple reader connections
        let reader_config = ConnectionConfig::read_only(path);
        let reader_manager = ConnectionManager::new(reader_config);
        let reader1 = reader_manager.open().unwrap();
        let reader2 = reader_manager.open().unwrap();

        // Both readers should be able to query concurrently
        let count1: i64 = reader1
            .query_row("SELECT COUNT(*) FROM test", [], |row| row.get(0))
            .unwrap();
        let count2: i64 = reader2
            .query_row("SELECT COUNT(*) FROM test", [], |row| row.get(0))
            .unwrap();

        assert_eq!(count1, 1);
        assert_eq!(count2, 1);
    }

    #[test]
    fn test_synchronous_mode_for_durability() {
        let temp_file = NamedTempFile::new().unwrap();
        let config = ConnectionConfig::file_with_wal(temp_file.path());
        let manager = ConnectionManager::new(config);
        let conn = manager.open().unwrap();

        // Check that synchronous mode is set for durability
        // WAL mode typically uses NORMAL (1) synchronous for good balance
        let sync_mode: i32 = conn
            .pragma_query_value(None, "synchronous", |row| row.get(0))
            .unwrap();

        // Should be at least NORMAL (1) or FULL (2), not OFF (0)
        // 0 = OFF, 1 = NORMAL, 2 = FULL, 3 = EXTRA
        assert!(
            sync_mode >= 1,
            "Synchronous mode should be at least NORMAL for durability, got: {}",
            sync_mode
        );
    }
}
