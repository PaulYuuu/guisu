//! Info command implementation
//!
//! Display current guisu status information.

use anyhow::Result;
use clap::Args;
use owo_colors::OwoColorize;
use std::path::Path;
use std::process::Command as ProcessCommand;
use tracing::debug;

use crate::command::Command;
use crate::common::RuntimeContext;
use guisu_config::Config;

use serde::Serialize;

/// Information about guisu status
#[derive(Debug, Serialize)]
struct InfoData {
    guisu: GuisuInfo,
    build: Option<BuildInfo>,
    system: SystemInfo,
    git: GitInfo,
    age: AgeInfo,
    bitwarden: BitwardenInfo,
}

#[derive(Debug, Serialize)]
struct GuisuInfo {
    version: String,
    config: String,
    config_note: Option<String>,
    editor: Option<String>,
}

#[derive(Debug, Serialize)]
struct BuildInfo {
    rustc: String,
    timestamp: Option<String>,
    git_sha: Option<String>,
}

#[derive(Debug, Serialize)]
struct SystemInfo {
    os: String,
    architecture: String,
    kernel: Option<String>,
}

#[derive(Debug, Serialize)]
struct GitInfo {
    version: Option<String>,
    repository: Option<String>,
    branch: Option<String>,
    sha: Option<String>,
    dirty: bool,
}

#[derive(Debug, Serialize)]
struct AgeInfo {
    identities: Vec<String>,
    status: Option<String>,
    derive: Option<String>,
    public_keys: Vec<String>,
    recipients: Option<String>,
    version: Option<String>,
}

#[derive(Debug, Serialize)]
struct BitwardenInfo {
    provider: Option<String>,
    version: Option<String>,
}

/// Info command
#[derive(Args)]
pub struct InfoCommand {
    /// Show all details (build info, versions, public keys, configuration, etc.)
    #[arg(long)]
    pub all: bool,

    /// Output in JSON format (default: table format)
    #[arg(long)]
    pub json: bool,
}

impl Command for InfoCommand {
    type Output = ();
    fn execute(&self, context: &RuntimeContext) -> crate::error::Result<()> {
        run_impl(context.source_dir(), &context.config, self.all, self.json).map_err(Into::into)
    }
}

/// Run the info command implementation
fn run_impl(source_dir: &Path, config: &Config, all: bool, json: bool) -> Result<()> {
    // Validate configuration
    validate_configuration(source_dir)?;

    let info = gather_info(source_dir, config, all);

    if json {
        display_json(&info, config, all)?;
    } else {
        display_table(&info);
    }

    Ok(())
}

