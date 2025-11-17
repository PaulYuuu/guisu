//! Configuration management for guisu
//!
//! This crate handles:
//! - Configuration loading and validation
//! - XDG directory management
//! - Git integration
//! - Variable loading
//! - Hook configuration
//! - Database helpers
//! - Logging initialization

pub mod config;
pub mod config_info;
pub mod dirs;
pub mod git;
pub mod ignores;
pub mod logging;
pub mod matcher;
pub mod variables;

// Re-export error types from core
pub use guisu_core::{Error, Result};

// Re-export main types
pub use config::{
    AgeConfig, BitwardenConfig, Config, GeneralConfig, IconMode, IgnoreConfig, UiConfig,
};
pub use config_info::{AgeConfigInfo, BitwardenConfigInfo, ConfigInfo, UiConfigInfo};
// NOTE: database module moved to guisu-engine
// CLI should import from engine::database directly
pub use dirs::{data_dir, default_source_dir, state_dir};
pub use ignores::IgnoresConfig;
pub use matcher::IgnoreMatcher;
