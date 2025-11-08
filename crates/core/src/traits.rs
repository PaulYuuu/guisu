//! Core behavioral traits for guisu components
//!
//! This module defines abstract interfaces that decouple high-level modules
//! from concrete implementations, following the Dependency Inversion Principle.
//!
//! By depending on these traits instead of concrete types, we achieve:
//! - **Reduced coupling**: Changes to implementations don't trigger recompilation of dependents
//! - **Better testability**: Easy to mock implementations for testing
//! - **Flexibility**: Can swap implementations at runtime if needed

use crate::Result;
use std::path::PathBuf;

/// Configuration provider interface
///
/// Abstracts configuration access to decouple consumers from specific config formats
/// or storage mechanisms.
///
/// # Examples
///
/// ```ignore
/// fn process_files(config: &dyn ConfigProvider) -> Result<()> {
///     let source = config.source_dir();
///     let dest = config.dest_dir();
///     // ... process files
/// }
/// ```
pub trait ConfigProvider {
    /// Get the source directory path
    fn source_dir(&self) -> Option<&PathBuf>;

    /// Get the destination directory path
    fn dest_dir(&self) -> Option<&PathBuf>;

    /// Get template variables as JSON values
    fn variables(&self) -> &indexmap::IndexMap<String, serde_json::Value>;
}

/// Encryption provider interface
///
/// Abstracts encryption/decryption operations to allow different encryption
/// backends (age, GPG, etc.) without changing consuming code.
///
/// # Examples
///
/// ```ignore
/// fn encrypt_secret(provider: &dyn EncryptionProvider, secret: &str) -> Result<Vec<u8>> {
///     provider.encrypt(secret.as_bytes())
/// }
/// ```
pub trait EncryptionProvider {
    /// Encrypt data
    fn encrypt(&self, data: &[u8]) -> Result<Vec<u8>>;

    /// Decrypt data
    fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>>;
}

/// Template renderer interface
///
/// Abstracts template rendering to decouple from specific template engines
/// (Jinja2, Handlebars, etc.).
///
/// Uses `serde_json::Value` for context to ensure trait object safety.
/// Any struct implementing `serde::Serialize` can be converted to `Value` with `serde_json::to_value()`.
///
/// # Examples
///
/// ```ignore
/// use guisu_core::TemplateRenderer;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct MyContext {
///     name: String,
/// }
///
/// fn render_greeting(renderer: &dyn TemplateRenderer) -> Result<String> {
///     let context = MyContext { name: "Alice".to_string() };
///     let value = serde_json::to_value(&context)?;
///     renderer.render_str("Hello {{ name }}!", &value)
/// }
/// ```
pub trait TemplateRenderer {
    /// Render a template string with the given context
    ///
    /// # Arguments
    ///
    /// * `template` - The template source code
    /// * `context` - Context data as a JSON value
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let context = serde_json::json!({"username": "Alice"});
    /// let result = renderer.render_str("Hello {{ username }}!", &context)?;
    /// ```
    fn render_str(&self, template: &str, context: &serde_json::Value) -> Result<String>;

    /// Render a template string with a specific name for better error messages
    ///
    /// # Arguments
    ///
    /// * `name` - Template name to use in error messages (e.g., file path)
    /// * `template` - The template source code
    /// * `context` - Context data as a JSON value
    fn render_named_str(
        &self,
        name: &str,
        template: &str,
        context: &serde_json::Value,
    ) -> Result<String>;
}

/// Vault provider interface for password managers
///
/// Abstracts secret retrieval from various password managers
/// (Bitwarden, 1Password, pass, etc.).
pub trait VaultProvider {
    /// Get the name of this vault provider (e.g., "bitwarden", "1password")
    fn name(&self) -> &str;

    /// Check if this vault provider is available (CLI installed and accessible)
    fn is_available(&self) -> bool;

    /// Check if this vault requires unlocking before use
    fn requires_unlock(&self) -> bool;

    /// Unlock the vault (e.g., login, get session token)
    fn unlock(&mut self) -> Result<()>;

    /// Get a secret value by key/identifier
    fn get_secret(&self, key: &str) -> Result<String>;
}
