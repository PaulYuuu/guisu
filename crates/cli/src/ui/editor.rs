//! Editor integration for manual conflict resolution

use anyhow::{Context, Result, anyhow};
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::NamedTempFile;

/// Open content in the user's preferred editor
///
/// This function:
/// 1. Creates a temporary file with the content
/// 2. Launches the editor specified by $EDITOR, $VISUAL, or a platform default
/// 3. Waits for the editor to close
/// 4. Returns the edited content
///
/// # Arguments
/// * `content` - Initial content to edit
/// * `file_path` - Optional path hint for the editor (helps with syntax highlighting)
///
/// # Returns
/// The edited content as a string
pub fn open_in_editor(content: &str, file_path: Option<&Path>) -> Result<String> {
    // Create temp file with appropriate extension
    let temp_file = if let Some(path) = file_path {
        if let Some(ext) = path.extension() {
            tempfile::Builder::new()
                .suffix(&format!(".{}", ext.to_string_lossy()))
                .tempfile()
                .context("Failed to create temporary file")?
        } else {
            NamedTempFile::new().context("Failed to create temporary file")?
        }
    } else {
        NamedTempFile::new().context("Failed to create temporary file")?
    };

    // Write content to temp file
    fs::write(temp_file.path(), content).with_context(|| {
        format!(
            "Failed to write to temporary file: {}",
            temp_file.path().display()
        )
    })?;

    // Get editor from environment
    let editor = get_editor()?;

    // Launch editor
    let status = Command::new(&editor)
        .arg(temp_file.path())
        .status()
        .with_context(|| format!("Failed to launch editor: {}", editor))?;

    if !status.success() {
        return Err(anyhow!(
            "Editor exited with non-zero status: {:?}",
            status.code()
        ));
    }

    // Read edited content
    let edited = fs::read_to_string(temp_file.path())
        .with_context(|| format!("Failed to read edited file: {}", temp_file.path().display()))?;

    Ok(edited)
}

/// Open content in editor for merging conflicts
///
/// Shows both complete files side-by-side for easy comparison and editing
pub fn open_for_merge(
    file_path: &Path,
    local_content: &str,
    remote_content: &str,
    base_content: Option<&str>,
) -> Result<String> {
    let mut content = String::new();

    // Add header comment explaining the split view format
    content.push_str(&format!(
        "# Split view merge for: {}\n",
        file_path.display()
    ));
    content.push_str("#\n");
    content.push_str("# This file shows BOTH complete versions below.\n");
    content.push_str("# Edit this content to create your desired final version.\n");
    content.push_str("# Lines starting with '#' will be ignored.\n");
    content.push_str("#\n");

    if base_content.is_some() {
        content.push_str("# THREE-WAY MERGE available:\n");
        content.push_str("# - DESTINATION: Your current file (below)\n");
        content.push_str("# - BASE: Last synchronized version (shown in middle section)\n");
        content.push_str("# - SOURCE: Incoming changes (shown at bottom)\n");
    } else {
        content.push_str("# TWO-WAY MERGE:\n");
        content.push_str("# - DESTINATION: Your current file (shown first)\n");
        content.push_str("# - SOURCE: Incoming changes (shown second)\n");
    }

    content.push_str("#\n");
    content.push_str("# Instructions:\n");
    content.push_str("#   1. Review both complete files below\n");
    content.push_str("#   2. Edit this content to create your desired final version\n");
    content.push_str("#   3. Delete the separator lines and section headers\n");
    content.push_str("#   4. Save and close the editor\n");
    content.push_str("#\n\n");

    // Show DESTINATION (local) content in full
    content.push_str(&"=".repeat(80));
    content.push('\n');
    content.push_str("# DESTINATION (current file) - COMPLETE CONTENT\n");
    content.push_str(&"=".repeat(80));
    content.push_str("\n\n");
    content.push_str(local_content);
    if !local_content.ends_with('\n') {
        content.push('\n');
    }
    content.push('\n');

    // If we have base content, show it
    if let Some(base) = base_content {
        content.push_str(&"=".repeat(80));
        content.push('\n');
        content.push_str("# BASE (last synchronized) - COMPLETE CONTENT\n");
        content.push_str(&"=".repeat(80));
        content.push_str("\n\n");
        content.push_str(base);
        if !base.ends_with('\n') {
            content.push('\n');
        }
        content.push('\n');
    }

    // Show SOURCE (remote) content in full
    content.push_str(&"=".repeat(80));
    content.push('\n');
    content.push_str("# SOURCE (incoming changes) - COMPLETE CONTENT\n");
    content.push_str(&"=".repeat(80));
    content.push_str("\n\n");
    content.push_str(remote_content);
    if !remote_content.ends_with('\n') {
        content.push('\n');
    }
    content.push('\n');
    content.push_str(&"=".repeat(80));
    content.push('\n');

    // Open in editor
    let edited = open_in_editor(&content, Some(file_path))?;

    // Remove header comments and separator lines
    let cleaned: String = edited
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            // Remove comment lines and separator lines
            !trimmed.starts_with('#') && !trimmed.chars().all(|c| c == '=')
        })
        .collect::<Vec<_>>()
        .join("\n");

    Ok(cleaned)
}

/// Get the editor command from environment variables
fn get_editor() -> Result<String> {
    // Try EDITOR first
    if let Ok(editor) = std::env::var("EDITOR")
        && !editor.is_empty()
    {
        return Ok(editor);
    }

    // Try VISUAL
    if let Ok(editor) = std::env::var("VISUAL")
        && !editor.is_empty()
    {
        return Ok(editor);
    }

    // Platform-specific defaults
    if cfg!(windows) {
        Ok("notepad".to_string())
    } else if cfg!(target_os = "macos") {
        // Check if nano is available, otherwise fall back to vi
        if Command::new("which")
            .arg("nano")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            Ok("nano".to_string())
        } else {
            Ok("vi".to_string())
        }
    } else {
        // Unix/Linux
        Ok("vi".to_string())
    }
}
