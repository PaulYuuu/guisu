//! Add command implementation
//!
//! Add files to the guisu source directory.

use anyhow::{Context, Result};
use guisu_core::path::AbsPath;
use once_cell::sync::Lazy;
use owo_colors::OwoColorize;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::warn;
use walkdir::WalkDir;

use guisu_config::Config;

/// How to handle files containing secrets
#[derive(Debug, Clone, Copy, PartialEq)]
enum SecretsMode {
    /// Ignore secrets and add files anyway
    Ignore,
    /// Show warnings about secrets but proceed
    Warning,
    /// Fail if secrets are detected
    Error,
}

/// Options for the add command
#[derive(Debug, Clone)]
pub struct AddOptions {
    pub template: bool,
    pub autotemplate: bool,
    pub encrypt: bool,
    pub create: bool,
    pub force: bool,
    pub secrets: String,
}

impl Default for AddOptions {
    fn default() -> Self {
        Self {
            template: false,
            autotemplate: false,
            encrypt: false,
            create: false,
            force: false,
            secrets: "error".to_string(),
        }
    }
}

/// Parameters for adding files to guisu (internal)
#[derive(Debug, Clone)]
struct AddParams<'a> {
    source_dir: &'a AbsPath,
    dest_dir: &'a AbsPath,
    template: bool,
    autotemplate: bool,
    encrypt: bool,
    force: bool,
    secrets_mode: SecretsMode,
    config: &'a Config,
}

pub fn run(
    source_dir: &Path,
    dest_dir: &Path,
    files: &[PathBuf],
    options: &AddOptions,
    config: &Config,
) -> Result<()> {
    // Parse secrets handling mode
    let secrets_mode = match options.secrets.as_str() {
        "ignore" => SecretsMode::Ignore,
        "warning" => SecretsMode::Warning,
        "error" => SecretsMode::Error,
        _ => anyhow::bail!(
            "Invalid secrets mode: {}. Must be one of: ignore, warning, error",
            options.secrets
        ),
    };

    // Create source directory if it doesn't exist
    if !source_dir.exists() {
        fs::create_dir_all(source_dir).with_context(|| {
            format!(
                "Failed to create source directory: {}",
                source_dir.display()
            )
        })?;
        println!(
            "\n  {} {}",
            "created".dimmed(),
            source_dir.display().to_string().bright_cyan()
        );
    }

    let source_abs = AbsPath::new(fs::canonicalize(source_dir).with_context(|| {
        format!(
            "Failed to access source directory: {}",
            source_dir.display()
        )
    })?)?;
    let dest_abs = AbsPath::new(fs::canonicalize(dest_dir).with_context(|| {
        format!(
            "Failed to access destination directory: {}",
            dest_dir.display()
        )
    })?)?;

    // Load metadata if create flag is used
    let mut metadata = if options.create {
        guisu_engine::state::Metadata::load(source_dir).context("Failed to load metadata")?
    } else {
        guisu_engine::state::Metadata::default()
    };

    // Create AddParams struct to pass to helper functions
    let params = AddParams {
        source_dir: &source_abs,
        dest_dir: &dest_abs,
        template: options.template,
        autotemplate: options.autotemplate,
        encrypt: options.encrypt,
        force: options.force,
        secrets_mode,
        config,
    };

    println!();
    let mut total_files_added = 0;

    for file_path in files {
        let (rel_path, count) = add_file(&params, file_path)
            .with_context(|| format!("Failed to add file: {}", file_path.display()))?;

        total_files_added += count;

        // Add to create-once list if requested
        if options.create {
            metadata.add_create_once(rel_path.to_string());
        }
    }

    // Save metadata if create flag was used
    if options.create {
        metadata
            .save(source_dir)
            .context("Failed to save metadata")?;
        println!(
            "  {} {}",
            "✓".bright_green(),
            "Updated .guisu/state.toml with create-once files".dimmed()
        );
    }

    println!(
        "\n  {} {}\n",
        "✓".bright_green(),
        format!("Added {} file(s)", total_files_added).bright_white()
    );
    Ok(())
}

