//! Database schema migrations.
//!
//! This module provides a simple, SQLite-appropriate migration system using
//! the user_version pragma to track the current schema version.

use crate::DbResult;
use rusqlite::Connection;

/// Represents a single database migration.
#[derive(Debug, Clone)]
pub struct Migration {
    /// The version number this migration upgrades to.
    pub version: i32,
    /// Description of what this migration does.
    pub description: &'static str,
    /// The SQL statements to execute for this migration.
    pub sql: &'static str,
}

/// Migrator handles applying database schema migrations.
pub struct Migrator {
    migrations: Vec<Migration>,
}

impl Migrator {
    /// Creates a new migrator with the given migrations.
    ///
    /// Migrations should be provided in order from version 1 upwards.
    pub fn new(migrations: Vec<Migration>) -> Self {
        Self { migrations }
    }

    /// Gets the current schema version from the database.
    ///
    /// Uses the user_version pragma, which is a SQLite built-in integer
    /// specifically designed for tracking schema versions.
    pub fn current_version(conn: &Connection) -> DbResult<i32> {
        let version: i32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        Ok(version)
    }

    /// Runs all pending migrations on the given connection.
    ///
    /// Migrations are run in a single transaction. If any migration fails,
    /// all changes are rolled back and the version is not updated.
    pub fn migrate(&self, conn: &Connection) -> DbResult<i32> {
        let current = Self::current_version(conn)?;
        let pending = self.pending_migrations(conn)?;

        if pending.is_empty() {
            return Ok(current);
        }

        // Run all pending migrations in a transaction
        let tx = conn.unchecked_transaction()?;

        for migration in pending {
            // Execute the migration SQL
            tx.execute_batch(migration.sql)?;

            // Update the version after each successful migration
            tx.pragma_update(None, "user_version", migration.version)?;
        }

        tx.commit()?;

        // Return the final version
        Self::current_version(conn)
    }

    /// Checks if there are pending migrations.
    pub fn has_pending_migrations(&self, conn: &Connection) -> DbResult<bool> {
        Ok(!self.pending_migrations(conn)?.is_empty())
    }

    /// Gets the list of pending migrations.
    ///
    /// Returns migrations with version numbers greater than the current
    /// database version, in ascending order.
    pub fn pending_migrations(&self, conn: &Connection) -> DbResult<Vec<&Migration>> {
        let current = Self::current_version(conn)?;
        Ok(self
            .migrations
            .iter()
            .filter(|m| m.version > current)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ConnectionManager;

    fn create_test_migrations() -> Vec<Migration> {
        vec![
            Migration {
                version: 1,
                description: "Create accounts table",
                sql: "CREATE TABLE accounts (
                    id INTEGER PRIMARY KEY,
                    provider TEXT NOT NULL,
                    email TEXT NOT NULL UNIQUE
                )",
            },
            Migration {
                version: 2,
                description: "Create files table",
                sql: "CREATE TABLE files (
                    id INTEGER PRIMARY KEY,
                    path TEXT NOT NULL UNIQUE,
                    account_id INTEGER NOT NULL,
                    FOREIGN KEY (account_id) REFERENCES accounts(id)
                )",
            },
            Migration {
                version: 3,
                description: "Add sync_state to files",
                sql: "ALTER TABLE files ADD COLUMN sync_state TEXT NOT NULL DEFAULT 'pending'",
            },
        ]
    }

    #[test]
    fn test_new_database_has_version_zero() {
        let manager = ConnectionManager::new(Default::default());
        let conn = manager.open().unwrap();

        let version = Migrator::current_version(&conn).unwrap();
        assert_eq!(version, 0, "New database should start at version 0");
    }

    #[test]
    fn test_migrate_from_zero_to_latest() {
        let manager = ConnectionManager::new(Default::default());
        let conn = manager.open().unwrap();
        let migrations = create_test_migrations();
        let migrator = Migrator::new(migrations);

        let final_version = migrator.migrate(&conn).unwrap();
        assert_eq!(final_version, 3, "Should migrate to version 3");

        let current = Migrator::current_version(&conn).unwrap();
        assert_eq!(current, 3);
    }

    #[test]
    fn test_tables_created_after_migration() {
        let manager = ConnectionManager::new(Default::default());
        let conn = manager.open().unwrap();
        let migrations = create_test_migrations();
        let migrator = Migrator::new(migrations);

        migrator.migrate(&conn).unwrap();

        // Verify accounts table exists
        let accounts_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='accounts'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(accounts_exists, "accounts table should exist");

        // Verify files table exists
        let files_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='files'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(files_exists, "files table should exist");
    }

