//! Edit command implementation
//!
//! Edit files in the source directory with transparent decryption for encrypted files.

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use owo_colors::OwoColorize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

use guisu_config::Config;

/// Cached regex for matching inline age encrypted values
static AGE_VALUE_REGEX: Lazy<regex::Regex> = Lazy::new(|| {
    regex::Regex::new(r"age:[A-Za-z0-9+/]+=*")
        .expect("AGE_VALUE_REGEX compilation should never fail")
});

/// Run the edit command
pub fn run(
    source_dir: &Path,
    dest_dir: &Path,
    target: &Path,
    apply: bool,
    config: &Config,
) -> Result<()> {
    // Find the source file corresponding to the target
    let source_file = find_source_file(source_dir, dest_dir, target, config)?;

    // Check if the file is encrypted
    let is_encrypted = source_file
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e == "age")
        .unwrap_or(false);

    if is_encrypted {
        edit_encrypted_file(&source_file, config)?;
    } else {
        edit_regular_file(&source_file, config)?;
    }

    // Apply if requested
    if apply {
        println!("\n  {} Applying changes...", "→".bright_blue());
        let options = crate::cmd::apply::ApplyOptions::default();
        crate::cmd::apply::run(
            source_dir,
            dest_dir,
            &[target.to_path_buf()],
            &options,
            config,
        )?;
    }

    println!();
    Ok(())
}

/// Find the source file corresponding to a target file
fn find_source_file(
    source_dir: &Path,
    dest_dir: &Path,
    target: &Path,
    config: &Config,
) -> Result<PathBuf> {
    // Convert target to absolute path
    let target_abs = fs::canonicalize(target)
        .with_context(|| format!("Target file not found: {}", target.display()))?;

    // Get relative path from destination
    let rel_path = target_abs.strip_prefix(dest_dir).with_context(|| {
        format!(
            "Target {} is not under destination directory {}",
            target_abs.display(),
            dest_dir.display()
        )
    })?;

    // Build base path with root_entry
    let base_path = source_dir.join(&config.general.root_entry).join(rel_path);

    // Try possible file name combinations
    let candidates = vec![
        base_path.clone(),
        // Try adding .age extension
        {
            let mut path = base_path.clone();
            if let Some(file_name) = base_path.file_name() {
                path.set_file_name(format!("{}.age", file_name.to_string_lossy()));
            }
            path
        },
        // Try adding .j2 extension
        {
            let mut path = base_path.clone();
            if let Some(file_name) = base_path.file_name() {
                path.set_file_name(format!("{}.j2", file_name.to_string_lossy()));
            }
            path
        },
        // Handle .j2.age case
        {
            let mut path = base_path.clone();
            if let Some(file_name) = base_path.file_name() {
                path.set_file_name(format!("{}.j2.age", file_name.to_string_lossy()));
            }
            path
        },
    ];

    for candidate in &candidates {
        if candidate.exists() {
            return Ok(candidate.clone());
        }
    }

    anyhow::bail!("File not managed by guisu: {}", target.display())
}

/// Get the editor command to use
fn get_editor(config: &Config) -> Result<(String, Vec<String>)> {
    // 1. Use configured editor if available
    if let Some(editor_cmd) = config.editor_command()
        && let Some((cmd, args)) = editor_cmd.split_first()
    {
        return Ok((cmd.clone(), args.to_vec()));
    }

    // 2. Try $VISUAL environment variable
    if let Ok(visual) = env::var("VISUAL") {
        return Ok((visual, vec![]));
    }

    // 3. Try $EDITOR environment variable
    if let Ok(editor) = env::var("EDITOR") {
        return Ok((editor, vec![]));
    }

    // 4. Use system default editor
    #[cfg(unix)]
    const DEFAULT_EDITOR: &str = "vi";
    #[cfg(windows)]
    const DEFAULT_EDITOR: &str = "notepad.exe";

    Ok((DEFAULT_EDITOR.to_string(), vec![]))
}

/// Run the editor with the given file
fn run_editor(editor: &str, args: &[String], file: &Path) -> Result<()> {
    let status = Command::new(editor)
        .args(args)
        .arg(file)
        .status()
        .with_context(|| format!("Failed to run editor: {}", editor))?;

    if !status.success() {
        anyhow::bail!("Editor exited with error: {}", status);
    }

    Ok(())
}

/// Edit a regular (non-encrypted) file
/// This also handles files with inline age: encrypted values (sops-like behavior)
fn edit_regular_file(source_file: &Path, config: &Config) -> Result<()> {
    // Try to load all configured identities for inline decryption
    let identities = config.age_identities().ok();

    // If we have identities, check if the file contains inline encrypted values
    if let Some(ref ids) = identities
        && let Ok(content) = fs::read_to_string(source_file)
    {
        // Check if content contains age: prefix
        if content.contains("age:") {
            // Edit with inline decryption/encryption
            return edit_file_with_inline_encryption(source_file, config, ids);
        }
    }

    // No inline encryption or no identities - edit normally
    let (editor, args) = get_editor(config)?;
    run_editor(&editor, &args, source_file)
}

