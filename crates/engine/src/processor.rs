//! Content processing for template rendering and decryption
//!
//! This module handles the processing pipeline for source file contents:
//! 1. Read source file
//! 2. Decrypt if encrypted (.age extension)
//! 3. Render template if templated (.j2 extension)
//! 4. Return processed content
//!
//! The order is important: for `.j2.age` files, we decrypt first, then render.

use crate::attr::FileAttributes;
use crate::content::{Decryptor, TemplateRenderer};
use guisu_core::path::AbsPath;
use guisu_core::{Error, Result};
use std::fs;

/// Content processor with pluggable decryption and rendering
///
/// This struct manages the processing pipeline using trait objects,
/// allowing any implementation of Decryptor and TemplateRenderer to be used.
pub struct ContentProcessor<D, R>
where
    D: Decryptor,
    R: TemplateRenderer,
{
    /// Decryptor for handling encrypted files
    decryptor: D,

    /// Renderer for processing templates
    renderer: R,
}

impl<D, R> ContentProcessor<D, R>
where
    D: Decryptor,
    R: TemplateRenderer,
{
    /// Create a new content processor
    ///
    /// # Arguments
    ///
    /// * `decryptor` - Implementation of Decryptor trait
    /// * `renderer` - Implementation of TemplateRenderer trait
    ///
    /// # Examples
    ///
    /// ```
    /// use guisu_engine::content::{NoOpDecryptor, NoOpRenderer};
    /// use guisu_engine::processor::ContentProcessor;
    ///
    /// let processor = ContentProcessor::new(
    ///     NoOpDecryptor,
    ///     NoOpRenderer,
    /// );
    /// ```
    pub fn new(decryptor: D, renderer: R) -> Self {
        Self {
            decryptor,
            renderer,
        }
    }

    /// Process a file based on its attributes
    ///
    /// # Arguments
    ///
    /// * `source_path` - Path to the source file
    /// * `attrs` - File attributes (is_encrypted, is_template, etc.)
    /// * `context` - Context data for template rendering
    ///
    /// # Returns
    ///
    /// Processed file content
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - File cannot be read
    /// - Decryption fails
    /// - Template rendering fails
    /// - Content is not valid UTF-8 (for templates)
    pub fn process_file(
        &self,
        source_path: &AbsPath,
        attrs: &FileAttributes,
        context: &serde_json::Value,
    ) -> Result<Vec<u8>> {
        let content = fs::read(source_path.as_path()).map_err(|e| Error::FileRead {
            path: source_path.as_path().to_path_buf(),
            source: e,
        })?;

        self.process_content(content, attrs, context, &source_path.to_string())
    }

    /// Process file content directly (without reading from disk)
    ///
    /// This is useful for testing or when content is already in memory.
    pub fn process_content(
        &self,
        mut content: Vec<u8>,
        attrs: &FileAttributes,
        context: &serde_json::Value,
        path_for_errors: &str,
    ) -> Result<Vec<u8>> {
        if attrs.is_encrypted() {
            content = self
                .decryptor
                .decrypt(&content)
                .map_err(|e| Error::Decryption {
                    path: path_for_errors.to_string(),
                    source: Box::new(e),
                })?;
        }

        if attrs.is_template() {
            let text = String::from_utf8(content).map_err(|e| Error::InvalidUtf8 {
                path: path_for_errors.to_string(),
                source: e,
            })?;

            let rendered =
                self.renderer
                    .render(&text, context)
                    .map_err(|e| Error::TemplateRender {
                        path: path_for_errors.to_string(),
                        source: Box::new(e),
                    })?;

            content = rendered.into_bytes();
        }

        Ok(content)
    }
}

// Type alias for no-op processor (useful for testing)
use crate::content::{NoOpDecryptor, NoOpRenderer};
pub type NoOpProcessor = ContentProcessor<NoOpDecryptor, NoOpRenderer>;

impl Default for NoOpProcessor {
    fn default() -> Self {
        Self::new(NoOpDecryptor, NoOpRenderer)
    }
}
