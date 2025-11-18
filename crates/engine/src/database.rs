//! Database helper for persistent state storage
//!
//! This module provides a singleton database instance stored in XDG state directory.

use crate::state::{ENTRY_STATE_BUCKET, EntryState, PersistentState, RedbPersistentState};
use guisu_config::dirs;
use guisu_core::{Error, Result};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use tracing::warn;

/// Global database instance
static DB_INSTANCE: OnceLock<Arc<Mutex<Option<RedbPersistentState>>>> = OnceLock::new();

/// Get the database path in XDG state directory
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
pub fn get_db() -> Result<Arc<Mutex<Option<RedbPersistentState>>>> {
    Ok(Arc::clone(
        DB_INSTANCE.get_or_init(|| Arc::new(Mutex::new(None))),
    ))
}

/// Open the database (creates if doesn't exist)
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
    let mut guard = db_instance.lock().unwrap_or_else(|poisoned| {
        warn!("Database lock was poisoned during open, attempting recovery");
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
pub fn save_entry_state(path: &str, content: &[u8], mode: Option<u32>) -> Result<()> {
    let db_instance = get_db()?;
    let guard = db_instance.lock().unwrap_or_else(|poisoned| {
        warn!("Database lock was poisoned during save, recovering");
        poisoned.into_inner()
    });

    if let Some(db) = guard.as_ref() {
        let state = EntryState::new(content, mode);
        db.set(ENTRY_STATE_BUCKET, path.as_bytes(), &state.to_bytes())
            .map_err(|e| Error::State(format!("Failed to save state for {}: {}", path, e)))?;
    } else {
        return Err(Error::State("Database not opened".to_string()));
    }

    Ok(())
}

/// Get entry state from database
pub fn get_entry_state(path: &str) -> Result<Option<EntryState>> {
    let db_instance = get_db()?;
    let guard = db_instance.lock().unwrap_or_else(|poisoned| {
        warn!("Database lock was poisoned during get, recovering");
        poisoned.into_inner()
    });

    if let Some(db) = guard.as_ref() {
        let bytes = db
            .get(ENTRY_STATE_BUCKET, path.as_bytes())
            .map_err(|e| Error::State(format!("Failed to get state for {}: {}", path, e)))?;

        Ok(bytes.and_then(|b| EntryState::from_bytes(&b)))
    } else {
        Err(Error::State("Database not opened".to_string()))
    }
}

/// Delete entry state from database
pub fn delete_entry_state(path: &str) -> Result<()> {
    let db_instance = get_db()?;
    let guard = db_instance.lock().unwrap_or_else(|poisoned| {
        warn!("Database lock was poisoned during delete, recovering");
        poisoned.into_inner()
    });

    if let Some(db) = guard.as_ref() {
        db.delete(ENTRY_STATE_BUCKET, path.as_bytes())
            .map_err(|e| Error::State(format!("Failed to delete state for {}: {}", path, e)))?;
    }

    Ok(())
}

/// Close the database
pub fn close_db() -> Result<()> {
    let db_instance = get_db()?;
    let mut guard = db_instance.lock().unwrap_or_else(|poisoned| {
        warn!("Database lock was poisoned during close, recovering");
        poisoned.into_inner()
    });

    if let Some(db) = guard.take() {
        db.close()?;
    }

    Ok(())
}