    #[test]
    fn test_migration_idempotency() {
        let manager = ConnectionManager::new(Default::default());
        let conn = manager.open().unwrap();
        let migrations = create_test_migrations();
        let migrator = Migrator::new(migrations);

        // Run migrations twice
        let v1 = migrator.migrate(&conn).unwrap();
        let v2 = migrator.migrate(&conn).unwrap();

        assert_eq!(v1, v2, "Re-running migrations should not change version");
        assert_eq!(v1, 3);
    }

    #[test]
    fn test_has_pending_migrations() {
        let manager = ConnectionManager::new(Default::default());
        let conn = manager.open().unwrap();
        let migrations = create_test_migrations();
        let migrator = Migrator::new(migrations);

        let has_pending = migrator.has_pending_migrations(&conn).unwrap();
        assert!(has_pending, "New database should have pending migrations");

        migrator.migrate(&conn).unwrap();

        let has_pending = migrator.has_pending_migrations(&conn).unwrap();
        assert!(
            !has_pending,
            "Should have no pending migrations after migrating"
        );
    }

    #[test]
    fn test_pending_migrations_list() {
        let manager = ConnectionManager::new(Default::default());
        let conn = manager.open().unwrap();
        let migrations = create_test_migrations();
        let migrator = Migrator::new(migrations);

        let pending = migrator.pending_migrations(&conn).unwrap();
        assert_eq!(pending.len(), 3, "Should have 3 pending migrations");
        assert_eq!(pending[0].version, 1);
        assert_eq!(pending[1].version, 2);
        assert_eq!(pending[2].version, 3);

        migrator.migrate(&conn).unwrap();

        let pending = migrator.pending_migrations(&conn).unwrap();
        assert_eq!(pending.len(), 0, "Should have no pending migrations");
    }

    #[test]
    fn test_partial_migration() {
        let manager = ConnectionManager::new(Default::default());
        let conn = manager.open().unwrap();

        // Only apply first migration
        let migrator = Migrator::new(vec![create_test_migrations()[0].clone()]);
        migrator.migrate(&conn).unwrap();

        let version = Migrator::current_version(&conn).unwrap();
        assert_eq!(version, 1);

        // Now apply all migrations
        let full_migrator = Migrator::new(create_test_migrations());
        let pending = full_migrator.pending_migrations(&conn).unwrap();
        assert_eq!(
            pending.len(),
            2,
            "Should have 2 pending migrations (v2 and v3)"
        );
        assert_eq!(pending[0].version, 2);
        assert_eq!(pending[1].version, 3);

        full_migrator.migrate(&conn).unwrap();
        let version = Migrator::current_version(&conn).unwrap();
        assert_eq!(version, 3);
    }

    #[test]
    fn test_migration_rollback_on_error() {
        let manager = ConnectionManager::new(Default::default());
        let conn = manager.open().unwrap();

        let bad_migrations = vec![
            create_test_migrations()[0].clone(),
            Migration {
                version: 2,
                description: "Invalid SQL",
                sql: "THIS IS NOT VALID SQL",
            },
        ];

        let migrator = Migrator::new(bad_migrations);
        let result = migrator.migrate(&conn);

        assert!(result.is_err(), "Should fail on invalid SQL");

        // Version should still be 0 (or 1 depending on transaction handling)
        let version = Migrator::current_version(&conn).unwrap();
        // We expect version 0 because all migrations should roll back as a unit
        assert_eq!(version, 0, "Version should not change on migration failure");

        // Accounts table should not exist
        let accounts_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='accounts'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!accounts_exists, "Tables should not exist after rollback");
    }

    #[test]
    fn test_foreign_key_constraints_enforced() {
        let manager = ConnectionManager::new(Default::default());
        let conn = manager.open().unwrap();
        let migrations = create_test_migrations();
        let migrator = Migrator::new(migrations);

        migrator.migrate(&conn).unwrap();

        // Try to insert a file with invalid account_id
        let result = conn.execute(
            "INSERT INTO files (path, account_id) VALUES ('test.txt', 999)",
            [],
        );

        assert!(result.is_err(), "Should fail due to foreign key constraint");
    }
}
