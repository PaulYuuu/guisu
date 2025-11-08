//! Ignore pattern matcher with include/exclude support
//!
//! Supports gitignore-style patterns with negation using ! prefix.
//!
//! Example:
//! ```toml
//! global = [
//!     ".config/*",        # Ignore all .config contents
//!     "!.config/atuin/",  # Re-include .config/atuin/
//!     "!.config/bat/",    # Re-include .config/bat/
//! ]
//! ```

use crate::{IgnoresConfig, Result};
use guisu_core::platform::CURRENT_PLATFORM;
use std::path::Path;

/// Pattern type for gitignore-style matching
#[derive(Debug, Clone)]
enum PatternType {
    /// Normal pattern - causes files to be ignored
    Include(String),
    /// Negation pattern (! prefix) - causes files to be re-included
    Exclude(String),
}

/// Ignore pattern matcher with include/exclude pattern support
///
/// Implements gitignore-style pattern matching with negation.
/// Patterns starting with ! are exclude patterns (re-include previously ignored paths).
/// Patterns are evaluated in order, with later patterns overriding earlier ones.
pub struct IgnoreMatcher {
    /// All patterns in their original order
    patterns: Vec<PatternType>,
}

impl IgnoreMatcher {
    /// Create from .guisu/ignores.toml file
    ///
    /// Loads patterns for the current platform (global + platform-specific).
    /// Patterns starting with ! are treated as exclude patterns (re-include).
    /// Pattern order is preserved for correct gitignore-style matching.
    pub fn from_ignores_toml(source_dir: &Path) -> Result<Self> {
        let config = IgnoresConfig::load(source_dir)
            .map_err(|e| crate::Error::Io(std::io::Error::other(e.to_string())))?;
        let platform = CURRENT_PLATFORM.os;

        let mut all_patterns = config.global.clone();

        // Add platform-specific patterns
        match platform {
            "darwin" => all_patterns.extend(config.darwin),
            "linux" => all_patterns.extend(config.linux),
            "windows" => all_patterns.extend(config.windows),
            _ => {}
        }

        // Parse patterns and preserve order
        let mut patterns = Vec::new();

        for pattern in all_patterns {
            if let Some(excluded) = pattern.strip_prefix('!') {
                // Pattern starts with !, it's an exclude (re-include) pattern
                patterns.push(PatternType::Exclude(excluded.to_string()));
            } else {
                // Normal include (ignore) pattern
                patterns.push(PatternType::Include(pattern));
            }
        }

        Ok(Self { patterns })
    }

    /// Check if path should be ignored
    ///
    /// Matching logic (similar to gitignore):
    /// 1. Check all patterns in order (both include and exclude)
    /// 2. Last matching pattern wins
    /// 3. Exclude patterns (! prefix) override previous include patterns
    /// 4. Include patterns can override previous exclude patterns
    /// 5. Default: NOT ignored
    ///
    /// Example:
    /// - Patterns: `.config/*`, `!.config/atuin/`, `.config/atuin/secret.key`
    /// - `.config/atuin/config.toml` -> matches `!.config/atuin/` -> NOT ignored
    /// - `.config/atuin/secret.key` -> matches `.config/atuin/secret.key` (after exclude) -> ignored
    /// - `.config/random/file` -> matches `.config/*` -> ignored
    pub fn is_ignored(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        let mut result = false; // Default: not ignored

        // Evaluate patterns in order, last match wins (gitignore behavior)
        for pattern in &self.patterns {
            match pattern {
                PatternType::Include(pat) => {
                    if self.matches_pattern(&path_str, pat) {
                        result = true; // Ignore
                    }
                }
                PatternType::Exclude(pat) => {
                    if self.matches_pattern(&path_str, pat) {
                        result = false; // Don't ignore (re-include)
                    }
                }
            }
        }

        result
    }

    /// Check if a path matches a pattern
    ///
    /// Supports:
    /// - Exact match: "file.txt"
    /// - Glob patterns: "*.log", ".config/*"
    /// - Directory prefix: ".config/" matches ".config/foo/bar"
    /// - Directory name: "DankMaterialShell" matches ".config/DankMaterialShell" or ".config/DankMaterialShell/foo"
    fn matches_pattern(&self, path_str: &str, pattern: &str) -> bool {
        // Exact match
        if path_str == pattern {
            return true;
        }

        // Directory prefix match: pattern ends with / and path starts with it
        if pattern.ends_with('/') && path_str.starts_with(pattern) {
            return true;
        }

        // Directory name match (without trailing /): check if pattern matches as directory component
        // Example: pattern "DankMaterialShell" should match ".config/DankMaterialShell" or ".config/DankMaterialShell/file"
        if !pattern.ends_with('/') && !pattern.contains('*') && !pattern.contains('?') {
            // Check if path contains the pattern as a complete directory component
            for component in path_str.split('/') {
                if component == pattern {
                    return true;
                }
            }

            // Also check if path starts with pattern/ (directory at root level)
            if path_str == pattern || path_str.starts_with(&format!("{}/", pattern)) {
                return true;
            }
        }

        // Glob pattern match
        if (pattern.contains('*') || pattern.contains('?'))
            && let Ok(glob_pattern) = glob::Pattern::new(pattern)
        {
            // Try direct match
            if glob_pattern.matches(path_str) {
                return true;
            }

            // Try matching with path separator (for patterns like ".config/*")
            if let Some(stripped) = pattern.strip_suffix('*')
                && path_str.starts_with(stripped)
            {
                return true;
            }
        }

        false
    }
}
