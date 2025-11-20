//! Hook configuration state tracking
//!
//! Tracks hook configuration file changes using SHA256 hashing.
//! This is separate from the execution state tracking in engine/state.rs.

use guisu_core::{Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::time::SystemTime;
use subtle::ConstantTimeEq;

/// Hook configuration state
///
/// This tracks changes to hook configuration files, not execution state.
/// For execution state (mode=once, mode=onchange), see engine/state.rs HookState.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookConfigState {
    /// SHA256 hash of the configuration file content
    pub config_hash: Vec<u8>,

    /// Last execution timestamp
    pub last_executed: SystemTime,
}

impl Default for HookConfigState {
    fn default() -> Self {
        Self {
            config_hash: Vec::new(),
            last_executed: SystemTime::UNIX_EPOCH,
        }
    }
}

impl HookConfigState {
    /// Create a new state with config hash
    pub fn new(config_path: &Path) -> Result<Self> {
        let config_hash = Self::compute_config_hash(config_path)?;
        Ok(Self {
            config_hash,
            last_executed: SystemTime::now(),
        })
    }

    /// Compute SHA256 hash of a configuration file
    pub fn compute_config_hash(config_path: &Path) -> Result<Vec<u8>> {
        let content = fs::read(config_path).map_err(|e| {
            Error::State(format!(
                "Failed to read config file {}: {}",
                config_path.display(),
                e
            ))
        })?;

        let mut hasher = Sha256::new();
        hasher.update(&content);
        Ok(hasher.finalize().to_vec())
    }

    /// Check if configuration has changed since last execution
    pub fn has_changed(&self, config_path: &Path) -> Result<bool> {
        let current_hash = Self::compute_config_hash(config_path)?;
        // Use constant-time comparison for hash to prevent timing side-channel attacks
        Ok(!bool::from(self.config_hash.ct_eq(&current_hash)))
    }

    /// Update state with new config hash
    pub fn update(&mut self, config_path: &Path) -> Result<()> {
        self.config_hash = Self::compute_config_hash(config_path)?;
        self.last_executed = SystemTime::now();
        Ok(())
    }
}
