//! Age encryption identity management
//!
//! Commands for generating and showing age identities.

use anyhow::{Context, Result};
use guisu_crypto::{Identity, IdentityFile};
use owo_colors::OwoColorize;
use std::path::PathBuf;

use guisu_config::Config;

/// Generate a new age identity
pub fn generate(output: Option<PathBuf>) -> Result<()> {
    let identity = Identity::generate();
    let public_key = identity.to_public();

    let output_path = match output {
        Some(path) => path,
        None => guisu_config::dirs::default_age_identity().ok_or_else(|| {
            anyhow::anyhow!(
                "Could not determine config directory. Please specify output path with --output."
            )
        })?,
    };

    // Ensure parent directory exists
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    // Save the identity
    IdentityFile::save(&output_path, &[identity]).context("Failed to save identity file")?;

    println!("Generated new age identity");
    println!("Private key saved to: {}", output_path.display());
    println!("\nPublic key: {}", public_key);
    println!("\nKeep your private key secure!");
    println!("Share your public key with others to allow them to encrypt files for you.");

    Ok(())
}

/// Show the public key for the current identity
pub fn show(config: &Config) -> Result<()> {
    let identities = config.age_identities()?;

    println!("{}", "Age Identities".bright_white().bold());
    println!();

    if identities.is_empty() {
        println!(
            "  {} {:14} {}",
            "✗".bright_red(),
            "Status",
            "No identities configured".dimmed()
        );
        return Ok(());
    }

    // Display identity files
    if let Some(ref identity_path) = config.age.identity {
        print_item("Identity", &identity_path.display().to_string(), true);
    }
    if let Some(ref identity_paths) = config.age.identities {
        for (i, path) in identity_paths.iter().enumerate() {
            let label = if i == 0 && config.age.identity.is_none() {
                "Identity"
            } else {
                ""
            };
            print_item(label, &path.display().to_string(), true);
        }
    }

    println!();
    println!("{}", "Public Keys".bright_white().bold());
    println!();

    // Display public keys
    for (i, identity) in identities.iter().enumerate() {
        let label = if i == 0 {
            if identities.len() == 1 {
                "Public key"
            } else {
                "Public keys"
            }
        } else {
            ""
        };
        print_item(label, &identity.to_public().to_string(), true);
    }

    Ok(())
}

/// Print a formatted item with status indicator
fn print_item(label: &str, value: &str, ok: bool) {
    let symbol = if ok {
        "✓".bright_green().to_string()
    } else {
        "✗".bright_red().to_string()
    };

    let formatted_value = if ok {
        value.bright_white().to_string()
    } else {
        value.dimmed().to_string()
    };

    println!("  {} {:14} {}", symbol, label, formatted_value);
}

/// Encrypt a value using inline encryption format
///
/// This encrypts a plaintext value and outputs it in the compact `age:base64...` format
/// suitable for embedding in configuration files.
pub fn encrypt(
    value: Option<String>,
    interactive: bool,
    recipient_strs: Vec<String>,
    config: &Config,
) -> Result<()> {
    use guisu_crypto::{Recipient, encrypt_inline};
    use std::io::{self, Write};

    // Determine recipients
    let recipients = if !recipient_strs.is_empty() {
        // Use explicitly specified recipients
        recipient_strs
            .iter()
            .map(|s| s.parse::<Recipient>())
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to parse recipient")?
    } else {
        // No recipients specified, derive from all configured identities
        let identities = config.age_identities()?;

        // Convert all identities to recipients
        identities.iter().map(|id| id.to_public()).collect()
    };

    // Get the value to encrypt
    let plaintext = match value {
        Some(v) if !interactive => v,
        _ => {
            // Interactive mode or no value provided: read from stdin
            if interactive {
                print!("Enter value to encrypt: ");
                io::stdout().flush()?;
            }

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            input.trim().to_string()
        }
    };

    if plaintext.is_empty() {
        anyhow::bail!("No value provided to encrypt");
    }

    // Encrypt the value with all recipients
    let encrypted = encrypt_inline(&plaintext, &recipients).context("Failed to encrypt value")?;

    // Output the encrypted value
    println!("{}", encrypted);

    if interactive {
        println!();
        println!("Copy the above encrypted value into your configuration file.");
        println!("Use {{ encrypted_value | decrypt }} in templates to decrypt it.");
    }

    Ok(())
}

