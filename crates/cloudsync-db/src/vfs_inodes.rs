//! VFS inode management for persistent inode-to-FileId mapping.
//!
//! This module provides database operations for managing the mapping between
//! FUSE inodes (u64) and cloud provider file IDs, ensuring inode stability
//! across mounts and daemon restarts.

use crate::{DbError, DbResult};
use rusqlite::{params, Connection};

/// Database migration for the vfs_inodes table.
pub const VFS_INODES_MIGRATION: &str = r#"
CREATE TABLE IF NOT EXISTS vfs_inodes (
    inode INTEGER PRIMARY KEY,
    file_id TEXT NOT NULL UNIQUE,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_vfs_inodes_file_id ON vfs_inodes(file_id);
"#;

/// Inserts a new inode mapping.
pub fn insert_inode(conn: &Connection, inode: u64, file_id: &str) -> DbResult<()> {
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "INSERT INTO vfs_inodes (inode, file_id, created_at) VALUES (?1, ?2, ?3)",
        params![inode as i64, file_id, now],
    )
    .map_err(|e| match e {
        rusqlite::Error::SqliteFailure(err, _)
            if err.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            DbError::Conflict(format!(
                "Inode mapping already exists for inode {} or file_id {}",
                inode, file_id
            ))
        }
        _ => DbError::from(e),
    })?;
    Ok(())
}

/// Looks up an inode by file ID.
pub fn get_inode_by_file_id(conn: &Connection, file_id: &str) -> DbResult<Option<u64>> {
    conn.query_row(
        "SELECT inode FROM vfs_inodes WHERE file_id = ?1",
        params![file_id],
        |row| row.get::<_, i64>(0).map(|v| v as u64),
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        _ => Err(DbError::from(e)),
    })
}

/// Looks up a file ID by inode.
pub fn get_file_id_by_inode(conn: &Connection, inode: u64) -> DbResult<Option<String>> {
    conn.query_row(
        "SELECT file_id FROM vfs_inodes WHERE inode = ?1",
        params![inode as i64],
        |row| row.get::<_, String>(0),
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        _ => Err(DbError::from(e)),
    })
}

/// Gets existing inode or inserts a new one atomically.
///
/// Uses the provided `next_inode` closure to allocate a new inode number
/// only when the file_id doesn't already have a mapping.
pub fn get_or_insert_inode(
    conn: &Connection,
    file_id: &str,
    next_inode: impl FnOnce() -> u64,
) -> DbResult<u64> {
    if let Some(inode) = get_inode_by_file_id(conn, file_id)? {
        return Ok(inode);
    }

    let inode = next_inode();
    insert_inode(conn, inode, file_id)?;
    Ok(inode)
}

/// Returns the maximum inode value in the table, or 0 if empty.
pub fn get_max_inode(conn: &Connection) -> DbResult<u64> {
    let max: Option<i64> =
        conn.query_row("SELECT MAX(inode) FROM vfs_inodes", [], |row| row.get(0))?;
    Ok(max.map(|v| v as u64).unwrap_or(0))
}

