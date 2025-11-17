//! Guisu CLI library
//!
//! This library contains all the CLI logic for guisu, making it reusable
//! for testing and integration with other tools.

pub mod cmd;
pub mod stats;
pub mod ui;

use anyhow::Result;
use clap::{Parser, Subcommand};
use owo_colors::OwoColorize;
use std::path::PathBuf;

/// Guisu - A dotfile manager inspired by chezmoi
#[derive(Parser)]
#[command(name = "guisu")]
#[command(about = "Manage your dotfiles with guisu (归宿)")]
#[command(version)]
#[command(long_about = "Manage your dotfiles with guisu (归宿)

A fast, secure dotfile manager written in Rust.
Inspired by chezmoi, designed for simplicity and security.

Features:
  • Template support with Jinja2-like syntax
  • Age encryption for sensitive files
  • Git integration for version control
  • Cross-platform (macOS, Linux, Windows)")]
pub struct Cli {
    /// Path to the source directory
    #[arg(long, env = "GUISU_SOURCE_DIR", value_name = "DIR")]
    pub source: Option<PathBuf>,

    /// Path to the destination directory (usually $HOME)
    #[arg(long, env = "GUISU_DEST_DIR", value_name = "DIR")]
    pub dest: Option<PathBuf>,

    /// Path to the config file
    #[arg(long, env = "GUISU_CONFIG", value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Enable verbose output (shows DEBUG level logs)
    #[arg(short, long)]
    pub verbose: bool,

    /// Write logs to a file (useful for debugging)
    #[arg(long, env = "GUISU_LOG_FILE", value_name = "FILE")]
    pub log_file: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize a new source directory or clone from GitHub
    Init {
        /// Path to initialize, GitHub username, or GitHub repo (owner/repo).
        ///
        /// If not specified, defaults to ~/.local/share/guisu
        #[arg(
            value_name = "PATH_OR_REPO",
            long_help = "Path to initialize, GitHub username, or GitHub repo (owner/repo).

If not specified, defaults to ~/.local/share/guisu

Examples:
  • guisu init
      → Initialize at ~/.local/share/guisu (default)

  • guisu init .
      → Initialize at current directory

  • guisu init PaulYuuu
      → Clone github.com/PaulYuuu/dotfiles to ~/.local/share/guisu

  • guisu init owner/repo
      → Clone github.com/owner/repo to ~/.local/share/guisu

  • guisu --source /custom/path init username
      → Clone to custom path /custom/path"
        )]
        path_or_repo: Option<String>,

        /// Apply changes after initialization
        #[arg(short, long)]
        apply: bool,

        /// Create a shallow clone with the specified depth (commits)
        #[arg(short, long)]
        depth: Option<usize>,

        /// Specify the branch to clone (default: repository's default branch)
        #[arg(short, long)]
        branch: Option<String>,

        /// Use SSH instead of HTTPS when guessing repo URL
        #[arg(long)]
        ssh: bool,

        /// Checkout submodules recursively
        #[arg(long)]
        recurse_submodules: bool,
    },

    /// Add a file to the source directory
    Add {
        /// Files to add to the source directory
        #[arg(required = true)]
        files: Vec<PathBuf>,

        /// Mark file as a template
        #[arg(short, long)]
        template: bool,

        /// Auto-detect template variables and create templates (implies --template)
        #[arg(short, long)]
        autotemplate: bool,

        /// Encrypt the file with age
        #[arg(short = 'E', long)]
        encrypt: bool,

        /// Mark file for create-once (only copy if destination doesn't exist)
        #[arg(short, long)]
        create: bool,

        /// Force overwrite if file already exists in source
        #[arg(short, long)]
        force: bool,

        /// How to handle files containing secrets (ignore, warning, error)
        #[arg(long, default_value = "warning")]
        secrets: String,
    },

    /// Apply the source state to the destination
    Apply {
        /// Specific files to apply (all if not specified)
        files: Vec<PathBuf>,

        /// Dry run - show what would be done
        #[arg(short = 'n', long)]
        dry_run: bool,

        /// Force overwrite of changed files
        #[arg(short, long)]
        force: bool,

        /// Interactive mode - prompt on conflicts
        #[arg(short, long)]
        interactive: bool,

        /// Include only these entry types (comma-separated: files,dirs,symlinks,templates,encrypted)
        #[arg(long, value_delimiter = ',')]
        include: Vec<String>,

        /// Exclude these entry types (comma-separated: files,dirs,symlinks,templates,encrypted)
        #[arg(long, value_delimiter = ',')]
        exclude: Vec<String>,
    },

    /// Show differences between source and destination
    Diff {
        /// Specific files to diff (all if not specified)
        files: Vec<PathBuf>,

        /// Use pager for output
        #[arg(long)]
        pager: bool,

        /// Interactive diff viewer
        #[arg(short, long)]
        interactive: bool,
    },

    /// Manage age encryption identities
    #[command(subcommand)]
    Age(AgeCommands),

    /// Show status of managed files
    Status {
        /// Specific files to check (all if not specified)
        files: Vec<PathBuf>,

        /// Show all files including synced ones
        #[arg(short, long)]
        all: bool,

        /// Display output in tree format
        #[arg(long)]
        tree: bool,
    },

    /// Display file contents (decrypt and render templates)
    Cat {
        /// Files to display
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },

    /// Edit the source state of a target file
    Edit {
        /// Target file to edit (e.g., ~/.bashrc)
        #[arg(required = true)]
        target: PathBuf,

        /// Apply changes after editing
        #[arg(short, long)]
        apply: bool,
    },

    /// View ignored files and patterns
    #[command(subcommand)]
    Ignored(IgnoredCommands),

    /// Manage template files
    #[command(subcommand)]
    Templates(TemplatesCommands),

    /// Pull and apply any changes from the source repository
    #[command(long_about = "Pull and apply any changes from the source repository

