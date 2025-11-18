//! State management for dotfiles
//!
//! Provides state tracking for source, target, destination, and persistent states.

use crate::attr::FileAttributes;
use crate::entry::{DestEntry, SourceEntry, TargetEntry};
use crate::error::{Error, Result};
use crate::processor::ContentProcessor;
use crate::system::System;
use guisu_core::path::{AbsPath, RelPath, SourceRelPath};
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::RwLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use subtle::ConstantTimeEq;
use walkdir::WalkDir;

/// Custom serde module for SystemTime serialization
mod systemtime_serde {
    use super::*;

    pub fn serialize<S>(
        time: &Option<SystemTime>,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match time {
            Some(t) => {
                let duration = t
                    .duration_since(UNIX_EPOCH)
                    .map_err(serde::ser::Error::custom)?;
                serializer.serialize_some(&duration.as_secs())
            }
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> std::result::Result<Option<SystemTime>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs: Option<u64> = Option::deserialize(deserializer)?;
        Ok(secs.map(|s| UNIX_EPOCH + Duration::from_secs(s)))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
pub struct HookState {
    #[serde(with = "systemtime_serde")]
    #[bincode(with_serde)]
    pub last_executed: Option<std::time::SystemTime>,
    /// SHA256 hash of the hooks directory content
    pub content_hash: Option<Vec<u8>>,
    /// Names of hooks that have been executed with mode=once
    /// These hooks will never be executed again unless state is reset
    #[serde(default)]
    pub once_executed: std::collections::HashSet<String>,
    /// Content hashes for hooks with mode=onchange
    /// Maps hook name to SHA256 hash of its content (cmd or script)
    #[serde(default)]
    pub onchange_hashes: std::collections::HashMap<String, Vec<u8>>,
}

impl HookState {
    /// Create new hook state
    pub fn new() -> Self {
        Self {
            last_executed: None,
            content_hash: None,
            once_executed: std::collections::HashSet::new(),
            onchange_hashes: std::collections::HashMap::new(),
        }
    }

    /// Check if a hook with mode=once has already been executed
    pub fn has_executed_once(&self, hook_name: &str) -> bool {
        self.once_executed.contains(hook_name)
    }

    /// Mark a hook with mode=once as executed
    pub fn mark_executed_once(&mut self, hook_name: String) {
        self.once_executed.insert(hook_name);
    }

    /// Check if a hook's content has changed (for mode=onchange)
    ///
    /// Returns true if:
    /// - No hash is stored (first run)
    /// - The stored hash differs from the provided hash
    pub fn hook_content_changed(&self, hook_name: &str, content_hash: &[u8]) -> bool {
        match self.onchange_hashes.get(hook_name) {
            None => true, // First run
            Some(stored_hash) => !bool::from(stored_hash.ct_eq(content_hash)),
        }
    }

    /// Update the content hash for a hook with mode=onchange
    pub fn update_onchange_hash(&mut self, hook_name: String, content_hash: Vec<u8>) {
        self.onchange_hashes.insert(hook_name, content_hash);
    }

    /// Update the state from a hooks directory
    ///
    /// This computes a hash of all files in the hooks directory and updates
    /// the last_executed timestamp.
    pub fn update(&mut self, hooks_dir: &Path) -> Result<()> {
        self.content_hash = Some(Self::compute_directory_hash(hooks_dir)?);
        self.last_executed = Some(std::time::SystemTime::now());
        Ok(())
    }

    /// Check if hooks directory has changed
    ///
    /// Compares the current directory hash with the stored hash.
    /// Returns true if:
    /// - The directory doesn't exist
    /// - No hash is stored (first run)
    /// - The hash has changed
    pub fn has_changed(&self, hooks_dir: &Path) -> Result<bool> {
        // If directory doesn't exist, consider it unchanged
        if !hooks_dir.exists() {
            return Ok(false);
        }

        // If we have no stored hash, consider it changed (first run)
        let Some(stored_hash) = &self.content_hash else {
            return Ok(true);
        };

        // Compute current hash and compare using constant-time comparison
        // to prevent timing side-channel attacks
        let current_hash = Self::compute_directory_hash(hooks_dir)?;
        Ok(!bool::from(current_hash.ct_eq(stored_hash)))
    }

    /// Compute a hash of all files in a directory
    ///
    /// This creates a combined hash by:
    /// 1. Collecting all file paths (sequential - required by WalkDir)
    /// 2. Reading files and computing hashes in parallel
    /// 3. Sorting by path for deterministic ordering
    /// 4. Computing a final combined hash
    fn compute_directory_hash(dir: &Path) -> Result<Vec<u8>> {
        use rayon::prelude::*;

        // First pass: collect all file paths (must be sequential due to WalkDir)
        let file_paths: Vec<std::path::PathBuf> = WalkDir::new(dir)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .map(|e| e.path().to_path_buf())
            .collect();

        // Second pass: parallel processing of files (read + hash)
        // This is where the performance benefit comes from for large hook directories
        let file_hashes: Result<Vec<(String, Vec<u8>)>> = file_paths
            .par_iter()
            .map(|path| {
                // Get relative path
                let rel_path = path
                    .strip_prefix(dir)
                    .map_err(|_| Error::InvalidConfig {
                        message: format!("Invalid path in hooks directory: {}", path.display()),
                    })?
                    .to_string_lossy()
                    .to_string();

                // Read file content and compute hash
                let content = fs::read(path).map_err(|e| Error::InvalidConfig {
                    message: format!("Failed to read hook file {}: {}", path.display(), e),
                })?;

                let file_hash = hash_data(&content);
                Ok((rel_path, file_hash))
            })
            .collect();

        let mut file_hashes = file_hashes?;

        // Sort by path for deterministic hashing
        file_hashes.sort_by(|a, b| a.0.cmp(&b.0));

        // Combine all hashes into a single hash
        let mut hasher = Sha256::new();
        for (path, hash) in file_hashes {
            hasher.update(path.as_bytes());
            hasher.update(&hash);
        }

        Ok(hasher.finalize().to_vec())
    }

    /// Serialize to bytes for database storage using bincode
    pub fn to_bytes(&self) -> Vec<u8> {
        bincode::encode_to_vec(self, bincode::config::standard())
            .expect("HookState serialization should never fail")
    }

    /// Deserialize from bytes using bincode
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        bincode::decode_from_slice(bytes, bincode::config::standard())
            .ok()
            .map(|(state, _len)| state)
    }
}

impl Default for HookState {
    fn default() -> Self {
        Self::new()
    }
}

/// Hook state persistence wrapper
pub struct HookStatePersistence<'a, T: PersistentState> {
    db: &'a T,
}

impl<'a, T: PersistentState> HookStatePersistence<'a, T> {
    /// Create new hook state persistence
    pub fn new(db: &'a T) -> Self {
        Self { db }
    }

