//! Database helper for persistent state storage
//!
//! This module provides a singleton database instance stored in XDG state directory.

use crate::state::{ENTRY_STATE_BUCKET, EntryState, PersistentState, RedbPersistentState};
use guisu_config::dirs;
use guisu_core::{Error, Result};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock, RwLock};
use tracing::warn;

/// Global database instance
static DB_INSTANCE: OnceLock<Arc<RwLock<Option<RedbPersistentState>>>> = OnceLock::new();

/// Get the database path in XDG state directory
///
/// # Errors
///
/// Returns an error if the state directory cannot be determined or created
pub fn get_db_path() -> Result<PathBuf> {
    let state_dir = dirs::state_dir()
        .ok_or_else(|| Error::State("Failed to get state directory".to_string()))?;

    // Ensure state directory exists
    std::fs::create_dir_all(&state_dir).map_err(|e| {
        Error::State(format!(
            "Failed to create state directory {}: {}",
            state_dir.display(),
            e
        ))
    })?;

    Ok(state_dir.join("state.db"))
}

/// Get or create the database instance
///
/// # Errors
///
/// Returns an error if the database instance cannot be accessed
pub fn get_db() -> Result<Arc<RwLock<Option<RedbPersistentState>>>> {
    Ok(Arc::clone(
        DB_INSTANCE.get_or_init(|| Arc::new(RwLock::new(None))),
    ))
}

/// Open the database (creates if doesn't exist)
///
/// # Errors
///
/// Returns an error if the database cannot be opened or created (e.g., permission denied, disk full, corrupted database file)
pub fn open_db() -> Result<()> {
    let db_path = get_db_path()?;
    let db = RedbPersistentState::new(&db_path).map_err(|e| {
        Error::State(format!(
            "Failed to open database at {}: {}",
            db_path.display(),
            e
        ))
    })?;

    let db_instance = get_db()?;
    let mut guard = db_instance.write().unwrap_or_else(|poisoned| {
        warn!("Database write lock was poisoned during open, attempting recovery");
        let mut guard = poisoned.into_inner();

        // Validate existing database state if present
        let is_corrupted = if let Some(ref existing_db) = *guard {
            // Perform integrity check: try a simple read operation
            match existing_db.get(ENTRY_STATE_BUCKET, b"_integrity_check") {
                Ok(_) => {
                    warn!("Database integrity check passed after lock poisoning");
                    false
                }
                Err(e) => {
                    warn!(
                        "Database corrupted after lock poisoning: {}. \
                         Deleting corrupted file and rebuilding.",
                        e
                    );
                    true
                }
            }
        } else {
            false
        };

        // If corrupted, clear and delete the file
        if is_corrupted {
            *guard = None;

            // Delete the corrupted database file
            if let Ok(db_path) = get_db_path()
                && db_path.exists()
            {
                if let Err(e) = std::fs::remove_file(&db_path) {
                    warn!("Failed to remove corrupted database file: {}", e);
                } else {
                    warn!("Corrupted database file removed: {}", db_path.display());
                }
            }
        }

        guard
    });
    *guard = Some(db);

    Ok(())
}

/// Save entry state to database
///
/// # Errors
///
/// Returns an error if the state cannot be saved (e.g., database not opened, serialization failure, write error)
pub fn save_entry_state(path: &str, content: &[u8], mode: Option<u32>) -> Result<()> {
    let db_instance = get_db()?;
    let guard = db_instance.write().unwrap_or_else(|poisoned| {
        warn!("Database write lock was poisoned during save, recovering");
        poisoned.into_inner()
    });

    if let Some(db) = guard.as_ref() {
        let state = EntryState::new(content, mode);
        db.set(ENTRY_STATE_BUCKET, path.as_bytes(), &state.to_bytes()?)
            .map_err(|e| Error::State(format!("Failed to save state for {path}: {e}")))?;
    } else {
        return Err(Error::State("Database not opened".to_string()));
    }

    Ok(())
}