fn add_file(params: &AddParams, file_path: &Path) -> Result<(guisu_core::path::RelPath, usize)> {
    // Check if file is a symlink before canonicalization
    // This prevents symlink-based path traversal attacks
    let metadata = fs::symlink_metadata(file_path)
        .with_context(|| format!("File not found: {}", file_path.display()))?;

    let file_abs = if metadata.is_symlink() {
        // For symlinks, resolve the parent directory but not the symlink itself
        let parent = file_path
            .parent()
            .with_context(|| format!("Cannot get parent directory of {}", file_path.display()))?;
        let file_name = file_path
            .file_name()
            .with_context(|| format!("Cannot get file name of {}", file_path.display()))?;

        let parent_abs = fs::canonicalize(parent)
            .with_context(|| format!("Cannot resolve parent directory: {}", parent.display()))?;

        AbsPath::new(parent_abs.join(file_name))?
    } else {
        // For regular files/directories, canonicalize normally
        AbsPath::new(
            fs::canonicalize(file_path)
                .with_context(|| format!("Cannot resolve path: {}", file_path.display()))?,
        )?
    };

    // Get relative path from destination
    let rel_path = file_abs.strip_prefix(params.dest_dir).with_context(|| {
        format!(
            "File {} is not under destination directory {}",
            file_abs.as_path().display(),
            params.dest_dir.as_path().display()
        )
    })?;

    // Check if it's a directory
    let metadata = fs::metadata(file_abs.as_path())
        .with_context(|| format!("Failed to read metadata: {}", file_path.display()))?;

    let count = if metadata.is_dir() {
        // Add directory recursively
        add_directory(params, &file_abs, &rel_path)?
    } else if metadata.is_symlink() {
        // Add symlink
        add_symlink(
            params.source_dir,
            &rel_path,
            &file_abs,
            params.force,
            params.config,
        )?;
        1
    } else {
        // Add regular file
        add_regular_file(params, &rel_path, &file_abs)?;
        1
    };

    Ok((rel_path, count))
}

