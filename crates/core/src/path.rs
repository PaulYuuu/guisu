//! Type-safe path types
//!
//! This module provides three distinct path types using the newtype pattern:
//!
//! - [`AbsPath`]: Absolute filesystem paths
//! - [`RelPath`]: Relative paths (no leading slash)
//! - [`SourceRelPath`]: Relative paths in the source directory with encoded attributes
//!
//! These types prevent common path manipulation errors at compile time.
//!
//! # Examples
//!
//! ```
//! use guisu_core::path::{AbsPath, RelPath};
//! use std::path::PathBuf;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Create an absolute path
//! let home = AbsPath::new("/home/user".into())?;
//!
//! // Create a relative path
//! let config = RelPath::new(".config/nvim/init.lua".into())?;
//!
//! // Join them to get a new absolute path
//! let nvim_config = home.join(&config);
//! assert_eq!(nvim_config.as_path().to_str().unwrap(), "/home/user/.config/nvim/init.lua");
//! # Ok(())
//! # }
//! ```

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// An absolute path on the filesystem
///
/// This type guarantees that the path is absolute (starts with `/` on Unix or a drive letter on Windows).
/// Use this for file operations and as base directories.
///
/// # Examples
///
/// ```
/// use guisu_core::path::AbsPath;
/// use std::path::PathBuf;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let abs = AbsPath::new("/home/user".into())?;
/// assert_eq!(abs.as_path(), std::path::Path::new("/home/user"));
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AbsPath(PathBuf);

impl AbsPath {
    /// Create a new `AbsPath` from a `PathBuf`
    ///
    /// # Errors
    ///
    /// Returns an error if the path is not absolute.
    ///
    /// # Examples
    ///
    /// ```
    /// use guisu_core::path::AbsPath;
    /// use std::path::PathBuf;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let abs = AbsPath::new("/home/user".into())?;
    /// assert!(abs.as_path().is_absolute());
    ///
    /// let err = AbsPath::new("relative/path".into());
    /// assert!(err.is_err());
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(path: PathBuf) -> Result<Self> {
        if path.is_absolute() {
            Ok(AbsPath(path))
        } else {
            Err(Error::PathNotAbsolute { path })
        }
    }

    /// Create a new `AbsPath` from a reference to a `Path`
    ///
    /// This will clone the path internally.
    ///
    /// # Errors
    ///
    /// Returns an error if the path is not absolute.
    pub fn from_path(path: &Path) -> Result<Self> {
        Self::new(path.to_path_buf())
    }

    /// Get the underlying `Path`
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// Convert to a `PathBuf`
    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }

    /// Join with a relative path to create a new absolute path
    ///
    /// # Examples
    ///
    /// ```
    /// use guisu_core::path::{AbsPath, RelPath};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let home = AbsPath::new("/home/user".into())?;
    /// let config = RelPath::new(".config".into())?;
    /// let path = home.join(&config);
    /// assert_eq!(path.as_path().to_str().unwrap(), "/home/user/.config");
    /// # Ok(())
    /// # }
    /// ```
    pub fn join(&self, rel: &RelPath) -> Self {
        AbsPath(self.0.join(rel.as_path()))
    }

    /// Get the parent directory
    ///
    /// Returns `None` if this is the root directory.
    pub fn parent(&self) -> Option<Self> {
        self.0.parent().map(|p| AbsPath(p.to_path_buf()))
    }

    /// Strip a base directory prefix to get a relative path
    ///
    /// # Errors
    ///
    /// Returns an error if `self` is not under `base`.
    ///
    /// # Examples
    ///
    /// ```
    /// use guisu_core::path::{AbsPath, RelPath};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let home = AbsPath::new("/home/user".into())?;
    /// let file = AbsPath::new("/home/user/.bashrc".into())?;
    /// let rel = file.strip_prefix(&home)?;
    /// assert_eq!(rel.as_path().to_str().unwrap(), ".bashrc");
    /// # Ok(())
    /// # }
    /// ```
    pub fn strip_prefix(&self, base: &AbsPath) -> Result<RelPath> {
        self.0
            .strip_prefix(&base.0)
            .map(|p| RelPath(p.to_path_buf()))
            .map_err(|_| Error::InvalidPathPrefix {
                path: std::sync::Arc::new(self.as_path().to_path_buf()),
                base: std::sync::Arc::new(base.as_path().to_path_buf()),
            })
    }

    /// Get the file name
    pub fn file_name(&self) -> Option<&str> {
        self.0.file_name().and_then(|s| s.to_str())
    }
}