/// Removes an inode mapping by file ID.
///
/// Returns true if a row was deleted, false if no mapping existed.
pub fn remove_inode(conn: &Connection, file_id: &str) -> DbResult<bool> {
    let rows = conn.execute(
        "DELETE FROM vfs_inodes WHERE file_id = ?1",
        params![file_id],
    )?;
    Ok(rows > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConnectionManager, Migration, Migrator};

    fn setup_test_db() -> Connection {
        let manager = ConnectionManager::new(Default::default());
        let conn = manager.open().unwrap();

        let migrator = Migrator::new(vec![Migration {
            version: 1,
            description: "Create vfs_inodes table",
            sql: VFS_INODES_MIGRATION,
        }]);
        migrator.migrate(&conn).unwrap();

        conn
    }

    #[test]
    fn test_insert_and_get_by_file_id() {
        let conn = setup_test_db();

        insert_inode(&conn, 2, "file_abc").unwrap();

        let inode = get_inode_by_file_id(&conn, "file_abc").unwrap();
        assert_eq!(inode, Some(2));
    }

    #[test]
    fn test_insert_and_get_by_inode() {
        let conn = setup_test_db();

        insert_inode(&conn, 5, "file_xyz").unwrap();

        let file_id = get_file_id_by_inode(&conn, 5).unwrap();
        assert_eq!(file_id, Some("file_xyz".to_string()));
    }

    #[test]
    fn test_get_nonexistent_file_id() {
        let conn = setup_test_db();

        assert_eq!(get_inode_by_file_id(&conn, "ghost").unwrap(), None);
    }

    #[test]
    fn test_get_nonexistent_inode() {
        let conn = setup_test_db();

        assert_eq!(get_file_id_by_inode(&conn, 999).unwrap(), None);
    }

    #[test]
    fn test_duplicate_inode_fails() {
        let conn = setup_test_db();

        insert_inode(&conn, 2, "file_a").unwrap();
        let result = insert_inode(&conn, 2, "file_b");

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), DbError::Conflict(_)));
    }

    #[test]
    fn test_duplicate_file_id_fails() {
        let conn = setup_test_db();

        insert_inode(&conn, 2, "file_a").unwrap();
        let result = insert_inode(&conn, 3, "file_a");

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), DbError::Conflict(_)));
    }

    #[test]
    fn test_get_or_insert_creates_new() {
        let conn = setup_test_db();

        let inode = get_or_insert_inode(&conn, "new_file", || 10).unwrap();
        assert_eq!(inode, 10);

        // Verify it persisted
        assert_eq!(get_inode_by_file_id(&conn, "new_file").unwrap(), Some(10));
    }

    #[test]
    fn test_get_or_insert_returns_existing() {
        let conn = setup_test_db();

        insert_inode(&conn, 7, "existing_file").unwrap();

        // The closure should NOT be called since the file already exists
        let inode =
            get_or_insert_inode(&conn, "existing_file", || panic!("should not allocate")).unwrap();
        assert_eq!(inode, 7);
    }

    #[test]
    fn test_get_or_insert_idempotent() {
        let conn = setup_test_db();

        let mut counter = 10u64;
        let inode1 = get_or_insert_inode(&conn, "file_x", || {
            let v = counter;
            counter += 1;
            v
        })
        .unwrap();

        let inode2 = get_or_insert_inode(&conn, "file_x", || {
            let v = counter;
            counter += 1;
            v
        })
        .unwrap();

        assert_eq!(inode1, inode2);
        assert_eq!(inode1, 10); // Only first allocation should have been used
    }

    #[test]
    fn test_get_max_inode_empty() {
        let conn = setup_test_db();

        assert_eq!(get_max_inode(&conn).unwrap(), 0);
    }

    #[test]
    fn test_get_max_inode_with_data() {
        let conn = setup_test_db();

        insert_inode(&conn, 5, "file_a").unwrap();
        insert_inode(&conn, 12, "file_b").unwrap();
        insert_inode(&conn, 8, "file_c").unwrap();

        assert_eq!(get_max_inode(&conn).unwrap(), 12);
    }

    #[test]
    fn test_remove_existing() {
        let conn = setup_test_db();

        insert_inode(&conn, 3, "to_remove").unwrap();

        let removed = remove_inode(&conn, "to_remove").unwrap();
        assert!(removed);

        assert_eq!(get_inode_by_file_id(&conn, "to_remove").unwrap(), None);
        assert_eq!(get_file_id_by_inode(&conn, 3).unwrap(), None);
    }

    #[test]
    fn test_remove_nonexistent() {
        let conn = setup_test_db();

        let removed = remove_inode(&conn, "ghost").unwrap();
        assert!(!removed);
    }

    #[test]
    fn test_remove_then_reinsert() {
        let conn = setup_test_db();

        insert_inode(&conn, 3, "recycled").unwrap();
        remove_inode(&conn, "recycled").unwrap();

        // Can now reinsert with a different inode
        insert_inode(&conn, 10, "recycled").unwrap();
        assert_eq!(get_inode_by_file_id(&conn, "recycled").unwrap(), Some(10));
    }
}