/// Add a regular file to the source directory
fn add_regular_file(
    params: &AddParams,
    rel_path: &guisu_core::path::RelPath,
    file_abs: &AbsPath,
) -> Result<()> {
    // Read the file content first (needed for autotemplate detection)
    let content = fs::read(file_abs.as_path())
        .with_context(|| format!("Failed to read file: {}", file_abs.as_path().display()))?;

    // Check for secrets unless mode is Ignore
    if params.secrets_mode != SecretsMode::Ignore
        && let Some(secret_findings) = detect_secrets(file_abs.as_path(), &content)?
    {
        let warning_msg = format!(
            "Potential secrets detected in {}:\n{}",
            rel_path.as_path().display(),
            secret_findings
        );

        match params.secrets_mode {
            SecretsMode::Error => {
                anyhow::bail!(
                    "{}\n\nTo add anyway, use: guisu add --secrets ignore\n\
                         To encrypt the file, use: guisu add --encrypt",
                    warning_msg
                );
            }
            SecretsMode::Warning => {
                warn!("{}", warning_msg);
                warn!("  Tip: Use --encrypt to protect sensitive data");
            }
            SecretsMode::Ignore => unreachable!(),
        }
    }

    // Determine if file should be templated
    let (is_template, processed_content) = if params.autotemplate && !params.encrypt {
        // Auto-detect template variables and convert content
        match auto_template_content(&content, params.config) {
            Ok((templated_content, has_replacements)) => {
                if has_replacements {
                    (true, templated_content)
                } else {
                    (params.template, content)
                }
            }
            Err(e) => {
                warn!(
                    "autotemplate failed for {}: {}",
                    file_abs.as_path().display(),
                    e
                );
                (params.template, content)
            }
        }
    } else {
        (params.template, content)
    };

    // Validate encryption configuration if needed (before deleting any files)
    if params.encrypt {
        // This will fail early if encryption config is invalid
        validate_encryption_config(params.config)?;
    }

    // Build source filename with V2 extensions
    let rel_str = rel_path.as_path().to_string_lossy();
    let mut source_filename = rel_str.to_string();

    // Add extensions in the correct order (template, then encryption)
    if is_template {
        source_filename.push_str(".j2");
    }
    if params.encrypt {
        source_filename.push_str(".age");
    }

    // Apply root_entry (defaults to "home")
    let source_file_path = params
        .source_dir
        .as_path()
        .join(&params.config.general.root_entry)
        .join(&source_filename);

    // Check if file already exists in source (in any form)
    if let Some(existing_file) =
        check_file_exists_in_source(params.source_dir, rel_path, params.config)?
    {
        if !params.force {
            // Determine the type of existing file
            let file_name = existing_file
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            let file_type = if file_name.ends_with(".j2.age") {
                "encrypted template"
            } else if file_name.ends_with(".age") {
                "encrypted file"
            } else if file_name.ends_with(".j2") {
                "template"
            } else {
                "file"
            };

            anyhow::bail!(
                "This file is already managed by guisu as a {}:\n  {}\n\n\
                 To re-add with different attributes, use: guisu add --force\n\
                 To see differences, use: guisu diff\n\
                 To merge changes, use: guisu merge (not yet implemented)",
                file_type,
                existing_file.display()
            );
        } else {
            // Force is true - handle re-adding with potentially different attributes

            // Detect existing file attributes
            let file_name = existing_file
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            let was_template = file_name.contains(".j2");
            let was_encrypted = file_name.ends_with(".age");

            // Determine if attributes are changing
            let attrs_changing = (is_template != was_template) || (params.encrypt != was_encrypted);

            if attrs_changing {
                // Attributes are changing - delete the old file
                fs::remove_file(&existing_file).with_context(|| {
                    format!("Failed to remove old file: {}", existing_file.display())
                })?;
            } else {
                // Attributes are the same - will overwrite in place
                // The existing source_file_path should match existing_file
            }
        }
    }

    // Create parent directory if needed
    if let Some(parent) = source_file_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    // Encrypt if requested
    let final_content = if params.encrypt {
        encrypt_content(&processed_content, params.config)?
    } else {
        processed_content.to_vec()
    };

    // Write the (possibly encrypted) content
    fs::write(&source_file_path, &final_content)
        .with_context(|| format!("Failed to write file: {}", source_file_path.display()))?;

    // Preserve file permissions (Unix only)
    #[cfg(unix)]
    {
        let metadata = fs::metadata(file_abs.as_path()).with_context(|| {
            format!("Failed to read metadata: {}", file_abs.as_path().display())
        })?;
        let perms = metadata.permissions();
        fs::set_permissions(&source_file_path, perms).with_context(|| {
            format!("Failed to set permissions: {}", source_file_path.display())
        })?;
    }

    // Determine file attributes for display
    let metadata = fs::metadata(file_abs.as_path())?;

    #[cfg(unix)]
    let (is_private, is_executable) = {
        use std::os::unix::fs::PermissionsExt;
        let mode = metadata.permissions().mode() & 0o777;
        let is_private = mode == 0o600 || mode == 0o700;
        let is_executable = (mode & 0o100) != 0;
        (is_private, is_executable)
    };

    #[cfg(not(unix))]
    let (is_private, is_executable) = (false, false);

    // Print with appropriate color based on file type
    let target_str = rel_path.to_string();
    if params.encrypt {
        println!("  → {}", target_str.bright_magenta());
    } else if is_template {
        println!("  → {}", target_str.bright_blue());
    } else if is_executable {
        println!("  → {}", target_str.bright_yellow());
    } else if is_private {
        println!("  → {}", target_str.bright_red());
    } else {
        println!("  → {}", target_str.bright_green());
    }

    Ok(())
}

/// Add a directory recursively to the source directory
fn add_directory(
    params: &AddParams,
    dir_abs: &AbsPath,
    rel_path: &guisu_core::path::RelPath,
) -> Result<usize> {
    // Create the directory in source (with root_entry if configured)
    let source_dir_path = params
        .source_dir
        .as_path()
        .join(&params.config.general.root_entry)
        .join(rel_path.as_path());
    fs::create_dir_all(&source_dir_path)
        .with_context(|| format!("Failed to create directory: {}", source_dir_path.display()))?;

    println!("  → {}", rel_path.to_string().bright_cyan());

    let mut count = 0;

    // Walk the directory and add all files
    for entry in WalkDir::new(dir_abs.as_path())
        .follow_links(false)
        .into_iter()
    {
        let entry = entry.with_context(|| {
            format!("Failed to read directory: {}", dir_abs.as_path().display())
        })?;
        let entry_path = entry.path();

        // Skip the root directory itself
        if entry_path == dir_abs.as_path() {
            continue;
        }

        // Get the entry as an absolute path
        let entry_abs = AbsPath::new(entry_path.to_path_buf())?;
        let entry_rel = entry_abs.strip_prefix(params.dest_dir)?;

        if entry.file_type().is_dir() {
            // Create directory in source (with root_entry)
            let source_subdir = params
                .source_dir
                .as_path()
                .join(&params.config.general.root_entry)
                .join(entry_rel.as_path());
            fs::create_dir_all(&source_subdir).with_context(|| {
                format!("Failed to create directory: {}", source_subdir.display())
            })?;
            println!("  → {}", entry_rel.to_string().bright_cyan());
        } else if entry.file_type().is_symlink() {
            add_symlink(
                params.source_dir,
                &entry_rel,
                &entry_abs,
                params.force,
                params.config,
            )?;
            count += 1;
        } else {
            add_regular_file(params, &entry_rel, &entry_abs)?;
            count += 1;
        }
    }

    Ok(count)
}

