//! Error types for guisu
//!
//! This module provides unified error types for all guisu crates.
//! All crates (engine, config, crypto, template, etc.) use this single error type.

use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;

/// Unified error type for all guisu operations
#[derive(Error, Debug)]
pub enum Error {
    // ========== I/O Errors ==========
    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Error reading a file
    #[error("Failed to read file {path}: {source}")]
    FileRead {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Error writing a file
    #[error("Failed to write file {path}: {source}")]
    FileWrite {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Error creating a directory
    #[error("Failed to create directory {path}: {source}")]
    DirectoryCreate {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Error reading a directory
    #[error("Failed to read directory {path}: {source}")]
    DirectoryRead {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Error with file metadata
    #[error("Failed to read metadata for {path}: {source}")]
    Metadata {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    // ========== Path Errors ==========
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

    /// Generic path error
    #[error("Path error: {0}")]
    Path(String),

    // ========== Attribute Parsing Errors ==========
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

    // ========== Entry Errors ==========
    /// Source entry not found
    #[error("Source entry not found: {0}")]
    EntryNotFound(String),

    // ========== Configuration Errors ==========
    /// Invalid configuration
    #[error("Invalid configuration: {message}")]
    InvalidConfig { message: String },

    // ========== Template and Encryption Errors ==========
    /// Template rendering error
    #[error("Template rendering failed for {path}: {source}")]
    TemplateRender {
        path: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Decryption error
    #[error("Decryption failed for {path}: {source}")]
    Decryption {
        path: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

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

    // ========== Hook Errors ==========
    /// Hook configuration error
    #[error("Hook configuration error: {0}")]
    HookConfig(String),

    /// Hook execution error
    #[error("Hook execution failed: {0}")]
    HookExecution(String),

    // ========== Variables Error ==========
    /// Variables loading error
    #[error("Variables error: {0}")]
    Variables(String),

    // ========== State Persistence Errors ==========
    /// State persistence error
    #[error("State error: {0}")]
    State(String),

    // ========== Generic Errors ==========
    /// Generic error message
    #[error("{0}")]
    Message(String),

    /// Other error with context
    #[error("{context}: {source}")]
    Other {
        context: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
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

/// Result type alias
pub type Result<T> = std::result::Result<T, Error>;