This command fetches the latest changes from the remote repository (origin)
and fast-forwards your local repository. If --apply is true (default), it
will also apply the changes to your destination directory.

The update will fail if:
  • The source directory is not a git repository
  • There is no remote named 'origin'
  • A fast-forward merge is not possible (diverged branches)

Examples:
  • guisu update
      → Pull changes and apply them

  • guisu update --no-apply
      → Pull changes without applying

  • guisu update && guisu status
      → Pull changes, apply, then check status

  • guisu update --rebase
      → Use rebase instead of fast-forward when branches diverge")]
    Update {
        /// Apply changes after pulling (default: true)
        #[arg(short, long, default_value_t = true)]
        apply: bool,

        /// Use rebase instead of merge when branches have diverged
        #[arg(short, long)]
        rebase: bool,
    },

    /// Display guisu status information and validate configuration
    Info {
        /// Show all details (build info, versions, public keys, configuration, etc.)
        #[arg(long)]
        all: bool,

        /// Output in JSON format (default: table format)
        #[arg(long)]
        json: bool,
    },

    /// Display all template variables
    Variables {
        /// Output in JSON format (default: pretty format)
        #[arg(long)]
        json: bool,

        /// Show only builtin (system) variables
        #[arg(long)]
        builtin: bool,

        /// Show only user-defined variables
        #[arg(long)]
        user: bool,
    },

    /// Manage hooks (run, list, show)
    #[command(subcommand)]
    Hooks(HooksCommands),
}