/// Add a symlink to the source directory
fn add_symlink(
    source_dir: &AbsPath,
    rel_path: &guisu_core::path::RelPath,
    link_abs: &AbsPath,
    force: bool,
    config: &Config,
) -> Result<()> {
    // Read the symlink target
    let link_target = fs::read_link(link_abs.as_path())
        .with_context(|| format!("Failed to read symlink: {}", link_abs.as_path().display()))?;

    // Apply root_entry
    let source_link_path = source_dir
        .as_path()
        .join(&config.general.root_entry)
        .join(rel_path.as_path());

    // Check if symlink already exists in source (in any form)
    if let Some(existing_file) = check_file_exists_in_source(source_dir, rel_path, config)? {
        if !force {
            anyhow::bail!(
                "This symlink is already managed by guisu:\n  {}\n\n\
                 To re-add, use: guisu add --force",
                existing_file.display()
            );
        } else {
            // Force is true - remove the existing symlink to overwrite it
            fs::remove_file(&existing_file).with_context(|| {
                format!("Failed to remove old symlink: {}", existing_file.display())
            })?;
        }
    }

    // Create parent directory if needed
    if let Some(parent) = source_link_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    // Create the symlink in source directory
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink(&link_target, &source_link_path)
            .with_context(|| format!("Failed to create symlink: {}", source_link_path.display()))?;
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::symlink_file;
        symlink_file(&link_target, &source_link_path)
            .with_context(|| format!("Failed to create symlink: {}", source_link_path.display()))?;
    }

    println!(
        "  → {} → {}",
        rel_path.to_string().bright_magenta(),
        link_target.display().to_string().dimmed()
    );

    Ok(())
}

/// Validate encryption configuration without actually encrypting
///
/// This allows us to fail fast before modifying any files
fn validate_encryption_config(config: &Config) -> Result<()> {
    // Try to get recipients from config first (for team collaboration)
    match config.age_recipients()? {
        Some(_recipients) => {
            // Recipients configured, all good
            Ok(())
        }
        None => {
            // No recipients configured - check if symmetric mode is enabled
            if !config.age.derive {
                anyhow::bail!(
                    "No recipients configured for encryption.\n\
                     \n\
                     You must either:\n\
                     \n\
                     1. Enable symmetric mode (auto-derive public key from identity):\n\
                        [age]\n\
                        identity = \"~/.config/guisu/key.txt\"\n\
                        symmetric = true\n\
                     \n\
                     2. Specify explicit recipients:\n\
                        [age]\n\
                        identity = \"~/.config/guisu/key.txt\"\n\
                        recipients = [\n\
                            \"age1ql3z7hjy54pw3hyww5ayyfg7zqgvc7w3j2elw8zmrj2kg5sfn9aqmcac8p\",  # Your public key\n\
                        ]\n\
                     \n\
                     3. For team collaboration:\n\
                        [age]\n\
                        identities = [\"~/.config/guisu/key.txt\"]\n\
                        recipients = [\n\
                            \"age1ql3z...\",  # Alice\n\
                            \"age1zvk...\",  # Bob\n\
                        ]\n\
                     \n\
                     Generate age key with: guisu age generate\n\
                     Get your public key with: guisu age show"
                );
            }

            // Symmetric mode enabled - verify identities can be loaded
            config.age_identities().context(
                "Symmetric mode enabled but no identity configured.\n\
                 \n\
                 Add to your config file:\n\
                 [age]\n\
                 identity = \"~/.config/guisu/key.txt\"\n\
                 symmetric = true\n\
                 \n\
                 Generate age key with: guisu age generate",
            )?;

            Ok(())
        }
    }
}

