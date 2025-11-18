//! Common utilities and types shared across CLI commands

use anyhow::Result;
use guisu_config::Config;
use guisu_core::path::AbsPath;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Resolved paths for dotfile operations
///
/// This struct holds the canonicalized absolute paths needed for most
/// guisu commands. It handles the complexity of resolving the source
/// directory (which may differ from the input due to `root_entry` config)
/// and ensures all paths are properly canonicalized.
#[derive(Debug, Clone)]
pub struct ResolvedPaths {
    /// Original source directory (may contain .guisu directory)
    pub source_dir: PathBuf,
    /// Absolute destination directory (canonicalized)
    pub dest_dir: AbsPath,
    /// Absolute dotfiles directory (source + root_entry if configured)
    pub dotfiles_dir: AbsPath,
}

impl ResolvedPaths {
    /// Resolve and canonicalize all paths for dotfile operations
    ///
    /// This function:
    /// 1. Determines the actual dotfiles directory (may be source_dir/root_entry)
    /// 2. Canonicalizes both the dotfiles directory and destination directory
    /// 3. Returns all three paths in a convenient struct
    ///
    /// # Arguments
    ///
    /// * `source_dir` - The source directory (typically ~/.local/share/guisu)
    /// * `dest_dir` - The destination directory (typically $HOME)
    /// * `config` - The guisu configuration (used to determine root_entry)
    ///
    /// # Returns
    ///
    /// Returns a `ResolvedPaths` struct containing all canonicalized paths.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The source directory cannot be canonicalized (doesn't exist)
    /// - The destination directory cannot be canonicalized (doesn't exist)
    /// - The paths cannot be converted to absolute paths
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use guisu_cli::common::ResolvedPaths;
    /// use guisu_config::Config;
    /// use std::path::Path;
    ///
    /// let config = Config::default();
    /// let paths = ResolvedPaths::resolve(
    ///     Path::new("~/.local/share/guisu"),
    ///     Path::new("/home/user"),
    ///     &config
    /// )?;
    ///
    /// // Use paths.dotfiles_dir for reading source files
    /// // Use paths.dest_dir for applying to destination
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn resolve(source_dir: &Path, dest_dir: &Path, config: &Config) -> Result<Self> {
        let dotfiles_dir = config.dotfiles_dir(source_dir);
        let dotfiles_abs = AbsPath::new(fs::canonicalize(&dotfiles_dir)?)?;
        let dest_abs = AbsPath::new(fs::canonicalize(dest_dir)?)?;

        Ok(Self {
            source_dir: source_dir.to_path_buf(),
            dest_dir: dest_abs,
            dotfiles_dir: dotfiles_abs,
        })
    }
}

/// Runtime context for CLI commands
///
/// This struct consolidates common parameters that are passed to most commands,
/// reducing parameter count and making it easier to add new shared resources.
///
/// # Benefits
///
/// - **Reduced boilerplate**: Commands receive one context instead of 4-5 parameters
/// - **Shared ownership**: Config is shared via Arc (no cloning)
/// - **Extensible**: Easy to add new shared resources without changing all commands
/// - **Testability**: Easy to create mock contexts for testing
///
/// # Examples
///
/// ```no_run
/// use guisu_cli::common::RuntimeContext;
/// use guisu_config::Config;
/// use std::path::Path;
///
/// let config = Config::default();
/// let context = RuntimeContext::new(
///     config,
///     Path::new("~/.local/share/guisu"),
///     Path::new("/home/user")
/// )?;
///
/// // Use in commands
/// // my_command::run(&context, &options)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Clone)]
pub struct RuntimeContext {
    /// Shared configuration (uses Arc to avoid cloning)
    pub config: Arc<Config>,
    /// Resolved and canonicalized paths
    pub paths: ResolvedPaths,
}

impl RuntimeContext {
    /// Create a new runtime context
    ///
    /// This will resolve and canonicalize all paths based on the configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - The guisu configuration
    /// * `source_dir` - The source directory (typically ~/.local/share/guisu)
    /// * `dest_dir` - The destination directory (typically $HOME)
    ///
    /// # Errors
    ///
    /// Returns an error if path resolution or canonicalization fails.
    pub fn new(config: Config, source_dir: &Path, dest_dir: &Path) -> Result<Self> {
        let paths = ResolvedPaths::resolve(source_dir, dest_dir, &config)?;
        Ok(Self {
            config: Arc::new(config),
            paths,
        })
    }

    /// Create a context from an already-resolved paths struct
    ///
    /// Useful when you've already resolved paths and want to create a context.
    pub fn from_parts(config: Arc<Config>, paths: ResolvedPaths) -> Self {
        Self { config, paths }
    }

    /// Get the source directory (original input, may contain .guisu)
    #[inline]
    pub fn source_dir(&self) -> &Path {
        &self.paths.source_dir
    }

    /// Get the destination directory (canonicalized)
    #[inline]
    pub fn dest_dir(&self) -> &AbsPath {
        &self.paths.dest_dir
    }

    /// Get the dotfiles directory (canonicalized, includes root_entry if configured)
    #[inline]
    pub fn dotfiles_dir(&self) -> &AbsPath {
        &self.paths.dotfiles_dir
    }
}
