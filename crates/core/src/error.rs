//! Base error types for guisu
//!
//! This module provides the foundation error types that all crates can use.

use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;

/// Base error type for shared functionality
#[derive(Error, Debug)]
pub enum Error {
    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

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

    /// Hook configuration error
    #[error("Hook configuration error: {0}")]
    HookConfig(String),

    /// Hook execution error
    #[error("Hook execution error: {0}")]
    HookExecution(String),

    /// State persistence error
    #[error("State error: {0}")]
    State(String),

    /// Generic error message
    #[error("{0}")]
    Message(String),
}

/// Result type alias
pub type Result<T> = std::result::Result<T, Error>;