    /// Load hook state from database
    ///
    /// Returns a new HookState if no state is stored.
    pub fn load(&self) -> Result<HookState> {
        const HOOK_STATE_KEY: &[u8] = b"hooks";

        match self.db.get(HOOK_STATE_BUCKET, HOOK_STATE_KEY)? {
            Some(bytes) => HookState::from_bytes(&bytes).ok_or_else(|| {
                Error::State("Failed to deserialize hook state from database".to_string())
            }),
            None => Ok(HookState::new()),
        }
    }

    /// Save hook state to database
    pub fn save(&self, state: &HookState) -> Result<()> {
        const HOOK_STATE_KEY: &[u8] = b"hooks";

        let bytes = state.to_bytes();
        self.db.set(HOOK_STATE_BUCKET, HOOK_STATE_KEY, &bytes)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct DestinationState {
    /// Root directory (typically home directory)
    root: AbsPath,

    /// Cached entries
    cache: HashMap<RelPath, DestEntry>,
}

impl DestinationState {
    /// Create a new destination state
    pub fn new(root: AbsPath) -> Self {
        Self {
            root,
            cache: HashMap::new(),
        }
    }

    /// Get the root directory
    pub fn root(&self) -> &AbsPath {
        &self.root
    }

    /// Read the current state of a file from the filesystem
    ///
    /// This reads the actual file and caches the result.
    pub fn read<S: System>(&mut self, path: &RelPath, system: &S) -> Result<&DestEntry> {
        if !self.cache.contains_key(path) {
            let abs_path = self.root.join(path);
            let entry = self.read_entry(path, &abs_path, system)?;
            self.cache.insert(path.clone(), entry);
        }

        Ok(self
            .cache
            .get(path)
            .expect("entry was just inserted into cache"))
    }

    /// Read an entry from the filesystem
    fn read_entry<S: System>(
        &self,
        rel_path: &RelPath,
        abs_path: &AbsPath,
        system: &S,
    ) -> Result<DestEntry> {
        if !system.exists(abs_path) {
            return Ok(DestEntry::missing(rel_path.clone()));
        }

        let metadata = system.metadata(abs_path)?;

        if metadata.is_dir() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = Some(metadata.permissions().mode());
                Ok(DestEntry::directory(rel_path.clone(), mode))
            }

            #[cfg(not(unix))]
            {
                Ok(DestEntry::directory(rel_path.clone(), None))
            }
        } else if metadata.is_symlink() {
            let target = system.read_link(abs_path)?;
            Ok(DestEntry::symlink(rel_path.clone(), target))
        } else {
            let content = system.read_file(abs_path)?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = Some(metadata.permissions().mode());
                Ok(DestEntry::file(rel_path.clone(), content, mode))
            }

            #[cfg(not(unix))]
            {
                Ok(DestEntry::file(rel_path.clone(), content, None))
            }
        }
    }

