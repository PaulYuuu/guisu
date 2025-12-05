//! Templates command operations
//!
//! This module provides commands for managing template files:
//! - list: List available template files for the current platform
//! - show: Display rendered content of a specific template

use anyhow::{Context, Result};
use guisu_core::platform::CURRENT_PLATFORM;
use guisu_template::TemplateContext;
use owo_colors::OwoColorize;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use crate::path_utils::SourceDirExt;
use guisu_config::Config;

/// Run templates list command
///
/// Lists all template files available for the current platform.
/// This includes templates from:
/// - .guisu/templates/ (common templates)
/// - .guisu/templates/`<platform>`/ (platform-specific templates)
///
/// # Errors
///
/// Currently never returns an error (Result is for future compatibility)
pub fn run_list(source_dir: &Path, _config: &Config) -> Result<()> {
    let platform = CURRENT_PLATFORM.os;
    println!(
        "{}: {}\n",
        "Templates for platform".bright_cyan().bold(),
        platform.bright_white()
    );

    // Get the templates directory
    let templates_dir = source_dir.templates_dir();

    if !templates_dir.exists() {
        println!("No templates directory found.");
        return Ok(());
    }

    // Collect template filenames from both common and platform-specific directories
    let mut template_names = BTreeSet::new();

    // Scan common templates (root of templates/)
    if let Ok(entries) = fs::read_dir(&templates_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            // Only include files, skip directories
            if path.is_file()
                && let Some(name) = path.file_name().and_then(|n| n.to_str())
            {
                template_names.insert(name.to_string());
            }
        }
    }

    // Scan platform-specific templates
    let platform_dir = templates_dir.join(platform);
    if platform_dir.exists()
        && let Ok(entries) = fs::read_dir(&platform_dir)
    {
        for entry in entries.flatten() {
            let path = entry.path();
            // Only include files
            if path.is_file()
                && let Some(name) = path.file_name().and_then(|n| n.to_str())
            {
                template_names.insert(name.to_string());
            }
        }
    }

    // Display results
    if template_names.is_empty() {
        println!("No templates found.");
    } else {
        for name in &template_names {
            println!("  {}", name.bright_white());
        }

        println!(
            "\n({} {})",
            template_names.len().to_string().bright_green().bold(),
            if template_names.len() == 1 {
                "template"
            } else {
                "templates"
            }
        );
    }

    Ok(())
}

/// Run templates show command
///
/// Displays the rendered content of a specific template.
/// The template is searched for in:
/// 1. .guisu/templates/`<platform>`/ (takes precedence)
/// 2. .guisu/templates/ (fallback)
///
/// The template is rendered with all available variables and guisu context.
///
/// # Errors
///
/// Returns an error if:
/// - The specified template is not found
/// - Reading the template file fails
/// - Loading variables from .guisu/variables/ fails
/// - Creating the template engine fails
/// - Template rendering fails
pub fn run_show(
    source_dir: &Path,
    dest_dir: &Path,
    template_name: &str,
    config: &Config,
) -> Result<()> {
    let platform = CURRENT_PLATFORM.os;

    // Get the templates directory
    let templates_dir = source_dir.templates_dir();

    // Search for the template (platform-specific takes precedence)
    let platform_template = templates_dir.join(platform).join(template_name);
    let common_template = templates_dir.join(template_name);

    let template_path = if platform_template.is_file() {
        platform_template
    } else if common_template.is_file() {
        common_template
    } else {
        anyhow::bail!(
            "Template '{template_name}' not found.\n\nRun 'guisu templates list' to see available templates."
        );
    };

    // Read template content
    let template_content = fs::read_to_string(&template_path)
        .with_context(|| format!("Failed to read template: {}", template_path.display()))?;

    // Load age identities for template rendering (encrypt/decrypt filters)
    let identities = config.age_identities().unwrap_or_default();

    // Load variables from .guisu/variables/ directory
    let guisu_dir = source_dir.guisu_dir();
    let guisu_variables = if guisu_dir.exists() {
        guisu_config::variables::load_variables(&guisu_dir, platform)
            .context("Failed to load variables from .guisu/variables/")?
    } else {
        indexmap::IndexMap::new()
    };

    // Merge variables: first from .guisu/variables/, then from config (config overrides)
    let mut all_variables = guisu_variables;
    all_variables.extend(config.variables.iter().map(|(k, v)| (k.clone(), v.clone())));

    // Create template context
    let context = create_template_context(config, source_dir, dest_dir, all_variables);

    // Render the template
    let rendered = render_template(
        &template_content,
        template_name,
        &context,
        &identities,
        source_dir,
        config,
    )?;

    // Output the rendered content
    print!("{rendered}");

    // Ensure output ends with newline (POSIX standard)
    if !rendered.ends_with('\n') {
        println!();
    }

    Ok(())
}