/// Decrypt an inline encrypted value
///
/// This decrypts a value in the `age:base64...` format and outputs the plaintext.
pub fn decrypt(value: String, config: &Config) -> Result<()> {
    use guisu_crypto::decrypt_inline;

    // Load all configured identities
    let identities = config.age_identities()?;

    // Decrypt the value
    let plaintext = decrypt_inline(&value, &identities).context("Failed to decrypt value")?;

    // Output the plaintext
    println!("{}", plaintext);

    Ok(())
}

/// Migrate encrypted files from old keys to new keys
///
/// Re-encrypts all encrypted files (.age) and inline encrypted values in the source directory
pub fn migrate(
    source_dir: &std::path::Path,
    old_identity_paths: &[PathBuf],
    new_identity_paths: &[PathBuf],
    dry_run: bool,
    yes: bool,
) -> Result<()> {
    use guisu_crypto::load_identities;
    use owo_colors::OwoColorize;
    use std::io::{self, Write};
    use walkdir::WalkDir;

    println!("{}", "Age Key Migration".bold().cyan());
    println!();

    // Load old identities
    println!("{}", "Loading old identities...".dimmed());
    let mut old_identities = Vec::new();
    for path in old_identity_paths {
        let is_ssh = guisu_config::Config::is_ssh_identity(path);
        let ids = load_identities(path, is_ssh)
            .with_context(|| format!("Failed to load old identity: {}", path.display()))?;
        println!("  ✓ {}", path.display());
        old_identities.extend(ids);
    }
    println!();

    // Load new identities and extract recipients
    println!("{}", "Loading new identities...".dimmed());
    let mut new_recipients = Vec::new();
    for path in new_identity_paths {
        let is_ssh = guisu_config::Config::is_ssh_identity(path);
        let ids = load_identities(path, is_ssh)
            .with_context(|| format!("Failed to load new identity: {}", path.display()))?;
        println!("  ✓ {}", path.display());
        for identity in &ids {
            new_recipients.push(identity.to_public());
        }
    }
    println!();

    // Scan source directory for encrypted files
    println!("{}", "Scanning for encrypted files...".dimmed());

    let mut encrypted_files = Vec::new(); // .age files
    let mut inline_files = Vec::new(); // Files with inline age: encryption

    // Compile regex once outside the loop
    let inline_pattern = regex::Regex::new(r"age:[A-Za-z0-9+/]+=*").unwrap();

    for entry in WalkDir::new(source_dir).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();

        // Check for .age files
        if path.extension().and_then(|s| s.to_str()) == Some("age") {
            encrypted_files.push(path.to_path_buf());
            continue;
        }

        // Check for inline encrypted content
        if let Ok(content) = std::fs::read_to_string(path)
            && content.contains("age:")
            && inline_pattern.is_match(&content)
        {
            inline_files.push(path.to_path_buf());
        }
    }

    let total_files = encrypted_files.len() + inline_files.len();

    if total_files == 0 {
        println!(
            "{}",
            "No encrypted files found in source directory.".yellow()
        );
        return Ok(());
    }

    // Display summary
    println!();
    println!("{}", "Migration Summary:".bold());
    println!("  Encrypted files (.age):  {}", encrypted_files.len());
    println!("  Files with inline encryption: {}", inline_files.len());
    println!("  Total files to migrate:  {}", total_files);
    println!();

    // Show files if dry-run
    if dry_run || !yes {
        println!("{}", "Files to be migrated:".bold());

        if !encrypted_files.is_empty() {
            println!("\n{}", "Encrypted files (.age):".cyan());
            for file in &encrypted_files {
                let relative = file.strip_prefix(source_dir).unwrap_or(file);
                println!("  • {}", relative.display());
            }
        }

        if !inline_files.is_empty() {
            println!("\n{}", "Files with inline encryption:".cyan());
            for file in &inline_files {
                let relative = file.strip_prefix(source_dir).unwrap_or(file);
                println!("  • {}", relative.display());
            }
        }
        println!();
    }

    if dry_run {
        println!("{}", "Dry run - no files were modified.".yellow().bold());
        return Ok(());
    }

    // Confirmation prompt
    if !yes {
        print!(
            "{} {}",
            "Proceed with migration?".yellow().bold(),
            "(yes/no):".dimmed()
        );
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let answer = input.trim().to_lowercase();

        if answer != "yes" && answer != "y" {
            println!("{}", "Migration cancelled.".yellow());
            return Ok(());
        }
        println!();
    }

    // Perform migration
    println!("{}", "Migrating files...".bold().cyan());

    let mut migrated_count = 0;
    let mut error_count = 0;

    // Migrate .age files
    for file in &encrypted_files {
        let relative = file.strip_prefix(source_dir).unwrap_or(file);
        print!("  Migrating {} ... ", relative.display());
        io::stdout().flush()?;

        match migrate_encrypted_file(file, &old_identities, &new_recipients) {
            Ok(()) => {
                println!("{}", "✓".green());
                migrated_count += 1;
            }
            Err(e) => {
                println!("{}", "✗".red());
                eprintln!("    Error: {}", e);
                error_count += 1;
            }
        }
    }

    // Migrate files with inline encryption
    for file in &inline_files {
        let relative = file.strip_prefix(source_dir).unwrap_or(file);
        print!("  Migrating {} ... ", relative.display());
        io::stdout().flush()?;

        match migrate_inline_file(file, &old_identities, &new_recipients) {
            Ok(()) => {
                println!("{}", "✓".green());
                migrated_count += 1;
            }
            Err(e) => {
                println!("{}", "✗".red());
                eprintln!("    Error: {}", e);
                error_count += 1;
            }
        }
    }

    // Summary
    println!();
    if error_count == 0 {
        println!(
            "{} Successfully migrated {} files.",
            "✓".green().bold(),
            migrated_count
        );
    } else {
        println!(
            "{} Migrated {} files with {} errors.",
            "!".yellow().bold(),
            migrated_count,
            error_count
        );
    }

    Ok(())
}

