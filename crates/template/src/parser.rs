//! Section parser for configuration files with `[section]` syntax.
//!
//! This module provides functionality to parse configuration files that use section headers
//! to organize content. It supports:
//!
//! - Global content (before any section header)
//! - Platform-specific sections: `[darwin]`, `[linux]`, `[windows]`
//! - Negated platform sections: `[!darwin]`
//! - Conditional sections: `[if:expression]`
//!
//! # Example
//!
//! ```
//! use guisu_engine::section_parser::{Section, SectionParser};
//!
//! let content = r#"
//! # Global content
//! *.log
//!
//! [darwin]
//! .DS_Store
//!
//! [!darwin]
//! .config
//! "#;
//!
//! let sections = SectionParser::parse(content);
//! assert!(sections.contains_key(&Section::Global));
//! assert!(sections.contains_key(&Section::Platform("darwin".to_string())));
//! ```

use indexmap::IndexMap;

/// Section type in configuration files.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Section {
    /// Global section (no section header).
    Global,

    /// Platform-specific section: `[darwin]`, `[linux]`, `[windows]`.
    Platform(String),

    /// Negated platform: `[!darwin]`.
    NotPlatform(String),

    /// Conditional expression: `[if:expression]`.
    Conditional(String),
}

/// Section parser for files with `[section]` syntax.
pub struct SectionParser;

impl SectionParser {
    /// Parse file content into sections.
    ///
    /// # Format
    ///
    /// ```text
    /// # Global content (before any section header)
    /// global_line_1
    /// global_line_2
    ///
    /// [section_name]
    /// section_line_1
    /// section_line_2
    ///
    /// [another_section]
    /// another_line_1
    /// ```
    ///
    /// # Returns
    ///
    /// IndexMap mapping `Section` to `Vec<String>` of lines.
    ///
    /// # Notes
    ///
    /// - Empty lines and comment-only lines (starting with `#`) are ignored
    /// - Lines are trimmed of leading and trailing whitespace
    /// - Content before any section header goes into `Section::Global`
    /// - Invalid section headers are treated as regular content lines
    ///
    /// # Example
    ///
    /// ```
    /// use guisu_engine::section_parser::{Section, SectionParser};
    ///
    /// let content = r#"
    /// global_content
    ///
    /// [darwin]
    /// darwin_content
    /// "#;
    ///
    /// let sections = SectionParser::parse(content);
    /// assert_eq!(sections[&Section::Global], vec!["global_content"]);
    /// assert_eq!(sections[&Section::Platform("darwin".to_string())], vec!["darwin_content"]);
    /// ```
    pub fn parse(content: &str) -> IndexMap<Section, Vec<String>> {
        let mut sections: IndexMap<Section, Vec<String>> = IndexMap::new();
        let mut current_section = Section::Global;

        for line in content.lines() {
            let trimmed = line.trim();

            // Skip empty lines and comment-only lines
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            // Check if this is a section header
            if let Some(section) = Self::parse_section_header(trimmed) {
                current_section = section;
            } else {
                // Add line to current section
                sections
                    .entry(current_section.clone())
                    .or_insert_with(Vec::new)
                    .push(trimmed.to_string());
            }
        }

        sections
    }

    /// Parse a section header line.
    ///
    /// # Format
    ///
    /// - `[darwin]` → `Some(Section::Platform("darwin"))`
    /// - `[!darwin]` → `Some(Section::NotPlatform("darwin"))`
    /// - `[if:gui == "false"]` → `Some(Section::Conditional("gui == \"false\""))`
    /// - `normal line` → `None`
    ///
    /// # Returns
    ///
    /// `Some(Section)` if the line is a valid section header, `None` otherwise.
    fn parse_section_header(line: &str) -> Option<Section> {
        let trimmed = line.trim();

        // Check if line starts with '[' and ends with ']'
        if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
            return None;
        }

        // Extract content between brackets
        let content = &trimmed[1..trimmed.len() - 1].trim();

        if content.is_empty() {
            return None;
        }

        // Check for conditional: [if:expression]
        if let Some(expr) = content.strip_prefix("if:") {
            return Some(Section::Conditional(expr.to_string()));
        }

        // Check for negated platform: [!platform]
        if let Some(platform) = content.strip_prefix('!') {
            return Some(Section::NotPlatform(platform.to_string()));
        }

        // Regular platform section
        Some(Section::Platform(content.to_string()))
    }
}
