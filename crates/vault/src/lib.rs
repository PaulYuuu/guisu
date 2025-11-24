//! Vault providers for password managers
//!
//! This crate provides a unified interface for accessing secrets from various
//! password manager vaults like `Bitwarden`, `1Password`, `LastPass`, etc.

use indexmap::IndexMap;
use serde_json::Value as JsonValue;
use thiserror::Error;

/// Result type for vault operations
pub type Result<T> = std::result::Result<T, Error>;

/// Error types for secret providers
#[derive(Error, Debug)]
pub enum Error {
    /// Secret provider is not available or not installed
    #[error("Provider not available: {0}")]
    ProviderNotAvailable(String),

    /// Authentication is required to access the vault
    #[error("Authentication required: {0}")]
    AuthenticationRequired(String),

    /// The requested secret was not found in the vault
    #[error("Secret not found: {0}")]
    SecretNotFound(String),

    /// Invalid arguments provided to the provider
    #[error("Invalid arguments: {0}")]
    InvalidArguments(String),

    /// Command execution failed
    #[error("Command execution failed: {0}")]
    ExecutionFailed(String),

    /// Failed to parse provider response
    #[error("Failed to parse response: {0}")]
    ParseError(String),

    /// User cancelled the operation
    #[error("User cancelled operation")]
    Cancelled,

    /// IO error occurred
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON parsing error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Other error
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
    fn name(&self) -> &'static str;

    /// Execute a command and return JSON result
    ///
    /// # Arguments
    ///
    /// * `args` - Command arguments (e.g., \["get", "item", "GitHub"\])
    ///
    /// # Errors
    ///
    /// Returns error if command execution fails or JSON parsing fails
    fn execute(&self, args: &[&str]) -> Result<JsonValue>;

    /// Check if the provider is available (CLI installed, etc.)
    fn is_available(&self) -> bool;

    /// Get help text for this provider
    fn help(&self) -> &'static str;
}

/// Secret manager that caches results
pub struct CachedSecretProvider<P: SecretProvider> {
    provider: P,
    cache: IndexMap<String, JsonValue>,
}

