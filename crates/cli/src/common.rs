//! Common utilities and types shared across CLI commands

use anyhow::Result;
use guisu_config::Config;
use guisu_core::path::AbsPath;
use std::fs;
use std::path::{Path, PathBuf};

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