    /// Get a cached entry
    pub fn get(&self, path: &RelPath) -> Option<&DestEntry> {
        self.cache.get(path)
    }

    /// Clear the cache
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Metadata {
    /// Files that should only be created once and not tracked afterwards
    #[serde(default, rename = "create-once")]
    pub create_once: CreateOnceConfig,
}

/// Configuration for create-once files
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CreateOnceConfig {
    /// List of file paths (relative to destination) that should only be created once
    #[serde(default)]
    pub files: HashSet<String>,
}

impl Metadata {
    /// Load state from `.guisu/state.toml`
    pub fn load(source_dir: &Path) -> Result<Self> {
        let metadata_path = source_dir.join(".guisu/state.toml");

        if !metadata_path.exists() {
            return Ok(Self::default());
        }

        // Make path absolute for better error messages
        let abs_metadata_path =
            fs::canonicalize(&metadata_path).unwrap_or_else(|_| metadata_path.clone());

        let content = fs::read_to_string(&metadata_path).map_err(|e| {
            // Safe to unwrap since we canonicalized it above
            let abs_path = guisu_core::path::AbsPath::new(abs_metadata_path.clone())
                .expect("canonicalized path should be absolute");
            Error::FileRead {
                path: abs_path,
                source: e,
            }
        })?;

        toml::from_str(&content).map_err(|e| Error::InvalidConfig {
            message: format!("Failed to parse .guisu/state.toml: {}", e),
        })
    }

    /// Save state to `.guisu/state.toml`
    pub fn save(&self, source_dir: &Path) -> Result<()> {
        let guisu_dir = source_dir.join(".guisu");

        // Make paths absolute for better error messages
        let abs_guisu_dir = fs::canonicalize(source_dir)
            .map(|p| p.join(".guisu"))
            .unwrap_or_else(|_| guisu_dir.clone());

        // Create .guisu directory if it doesn't exist
        if !guisu_dir.exists() {
            fs::create_dir_all(&guisu_dir).map_err(|e| {
                let abs_path = guisu_core::path::AbsPath::new(abs_guisu_dir.clone())
                    .expect("canonicalized path should be absolute");
                Error::DirectoryCreate {
                    path: abs_path,
                    source: e,
                }
            })?;
        }

        let metadata_path = guisu_dir.join("state.toml");
        let abs_metadata_path = abs_guisu_dir.join("state.toml");

        let content = toml::to_string_pretty(self).map_err(|e| Error::InvalidConfig {
            message: format!("Failed to serialize metadata: {}", e),
        })?;

        fs::write(&metadata_path, content).map_err(|e| {
            let abs_path = guisu_core::path::AbsPath::new(abs_metadata_path.clone())
                .expect("canonicalized path should be absolute");
            Error::FileWrite {
                path: abs_path,
                source: e,
            }
        })?;

        Ok(())
    }

