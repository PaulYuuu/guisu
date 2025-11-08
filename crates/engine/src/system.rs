//! System abstraction for filesystem operations
//!
//! This module provides a trait-based abstraction over filesystem operations,
//! enabling testing and dry-run mode.

use crate::error::{Error, Result};
use guisu_core::path::AbsPath;
use std::fs::{self, Metadata};
use std::path::Path;

/// Abstraction over filesystem operations
///
/// This trait allows us to implement different backends:
/// - `RealSystem`: Actual filesystem operations
/// - `DryRunSystem`: Records operations without executing them
/// - Mock implementations for testing
pub trait System {
    /// Read a file's contents
    fn read_file(&self, path: &AbsPath) -> Result<Vec<u8>>;

    /// Write a file's contents with optional permissions
    fn write_file(&self, path: &AbsPath, content: &[u8], mode: Option<u32>) -> Result<()>;

    /// Create a directory with optional permissions
    fn create_dir(&self, path: &AbsPath, mode: Option<u32>) -> Result<()>;

    /// Create all parent directories
    fn create_dir_all(&self, path: &AbsPath, mode: Option<u32>) -> Result<()>;

    /// Remove a file or directory
    fn remove(&self, path: &AbsPath) -> Result<()>;

    /// Remove a directory and all its contents
    fn remove_all(&self, path: &AbsPath) -> Result<()>;

    /// Check if a path exists
    fn exists(&self, path: &AbsPath) -> bool;

    /// Get file metadata
    fn metadata(&self, path: &AbsPath) -> Result<Metadata>;

    /// Create a symbolic link
    fn symlink(&self, target: &Path, link: &AbsPath) -> Result<()>;

    /// Read a symbolic link
    fn read_link(&self, path: &AbsPath) -> Result<std::path::PathBuf>;
}

/// Real filesystem implementation
///
/// This implementation performs actual filesystem operations.
pub struct RealSystem;

impl System for RealSystem {
    fn read_file(&self, path: &AbsPath) -> Result<Vec<u8>> {
        fs::read(path.as_path()).map_err(|e| Error::FileRead {
            path: path.clone(),
            source: e,
        })
    }

    fn write_file(&self, path: &AbsPath, content: &[u8], mode: Option<u32>) -> Result<()> {
        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            self.create_dir_all(&parent, None)?;
        }

        // Write the file
        fs::write(path.as_path(), content).map_err(|e| Error::FileWrite {
            path: path.clone(),
            source: e,
        })?;

        // Set permissions if specified
        #[cfg(unix)]
        if let Some(mode) = mode {
            use std::os::unix::fs::PermissionsExt;
            let permissions = fs::Permissions::from_mode(mode);
            fs::set_permissions(path.as_path(), permissions).map_err(|e| Error::FileWrite {
                path: path.clone(),
                source: e,
            })?;
        }

        Ok(())
    }

    fn create_dir(&self, path: &AbsPath, mode: Option<u32>) -> Result<()> {
        fs::create_dir(path.as_path()).map_err(|e| Error::DirectoryCreate {
            path: path.clone(),
            source: e,
        })?;

        // Set permissions if specified
        #[cfg(unix)]
        if let Some(mode) = mode {
            use std::os::unix::fs::PermissionsExt;
            let permissions = fs::Permissions::from_mode(mode);
            fs::set_permissions(path.as_path(), permissions).map_err(|e| {
                Error::DirectoryCreate {
                    path: path.clone(),
                    source: e,
                }
            })?;
        }

        Ok(())
    }

    fn create_dir_all(&self, path: &AbsPath, mode: Option<u32>) -> Result<()> {
        fs::create_dir_all(path.as_path()).map_err(|e| Error::DirectoryCreate {
            path: path.clone(),
            source: e,
        })?;

        // Set permissions if specified
        #[cfg(unix)]
        if let Some(mode) = mode {
            use std::os::unix::fs::PermissionsExt;
            let permissions = fs::Permissions::from_mode(mode);
            fs::set_permissions(path.as_path(), permissions).map_err(|e| {
                Error::DirectoryCreate {
                    path: path.clone(),
                    source: e,
                }
            })?;
        }

        Ok(())
    }

    fn remove(&self, path: &AbsPath) -> Result<()> {
        let metadata = self.metadata(path)?;
        if metadata.is_dir() {
            fs::remove_dir(path.as_path()).map_err(Error::Io)
        } else {
            fs::remove_file(path.as_path()).map_err(Error::Io)
        }
    }

    fn remove_all(&self, path: &AbsPath) -> Result<()> {
        fs::remove_dir_all(path.as_path()).map_err(Error::Io)
    }

    fn exists(&self, path: &AbsPath) -> bool {
        path.as_path().exists()
    }

    fn metadata(&self, path: &AbsPath) -> Result<Metadata> {
        fs::metadata(path.as_path()).map_err(|e| Error::Metadata {
            path: path.clone(),
            source: e,
        })
    }

    fn symlink(&self, target: &Path, link: &AbsPath) -> Result<()> {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link.as_path()).map_err(Error::Io)
        }

        #[cfg(windows)]
        {
            // On Windows, we need to check if target is a dir or file
            if target.is_dir() {
                std::os::windows::fs::symlink_dir(target, link.as_path()).map_err(|e| Error::Io(e))
            } else {
                std::os::windows::fs::symlink_file(target, link.as_path()).map_err(|e| Error::Io(e))
            }
        }
    }

    fn read_link(&self, path: &AbsPath) -> Result<std::path::PathBuf> {
        fs::read_link(path.as_path()).map_err(Error::Io)
    }
}