/// A relative path (no leading slash)
///
/// This type guarantees that the path is relative (does not start with `/`).
/// Use this for paths relative to a base directory.
///
/// # Examples
///
/// ```
/// use guisu_core::path::RelPath;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let rel = RelPath::new(".config/nvim/init.lua".into())?;
/// assert_eq!(rel.as_path().to_str().unwrap(), ".config/nvim/init.lua");
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RelPath(PathBuf);

impl RelPath {
    /// Create a new `RelPath` from a `PathBuf`
    ///
    /// # Errors
    ///
    /// Returns an error if the path is absolute.
    pub fn new(path: PathBuf) -> Result<Self> {
        if path.is_relative() {
            Ok(RelPath(path))
        } else {
            Err(Error::PathNotRelative { path })
        }
    }

    /// Get the underlying `Path`
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// Convert to a `PathBuf`
    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }

    /// Join with another relative path
    ///
    /// # Examples
    ///
    /// ```
    /// use guisu_core::path::RelPath;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = RelPath::new(".config".into())?;
    /// let nvim = RelPath::new("nvim".into())?;
    /// let path = config.join(&nvim);
    /// assert_eq!(path.as_path().to_str().unwrap(), ".config/nvim");
    /// # Ok(())
    /// # }
    /// ```
    pub fn join(&self, other: &RelPath) -> Self {
        RelPath(self.0.join(&other.0))
    }

    /// Get the parent directory
    ///
    /// Returns `None` if this is a single component path.
    pub fn parent(&self) -> Option<Self> {
        self.0.parent().map(|p| RelPath(p.to_path_buf()))
    }

    /// Get the file name
    pub fn file_name(&self) -> Option<&str> {
        self.0.file_name().and_then(|s| s.to_str())
    }

    /// Convert to a `SourceRelPath` (assumes no attribute encoding)
    pub fn to_source(&self) -> SourceRelPath {
        SourceRelPath(self.0.clone())
    }
}

/// A relative path in the source directory with encoded attributes
///
/// This type represents paths in the source directory where attributes are encoded
/// in the filename (e.g., `dot_bashrc`, `private_dot_ssh/`).
///
/// # Examples
///
/// ```
/// use guisu_core::path::SourceRelPath;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let source = SourceRelPath::new("dot_config/nvim/init.lua".into())?;
/// assert_eq!(source.as_path().to_str().unwrap(), "dot_config/nvim/init.lua");
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceRelPath(PathBuf);

impl SourceRelPath {
    /// Create a new `SourceRelPath` from a `PathBuf`
    ///
    /// # Errors
    ///
    /// Returns an error if the path is absolute.
    pub fn new(path: PathBuf) -> Result<Self> {
        if path.is_relative() {
            Ok(SourceRelPath(path))
        } else {
            Err(Error::PathNotRelative { path })
        }
    }

    /// Get the underlying `Path`
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// Convert to a `PathBuf`
    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }

    /// Join with another source relative path
    pub fn join(&self, other: &SourceRelPath) -> Self {
        SourceRelPath(self.0.join(&other.0))
    }

    /// Get the parent directory
    pub fn parent(&self) -> Option<Self> {
        self.0.parent().map(|p| SourceRelPath(p.to_path_buf()))
    }

    /// Get the file name
    pub fn file_name(&self) -> Option<&str> {
        self.0.file_name().and_then(|s| s.to_str())
    }

    /// Convert to a regular `RelPath` (preserves the encoded attributes)
    pub fn to_rel_path(&self) -> RelPath {
        RelPath(self.0.clone())
    }
}

// Implement Display for all path types
impl std::fmt::Display for AbsPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

impl std::fmt::Display for RelPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

impl std::fmt::Display for SourceRelPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.display())
    }
}