    /// Add a file to the create-once list
    pub fn add_create_once(&mut self, file_path: String) {
        self.create_once.files.insert(file_path);
    }

    /// Check if a file is in the create-once list
    pub fn is_create_once(&self, file_path: &str) -> bool {
        self.create_once.files.contains(file_path)
    }

    /// Remove a file from the create-once list
    pub fn remove_create_once(&mut self, file_path: &str) -> bool {
        self.create_once.files.remove(file_path)
    }
}

pub const ENTRY_STATE_BUCKET: &str = "entryState";
pub const SCRIPT_STATE_BUCKET: &str = "scriptState";
pub const CONFIG_STATE_BUCKET: &str = "configState";
pub const PACKAGE_STATE_BUCKET: &str = "packageState";
pub const HOOK_STATE_BUCKET: &str = "hookState";

/// Trait for persistent state storage
pub trait PersistentState: Send + Sync {
    /// Get a value from a bucket
    fn get(&self, bucket: &str, key: &[u8]) -> Result<Option<Vec<u8>>>;

    /// Set a value in a bucket
    fn set(&self, bucket: &str, key: &[u8], value: &[u8]) -> Result<()>;

    /// Delete a key from a bucket
    fn delete(&self, bucket: &str, key: &[u8]) -> Result<()>;

    /// Delete an entire bucket
    fn delete_bucket(&self, bucket: &str) -> Result<()>;

    /// Iterate over all key-value pairs in a bucket
    fn for_each<F>(&self, bucket: &str, f: F) -> Result<()>
    where
        F: FnMut(&[u8], &[u8]) -> Result<()>;

    /// Close the database
    fn close(self) -> Result<()>;
}

/// Persistent state implementation using redb
///
/// # Thread Safety
///
/// While `RedbPersistentState` is `Send + Sync` and can be shared across threads,
/// concurrent write operations are serialized internally by redb.
///
/// For application-level access control, use the singleton pattern in `database.rs`
/// which wraps this in `Arc<Mutex<Option<RedbPersistentState>>>` to ensure
/// exclusive access during operations.
pub struct RedbPersistentState {
    db: Database,
}

// Static assertions to ensure thread safety
const _: () = {
    const fn assert_send<T: Send>() {}
    const fn assert_sync<T: Sync>() {}

    let _ = assert_send::<RedbPersistentState>;
    let _ = assert_sync::<RedbPersistentState>;
};

impl RedbPersistentState {
    /// Create or open a persistent state database
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let db = Database::create(path)
            .map_err(|e| crate::Error::State(format!("Failed to create database: {}", e)))?;
        Ok(Self { db })
    }

    /// Open in read-only mode
    pub fn read_only(path: impl AsRef<Path>) -> Result<Self> {
        let db = Database::open(path)
            .map_err(|e| crate::Error::State(format!("Failed to open database: {}", e)))?;
        Ok(Self { db })
    }

    /// Create table definition, avoiding String allocation for known buckets
    ///
    /// Returns a tuple of (String, TableDefinition) where String is only populated
    /// for unknown buckets (to satisfy 'static lifetime requirement).
    #[inline]
    fn table_def_with_storage(
        bucket: &str,
    ) -> (
        Option<String>,
        TableDefinition<'_, &'static [u8], &'static [u8]>,
    ) {
        // For known buckets, use static strings to avoid allocation
        match bucket {
            ENTRY_STATE_BUCKET => (None, TableDefinition::new(ENTRY_STATE_BUCKET)),
            SCRIPT_STATE_BUCKET => (None, TableDefinition::new(SCRIPT_STATE_BUCKET)),
            CONFIG_STATE_BUCKET => (None, TableDefinition::new(CONFIG_STATE_BUCKET)),
            PACKAGE_STATE_BUCKET => (None, TableDefinition::new(PACKAGE_STATE_BUCKET)),
            HOOK_STATE_BUCKET => (None, TableDefinition::new(HOOK_STATE_BUCKET)),
            // For unknown buckets, allocate String and leak to satisfy 'static
            _ => {
                let bucket_string = bucket.to_string();
                let leaked: &'static str = Box::leak(bucket_string.clone().into_boxed_str());
                (Some(bucket_string), TableDefinition::new(leaked))
            }
        }
    }
}