/// Get entry state from database
///
/// # Errors
///
/// Returns an error if the state cannot be retrieved (e.g., database not opened, deserialization failure, read error)
pub fn get_entry_state(path: &str) -> Result<Option<EntryState>> {
    let db_instance = get_db()?;
    let guard = db_instance.read().unwrap_or_else(|poisoned| {
        warn!("Database read lock was poisoned during get, recovering");
        poisoned.into_inner()
    });

    if let Some(db) = guard.as_ref() {
        let bytes = db
            .get(ENTRY_STATE_BUCKET, path.as_bytes())
            .map_err(|e| Error::State(format!("Failed to get state for {path}: {e}")))?;

        Ok(bytes.and_then(|b| EntryState::from_bytes(&b)))
    } else {
        Err(Error::State("Database not opened".to_string()))
    }
}

/// Delete entry state from database
///
/// # Errors
///
/// Returns an error if the state cannot be deleted (e.g., database not opened, write error)
pub fn delete_entry_state(path: &str) -> Result<()> {
    let db_instance = get_db()?;
    let guard = db_instance.write().unwrap_or_else(|poisoned| {
        warn!("Database write lock was poisoned during delete, recovering");
        poisoned.into_inner()
    });

    if let Some(db) = guard.as_ref() {
        db.delete(ENTRY_STATE_BUCKET, path.as_bytes())
            .map_err(|e| Error::State(format!("Failed to delete state for {path}: {e}")))?;
    }

    Ok(())
}