/// Create template context with system and guisu information
fn create_template_context(
    config: &Config,
    source_dir: &Path,
    dest_dir: &Path,
    variables: indexmap::IndexMap<String, serde_json::Value>,
) -> TemplateContext {
    let dotfiles_dir_str = crate::path_to_string(&config.dotfiles_dir(source_dir));
    let root_entry_str = crate::path_to_string(&config.general.root_entry);
    let working_tree = guisu_engine::git::find_working_tree(source_dir)
        .unwrap_or_else(|| source_dir.to_path_buf());

    TemplateContext::new()
        .with_guisu_info(
            dotfiles_dir_str,
            crate::path_to_string(&working_tree),
            crate::path_to_string(dest_dir),
            root_entry_str,
        )
        .with_variables(variables)
}

/// Render a template with the given context
fn render_template(
    template: &str,
    template_name: &str,
    context: &TemplateContext,
    identities: &[guisu_crypto::Identity],
    source_dir: &Path,
    config: &guisu_config::Config,
) -> Result<String> {
    // Create engine with template directory support for includes and bitwarden provider
    let engine = crate::create_template_engine(
        source_dir,
        &std::sync::Arc::new(identities.to_vec()),
        config,
    );

    // Render the template
    engine
        .render_named_str(template_name, template, context)
        .map_err(|e| {
            // Enhance error message with source line content
            let error_msg = e.to_string();
            let enhanced_msg = enhance_template_error(&error_msg, template);
            anyhow::anyhow!("{enhanced_msg}")
        })
}