#[derive(Subcommand)]
pub enum AgeCommands {
    /// Generate a new age identity
    Generate {
        /// Output file (default: ~/.config/guisu/key.txt)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Show the public key for the current identity
    Show,

    /// Encrypt a value using inline encryption format
    ///
    /// This encrypts a plaintext value and outputs it in the compact `age:base64...`
    /// format suitable for embedding in configuration files.
    Encrypt {
        /// Value to encrypt (if not provided, reads from stdin)
        value: Option<String>,

        /// Interactive mode - prompts for input
        #[arg(short, long)]
        interactive: bool,

        /// Recipients to encrypt for (age public keys or SSH public keys)
        ///
        /// If not specified, uses all identities from config.
        /// Can be specified multiple times to encrypt for multiple recipients.
        ///
        /// Examples:
        ///   --recipient age1ql3z7hjy54pw3hyww5ayyfg7zqgvc7w3j2elw8zmrj2kg5sfn9aqmcac8p
        ///   --recipient ssh-ed25519 AAAAC3NzaC1lZDI1NTE5...
        #[arg(short, long)]
        recipients: Vec<String>,
    },

    /// Decrypt an inline encrypted value
    ///
    /// This decrypts a value in the `age:base64...` format and outputs the plaintext.
    Decrypt {
        /// Encrypted value to decrypt
        #[arg(required = true)]
        value: String,
    },

    /// Migrate encrypted files from old keys to new keys
    ///
    /// This command re-encrypts all encrypted files and inline encrypted values
    /// in your source directory from old identities to new recipients.
    Migrate {
        /// Old identity files (private keys) to decrypt with
        #[arg(long = "from", required = true)]
        old_identities: Vec<PathBuf>,

        /// New identity files (private keys) to encrypt with
        /// Public keys will be automatically extracted from these identities
        #[arg(long = "to", required = true)]
        new_identities: Vec<PathBuf>,

        /// Dry run - show what would be migrated without making changes
        #[arg(short = 'n', long)]
        dry_run: bool,

        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
pub enum IgnoredCommands {
    /// List files that are ignored on the current platform
    List,

    /// Show ignore rules for the current platform
    Rules {
        /// Show rules for all platforms
        #[arg(short, long)]
        all: bool,
    },
}

#[derive(Subcommand)]
pub enum TemplatesCommands {
    /// List available template files for the current platform
    List,

    /// Show rendered content of a specific template
    Show {
        /// Template name to display
        #[arg(required = true)]
        name: String,
    },
}

#[derive(Subcommand)]
pub enum HooksCommands {
    /// Run hooks from configuration
    Run {
        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,

        /// Run only the specified hook by name (optional)
        #[arg(long)]
        hook: Option<String>,
    },

    /// List configured hooks
    List {
        /// Output format (simple, json)
        #[arg(short, long, default_value = "simple")]
        format: String,
    },

    /// Show detailed information about a specific hook
    Show {
        /// Name of the hook to show
        name: String,
    },
}

/// Main entry point for the CLI logic
pub fn run(cli: Cli) -> Result<()> {
    // Initialize logging based on verbosity
    guisu_config::logging::init(cli.verbose, cli.log_file.as_deref())?;

    // Save custom source for init command before it's consumed
    let custom_source = cli.source.clone();

    // First, load base config to determine source_dir
    let base_config = if let Some(source_dir) = guisu_config::default_source_dir()
        && source_dir.exists()
        && let Ok(config) = load_config_with_template_support(None, &source_dir)
    {
        config
    } else {
        guisu_config::Config::default()
    };

    // Determine source and destination directories
    let source_dir = cli
        .source
        .or_else(|| base_config.source_dir().cloned())
        .or_else(guisu_config::dirs::default_source_dir)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Could not determine source directory. Please specify with --source or set in config file."
            )
        })?;

