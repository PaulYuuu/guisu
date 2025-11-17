//! Cat command implementation
//!
//! Display the processed content of managed files (decrypt + render templates).

use anyhow::{Context, Result};
use guisu_core::path::AbsPath;
use guisu_engine::state::SourceState;
use guisu_template::TemplateContext;
use std::fs;
use std::path::{Path, PathBuf};

use guisu_config::Config;

/// Run the cat command
pub fn run(source_dir: &Path, dest_dir: &Path, files: &[PathBuf], config: &Config) -> Result<()> {
    if files.is_empty() {
        anyhow::bail!("No files specified. Usage: guisu cat <file>");
    }

    // Resolve all paths (handles root_entry and canonicalization)
    let paths = crate::common::ResolvedPaths::resolve(source_dir, dest_dir, config)?;
    let source_abs = &paths.dotfiles_dir;
    let dest_abs = &paths.dest_dir;

    // Create ignore matcher from .guisu/ignores.toml
    // Use dotfiles_dir as the match root so patterns match relative to the dotfiles directory
    let _ignore_matcher = guisu_config::IgnoreMatcher::from_ignores_toml(source_dir)
        .context("Failed to load ignore patterns from .guisu/ignores.toml")?;

    // Read source state
    let source_state =
        SourceState::read(source_abs.to_owned()).context("Failed to read source state")?;

    if source_state.is_empty() {
        anyhow::bail!("No files managed. Add files with: guisu add <file>");
    }

    // Process each file
    for file_path in files {
        cat_file(
            &source_state,
            dest_abs,
            file_path,
            config,
            source_dir,
            dest_dir,
        )?;
    }

    Ok(())
}

fn cat_file(
    source_state: &SourceState,
    dest_abs: &AbsPath,
    file_path: &Path,
    config: &Config,
    source_dir: &Path,
    dest_dir: &Path,
) -> Result<()> {
    // Expand tilde in path
    let expanded_path = if file_path.starts_with("~") {
        if let Some(home) = dirs::home_dir() {
            let path_str = file_path.to_string_lossy();
            let without_tilde = path_str
                .strip_prefix("~/")
                .or(path_str.strip_prefix("~"))
                .unwrap_or(&path_str);
            home.join(without_tilde)
        } else {
            file_path.to_path_buf()
        }
    } else {
        file_path.to_path_buf()
    };

    // Try to get absolute path, but if file doesn't exist, construct it manually
    let file_abs = if expanded_path.exists() {
        AbsPath::new(
            fs::canonicalize(&expanded_path)
                .with_context(|| format!("Failed to resolve path: {}", expanded_path.display()))?,
        )?
    } else {
        // File doesn't exist yet, construct absolute path manually
        let abs_path = if expanded_path.is_absolute() {
            expanded_path
        } else {
            std::env::current_dir()?.join(&expanded_path)
        };
        AbsPath::new(abs_path)?
    };

    // Get relative path from destination
    let rel_path = file_abs.strip_prefix(dest_abs).with_context(|| {
        format!(
            "File {} is not under destination directory {}",
            file_abs.as_path().display(),
            dest_abs.as_path().display()
        )
    })?;

    // Find the entry in source state
    let entry = source_state
        .get(&rel_path)
        .with_context(|| format!("File not managed by guisu: {}", file_path.display()))?;

    // Get source file path and attributes
    let (source_path, is_template, is_encrypted) = match entry {
        guisu_engine::entry::SourceEntry::File {
            source_path,
            attributes,
            ..
        } => (
            source_path,
            attributes.is_template(),
            attributes.is_encrypted(),
        ),
        guisu_engine::entry::SourceEntry::Directory { .. } => {
            anyhow::bail!("{} is a directory", file_path.display());
        }
        guisu_engine::entry::SourceEntry::Symlink { .. } => {
            anyhow::bail!("{} is a symlink", file_path.display());
        }
    };

    let source_file_path = source_state.source_file_path(source_path);

    // Read the file content
    let mut content = fs::read(source_file_path.as_path())
        .with_context(|| format!("Failed to read source file: {:?}", source_file_path))?;

    // Decrypt if encrypted
    if is_encrypted {
        content = decrypt_content(&content, config)?;
    }

    // Render template if needed
    if is_template {
        let content_str = String::from_utf8(content).context("File content is not valid UTF-8")?;

        // Load identities for encryption/decryption filters
        let identities = load_identities_for_template(config)?;

        // Create template engine with template directory support and bitwarden provider
        let engine =
            crate::create_template_engine(source_dir, std::sync::Arc::new(identities), config);

        let dotfiles_dir_str = config
            .dotfiles_dir(source_dir)
            .to_string_lossy()
            .to_string();
        let root_entry_str = config.general.root_entry.to_string_lossy().to_string();
        let mut context = TemplateContext::new().with_guisu_info(
            dotfiles_dir_str,
            dest_dir.to_string_lossy().to_string(),
            root_entry_str.clone(),
        );

        // Add user variables from config
        context = context.with_variables(config.variables.clone());

        // Build template name with root_entry prefix
        let template_name = format!("{}/{}", root_entry_str, source_path.as_path().display());

        let rendered = engine
            .render_named_str(&template_name, &content_str, &context)
            .map_err(|e| {
                // Enhance error message with source line content
                let error_msg = e.to_string();
                let enhanced_msg = enhance_template_error(&error_msg, &content_str);
                anyhow::anyhow!("{}", enhanced_msg)
            })?;

        content = rendered.into_bytes();
    }

    // Decrypt inline age values (sops-like behavior)
    // This allows files to contain age:base64(...) encrypted values
    // that are automatically decrypted when viewing with guisu cat
    let content_str = String::from_utf8(content).context("File content is not valid UTF-8")?;
    let identities = load_identities_for_template(config)?;
    if !identities.is_empty() {
        // Only try to decrypt if we have identities available
        let decrypted_content =
            guisu_crypto::decrypt_file_content(&content_str, &identities).unwrap_or(content_str); // If decryption fails, show original content
        content = decrypted_content.into_bytes();
    } else {
        content = content_str.into_bytes();
    }

    // Output the content
    std::io::Write::write_all(&mut std::io::stdout(), &content)?;

    // Ensure output ends with newline (POSIX standard for text files)
    // This prevents the shell '%' symbol from appearing
    if !content.is_empty() && !content.ends_with(b"\n") {
        println!();
    }

    Ok(())
}

/// Decrypt content using age
fn decrypt_content(encrypted: &[u8], config: &Config) -> Result<Vec<u8>> {
    // Load all configured identities
    let identities = config.age_identities()?;

    guisu_crypto::decrypt(encrypted, &identities).context("Failed to decrypt content")
}

/// Load age identities for template rendering (encrypt/decrypt filters)
fn load_identities_for_template(config: &Config) -> Result<Vec<guisu_crypto::Identity>> {
    // Try to load all configured identities
    // If no identities configured or files don't exist, return empty vec
    // Templates with encrypt/decrypt filters will fail with helpful error
    config.age_identities().or_else(|_| Ok(Vec::new()))
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
                "{}\nSource line {}:\n  {}",
                cleaned_msg, line_no, source_line
            );
        }
    }

    // If we couldn't enhance the message, return the cleaned version
    cleaned_msg
}