impl PersistentState for RedbPersistentState {
    fn get(&self, bucket: &str, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let read_txn = self
            .db
            .begin_read()
            .map_err(|e| crate::Error::State(format!("Failed to begin read transaction: {}", e)))?;
        let (_storage, table_def) = Self::table_def_with_storage(bucket);

        let table = match read_txn.open_table(table_def) {
            Ok(t) => t,
            Err(_) => return Ok(None), // Table doesn't exist yet
        };

        match table.get(key) {
            Ok(Some(value)) => Ok(Some(value.value().to_vec())),
            Ok(None) => Ok(None),
            Err(e) => Err(crate::Error::State(format!("Failed to get value: {}", e))),
        }
    }

    fn set(&self, bucket: &str, key: &[u8], value: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write().map_err(|e| {
            crate::Error::State(format!("Failed to begin write transaction: {}", e))
        })?;
        {
            let (_storage, table_def) = Self::table_def_with_storage(bucket);
            let mut table = write_txn
                .open_table(table_def)
                .map_err(|e| crate::Error::State(format!("Failed to open table: {}", e)))?;
            table
                .insert(key, value)
                .map_err(|e| crate::Error::State(format!("Failed to insert value: {}", e)))?;
        }
        write_txn
            .commit()
            .map_err(|e| crate::Error::State(format!("Failed to commit transaction: {}", e)))?;
        Ok(())
    }

    fn delete(&self, bucket: &str, key: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write().map_err(|e| {
            crate::Error::State(format!("Failed to begin write transaction: {}", e))
        })?;
        {
            let (_storage, table_def) = Self::table_def_with_storage(bucket);
            let mut table = write_txn
                .open_table(table_def)
                .map_err(|e| crate::Error::State(format!("Failed to open table: {}", e)))?;
            table
                .remove(key)
                .map_err(|e| crate::Error::State(format!("Failed to remove value: {}", e)))?;
        }
        write_txn
            .commit()
            .map_err(|e| crate::Error::State(format!("Failed to commit transaction: {}", e)))?;
        Ok(())
    }

    fn delete_bucket(&self, bucket: &str) -> Result<()> {
        let write_txn = self.db.begin_write().map_err(|e| {
            crate::Error::State(format!("Failed to begin write transaction: {}", e))
        })?;
        let (_storage, table_def) = Self::table_def_with_storage(bucket);
        write_txn
            .delete_table(table_def)
            .map_err(|e| crate::Error::State(format!("Failed to delete table: {}", e)))?;
        write_txn
            .commit()
            .map_err(|e| crate::Error::State(format!("Failed to commit transaction: {}", e)))?;
        Ok(())
    }

    fn for_each<F>(&self, bucket: &str, mut f: F) -> Result<()>
    where
        F: FnMut(&[u8], &[u8]) -> Result<()>,
    {
        let read_txn = self
            .db
            .begin_read()
            .map_err(|e| crate::Error::State(format!("Failed to begin read transaction: {}", e)))?;
        let (_storage, table_def) = Self::table_def_with_storage(bucket);

        let table = match read_txn.open_table(table_def) {
            Ok(t) => t,
            Err(_) => return Ok(()), // No bucket yet
        };

        let iter = table
            .iter()
            .map_err(|e| crate::Error::State(format!("Failed to iterate table: {}", e)))?;

        for item in iter {
            let (key, value) =
                item.map_err(|e| crate::Error::State(format!("Failed to read item: {}", e)))?;
            f(key.value(), value.value())?;
        }

        Ok(())
    }

    fn close(self) -> Result<()> {
        // redb closes automatically when dropped
        drop(self.db);
        Ok(())
    }
}

