//! CloudSync Database Layer
//!
//! This crate provides SQLite-based storage for CloudSync metadata,
//! including accounts, file states, sync queues, and conflict history.
//!
//! # Example
//!
//! ```rust
//! use cloudsync_db::{Database, Migration};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Create an in-memory database for testing
//! let db = Database::in_memory()?;
//!
//! // Query the database
//! db.with_conn(|conn| {
//!     // Use the connection...
//!     Ok(())
//! })?;
//! # Ok(())
//! # }
//! ```

pub mod accounts;
pub mod connection;
pub mod error;
pub mod files;
pub mod migrations;
pub mod vfs_inodes;

use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub use accounts::{
    create_account, delete_account, get_account_by_id, list_accounts, update_account, Account,
    ACCOUNTS_MIGRATION,
};
pub use connection::{ConnectionConfig, ConnectionManager};
pub use error::{DbError, DbResult};
pub use files::{
    create_file, delete_file, delete_files_by_account, get_file_by_id, get_file_by_provider_id,
    list_files, update_file, File, FILES_MIGRATION,
};
pub use migrations::{Migration, Migrator};
pub use vfs_inodes::VFS_INODES_MIGRATION;

/// High-level database interface with connection management and migrations.
///
/// This provides a simple API for opening databases with automatic migration
/// support. The database uses WAL mode for file-based databases to support
/// concurrent readers.
#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    /// Opens a file-based database with WAL mode and runs migrations.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the database file (will be created if it doesn't exist)
    /// * `migrations` - List of migrations to apply in order
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use cloudsync_db::{Database, Migration};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let migrations = vec![
    ///     Migration {
    ///         version: 1,
    ///         description: "Create users table",
    ///         sql: "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)",
    ///     }
    /// ];
    /// let db = Database::open("app.db", migrations)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn open<P: AsRef<Path>>(path: P, migrations: Vec<Migration>) -> DbResult<Self> {
        let config = ConnectionConfig::file_with_wal(path);
        let manager = ConnectionManager::new(config);
        let conn = manager.open()?;

        // Run migrations
        if !migrations.is_empty() {
            let migrator = Migrator::new(migrations);
            migrator.migrate(&conn)?;
        }

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Opens an in-memory database for testing.
    ///
    /// Note: In-memory databases do not support WAL mode.
    pub fn in_memory() -> DbResult<Self> {
        let config = ConnectionConfig::default();
        let manager = ConnectionManager::new(config);
        let conn = manager.open()?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Opens an in-memory database with migrations for testing.
    pub fn in_memory_with_migrations(migrations: Vec<Migration>) -> DbResult<Self> {
        let db = Self::in_memory()?;

        if !migrations.is_empty() {
            let conn = db.conn.lock().unwrap();
            let migrator = Migrator::new(migrations);
            migrator.migrate(&conn)?;
        }

        Ok(db)
    }

    /// Executes a function with access to the database connection.
    ///
    /// This provides thread-safe access to the underlying connection.
    pub fn with_conn<F, T>(&self, f: F) -> DbResult<T>
    where
        F: FnOnce(&Connection) -> DbResult<T>,
    {
        let conn = self.conn.lock().unwrap();
        f(&conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn can_create_in_memory_database() {
        let db = Database::in_memory();
        assert!(db.is_ok());
    }

    #[test]
    fn can_create_file_database_with_migrations() {
        let temp_file = NamedTempFile::new().unwrap();
        let migrations = vec![Migration {
            version: 1,
            description: "Create test table",
            sql: "CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT)",
        }];

        let db = Database::open(temp_file.path(), migrations);
        assert!(db.is_ok());
    }

    #[test]
    fn can_execute_queries_with_conn() {
        let migrations = vec![Migration {
            version: 1,
            description: "Create users table",
            sql: "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)",
        }];

        let db = Database::in_memory_with_migrations(migrations).unwrap();

        // Insert data
        db.with_conn(|conn| {
            conn.execute("INSERT INTO users (name) VALUES (?)", ["Alice"])?;
            Ok(())
        })
        .unwrap();

        // Query data
        let count: i64 = db
            .with_conn(|conn| {
                conn.query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))
                    .map_err(Into::into)
            })
            .unwrap();

        assert_eq!(count, 1);
    }

    #[test]
    fn migrations_are_applied_on_open() {
        let temp_file = NamedTempFile::new().unwrap();
        let migrations = vec![
            Migration {
                version: 1,
                description: "Create users",
                sql: "CREATE TABLE users (id INTEGER PRIMARY KEY)",
            },
            Migration {
                version: 2,
                description: "Create posts",
                sql: "CREATE TABLE posts (id INTEGER PRIMARY KEY)",
            },
        ];

        let db = Database::open(temp_file.path(), migrations).unwrap();

        // Verify both tables exist
        let users_exists: bool = db
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='users'",
                    [],
                    |row| row.get(0),
                )
                .map_err(Into::into)
            })
            .unwrap();

        let posts_exists: bool = db
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='posts'",
                    [],
                    |row| row.get(0),
                )
                .map_err(Into::into)
            })
            .unwrap();

        assert!(users_exists);
        assert!(posts_exists);
    }
}
