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

use guisu_config::Config;

/// Run templates list command
///
/// Lists all template files available for the current platform.
/// This includes templates from:
/// - .guisu/templates/ (common templates)
/// - .guisu/templates/`<platform>`/ (platform-specific templates)
pub fn run_list(source_dir: &Path, _config: &Config) -> Result<()> {
    let platform = CURRENT_PLATFORM.os;
    println!(
        "{}: {}\n",
        "Templates for platform".bright_cyan().bold(),
        platform.bright_white()
    );

    // Get the .guisu directory
    let guisu_dir = source_dir.join(".guisu");
    let templates_dir = guisu_dir.join("templates");

    if !templates_dir.exists() {
        println!("No templates directory found.");
        println!(
            "\nTo create templates, add files to: {}",
            templates_dir.display()
        );
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
        println!("\nTo create templates, add files to:");
        println!("  {} (common)", templates_dir.display());
        println!("  {} (platform-specific)", platform_dir.display());
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
pub fn run_show(
    source_dir: &Path,
    dest_dir: &Path,
    template_name: &str,
    config: &Config,
) -> Result<()> {
    let platform = CURRENT_PLATFORM.os;

    // Get the .guisu directory
    let guisu_dir = source_dir.join(".guisu");
    let templates_dir = guisu_dir.join("templates");

    // Search for the template (platform-specific takes precedence)
    let platform_template = templates_dir.join(platform).join(template_name);
    let common_template = templates_dir.join(template_name);

    let template_path = if platform_template.is_file() {
        platform_template
    } else if common_template.is_file() {
        common_template
    } else {
        anyhow::bail!(
            "Template '{}' not found.\n\nRun 'guisu templates list' to see available templates.",
            template_name
        );
    };

    // Read template content
    let template_content = fs::read_to_string(&template_path)
        .with_context(|| format!("Failed to read template: {}", template_path.display()))?;

    // Load age identities for template rendering (encrypt/decrypt filters)
    let identities = config.age_identities().unwrap_or_default();

    // Load variables from .guisu/variables/ directory
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
    let context = create_template_context(config, source_dir, dest_dir, all_variables)?;

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
    print!("{}", rendered);

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
) -> Result<TemplateContext> {
    let dotfiles_dir_str = crate::path_to_string(&config.dotfiles_dir(source_dir));
    let root_entry_str = crate::path_to_string(&config.general.root_entry);
    let working_tree = guisu_engine::git::find_working_tree(source_dir)
        .unwrap_or_else(|| source_dir.to_path_buf());

    let context = TemplateContext::new()
        .with_guisu_info(
            dotfiles_dir_str,
            crate::path_to_string(&working_tree),
            crate::path_to_string(dest_dir),
            root_entry_str,
        )
        .with_variables(variables);

    Ok(context)
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
    let engine =
        crate::create_template_engine(source_dir, std::sync::Arc::new(identities.to_vec()), config);

    // Render the template
    engine
        .render_named_str(template_name, template, context)
        .map_err(|e| {
            // Enhance error message with source line content
            let error_msg = e.to_string();
            let enhanced_msg = enhance_template_error(&error_msg, template);
            anyhow::anyhow!("{}", enhanced_msg)
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
            .take_while(|c| c.is_ascii_digit())
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
            return format!(
                "{}\\nSource line {}:\\n  {}",
                cleaned_msg, line_no, source_line
            );
        }
    }

    // If we couldn't enhance the message, return the cleaned version
    cleaned_msg
}