/// Compute SHA256 hash of data
pub fn hash_data(data: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

/// Entry state - tracks file state
#[derive(Debug, Clone, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub struct EntryState {
    pub content_hash: Vec<u8>,
    pub mode: Option<u32>,
}

impl EntryState {
    /// Create a new entry state from content and mode
    pub fn new(content: &[u8], mode: Option<u32>) -> Self {
        Self {
            content_hash: hash_data(content),
            mode,
        }
    }

    /// Serialize to bytes using bincode
    pub fn to_bytes(&self) -> Vec<u8> {
        bincode::encode_to_vec(self, bincode::config::standard())
            .expect("EntryState serialization should never fail")
    }

    /// Deserialize from bytes using bincode
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        bincode::decode_from_slice(bytes, bincode::config::standard())
            .ok()
            .map(|(state, _len)| state)
    }
}

/// Script state - tracks script execution
#[derive(Debug, Clone, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub struct ScriptState {
    pub content_hash: Vec<u8>,
}

impl ScriptState {
    /// Create a new script state from content
    pub fn new(content: &[u8]) -> Self {
        Self {
            content_hash: hash_data(content),
        }
    }

    /// Serialize to bytes using bincode
    pub fn to_bytes(&self) -> Vec<u8> {
        bincode::encode_to_vec(self, bincode::config::standard())
            .expect("ScriptState serialization should never fail")
    }

    /// Deserialize from bytes using bincode
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        bincode::decode_from_slice(bytes, bincode::config::standard())
            .ok()
            .map(|(state, _len)| state)
    }
}

/// Type aliases for mock state data structure
/// Inner map: key-value pairs within a bucket
type BucketData = HashMap<Vec<u8>, Vec<u8>>;
/// Outer map: bucket name -> bucket data
type StateData = HashMap<String, BucketData>;

/// Mock persistent state for testing
pub struct MockPersistentState {
    data: RwLock<StateData>,
}

impl MockPersistentState {
    /// Create a new mock persistent state
    pub fn new() -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for MockPersistentState {
    fn default() -> Self {
        Self::new()
    }
}

impl PersistentState for MockPersistentState {
    fn get(&self, bucket: &str, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let data = self
            .data
            .read()
            .expect("MockPersistentState lock should not be poisoned");
        Ok(data.get(bucket).and_then(|b| b.get(key).cloned()))
    }

    fn set(&self, bucket: &str, key: &[u8], value: &[u8]) -> Result<()> {
        let mut data = self
            .data
            .write()
            .expect("MockPersistentState lock should not be poisoned");
        data.entry(bucket.to_string())
            .or_default()
            .insert(key.to_vec(), value.to_vec());
        Ok(())
    }

    fn delete(&self, bucket: &str, key: &[u8]) -> Result<()> {
        let mut data = self
            .data
            .write()
            .expect("MockPersistentState lock should not be poisoned");
        if let Some(bucket_data) = data.get_mut(bucket) {
            bucket_data.remove(key);
        }
        Ok(())
    }

    fn delete_bucket(&self, bucket: &str) -> Result<()> {
        let mut data = self
            .data
            .write()
            .expect("MockPersistentState lock should not be poisoned");
        data.remove(bucket);
        Ok(())
    }

    fn for_each<F>(&self, bucket: &str, mut f: F) -> Result<()>
    where
        F: FnMut(&[u8], &[u8]) -> Result<()>,
    {
        let data = self
            .data
            .read()
            .expect("MockPersistentState lock should not be poisoned");
        if let Some(bucket_data) = data.get(bucket) {
            for (k, v) in bucket_data {
                f(k, v)?;
            }
        }
        Ok(())
    }

    fn close(self) -> Result<()> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct SourceState {
    /// Root directory of the source files
    root: AbsPath,

    /// Map of target paths to source entries
    entries: HashMap<RelPath, SourceEntry>,
}

impl SourceState {
    /// Read the source state from a directory
    ///
    /// Preserves original filenames and uses file extensions and permissions.
    ///
    /// # Arguments
    ///
    /// * `root` - The root directory to read from
    /// * `matcher` - Optional ignore matcher to filter files
    pub fn read(root: AbsPath) -> Result<Self> {
        Self::read_with_matcher(root, None)
    }

