//! Info command implementation
//!
//! Display current guisu status information.

use anyhow::Result;
use owo_colors::OwoColorize;
use std::path::Path;
use std::process::Command;
use tracing::debug;

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
    identity_files: Vec<String>,
    identity_note: Option<String>,
    derive: Option<String>,
    public_keys: Vec<String>,
    version: Option<String>,
}

#[derive(Debug, Serialize)]
struct BitwardenInfo {
    provider: Option<String>,
    version: Option<String>,
}

/// Run the info command
pub fn run(source_dir: &Path, config: &Config, all: bool, json: bool) -> Result<()> {
    // Validate configuration
    validate_configuration(source_dir)?;

    let info = gather_info(source_dir, config, all)?;

    if json {
        display_json(&info, config, all)?;
    } else {
        display_table(&info, config, all);
    }

    Ok(())
}

/// Gather all system information
fn gather_info(source_dir: &Path, config: &Config, all: bool) -> Result<InfoData> {
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
            git_sha: option_env!("VERGEN_GIT_SHA").map(|s| s.to_string()),
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

    Ok(InfoData {
        guisu: GuisuInfo {
            version: guisu_version,
            config: config_display,
            config_note,
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
            identity_files,
            identity_note,
            derive,
            public_keys,
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
    })
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
                remote.url().map(|s| s.to_string())
            } else {
                None
            };

            // Get current branch
            let branch = repo
                .head()
                .ok()
                .and_then(|head| head.shorthand().map(|s| s.to_string()))
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
    let command_version = match Command::new(&provider).arg("--version").output() {
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
        if !release.is_empty() {
            release
        } else {
            "unknown".to_string()
        }
    }

    #[cfg(not(unix))]
    {
        "unknown".to_string()
    }
}

/// Get OS name with version if possible using os_info crate
fn get_os_name() -> String {
    let info = os_info::get();

    // Format: "OS Type Version"
    let version = info.version();
    if version != &os_info::Version::Unknown {
        format!("{} {}", info.os_type(), version)
    } else {
        info.os_type().to_string()
    }
}

/// Get age encryption information
/// Returns: (identity_files, identity_note, derive, public_keys)
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
    let identity_note = if !all_exist {
        Some("some files not found".to_string())
    } else {
        None
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
fn display_table(info: &InfoData, config: &Config, all: bool) {
    // Guisu section
    println!("{}", "Guisu".bright_white().bold());
    print_row("Version", &info.guisu.version, true, None);
    print_row(
        "Config",
        &info.guisu.config,
        info.guisu.config_note.is_none(),
        info.guisu.config_note.as_deref(),
    );

    println!();

    // Build section (only shown if present = --all mode)
    if let Some(build) = info.build.as_ref() {
        println!("{}", "Build".bright_white().bold());
        print_row("Rustc", &build.rustc, true, None);
        if let Some(time) = build.timestamp.as_ref() {
            print_row("Timestamp", time, true, None)
        }
        if let Some(sha) = build.git_sha.as_ref() {
            print_row("Git SHA", sha, true, None)
        }
        println!();
    }

    // System section
    println!("{}", "System".bright_white().bold());
    print_row("OS", &info.system.os, true, None);
    print_row("Architecture", &info.system.architecture, true, None);
    if let Some(kernel) = info.system.kernel.as_ref() {
        print_row("Kernel", kernel, true, None)
    }

    println!();

    // Git section (only show if has content)
    if info.git.version.is_some()
        || info.git.repository.is_some()
        || info.git.branch.is_some()
        || info.git.sha.is_some()
    {
        println!("{}", "Git".bright_white().bold());

        // Show version only in --all mode
        if let Some(version) = info.git.version.as_ref() {
            print_row("Version", version, true, None)
        }

        // Show repository if available
        if let Some(repo) = info.git.repository.as_ref() {
            print_row("Repository", repo, true, None)
        }

        // Show branch (always show if available)
        if let Some(branch) = info.git.branch.as_ref() {
            let note = if info.git.dirty && info.git.version.is_none() {
                // Only show note in normal mode
                Some("uncommitted changes")
            } else {
                None
            };
            print_row(
                "Branch",
                branch,
                !info.git.dirty || info.git.version.is_some(),
                note,
            )
        }

        // Show SHA in --all mode
        if info.git.version.is_some()
            && let Some(sha) = info.git.sha.as_ref()
        {
            let note = if info.git.dirty {
                Some("uncommitted changes")
            } else {
                None
            };
            print_row("SHA", sha, !info.git.dirty, note)
        }

        println!();
    }

    // Age encryption section
    println!("{}", "Age".bright_white().bold());
    if let Some(version) = info.age.version.as_ref() {
        print_row("Version", version, true, None)
    }

    // Display identity files
    if info.age.identity_files.len() == 1 {
        print_row(
            "Identity",
            &info.age.identity_files[0],
            info.age.identity_note.is_none(),
            info.age.identity_note.as_deref(),
        );
    } else {
        for (i, file) in info.age.identity_files.iter().enumerate() {
            let label = if i == 0 {
                "Identities".to_string()
            } else {
                "".to_string()
            };
            print_row(
                &label,
                file,
                info.age.identity_note.is_none(),
                if i == 0 {
                    info.age.identity_note.as_deref()
                } else {
                    None
                },
            );
        }
    }

    if let Some(derive_val) = info.age.derive.as_ref() {
        print_row("Derive", derive_val, true, None)
    }

    // Display public keys
    if !info.age.public_keys.is_empty() {
        for (i, key) in info.age.public_keys.iter().enumerate() {
            let label = if i == 0 {
                if info.age.public_keys.len() == 1 {
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

    println!();

    // Bitwarden section (only show if has content)
    if info.bitwarden.provider.is_some() || info.bitwarden.version.is_some() {
        println!("{}", "Bitwarden".bright_white().bold());
        if let Some(provider) = info.bitwarden.provider.as_ref() {
            print_row("Provider", provider, true, None)
        }
        if let Some(version) = info.bitwarden.version.as_ref() {
            print_row("Version", version, true, None)
        }

        println!();
    }

    // Configuration section (only in --all mode)
    if all {
        show_config_table_simple(config);
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
            format!("({})", note_text).dimmed()
        );
    } else {
        println!("  {} {:14} {}", symbol, label, formatted_value);
    }
}

/// Display configuration in simplified table format (for --all mode)
fn show_config_table_simple(config: &Config) {
    println!("{}", "Configuration".bright_white().bold());

    // General section
    if let Some(ref editor) = config.general.editor {
        print_row("Editor", editor, true, None);
    }

    // Age section
    if !config.age.recipients.is_empty() {
        print_row(
            "Age Recipients",
            &format!("{} keys", config.age.recipients.len()),
            true,
            None,
        );
    }

    // Ignore patterns
    let total_ignores = config.ignore.global.len()
        + config.ignore.darwin.len()
        + config.ignore.linux.len()
        + config.ignore.windows.len();
    if total_ignores > 0 {
        print_row("Ignore Patterns", &total_ignores.to_string(), true, None);
    }

    println!();
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
    crate::load_config_with_template_support(None, source_dir)
        .map_err(|e| anyhow::anyhow!("Configuration validation failed: {}", e))?;

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
        println!("{}", json);
    } else {
        let json = serde_json::to_string_pretty(info)?;
        println!("{}", json);
    }
    Ok(())
}