/// Gather all system information
fn gather_info(source_dir: &Path, config: &Config, all: bool) -> InfoData {
    debug!("Gathering system information");

    // Guisu information
    let guisu_version = env!("CARGO_PKG_VERSION").to_string();
    let config_file_path = find_config_file(source_dir);

    // Build information (only in --all mode)
    let build_info = if all {
        Some(BuildInfo {
            rustc: option_env!("VERGEN_RUSTC_SEMVER")
                .unwrap_or(env!("CARGO_PKG_RUST_VERSION"))
                .to_string(),
            timestamp: option_env!("VERGEN_BUILD_TIMESTAMP").and_then(|s| {
                chrono::DateTime::parse_from_rfc3339(s)
                    .ok()
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
            }),
            git_sha: option_env!("VERGEN_GIT_SHA").map(std::string::ToString::to_string),
        })
    } else {
        None
    };

    // System information
    let os = get_os_name();
    let architecture = std::env::consts::ARCH.to_string();
    let kernel = if all {
        Some(get_kernel_version())
    } else {
        None
    };

    // Git information
    let (git_repository, git_branch, git_sha, git_dirty) = get_git_info(source_dir);
    let git_version = if all {
        Some("builtin".to_string())
    } else {
        None
    };

    // Age encryption information
    let (identity_files, identity_note, derive, public_keys) = get_age_info(config, all);

    // Bitwarden information
    let (bw_provider, bw_command_version) = get_bitwarden_info(config);
    let bw_installed = bw_command_version.is_some();

    // Config display: filename in normal mode, full path in --all mode
    let (config_display, config_note) = match config_file_path {
        Some(ref path) => {
            let display = if all {
                // All mode: show full path
                path.clone()
            } else {
                // Normal mode: show filename only
                std::path::Path::new(path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(path)
                    .to_string()
            };
            (display, None)
        }
        None => ("not found".to_string(), Some("not found".to_string())),
    };

    InfoData {
        guisu: GuisuInfo {
            version: guisu_version,
            config: config_display,
            config_note,
            editor: if all {
                config.general.editor.clone()
            } else {
                None
            },
        },
        build: build_info,
        system: SystemInfo {
            os,
            architecture,
            kernel,
        },
        git: GitInfo {
            version: git_version,
            repository: git_repository,
            branch: git_branch,
            sha: git_sha,
            dirty: git_dirty,
        },
        age: AgeInfo {
            identities: identity_files,
            status: identity_note,
            derive,
            public_keys,
            recipients: if all && !config.age.recipients.is_empty() {
                Some(format!("{} keys", config.age.recipients.len()))
            } else {
                None
            },
            version: if all {
                Some("builtin".to_string())
            } else {
                None
            },
        },
        bitwarden: BitwardenInfo {
            provider: if bw_installed {
                Some(bw_provider)
            } else {
                None
            },
            version: if all { bw_command_version } else { None },
        },
    }
}

/// Find config file path
fn find_config_file(source_dir: &Path) -> Option<String> {
    let config_path = source_dir.join(".guisu.toml");
    let template_path = source_dir.join(".guisu.toml.j2");

    if config_path.exists() {
        Some(config_path.display().to_string())
    } else if template_path.exists() {
        Some(template_path.display().to_string())
    } else {
        None
    }
}

/// Get git repository information
/// Returns: (repository, branch, sha, dirty)
fn get_git_info(source_dir: &Path) -> (Option<String>, Option<String>, Option<String>, bool) {
    // Check if source_dir is a git repository
    if !source_dir.join(".git").exists() {
        return (None, None, None, false);
    }

    // Try to get git information using git2
    match git2::Repository::open(source_dir) {
        Ok(repo) => {
            // Get repository URL
            let repository = if let Ok(remote) = repo.find_remote("origin") {
                remote.url().map(std::string::ToString::to_string)
            } else {
                None
            };

            // Get current branch
            let branch = repo
                .head()
                .ok()
                .and_then(|head| head.shorthand().map(std::string::ToString::to_string))
                .or_else(|| {
                    // If HEAD doesn't exist (no commits yet), parse .git/HEAD file
                    let git_head = source_dir.join(".git").join("HEAD");
                    std::fs::read_to_string(git_head).ok().and_then(|content| {
                        content
                            .strip_prefix("ref: refs/heads/")
                            .map(|s| s.trim().to_string())
                    })
                });

            // Get HEAD commit SHA
            let sha = repo.head().ok().and_then(|head| {
                head.peel_to_commit()
                    .ok()
                    .map(|commit| commit.id().to_string()[..8].to_string())
            });

            // Check if working tree is dirty
            let dirty = repo
                .statuses(None)
                .map(|statuses| !statuses.is_empty())
                .unwrap_or(false);

            (repository, branch, sha, dirty)
        }
        Err(_) => (Some("local repository".to_string()), None, None, false),
    }
}

/// Get bitwarden provider and command version
fn get_bitwarden_info(config: &Config) -> (String, Option<String>) {
    // Get provider from config (default is "bw")
    let provider = config.bitwarden.provider.clone();

    // Get command version based on provider
    let command_version = match ProcessCommand::new(&provider).arg("--version").output() {
        Ok(output) if output.status.success() => {
            let version_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            // Remove provider prefix (e.g., "rbw 1.14.1" -> "1.14.1")
            let version = version_str
                .strip_prefix("rbw ")
                .or_else(|| version_str.strip_prefix("bw "))
                .unwrap_or(&version_str)
                .to_string();
            Some(version)
        }
        _ => None,
    };

    (provider, command_version)
}

/// Get kernel version using rustix uname system call
fn get_kernel_version() -> String {
    #[cfg(unix)]
    {
        // Safe: rustix provides a safe wrapper around uname()
        let info = rustix::system::uname();
        let release = info.release().to_string_lossy().to_string();
        if release.is_empty() {
            "unknown".to_string()
        } else {
            release
        }
    }

    #[cfg(not(unix))]
    {
        "unknown".to_string()
    }
}

/// Get OS name with version if possible using `os_info` crate
fn get_os_name() -> String {
    let info = os_info::get();

    // Format: "OS Type Version"
    let version = info.version();
    if version == &os_info::Version::Unknown {
        info.os_type().to_string()
    } else {
        format!("{} {}", info.os_type(), version)
    }
}

/// Get age encryption information
/// Returns: (`identity_files`, `identity_note`, derive, `public_keys`)
fn get_age_info(
    config: &Config,
    all: bool,
) -> (Vec<String>, Option<String>, Option<String>, Vec<String>) {
    // Collect all configured identity file paths
    let mut identity_paths = Vec::new();

    if let Some(ref identity) = config.age.identity {
        identity_paths.push(identity.clone());
    }

    if let Some(ref identities) = config.age.identities {
        identity_paths.extend(identities.clone());
    }

    // If no identities configured, use default path for display
    if identity_paths.is_empty() {
        let default_path = guisu_config::dirs::default_age_identity()
            .unwrap_or_else(|| std::path::PathBuf::from("~/.config/guisu/key.txt"));
        return (
            vec![default_path.display().to_string()],
            Some("not configured".to_string()),
            None,
            vec![],
        );
    }

    let path_strs: Vec<String> = identity_paths
        .iter()
        .map(|p| p.display().to_string())
        .collect();

    // Check if all files exist
    let all_exist = identity_paths.iter().all(|p| p.exists());
    let identity_note = if all_exist {
        None
    } else {
        Some("some files not found".to_string())
    };

    // Read derive flag from config
    let derive = Some(config.age.derive.to_string());

    // Try to extract public keys (only in --all mode)
    let public_keys = if all {
        extract_public_keys(config)
    } else {
        vec![]
    };

    (path_strs, identity_note, derive, public_keys)
}

/// Extract all public keys from configured identities
fn extract_public_keys(config: &Config) -> Vec<String> {
    // Load all configured identities and return all public keys
    match config.age_identities() {
        Ok(identities) => identities
            .iter()
            .map(|id| id.to_public().to_string())
            .collect(),
        Err(e) => {
            debug!("Failed to load identities: {}", e);
            vec![]
        }
    }
}

/// Display information in table format
fn display_table(info: &InfoData) {
    display_guisu_section(&info.guisu);
    display_build_section(info.build.as_ref());
    display_system_section(&info.system);
    display_git_section(&info.git);
    display_age_section(&info.age);
    display_bitwarden_section(&info.bitwarden);
}

/// Display guisu version and configuration
fn display_guisu_section(guisu: &GuisuInfo) {
    println!("{}", "Guisu".bright_white().bold());
    print_row("Version", &guisu.version, true, None);
    print_row(
        "Config",
        &guisu.config,
        guisu.config_note.is_none(),
        guisu.config_note.as_deref(),
    );
    if let Some(ref editor) = guisu.editor {
        print_row("Editor", editor, true, None);
    }
    println!();
}

/// Display build information (if present)
fn display_build_section(build: Option<&BuildInfo>) {
    if let Some(build) = build {
        println!("{}", "Build".bright_white().bold());
        print_row("Rustc", &build.rustc, true, None);
        if let Some(time) = build.timestamp.as_ref() {
            print_row("Timestamp", time, true, None);
        }
        if let Some(sha) = build.git_sha.as_ref() {
            print_row("Git SHA", sha, true, None);
        }
        println!();
    }
}

/// Display system information
fn display_system_section(system: &SystemInfo) {
    println!("{}", "System".bright_white().bold());
    print_row("OS", &system.os, true, None);
    print_row("Architecture", &system.architecture, true, None);
    if let Some(kernel) = system.kernel.as_ref() {
        print_row("Kernel", kernel, true, None);
    }
    println!();
}

/// Display git repository information
fn display_git_section(git: &GitInfo) {
    if git.version.is_some()
        || git.repository.is_some()
        || git.branch.is_some()
        || git.sha.is_some()
    {
        println!("{}", "Git".bright_white().bold());

        if let Some(version) = git.version.as_ref() {
            print_row("Version", version, true, None);
        }

        if let Some(repo) = git.repository.as_ref() {
            print_row("Repository", repo, true, None);
        }

        if let Some(branch) = git.branch.as_ref() {
            let note = if git.dirty && git.version.is_none() {
                Some("uncommitted changes")
            } else {
                None
            };
            print_row("Branch", branch, !git.dirty || git.version.is_some(), note);
        }

        if git.version.is_some()
            && let Some(sha) = git.sha.as_ref()
        {
            let note = if git.dirty {
                Some("uncommitted changes")
            } else {
                None
            };
            print_row("SHA", sha, !git.dirty, note);
        }

        println!();
    }
}

/// Display age encryption information
fn display_age_section(age: &AgeInfo) {
    println!("{}", "Age".bright_white().bold());

    if let Some(version) = age.version.as_ref() {
        print_row("Version", version, true, None);
    }

    display_age_identity_files(age);

    if let Some(derive_val) = age.derive.as_ref() {
        print_row("Derive", derive_val, true, None);
    }

    display_age_public_keys(&age.public_keys);

    if let Some(ref recipients) = age.recipients {
        print_row("Recipients", recipients, true, None);
    }

    println!();
}

/// Display age identity files (single or multiple)
fn display_age_identity_files(age: &AgeInfo) {
    if age.identities.len() == 1 {
        print_row(
            "Identity",
            &age.identities[0],
            age.status.is_none(),
            age.status.as_deref(),
        );
    } else {
        for (i, file) in age.identities.iter().enumerate() {
            let label = if i == 0 {
                "Identities".to_string()
            } else {
                String::new()
            };
            print_row(
                &label,
                file,
                age.status.is_none(),
                if i == 0 { age.status.as_deref() } else { None },
            );
        }
    }
}

/// Display age public keys
fn display_age_public_keys(public_keys: &[String]) {
    if !public_keys.is_empty() {
        for (i, key) in public_keys.iter().enumerate() {
            let label = if i == 0 {
                if public_keys.len() == 1 {
                    "Public key"
                } else {
                    "Public keys"
                }
            } else {
                ""
            };
            print_row(label, key, true, None);
        }
    }
}

/// Display bitwarden information
fn display_bitwarden_section(bitwarden: &BitwardenInfo) {
    if bitwarden.provider.is_some() || bitwarden.version.is_some() {
        println!("{}", "Bitwarden".bright_white().bold());
        if let Some(provider) = bitwarden.provider.as_ref() {
            print_row("Provider", provider, true, None);
        }
        if let Some(version) = bitwarden.version.as_ref() {
            print_row("Version", version, true, None);
        }
        println!();
    }
}

/// Print a single table row with status indicator
fn print_row(label: &str, value: &str, ok: bool, note: Option<&str>) {
    let symbol = if ok {
        "✓".bright_green().to_string()
    } else if note.is_some() {
        "✗".bright_red().to_string()
    } else {
        "⚠".yellow().to_string()
    };

    let formatted_value = if ok {
        value.bright_white().to_string()
    } else {
        value.dimmed().to_string()
    };

    if let Some(note_text) = note {
        println!(
            "  {} {:14} {} {}",
            symbol,
            label,
            formatted_value,
            format!("({note_text})").dimmed()
        );
    } else {
        println!("  {symbol} {label:14} {formatted_value}");
    }
}

/// Validate configuration file
fn validate_configuration(source_dir: &Path) -> Result<()> {
    // Check if .guisu.toml or .guisu.toml.j2 exists
    let config_file = source_dir.join(".guisu.toml");
    let config_template = source_dir.join(".guisu.toml.j2");

    if !config_template.exists() && !config_file.exists() {
        anyhow::bail!(
            "Configuration file not found.\n\
             Expected: .guisu.toml or .guisu.toml.j2 in {}",
            source_dir.display()
        );
    }

    // Try to load config to validate it
    crate::load_config_with_template_support(None, source_dir, None)
        .map_err(|e| anyhow::anyhow!("Configuration validation failed: {e}"))?;

    Ok(())
}

/// Display info data in JSON format
fn display_json(info: &InfoData, config: &Config, all: bool) -> Result<()> {
    if all {
        // Include configuration in JSON output
        use serde::Serialize;

        #[derive(Serialize)]
        struct InfoWithConfig<'a> {
            #[serde(flatten)]
            info: &'a InfoData,
            config: ConfigDisplay<'a>,
        }

        #[derive(Serialize)]
        struct ConfigDisplay<'a> {
            general: &'a guisu_config::GeneralConfig,
            age: &'a guisu_config::AgeConfig,
            bitwarden: &'a guisu_config::BitwardenConfig,
            ignore: &'a guisu_config::IgnoreConfig,
        }

        let output = InfoWithConfig {
            info,
            config: ConfigDisplay {
                general: &config.general,
                age: &config.age,
                bitwarden: &config.bitwarden,
                ignore: &config.ignore,
            },
        };

        let json = serde_json::to_string_pretty(&output)?;
        println!("{json}");
    } else {
        let json = serde_json::to_string_pretty(info)?;
        println!("{json}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use super::*;

    // Tests for InfoCommand

    #[test]
    fn test_info_command_default() {
        let cmd = InfoCommand {
            all: false,
            json: false,
        };

        assert!(!cmd.all);
        assert!(!cmd.json);
    }

    #[test]
    fn test_info_command_all_flag() {
        let cmd = InfoCommand {
            all: true,
            json: false,
        };

        assert!(cmd.all);
        assert!(!cmd.json);
    }

    #[test]
    fn test_info_command_json_flag() {
        let cmd = InfoCommand {
            all: false,
            json: true,
        };

        assert!(!cmd.all);
        assert!(cmd.json);
    }

    #[test]
    fn test_info_command_both_flags() {
        let cmd = InfoCommand {
            all: true,
            json: true,
        };

        assert!(cmd.all);
        assert!(cmd.json);
    }

    // Tests for InfoData structures

    #[test]
    fn test_info_data_debug() {
        let info = InfoData {
            guisu: GuisuInfo {
                version: "test".to_string(),
                config: "/test/.guisu.toml".to_string(),
                config_note: Some("note".to_string()),
                editor: None,
            },
            build: Some(BuildInfo {
                rustc: "1.70.0".to_string(),
                timestamp: Some("2025-01-01".to_string()),
                git_sha: Some("abc123".to_string()),
            }),
            system: SystemInfo {
                os: "Linux".to_string(),
                architecture: "x86_64".to_string(),
                kernel: Some("6.0.0".to_string()),
            },
            git: GitInfo {
                version: Some("builtin".to_string()),
                repository: Some("repo".to_string()),
                branch: Some("main".to_string()),
                sha: Some("abc".to_string()),
                dirty: false,
            },
            age: AgeInfo {
                identities: vec!["/path".to_string()],
                status: Some("note".to_string()),
                derive: None,
                public_keys: vec!["key1".to_string()],
                recipients: None,
                version: Some("1.0".to_string()),
            },
            bitwarden: BitwardenInfo {
                provider: Some("bw".to_string()),
                version: Some("1.0".to_string()),
            },
        };

        let debug_str = format!("{info:?}");
        assert!(debug_str.contains("InfoData"));
        assert!(debug_str.contains("test"));
    }

    #[test]
    fn test_info_data_serialize() {
        let info = InfoData {
            guisu: GuisuInfo {
                version: "test".to_string(),
                config: "/config".to_string(),
                config_note: None,
                editor: None,
            },
            build: None,
            system: SystemInfo {
                os: "Linux".to_string(),
                architecture: "x86_64".to_string(),
                kernel: None,
            },
            git: GitInfo {
                version: None,
                repository: None,
                branch: None,
                sha: None,
                dirty: false,
            },
            age: AgeInfo {
                identities: vec![],
                status: None,
                derive: None,
                public_keys: vec![],
                recipients: None,
                version: None,
            },
            bitwarden: BitwardenInfo {
                provider: None,
                version: None,
            },
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"version\":\"test\""));
        assert!(json.contains("\"os\":\"Linux\""));
    }

    #[test]
    fn test_guisu_info_debug() {
        let guisu = GuisuInfo {
            version: "1.0.0".to_string(),
            config: "/config/.guisu.toml".to_string(),
            config_note: Some("Template file".to_string()),
            editor: None,
        };

        let debug_str = format!("{guisu:?}");
        assert!(debug_str.contains("GuisuInfo"));
        assert!(debug_str.contains("1.0.0"));
    }

    #[test]
    fn test_guisu_info_serialize() {
        let guisu = GuisuInfo {
            version: "1.0.0".to_string(),
            config: "/config/.guisu.toml".to_string(),
            config_note: None,
            editor: None,
        };

        let json = serde_json::to_string(&guisu).unwrap();
        assert!(json.contains("\"version\":\"1.0.0\""));
        assert!(json.contains("\"config\":\"/config/.guisu.toml\""));
    }

    #[test]
    fn test_build_info_debug() {
        let build = BuildInfo {
            rustc: "1.70.0".to_string(),
            timestamp: Some("2025-01-01T00:00:00Z".to_string()),
            git_sha: Some("abc123".to_string()),
        };

        let debug_str = format!("{build:?}");
        assert!(debug_str.contains("BuildInfo"));
        assert!(debug_str.contains("abc123"));
    }

    #[test]
    fn test_build_info_serialize() {
        let build = BuildInfo {
            rustc: "1.70.0".to_string(),
            timestamp: None,
            git_sha: None,
        };

        let json = serde_json::to_string(&build).unwrap();
        assert!(json.contains("\"rustc\":\"1.70.0\""));
    }

    #[test]
    fn test_system_info_debug() {
        let system = SystemInfo {
            os: "Linux".to_string(),
            architecture: "x86_64".to_string(),
            kernel: Some("6.0.0".to_string()),
        };

        let debug_str = format!("{system:?}");
        assert!(debug_str.contains("SystemInfo"));
        assert!(debug_str.contains("Linux"));
    }

    #[test]
    fn test_system_info_serialize() {
        let system = SystemInfo {
            os: "macOS".to_string(),
            architecture: "aarch64".to_string(),
            kernel: None,
        };

        let json = serde_json::to_string(&system).unwrap();
        assert!(json.contains("\"os\":\"macOS\""));
        assert!(json.contains("\"architecture\":\"aarch64\""));
    }

    #[test]
    fn test_git_info_debug() {
        let git = GitInfo {
            version: Some("builtin".to_string()),
            repository: Some("repo".to_string()),
            branch: Some("main".to_string()),
            sha: Some("abc123".to_string()),
            dirty: false,
        };

        let debug_str = format!("{git:?}");
        assert!(debug_str.contains("GitInfo"));
        assert!(debug_str.contains("main"));
    }

    #[test]
    fn test_git_info_serialize() {
        let git = GitInfo {
            version: None,
            repository: None,
            branch: None,
            sha: None,
            dirty: false,
        };

        let json = serde_json::to_string(&git).unwrap();
        assert!(json.contains("\"dirty\":false"));
    }

    #[test]
    fn test_age_info_debug() {
        let age = AgeInfo {
            identities: vec!["/path1".to_string(), "/path2".to_string()],
            status: Some("Template identity".to_string()),
            derive: Some("key".to_string()),
            public_keys: vec!["age1...".to_string()],
            recipients: None,
            version: Some("1.0".to_string()),
        };

        let debug_str = format!("{age:?}");
        assert!(debug_str.contains("AgeInfo"));
        assert!(debug_str.contains("identities"));
    }

    #[test]
    fn test_age_info_serialize() {
        let age = AgeInfo {
            identities: vec!["/identity".to_string()],
            status: None,
            derive: None,
            public_keys: vec![],
            recipients: None,
            version: None,
        };

        let json = serde_json::to_string(&age).unwrap();
        assert!(json.contains("\"identities\":[\"/identity\"]"));
    }

    #[test]
    fn test_bitwarden_info_debug() {
        let bw = BitwardenInfo {
            provider: Some("bw".to_string()),
            version: Some("1.0.0".to_string()),
        };

        let debug_str = format!("{bw:?}");
        assert!(debug_str.contains("BitwardenInfo"));
        assert!(debug_str.contains("bw"));
    }

    #[test]
    fn test_bitwarden_info_serialize() {
        let bw = BitwardenInfo {
            provider: None,
            version: None,
        };

        let json = serde_json::to_string(&bw).unwrap();
        assert!(json.contains("null"));
    }

    // Tests for pure functions

    #[test]
    fn test_get_os_name_from_os_info() {
        // This function uses os_info::get() which returns the actual OS
        let os_name = get_os_name();

        // Just verify it returns a non-empty string
        assert!(!os_name.is_empty());
    }

    #[test]
    fn test_get_kernel_version() {
        // This function returns String (not Option)
        let kernel = get_kernel_version();

        // Just verify it returns a non-empty string
        assert!(!kernel.is_empty());
    }
}
