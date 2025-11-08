//! XDG directory utilities
//!
//! This module provides XDG-compliant directory paths for guisu.
//! It follows the XDG Base Directory specification using the `xdg` crate:
//! - XDG_DATA_HOME defaults to ~/.local/share
//! - XDG_CONFIG_HOME defaults to ~/.config
//! - XDG_STATE_HOME defaults to ~/.local/state

use std::path::PathBuf;
use xdg::BaseDirectories;

/// Get the guisu data directory
///
/// Returns $XDG_DATA_HOME/guisu or ~/.local/share/guisu
pub fn data_dir() -> Option<PathBuf> {
    // xdg 3.0: with_prefix returns BaseDirectories, get_*_home returns Option<PathBuf>
    BaseDirectories::with_prefix("guisu").get_data_home()
}

/// Get the guisu state directory
///
/// Returns $XDG_STATE_HOME/guisu or ~/.local/state/guisu
pub fn state_dir() -> Option<PathBuf> {
    // xdg 3.0: with_prefix returns BaseDirectories, get_*_home returns Option<PathBuf>
    BaseDirectories::with_prefix("guisu").get_state_home()
}

/// Get the default source directory for dotfiles
///
/// Returns $XDG_DATA_HOME/guisu or ~/.local/share/guisu
pub fn default_source_dir() -> Option<PathBuf> {
    data_dir()
}

/// Get the default config file path
///
/// Returns $XDG_DATA_HOME/guisu/config.toml or ~/.local/share/guisu/config.toml
pub fn default_config_file() -> Option<PathBuf> {
    data_dir().map(|d| d.join("config.toml"))
}

/// Get the default age identity file path
///
/// Returns $XDG_DATA_HOME/guisu/key.txt or ~/.local/share/guisu/key.txt
pub fn default_age_identity() -> Option<PathBuf> {
    data_dir().map(|d| d.join("key.txt"))
}
