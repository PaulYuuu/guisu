//! Vault providers for password managers
//!
//! This crate provides a unified interface for accessing secrets from various
//! password manager vaults like Bitwarden, 1Password, LastPass, etc.

use indexmap::IndexMap;
use serde_json::Value as JsonValue;
use thiserror::Error;

/// Result type for vault operations
pub type Result<T> = std::result::Result<T, Error>;

/// Error types for secret providers
#[derive(Error, Debug)]
pub enum Error {
    #[error("Provider not available: {0}")]
    ProviderNotAvailable(String),

    #[error("Authentication required: {0}")]
    AuthenticationRequired(String),

    #[error("Secret not found: {0}")]
    SecretNotFound(String),

    #[error("Invalid arguments: {0}")]
    InvalidArguments(String),

    #[error("Command execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Failed to parse response: {0}")]
    ParseError(String),

    #[error("User cancelled operation")]
    Cancelled,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("{0}")]
    Other(String),
}

// Bitwarden Vault (personal/team passwords)
// Provides BwCli and RbwCli
#[cfg(feature = "bw")]
pub mod bw;

// Bitwarden Secrets Manager (organization secrets)
// Provides BwsCli
#[cfg(feature = "bws")]
pub mod bws;

// Future providers
// #[cfg(feature = "onepassword")]
// pub mod onepassword;

/// Trait for secret providers
///
/// All password manager integrations should implement this trait.
pub trait SecretProvider: Send + Sync {
    /// Get the name of this provider
    fn name(&self) -> &str;

    /// Execute a command and return JSON result
    ///
    /// # Arguments
    ///
    /// * `args` - Command arguments (e.g., ["get", "item", "GitHub"])
    fn execute(&self, args: &[&str]) -> Result<JsonValue>;

    /// Check if the provider is available (CLI installed, etc.)
    fn is_available(&self) -> bool;

    /// Get help text for this provider
    fn help(&self) -> &str;
}

/// Secret manager that caches results
pub struct CachedSecretProvider<P: SecretProvider> {
    provider: P,
    cache: IndexMap<String, JsonValue>,
}

impl<P: SecretProvider> CachedSecretProvider<P> {
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            cache: IndexMap::new(),
        }
    }

    pub fn execute_cached(&mut self, args: &[&str]) -> Result<JsonValue> {
        let cache_key = args.join("|");

        if let Some(cached) = self.cache.get(&cache_key) {
            return Ok(cached.clone());
        }

        let result = self.provider.execute(args)?;
        self.cache.insert(cache_key, result.clone());

        Ok(result)
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }
}