/// Migrate a single .age encrypted file
fn migrate_encrypted_file(
    file_path: &std::path::Path,
    old_identities: &[guisu_crypto::Identity],
    new_recipients: &[guisu_crypto::Recipient],
) -> Result<()> {
    use guisu_crypto::{decrypt, encrypt};

    // Read and decrypt with old key
    let encrypted_content = std::fs::read(file_path)?;
    let decrypted = decrypt(&encrypted_content, old_identities)
        .context("Failed to decrypt with old identities")?;

    // Re-encrypt with new key
    let re_encrypted =
        encrypt(&decrypted, new_recipients).context("Failed to encrypt with new recipients")?;

    // Write back
    std::fs::write(file_path, re_encrypted)
        .with_context(|| format!("Failed to write file: {}", file_path.display()))?;

    Ok(())
}

/// Migrate a file with inline encrypted values
fn migrate_inline_file(
    file_path: &std::path::Path,
    old_identities: &[guisu_crypto::Identity],
    new_recipients: &[guisu_crypto::Recipient],
) -> Result<()> {
    use guisu_crypto::encrypt_file_content;

    // Read file content
    let content = std::fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read file: {}", file_path.display()))?;

    // Re-encrypt inline values
    let re_encrypted = encrypt_file_content(&content, old_identities, new_recipients)
        .context("Failed to re-encrypt inline values")?;

    // Write back
    std::fs::write(file_path, re_encrypted)
        .with_context(|| format!("Failed to write file: {}", file_path.display()))?;

    Ok(())
}
