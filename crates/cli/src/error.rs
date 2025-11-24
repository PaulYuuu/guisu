//! Error types for CLI commands
//!
//! This module defines structured error types using thiserror, providing better
//! type safety and error handling compared to using `anyhow::Error` everywhere.

use std::path::PathBuf;
use thiserror::Error;

/// Error data for `PathNotUnderDestination`
///
/// Separated to allow boxing and reduce `CommandError` enum size
#[derive(Debug)]
pub struct PathNotUnderDestinationError {
    /// The path that is not under the destination directory
    pub path: PathBuf,
    /// The destination directory path
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
        /// The invalid path
        path: String,
        /// The underlying I/O error
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
    ApplyFailed {
        /// Number of entries that failed
        failed: usize,
        /// Total number of entries
        total: usize,
    },

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

// Note: guisu_engine now re-exports guisu_core::Error, so we only need one From impl
// Note: guisu_config, guisu_template, and guisu_crypto may not have
// their own error types, so errors from those crates will be wrapped
// in anyhow::Error and converted via the Other variant

/// Result type alias for command operations
pub type Result<T> = std::result::Result<T, CommandError>;

impl CommandError {
    /// Create a `PathNotUnderDestination` error
    #[must_use]
    pub fn path_not_under_dest(path: PathBuf, dest_dir: PathBuf) -> Self {
        Self::PathNotUnderDestination(Box::new(PathNotUnderDestinationError { path, dest_dir }))
    }

    /// Create an `IdentityLoadError` from any error type
    pub fn identity_load<E: std::error::Error + Send + Sync + 'static>(err: E) -> Self {
        Self::IdentityLoadError(Box::new(err))
    }

    /// Create a `ConfigError` from any error type
    pub fn config<E: std::error::Error + Send + Sync + 'static>(err: E) -> Self {
        Self::ConfigError(Box::new(err))
    }

    /// Create a `TemplateError` from any error type
    pub fn template<E: std::error::Error + Send + Sync + 'static>(err: E) -> Self {
        Self::TemplateError(Box::new(err))
    }

    /// Create an `EncryptionError` from any error type
    pub fn encryption<E: std::error::Error + Send + Sync + 'static>(err: E) -> Self {
        Self::EncryptionError(Box::new(err))
    }

    /// Create a `DatabaseError` from any error type
    pub fn database<E: std::error::Error + Send + Sync + 'static>(err: E) -> Self {
        Self::DatabaseError(Box::new(err))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use super::*;
    use std::io;

    #[test]
    fn test_path_not_under_dest_error_creation() {
        let path = PathBuf::from("/home/user/file.txt");
        let dest_dir = PathBuf::from("/var/destination");

        let error = CommandError::path_not_under_dest(path.clone(), dest_dir.clone());

        let error_msg = error.to_string();
        assert!(error_msg.contains("/home/user/file.txt"));
        assert!(error_msg.contains("/var/destination"));
        assert!(error_msg.contains("not under destination directory"));
    }

    #[test]
    fn test_identity_load_error() {
        let io_error = io::Error::new(io::ErrorKind::NotFound, "identity file not found");
        let error = CommandError::identity_load(io_error);

        let error_msg = error.to_string();
        assert!(error_msg.contains("Failed to load age identities"));
    }

    #[test]
    fn test_config_error() {
        let io_error = io::Error::new(io::ErrorKind::InvalidData, "invalid config");
        let error = CommandError::config(io_error);

        let error_msg = error.to_string();
        assert!(error_msg.contains("Configuration error"));
    }

    #[test]
    fn test_template_error() {
        let io_error = io::Error::other("template syntax error");
        let error = CommandError::template(io_error);

        let error_msg = error.to_string();
        assert!(error_msg.contains("Template error"));
    }

    #[test]
    fn test_encryption_error() {
        let io_error = io::Error::new(io::ErrorKind::InvalidData, "decryption failed");
        let error = CommandError::encryption(io_error);

        let error_msg = error.to_string();
        assert!(error_msg.contains("Encryption error"));
    }

    #[test]
    fn test_database_error() {
        let io_error = io::Error::new(io::ErrorKind::PermissionDenied, "database locked");
        let error = CommandError::database(io_error);

        let error_msg = error.to_string();
        assert!(error_msg.contains("Database error"));
    }

    #[test]
    fn test_apply_failed_error() {
        let error = CommandError::ApplyFailed {
            failed: 3,
            total: 10,
        };

        let error_msg = error.to_string();
        assert!(error_msg.contains("Apply failed"));
        assert!(error_msg.contains('3'));
        assert!(error_msg.contains("10"));
    }

    #[test]
    fn test_file_not_found_error() {
        let path = PathBuf::from("/missing/file.txt");
        let error = CommandError::FileNotFound(path);

        let error_msg = error.to_string();
        assert!(error_msg.contains("File not found"));
        assert!(error_msg.contains("missing/file.txt"));
    }

    #[test]
    fn test_file_already_exists_error() {
        let path = PathBuf::from("/existing/file.txt");
        let error = CommandError::FileAlreadyExists(path);

        let error_msg = error.to_string();
        assert!(error_msg.contains("File already exists"));
        assert!(error_msg.contains("existing/file.txt"));
    }

    #[test]
    fn test_io_error_conversion() {
        let io_error = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let error: CommandError = io_error.into();

        let error_msg = error.to_string();
        assert!(error_msg.contains("IO error"));
    }

    #[test]
    fn test_invalid_path_error() {
        let io_error = io::Error::new(io::ErrorKind::InvalidInput, "invalid path");
        let error = CommandError::InvalidPath {
            path: "/bad/path".to_string(),
            source: io_error,
        };

        let error_msg = error.to_string();
        assert!(error_msg.contains("Invalid path"));
        assert!(error_msg.contains("/bad/path"));
    }

    #[test]
    fn test_anyhow_error_conversion() {
        let anyhow_err = anyhow::anyhow!("something went wrong");
        let error: CommandError = anyhow_err.into();

        let error_msg = error.to_string();
        assert!(error_msg.contains("something went wrong"));
    }

    #[test]
    fn test_core_error_conversion() {
        use guisu_core::Error as CoreError;

        // Create a core error (PathNotAbsolute)
        let core_error = CoreError::PathNotAbsolute {
            path: PathBuf::from("relative/path"),
        };
        let error: CommandError = core_error.into();

        // Should be converted through Other variant
        assert!(matches!(error, CommandError::Other(_)));
    }

    #[test]
    fn test_git_error_conversion() {
        // Create a git2 error
        let git_error = git2::Error::from_str("repository not found");
        let error: CommandError = git_error.into();

        let error_msg = error.to_string();
        assert!(error_msg.contains("Git error"));
    }
}