    let dest_dir = cli
        .dest
        .or_else(|| base_config.dest_dir().cloned())
        .or_else(::dirs::home_dir)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Could not determine destination directory (home directory not found). \
                 Please specify with --dest or set in config file."
            )
        })?;

    // Handle init command separately (doesn't need config before directory creation)
    if let Commands::Init {
        path_or_repo,
        apply,
        depth,
        branch,
        ssh,
        recurse_submodules,
    } = cli.command
    {
        let init_result = crate::cmd::init::run(
            path_or_repo.as_deref(),
            custom_source.as_deref(),
            depth,
            branch.as_deref(),
            ssh,
            recurse_submodules,
        )?;

        // Apply if requested
        if apply && let Some(source_path) = init_result {
            println!("\nApplying changes...");
            // Now load config after source directory is created
            let config = load_config_with_template_support(cli.config.as_deref(), &source_path)?;
            let options = crate::cmd::apply::ApplyOptions::default();
            crate::cmd::apply::run(&source_path, &dest_dir, &[], &options, &config)?;
        }
        return Ok(());
    }

    // For all other commands, load config now
    let config = load_config_with_template_support(cli.config.as_deref(), &source_dir)?;

    // Execute the command
    match cli.command {
        Commands::Init { .. } => {
            unreachable!("Init command already handled above")
        }
        Commands::Add {
            files,
            template,
            autotemplate,
            encrypt,
            create,
            force,
            secrets,
        } => {
            let options = cmd::add::AddOptions {
                template,
                autotemplate,
                encrypt,
                create,
                force,
                secrets,
            };
            cmd::add::run(&source_dir, &dest_dir, &files, &options, &config)?;
        }
        Commands::Apply {
            files,
            dry_run,
            force,
            interactive,
            include,
            exclude,
        } => {
            // Handle pre-apply hooks (unless it's a dry run)
            if !dry_run && let Err(e) = cmd::hooks::handle_hooks_pre(&source_dir, &config) {
                tracing::warn!("Pre-apply hooks failed: {}", e);
                println!(
                    "{}: Pre-apply hooks encountered issues: {}",
                    "Warning".yellow(),
                    e
                );
                println!("Continuing with file application...\n");
            }

            // Apply dotfiles
            let options = cmd::apply::ApplyOptions {
                dry_run,
                force,
                interactive,
                include,
                exclude,
            };
            cmd::apply::run(&source_dir, &dest_dir, &files, &options, &config)?;

            // Handle post-apply hooks (unless it's a dry run)
            if !dry_run && let Err(e) = cmd::hooks::handle_hooks_post(&source_dir, &config) {
                tracing::warn!("Post-apply hooks failed: {}", e);
                println!(
                    "{}: Post-apply hooks encountered issues: {}",
                    "Warning".yellow(),
                    e
                );
            }
        }
        Commands::Diff {
            files,
            pager,
            interactive,
        } => {
            cmd::diff::run(&source_dir, &dest_dir, &files, pager, interactive, &config)?;
        }
        Commands::Age(age_cmd) => match age_cmd {
            AgeCommands::Generate { output } => {
                cmd::age::generate(output)?;
            }
            AgeCommands::Show => {
                cmd::age::show(&config)?;
            }
            AgeCommands::Encrypt {
                value,
                interactive,
                recipients,
            } => {
                cmd::age::encrypt(value, interactive, recipients, &config)?;
            }
            AgeCommands::Decrypt { value } => {
                cmd::age::decrypt(value, &config)?;
            }
            AgeCommands::Migrate {
                old_identities,
                new_identities,
                dry_run,
                yes,
            } => {
                cmd::age::migrate(&source_dir, &old_identities, &new_identities, dry_run, yes)?;
            }
        },
        Commands::Status { files, all, tree } => {
            let output_format = if tree {
                cmd::status::OutputFormat::Tree
            } else {
                cmd::status::OutputFormat::Simple
            };
            cmd::status::run(&source_dir, &dest_dir, &config, &files, all, output_format)?;
        }
        Commands::Cat { files } => {
            cmd::cat::run(&source_dir, &dest_dir, &files, &config)?;
        }
        Commands::Edit { target, apply } => {
            cmd::edit::run(&source_dir, &dest_dir, &target, apply, &config)?;
        }
        Commands::Ignored(ignored_cmd) => match ignored_cmd {
            IgnoredCommands::List => {
                cmd::ignored::run_list(&source_dir, &config)?;
            }
            IgnoredCommands::Rules { all } => {
                cmd::ignored::run_show(&source_dir, &config, all)?;
            }
        },
        Commands::Templates(templates_cmd) => match templates_cmd {
            TemplatesCommands::List => {
                cmd::templates::run_list(&source_dir, &config)?;
            }
            TemplatesCommands::Show { name } => {
                cmd::templates::run_show(&source_dir, &dest_dir, &name, &config)?;
            }
        },
        Commands::Update { apply, rebase } => {
            cmd::update::run(&source_dir, &dest_dir, apply, rebase, &config)?;
        }
        Commands::Info { all, json } => {
            cmd::info::run(&source_dir, &config, all, json)?;
        }
        Commands::Variables {
            json,
            builtin,
            user,
        } => {
            // Determine filter based on flags
            let filter = match (builtin, user) {
                (true, true) => cmd::variables::VariableFilter::All, // Both = show all
                (true, false) => cmd::variables::VariableFilter::BuiltinOnly,
                (false, true) => cmd::variables::VariableFilter::UserOnly,
                (false, false) => cmd::variables::VariableFilter::All, // Default = show all
            };

            cmd::variables::run(&source_dir, &config, json, filter)?;
        }
        Commands::Hooks(hooks_cmd) => match hooks_cmd {
            HooksCommands::Run { yes, hook } => {
                cmd::hooks::run_hooks(&source_dir, &config, yes, hook.as_deref())?;
            }
            HooksCommands::List { format } => {
                cmd::hooks::run_list(&source_dir, &config, &format)?;
            }
            HooksCommands::Show { name } => {
                cmd::hooks::run_show(&source_dir, &config, &name)?;
            }
        },
    }

    Ok(())
}

