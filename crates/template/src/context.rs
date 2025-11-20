//! Template context management
//!
//! The context provides data that is available to templates during rendering.

use crate::info::ConfigInfo;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::env;

/// Context data available to templates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateContext {
    /// System information
    pub system: SystemInfo,

    /// Guisu-specific information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guisu: Option<GuisuInfo>,

    /// Environment variables
    pub env: IndexMap<String, String>,

    /// Custom user-defined variables
    /// These are flattened so they can be accessed directly in templates
    /// e.g., {{ my_var }} instead of {{ variables.my_var }}
    #[serde(flatten)]
    pub variables: IndexMap<String, serde_json::Value>,
}

/// Guisu-specific runtime information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuisuInfo {
    /// Source directory path (includes root_entry)
    #[serde(rename = "srcDir")]
    pub src_dir: String,

    /// Git working tree directory (repository root)
    #[serde(rename = "workingTree")]
    pub working_tree: String,

    /// Destination directory path
    #[serde(rename = "dstDir")]
    pub dst_dir: String,

    /// Root entry (subdirectory within source, defaults to "home")
    #[serde(rename = "rootEntry")]
    pub root_entry: String,

    /// Configuration object (exposed to templates)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<ConfigInfo>,
}

/// System information available to templates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    /// Operating system (e.g., "linux", "macos", "windows")
    pub os: String,

    /// Operating system family (e.g., "unix", "windows")
    #[serde(rename = "osFamily")]
    pub os_family: String,

    /// Linux distribution (e.g., "ubuntu", "fedora", "arch", "debian")
    /// Empty string on non-Linux systems
    pub distro: String,

    /// Linux distribution ID (lowercase, no spaces)
    #[serde(rename = "distroId")]
    pub distro_id: String,

    /// Linux distribution version (e.g., "22.04", "38")
    #[serde(rename = "distroVersion")]
    pub distro_version: String,

    /// Architecture (e.g., "x86_64", "aarch64")
    pub arch: String,

    /// Hostname
    pub hostname: String,

    /// Username
    pub username: String,

    /// User ID
    pub uid: String,

    /// Primary group ID
    pub gid: String,

    /// Primary group name
    pub group: String,

    /// Home directory path
    #[serde(rename = "homeDir")]
    pub home_dir: String,
}

impl TemplateContext {
    /// Create a new template context with system information
    pub fn new() -> Self {
        Self {
            system: SystemInfo::detect(),
            guisu: None,
            env: Self::collect_env(),
            variables: IndexMap::new(),
        }
    }

    /// Create a context with custom variables (takes ownership)
    ///
    /// For cases where you already have an owned `IndexMap`, this moves it without cloning.
    /// If you need to borrow the variables, use `with_variables_ref` instead.
    pub fn with_variables(mut self, variables: IndexMap<String, serde_json::Value>) -> Self {
        self.variables = variables;
        self
    }

    /// Create a context with custom variables (borrows and clones)
    ///
    /// This is a convenience method that accepts a reference and clones it internally.
    /// Use this when you need to preserve the original `IndexMap`.
    pub fn with_variables_ref(mut self, variables: &IndexMap<String, serde_json::Value>) -> Self {
        self.variables = variables.clone();
        self
    }

    /// Set guisu-specific information (source and destination directories, rootEntry)
    pub fn with_guisu_info(
        mut self,
        src_dir: String,
        working_tree: String,
        dst_dir: String,
        root_entry: String,
    ) -> Self {
        self.guisu = Some(GuisuInfo {
            src_dir,
            working_tree,
            dst_dir,
            root_entry,
            config: None,
        });
        self
    }

    /// Set guisu-specific information with config
    pub fn with_guisu_info_and_config(
        mut self,
        src_dir: String,
        working_tree: String,
        dst_dir: String,
        root_entry: String,
        config: ConfigInfo,
    ) -> Self {
        self.guisu = Some(GuisuInfo {
            src_dir,
            working_tree,
            dst_dir,
            root_entry,
            config: Some(config),
        });
        self
    }

    /// Add a custom variable
    pub fn add_variable(&mut self, key: String, value: serde_json::Value) {
        self.variables.insert(key, value);
    }

    /// Load variables from .guisu/variables/ directory and merge with config variables
    ///
    /// This method:
    /// 1. Loads platform-specific variables from .guisu/variables/{platform}/
    /// 2. Loads common variables from .guisu/variables/
    /// 3. Merges with config.variables (config overrides file-based variables)
    ///
    /// # Arguments
    ///
    /// * `source_dir` - Path to the source directory (containing .guisu/)
    /// * `config` - Configuration containing user-defined variables
    ///
    /// # Returns
    ///
    /// Self with all variables loaded and merged
    pub fn with_loaded_variables(
        mut self,
        source_dir: &std::path::Path,
        config: &guisu_config::Config,
    ) -> Result<Self, guisu_config::Error> {
        let guisu_dir = source_dir.join(".guisu");
        let platform_name = guisu_core::platform::CURRENT_PLATFORM.os;

        // Load variables from .guisu/variables/ directory
        let guisu_variables = if guisu_dir.exists() {
            guisu_config::variables::load_variables(&guisu_dir, platform_name)?
        } else {
            IndexMap::new()
        };

        // Merge variables: .guisu/variables/ first, then config (config overrides)
        let mut all_variables = guisu_variables;
        all_variables.extend(config.variables.iter().map(|(k, v)| (k.clone(), v.clone())));

        self.variables = all_variables;
        Ok(self)
    }

