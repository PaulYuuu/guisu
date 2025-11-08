//! Configuration information exposed to templates
//!
//! This module defines simplified configuration structures that are exposed
//! to the template engine. These are lighter-weight versions of the full
//! Config structs, containing only the information needed by templates.

use serde::{Deserialize, Serialize};

/// Configuration information exposed to templates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigInfo {
    /// Age encryption configuration
    pub age: AgeConfigInfo,

    /// Bitwarden configuration
    pub bitwarden: BitwardenConfigInfo,

    /// UI configuration
    pub ui: UiConfigInfo,
}

impl ConfigInfo {
    /// Create ConfigInfo from individual config components
    pub fn new(age: AgeConfigInfo, bitwarden: BitwardenConfigInfo, ui: UiConfigInfo) -> Self {
        Self { age, bitwarden, ui }
    }
}

/// Age configuration exposed to templates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgeConfigInfo {
    /// Whether to derive recipient from identity
    ///
    /// When enabled, the public key is automatically derived from the identity
    /// for encryption. The encryption still uses asymmetric age encryption.
    ///
    /// In templates, accessible as `{{ guisu.config.age.derive }}`.
    /// Legacy name `symmetric` is still supported for backward compatibility.
    #[serde(rename = "derive", alias = "symmetric")]
    pub derive: bool,
}

/// Bitwarden configuration exposed to templates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitwardenConfigInfo {
    /// Which Bitwarden CLI provider is used: "bw" or "rbw"
    pub provider: String,
}

/// UI configuration exposed to templates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfigInfo {
    /// Icon display mode: "auto", "always", or "never"
    pub icons: String,

    /// Diff format: "unified", "split", or "inline"
    #[serde(rename = "diffFormat")]
    pub diff_format: String,

    /// Number of context lines for diffs
    #[serde(rename = "contextLines")]
    pub context_lines: usize,

    /// Number of lines to show in preview
    #[serde(rename = "previewLines")]
    pub preview_lines: usize,
}
