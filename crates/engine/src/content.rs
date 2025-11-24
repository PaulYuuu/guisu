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
    /// # Errors
    ///
    /// Returns an error if decryption fails (e.g., invalid format, wrong key, corrupted data)
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
    /// # Errors
    ///
    /// Returns an error if decryption fails or the decrypted data is not valid UTF-8
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
    /// # Errors
    ///
    /// Returns an error if template parsing or rendering fails (e.g., syntax error, missing variable)
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use super::*;

    #[test]
    fn test_noop_decryptor_decrypt() {
        let decryptor = NoOpDecryptor;
        let data = b"test data";

        let result = decryptor.decrypt(data).expect("Decrypt failed");
        assert_eq!(result, data);
    }

    #[test]
    fn test_noop_decryptor_decrypt_empty() {
        let decryptor = NoOpDecryptor;
        let data = b"";

        let result = decryptor.decrypt(data).expect("Decrypt failed");
        assert_eq!(result, data);
    }

    #[test]
    fn test_noop_decryptor_decrypt_binary() {
        let decryptor = NoOpDecryptor;
        let data = b"\x00\x01\x02\xFF\xFE\xFD";

        let result = decryptor.decrypt(data).expect("Decrypt failed");
        assert_eq!(result, data);
    }

    #[test]
    fn test_noop_decryptor_decrypt_inline() {
        let decryptor = NoOpDecryptor;
        let text = "Hello, World!";

        let result = decryptor
            .decrypt_inline(text)
            .expect("Decrypt inline failed");
        assert_eq!(result, text);
    }

    #[test]
    fn test_noop_decryptor_decrypt_inline_empty() {
        let decryptor = NoOpDecryptor;
        let text = "";

        let result = decryptor
            .decrypt_inline(text)
            .expect("Decrypt inline failed");
        assert_eq!(result, text);
    }

    #[test]
    fn test_noop_decryptor_decrypt_inline_with_special_chars() {
        let decryptor = NoOpDecryptor;
        let text = "age:base64_encrypted_data";

        let result = decryptor
            .decrypt_inline(text)
            .expect("Decrypt inline failed");
        assert_eq!(result, text);
    }

    #[test]
    fn test_noop_renderer_render() {
        let renderer = NoOpRenderer;
        let template = "Hello, {{ name }}!";
        let context = serde_json::json!({"name": "World"});

        let result = renderer.render(template, &context).expect("Render failed");
        // NoOpRenderer should return template as-is without processing
        assert_eq!(result, template);
    }

    #[test]
    fn test_noop_renderer_render_empty() {
        let renderer = NoOpRenderer;
        let template = "";
        let context = serde_json::json!({});

        let result = renderer.render(template, &context).expect("Render failed");
        assert_eq!(result, "");
    }

    #[test]
    fn test_noop_renderer_render_plain_text() {
        let renderer = NoOpRenderer;
        let template = "This is plain text without variables.";
        let context = serde_json::json!({});

        let result = renderer.render(template, &context).expect("Render failed");
        assert_eq!(result, template);
    }

    #[test]
    fn test_noop_renderer_render_ignores_context() {
        let renderer = NoOpRenderer;
        let template = "Template text";

        // Context should be completely ignored
        let context1 = serde_json::json!({"key": "value"});
        let result1 = renderer.render(template, &context1).expect("Render failed");

        let context2 = serde_json::json!({"different": "data"});
        let result2 = renderer.render(template, &context2).expect("Render failed");

        assert_eq!(result1, template);
        assert_eq!(result2, template);
        assert_eq!(result1, result2);
    }

    #[test]
    fn test_noop_renderer_render_with_complex_context() {
        let renderer = NoOpRenderer;
        let template = "{{ nested.value }}";
        let context = serde_json::json!({
            "nested": {
                "value": "ignored"
            }
        });

        let result = renderer.render(template, &context).expect("Render failed");
        // Should return template as-is, not process it
        assert_eq!(result, template);
    }
}
