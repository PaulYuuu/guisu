//! Common utilities and types shared across CLI commands

use anyhow::Result;
use guisu_config::Config;
use guisu_core::path::AbsPath;
use once_cell::sync::OnceCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Resolved paths for dotfile operations
///
/// Holds canonicalized absolute paths, handling `root_entry` configuration.
#[derive(Debug, Clone)]
pub struct ResolvedPaths {
    pub source_dir: PathBuf,
    pub dest_dir: AbsPath,
    pub dotfiles_dir: AbsPath,
}

impl ResolvedPaths {
    /// Resolve and canonicalize paths from source/dest directories
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
/// Consolidates config, paths, and caches to reduce parameter passing.
#[derive(Clone)]
pub struct RuntimeContext {
    pub config: Arc<Config>,
    pub paths: ResolvedPaths,
    identities_cache: Arc<OnceCell<Arc<[guisu_crypto::Identity]>>>,
    guisu_dir_cache: Arc<OnceCell<PathBuf>>,
    templates_dir_cache: Arc<OnceCell<Option<PathBuf>>>,
}

impl RuntimeContext {
    /// Create runtime context with resolved and canonicalized paths
    pub fn new(config: Config, source_dir: &Path, dest_dir: &Path) -> Result<Self> {
        let paths = ResolvedPaths::resolve(source_dir, dest_dir, &config)?;
        Ok(Self {
            config: Arc::new(config),
            paths,
            identities_cache: Arc::new(OnceCell::new()),
            guisu_dir_cache: Arc::new(OnceCell::new()),
            templates_dir_cache: Arc::new(OnceCell::new()),
        })
    }

    /// Create context from already-resolved paths
    pub fn from_parts(config: Arc<Config>, paths: ResolvedPaths) -> Self {
        Self {
            config,
            paths,
            identities_cache: Arc::new(OnceCell::new()),
            guisu_dir_cache: Arc::new(OnceCell::new()),
            templates_dir_cache: Arc::new(OnceCell::new()),
        }
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

    /// Load age identities (cached)
    pub fn load_identities(&self) -> crate::error::Result<Arc<[guisu_crypto::Identity]>> {
        self.identities_cache
            .get_or_try_init(|| {
                let identities = self
                    .config
                    .age_identities()
                    .map_err(crate::error::CommandError::identity_load)?;
                Ok(Arc::from(identities.into_boxed_slice()))
            })
            .map(Arc::clone)
    }

    /// Get the primary identity or generate a dummy one
    pub fn primary_identity(&self) -> crate::error::Result<guisu_crypto::Identity> {
        let identities = self.load_identities()?;
        Ok(identities
            .first()
            .cloned()
            .unwrap_or_else(guisu_crypto::Identity::generate))
    }

    /// Get the .guisu directory path
    pub fn guisu_dir(&self) -> &PathBuf {
        self.guisu_dir_cache
            .get_or_init(|| self.source_dir().join(".guisu"))
    }

    /// Get the templates directory path if it exists
    pub fn templates_dir(&self) -> Option<&PathBuf> {
        self.templates_dir_cache
            .get_or_init(|| {
                let dir = self.source_dir().join(".guisu").join("templates");
                dir.exists().then_some(dir)
            })
            .as_ref()
    }
}