/// Edit a file that contains inline age: encrypted values
/// Decrypts them before editing and re-encrypts after
fn edit_file_with_inline_encryption(
    source_file: &Path,
    config: &Config,
    identities: &[guisu_crypto::Identity],
) -> Result<()> {
    use guisu_crypto::{decrypt_file_content, encrypt_inline};

    // Read the original file content
    let original_content = fs::read_to_string(source_file)
        .with_context(|| format!("Failed to read file: {}", source_file.display()))?;

    // Track all encrypted values for re-encryption using cached regex
    let encrypted_positions: Vec<_> = AGE_VALUE_REGEX
        .find_iter(&original_content)
        .map(|m| (m.start(), m.end(), m.as_str().to_string()))
        .collect();

    // Decrypt all inline values for editing
    let decrypted_content = decrypt_file_content(&original_content, identities)
        .context("Failed to decrypt inline age values")?;

    // Create temporary file
    let temp_dir = tempfile::TempDir::new().context("Failed to create temporary directory")?;
    let temp_file = temp_dir
        .path()
        .join(source_file.file_name().context("Invalid file name")?);

    // Write decrypted content to temp file
    fs::write(&temp_file, &decrypted_content)
        .context("Failed to write decrypted content to temporary file")?;

    // Open editor
    let (editor, args) = get_editor(config)?;
    run_editor(&editor, &args, &temp_file)?;

    // Read edited content
    let edited_content = fs::read_to_string(&temp_file).context("Failed to read edited content")?;

    // Check if content changed
    if edited_content == decrypted_content {
        println!("  {} No changes made", "ℹ".bright_blue());
        return Ok(());
    }

    // Re-encrypt the edited plaintext values
    let mut final_content = edited_content;

    // Convert all identities to recipients for re-encryption
    let recipients: Vec<_> = identities.iter().map(|id| id.to_public()).collect();

    for (_, _, encrypted_value) in encrypted_positions {
        if let Ok(decrypted_value) = guisu_crypto::decrypt_inline(&encrypted_value, identities)
            && final_content.contains(&decrypted_value)
        {
            let new_encrypted = encrypt_inline(&decrypted_value, &recipients)
                .context("Failed to re-encrypt value")?;
            final_content = final_content.replacen(&decrypted_value, &new_encrypted, 1);
        }
    }

    // Write the final content back to source file
    fs::write(source_file, &final_content)
        .with_context(|| format!("Failed to write file: {}", source_file.display()))?;

    println!(
        "  {} File updated with re-encrypted values",
        "✓".bright_green()
    );

    Ok(())
}

/// Edit an encrypted file with transparent decryption/encryption
fn edit_encrypted_file(source_file: &Path, config: &Config) -> Result<()> {
    use guisu_crypto::{decrypt, encrypt};

    // Load all configured identities
    let identities = config
        .age_identities()
        .context("Age identity not configured. Cannot edit encrypted files.")?;

    // Read and decrypt the file
    let encrypted_content = fs::read(source_file)
        .with_context(|| format!("Failed to read encrypted file: {}", source_file.display()))?;

    let decrypted_content =
        decrypt(&encrypted_content, &identities).context("Failed to decrypt file")?;

    // Create temporary directory and file
    let temp_dir = TempDir::new().context("Failed to create temporary directory")?;

    // Build temporary file name (remove .age extension)
    let temp_file_name = source_file
        .file_stem()
        .and_then(|s| s.to_str())
        .context("Invalid file name")?;

    let temp_file = temp_dir.path().join(temp_file_name);

    // Write decrypted content to temporary file
    fs::write(&temp_file, &decrypted_content)
        .context("Failed to write decrypted content to temporary file")?;

    // Get editor and run it
    let (editor, args) = get_editor(config)?;
    run_editor(&editor, &args, &temp_file)?;

    // Read the edited content
    let edited_content =
        fs::read(&temp_file).context("Failed to read edited content from temporary file")?;

    // Check if content changed
    if edited_content == decrypted_content {
        return Ok(());
    }

    // Re-encrypt the content with all recipients
    let recipients: Vec<_> = identities.iter().map(|id| id.to_public()).collect();
    let reencrypted_content =
        encrypt(&edited_content, &recipients).context("Failed to re-encrypt file")?;

    // Write back to source file
    fs::write(source_file, &reencrypted_content)
        .with_context(|| format!("Failed to write encrypted file: {}", source_file.display()))?;

    Ok(())
}