// ============================================================================
// Common utility functions
// ============================================================================

/// Build filter paths from user-provided file arguments
///
/// This function converts file paths (which may be relative, absolute, or use ~)
/// into RelPath entries that can be used to filter source/target states.
///
/// # Arguments
///
/// * `files` - List of file paths provided by the user
/// * `dest_abs` - Absolute path to the destination directory
///
/// # Returns
///
/// Returns a vector of RelPath entries representing the files relative to dest_dir.
///
/// # Errors
///
/// Returns an error if:
/// - A file path cannot be canonicalized
/// - A file path is not under the destination directory
pub fn build_filter_paths(
    files: &[std::path::PathBuf],
    dest_abs: &guisu_core::path::AbsPath,
) -> Result<Vec<guisu_core::path::RelPath>> {
    use anyhow::Context;

    let mut rel_paths = Vec::new();

    for file_path in files {
        // Expand tilde in path
        let expanded_path = if file_path.starts_with("~") {
            if let Some(home) = dirs::home_dir() {
                let path_str = file_path.to_string_lossy();
                let without_tilde = path_str
                    .strip_prefix("~/")
                    .or(path_str.strip_prefix("~"))
                    .unwrap_or(&path_str);
                home.join(without_tilde)
            } else {
                file_path.clone()
            }
        } else {
            file_path.clone()
        };

        // Get absolute path
        let file_abs = if expanded_path.exists() {
            guisu_core::path::AbsPath::new(
                std::fs::canonicalize(&expanded_path).with_context(|| {
                    format!("Failed to resolve path: {}", expanded_path.display())
                })?,
            )?
        } else {
            // File doesn't exist yet, construct absolute path manually
            let abs_path = if expanded_path.is_absolute() {
                expanded_path
            } else {
                std::env::current_dir()?.join(&expanded_path)
            };
            guisu_core::path::AbsPath::new(abs_path)?
        };

        let rel = file_abs.strip_prefix(dest_abs).with_context(|| {
            format!(
                "File {} is not under destination directory {}",
                file_abs.as_path().display(),
                dest_abs.as_path().display()
            )
        })?;
        rel_paths.push(rel);
    }

    Ok(rel_paths)
}

