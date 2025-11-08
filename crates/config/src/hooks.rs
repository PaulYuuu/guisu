//! Hook system for custom commands
//!
//! Provides a flexible hook system that can execute scripts or commands
//! at different stages (pre, post) with ordering support.
//!
//! Hooks are executed before and after applying dotfiles.

use crate::Result;
use duct::cmd;
use guisu_core::platform::CURRENT_PLATFORM;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use subtle::ConstantTimeEq;

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

    /// Working directory for the command/script
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,

    /// Environment variables to set
    #[serde(default)]
    pub env: IndexMap<String, String>,

    /// Continue on error (default: false)
    #[serde(default)]
    pub continue_on_error: bool,
}

impl Hook {
    /// Validate that the hook has either cmd or script (but not both)
    pub fn validate(&self) -> Result<()> {
        match (&self.cmd, &self.script) {
            (None, None) => Err(crate::Error::HookConfig(format!(
                "Hook '{}' must have either 'cmd' or 'script'",
                self.name
            ))),
            (Some(_), Some(_)) => Err(crate::Error::HookConfig(format!(
                "Hook '{}' cannot have both 'cmd' and 'script'",
                self.name
            ))),
            _ => Ok(()),
        }
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

/// Default order value
fn default_order() -> i32 {
    100
}

// ======================================================================

/// Hook configuration state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookState {
    /// SHA256 hash of the configuration file content
    pub config_hash: Vec<u8>,

    /// Last execution timestamp
    pub last_executed: SystemTime,
}

impl Default for HookState {
    fn default() -> Self {
        Self {
            config_hash: Vec::new(),
            last_executed: SystemTime::UNIX_EPOCH,
        }
    }
}

impl HookState {
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
            crate::Error::State(format!(
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

// ======================================================================

/// Discover and load hooks from the hooks directory
pub struct HookLoader {
    hooks_dir: PathBuf,
}

impl HookLoader {
    /// Create a new hook loader for the given source directory
    pub fn new(source_dir: &Path) -> Self {
        Self {
            hooks_dir: source_dir.join(".guisu/hooks"),
        }
    }

    /// Check if hooks directory exists
    pub fn exists(&self) -> bool {
        self.hooks_dir.exists()
    }

    /// Load all hooks from the hooks directory
    pub fn load(&self) -> Result<HookCollections> {
        if !self.hooks_dir.exists() {
            tracing::debug!(
                "Hooks directory does not exist: {}",
                self.hooks_dir.display()
            );
            return Ok(HookCollections::default());
        }

        let mut collections = HookCollections::default();

        // Load pre hooks
        let pre_dir = self.hooks_dir.join("pre");
        if pre_dir.exists() {
            collections.pre = self.load_hooks_from_dir(&pre_dir).map_err(|e| {
                crate::Error::HookConfig(format!("Failed to load pre hooks: {}", e))
            })?;
        }

        // Load post hooks
        let post_dir = self.hooks_dir.join("post");
        if post_dir.exists() {
            collections.post = self.load_hooks_from_dir(&post_dir).map_err(|e| {
                crate::Error::HookConfig(format!("Failed to load post hooks: {}", e))
            })?;
        }

        Ok(collections)
    }

    /// Load hooks from a specific directory (pre or post)
    fn load_hooks_from_dir(&self, dir: &Path) -> Result<Vec<Hook>> {
        let mut entries: Vec<_> = fs::read_dir(dir)
            .map_err(|e| {
                crate::Error::HookConfig(format!(
                    "Failed to read directory {}: {}",
                    dir.display(),
                    e
                ))
            })?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .collect();

        // Sort by filename (for numeric prefix ordering)
        entries.sort_by_key(|e| e.file_name());

        let mut hooks = Vec::new();
        let mut order = 0;

        for entry in entries {
            let path = entry.path();
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            tracing::debug!("Loading hook file: {}", path.display());

            // Skip hidden files and editor backups
            if file_name.starts_with('.') || file_name.ends_with('~') || file_name.ends_with(".swp")
            {
                tracing::debug!("Skipping hidden/backup file: {}", file_name);
                continue;
            }

            let file_hooks = self.load_hook_file(&path, order)?;
            order += file_hooks.len() as i32;
            hooks.extend(file_hooks);
        }

        Ok(hooks)
    }

    /// Load hooks from a single file
    fn load_hook_file(&self, path: &Path, base_order: i32) -> Result<Vec<Hook>> {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        match ext {
            // Script files - create a hook that executes the script
            "sh" | "bash" | "zsh" | "py" | "rb" | "pl" => {
                let hook = Hook {
                    name: file_name.to_string(),
                    order: base_order,
                    platforms: vec![],
                    cmd: Some(path.to_string_lossy().to_string()),
                    script: None,
                    working_dir: None,
                    env: Default::default(),
                    continue_on_error: false,
                };
                Ok(vec![hook])
            }

            // Configuration files - parse and load hooks
            "toml" => self.load_toml_hooks(path, base_order),

            // Unknown extension - try to determine if executable
            _ => {
                // Check if file is executable
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let metadata = fs::metadata(path)?;
                    let permissions = metadata.permissions();
                    if permissions.mode() & 0o111 != 0 {
                        // File is executable
                        let hook = Hook {
                            name: file_name.to_string(),
                            order: base_order,
                            platforms: vec![],
                            cmd: Some(path.to_string_lossy().to_string()),
                            script: None,
                            working_dir: None,
                            env: Default::default(),
                            continue_on_error: false,
                        };
                        return Ok(vec![hook]);
                    }
                }

                tracing::warn!("Skipping unknown file type: {}", path.display());
                Ok(vec![])
            }
        }
    }

    /// Load hooks from TOML file
    fn load_toml_hooks(&self, path: &Path, base_order: i32) -> Result<Vec<Hook>> {
        let content = fs::read_to_string(path).map_err(|e| {
            crate::Error::HookConfig(format!(
                "Failed to read TOML file {}: {}",
                path.display(),
                e
            ))
        })?;

        // Try to parse as array of hooks first
        if let Ok(mut hooks) = toml::from_str::<Vec<Hook>>(&content) {
            // Adjust order
            for (i, hook) in hooks.iter_mut().enumerate() {
                hook.order = base_order + i as i32;
            }
            return Ok(hooks);
        }

        // Try to parse as single hook
        if let Ok(mut hook) = toml::from_str::<Hook>(&content) {
            hook.order = base_order;
            return Ok(vec![hook]);
        }

        Err(crate::Error::HookConfig(format!(
            "Failed to parse TOML hooks from: {}",
            path.display()
        )))
    }
}

// ======================================================================

/// Hook execution runner
pub struct HookRunner<'a> {
    collections: &'a HookCollections,
    source_dir: &'a Path,
    env_vars: IndexMap<String, String>,
}

impl<'a> HookRunner<'a> {
    /// Create a new hook runner
    pub fn new(collections: &'a HookCollections, source_dir: &'a Path) -> Self {
        let mut env_vars = IndexMap::new();

        // Set up default environment variables
        env_vars.insert("GUISU_SOURCE".to_string(), source_dir.display().to_string());

        if let Some(home) = dirs::home_dir() {
            env_vars.insert("HOME".to_string(), home.display().to_string());
        }

        Self {
            collections,
            source_dir,
            env_vars,
        }
    }

