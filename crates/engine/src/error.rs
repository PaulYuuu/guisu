//! Error types for guisu-core
//!
//! This module defines all error types used throughout the core library.
//! We use `thiserror` for structured error handling with good error messages.

use guisu_core::path::{AbsPath, SourceRelPath};
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;

/// Result type alias for guisu-core operations
pub type Result<T> = std::result::Result<T, Error>;

/// Main error type for guisu-core
#[derive(Error, Debug)]
pub enum Error {
    /// Error reading a file
    #[error("Failed to read file {path}: {source}")]
    FileRead {
        path: AbsPath,
        #[source]
        source: std::io::Error,
    },

    /// Error writing a file
    #[error("Failed to write file {path}: {source}")]
    FileWrite {
        path: AbsPath,
        #[source]
        source: std::io::Error,
    },

    /// Error creating a directory
    #[error("Failed to create directory {path}: {source}")]
    DirectoryCreate {
        path: AbsPath,
        #[source]
        source: std::io::Error,
    },

    /// Error reading a directory
    #[error("Failed to read directory {path}: {source}")]
    DirectoryRead {
        path: AbsPath,
        #[source]
        source: std::io::Error,
    },

    /// Error with file metadata
    #[error("Failed to read metadata for {path}: {source}")]
    Metadata {
        path: AbsPath,
        #[source]
        source: std::io::Error,
    },

    /// Path is not absolute
    #[error("Path must be absolute: {path}")]
    PathNotAbsolute { path: PathBuf },

    /// Path is not relative
    #[error("Path must be relative: {path}")]
    PathNotRelative { path: PathBuf },

    /// Invalid path prefix
    #[error("Path {} is not under base directory {}", path.display(), base.display())]
    InvalidPathPrefix {
        path: Arc<PathBuf>,
        base: Arc<PathBuf>,
    },

    /// Invalid attributes in filename
    #[error("Invalid attributes in filename '{filename}': {reason}")]
    InvalidAttributes { filename: String, reason: String },

    /// Duplicate attribute
    #[error("Duplicate attribute '{attribute}' in filename '{filename}'")]
    DuplicateAttribute { filename: String, attribute: String },

    /// Invalid attribute order
    #[error(
        "Invalid attribute order in '{filename}'.\n\
         Attributes must be in this order:\n\
         1. private_ or readonly_\n\
         2. executable_\n\
         3. dot_\n\
         \n\
         Got: {found}\n\
         Suggestion: {suggestion}"
    )]
    InvalidAttributeOrder {
        filename: String,
        found: String,
        suggestion: String,
    },

    /// Source entry not found
    #[error("Source entry not found: {path}")]
    EntryNotFound { path: SourceRelPath },

    /// Invalid configuration
    #[error("Invalid configuration: {message}")]
    InvalidConfig { message: String },

    /// Template rendering error
    #[error("Template rendering failed for {path}: {message}")]
    TemplateRender { path: String, message: String },

    /// Decryption error
    #[error("Decryption failed for {path}: {message}")]
    Decryption { path: String, message: String },

    /// Inline decryption error (for template content)
    #[error("Inline decryption failed: {message}")]
    InlineDecryption { message: String },

    /// Invalid UTF-8 encountered during processing
    #[error("Invalid UTF-8 in {path}: {source}")]
    InvalidUtf8 {
        path: String,
        #[source]
        source: std::string::FromUtf8Error,
    },

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Hook configuration error
    #[error("Hook configuration error: {0}")]
    HookConfig(String),

    /// Hook execution error
    #[error("Hook execution failed: {0}")]
    HookExecution(String),

    /// Variables loading error
    #[error("Variables error: {0}")]
    Variables(String),

    /// State persistence error
    #[error("State persistence error: {0}")]
    State(String),

    /// Other error with context
    #[error("{context}: {source}")]
    Other {
        context: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

// Convert from guisu_core::Error
impl From<guisu_core::Error> for Error {
    fn from(err: guisu_core::Error) -> Self {
        match err {
            guisu_core::Error::PathNotAbsolute { path } => Error::PathNotAbsolute { path },
            guisu_core::Error::PathNotRelative { path } => Error::PathNotRelative { path },
            guisu_core::Error::InvalidPathPrefix { path, base } => {
                Error::InvalidPathPrefix { path, base }
            }
            guisu_core::Error::Io(e) => Error::Io(e),
            _ => Error::Other {
                context: "Shared error".to_string(),
                source: Box::new(err),
            },
        }
    }
}

impl Error {
    /// Create an error with additional context
    pub fn context(self, context: impl Into<String>) -> Self {
        Error::Other {
            context: context.into(),
            source: Box::new(self),
        }
    }
}
