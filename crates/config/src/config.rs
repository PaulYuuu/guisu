//! Configuration management
//!
//! This module handles loading and saving guisu configuration.

use crate::Result;
use crate::variables::load_variables;
use guisu_core::platform::CURRENT_PLATFORM;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Auto boolean type supporting "auto", true, or false
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum AutoBool {
    #[default]
    Auto,
    #[serde(rename = "true")]
    True,
    #[serde(rename = "false")]
    False,
}

/// Icon display mode (similar to eza's --icons option)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum IconMode {
    /// Automatically show icons when output is a terminal
    #[default]
    #[serde(alias = "automatic")]
    Auto,
    /// Always show icons
    Always,
    /// Never show icons
    Never,
}

impl IconMode {
    /// Determine if icons should be shown based on mode and terminal detection
    pub fn should_show_icons(&self, is_tty: bool) -> bool {
        match self {
            Self::Always => true,
            Self::Never => false,
            Self::Auto => is_tty,
        }
    }
}

/// General configuration section
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    /// Source directory path (simplified name)
    #[serde(default, rename = "srcDir")]
    pub src_dir: Option<PathBuf>,

    /// Destination directory path (simplified name)
    #[serde(default, rename = "dstDir")]
    pub dst_dir: Option<PathBuf>,

    /// Subdirectory within source directory where dotfiles are stored
    /// Defaults to "home" to separate dotfiles from repository metadata (.git, .guisu)
    #[serde(default = "default_root_entry", rename = "rootEntry")]
    pub root_entry: PathBuf,

    /// Enable colored output
    #[serde(default = "default_color")]
    pub color: bool,

    /// Show progress bars
    #[serde(default = "default_progress")]
    pub progress: bool,

    /// Use builtin age encryption (auto, true, or false)
    #[serde(default, rename = "useBuiltinAge")]
    pub use_builtin_age: AutoBool,

    /// Use builtin git (auto, true, or false)
    #[serde(default, rename = "useBuiltinGit")]
    pub use_builtin_git: AutoBool,

    /// Custom editor command
    #[serde(default)]
    pub editor: Option<String>,

    /// Arguments to pass to the editor
    #[serde(default, rename = "editorArgs")]
    pub editor_args: Vec<String>,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            src_dir: None,
            dst_dir: None,
            root_entry: default_root_entry(),
            color: default_color(),
            progress: default_progress(),
            use_builtin_age: AutoBool::Auto,
            use_builtin_git: AutoBool::Auto,
            editor: None,
            editor_args: Vec::new(),
        }
    }
}

/// Ignore configuration section
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IgnoreConfig {
    /// Global ignore patterns for all platforms
    #[serde(default)]
    pub global: Vec<String>,

    /// Darwin (macOS) specific ignore patterns
    #[serde(default)]
    pub darwin: Vec<String>,

    /// Linux specific ignore patterns
    #[serde(default)]
    pub linux: Vec<String>,

    /// Windows specific ignore patterns
    #[serde(default)]
    pub windows: Vec<String>,
}

/// Bitwarden configuration
///
/// Configure which Bitwarden CLI to use: bw (official Node.js CLI) or rbw (Rust CLI)
///
/// ```toml
/// [bitwarden]
/// provider = "rbw"  # or "bw" (default)
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitwardenConfig {
    /// Which Bitwarden CLI to use: "bw" or "rbw"
    /// - "bw": Official Bitwarden CLI (Node.js based)
    /// - "rbw": Rust Bitwarden CLI (faster, daemon-based)
    #[serde(default = "default_bitwarden_provider")]
    pub provider: String,
}

fn default_bitwarden_provider() -> String {
    "bw".to_string()
}

impl Default for BitwardenConfig {
    fn default() -> Self {
        Self {
            provider: default_bitwarden_provider(),
        }
    }
}