    /// Add custom environment variable
    pub fn with_env(mut self, key: String, value: String) -> Self {
        self.env_vars.insert(key, value);
        self
    }

    /// Run all hooks for a specific stage
    pub fn run_stage(&self, stage: HookStage) -> Result<()> {
        let hooks = match stage {
            HookStage::Pre => &self.collections.pre,
            HookStage::Post => &self.collections.post,
        };

        if hooks.is_empty() {
            tracing::debug!("No hooks defined for stage: {}", stage.name());
            return Ok(());
        }

        tracing::info!("Running {} hooks (total: {})", stage.name(), hooks.len());

        // Sort hooks by order
        let mut sorted_hooks = hooks.clone();
        sorted_hooks.sort_by_key(|h| h.order);

        // Get current platform
        let platform = CURRENT_PLATFORM.os;

        // Execute each hook
        for hook in sorted_hooks {
            // Skip if not for this platform
            if !hook.should_run_on(platform) {
                tracing::debug!("Skipping hook '{}' (platform mismatch)", hook.name);
                continue;
            }

            // Validate hook
            if let Err(e) = hook.validate() {
                if hook.continue_on_error {
                    tracing::warn!("Invalid hook '{}': {}", hook.name, e);
                    continue;
                } else {
                    return Err(e);
                }
            }

            // Execute hook
            tracing::info!("Executing hook '{}' (order: {})", hook.name, hook.order);

            if let Err(e) = self.execute_hook(&hook) {
                if hook.continue_on_error {
                    tracing::warn!("Hook '{}' failed: {}", hook.name, e);
                    continue;
                } else {
                    return Err(crate::Error::HookExecution(format!(
                        "Hook '{}' failed: {}",
                        hook.name, e
                    )));
                }
            }

            tracing::info!("Hook '{}' completed successfully", hook.name);
        }

        Ok(())
    }

    /// Execute a single hook
    fn execute_hook(&self, hook: &Hook) -> Result<()> {
        // Determine what to execute
        let command = if let Some(cmd) = &hook.cmd {
            cmd.clone()
        } else if let Some(script_path) = &hook.script {
            // Resolve script path relative to source directory

            if script_path.starts_with('/') {
                script_path.clone()
            } else {
                self.source_dir.join(script_path).display().to_string()
            }
        } else {
            return Err(crate::Error::HookExecution(
                "Hook has neither cmd nor script".to_string(),
            ));
        };

        // Expand environment variables in command
        let command = self.expand_env_vars(&command);

        // Determine working directory
        let working_dir = if let Some(wd) = &hook.working_dir {
            let expanded = self.expand_env_vars(wd);
            std::path::PathBuf::from(expanded)
        } else {
            self.source_dir.to_path_buf()
        };

        tracing::debug!("Executing command: {}", command);
        tracing::debug!("Working directory: {}", working_dir.display());

        // Build environment variables
        let mut env = self.env_vars.clone();
        for (k, v) in &hook.env {
            let expanded_value = self.expand_env_vars(v);
            env.insert(k.clone(), expanded_value);
        }

        // Execute command via shell
        let output = cmd!("sh", "-c", command)
            .dir(&working_dir)
            .full_env(&env)
            .stderr_to_stdout()
            .run();

        match output {
            Ok(_) => Ok(()),
            Err(e) => Err(crate::Error::HookExecution(format!(
                "Command failed: {}",
                e
            ))),
        }
    }

    /// Expand environment variables in a string (simple ${VAR} expansion)
    fn expand_env_vars(&self, input: &str) -> String {
        let mut result = input.to_string();

        for (key, value) in &self.env_vars {
            let pattern = format!("${{{}}}", key);
            result = result.replace(&pattern, value);
        }

        result
    }
}