/// Encrypt content using age
fn encrypt_content(content: &[u8], config: &Config) -> Result<Vec<u8>> {
    use guisu_crypto::encrypt;

    // Try to get recipients from config first (for team collaboration)
    let recipients = match config.age_recipients()? {
        Some(recipients) => {
            // Use configured recipients
            recipients
        }
        None => {
            // No recipients configured - check if symmetric mode is enabled
            if !config.age.derive {
                anyhow::bail!(
                    "No recipients configured for encryption.\n\
                     \n\
                     You must either:\n\
                     \n\
                     1. Enable symmetric mode (auto-derive public key from identity):\n\
                        [age]\n\
                        identity = \"~/.config/guisu/key.txt\"\n\
                        symmetric = true\n\
                     \n\
                     2. Specify explicit recipients:\n\
                        [age]\n\
                        identity = \"~/.config/guisu/key.txt\"\n\
                        recipients = [\n\
                            \"age1ql3z7hjy54pw3hyww5ayyfg7zqgvc7w3j2elw8zmrj2kg5sfn9aqmcac8p\",  # Your public key\n\
                        ]\n\
                     \n\
                     3. For team collaboration:\n\
                        [age]\n\
                        identities = [\"~/.config/guisu/key.txt\"]\n\
                        recipients = [\n\
                            \"age1ql3z...\",  # Alice\n\
                            \"age1zvk...\",  # Bob\n\
                        ]\n\
                     \n\
                     Generate age key with: guisu age generate\n\
                     Get your public key with: guisu age show"
                );
            }

            // Symmetric mode enabled - derive recipients from all identities
            // This ensures that if the identity file contains multiple keys (e.g., for team
            // collaboration or key rotation), all of them can decrypt the encrypted file.
            let identities = config.age_identities().context(
                "Symmetric mode enabled but no identity configured.\n\
                 \n\
                 Add to your config file:\n\
                 [age]\n\
                 identity = \"~/.config/guisu/key.txt\"\n\
                 symmetric = true\n\
                 \n\
                 Generate age key with: guisu age generate",
            )?;

            identities
                .iter()
                .map(|identity| identity.to_public())
                .collect()
        }
    };

    // Encrypt the content with all recipients
    encrypt(content, &recipients).context("Failed to encrypt content")
}

