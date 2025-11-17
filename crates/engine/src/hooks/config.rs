//! Hook configuration structures
//!
//! Defines the core types for hook configuration including Hook definitions,
//! collections, stages, and execution modes.

use guisu_core::{Error, Result};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// Collections of hooks for different stages
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HookCollections {
    /// Hooks to run before applying dotfiles
    #[serde(default)]
    pub pre: Vec<Hook>,

    /// Hooks to run after applying dotfiles
    #[serde(default)]
    pub post: Vec<Hook>,
}

impl HookCollections {
    /// Check if there are no hooks defined
    pub fn is_empty(&self) -> bool {
        self.pre.is_empty() && self.post.is_empty()
    }

    /// Get total number of hooks
    pub fn total(&self) -> usize {
        self.pre.len() + self.post.len()
    }
}

/// A single hook definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hook {
    /// Name of the hook (for logging and identification)
    pub name: String,

    /// Execution order (lower numbers run first)
    #[serde(default = "default_order")]
    pub order: i32,

    /// Platforms this hook should run on (empty = all platforms)
    #[serde(default)]
    pub platforms: Vec<String>,

    /// Direct command to execute
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cmd: Option<String>,

    /// Path to script file to execute
    #[serde(skip_serializing_if = "Option::is_none")]
    pub script: Option<String>,

    /// Environment variables to set
    #[serde(default)]
    pub env: IndexMap<String, String>,

    /// Fail fast on error (default: true)
    ///
    /// If true, stop execution when this hook fails.
    /// If false, continue executing remaining hooks even if this one fails.
    #[serde(default = "default_failfast")]
    pub failfast: bool,

    /// Execution mode (always, once, onchange)
    ///
    /// - `always`: Run every time (default)
    /// - `once`: Run only once, tracked by name
    /// - `onchange`: Run when hook content changes, tracked by content hash
    #[serde(default)]
    pub mode: HookMode,

    /// Timeout in seconds (default: 0 = no timeout)
    ///
    /// Set to 0 or omit for no timeout. Otherwise, the hook will be terminated
    /// if it runs longer than the specified number of seconds.
    #[serde(default)]
    pub timeout: u64,
}

impl Hook {
    /// Get the content of this hook for hashing (used in onchange mode)
    ///
    /// Returns the cmd or script content that should be hashed to detect changes
    pub fn get_content(&self) -> String {
        if let Some(cmd) = &self.cmd {
            cmd.clone()
        } else if let Some(script) = &self.script {
            script.clone()
        } else {
            String::new()
        }
    }

    /// Validate hook configuration
    ///
    /// Checks for:
    /// - Either cmd or script (but not both)
    /// - Valid platform names
    /// - Valid environment variable names
    /// - Non-empty name
    pub fn validate(&self) -> Result<()> {
        // Check name is not empty
        if self.name.is_empty() {
            return Err(Error::HookConfig("Hook name cannot be empty".to_string()));
        }

        // Check cmd/script exclusivity
        match (&self.cmd, &self.script) {
            (None, None) => {
                return Err(Error::HookConfig(format!(
                    "Hook '{}' must have either 'cmd' or 'script'",
                    self.name
                )));
            }
            (Some(_), Some(_)) => {
                return Err(Error::HookConfig(format!(
                    "Hook '{}' cannot have both 'cmd' and 'script'",
                    self.name
                )));
            }
            _ => {}
        }

        // Validate platform names (supported platforms)
        const VALID_PLATFORMS: &[&str] = &["darwin", "linux", "windows"];

        for platform in &self.platforms {
            if !VALID_PLATFORMS.contains(&platform.as_str()) {
                tracing::warn!(
                    hook_name = %self.name,
                    platform = %platform,
                    "Hook specifies unknown platform (typo?). Valid platforms: {}",
                    VALID_PLATFORMS.join(", ")
                );
            }
        }

        // Validate environment variable names (basic check: alphanumeric + underscore)
        for (key, _value) in &self.env {
            if key.is_empty() {
                return Err(Error::HookConfig(format!(
                    "Hook '{}' has empty environment variable name",
                    self.name
                )));
            }

            // Check if env var name is valid (starts with letter/underscore, contains alphanumeric/underscore)
            if !key
                .chars()
                .next()
                .map(|c| c.is_ascii_alphabetic() || c == '_')
                .unwrap_or(false)
            {
                return Err(Error::HookConfig(format!(
                    "Hook '{}' has invalid environment variable name '{}': must start with letter or underscore",
                    self.name, key
                )));
            }

            if !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err(Error::HookConfig(format!(
                    "Hook '{}' has invalid environment variable name '{}': must contain only alphanumeric characters and underscores",
                    self.name, key
                )));
            }
        }

        // Validate cmd/script is not empty
        if let Some(cmd) = &self.cmd
            && cmd.trim().is_empty()
        {
            return Err(Error::HookConfig(format!(
                "Hook '{}' has empty 'cmd' field",
                self.name
            )));
        }

        if let Some(script) = &self.script
            && script.trim().is_empty()
        {
            return Err(Error::HookConfig(format!(
                "Hook '{}' has empty 'script' field",
                self.name
            )));
        }

        Ok(())
    }

    /// Check if this hook should run on the given platform
    pub fn should_run_on(&self, platform: &str) -> bool {
        self.platforms.is_empty() || self.platforms.iter().any(|p| p == platform)
    }
}

/// Hook execution stage
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookStage {
    /// Before applying dotfiles
    Pre,
    /// After applying dotfiles
    Post,
}

impl HookStage {
    pub fn name(&self) -> &'static str {
        match self {
            HookStage::Pre => "pre",
            HookStage::Post => "post",
        }
    }
}

/// Hook execution mode
///
/// Controls when a hook should be executed based on its execution history
/// and content changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum HookMode {
    /// Always run the hook (default behavior)
    #[default]
    Always,

    /// Run the hook only once, ever
    ///
    /// After successful execution, the hook will never run again unless
    /// the state is manually reset. Tracked by hook name in persistent state.
    Once,

    /// Run the hook when its content changes
    ///
    /// The hook's content (script or command) is hashed and compared with
    /// the previous execution. Runs again when the hash differs.
    OnChange,
}

/// Default order value
pub(crate) fn default_order() -> i32 {
    100
}

/// Default failfast value (true = stop on error)
pub(crate) fn default_failfast() -> bool {
    true
}