/// UI configuration section
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    /// Icon display mode: "auto", "always", or "never"
    /// - auto: Show icons when output is a terminal (default)
    /// - always: Always show icons
    /// - never: Never show icons
    #[serde(default)]
    pub icons: IconMode,

    /// Diff format: "unified", "split", "inline"
    #[serde(default = "default_diff_format", rename = "diffFormat")]
    pub diff_format: String,

    /// Number of context lines for diffs
    #[serde(default = "default_context_lines", rename = "contextLines")]
    pub context_lines: usize,

    /// Number of lines to show in preview
    #[serde(default = "default_preview_lines", rename = "previewLines")]
    pub preview_lines: usize,
}

fn default_diff_format() -> String {
    "unified".to_string()
}

fn default_context_lines() -> usize {
    3
}

fn default_preview_lines() -> usize {
    10
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            icons: IconMode::default(),
            diff_format: default_diff_format(),
            context_lines: default_context_lines(),
            preview_lines: default_preview_lines(),
        }
    }
}

/// Age encryption configuration
///
/// Supports both chezmoi-compatible and simplified configurations:
///
/// ```toml
/// # Single identity and recipient
/// [age]
/// identity = "~/.config/guisu/key.txt"
/// recipient = "age1ql3z7hjy54pw3hyww5ayyfg7zqgvc7w3j2elw8zmrj2kg5sfn9aqmcac8p"
///
/// # Multiple identities and recipients
/// [age]
/// identities = ["~/.config/guisu/key1.txt", "~/.config/guisu/key2.txt"]
/// recipients = ["age1...", "age2..."]
///
/// # Symmetric encryption (same key for encryption and decryption)
/// [age]
/// identity = "~/.config/guisu/key.txt"
/// symmetric = true
///
/// # SSH key support
/// [age]
/// identity = "~/.ssh/id_ed25519"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgeConfig {
    /// Single identity file path (age or SSH key)
    /// Can use ~ for home directory
    /// Mutually exclusive with `identities`
    pub identity: Option<PathBuf>,

    /// Multiple identity file paths
    /// Mutually exclusive with `identity`
    pub identities: Option<Vec<PathBuf>>,

    /// Single recipient public key
    /// Mutually exclusive with `recipients`
    pub recipient: Option<String>,

    /// Multiple recipient public keys
    #[serde(default)]
    pub recipients: Vec<String>,

    /// Derive recipient from identity
    ///
    /// When true, automatically derives the public key from `identity` for encryption.
    /// This is required when no `recipient/recipients` are specified.
    ///
    /// Note: This still uses asymmetric age encryption - the identity's public key
    /// is derived and used as the recipient. The name `derive` accurately reflects
    /// this behavior (vs the misleading `symmetric` used by chezmoi).
    ///
    /// Configuration accepts both `derive` (recommended) and `symmetric` (legacy):
    /// ```toml
    /// [age]
    /// identity = "~/.config/guisu/key.txt"
    /// derive = true      # Recommended: derive recipient from identity
    /// # symmetric = true # Legacy name (still supported for backward compatibility)
    /// ```
    #[serde(default, alias = "symmetric")]
    pub derive: bool,
}

/// Guisu configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    /// General configuration section
    #[serde(default)]
    pub general: GeneralConfig,

    /// Age encryption configuration
    #[serde(default)]
    pub age: AgeConfig,

    /// Bitwarden configuration
    #[serde(default)]
    pub bitwarden: BitwardenConfig,

    /// UI configuration
    #[serde(default)]
    pub ui: UiConfig,

    /// Ignore patterns configuration
    #[serde(default)]
    pub ignore: IgnoreConfig,

    /// Template variables
    #[serde(default)]
    pub variables: IndexMap<String, serde_json::Value>,

    /// Base directory for resolving relative paths (not serialized)
    /// This is set internally when loading config from source directory
    #[serde(skip)]
    base_dir: Option<PathBuf>,
}

fn default_color() -> bool {
    true
}

fn default_progress() -> bool {
    true
}

fn default_root_entry() -> PathBuf {
    PathBuf::from("home")
}