/// Cached secret detection regex patterns
static SECRET_PATTERNS: Lazy<Vec<(regex::Regex, &'static str)>> = Lazy::new(|| {
    vec![
        (
            regex::Regex::new(r#"(?i)(password|passwd|pwd)\s*[:=]\s*['"]?[^\s'"]{3,}"#)
                .expect("Valid regex"),
            "Password",
        ),
        (
            regex::Regex::new(r#"(?i)(api[_-]?key|apikey)\s*[:=]\s*['"]?[^\s'"]{8,}"#)
                .expect("Valid regex"),
            "API Key",
        ),
        (
            regex::Regex::new(r#"(?i)(secret[_-]?key|secret)\s*[:=]\s*['"]?[^\s'"]{8,}"#)
                .expect("Valid regex"),
            "Secret Key",
        ),
        (
            regex::Regex::new(r#"(?i)(access[_-]?token|token)\s*[:=]\s*['"]?[^\s'"]{8,}"#)
                .expect("Valid regex"),
            "Access Token",
        ),
        (
            regex::Regex::new(r#"(?i)(auth[_-]?token|bearer)\s*[:=]\s*['"]?[^\s'"]{8,}"#)
                .expect("Valid regex"),
            "Auth Token",
        ),
        (
            regex::Regex::new(r#"(?i)(client[_-]?secret)\s*[:=]\s*['"]?[^\s'"]{8,}"#)
                .expect("Valid regex"),
            "Client Secret",
        ),
        (
            regex::Regex::new(r"(?i)(private[_-]?key)\s*[:=]").expect("Valid regex"),
            "Private Key",
        ),
        (
            regex::Regex::new(r"-----BEGIN (RSA |DSA |EC )?PRIVATE KEY-----").expect("Valid regex"),
            "PEM Private Key",
        ),
        (
            regex::Regex::new(r#"(?i)(aws[_-]?access[_-]?key[_-]?id)\s*[:=]\s*['"]?[A-Z0-9]{20}"#)
                .expect("Valid regex"),
            "AWS Access Key",
        ),
        (
            regex::Regex::new(
                r#"(?i)(aws[_-]?secret[_-]?access[_-]?key)\s*[:=]\s*['"]?[A-Za-z0-9/+=]{40}"#,
            )
            .expect("Valid regex"),
            "AWS Secret Key",
        ),
    ]
});

/// Cached high-entropy pattern for token detection
static HIGH_ENTROPY_PATTERN: Lazy<regex::Regex> =
    Lazy::new(|| regex::Regex::new(r"[A-Za-z0-9+/=]{32,}").expect("Valid regex"));

/// Detect potential secrets in a file
///
/// Returns Some(findings) if secrets are detected, None otherwise
fn detect_secrets(file_path: &Path, content: &[u8]) -> Result<Option<String>> {
    let mut findings = Vec::new();

    // 1. Check filename for known private key patterns
    if let Some(filename) = file_path.file_name().and_then(|n| n.to_str()) {
        let private_key_patterns = [
            "id_rsa",
            "id_dsa",
            "id_ecdsa",
            "id_ed25519",
            ".pem",
            ".key",
            ".p12",
            ".pfx",
            "private-key",
            "privatekey",
        ];

        for pattern in &private_key_patterns {
            if filename.contains(pattern) {
                findings.push(format!("  • Filename contains '{}'", pattern));
                break;
            }
        }
    }

    // 2. Check content for secret patterns (only for text files)
    if !content.iter().take(8000).any(|&b| b == 0)
        && let Ok(text) = String::from_utf8(content.to_vec())
    {
        // Check against cached secret patterns
        for (re, description) in SECRET_PATTERNS.iter() {
            if re.is_match(&text) {
                findings.push(format!("  • Contains {}", description));
            }
        }

        // 3. Check for high-entropy strings (potential tokens)
        // Look for long alphanumeric strings that might be tokens
        for cap in HIGH_ENTROPY_PATTERN.find_iter(&text).take(5) {
            let s = cap.as_str();
            if calculate_entropy(s) > 4.5 {
                // Safe string slicing using char-based approach to avoid UTF-8 boundary issues
                let preview: String = s.chars().take(32).collect();
                findings.push(format!(
                    "  • High-entropy string (potential token): {}...",
                    preview
                ));
                break; // Only report one to avoid spam
            }
        }
    }

    if findings.is_empty() {
        Ok(None)
    } else {
        Ok(Some(findings.join("\n")))
    }
}

/// Calculate Shannon entropy of a string
fn calculate_entropy(s: &str) -> f64 {
    // Empty string has zero entropy
    if s.is_empty() {
        return 0.0;
    }

    let mut char_counts = indexmap::IndexMap::new();
    for c in s.chars() {
        *char_counts.entry(c).or_insert(0) += 1;
    }

    // Single unique character has zero entropy
    if char_counts.len() == 1 {
        return 0.0;
    }

    let len = s.len() as f64;
    let mut entropy = 0.0;

    for count in char_counts.values() {
        let probability = *count as f64 / len;
        // Guard against log2(0) which would produce -inf
        if probability > 0.0 {
            entropy -= probability * probability.log2();
        }
    }

    // Ensure non-negative result (entropy is always >= 0)
    entropy.max(0.0)
}

/// Auto-detect template variables in content and replace them
///
/// Returns (templated_content, has_replacements)
fn auto_template_content(content: &[u8], config: &Config) -> Result<(Vec<u8>, bool)> {
    // Only process text files
    if content.iter().take(8000).any(|&b| b == 0) {
        // Binary file, don't template
        return Ok((content.to_vec(), false));
    }

    let text = String::from_utf8_lossy(content);

    // Convert config.variables (IndexMap) to serde_json::Value for processing
    let variables_value =
        serde_json::to_value(&config.variables).context("Failed to convert variables to JSON")?;

    // Extract all variables from config with their paths
    let mut variables = extract_variables(&variables_value, "");

    // Sort by priority: longer values first, then shallower paths, then alphabetically
    variables.sort_by(|a, b| {
        // First priority: longer value
        let len_cmp = b.value.len().cmp(&a.value.len());
        if len_cmp != std::cmp::Ordering::Equal {
            return len_cmp;
        }

        // Second priority: shallower path (fewer dots)
        let depth_a = a.path.matches('.').count();
        let depth_b = b.path.matches('.').count();
        let depth_cmp = depth_a.cmp(&depth_b);
        if depth_cmp != std::cmp::Ordering::Equal {
            return depth_cmp;
        }

        // Third priority: alphabetical
        a.path.cmp(&b.path)
    });

    // Collect all replacements with their positions
    // Positions are stored as (start, end, replacement_string)
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();

    for var in variables {
        // Skip very short values to avoid false matches
        if var.value.len() < 3 {
            continue;
        }

        // Find all matches in the text (avoiding already-replaced regions)
        let mut pos = 0;
        while let Some(idx) = text[pos..].find(&var.value) {
            let start = pos + idx;
            let end = start + var.value.len();

            // Check if this region overlaps with any existing replacement
            let overlaps = replacements.iter().any(|(r_start, r_end, _)| {
                (start >= *r_start && start < *r_end) || (end > *r_start && end <= *r_end)
            });

            if !overlaps {
                let template_var = format!("{{{{ {} }}}}", var.path);
                replacements.push((start, end, template_var));
            }

            pos = end;
        }
    }

    // Sort replacements by position (earlier positions first)
    replacements.sort_by_key(|(start, _, _)| *start);

    // Build result string in one pass if there are replacements
    let (result, has_replacements) = if replacements.is_empty() {
        (text.to_string(), false)
    } else {
        let mut result = String::with_capacity(text.len());
        let mut last_end = 0;

        for (start, end, replacement) in replacements {
            // Copy text between last replacement and this one
            result.push_str(&text[last_end..start]);
            // Add the replacement
            result.push_str(&replacement);
            last_end = end;
        }

        // Copy remaining text after last replacement
        result.push_str(&text[last_end..]);

        (result, true)
    };

    Ok((result.into_bytes(), has_replacements))
}

/// Variable with its path and value for autotemplate
#[derive(Debug)]
struct TemplateVariable {
    path: String,
    value: String,
}

/// Extract all variables from config with their full paths
fn extract_variables(value: &serde_json::Value, prefix: &str) -> Vec<TemplateVariable> {
    let mut variables = Vec::new();

    match value {
        serde_json::Value::Object(map) => {
            for (key, val) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", prefix, key)
                };

                if let serde_json::Value::String(s) = val {
                    // This is a leaf string value
                    variables.push(TemplateVariable {
                        path: path.clone(),
                        value: s.clone(),
                    });
                }

                // Recursively extract from nested objects
                variables.extend(extract_variables(val, &path));
            }
        }
        serde_json::Value::String(s) => {
            // Direct string value
            if !prefix.is_empty() {
                variables.push(TemplateVariable {
                    path: prefix.to_string(),
                    value: s.clone(),
                });
            }
        }
        _ => {
            // Ignore other types (numbers, booleans, arrays, null)
        }
    }

    variables
}

/// Check if a file with the given relative path already exists in source directory
///
/// This checks for all possible variants of the file:
/// - Without any extension (plain file)
/// - With .j2 extension (template)
/// - With .age extension (encrypted)
/// - With .j2.age extension (encrypted template)
///
/// Returns the path of the existing file if found, None otherwise.
fn check_file_exists_in_source(
    source_dir: &AbsPath,
    rel_path: &guisu_core::path::RelPath,
    config: &Config,
) -> Result<Option<PathBuf>> {
    let rel_str = rel_path.as_path().to_string_lossy();

    // All possible variants in order of checking
    let variants = [
        rel_str.to_string(),           // Plain file
        format!("{}.j2", rel_str),     // Template
        format!("{}.age", rel_str),    // Encrypted
        format!("{}.j2.age", rel_str), // Encrypted template
    ];

    for variant in &variants {
        let source_file_path = source_dir
            .as_path()
            .join(&config.general.root_entry)
            .join(variant);

        if source_file_path.exists() {
            return Ok(Some(source_file_path));
        }
    }

    Ok(None)
}
