//! Hook system for custom commands
//!
//! Provides a flexible hook system that can execute scripts or commands
//! at different stages (pre, post) with ordering and parallel execution support.
//!
//! ## Execution Model
//!
//! - Hooks are executed before and after applying dotfiles
//! - Different order values execute sequentially (order 10 before order 20)
//! - Hooks with the same order value execute **in parallel** for maximum performance
//! - Supports execution modes: Always, Once, OnChange

use crate::Result;
use guisu_core::platform::CURRENT_PLATFORM;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
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

    /// Execution mode (always, once, onchange)
    ///
    /// - `always`: Run every time (default)
    /// - `once`: Run only once, tracked by name
    /// - `onchange`: Run when hook content changes, tracked by content hash
    #[serde(default)]
    pub mode: HookMode,

    /// Timeout in seconds (default: 300 seconds = 5 minutes)
    ///
    /// Prevents hooks from hanging indefinitely. Set to 0 for no timeout.
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,

    /// Whether this hook script is a template (has .j2 extension)
    #[serde(skip)]
    pub is_template: bool,
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
    /// - Valid timeout value
    /// - Valid platform names
    /// - Valid environment variable names
    /// - Non-empty name
    pub fn validate(&self) -> Result<()> {
        // Check name is not empty
        if self.name.is_empty() {
            return Err(crate::Error::HookConfig(
                "Hook name cannot be empty".to_string(),
            ));
        }

        // Check cmd/script exclusivity
        match (&self.cmd, &self.script) {
            (None, None) => {
                return Err(crate::Error::HookConfig(format!(
                    "Hook '{}' must have either 'cmd' or 'script'",
                    self.name
                )));
            }
            (Some(_), Some(_)) => {
                return Err(crate::Error::HookConfig(format!(
                    "Hook '{}' cannot have both 'cmd' and 'script'",
                    self.name
                )));
            }
            _ => {}
        }

        // Validate timeout (max 24 hours = 86400 seconds)
        const MAX_TIMEOUT_SECS: u64 = 86400;
        if self.timeout_secs > MAX_TIMEOUT_SECS {
            return Err(crate::Error::HookConfig(format!(
                "Hook '{}' has invalid timeout: {} seconds (max: {} seconds / 24 hours)",
                self.name, self.timeout_secs, MAX_TIMEOUT_SECS
            )));
        }

        // Validate platform names (common platforms)
        const VALID_PLATFORMS: &[&str] = &[
            "darwin",
            "linux",
            "windows",
            "freebsd",
            "openbsd",
            "netbsd",
            "dragonfly",
            "solaris",
            "illumos",
            "android",
            "ios",
        ];

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
                return Err(crate::Error::HookConfig(format!(
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
                return Err(crate::Error::HookConfig(format!(
                    "Hook '{}' has invalid environment variable name '{}': must start with letter or underscore",
                    self.name, key
                )));
            }

            if !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err(crate::Error::HookConfig(format!(
                    "Hook '{}' has invalid environment variable name '{}': must contain only alphanumeric characters and underscores",
                    self.name, key
                )));
            }
        }

        // Validate cmd/script is not empty
        if let Some(cmd) = &self.cmd
            && cmd.trim().is_empty()
        {
            return Err(crate::Error::HookConfig(format!(
                "Hook '{}' has empty 'cmd' field",
                self.name
            )));
        }

        if let Some(script) = &self.script
            && script.trim().is_empty()
        {
            return Err(crate::Error::HookConfig(format!(
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
fn default_order() -> i32 {
    100
}

/// Default timeout value in seconds (5 minutes)
fn default_timeout() -> u64 {
    300
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
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        // Parse template extension first (e.g., "script.sh.j2" → (true, "script.sh"))
        let (is_template, clean_name) = parse_template_extension(file_name);

        // Get the base extension after removing .j2
        let ext = get_base_extension(&clean_name).unwrap_or("");

        match ext {
            // Script files - create a hook that executes the script
            "sh" | "bash" | "zsh" | "py" | "rb" | "pl" => {
                let hook = Hook {
                    name: clean_name.clone(),
                    order: base_order,
                    platforms: vec![],
                    cmd: Some(path.to_string_lossy().to_string()),
                    script: None,
                    working_dir: None,
                    env: Default::default(),
                    continue_on_error: false,
                    mode: HookMode::default(),
                    timeout_secs: default_timeout(),
                    is_template,
                };
                Ok(vec![hook])
            }

            // Configuration files - parse and load hooks
            "toml" => self.load_toml_hooks(path, base_order),

            // .j2 alone - skip (should have base extension like .sh.j2)
            "j2" => {
                tracing::warn!("Template file without base extension: {}", path.display());
                Ok(vec![])
            }

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
                            name: clean_name.clone(),
                            order: base_order,
                            platforms: vec![],
                            cmd: Some(path.to_string_lossy().to_string()),
                            script: None,
                            working_dir: None,
                            env: Default::default(),
                            continue_on_error: false,
                            mode: HookMode::default(),
                            timeout_secs: default_timeout(),
                            is_template,
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
    fn load_toml_hooks(&self, path: &Path, _base_order: i32) -> Result<Vec<Hook>> {
        let content = fs::read_to_string(path).map_err(|e| {
            crate::Error::HookConfig(format!(
                "Failed to read TOML file {}: {}",
                path.display(),
                e
            ))
        })?;

        // Try to parse as array of hooks first
        if let Ok(mut hooks) = toml::from_str::<Vec<Hook>>(&content) {
            // Resolve script paths relative to hook file directory
            for hook in &mut hooks {
                self.resolve_script_path(hook, path)?;
            }
            return Ok(hooks);
        }

        // Try to parse as single hook
        if let Ok(mut hook) = toml::from_str::<Hook>(&content) {
            // Resolve script path relative to hook file directory
            self.resolve_script_path(&mut hook, path)?;
            return Ok(vec![hook]);
        }

        Err(crate::Error::HookConfig(format!(
            "Failed to parse TOML hooks from: {}",
            path.display()
        )))
    }

    /// Resolve script path relative to hook file directory
    ///
    /// This function supports automatic .j2 template detection:
    /// - If script = "script.sh.j2", uses it directly as a template
    /// - If script = "script.sh" and "script.sh.j2" exists, uses the template version
    /// - Otherwise, uses the specified path as-is
    fn resolve_script_path(&self, hook: &mut Hook, hook_file_path: &Path) -> Result<()> {
        if let Some(script) = &hook.script {
            // Skip absolute paths
            if script.starts_with('/') {
                // Still check for .j2 extension on absolute paths
                let (is_template, _clean_script) = parse_template_extension(script);
                hook.is_template = is_template;
                return Ok(());
            }

            // Get hook file directory
            let hook_dir = hook_file_path.parent().ok_or_else(|| {
                crate::Error::HookConfig(format!(
                    "Cannot get parent directory of hook file: {}",
                    hook_file_path.display()
                ))
            })?;

            // Resolve script path relative to hook directory
            let script_abs = hook_dir.join(script);

            // Auto-detect .j2 template version
            let (final_script_abs, is_template) = if script.ends_with(".j2") {
                // Explicitly specified as template
                (script_abs, true)
            } else {
                // Check if .j2 version exists
                let template_version = hook_dir.join(format!("{}.j2", script));
                if template_version.exists() {
                    tracing::debug!(
                        "Auto-detected template version: {} -> {}",
                        script,
                        template_version.display()
                    );
                    (template_version, true)
                } else {
                    // Use original path
                    (script_abs, false)
                }
            };

            hook.is_template = is_template;

            // Get source directory (.guisu/hooks -> .guisu -> source_dir)
            let source_dir = self
                .hooks_dir
                .parent()
                .and_then(|p| p.parent())
                .ok_or_else(|| {
                    crate::Error::HookConfig(format!(
                        "Cannot determine source directory from hooks dir: {}",
                        self.hooks_dir.display()
                    ))
                })?;

            // Convert to relative path from source_dir
            let script_rel = final_script_abs.strip_prefix(source_dir).map_err(|_| {
                crate::Error::HookConfig(format!(
                    "Script path is outside source directory: {}",
                    final_script_abs.display()
                ))
            })?;

            hook.script = Some(script_rel.display().to_string());
        }

        Ok(())
    }
}

// ======================================================================

/// Template rendering trait for hook scripts
pub trait TemplateRenderer {
    fn render(&self, input: &str) -> Result<String>;
}

/// No-op template renderer (returns input unchanged)
#[derive(Debug, Clone, Copy, Default)]
pub struct NoOpRenderer;

impl TemplateRenderer for NoOpRenderer {
    fn render(&self, input: &str) -> Result<String> {
        Ok(input.to_string())
    }
}

/// Implement TemplateRenderer for closures
impl<F> TemplateRenderer for F
where
    F: Fn(&str) -> Result<String>,
{
    fn render(&self, input: &str) -> Result<String> {
        self(input)
    }
}

/// Hook execution runner with parallel execution support
///
/// Executes hooks in parallel within each order group, utilizing multi-core CPUs
/// for improved performance. Thread-safe state tracking ensures correct execution
/// for mode=once and mode=onchange hooks.
pub struct HookRunner<'a, R = NoOpRenderer>
where
    R: TemplateRenderer,
{
    collections: &'a HookCollections,
    source_dir: &'a Path,
    /// Shared environment variables (Arc to avoid cloning for each hook)
    env_vars: std::sync::Arc<IndexMap<String, String>>,
    template_renderer: R,
    /// Track which hooks with mode=once have been executed in this session (thread-safe)
    once_executed: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
    /// State from persistent storage (for checking already executed once hooks)
    persistent_once: std::collections::HashSet<String>,
    /// Content hashes for onchange hooks from persistent storage
    persistent_onchange: std::collections::HashMap<String, Vec<u8>>,
    /// Content hashes for onchange hooks executed in this session (thread-safe)
    onchange_hashes: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>>,
}

impl<'a> HookRunner<'a, NoOpRenderer> {
    /// Create a new hook runner with no template renderer
    ///
    /// This is a convenience method that immediately builds a runner with default settings.
    /// For custom configuration, use [`HookRunner::builder`].
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Simple usage - no template rendering needed
    /// let runner = HookRunner::new(&collections, source_dir);
    /// runner.run_stage(HookStage::Pre)?;
    ///
    /// // For custom configuration, use builder:
    /// let runner = HookRunner::builder(&collections, source_dir)
    ///     .template_renderer(my_renderer)
    ///     .build();
    /// ```
    pub fn new(collections: &'a HookCollections, source_dir: &'a Path) -> Self {
        Self::builder(collections, source_dir).build()
    }

    /// Create a builder for configuring a HookRunner
    ///
    /// This is the primary way to create a HookRunner with custom configuration.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let runner = HookRunner::builder(&collections, source_dir)
    ///     .template_renderer(my_renderer)
    ///     .persistent_state(once_executed, onchange_hashes)
    ///     .env("CUSTOM_VAR", "value")
    ///     .build();
    /// ```
    pub fn builder(
        collections: &'a HookCollections,
        source_dir: &'a Path,
    ) -> HookRunnerBuilder<'a, NoOpRenderer> {
        HookRunnerBuilder::new(collections, source_dir)
    }
}

impl<'a, R> HookRunner<'a, R>
where
    R: TemplateRenderer + Sync,
{
    /// Get the set of hooks with mode=once that were executed in this session
    ///
    /// This should be saved to persistent state after running hooks
    pub fn get_once_executed(&self) -> std::collections::HashSet<String> {
        self.once_executed.lock().unwrap().clone()
    }

    /// Get the content hashes for hooks with mode=onchange from this session
    ///
    /// This should be saved to persistent state after running hooks
    pub fn get_onchange_hashes(&self) -> std::collections::HashMap<String, Vec<u8>> {
        self.onchange_hashes.lock().unwrap().clone()
    }

    /// Check if a hook should be skipped based on its mode
    ///
    /// Returns (should_skip, reason, cached_hash) for logging and state update
    /// The cached_hash is only computed for OnChange mode to avoid redundant hashing
    #[tracing::instrument(skip(self), fields(hook_name = %hook.name, hook_mode = ?hook.mode))]
    fn should_skip_hook(&self, hook: &Hook) -> (bool, &'static str, Option<Vec<u8>>) {
        use sha2::{Digest, Sha256};

        match hook.mode {
            HookMode::Always => {
                tracing::trace!("Hook will run (mode=always)");
                (false, "", None)
            }

            HookMode::Once => {
                // Check if executed in this session
                if self.once_executed.lock().unwrap().contains(&hook.name) {
                    tracing::debug!("Skipping hook: already executed in this session");
                    return (true, "already executed in this session (mode=once)", None);
                }

                // Check if executed in previous sessions
                if self.persistent_once.contains(&hook.name) {
                    tracing::debug!("Skipping hook: already executed previously");
                    return (true, "already executed previously (mode=once)", None);
                }

                tracing::trace!("Hook will run (mode=once, first execution)");
                (false, "", None)
            }

            HookMode::OnChange => {
                // Compute content hash (cached for later use)
                let content = hook.get_content();
                let mut hasher = Sha256::new();
                hasher.update(content.as_bytes());
                let current_hash = hasher.finalize().to_vec();

                // Check if content changed from this session
                if let Some(session_hash) = self.onchange_hashes.lock().unwrap().get(&hook.name)
                    && session_hash == &current_hash
                {
                    tracing::debug!("Skipping hook: content unchanged in this session");
                    return (
                        true,
                        "content unchanged in this session (mode=onchange)",
                        Some(current_hash),
                    );
                }

                // Check if content changed from previous sessions
                if let Some(stored_hash) = self.persistent_onchange.get(&hook.name) {
                    use subtle::ConstantTimeEq;
                    if bool::from(stored_hash.ct_eq(&current_hash)) {
                        tracing::debug!("Skipping hook: content unchanged from previous session");
                        return (
                            true,
                            "content unchanged (mode=onchange)",
                            Some(current_hash),
                        );
                    }
                }

                tracing::trace!("Hook will run (mode=onchange, content changed)");
                (false, "", Some(current_hash))
            }
        }
    }

    /// Mark a hook as executed based on its mode
    ///
    /// Accepts a cached_hash from should_skip_hook to avoid redundant hash computation
    fn mark_hook_executed(&self, hook: &Hook, cached_hash: Option<Vec<u8>>) {
        match hook.mode {
            HookMode::Always => {
                // No tracking needed
            }

            HookMode::Once => {
                self.once_executed.lock().unwrap().insert(hook.name.clone());
            }

            HookMode::OnChange => {
                // Use cached hash if available, otherwise compute
                let content_hash = cached_hash.unwrap_or_else(|| {
                    use sha2::{Digest, Sha256};
                    let content = hook.get_content();
                    let mut hasher = Sha256::new();
                    hasher.update(content.as_bytes());
                    hasher.finalize().to_vec()
                });

                self.onchange_hashes
                    .lock()
                    .unwrap()
                    .insert(hook.name.clone(), content_hash);
            }
        }
    }

    /// Run all hooks for a specific stage
    #[tracing::instrument(skip(self), fields(stage = %stage.name()))]
    pub fn run_stage(&self, stage: HookStage) -> Result<()> {
        use std::collections::BTreeMap;

        let hooks = match stage {
            HookStage::Pre => &self.collections.pre,
            HookStage::Post => &self.collections.post,
        };

        if hooks.is_empty() {
            tracing::debug!("No hooks defined for stage");
            return Ok(());
        }

        tracing::info!(hook_count = hooks.len(), "Running hooks for stage");

        // Get current platform
        let platform = CURRENT_PLATFORM.os;

        // Filter and validate hooks, then group by order
        let mut hooks_by_order: BTreeMap<i32, Vec<&Hook>> = BTreeMap::new();

        for hook in hooks {
            // Skip if not for this platform
            if !hook.should_run_on(platform) {
                tracing::debug!("Skipping hook '{}' (platform mismatch)", hook.name);
                continue;
            }

            // Skip based on execution mode
            let (should_skip, reason, _cached_hash) = self.should_skip_hook(hook);
            if should_skip {
                tracing::debug!("Skipping hook '{}' ({})", hook.name, reason);
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

            hooks_by_order.entry(hook.order).or_default().push(hook);
        }

        // Execute hooks in order, parallelizing within each order group
        for (order, order_hooks) in hooks_by_order {
            tracing::debug!(
                order = order,
                count = order_hooks.len(),
                "Executing hooks in parallel for order group"
            );

            // Parallel execution within same order group
            // All hooks with the same order number run concurrently
            use rayon::prelude::*;

            let results: Vec<(Option<Vec<u8>>, Result<()>)> = order_hooks
                .par_iter()
                .map(|hook| {
                    // Get cached hash for state tracking (avoids redundant hash computation)
                    let (_should_skip, _reason, cached_hash) = self.should_skip_hook(hook);

                    // Create a span for this hook execution with structured fields
                    let span = tracing::info_span!(
                        "hook_execution",
                        hook_name = %hook.name,
                        hook_order = hook.order,
                        hook_mode = ?hook.mode,
                        timeout_secs = hook.timeout_secs,
                        continue_on_error = hook.continue_on_error,
                    );
                    let _guard = span.enter();

                    let start = std::time::Instant::now();
                    tracing::info!("Starting hook execution");

                    // Execute hook
                    let result = self.execute_hook(hook);

                    let elapsed = start.elapsed();
                    match &result {
                        Ok(_) => {
                            tracing::info!(
                                elapsed_ms = elapsed.as_millis(),
                                "Hook completed successfully"
                            );
                        }
                        Err(e) => {
                            if hook.continue_on_error {
                                tracing::warn!(
                                    elapsed_ms = elapsed.as_millis(),
                                    error = %e,
                                    "Hook failed but continuing"
                                );
                            } else {
                                tracing::error!(
                                    elapsed_ms = elapsed.as_millis(),
                                    error = %e,
                                    "Hook failed"
                                );
                            }
                        }
                    }

                    (cached_hash, result)
                })
                .collect();

            // Process results: mark hooks as executed and check for errors
            for ((cached_hash, result), hook) in results.into_iter().zip(order_hooks.iter()) {
                match result {
                    Ok(_) => {
                        // Mark hook as executed based on mode (with cached hash)
                        self.mark_hook_executed(hook, cached_hash);
                    }
                    Err(e) => {
                        if hook.continue_on_error {
                            // Still mark as executed for continue_on_error hooks
                            self.mark_hook_executed(hook, cached_hash);
                        } else {
                            // Fail-fast: return first error
                            return Err(crate::Error::HookExecution(format!(
                                "Hook '{}' failed: {}",
                                hook.name, e
                            )));
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Execute a single hook
    fn execute_hook(&self, hook: &Hook) -> Result<()> {
        // If hook uses 'script' and is a template, process it specially
        if hook.script.is_some() && hook.is_template {
            return self.execute_template_script(hook);
        }

        // Determine working directory
        let working_dir = if let Some(wd) = &hook.working_dir {
            let expanded = self.expand_env_vars(wd);
            std::path::PathBuf::from(expanded.into_owned())
        } else {
            self.source_dir.to_path_buf()
        };

        // Build environment variables (only clone if hook has custom env)
        let env = if hook.env.is_empty() {
            // No custom env vars, use shared Arc (just increment refcount)
            self.env_vars.clone()
        } else {
            // Clone-on-write: only allocate when hook has custom env vars
            let mut env = (*self.env_vars).clone();
            for (k, v) in &hook.env {
                let expanded_value = self.expand_env_vars(v);
                env.insert(k.clone(), expanded_value.into_owned());
            }
            std::sync::Arc::new(env)
        };

        // Execute based on hook type
        match (&hook.cmd, &hook.script) {
            (Some(cmd), None) => {
                // Direct command execution (no shell)
                self.execute_command(cmd, &working_dir, &env, hook.timeout_secs)
                    .map_err(|e| {
                        crate::Error::HookExecution(format!(
                            "Hook '{}' command failed: {}",
                            hook.name, e
                        ))
                    })
            }
            (None, Some(script_path)) => {
                // Script execution via shebang
                let script_abs = if script_path.starts_with('/') {
                    PathBuf::from(script_path)
                } else {
                    self.source_dir.join(script_path)
                };
                self.execute_script(&script_abs, &working_dir, &env, hook.timeout_secs)
                    .map_err(|e| {
                        crate::Error::HookExecution(format!(
                            "Hook '{}' script '{}' failed: {}",
                            hook.name, script_path, e
                        ))
                    })
            }
            (None, None) => Err(crate::Error::HookExecution(format!(
                "Hook '{}' has neither cmd nor script (validation should have caught this)",
                hook.name
            ))),
            (Some(_), Some(_)) => {
                // This should be impossible due to validation
                unreachable!(
                    "Hook '{}' validation ensures only cmd or script, not both",
                    hook.name
                )
            }
        }
    }

    /// Execute a command directly without shell
    ///
    /// Parses the command string into program and arguments, then executes
    /// without invoking a shell. This prevents shell injection vulnerabilities.
    ///
    /// Supports quoted arguments: `git commit -m "Initial commit"`
    #[tracing::instrument(skip(self, env), fields(cmd = %cmd, working_dir = %working_dir.display(), timeout_secs))]
    fn execute_command(
        &self,
        cmd: &str,
        working_dir: &Path,
        env: &IndexMap<String, String>,
        timeout_secs: u64,
    ) -> Result<()> {
        use std::time::Duration;

        // Expand environment variables in command
        let expanded_cmd = self.expand_env_vars(cmd);

        // Parse command using shell-words for proper quote handling
        // Handles: git commit -m "Initial commit" → ["git", "commit", "-m", "Initial commit"]
        let parts = shell_words::split(&expanded_cmd).map_err(|e| {
            crate::Error::HookExecution(format!("Failed to parse command '{}': {}", cmd, e))
        })?;

        if parts.is_empty() {
            return Err(crate::Error::HookExecution("Empty command".to_string()));
        }

        let program = &parts[0];
        let args = &parts[1..];

        tracing::debug!("Executing command: {} {:?}", program, args);
        tracing::debug!("Working directory: {}", working_dir.display());
        if timeout_secs > 0 {
            tracing::debug!("Timeout: {} seconds", timeout_secs);
        }

        // Build command
        let cmd_builder = duct::cmd(program, args)
            .dir(working_dir)
            .full_env(env)
            .stderr_to_stdout();

        // Execute with or without timeout
        if timeout_secs > 0 {
            let handle = cmd_builder.start().map_err(|e| {
                crate::Error::HookExecution(format!("Failed to start command '{}': {}", program, e))
            })?;

            match handle.wait_timeout(Duration::from_secs(timeout_secs)) {
                Ok(Some(_output)) => Ok(()),
                Ok(None) => Err(crate::Error::HookExecution(format!(
                    "Command '{}' timed out after {} seconds",
                    program, timeout_secs
                ))),
                Err(e) => Err(crate::Error::HookExecution(format!(
                    "Command '{}' failed: {}",
                    program, e
                ))),
            }
        } else {
            cmd_builder.run().map(|_| ()).map_err(|e| {
                crate::Error::HookExecution(format!("Command '{}' failed: {}", program, e))
            })
        }
    }

    /// Execute a script using its shebang interpreter
    ///
    /// Reads the script's shebang line to determine the interpreter,
    /// then executes the script with that interpreter.
    #[tracing::instrument(skip(self, env), fields(script_path = %script_path.display(), working_dir = %working_dir.display(), timeout_secs))]
    fn execute_script(
        &self,
        script_path: &Path,
        working_dir: &Path,
        env: &IndexMap<String, String>,
        timeout_secs: u64,
    ) -> Result<()> {
        use std::time::Duration;

        if !script_path.exists() {
            return Err(crate::Error::HookExecution(format!(
                "Script not found: {}",
                script_path.display()
            )));
        }

        tracing::debug!("Executing script: {}", script_path.display());
        tracing::debug!("Working directory: {}", working_dir.display());
        if timeout_secs > 0 {
            tracing::debug!("Timeout: {} seconds", timeout_secs);
        }

        // Parse shebang to get interpreter
        let (interpreter, args) = self.parse_shebang(script_path)?;

        // Build command: interpreter + args + script_path
        let mut cmd_args = args;
        cmd_args.push(script_path.to_string_lossy().to_string());

        tracing::debug!("Using interpreter: {} {:?}", interpreter, cmd_args);

        // Build command
        let cmd_builder = duct::cmd(&interpreter, &cmd_args)
            .dir(working_dir)
            .full_env(env)
            .stderr_to_stdout();

        // Execute with or without timeout
        if timeout_secs > 0 {
            let handle = cmd_builder.start().map_err(|e| {
                crate::Error::HookExecution(format!(
                    "Failed to start script '{}': {}",
                    script_path.display(),
                    e
                ))
            })?;

            match handle.wait_timeout(Duration::from_secs(timeout_secs)) {
                Ok(Some(_output)) => Ok(()),
                Ok(None) => Err(crate::Error::HookExecution(format!(
                    "Script '{}' timed out after {} seconds",
                    script_path.display(),
                    timeout_secs
                ))),
                Err(e) => Err(crate::Error::HookExecution(format!(
                    "Script '{}' failed: {}",
                    script_path.display(),
                    e
                ))),
            }
        } else {
            cmd_builder.run().map(|_| ()).map_err(|e| {
                crate::Error::HookExecution(format!(
                    "Script '{}' failed: {}",
                    script_path.display(),
                    e
                ))
            })
        }
    }

    /// Parse shebang line from a script file
    ///
    /// Returns (interpreter, args)
    ///
    /// # Examples
    ///
    /// - `#!/bin/bash` → ("bash", [])
    /// - `#!/usr/bin/env python3` → ("python3", [])
    /// - `#!/bin/bash -e` → ("bash", ["-e"])
    fn parse_shebang(&self, script_path: &Path) -> Result<(String, Vec<String>)> {
        use std::io::{BufRead, BufReader};

        let file = fs::File::open(script_path).map_err(|e| {
            crate::Error::HookExecution(format!(
                "Failed to open script {}: {}",
                script_path.display(),
                e
            ))
        })?;

        let mut reader = BufReader::new(file);
        let mut first_line = String::new();
        reader.read_line(&mut first_line).map_err(|e| {
            crate::Error::HookExecution(format!(
                "Failed to read script {}: {}",
                script_path.display(),
                e
            ))
        })?;

        // Check for shebang
        if !first_line.starts_with("#!") {
            // No shebang, try to infer from extension or use default
            return self.infer_interpreter(script_path);
        }

        // Parse shebang line
        let shebang = first_line[2..].trim();

        // Handle "#! /usr/bin/env interpreter"
        if shebang.starts_with("/usr/bin/env") || shebang.starts_with("/bin/env") {
            let parts: Vec<&str> = shebang.split_whitespace().collect();
            if parts.len() < 2 {
                return Err(crate::Error::HookExecution(format!(
                    "Invalid env shebang: {}",
                    first_line
                )));
            }

            let interpreter = parts[1].to_string();
            let args = parts[2..].iter().map(|s| s.to_string()).collect();
            return Ok((interpreter, args));
        }

        // Handle "#! /bin/bash" or "#! /bin/bash -e"
        let parts: Vec<&str> = shebang.split_whitespace().collect();
        if parts.is_empty() {
            return Err(crate::Error::HookExecution(format!(
                "Empty shebang: {}",
                first_line
            )));
        }

        // Extract interpreter name from path
        let interpreter_path = PathBuf::from(parts[0]);
        let interpreter = interpreter_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| {
                crate::Error::HookExecution(format!("Invalid interpreter path: {}", parts[0]))
            })?
            .to_string();

        let args = parts[1..].iter().map(|s| s.to_string()).collect();

        Ok((interpreter, args))
    }

    /// Infer interpreter from script extension when no shebang is present
    fn infer_interpreter(&self, script_path: &Path) -> Result<(String, Vec<String>)> {
        let extension = script_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let interpreter = match extension {
            "sh" => "sh",
            "bash" => "bash",
            "zsh" => "zsh",
            "py" => "python3",
            "rb" => "ruby",
            "pl" => "perl",
            "js" => "node",
            "" => {
                // No extension, check if executable
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let metadata = fs::metadata(script_path)?;
                    if metadata.permissions().mode() & 0o111 != 0 {
                        // Executable, try to execute directly
                        return Ok((script_path.to_string_lossy().to_string(), vec![]));
                    }
                }

                // Default to sh
                "sh"
            }
            _ => {
                return Err(crate::Error::HookExecution(format!(
                    "Cannot infer interpreter for script: {} (extension: {})",
                    script_path.display(),
                    extension
                )));
            }
        };

        Ok((interpreter.to_string(), vec![]))
    }

    /// Execute a template script by rendering it first
    fn execute_template_script(&self, hook: &Hook) -> Result<()> {
        let script_path = hook.script.as_ref().ok_or_else(|| {
            crate::Error::HookExecution("Template hook missing script path".to_string())
        })?;

        // Resolve full script path
        let full_script_path = if script_path.starts_with('/') {
            PathBuf::from(script_path)
        } else {
            self.source_dir.join(script_path)
        };

        tracing::debug!("Reading template script: {}", full_script_path.display());

        // Read script content
        let content = fs::read_to_string(&full_script_path).map_err(|e| {
            crate::Error::HookExecution(format!(
                "Failed to read script {}: {}",
                full_script_path.display(),
                e
            ))
        })?;

        // Render template using the renderer
        tracing::debug!("Rendering template for hook '{}'", hook.name);
        let processed_content = self.template_renderer.render(&content).map_err(|e| {
            crate::Error::HookExecution(format!("Failed to render template: {}", e))
        })?;

        // Execute the processed script
        self.execute_processed_script(&processed_content, hook)
    }

    /// Execute a processed script via temporary file
    fn execute_processed_script(&self, content: &str, hook: &Hook) -> Result<()> {
        use tempfile::NamedTempFile;

        // Create temporary file
        let mut temp_file = NamedTempFile::new().map_err(|e| {
            crate::Error::HookExecution(format!("Failed to create temp file: {}", e))
        })?;

        // Write content
        temp_file.write_all(content.as_bytes()).map_err(|e| {
            crate::Error::HookExecution(format!("Failed to write temp file: {}", e))
        })?;

        // Set executable permissions (Unix)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::Permissions::from_mode(0o700);
            temp_file.as_file().set_permissions(perms).map_err(|e| {
                crate::Error::HookExecution(format!("Failed to set permissions: {}", e))
            })?;
        }

        // Determine working directory
        let working_dir = if let Some(wd) = &hook.working_dir {
            let expanded = self.expand_env_vars(wd);
            PathBuf::from(expanded.into_owned())
        } else {
            self.source_dir.to_path_buf()
        };

        // Build environment variables (only clone if hook has custom env)
        let env = if hook.env.is_empty() {
            // No custom env vars, use shared Arc (just increment refcount)
            self.env_vars.clone()
        } else {
            // Clone-on-write: only allocate when hook has custom env vars
            let mut env = (*self.env_vars).clone();
            for (k, v) in &hook.env {
                let expanded_value = self.expand_env_vars(v);
                env.insert(k.clone(), expanded_value.into_owned());
            }
            std::sync::Arc::new(env)
        };

        let temp_path = temp_file.path();
        tracing::debug!("Executing processed script: {}", temp_path.display());
        tracing::debug!("Working directory: {}", working_dir.display());

        // Execute script using shebang (same as regular scripts)
        // temp_file is automatically deleted when dropped
        self.execute_script(temp_path, &working_dir, &env, hook.timeout_secs)
    }

    /// Expand environment variables in a string (simple ${VAR} expansion)
    ///
    /// Uses Cow to avoid allocation when no substitution is needed.
    fn expand_env_vars<'b>(&self, input: &'b str) -> std::borrow::Cow<'b, str> {
        use std::borrow::Cow;

        // Quick check: does input contain any '${'?
        if !input.contains("${") {
            return Cow::Borrowed(input);
        }

        let mut result = String::with_capacity(input.len());
        let mut last_end = 0;
        let chars: Vec<char> = input.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            // Look for ${
            if i + 1 < chars.len() && chars[i] == '$' && chars[i + 1] == '{' {
                // Add everything before this variable
                result.push_str(&input[last_end..i]);

                // Find closing }
                if let Some(close_idx) = chars[i + 2..].iter().position(|&c| c == '}') {
                    let var_start = i + 2;
                    let var_end = i + 2 + close_idx;

                    // Extract variable name
                    let var_name: String = chars[var_start..var_end].iter().collect();

                    // Replace with value or keep original
                    if let Some(value) = self.env_vars.get(&var_name) {
                        result.push_str(value);
                    } else {
                        // Variable not found, keep original
                        result.push_str(&input[i..=var_end]);
                    }

                    last_end = var_end + 1;
                    i = var_end + 1;
                    continue;
                }
            }

            i += 1;
        }

        if last_end == 0 {
            // No replacements made
            Cow::Borrowed(input)
        } else {
            result.push_str(&input[last_end..]);
            Cow::Owned(result)
        }
    }
}

// ======================================================================
// HookRunnerBuilder - Type-safe builder pattern for HookRunner
// ======================================================================

/// Builder for creating a HookRunner with custom configuration
///
/// This builder provides a fluent API for configuring a HookRunner before
/// creating it. It ensures all necessary configuration is provided while
/// making optional configuration clear.
///
/// # Examples
///
/// ```ignore
/// use guisu_config::hooks::{HookRunner, HookStage};
///
/// let runner = HookRunner::builder(&collections, source_dir)
///     .template_renderer(my_renderer)
///     .persistent_state(once_executed, onchange_hashes)
///     .env("CUSTOM_VAR", "custom_value")
///     .build();
///
/// runner.run_stage(HookStage::Pre)?;
/// ```
pub struct HookRunnerBuilder<'a, R = NoOpRenderer>
where
    R: TemplateRenderer,
{
    collections: &'a HookCollections,
    source_dir: &'a Path,
    env_vars: IndexMap<String, String>,
    template_renderer: R,
    persistent_once: std::collections::HashSet<String>,
    persistent_onchange: std::collections::HashMap<String, Vec<u8>>,
}

impl<'a> HookRunnerBuilder<'a, NoOpRenderer> {
    /// Create a new builder with required parameters
    ///
    /// This is typically called via [`HookRunner::builder`].
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
            template_renderer: NoOpRenderer,
            persistent_once: std::collections::HashSet::new(),
            persistent_onchange: std::collections::HashMap::new(),
        }
    }

    /// Set the template renderer for processing template scripts
    ///
    /// Transforms the builder to use a specific renderer type.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let builder = HookRunner::builder(&collections, source_dir)
    ///     .template_renderer(|content| {
    ///         // Custom template rendering logic
    ///         Ok(content.to_string())
    ///     });
    /// ```
    pub fn template_renderer<F>(self, renderer: F) -> HookRunnerBuilder<'a, F>
    where
        F: TemplateRenderer,
    {
        HookRunnerBuilder {
            collections: self.collections,
            source_dir: self.source_dir,
            env_vars: self.env_vars,
            template_renderer: renderer,
            persistent_once: self.persistent_once,
            persistent_onchange: self.persistent_onchange,
        }
    }
}

impl<'a, R> HookRunnerBuilder<'a, R>
where
    R: TemplateRenderer,
{
    /// Set persistent state for mode=once and mode=onchange hooks
    ///
    /// This tells the runner which hooks have already been executed and
    /// what their content hashes were.
    ///
    /// # Parameters
    ///
    /// * `once_executed` - Set of hook names that have been executed with mode=once
    /// * `onchange_hashes` - Map of hook names to their content hashes for mode=onchange
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let runner = HookRunner::builder(&collections, source_dir)
    ///     .persistent_state(
    ///         HashSet::from(["setup-once".to_string()]),
    ///         HashMap::from([("config-update".to_string(), vec![0x12, 0x34])])
    ///     )
    ///     .build();
    /// ```
    pub fn persistent_state(
        mut self,
        once_executed: std::collections::HashSet<String>,
        onchange_hashes: std::collections::HashMap<String, Vec<u8>>,
    ) -> Self {
        self.persistent_once = once_executed;
        self.persistent_onchange = onchange_hashes;
        self
    }

    /// Add a custom environment variable
    ///
    /// This environment variable will be available to all hooks.
    /// Can be called multiple times to add multiple variables.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let runner = HookRunner::builder(&collections, source_dir)
    ///     .env("DEPLOY_ENV", "production")
    ///     .env("REGION", "us-west-2")
    ///     .build();
    /// ```
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_vars.insert(key.into(), value.into());
        self
    }

    /// Add multiple environment variables at once
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use indexmap::IndexMap;
    ///
    /// let mut vars = IndexMap::new();
    /// vars.insert("VAR1".to_string(), "value1".to_string());
    /// vars.insert("VAR2".to_string(), "value2".to_string());
    ///
    /// let runner = HookRunner::builder(&collections, source_dir)
    ///     .env_vars(vars)
    ///     .build();
    /// ```
    pub fn env_vars(mut self, vars: IndexMap<String, String>) -> Self {
        self.env_vars.extend(vars);
        self
    }

    /// Build the HookRunner
    ///
    /// Consumes the builder and creates a configured HookRunner.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let runner = HookRunner::builder(&collections, source_dir)
    ///     .template_renderer(my_renderer)
    ///     .persistent_state(once_executed, onchange_hashes)
    ///     .build();
    /// ```
    pub fn build(self) -> HookRunner<'a, R> {
        HookRunner {
            collections: self.collections,
            source_dir: self.source_dir,
            env_vars: std::sync::Arc::new(self.env_vars),
            template_renderer: self.template_renderer,
            once_executed: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashSet::new(),
            )),
            persistent_once: self.persistent_once,
            persistent_onchange: self.persistent_onchange,
            onchange_hashes: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
        }
    }
}

// ======================================================================
// Helper functions for template extension parsing
// ======================================================================

/// Parse template extension from filename
/// Returns (is_template, clean_name)
///
/// Examples:
/// - "script.sh.j2" → (true, "script.sh")
/// - "script.sh" → (false, "script.sh")
fn parse_template_extension(filename: &str) -> (bool, String) {
    if filename.ends_with(".j2") {
        let clean = filename.strip_suffix(".j2").unwrap();
        (true, clean.to_string())
    } else {
        (false, filename.to_string())
    }
}

/// Get the base extension from a filename (ignoring .j2)
///
/// Examples:
/// - "script.sh" → Some("sh")
/// - "script.sh.j2" → (should call parse_template_extension first)
/// - "script" → None
fn get_base_extension(filename: &str) -> Option<&str> {
    Path::new(filename).extension()?.to_str()
}