/// Load configuration with support for template files (.guisu.toml.j2)
///
/// This helper function handles loading configuration from either .guisu.toml
/// or .guisu.toml.j2 (template) files. If a template file is found, it will
/// be rendered using system variables before parsing.
///
/// # Arguments
///
/// * `_config_path` - Optional path to config file (currently unused)
/// * `source_dir` - The source directory containing .guisu.toml or .guisu.toml.j2
///
/// # Returns
///
/// A loaded and configured Config instance with all variables merged.
pub fn load_config_with_template_support(
    _config_path: Option<&std::path::Path>,
    source_dir: &std::path::Path,
) -> Result<guisu_config::Config> {
    use std::fs;

    let toml_path = source_dir.join(".guisu.toml");
    let template_path = source_dir.join(".guisu.toml.j2");

    // If .guisu.toml exists, use the standard loader
    if toml_path.exists() {
        return guisu_config::Config::load_with_variables(None, source_dir)
            .map_err(|e| anyhow::anyhow!("Failed to load config: {}", e));
    }

    // If .guisu.toml.j2 exists, render it first
    if template_path.exists() {
        let template_content = fs::read_to_string(&template_path)?;

        // Create a minimal template engine for rendering config template
        // Use system variables only (no user variables since we haven't loaded config yet)
        let engine = guisu_template::TemplateEngine::new();

        // Create context with only system info
        let context = guisu_template::TemplateContext::new().with_guisu_info(
            source_dir.to_string_lossy().to_string(),
            dirs::home_dir()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            "home".to_string(),
        );

        // Render the template
        let rendered_toml = engine
            .render_str(&template_content, &context)
            .map_err(|e| anyhow::anyhow!("Failed to render .guisu.toml.j2 template: {}", e))?;

        // Parse the rendered TOML
        let mut config = guisu_config::Config::from_toml_str(&rendered_toml, source_dir)
            .map_err(|e| anyhow::anyhow!("Failed to parse rendered config: {}", e))?;

        // Load and merge platform-specific variables and ignores (same as load_with_variables)
        let platform = guisu_core::platform::CURRENT_PLATFORM.os;
        let guisu_dir = source_dir.join(".guisu");
        if guisu_dir.exists() {
            // Load variables from .guisu/variables directory
            if let Ok(loaded_vars) = guisu_config::variables::load_variables(&guisu_dir, platform) {
                for (key, value) in loaded_vars {
                    config.variables.insert(key, value);
                }
            }

            // Load ignore patterns from .guisu/ignores.toml
            if let Ok(ignores_config) = guisu_config::IgnoresConfig::load(source_dir) {
                config.ignore.global.extend(ignores_config.global);
                config.ignore.darwin.extend(ignores_config.darwin);
                config.ignore.linux.extend(ignores_config.linux);
                config.ignore.windows.extend(ignores_config.windows);
            }
        }

        return Ok(config);
    }

    // Neither file exists, return error
    Err(anyhow::anyhow!(
        "Configuration file not found in source directory.\n\
         Expected: .guisu.toml or .guisu.toml.j2 in {}\n\
         \n\
         Create .guisu.toml with:\n\
         cat > .guisu.toml << 'EOF'\n\
         # Guisu configuration\n\
         \n\
         [age]\n\
         identity = \"~/.config/guisu/key.txt\"\n\
         EOF",
        source_dir.display()
    ))
}

/// Create a template engine with common configuration
///
/// This helper function centralizes the template engine initialization logic
/// used across multiple commands (apply, cat, diff, status, templates).
///
/// # Arguments
///
/// * `source_dir` - The source directory path
/// * `identities` - Arc-wrapped vector of age identities for decryption
/// * `config` - The configuration object
///
/// # Returns
///
/// A configured TemplateEngine instance with:
/// - Age identities for inline decryption
/// - Template directory (if .guisu/templates exists)
/// - Bitwarden provider configuration
pub fn create_template_engine(
    source_dir: &std::path::Path,
    identities: std::sync::Arc<Vec<guisu_crypto::Identity>>,
    config: &guisu_config::Config,
) -> guisu_template::TemplateEngine {
    let templates_dir = source_dir.join(".guisu").join("templates");

    guisu_template::TemplateEngine::with_identities_arc_template_dir_and_bitwarden_provider(
        identities,
        if templates_dir.exists() {
            Some(templates_dir)
        } else {
            None
        },
        &config.bitwarden.provider,
    )
}

/// Helper function to create IO errors with path context
///
/// This reduces boilerplate when wrapping IO errors with path information.
///
/// # Arguments
///
/// * `path` - The path that caused the error
/// * `operation` - Description of the operation (e.g., "read config file", "write file")
/// * `error` - The underlying IO error
///
/// # Returns
///
/// A formatted error message with path context
///
/// # Examples
///
/// ```no_run
/// # use std::fs;
/// # use std::path::Path;
/// # use guisu_cli::path_io_error;
/// let path = Path::new("config.toml");
/// let content = fs::read_to_string(path)
///     .map_err(|e| path_io_error(path, "read config file", e))?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn path_io_error(
    path: &std::path::Path,
    operation: &str,
    error: std::io::Error,
) -> anyhow::Error {
    anyhow::anyhow!("Failed to {} '{}': {}", operation, path.display(), error)
}