/// Close the database
///
/// # Errors
///
/// Returns an error if the database cannot be closed properly (e.g., outstanding transactions, I/O error)
pub fn close_db() -> Result<()> {
    let db_instance = get_db()?;
    let mut guard = db_instance.write().unwrap_or_else(|poisoned| {
        warn!("Database write lock was poisoned during close, recovering");
        poisoned.into_inner()
    });

    if let Some(db) = guard.take() {
        db.close()?;
    }

    Ok(())
}
#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    /// Create an isolated test database in a temporary directory
    ///
    /// Returns (temp_dir, database_instance)
    /// The temp_dir must be kept alive for the duration of the test
    fn test_db_setup() -> (TempDir, RedbPersistentState) {
        let temp = TempDir::new().expect("Failed to create temp dir");
        let db_path = temp.path().join("test.db");
        let db = RedbPersistentState::new(&db_path).expect("Failed to create test db");
        (temp, db)
    }

    #[test]
    #[serial]
    fn test_get_db_path() {
        let result = get_db_path();
        assert!(result.is_ok());

        let path = result.unwrap();
        assert!(path.to_string_lossy().contains("state.db"));
    }

    #[test]
    #[serial]
    fn test_get_db_returns_singleton() {
        let db1 = get_db().expect("Failed to get db");
        let db2 = get_db().expect("Failed to get db");

        // Both should be the same Arc instance
        assert!(Arc::ptr_eq(&db1, &db2));
    }

    #[test]
    fn test_save_and_get_entry_state() {
        let (_temp, db) = test_db_setup();

        // Save entry
        let content = b"test content";
        let state = EntryState::new(content, Some(0o644));
        db.set(
            ENTRY_STATE_BUCKET,
            b"test/file.txt",
            &state.to_bytes().unwrap(),
        )
        .expect("Failed to save");

        // Get entry back
        let bytes = db
            .get(ENTRY_STATE_BUCKET, b"test/file.txt")
            .expect("Failed to get")
            .expect("Entry not found");

        let retrieved = EntryState::from_bytes(&bytes).expect("Failed to deserialize");
        assert_eq!(retrieved.mode, Some(0o644));
    }

    #[test]
    fn test_save_without_mode() {
        let (_temp, db) = test_db_setup();

        let content = b"content";
        let state = EntryState::new(content, None);
        db.set(ENTRY_STATE_BUCKET, b"file.txt", &state.to_bytes().unwrap())
            .expect("Failed to save");

        let bytes = db
            .get(ENTRY_STATE_BUCKET, b"file.txt")
            .expect("Failed to get")
            .expect("Entry not found");

        let retrieved = EntryState::from_bytes(&bytes).expect("Failed to deserialize");
        assert_eq!(retrieved.mode, None);
    }

    #[test]
    fn test_get_nonexistent_entry() {
        let (_temp, db) = test_db_setup();

        let result = db
            .get(ENTRY_STATE_BUCKET, b"nonexistent/file")
            .expect("Failed to get");

        assert!(result.is_none());
    }

    #[test]
    fn test_delete_entry_state() {
        let (_temp, db) = test_db_setup();

        // Save entry
        let state = EntryState::new(b"content", None);
        db.set(
            ENTRY_STATE_BUCKET,
            b"to_delete.txt",
            &state.to_bytes().unwrap(),
        )
        .expect("Failed to save");

        // Verify it exists
        assert!(
            db.get(ENTRY_STATE_BUCKET, b"to_delete.txt")
                .expect("Failed to get")
                .is_some()
        );

        // Delete it
        db.delete(ENTRY_STATE_BUCKET, b"to_delete.txt")
            .expect("Failed to delete");

        // Verify it's gone
        assert!(
            db.get(ENTRY_STATE_BUCKET, b"to_delete.txt")
                .expect("Failed to get")
                .is_none()
        );
    }

    #[test]
    fn test_delete_nonexistent_entry() {
        let (_temp, db) = test_db_setup();

        // Deleting non-existent entry should not error
        let result = db.delete(ENTRY_STATE_BUCKET, b"nonexistent");
        assert!(result.is_ok());
    }

    #[test]
    #[serial]
    fn test_save_before_open_fails() {
        let _ = close_db();

        // Try to save without opening database
        let result = save_entry_state("test.txt", b"content", None);
        assert!(result.is_err());
    }

    #[test]
    #[serial]
    fn test_get_before_open_fails() {
        let _ = close_db();

        // Try to get without opening database
        let result = get_entry_state("test.txt");
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_saves_same_path() {
        let (_temp, db) = test_db_setup();

        // Save first version
        let state1 = EntryState::new(b"version 1", Some(0o644));
        db.set(ENTRY_STATE_BUCKET, b"file.txt", &state1.to_bytes().unwrap())
            .expect("Failed to save v1");

        // Save second version (should overwrite)
        let state2 = EntryState::new(b"version 2", Some(0o600));
        db.set(ENTRY_STATE_BUCKET, b"file.txt", &state2.to_bytes().unwrap())
            .expect("Failed to save v2");

        // Get should return latest version
        let bytes = db
            .get(ENTRY_STATE_BUCKET, b"file.txt")
            .expect("Failed to get")
            .expect("Entry not found");

        let retrieved = EntryState::from_bytes(&bytes).expect("Failed to deserialize");
        assert_eq!(retrieved.mode, Some(0o600));
    }

    #[test]
    fn test_save_multiple_entries() {
        let (_temp, db) = test_db_setup();

        // Save multiple entries
        for i in 0..10 {
            let path = format!("file{i}.txt");
            let content = format!("content {i}");
            let state = EntryState::new(content.as_bytes(), Some(0o644));
            db.set(
                ENTRY_STATE_BUCKET,
                path.as_bytes(),
                &state.to_bytes().unwrap(),
            )
            .expect("Failed to save");
        }

        // Verify all can be retrieved
        for i in 0..10 {
            let path = format!("file{i}.txt");
            let bytes = db
                .get(ENTRY_STATE_BUCKET, path.as_bytes())
                .expect("Failed to get")
                .expect("Entry not found");

            let result = EntryState::from_bytes(&bytes).expect("Failed to deserialize");
            assert_eq!(result.mode, Some(0o644));
        }
    }

    #[test]
    #[serial]
    fn test_close_and_reopen() {
        let _ = close_db();

        // Open, save, close
        open_db().expect("Failed to open db");
        save_entry_state("persistent.txt", b"data", Some(0o644)).expect("Failed to save");
        close_db().expect("Failed to close");

        // Reopen and verify data persists
        open_db().expect("Failed to reopen db");
        let retrieved = get_entry_state("persistent.txt")
            .expect("Failed to get")
            .expect("Entry not found");

        assert_eq!(retrieved.mode, Some(0o644));

        let _ = close_db();
    }

    #[test]
    fn test_path_with_special_characters() {
        let (_temp, db) = test_db_setup();

        let paths = vec![
            "file with spaces.txt",
            "file-with-dashes.txt",
            "file_with_underscores.txt",
            ".hidden_file",
            "directory/subdirectory/file.txt",
        ];

        for path in paths {
            let state = EntryState::new(b"content", None);
            db.set(
                ENTRY_STATE_BUCKET,
                path.as_bytes(),
                &state.to_bytes().unwrap(),
            )
            .unwrap_or_else(|_| panic!("Failed to save {path}"));

            let result = db
                .get(ENTRY_STATE_BUCKET, path.as_bytes())
                .unwrap_or_else(|_| panic!("Failed to get {path}"));
            assert!(result.is_some(), "Entry not found: {path}");
        }
    }

    #[test]
    #[serial]
    fn test_close_already_closed() {
        let _ = close_db();

        // Close when already closed should not error
        let result = close_db();
        assert!(result.is_ok());
    }

    #[test]
    fn test_content_hash_changes() {
        let (_temp, db) = test_db_setup();

        // Save with content A
        let state_a = EntryState::new(b"content A", None);
        db.set(
            ENTRY_STATE_BUCKET,
            b"file.txt",
            &state_a.to_bytes().unwrap(),
        )
        .expect("Failed to save A");
        let bytes_a = db
            .get(ENTRY_STATE_BUCKET, b"file.txt")
            .expect("Failed to get A")
            .expect("Entry not found");
        let hash_a = EntryState::from_bytes(&bytes_a)
            .expect("Failed to deserialize")
            .content_hash;

        // Save with different content B
        let state_b = EntryState::new(b"content B", None);
        db.set(
            ENTRY_STATE_BUCKET,
            b"file.txt",
            &state_b.to_bytes().unwrap(),
        )
        .expect("Failed to save B");
        let bytes_b = db
            .get(ENTRY_STATE_BUCKET, b"file.txt")
            .expect("Failed to get B")
            .expect("Entry not found");
        let hash_b = EntryState::from_bytes(&bytes_b)
            .expect("Failed to deserialize")
            .content_hash;

        // Hashes should be different
        assert_ne!(hash_a, hash_b);
    }

    #[test]
    fn test_empty_content() {
        let (_temp, db) = test_db_setup();

        // Save empty content
        let state = EntryState::new(b"", Some(0o644));
        db.set(ENTRY_STATE_BUCKET, b"empty.txt", &state.to_bytes().unwrap())
            .expect("Failed to save empty content");

        let bytes = db
            .get(ENTRY_STATE_BUCKET, b"empty.txt")
            .expect("Failed to get")
            .expect("Entry not found");

        let retrieved = EntryState::from_bytes(&bytes).expect("Failed to deserialize");
        assert_eq!(retrieved.mode, Some(0o644));
        // Empty content should have a hash (even if it's the hash of empty bytes)
        assert!(!retrieved.content_hash.is_empty());
    }

    #[test]
    fn test_large_content() {
        let (_temp, db) = test_db_setup();

        // Create large content (1MB)
        let large_content = vec![b'X'; 1024 * 1024];

        let state = EntryState::new(&large_content, None);
        db.set(
            ENTRY_STATE_BUCKET,
            b"large_file.bin",
            &state.to_bytes().unwrap(),
        )
        .expect("Failed to save large content");

        let bytes = db
            .get(ENTRY_STATE_BUCKET, b"large_file.bin")
            .expect("Failed to get")
            .expect("Entry not found");

        let retrieved = EntryState::from_bytes(&bytes).expect("Failed to deserialize");
        // Hash should be computed correctly
        assert!(!retrieved.content_hash.is_empty());
    }

    #[test]
    fn test_unicode_in_path() {
        let (_temp, db) = test_db_setup();

        let unicode_paths = vec![
            "файл.txt",     // Russian
            "文件.txt",     // Chinese
            "ファイル.txt", // Japanese
            "αρχείο.txt",   // Greek
        ];

        for path in unicode_paths {
            let state = EntryState::new(b"content", None);
            db.set(
                ENTRY_STATE_BUCKET,
                path.as_bytes(),
                &state.to_bytes().unwrap(),
            )
            .unwrap_or_else(|_| panic!("Failed to save {path}"));

            let result = db
                .get(ENTRY_STATE_BUCKET, path.as_bytes())
                .unwrap_or_else(|_| panic!("Failed to get {path}"));
            assert!(result.is_some(), "Entry not found: {path}");
        }
    }

    #[test]
    fn test_delete_and_recreate() {
        let (_temp, db) = test_db_setup();

        // Save, delete, and recreate entry
        let state1 = EntryState::new(b"version 1", Some(0o644));
        db.set(ENTRY_STATE_BUCKET, b"file.txt", &state1.to_bytes().unwrap())
            .expect("Failed to save");

        db.delete(ENTRY_STATE_BUCKET, b"file.txt")
            .expect("Failed to delete");

        // Recreate with different content
        let state2 = EntryState::new(b"version 2", Some(0o600));
        db.set(ENTRY_STATE_BUCKET, b"file.txt", &state2.to_bytes().unwrap())
            .expect("Failed to recreate");

        let bytes = db
            .get(ENTRY_STATE_BUCKET, b"file.txt")
            .expect("Failed to get")
            .expect("Entry not found");

        let retrieved = EntryState::from_bytes(&bytes).expect("Failed to deserialize");
        assert_eq!(retrieved.mode, Some(0o600));
    }

    #[test]
    fn test_binary_content() {
        let (_temp, db) = test_db_setup();

        // Binary content with all byte values
        let binary: Vec<u8> = (0u8..=255).collect();

        let state = EntryState::new(&binary, None);
        db.set(
            ENTRY_STATE_BUCKET,
            b"binary.dat",
            &state.to_bytes().unwrap(),
        )
        .expect("Failed to save binary");

        let retrieved = db
            .get(ENTRY_STATE_BUCKET, b"binary.dat")
            .expect("Failed to get");

        assert!(retrieved.is_some());
    }

    #[test]
    fn test_same_content_same_hash() {
        let (_temp, db) = test_db_setup();

        // Save same content twice with different paths
        let content = b"identical content";

        let state1 = EntryState::new(content, None);
        db.set(
            ENTRY_STATE_BUCKET,
            b"file1.txt",
            &state1.to_bytes().unwrap(),
        )
        .expect("Failed to save file1");

        let state2 = EntryState::new(content, None);
        db.set(
            ENTRY_STATE_BUCKET,
            b"file2.txt",
            &state2.to_bytes().unwrap(),
        )
        .expect("Failed to save file2");

        let bytes1 = db
            .get(ENTRY_STATE_BUCKET, b"file1.txt")
            .expect("Failed to get file1")
            .expect("Entry not found");
        let hash1 = EntryState::from_bytes(&bytes1)
            .expect("Failed to deserialize")
            .content_hash;

        let bytes2 = db
            .get(ENTRY_STATE_BUCKET, b"file2.txt")
            .expect("Failed to get file2")
            .expect("Entry not found");
        let hash2 = EntryState::from_bytes(&bytes2)
            .expect("Failed to deserialize")
            .content_hash;

        // Same content should produce same hash
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_mode_values() {
        let (_temp, db) = test_db_setup();

        let mode_values = [
            0o000, // No permissions
            0o400, // Read only
            0o644, // Standard file
            0o755, // Executable
            0o777, // All permissions
        ];

        for (i, mode) in mode_values.iter().enumerate() {
            let path = format!("file_mode_{i}.txt");
            let state = EntryState::new(b"content", Some(*mode));
            db.set(
                ENTRY_STATE_BUCKET,
                path.as_bytes(),
                &state.to_bytes().unwrap(),
            )
            .expect("Failed to save");

            let bytes = db
                .get(ENTRY_STATE_BUCKET, path.as_bytes())
                .expect("Failed to get")
                .expect("Entry not found");

            let retrieved = EntryState::from_bytes(&bytes).expect("Failed to deserialize");
            assert_eq!(retrieved.mode, Some(*mode));
        }
    }

    #[test]
    #[serial]
    fn test_db_path_contains_state_db() {
        let path = get_db_path().expect("Failed to get db path");
        let path_str = path.to_string_lossy();

        // Should end with state.db
        assert!(path_str.ends_with("state.db"));

        // Should be in a guisu-related directory
        assert!(path_str.contains("guisu") || path_str.contains(".local/state"));
    }

    #[test]
    #[serial]
    fn test_reopen_after_close() {
        let _ = close_db();

        // Open database
        open_db().expect("First open failed");

        // Save data
        save_entry_state("test.txt", b"content", None).expect("Failed to save");

        // Close database
        close_db().expect("Failed to close");

        // Reopen database
        open_db().expect("Reopen failed");

        // Data should still be there
        let result = get_entry_state("test.txt").expect("Failed to get after reopen");
        assert!(result.is_some());

        let _ = close_db();
    }

    #[test]
    fn test_save_get_delete_cycle() {
        let (_temp, db) = test_db_setup();

        let path = b"cycle.txt";

        // Initial save
        let state1 = EntryState::new(b"content1", Some(0o644));
        db.set(ENTRY_STATE_BUCKET, path, &state1.to_bytes().unwrap())
            .expect("Failed to save 1");
        assert!(
            db.get(ENTRY_STATE_BUCKET, path)
                .expect("Get 1 failed")
                .is_some()
        );

        // Delete
        db.delete(ENTRY_STATE_BUCKET, path)
            .expect("Delete 1 failed");
        assert!(
            db.get(ENTRY_STATE_BUCKET, path)
                .expect("Get 2 failed")
                .is_none()
        );

        // Save again
        let state2 = EntryState::new(b"content2", Some(0o600));
        db.set(ENTRY_STATE_BUCKET, path, &state2.to_bytes().unwrap())
            .expect("Failed to save 2");
        assert!(
            db.get(ENTRY_STATE_BUCKET, path)
                .expect("Get 3 failed")
                .is_some()
        );

        // Delete again
        db.delete(ENTRY_STATE_BUCKET, path)
            .expect("Delete 2 failed");
        assert!(
            db.get(ENTRY_STATE_BUCKET, path)
                .expect("Get 4 failed")
                .is_none()
        );
    }

    #[test]
    fn test_very_long_path() {
        let (_temp, db) = test_db_setup();

        // Create a very long path (but not exceeding filesystem limits)
        let long_path = "a/".repeat(100) + "file.txt";

        let state = EntryState::new(b"content", None);
        db.set(
            ENTRY_STATE_BUCKET,
            long_path.as_bytes(),
            &state.to_bytes().unwrap(),
        )
        .expect("Failed to save long path");

        let retrieved = db
            .get(ENTRY_STATE_BUCKET, long_path.as_bytes())
            .expect("Failed to get long path");
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_path_with_dots() {
        let (_temp, db) = test_db_setup();

        let paths = vec![
            ".hidden",
            "dir/.hidden",
            "../relative",
            "./current",
            "...multiple",
        ];

        for path in paths {
            let state = EntryState::new(b"content", None);
            db.set(
                ENTRY_STATE_BUCKET,
                path.as_bytes(),
                &state.to_bytes().unwrap(),
            )
            .unwrap_or_else(|_| panic!("Failed to save {path}"));

            let result = db
                .get(ENTRY_STATE_BUCKET, path.as_bytes())
                .unwrap_or_else(|_| panic!("Failed to get {path}"));
            assert!(result.is_some(), "Entry not found: {path}");
        }
    }

    #[test]
    fn test_overwrite_with_different_mode() {
        let (_temp, db) = test_db_setup();

        let path = b"file.txt";

        // Save with mode 0o644
        let state1 = EntryState::new(b"content1", Some(0o644));
        db.set(ENTRY_STATE_BUCKET, path, &state1.to_bytes().unwrap())
            .expect("Failed to save");

        let bytes1 = db
            .get(ENTRY_STATE_BUCKET, path)
            .expect("Failed to get")
            .expect("Entry not found");
        let retrieved1 = EntryState::from_bytes(&bytes1).expect("Failed to deserialize");
        assert_eq!(retrieved1.mode, Some(0o644));

        // Overwrite with mode 0o755
        let state2 = EntryState::new(b"content2", Some(0o755));
        db.set(ENTRY_STATE_BUCKET, path, &state2.to_bytes().unwrap())
            .expect("Failed to overwrite");

        let bytes2 = db
            .get(ENTRY_STATE_BUCKET, path)
            .expect("Failed to get after overwrite")
            .expect("Entry not found");
        let retrieved2 = EntryState::from_bytes(&bytes2).expect("Failed to deserialize");
        assert_eq!(retrieved2.mode, Some(0o755));
    }

    #[test]
    fn test_overwrite_with_none_mode() {
        let (_temp, db) = test_db_setup();

        let path = b"file.txt";

        // Save with mode
        let state1 = EntryState::new(b"content1", Some(0o644));
        db.set(ENTRY_STATE_BUCKET, path, &state1.to_bytes().unwrap())
            .expect("Failed to save");

        // Overwrite with None mode
        let state2 = EntryState::new(b"content2", None);
        db.set(ENTRY_STATE_BUCKET, path, &state2.to_bytes().unwrap())
            .expect("Failed to overwrite");

        let bytes = db
            .get(ENTRY_STATE_BUCKET, path)
            .expect("Failed to get")
            .expect("Entry not found");
        let state = EntryState::from_bytes(&bytes).expect("Failed to deserialize");
        assert_eq!(state.mode, None);
    }

    #[test]
    #[serial]
    fn test_get_db_returns_same_instance() {
        let db1 = get_db().expect("Failed to get db 1");
        let db2 = get_db().expect("Failed to get db 2");

        // Both should point to the same Arc
        assert!(Arc::ptr_eq(&db1, &db2));
    }

    #[test]
    fn test_multiple_deletes_same_path() {
        let (_temp, db) = test_db_setup();

        let path = b"file.txt";

        let state = EntryState::new(b"content", None);
        db.set(ENTRY_STATE_BUCKET, path, &state.to_bytes().unwrap())
            .expect("Failed to save");

        // First delete
        db.delete(ENTRY_STATE_BUCKET, path)
            .expect("First delete failed");
        assert!(
            db.get(ENTRY_STATE_BUCKET, path)
                .expect("Get failed")
                .is_none()
        );

        // Second delete (should not error)
        db.delete(ENTRY_STATE_BUCKET, path)
            .expect("Second delete failed");
        assert!(
            db.get(ENTRY_STATE_BUCKET, path)
                .expect("Get failed")
                .is_none()
        );
    }

    #[test]
    fn test_save_many_entries() {
        let (_temp, db) = test_db_setup();

        // Save 100 entries
        for i in 0..100 {
            let path = format!("file_{i}.txt");
            let content = format!("content {i}");
            let state = EntryState::new(content.as_bytes(), Some(0o644));
            db.set(
                ENTRY_STATE_BUCKET,
                path.as_bytes(),
                &state.to_bytes().unwrap(),
            )
            .unwrap_or_else(|_| panic!("Failed to save {i}"));
        }

        // Verify all were saved
        for i in 0..100 {
            let path = format!("file_{i}.txt");
            let result = db
                .get(ENTRY_STATE_BUCKET, path.as_bytes())
                .unwrap_or_else(|_| panic!("Failed to get {i}"));
            assert!(result.is_some(), "Entry {i} not found");
        }
    }

    #[test]
    fn test_hash_changes_on_content_change_only() {
        let (_temp, db) = test_db_setup();

        let path = b"file.txt";

        // Save with mode 0o644
        let state1 = EntryState::new(b"content", Some(0o644));
        db.set(ENTRY_STATE_BUCKET, path, &state1.to_bytes().unwrap())
            .expect("Failed to save");
        let bytes1 = db
            .get(ENTRY_STATE_BUCKET, path)
            .expect("Failed to get")
            .expect("Entry not found");
        let hash1 = EntryState::from_bytes(&bytes1)
            .expect("Failed to deserialize")
            .content_hash;

        // Save with different mode but same content
        let state2 = EntryState::new(b"content", Some(0o755));
        db.set(ENTRY_STATE_BUCKET, path, &state2.to_bytes().unwrap())
            .expect("Failed to save");
        let bytes2 = db
            .get(ENTRY_STATE_BUCKET, path)
            .expect("Failed to get")
            .expect("Entry not found");
        let hash2 = EntryState::from_bytes(&bytes2)
            .expect("Failed to deserialize")
            .content_hash;

        // Hash should be same (only content matters)
        assert_eq!(hash1, hash2);

        // Save with different content
        let state3 = EntryState::new(b"different content", Some(0o755));
        db.set(ENTRY_STATE_BUCKET, path, &state3.to_bytes().unwrap())
            .expect("Failed to save");
        let bytes3 = db
            .get(ENTRY_STATE_BUCKET, path)
            .expect("Failed to get")
            .expect("Entry not found");
        let hash3 = EntryState::from_bytes(&bytes3)
            .expect("Failed to deserialize")
            .content_hash;

        // Hash should be different
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_db_path_consistency() {
        let (_temp, _db) = test_db_setup();

        let path1 = get_db_path().expect("Failed to get path 1");
        let path2 = get_db_path().expect("Failed to get path 2");

        assert_eq!(path1, path2);
        assert!(path1.to_string_lossy().contains("state.db"));
    }
}