impl Config {
    /// Load configuration from a file
    ///
    /// This is primarily used for testing. In production, use `load_from_source()` instead.
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path.as_ref()).map_err(|e| {
            guisu_core::Error::Message(format!(
                "Failed to read config file {:?}: {}",
                path.as_ref(),
                e
            ))
        })?;

        let mut config: Self = toml::from_str(&content).map_err(|e| {
            guisu_core::Error::Message(format!(
                "Failed to parse config file {:?}: {}",
                path.as_ref(),
                e
            ))
        })?;

        // Resolve relative paths using the config file's directory as base
        if let Some(parent) = path.as_ref().parent() {
            config.resolve_relative_paths(parent);
        }

        Ok(config)
    }

    /// Load configuration from TOML string
    ///
    /// This method parses configuration from a TOML string and resolves paths
    /// relative to the provided source directory.
    ///
    /// This is useful for loading configuration from rendered templates.
    pub fn from_toml_str(toml_content: &str, source_dir: &Path) -> Result<Self> {
        let mut config: Self = toml::from_str(toml_content).map_err(|e| {
            guisu_core::Error::Message(format!("Failed to parse config TOML: {}", e))
        })?;

        // Store the source directory for relative path resolution
        config.resolve_relative_paths(source_dir);

        Ok(config)
    }

    /// Load configuration from source directory (.guisu.toml)
    ///
    /// This method looks for .guisu.toml in the source directory and parses it directly.
    ///
    /// Note: For template support (.guisu.toml.j2), use the CLI wrapper which handles
    /// template rendering before calling this method.
    ///
    /// No syncing to ~/.config/guisu - config only exists in the repo.
    pub fn load_from_source(source_dir: &Path) -> Result<Self> {
        let config_path = source_dir.join(".guisu.toml");
        let template_path = source_dir.join(".guisu.toml.j2");

        // Check if .guisu.toml exists
        if !config_path.exists() {
            // If .guisu.toml.j2 exists, provide helpful error
            if template_path.exists() {
                return Err(guisu_core::Error::Message(
                    "Found .guisu.toml.j2 template but .guisu.toml is missing.\n\
                     \n\
                     Template rendering should be handled by CLI layer.\n\
                     This is likely a bug - please use Config::load_with_variables() instead."
                        .to_string(),
                ));
            }

            return Err(guisu_core::Error::Message(format!(
                "Configuration file not found in source directory.\n\
                 Expected: .guisu.toml in {}\n\
                 \n\
                 Create one with:\n\
                 cat > .guisu.toml << 'EOF'\n\
                 # Guisu configuration\n\
                 \n\
                 [age]\n\
                 identity = \"~/.config/guisu/key.txt\"\n\
                 # Or use a key in the repo:\n\
                 # identity = \"./key.txt\"\n\
                 EOF",
                source_dir.display()
            )));
        }

        // Read and parse TOML config
        let content = fs::read_to_string(&config_path).map_err(|e| {
            guisu_core::Error::Message(format!(
                "Failed to read config file {:?}: {}",
                config_path, e
            ))
        })?;

        Self::from_toml_str(&content, source_dir)
    }

    /// Resolve relative paths in configuration
    ///
    /// Converts relative paths (starting with `./ ` or `../`) to absolute paths
    /// based on the source directory. Also expands `~/` to home directory.
    fn resolve_relative_paths(&mut self, base_dir: &Path) {
        self.base_dir = Some(base_dir.to_path_buf());

        // Resolve general config paths
        if let Some(ref src_dir) = self.general.src_dir {
            self.general.src_dir = Some(Self::resolve_path(src_dir, base_dir));
        }
        if let Some(ref dst_dir) = self.general.dst_dir {
            self.general.dst_dir = Some(Self::resolve_path(dst_dir, base_dir));
        }
        // Note: root_entry should NOT be resolved - it's a relative subdirectory name
        // used with join() operations, not an absolute path

        // Resolve age identity paths
        if let Some(ref identity) = self.age.identity {
            self.age.identity = Some(Self::resolve_path(identity, base_dir));
        }
        if let Some(ref identities) = self.age.identities {
            self.age.identities = Some(
                identities
                    .iter()
                    .map(|p| Self::resolve_path(p, base_dir))
                    .collect(),
            );
        }
    }

    /// Resolve a single path: expand ~/ and resolve relative paths
    fn resolve_path(path: &PathBuf, base_dir: &Path) -> PathBuf {
        let path_str = path.to_string_lossy();

        // First expand ~/
        if let Some(stripped) = path_str.strip_prefix("~/") {
            if let Some(home) = ::dirs::home_dir() {
                return home.join(stripped);
            }
        } else if path_str == "~"
            && let Some(home) = ::dirs::home_dir()
        {
            return home;
        }

        // Then resolve relative paths (./ or ../)
        if path.is_relative() {
            base_dir.join(path)
        } else {
            path.clone()
        }
    }

    /// Load configuration with platform-aware variables
    ///
    /// This method extends the standard configuration loading with automatic
    /// variables loading from multiple sources. Variables are merged with
    /// section-based smart merge logic.
    ///
    /// # Variable Sources:
    ///
    /// 1. `.guisu.toml[variables]` - Global variables (no section)
    /// 2. `.guisu.toml[variables.section]` - Sectioned variables
    /// 3. `.guisu/{platform}/*.yaml` - Platform-specific YAML files
    /// 4. `.guisu/{platform}/*.toml` - Platform-specific TOML files
    ///
    /// # Smart Merge Behavior:
    ///
    /// Platform files override global config **within the same section only**.
    /// Different sections remain independent. This allows you to:
    /// - Define common variables in `.guisu.toml[variables]`
    /// - Organize variables by section (e.g., `[variables.visual]`)
    /// - Override specific sections per platform (e.g., `darwin/visual.yaml`)
    ///
    /// # Arguments
    ///
    /// * `config_path` - Optional path to config file (.guisu.toml)
    /// * `source_dir` - Path to source directory
    ///
    /// # Error Handling
    ///
    /// If loading platform variables fails (e.g., file not found, invalid TOML),
    /// the error is logged as debug and an empty variables map is used. This
    /// ensures that configuration loading never fails solely due to missing or
    /// invalid variables files.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use guisu_config::Config;
    /// use std::path::Path;
    ///
    /// # fn main() -> anyhow::Result<()> {
    /// // Load config with variables from default locations
    /// let config = Config::load_with_variables(
    ///     None,
    ///     Path::new("/home/user/dotfiles"),
    /// )?;
    ///
    /// // Access merged variables
    /// if let Some(email) = config.variables.get("email") {
    ///     println!("Email: {}", email);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Difference from `load()` and `load_from_source()`
    ///
    /// - `load()`: Loads only .guisu.toml, no variables merging
    /// - `load_from_source()`: Loads .guisu.toml from source directory, no variables merging
    /// - `load_with_variables()`: Loads config AND merges variables from all sources
    ///
    /// Use this method when you need full template variable support across
    /// multiple configuration sources.
    pub fn load_with_variables(_config_path: Option<&Path>, source_dir: &Path) -> Result<Self> {
        // 1. Load config from source directory (.guisu.toml or .guisu.toml.j2)
        // This already includes [variables] from the config file
        let mut config = Self::load_from_source(source_dir)?;

        // 2. Load platform-specific variables from .guisu/variables/*.toml
        // These will be merged with the variables from the config file
        let platform = CURRENT_PLATFORM.os;

        // Load variables from .guisu/variables directory
        let guisu_dir = source_dir.join(".guisu");
        if guisu_dir.exists() {
            match load_variables(&guisu_dir, platform) {
                Ok(loaded_vars) => {
                    // Merge platform variables with config variables
                    // Platform files override config file within the same section
                    for (key, value) in loaded_vars {
                        config.variables.insert(key, value);
                    }
                }
                Err(e) => {
                    tracing::debug!("Failed to load platform variables: {}", e);
                }
            }

            // 3. Load ignore patterns from .guisu/ignores.toml
            match crate::ignores::IgnoresConfig::load(source_dir) {
                Ok(ignores_config) => {
                    // Merge loaded ignores with config ignores
                    // .guisu/ignores.toml patterns are appended to config file patterns
                    config.ignore.global.extend(ignores_config.global);
                    config.ignore.darwin.extend(ignores_config.darwin);
                    config.ignore.linux.extend(ignores_config.linux);
                    config.ignore.windows.extend(ignores_config.windows);
                }
                Err(e) => {
                    tracing::debug!("Failed to load ignores: {}", e);
                }
            }
        }

        Ok(config)
    }

    /// Save configuration to a file
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let content = toml::to_string_pretty(self).map_err(|e| {
            guisu_core::Error::Message(format!("Failed to serialize config: {}", e))
        })?;

        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent).map_err(|e| {
                guisu_core::Error::Message(format!(
                    "Failed to create config directory {:?}: {}",
                    parent, e
                ))
            })?;
        }

        fs::write(path.as_ref(), content).map_err(|e| {
            guisu_core::Error::Message(format!(
                "Failed to write config file {:?}: {}",
                path.as_ref(),
                e
            ))
        })?;

        Ok(())
    }

    /// Get age recipients from configuration
    ///
    /// Returns recipients from either `recipient` (single) or `recipients` (multiple).
    /// Returns None if no recipients are configured.
    ///
    /// Merges both `recipient` and `recipients` fields to support flexible configurations.
    ///
    /// # Examples
    ///
    /// Single recipient:
    /// ```toml
    /// [age]
    /// identity = "~/.config/guisu/key.txt"
    /// recipient = "age1ql3z..."
    /// ```
    ///
    /// Multiple recipients:
    /// ```toml
    /// [age]
    /// identity = "~/.config/guisu/key.txt"
    /// recipients = [
    ///     "age1ql3z...",  # Alice
    ///     "age1zvk...",  # Bob
    /// ]
    /// ```
    ///
    /// Combined (both fields):
    /// ```toml
    /// [age]
    /// recipient = "age1ql3z..."
    /// recipients = ["age1zvk..."]  # Will be merged
    /// ```
    pub fn age_recipients(&self) -> Result<Option<Vec<guisu_crypto::Recipient>>> {
        // Collect recipients from both fields
        let mut recipient_strings = Vec::new();

        // Add single recipient if configured
        if let Some(ref recipient) = self.age.recipient {
            recipient_strings.push(recipient.clone());
        }

        // Add multiple recipients if configured
        if !self.age.recipients.is_empty() {
            recipient_strings.extend(self.age.recipients.clone());
        }

        if recipient_strings.is_empty() {
            return Ok(None);
        }

        // Parse all recipient strings
        let mut recipients = Vec::new();
        for recipient_str in recipient_strings {
            let recipient = recipient_str
                .parse::<guisu_crypto::Recipient>()
                .map_err(|e| {
                    guisu_core::Error::Message(format!(
                        "Failed to parse recipient '{}': {}",
                        recipient_str, e
                    ))
                })?;
            recipients.push(recipient);
        }

        Ok(Some(recipients))
    }

    /// Load all age identities from configuration
    ///
    /// Loads identities from all configured identity files, supporting both
    /// single `identity` and multiple `identities` configurations.
    /// Each identity file may contain multiple keys.
    ///
    /// # Returns
    ///
    /// - `Ok(Vec<Identity>)` - All loaded identities from all configured files
    /// - `Err(_)` - If no identities are configured or loading fails
    ///
    /// # Examples
    ///
    /// Single identity file:
    /// ```toml
    /// [age]
    /// identity = "~/.config/guisu/key.txt"
    /// ```
    ///
    /// Multiple identity files:
    /// ```toml
    /// [age]
    /// identities = [
    ///     "~/.config/guisu/key1.txt",
    ///     "~/.config/guisu/key2.txt",
    /// ]
    /// ```
    pub fn age_identities(&self) -> Result<Vec<guisu_crypto::Identity>> {
        use guisu_crypto::load_identities;

        // Collect all configured identity paths
        let mut identity_paths = Vec::new();

        if let Some(ref identity) = self.age.identity {
            identity_paths.push(identity.clone());
        }

        if let Some(ref identities) = self.age.identities {
            identity_paths.extend(identities.clone());
        }

        // Check if any identities are configured
        if identity_paths.is_empty() {
            return Err(guisu_core::Error::Message(
                "No identity file configured. Add to your config file (~/.config/guisu/config.toml):\n\n\
                 [age]\n\
                 identity = \"~/.config/guisu/key.txt\"\n\n\
                 Or use SSH key:\n\
                 identity = \"~/.ssh/id_ed25519\"\n\n\
                 Generate age key with: guisu age generate".to_string()
            ));
        }

        let mut all_identities = Vec::new();

        for identity_path in identity_paths {
            if !identity_path.exists() {
                return Err(guisu_core::Error::Message(format!(
                    "Identity file not found: {}\n\
                     \n\
                     For age key: guisu age generate\n\
                     For SSH key: use existing SSH private key",
                    identity_path.display()
                )));
            }

            let is_ssh = Self::is_ssh_identity(&identity_path);
            let identities = load_identities(&identity_path, is_ssh).map_err(|e| {
                guisu_core::Error::Message(format!(
                    "Failed to load identity from {}: {}",
                    identity_path.display(),
                    e
                ))
            })?;

            if identities.is_empty() {
                return Err(guisu_core::Error::Message(format!(
                    "No identities found in {}",
                    identity_path.display()
                )));
            }

            all_identities.extend(identities);
        }

        if all_identities.is_empty() {
            return Err(guisu_core::Error::Message(
                "No identities loaded from configured files".to_string(),
            ));
        }

        Ok(all_identities)
    }

    /// Check if an identity file is an SSH key
    ///
    /// Simple rule: SSH keys are in `.ssh` directory.
    /// For keys in other locations, users should set `symmetric = true` in config.
    ///
    /// # Examples
    ///
    /// SSH keys (auto-detected):
    /// - `~/.ssh/id_ed25519` → SSH key
    /// - `~/.ssh/age` → SSH key
    /// - `/home/user/.ssh/my_key` → SSH key
    ///
    /// Age keys (default):
    /// - `~/.config/guisu/key.txt` → Age key
    /// - `/etc/age/key.txt` → Age key (use symmetric=true if it's SSH)
    pub fn is_ssh_identity(path: &Path) -> bool {
        // Simple check: if path contains "/.ssh/" or ends with "/.ssh", it's an SSH key
        let path_str = path.to_string_lossy();
        path_str.contains("/.ssh/") || path_str.ends_with("/.ssh")
    }

    /// Get the actual dotfiles directory
    ///
    /// Returns source_dir/root_entry (defaults to source_dir/home).
    /// This separates dotfiles from repository metadata (.git, .guisu).
    pub fn dotfiles_dir(&self, source_dir: &Path) -> PathBuf {
        source_dir.join(&self.general.root_entry)
    }

    /// Get the source directory from general config
    pub fn source_dir(&self) -> Option<&PathBuf> {
        self.general.src_dir.as_ref()
    }

    /// Get the destination directory from general config
    pub fn dest_dir(&self) -> Option<&PathBuf> {
        self.general.dst_dir.as_ref()
    }

    /// Get the editor command with arguments
    ///
    /// Returns None if no editor is configured.
    /// Returns a Vec with the editor command as first element and args following.
    pub fn editor_command(&self) -> Option<Vec<String>> {
        self.general.editor.as_ref().map(|editor| {
            let mut cmd = vec![editor.clone()];
            cmd.extend(self.general.editor_args.clone());
            cmd
        })
    }

    /// Get platform-specific ignore patterns for the current platform
    ///
    /// Returns the patterns from the ignore section that apply to the current platform.
    /// This combines global patterns with platform-specific patterns.
    pub fn platform_ignore_patterns(&self) -> (Vec<String>, Vec<String>) {
        let platform = CURRENT_PLATFORM.os;
        let platform_patterns = match platform {
            "darwin" => &self.ignore.darwin,
            "linux" => &self.ignore.linux,
            "windows" => &self.ignore.windows,
            _ => &vec![],
        };

        (self.ignore.global.clone(), platform_patterns.clone())
    }
}

// Implement ConfigProvider trait for Config
impl guisu_core::ConfigProvider for Config {
    fn source_dir(&self) -> Option<&PathBuf> {
        self.general.src_dir.as_ref()
    }

    fn dest_dir(&self) -> Option<&PathBuf> {
        self.general.dst_dir.as_ref()
    }

    fn variables(&self) -> &IndexMap<String, serde_json::Value> {
        &self.variables
    }
}
