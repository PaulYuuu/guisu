//! Error types for CLI commands
//!
//! This module defines structured error types using thiserror, providing better
//! type safety and error handling compared to using anyhow::Error everywhere.

use std::path::PathBuf;
use thiserror::Error;

/// Error data for PathNotUnderDestination
///
/// Separated to allow boxing and reduce CommandError enum size
#[derive(Debug)]
pub struct PathNotUnderDestinationError {
    pub path: PathBuf,
    pub dest_dir: PathBuf,
}

/// Errors that can occur during command execution
#[non_exhaustive]
#[derive(Error, Debug)]
pub enum CommandError {
    /// Failed to load age identities
    #[error("Failed to load age identities: {0}")]
    IdentityLoadError(#[source] Box<dyn std::error::Error + Send + Sync>),

    /// Invalid path error
    #[error("Invalid path: {path}")]
    InvalidPath {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// Path not under destination directory
    ///
    /// Boxed to reduce enum size (48 bytes -> 16 bytes for this variant)
    #[error("Path {} is not under destination directory {}", .0.path.display(), .0.dest_dir.display())]
    PathNotUnderDestination(Box<PathNotUnderDestinationError>),

    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigError(#[source] Box<dyn std::error::Error + Send + Sync>),

    /// Template rendering error
    #[error("Template error: {0}")]
    TemplateError(#[source] Box<dyn std::error::Error + Send + Sync>),

    /// Encryption/decryption error
    #[error("Encryption error: {0}")]
    EncryptionError(#[source] Box<dyn std::error::Error + Send + Sync>),

    /// Git operation error
    #[error("Git error: {0}")]
    GitError(#[from] git2::Error),

    /// Database operation error
    #[error("Database error: {0}")]
    DatabaseError(#[source] Box<dyn std::error::Error + Send + Sync>),

    /// Apply operation failed
    #[error("Apply failed: {failed} out of {total} entries")]
    ApplyFailed { failed: usize, total: usize },

    /// File not found
    #[error("File not found: {0}")]
    FileNotFound(PathBuf),

    /// File already exists
    #[error("File already exists: {0}")]
    FileAlreadyExists(PathBuf),

    /// IO error
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// Generic error (for migration from anyhow)
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

// Additional From implementations for common error types
impl From<guisu_core::Error> for CommandError {
    fn from(err: guisu_core::Error) -> Self {
        Self::Other(err.into())
    }
}

impl From<guisu_engine::Error> for CommandError {
    fn from(err: guisu_engine::Error) -> Self {
        Self::Other(err.into())
    }
}

// Note: guisu_config, guisu_template, and guisu_crypto may not have
// their own error types, so errors from those crates will be wrapped
// in anyhow::Error and converted via the Other variant

/// Result type alias for command operations
pub type Result<T> = std::result::Result<T, CommandError>;

impl CommandError {
    /// Create a PathNotUnderDestination error
    pub fn path_not_under_dest(path: PathBuf, dest_dir: PathBuf) -> Self {
        Self::PathNotUnderDestination(Box::new(PathNotUnderDestinationError { path, dest_dir }))
    }

    /// Create an IdentityLoadError from any error type
    pub fn identity_load<E: std::error::Error + Send + Sync + 'static>(err: E) -> Self {
        Self::IdentityLoadError(Box::new(err))
    }

    /// Create a ConfigError from any error type
    pub fn config<E: std::error::Error + Send + Sync + 'static>(err: E) -> Self {
        Self::ConfigError(Box::new(err))
    }

    /// Create a TemplateError from any error type
    pub fn template<E: std::error::Error + Send + Sync + 'static>(err: E) -> Self {
        Self::TemplateError(Box::new(err))
    }

    /// Create an EncryptionError from any error type
    pub fn encryption<E: std::error::Error + Send + Sync + 'static>(err: E) -> Self {
        Self::EncryptionError(Box::new(err))
    }

    /// Create a DatabaseError from any error type
    pub fn database<E: std::error::Error + Send + Sync + 'static>(err: E) -> Self {
        Self::DatabaseError(Box::new(err))
    }
}