/// Enhance template error messages with source line content
///
/// Extracts the line number from the error message, removes redundant path info,
/// and includes the actual source line content to help users identify issues.
fn enhance_template_error(error_msg: &str, template_source: &str) -> String {
    // Clean up the error message - remove redundant " (in filename:line)" suffix
    let cleaned_msg = if let Some(pos) = error_msg.find(" (in ") {
        error_msg[..pos].to_string()
    } else {
        error_msg.to_string()
    };

    // Try to extract line number from error message
    // Format: "... line 25 ..." or "... line 25, column 10 ..."
    let line_num = if let Some(pos) = cleaned_msg.find(" line ") {
        let after_line = &cleaned_msg[pos + 6..];
        // Extract digits until non-digit
        let num_str: String = after_line
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        num_str.parse::<usize>().ok()
    } else {
        None
    };

    // If we found a line number, try to include the full source line
    if let Some(line_no) = line_num {
        let lines: Vec<&str> = template_source.lines().collect();
        if line_no > 0 && line_no <= lines.len() {
            let source_line = lines[line_no - 1]; // lines are 1-indexed
            // Show the full line content (no truncation)
            return format!("{cleaned_msg}\\nSource line {line_no}:\\n  {source_line}");
        }
    }

    // If we couldn't enhance the message, return the cleaned version
    cleaned_msg
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use super::*;

    // Tests for enhance_template_error

    #[test]
    fn test_enhance_template_error_with_line_number() {
        let error_msg = "undefined variable 'foo' at line 3";
        let template = "line 1\nline 2\nline 3 has {{ foo }}\nline 4";

        let result = enhance_template_error(error_msg, template);

        assert!(result.contains("undefined variable 'foo' at line 3"));
        assert!(result.contains("Source line 3:"));
        assert!(result.contains("line 3 has {{ foo }}"));
    }

    #[test]
    fn test_enhance_template_error_with_line_and_column() {
        let error_msg = "syntax error at line 2, column 5";
        let template = "line 1\nline 2 error here\nline 3";

        let result = enhance_template_error(error_msg, template);

        assert!(result.contains("syntax error at line 2"));
        assert!(result.contains("Source line 2:"));
        assert!(result.contains("line 2 error here"));
    }

    #[test]
    fn test_enhance_template_error_removes_redundant_path() {
        let error_msg = "error message (in template.j2:5)";
        let template = "1\n2\n3\n4\n5 error\n6";

        let result = enhance_template_error(error_msg, template);

        // Should remove the "(in template.j2:5)" part
        assert!(!result.contains("(in template.j2:5)"));
        assert!(result.contains("error message"));
    }

    #[test]
    fn test_enhance_template_error_no_line_number() {
        let error_msg = "general error with no line info";
        let template = "some\ntemplate\ncontent";

        let result = enhance_template_error(error_msg, template);

        // Should just return the error message unchanged (since no line number)
        assert_eq!(result, "general error with no line info");
    }

    #[test]
    fn test_enhance_template_error_line_number_out_of_bounds() {
        let error_msg = "error at line 100";
        let template = "only\nthree\nlines";

        let result = enhance_template_error(error_msg, template);

        // Should return cleaned message without source line (line 100 doesn't exist)
        assert_eq!(result, "error at line 100");
        assert!(!result.contains("Source line"));
    }

    #[test]
    fn test_enhance_template_error_line_zero() {
        let error_msg = "error at line 0";
        let template = "line 1\nline 2";

        let result = enhance_template_error(error_msg, template);

        // Line 0 is invalid (lines are 1-indexed)
        assert_eq!(result, "error at line 0");
        assert!(!result.contains("Source line"));
    }

    #[test]
    fn test_enhance_template_error_first_line() {
        let error_msg = "error at line 1";
        let template = "first line with error\nsecond line";

        let result = enhance_template_error(error_msg, template);

        assert!(result.contains("Source line 1:"));
        assert!(result.contains("first line with error"));
    }

    #[test]
    fn test_enhance_template_error_last_line() {
        let error_msg = "error at line 3";
        let template = "line 1\nline 2\nline 3 is last";

        let result = enhance_template_error(error_msg, template);

        assert!(result.contains("Source line 3:"));
        assert!(result.contains("line 3 is last"));
    }

    #[test]
    fn test_enhance_template_error_empty_template() {
        let error_msg = "error at line 1";
        let template = "";

        let result = enhance_template_error(error_msg, template);

        // Can't show source line from empty template
        assert_eq!(result, "error at line 1");
    }

    #[test]
    fn test_enhance_template_error_complex_error_message() {
        let error_msg =
            "template error: undefined filter 'missing' at line 5, column 10 (in my_template.j2:5)";
        let template = "line 1\nline 2\nline 3\nline 4\nline 5 {{ value | missing }}\nline 6";

        let result = enhance_template_error(error_msg, template);

        // Should clean the path suffix
        assert!(!result.contains("(in my_template.j2:5)"));
        // Should show original error
        assert!(result.contains("undefined filter 'missing'"));
        // Should include source line
        assert!(result.contains("Source line 5:"));
        assert!(result.contains("line 5 {{ value | missing }}"));
    }

    #[test]
    fn test_enhance_template_error_multiple_line_keywords() {
        // Test that we extract the first "line" keyword
        let error_msg = "error on line 2, see line 5 for details";
        let template = "line 1\nline 2 has error\nline 3";

        let result = enhance_template_error(error_msg, template);

        // Should use the first line number (2)
        assert!(result.contains("Source line 2:"));
        assert!(result.contains("line 2 has error"));
    }

    #[test]
    fn test_enhance_template_error_preserves_special_characters() {
        let error_msg = "error at line 2";
        let template = "line 1\nline 2: {{ special }} <>&\nline 3";

        let result = enhance_template_error(error_msg, template);

        // Should preserve all special characters in source line
        assert!(result.contains("line 2: {{ special }} <>&"));
    }
}
