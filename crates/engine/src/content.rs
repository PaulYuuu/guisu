//! Content processing traits
//!
//! This module defines the traits for content decryption and template rendering.
//! Engine uses these traits without depending on specific implementations.

/// Trait for content decryption
///
/// Implementations of this trait provide age decryption capabilities.
/// This allows engine to decrypt content without depending on the crypto crate.
pub trait Decryptor: Send + Sync {
    /// Error type for decryption operations
    type Error: std::error::Error + Send + Sync + 'static;

    /// Decrypt encrypted content
    ///
    /// # Arguments
    ///
    /// * `encrypted` - The encrypted data
    ///
    /// # Returns
    ///
    /// Decrypted data or an error
    fn decrypt(&self, encrypted: &[u8]) -> Result<Vec<u8>, Self::Error>;

    /// Decrypt inline encrypted text (for use in templates)
    ///
    /// # Arguments
    ///
    /// * `text` - Text containing encrypted content
    ///
    /// # Returns
    ///
    /// Decrypted text or an error
    fn decrypt_inline(&self, text: &str) -> Result<String, Self::Error>;
}

/// Trait for template rendering
///
/// Implementations of this trait provide template rendering capabilities.
/// This allows engine to render templates without depending on the template crate.
pub trait TemplateRenderer: Send + Sync {
    /// Error type for rendering operations
    type Error: std::error::Error + Send + Sync + 'static;

    /// Render a template with the given context
    ///
    /// # Arguments
    ///
    /// * `template` - The template string
    /// * `context` - Context data for rendering
    ///
    /// # Returns
    ///
    /// Rendered string or an error
    fn render(&self, template: &str, context: &serde_json::Value) -> Result<String, Self::Error>;
}

/// No-op decryptor for testing or when encryption is disabled
pub struct NoOpDecryptor;

impl Decryptor for NoOpDecryptor {
    type Error = std::io::Error;

    fn decrypt(&self, encrypted: &[u8]) -> Result<Vec<u8>, Self::Error> {
        // Return data as-is
        Ok(encrypted.to_vec())
    }

    fn decrypt_inline(&self, text: &str) -> Result<String, Self::Error> {
        // Return text as-is
        Ok(text.to_string())
    }
}

/// No-op renderer for testing or when templating is disabled
pub struct NoOpRenderer;

impl TemplateRenderer for NoOpRenderer {
    type Error = std::io::Error;

    fn render(&self, template: &str, _context: &serde_json::Value) -> Result<String, Self::Error> {
        // Return template as-is
        Ok(template.to_string())
    }
}