    /// Read the source state from a directory with ignore matcher
    ///
    /// This version allows filtering files using an IgnoreMatcher.
    ///
    /// # Arguments
    ///
    /// * `root` - The root directory to read from
    /// * `matcher` - Optional ignore matcher to filter files based on patterns
    pub fn read_with_matcher(
        root: AbsPath,
        matcher: Option<&guisu_config::IgnoreMatcher>,
    ) -> Result<Self> {
        use rayon::prelude::*;

        let root_path = root.as_path();

        // First, collect all file paths (WalkDir must be sequential)
        let file_paths: Vec<std::path::PathBuf> = WalkDir::new(root_path)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter_map(|entry| {
                let path = entry.path();

                // Skip the root directory itself
                if path == root_path {
                    return None;
                }

                // Only process files, not directories
                // Note: With rootEntry enforced (defaults to "home"), all dotfiles are in a
                // subdirectory, so we don't need to skip .git, .guisu, etc.
                if !entry.file_type().is_file() {
                    return None;
                }

                // Apply ignore matcher if provided
                if let Some(matcher) = matcher
                    && let Ok(rel_path) = path.strip_prefix(root_path)
                    && matcher.is_ignored(rel_path)
                {
                    return None;
                }

                Some(path.to_path_buf())
            })
            .collect();

        // Now process all files in parallel (metadata reading + attribute parsing)
        let entries: Result<Vec<_>> = file_paths
            .par_iter()
            .map(|path| {
                // Get relative path from root
                let rel_path =
                    path.strip_prefix(root_path)
                        .map_err(|_| Error::InvalidPathPrefix {
                            path: std::sync::Arc::new(path.to_path_buf()),
                            base: std::sync::Arc::new(root_path.to_path_buf()),
                        })?;

                let source_rel_path = SourceRelPath::new(rel_path.to_path_buf())?;

                // Parse attributes from filename
                let file_name = path
                    .file_name()
                    .ok_or_else(|| Error::InvalidConfig {
                        message: format!("Invalid path: {}", path.display()),
                    })?
                    .to_string_lossy();

                let metadata = std::fs::metadata(path).map_err(|e| Error::FileRead {
                    path: root.join(&source_rel_path.to_rel_path()),
                    source: e,
                })?;

                #[cfg(unix)]
                let permissions = {
                    use std::os::unix::fs::PermissionsExt;
                    Some(metadata.permissions().mode())
                };

                #[cfg(not(unix))]
                let permissions = None;

                let (attrs, target_name) =
                    FileAttributes::parse_from_source(&file_name, permissions)?;

                // Calculate target path
                let target_rel = if let Some(parent) = rel_path.parent() {
                    parent.join(&target_name)
                } else {
                    std::path::PathBuf::from(&target_name)
                };

                let target_path = RelPath::new(target_rel)?;

                let source_entry = SourceEntry::File {
                    source_path: source_rel_path,
                    target_path: target_path.clone(),
                    attributes: attrs,
                };

                Ok((target_path, source_entry))
            })
            .collect();

        let mut entry_map = HashMap::new();
        for (target_path, source_entry) in entries? {
            entry_map.insert(target_path, source_entry);
        }

        Ok(Self {
            root,
            entries: entry_map,
        })
    }

    /// Get all source entries
    pub fn entries(&self) -> impl Iterator<Item = &SourceEntry> {
        self.entries.values()
    }

    /// Get a source entry by target path
    pub fn get(&self, target_path: &RelPath) -> Option<&SourceEntry> {
        self.entries.get(target_path)
    }

    /// Get the root directory
    pub fn root(&self) -> &AbsPath {
        &self.root
    }