impl<P: SecretProvider> CachedSecretProvider<P> {
    /// Create a new cached secret provider
    #[must_use]
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            cache: IndexMap::new(),
        }
    }

    /// Execute command with caching
    ///
    /// # Errors
    ///
    /// Returns error if command execution fails or JSON parsing fails
    pub fn execute_cached(&mut self, args: &[&str]) -> Result<JsonValue> {
        let cache_key = args.join("|");

        if let Some(cached) = self.cache.get(&cache_key) {
            return Ok(cached.clone());
        }

        let result = self.provider.execute(args)?;
        self.cache.insert(cache_key, result.clone());

        Ok(result)
    }

    /// Clear all cached secrets
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Mock provider for testing
    struct MockProvider {
        name: String,
        available: bool,
        response: JsonValue,
        call_count: AtomicUsize,
    }

    impl MockProvider {
        fn new(name: &str, response: JsonValue) -> Self {
            Self {
                name: name.to_string(),
                available: true,
                response,
                call_count: AtomicUsize::new(0),
            }
        }

        fn unavailable(name: &str) -> Self {
            Self {
                name: name.to_string(),
                available: false,
                response: JsonValue::Null,
                call_count: AtomicUsize::new(0),
            }
        }

        fn get_call_count(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    impl SecretProvider for MockProvider {
        fn name(&self) -> &'static str {
            // For tests, we can't return a reference to self.name since it's not static
            // But in real implementations, provider names are always static strings
            "mock"
        }

        fn execute(&self, _args: &[&str]) -> Result<JsonValue> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(self.response.clone())
        }

        fn is_available(&self) -> bool {
            self.available
        }

        fn help(&self) -> &'static str {
            "Mock provider help text"
        }
    }

    // Tests for Error enum

    #[test]
    fn test_error_provider_not_available() {
        let err = Error::ProviderNotAvailable("test".to_string());
        assert_eq!(err.to_string(), "Provider not available: test");
    }

    #[test]
    fn test_error_authentication_required() {
        let err = Error::AuthenticationRequired("login needed".to_string());
        assert_eq!(err.to_string(), "Authentication required: login needed");
    }

    #[test]
    fn test_error_secret_not_found() {
        let err = Error::SecretNotFound("my-secret".to_string());
        assert_eq!(err.to_string(), "Secret not found: my-secret");
    }

    #[test]
    fn test_error_invalid_arguments() {
        let err = Error::InvalidArguments("bad args".to_string());
        assert_eq!(err.to_string(), "Invalid arguments: bad args");
    }

    #[test]
    fn test_error_execution_failed() {
        let err = Error::ExecutionFailed("command failed".to_string());
        assert_eq!(err.to_string(), "Command execution failed: command failed");
    }

    #[test]
    fn test_error_parse_error() {
        let err = Error::ParseError("invalid json".to_string());
        assert_eq!(err.to_string(), "Failed to parse response: invalid json");
    }

    #[test]
    fn test_error_cancelled() {
        let err = Error::Cancelled;
        assert_eq!(err.to_string(), "User cancelled operation");
    }

    #[test]
    fn test_error_other() {
        let err = Error::Other("custom error".to_string());
        assert_eq!(err.to_string(), "custom error");
    }

    #[test]
    fn test_error_debug() {
        let err = Error::SecretNotFound("test".to_string());
        let debug = format!("{err:?}");
        assert!(debug.contains("SecretNotFound"));
    }

    // Tests for CachedSecretProvider

    #[test]
    fn test_cached_provider_new() {
        let provider = MockProvider::new("test", JsonValue::Null);
        let cached = CachedSecretProvider::new(provider);
        assert_eq!(cached.cache.len(), 0);
    }

    #[test]
    fn test_cached_provider_execute_caches_result() {
        let response = serde_json::json!({"key": "value"});
        let provider = MockProvider::new("test", response.clone());
        let mut cached = CachedSecretProvider::new(provider);

        // First call - should execute
        let result1 = cached.execute_cached(&["get", "item"]).unwrap();
        assert_eq!(result1, response);
        assert_eq!(cached.provider.get_call_count(), 1);

        // Second call with same args - should use cache
        let result2 = cached.execute_cached(&["get", "item"]).unwrap();
        assert_eq!(result2, response);
        assert_eq!(cached.provider.get_call_count(), 1); // Still 1!
    }

    #[test]
    fn test_cached_provider_different_args_different_cache() {
        let response = serde_json::json!({"key": "value"});
        let provider = MockProvider::new("test", response.clone());
        let mut cached = CachedSecretProvider::new(provider);

        // First call
        cached.execute_cached(&["get", "item1"]).unwrap();
        assert_eq!(cached.provider.get_call_count(), 1);

        // Different args - should execute again
        cached.execute_cached(&["get", "item2"]).unwrap();
        assert_eq!(cached.provider.get_call_count(), 2);

        // Same as first - should use cache
        cached.execute_cached(&["get", "item1"]).unwrap();
        assert_eq!(cached.provider.get_call_count(), 2);
    }

    #[test]
    fn test_cached_provider_clear_cache() {
        let response = serde_json::json!({"key": "value"});
        let provider = MockProvider::new("test", response);
        let mut cached = CachedSecretProvider::new(provider);

        // Execute and cache
        cached.execute_cached(&["get", "item"]).unwrap();
        assert_eq!(cached.cache.len(), 1);
        assert_eq!(cached.provider.get_call_count(), 1);

        // Clear cache
        cached.clear_cache();
        assert_eq!(cached.cache.len(), 0);

        // Execute again - should call provider
        cached.execute_cached(&["get", "item"]).unwrap();
        assert_eq!(cached.provider.get_call_count(), 2);
    }

    #[test]
    fn test_cached_provider_cache_key_includes_all_args() {
        let response = serde_json::json!({"key": "value"});
        let provider = MockProvider::new("test", response);
        let mut cached = CachedSecretProvider::new(provider);

        // These should be different cache entries
        cached.execute_cached(&["get", "item", "name"]).unwrap();
        cached.execute_cached(&["get", "item"]).unwrap();
        cached.execute_cached(&["get"]).unwrap();

        assert_eq!(cached.provider.get_call_count(), 3);
        assert_eq!(cached.cache.len(), 3);
    }

    #[test]
    fn test_cached_provider_empty_args() {
        let response = serde_json::json!({"status": "ok"});
        let provider = MockProvider::new("test", response.clone());
        let mut cached = CachedSecretProvider::new(provider);

        let result = cached.execute_cached(&[]).unwrap();
        assert_eq!(result, response);
        assert_eq!(cached.cache.len(), 1);
    }

    #[test]
    fn test_cached_provider_preserves_json_types() {
        let response = serde_json::json!({
            "string": "value",
            "number": 42,
            "bool": true,
            "null": null,
            "array": [1, 2, 3],
            "object": {"nested": "value"}
        });
        let provider = MockProvider::new("test", response.clone());
        let mut cached = CachedSecretProvider::new(provider);

        let result = cached.execute_cached(&["get"]).unwrap();
        assert_eq!(result, response);
        assert_eq!(result["string"], "value");
        assert_eq!(result["number"], 42);
        assert_eq!(result["bool"], true);
        assert_eq!(result["null"], JsonValue::Null);
    }

    // Tests for MockProvider (to verify test infrastructure)

    #[test]
    fn test_mock_provider_name() {
        let provider = MockProvider::new("test-provider", JsonValue::Null);
        assert_eq!(provider.name(), "mock");
    }

    #[test]
    fn test_mock_provider_is_available() {
        let available = MockProvider::new("test", JsonValue::Null);
        assert!(available.is_available());

        let unavailable = MockProvider::unavailable("test");
        assert!(!unavailable.is_available());
    }

    #[test]
    fn test_mock_provider_help() {
        let provider = MockProvider::new("test", JsonValue::Null);
        assert_eq!(provider.help(), "Mock provider help text");
    }

    #[test]
    fn test_mock_provider_execute_returns_response() {
        let response = serde_json::json!({"test": "data"});
        let provider = MockProvider::new("test", response.clone());

        let result = provider.execute(&["arg1", "arg2"]).unwrap();
        assert_eq!(result, response);
    }

    #[test]
    fn test_mock_provider_tracks_call_count() {
        let provider = MockProvider::new("test", JsonValue::Null);
        assert_eq!(provider.get_call_count(), 0);

        provider.execute(&["arg1"]).unwrap();
        assert_eq!(provider.get_call_count(), 1);

        provider.execute(&["arg2"]).unwrap();
        assert_eq!(provider.get_call_count(), 2);
    }
}
