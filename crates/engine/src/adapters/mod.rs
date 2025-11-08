//! Adapter implementations for engine traits
//!
//! This module provides concrete implementations of engine traits
//! using the crypto and template services.

pub mod crypto;
pub mod template;

pub use self::crypto::CryptoDecryptorAdapter;
pub use self::template::TemplateRendererAdapter;

use crate::processor::ContentProcessor;
use ::guisu_crypto::Identity;
use ::guisu_template::TemplateEngine;

/// Convenience function to create a fully configured ContentProcessor
///
/// # Arguments
///
/// * `identity` - Age identity for decryption
/// * `template_engine` - Template engine for rendering
///
/// # Returns
///
/// A ContentProcessor configured with CryptoDecryptor and TemplateRenderer adapters
pub fn create_processor(
    identity: Identity,
    template_engine: TemplateEngine,
) -> ContentProcessor<CryptoDecryptorAdapter, TemplateRendererAdapter> {
    let decryptor = CryptoDecryptorAdapter::new(identity);
    let renderer = TemplateRendererAdapter::new(template_engine);
    ContentProcessor::new(decryptor, renderer)
}