    /// Get an environment variable
    pub fn get_env(&self, key: &str) -> Option<&String> {
        self.env.get(key)
    }

    /// Collect environment variables
    fn collect_env() -> IndexMap<String, String> {
        env::vars().collect()
    }
}

impl Default for TemplateContext {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemInfo {
    /// Detect system information
    pub fn detect() -> Self {
        let (distro, distro_id, distro_version) = Self::detect_distro();

        Self {
            os: Self::detect_os(),
            os_family: Self::detect_os_family(),
            distro,
            distro_id,
            distro_version,
            arch: Self::detect_arch(),
            hostname: Self::detect_hostname(),
            username: Self::detect_username(),
            uid: Self::detect_uid(),
            gid: Self::detect_gid(),
            group: Self::detect_group(),
            home_dir: Self::detect_home_dir(),
        }
    }

    fn detect_os() -> String {
        #[cfg(target_os = "linux")]
        return "linux".to_string();

        #[cfg(target_os = "macos")]
        return "darwin".to_string();

        #[cfg(target_os = "windows")]
        return "windows".to_string();

        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        return env::consts::OS.to_string();
    }

    fn detect_os_family() -> String {
        #[cfg(unix)]
        return "unix".to_string();

        #[cfg(windows)]
        return "windows".to_string();

        #[cfg(not(any(unix, windows)))]
        return env::consts::FAMILY.to_string();
    }

    fn detect_arch() -> String {
        env::consts::ARCH.to_string()
    }

    fn detect_hostname() -> String {
        hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "unknown".to_string())
    }

    fn detect_username() -> String {
        env::var("USER")
            .or_else(|_| env::var("USERNAME"))
            .unwrap_or_else(|_| "unknown".to_string())
    }

    fn detect_uid() -> String {
        #[cfg(unix)]
        {
            // Safe: rustix provides a safe wrapper around getuid()
            rustix::process::getuid().as_raw().to_string()
        }

        #[cfg(not(unix))]
        {
            String::new()
        }
    }

    fn detect_gid() -> String {
        #[cfg(unix)]
        {
            // Safe: rustix provides a safe wrapper around getgid()
            rustix::process::getgid().as_raw().to_string()
        }

        #[cfg(not(unix))]
        {
            String::new()
        }
    }

    fn detect_group() -> String {
        #[cfg(unix)]
        {
            // Safe: uzers provides a safe API for querying group information
            let gid = rustix::process::getgid();
            if let Some(group) = uzers::get_group_by_gid(gid.as_raw()) {
                return group.name().to_string_lossy().to_string();
            }
            String::new()
        }

        #[cfg(not(unix))]
        {
            String::new()
        }
    }

    fn detect_home_dir() -> String {
        dirs::home_dir()
            .and_then(|p| p.to_str().map(|s| s.to_string()))
            .unwrap_or_default()
    }

    /// Detect Linux distribution information
    ///
    /// Returns (distro_name, distro_id, distro_version)
    ///
    /// On Linux, reads /etc/os-release file to determine the distribution.
    /// On non-Linux systems, returns empty strings.
    fn detect_distro() -> (String, String, String) {
        #[cfg(target_os = "linux")]
        {
            use std::fs;

            // Try to read /etc/os-release (standard location)
            let os_release_content = fs::read_to_string("/etc/os-release")
                .or_else(|_| fs::read_to_string("/usr/lib/os-release"))
                .unwrap_or_default();

            let mut name = String::new();
            let mut id = String::new();
            let mut version = String::new();

            for line in os_release_content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }

                if let Some(value) = line.strip_prefix("NAME=") {
                    name = Self::unquote(value);
                } else if let Some(value) = line.strip_prefix("ID=") {
                    id = Self::unquote(value);
                } else if let Some(value) = line.strip_prefix("VERSION_ID=") {
                    version = Self::unquote(value);
                }
            }

            // If we couldn't detect from os-release, try some fallbacks
            if id.is_empty() {
                if fs::metadata("/etc/fedora-release").is_ok() {
                    id = "fedora".to_string();
                } else if fs::metadata("/etc/debian_version").is_ok() {
                    id = "debian".to_string();
                } else if fs::metadata("/etc/arch-release").is_ok() {
                    id = "arch".to_string();
                } else if fs::metadata("/etc/redhat-release").is_ok() {
                    id = "rhel".to_string();
                }
            }

            (name, id, version)
        }

        #[cfg(not(target_os = "linux"))]
        {
            (String::new(), String::new(), String::new())
        }
    }

    /// Remove quotes from a string value
    #[cfg(target_os = "linux")]
    fn unquote(s: &str) -> String {
        let s = s.trim();
        if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
            s[1..s.len() - 1].to_string()
        } else {
            s.to_string()
        }
    }
}
