//! Ignore patterns loading from .guisu/ignores.toml

use crate::Result;
use serde::Deserialize;
use std::fs;
use std::path::Path;

/// Configuration for ignore patterns loaded from .guisu/ignores.toml
///
/// Supports gitignore-style patterns with negation using ! prefix.
///
/// Example:
/// ```toml
/// global = [
///     ".DS_Store",
///     "*.log",
///     ".config/*",
///     "!.config/atuin/",
///     "!.config/bat/",
/// ]
///
/// darwin = [
///     ".Trash/",
/// ]
/// ```
#[derive(Debug, Default, Deserialize)]
pub struct IgnoresConfig {
    #[serde(default)]
    pub global: Vec<String>,
    #[serde(default)]
    pub darwin: Vec<String>,
    #[serde(default)]
    pub linux: Vec<String>,
    #[serde(default)]
    pub windows: Vec<String>,
}

impl IgnoresConfig {
    /// Load ignore configuration from .guisu/ignores.toml
    pub fn load(source_dir: &Path) -> Result<Self> {
        let ignores_path = source_dir.join(".guisu").join("ignores.toml");

        if !ignores_path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&ignores_path).map_err(|e| {
            guisu_core::Error::Message(format!("Failed to read {}: {}", ignores_path.display(), e))
        })?;

        let config: Self = toml::from_str(&content).map_err(|e| {
            guisu_core::Error::Message(format!("Failed to parse {}: {}", ignores_path.display(), e))
        })?;

        Ok(config)
    }
}