/// Dry-run system that records operations without executing them
///
/// This is useful for showing what would be done without actually modifying the filesystem.
#[derive(Debug, Default)]
pub struct DryRunSystem {
    operations: std::cell::RefCell<Vec<Operation>>,
}

/// An operation that would be performed on the filesystem
#[derive(Debug, Clone, PartialEq)]
pub enum Operation {
    /// Read a file
    ReadFile { path: AbsPath },
    /// Write a file
    WriteFile {
        path: AbsPath,
        size: usize,
        mode: Option<u32>,
    },
    /// Create a directory
    CreateDir { path: AbsPath, mode: Option<u32> },
    /// Remove a path
    Remove { path: AbsPath },
    /// Create a symlink
    Symlink {
        link: AbsPath,
        target: std::path::PathBuf,
    },
}

impl DryRunSystem {
    /// Create a new dry-run system
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the list of operations that would be performed
    pub fn operations(&self) -> Vec<Operation> {
        self.operations.borrow().clone()
    }

    /// Record an operation
    fn record(&self, op: Operation) {
        self.operations.borrow_mut().push(op);
    }
}

impl System for DryRunSystem {
    fn read_file(&self, path: &AbsPath) -> Result<Vec<u8>> {
        self.record(Operation::ReadFile { path: path.clone() });
        // In dry-run mode, we can't actually read files that don't exist yet
        // Return empty content
        Ok(Vec::new())
    }

    fn write_file(&self, path: &AbsPath, content: &[u8], mode: Option<u32>) -> Result<()> {
        self.record(Operation::WriteFile {
            path: path.clone(),
            size: content.len(),
            mode,
        });
        Ok(())
    }

    fn create_dir(&self, path: &AbsPath, mode: Option<u32>) -> Result<()> {
        self.record(Operation::CreateDir {
            path: path.clone(),
            mode,
        });
        Ok(())
    }

    fn create_dir_all(&self, path: &AbsPath, mode: Option<u32>) -> Result<()> {
        self.record(Operation::CreateDir {
            path: path.clone(),
            mode,
        });
        Ok(())
    }

    fn remove(&self, path: &AbsPath) -> Result<()> {
        self.record(Operation::Remove { path: path.clone() });
        Ok(())
    }

    fn remove_all(&self, path: &AbsPath) -> Result<()> {
        self.record(Operation::Remove { path: path.clone() });
        Ok(())
    }

    fn exists(&self, _path: &AbsPath) -> bool {
        // In dry-run mode, assume paths don't exist
        false
    }

    fn metadata(&self, path: &AbsPath) -> Result<Metadata> {
        // Can't get metadata in dry-run mode
        Err(Error::Metadata {
            path: path.clone(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "dry-run mode"),
        })
    }

    fn symlink(&self, target: &Path, link: &AbsPath) -> Result<()> {
        self.record(Operation::Symlink {
            link: link.clone(),
            target: target.to_path_buf(),
        });
        Ok(())
    }

    fn read_link(&self, _path: &AbsPath) -> Result<std::path::PathBuf> {
        // Can't read links in dry-run mode
        Ok(std::path::PathBuf::new())
    }
}