    /// Get the number of entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if there are no entries
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get the absolute path to a source file
    pub fn source_file_path(&self, source_path: &SourceRelPath) -> AbsPath {
        // Convert SourceRelPath to RelPath first, then join
        self.root.join(&source_path.to_rel_path())
    }
}

#[derive(Debug)]
pub struct TargetState {
    /// Map of target paths to target entries
    entries: HashMap<RelPath, TargetEntry>,
}

impl TargetState {
    /// Create a new empty target state
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Create a target state from a source state
    ///
    /// This processes all source entries through the content processor,
    /// applying template rendering and decryption as needed.
    ///
    /// # Arguments
    ///
    /// * `source` - The source state to process
    /// * `processor` - The content processor to use for transformations
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use guisu_engine::state::{SourceState, TargetState};
    /// use guisu_engine::processor::ContentProcessor;
    /// use guisu_core::path::AbsPath;
    /// use serde_json::json;
    ///
    /// let source_dir = AbsPath::new("/home/user/.local/share/guisu".into())?;
    /// let source = SourceState::read(source_dir)?;
    ///
    /// // Create processor with decryptor and renderer
    /// let processor = ContentProcessor::new(my_decryptor, my_renderer);
    /// let context = json!({});
    /// let target = TargetState::from_source(&source, &processor, &context)?;
    /// ```
    pub fn from_source<D, R>(
        source: &SourceState,
        processor: &ContentProcessor<D, R>,
        context: &serde_json::Value,
    ) -> Result<Self>
    where
        D: crate::content::Decryptor + Sync,
        R: crate::content::TemplateRenderer + Sync,
    {
        use rayon::prelude::*;

        // Parallel processing of source entries (template rendering + decryption are CPU-intensive)
        let entries: Result<Vec<_>> = source
            .entries()
            .par_bridge()
            .map(|source_entry| Self::process_entry(source, source_entry, processor, context))
            .collect();

        let mut target_state = Self::new();
        for entry in entries? {
            target_state.add(entry);
        }

        Ok(target_state)
    }

    /// Process a single source entry into a target entry
    ///
    /// This applies the appropriate transformations based on the entry type:
    /// - Files: Read contents, decrypt if needed, render templates if needed
    /// - Directories: Create directory entry with permissions
    /// - Symlinks: Create symlink entry (no content processing)
    fn process_entry<D, R>(
        source: &SourceState,
        source_entry: &SourceEntry,
        processor: &ContentProcessor<D, R>,
        context: &serde_json::Value,
    ) -> Result<TargetEntry>
    where
        D: crate::content::Decryptor,
        R: crate::content::TemplateRenderer,
    {
        match source_entry {
            SourceEntry::File {
                source_path,
                target_path,
                attributes,
            } => {
                // Get the absolute path to the source file
                let abs_source_path = source.source_file_path(source_path);

                // Process the file contents through the decrypt→render pipeline
                // Note: process_file already provides detailed error context,
                // so we don't wrap it here to avoid redundant error messages
                let content = processor.process_file(&abs_source_path, attributes, context)?;

                // Get the file mode from attributes
                let mode = attributes.mode();

                Ok(TargetEntry::File {
                    path: target_path.clone(),
                    content,
                    mode,
                })
            }

            SourceEntry::Directory {
                target_path,
                attributes,
                ..
            } => {
                // Directories don't have content processing
                let mode = attributes.mode();

                Ok(TargetEntry::Directory {
                    path: target_path.clone(),
                    mode,
                })
            }

            SourceEntry::Symlink {
                target_path,
                link_target,
                ..
            } => {
                // Symlinks don't have content processing currently
                // NOTE: Future enhancement - support templating in symlink targets
                // Chezmoi supports this via .tmpl suffix on symlink files
                // See CLAUDE.md: "Symlink Target Templating"
                Ok(TargetEntry::Symlink {
                    path: target_path.clone(),
                    target: link_target.clone(),
                })
            }
        }
    }

    /// Add an entry to the target state
    pub fn add(&mut self, entry: TargetEntry) {
        let path = entry.path().clone();
        self.entries.insert(path, entry);
    }

    /// Get a target entry by path
    pub fn get(&self, path: &RelPath) -> Option<&TargetEntry> {
        self.entries.get(path)
    }

    /// Iterate over all entries
    pub fn entries(&self) -> impl Iterator<Item = &TargetEntry> {
        self.entries.values()
    }

    /// Get the number of entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the target state is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for TargetState {
    fn default() -> Self {
        Self::new()
    }
}
