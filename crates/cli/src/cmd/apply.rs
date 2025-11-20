//! Apply command implementation
//!
//! Apply the source state to the destination directory.

use anyhow::{Context, Result};
use clap::Args;
use guisu_core::path::AbsPath;
use guisu_engine::entry::TargetEntry;
use guisu_engine::processor::ContentProcessor;
use guisu_engine::state::{SourceState, TargetState};
use owo_colors::OwoColorize;
use rayon::prelude::*;
use std::fs;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::Arc;
use subtle::ConstantTimeEq;
use tracing::{debug, info, warn};

use crate::cmd::conflict::{ChangeType, ConflictHandler};
use crate::command::Command;
use crate::common::RuntimeContext;
use crate::stats::ApplyStats;
use crate::ui::ConflictAction;
use crate::ui::progress;

/// Apply the source state to the destination
#[derive(Debug, Clone, Args)]
pub struct ApplyCommand {
    /// Specific files to apply (all if not specified)
    #[arg(value_name = "FILES")]
    pub files: Vec<PathBuf>,

    /// Dry run - show what would be done
    #[arg(short = 'n', long)]
    pub dry_run: bool,

    /// Force overwrite of changed files
    #[arg(short, long)]
    pub force: bool,

    /// Interactive mode - prompt on conflicts
    #[arg(short, long)]
    pub interactive: bool,

    /// Include only these entry types (comma-separated)
    #[arg(long, value_delimiter = ',')]
    pub include: Vec<String>,

    /// Exclude these entry types (comma-separated)
    #[arg(long, value_delimiter = ',')]
    pub exclude: Vec<String>,
}

/// Get the last written content hash for an entry from the database
///
/// Returns the content hash if the entry is a file and has state in the database.
/// Returns None for non-file entries or if no state exists.
fn get_last_written_hash(entry: &TargetEntry) -> Option<Vec<u8>> {
    match entry {
        TargetEntry::File { .. } => {
            let path_str = entry.path().to_string();
            guisu_engine::database::get_entry_state(&path_str)
                .ok()
                .flatten()
                .map(|state| state.content_hash)
        }
        _ => None,
    }
}

impl Command for ApplyCommand {
    type Output = ApplyStats;
    fn execute(&self, context: &RuntimeContext) -> crate::error::Result<ApplyStats> {
        // Parse entry type filters
        let include_types: Result<Vec<EntryType>> =
            self.include.iter().map(|s| s.parse()).collect();
        let _include_types = include_types?;

        let exclude_types: Result<Vec<EntryType>> =
            self.exclude.iter().map(|s| s.parse()).collect();
        let _exclude_types = exclude_types?;

        // Extract paths and config from context
        let source_abs = context.dotfiles_dir();
        let dest_abs = context.dest_dir();
        let source_dir = context.source_dir();
        let config = &context.config;

        if self.dry_run {
            info!("{}", "Dry run mode - no changes will be made".dimmed());
        }

        // Load age identities for decryption
        let spinner = progress::create_spinner("Loading identities...");
        let identities = std::sync::Arc::new(config.age_identities().unwrap_or_default());
        spinner.finish_and_clear();

        // Detect if output is to a terminal for icon auto mode
        let is_tty = std::io::stdout().is_terminal();
        let show_icons = config.ui.icons.should_show_icons(is_tty);

        // Load variables from .guisu/variables/ directory
        let guisu_dir = source_dir.join(".guisu");
        let platform_name = guisu_core::platform::CURRENT_PLATFORM.os;

        let guisu_variables = if guisu_dir.exists() {
            guisu_config::variables::load_variables(&guisu_dir, platform_name)
                .context("Failed to load variables from .guisu/variables/")?
        } else {
            indexmap::IndexMap::new()
        };

        // Merge variables: guisu variables + config variables (config overrides)
        let mut all_variables = guisu_variables;
        all_variables.extend(config.variables.iter().map(|(k, v)| (k.clone(), v.clone())));

        // Create template engine with identities, template directory, and bitwarden provider
        let template_engine =
            crate::create_template_engine(source_dir, Arc::clone(&identities), config);

        // Create content processor with real decryptor and renderer
        use guisu_engine::adapters::crypto::CryptoDecryptorAdapter;
        use guisu_engine::adapters::template::TemplateRendererAdapter;

        // Use Arc to share identity without cloning
        let identity_arc = if let Some(first) = identities.first() {
            // Share the first identity from the Arc<Vec>
            Arc::new(first.clone())
        } else {
            // Generate a new identity if none configured
            Arc::new(guisu_crypto::Identity::generate())
        };
        let decryptor = CryptoDecryptorAdapter::from_arc(identity_arc);
        let renderer = TemplateRendererAdapter::new(template_engine);
        let processor = ContentProcessor::new(decryptor, renderer);

        // Load metadata for create-once tracking
        let metadata =
            guisu_engine::state::Metadata::load(source_dir).context("Failed to load metadata")?;

        // Create ignore matcher from .guisu/ignores.toml
        // Use dotfiles_dir as the match root so patterns match relative to the dotfiles directory
        let ignore_matcher = guisu_config::IgnoreMatcher::from_ignores_toml(source_dir)
            .context("Failed to load ignore patterns from .guisu/ignores.toml")?;

        // Check if we're applying a single file (affects output verbosity)
        let is_single_file = !self.files.is_empty() && self.files.len() == 1;

        // Build filter paths if specific files requested
        let filter_paths = if !self.files.is_empty() {
            Some(crate::build_filter_paths(&self.files, dest_abs)?)
        } else {
            None
        };

        // Read source state with ignore matcher from config
        let spinner = if !is_single_file {
            Some(progress::create_spinner("Reading source state..."))
        } else {
            None
        };

        // Load ignore matcher if .guisu/ignores.toml exists
        let matcher = guisu_config::IgnoreMatcher::from_ignores_toml(source_dir).ok();

        // Read source state with optional ignore filtering
        let source_state = if let Some(ref matcher) = matcher {
            SourceState::read_with_matcher(source_abs.to_owned(), Some(matcher))
                .context("Failed to read source state with ignore matcher")?
        } else {
            SourceState::read(source_abs.to_owned()).context("Failed to read source state")?
        };

        if let Some(spinner) = spinner {
            spinner.finish_and_clear();
        }

        if source_state.is_empty() {
            if !is_single_file {
                info!("No files to apply");
            }
            return Ok(ApplyStats::new());
        }

        // Use the full source state - we'll filter later
        let filtered_source_state = source_state;

        // Build target state from filtered source state (processes templates and decrypts files)
        let spinner = if !is_single_file {
            Some(progress::create_spinner(
                "Processing templates and encrypted files...",
            ))
        } else {
            None
        };
        // Create template context with system variables and guisu info
        let working_tree = context.working_tree();
        let template_context = guisu_template::TemplateContext::new()
            .with_variables(all_variables)
            .with_guisu_info(
                source_abs.to_string(),
                working_tree.display().to_string(),
                dest_abs.to_string(),
                config.general.root_entry.display().to_string(),
            );
        let template_context_value = serde_json::to_value(&template_context)
            .context("Failed to serialize template context")?;
        let target_state =
            TargetState::from_source(&filtered_source_state, &processor, &template_context_value)?;
        if let Some(spinner) = spinner {
            spinner.finish_and_clear();
        }

        // Filter out create-once files that already exist at the destination
        // Also filter out ignored files
        // Also filter by specific file paths if requested
        let mut entries_to_apply: Vec<&TargetEntry> = target_state
            .entries()
            .filter(|entry| {
                let path_str = entry.path().to_string();
                let target_path = entry.path();

                // If filtering by specific files, skip entries not in the filter
                if let Some(ref filter) = filter_paths
                    && !filter.iter().any(|p| p == target_path)
                {
                    return false;
                }

                // Skip if file is ignored
                if ignore_matcher.is_ignored(entry.path().as_path()) {
                    debug!(
                        path = %path_str,
                        "Skipping ignored file"
                    );
                    return false;
                }

                // If file is marked as create-once and already exists, skip it
                if metadata.is_create_once(&path_str) {
                    let dest_path = dest_abs.join(entry.path());
                    if dest_path.as_path().exists() {
                        debug!(
                            path = %path_str,
                            "Skipping create-once file that already exists"
                        );
                        return false;
                    }
                }

                true
            })
            .collect();

        // Sort entries by path for consistent output
        entries_to_apply.sort_by(|a, b| a.path().as_path().cmp(b.path().as_path()));

        if entries_to_apply.is_empty() {
            info!("No matching files to apply");
            return Ok(ApplyStats::new());
        }

        // Open database for state tracking (only if not dry run)
        if !self.dry_run {
            guisu_engine::database::open_db().context("Failed to open state database")?;
        }

        // Check for configuration drift (files modified by user AND source updated)
        if !self.dry_run && !is_single_file {
            let drift_warnings = detect_config_drift(&entries_to_apply, dest_abs)?;
            if !drift_warnings.is_empty() {
                println!("\n{}", "Configuration Drift Detected".yellow().bold());
                println!(
                    "{}",
                    "The following files have been modified both locally and in the source:"
                        .yellow()
                );
                for warning in &drift_warnings {
                    println!("  {} {}", "•".yellow(), warning.bright_white());
                }
                println!();
                println!(
                    "{}",
                    "These local changes will be overwritten during apply.".yellow()
                );
                println!(
                    "{}",
                    "Consider backing up modified files or using interactive mode (-i) for control."
                        .dimmed()
                );
                println!();
            }
        }

        // Create conflict handler for interactive mode
        let mut conflict_handler = if self.interactive && !self.dry_run {
            Some(ConflictHandler::new(
                Arc::clone(config),
                Arc::clone(&identities),
            ))
        } else {
            None
        };

        // Apply entries
        let stats = Arc::new(ApplyStats::new());

        // Use parallel processing only when NOT in interactive mode
        if self.interactive || self.dry_run {
            // Sequential processing for interactive mode or dry run
            for entry in entries_to_apply {
                let dest_path = dest_abs.join(entry.path());

                if self.dry_run {
                    // Skip if file doesn't need update
                    if !needs_update(entry, &dest_path, &identities) {
                        debug!(path = %entry.path(), "File is already up to date, skipping");
                        continue;
                    }

                    debug!(path = %entry.path(), "Would apply entry");
                    print_dry_run_entry(entry, show_icons);
                    stats.record_dry_run(entry);
                } else {
                    // Check for conflicts if interactive mode is enabled
                    let should_apply = if let Some(ref mut handler) = conflict_handler {
                        // Load last written state from database
                        let last_written_hash = get_last_written_hash(entry);

                        // Detect type of change
                        let change_type = ConflictHandler::detect_change_type(
                            entry,
                            dest_abs,
                            last_written_hash.as_deref(),
                            &identities,
                        )?;

                        if let Some(change_type) = change_type {
                            // Prompt user for action with change type information
                            // Note: We don't store full content in DB, only hash, so last_written_content is None
                            // This means merge operations will use two-way merge instead of three-way
                            match handler.prompt_action(entry, dest_abs, None, change_type)? {
                                ConflictAction::Override => true,
                                ConflictAction::Skip => {
                                    debug!(path = %entry.path(), "Skipping due to user choice");
                                    println!("  {} {}", "⏭".yellow(), entry.path().bright_white());
                                    false
                                }
                                ConflictAction::Quit => {
                                    info!("Apply operation cancelled by user");
                                    return Ok(ApplyStats::new());
                                }
                                // Merge and Edit are handled internally by prompt_action and return Override when done
                                // Diff continues the prompt loop internally
                                _ => unreachable!("Unexpected action returned from prompt_action"),
                            }
                        } else {
                            // No change detected, but check if file actually needs update
                            needs_update(entry, &dest_path, &identities)
                        }
                    } else {
                        // Non-interactive mode: check for local modifications and warn user
                        if !needs_update(entry, &dest_path, &identities) {
                            false
                        } else {
                            // Load last written state to detect change type
                            let last_written_hash = get_last_written_hash(entry);

                            // Detect type of change
                            let change_type = ConflictHandler::detect_change_type(
                                entry,
                                dest_abs,
                                last_written_hash.as_deref(),
                                &identities,
                            )?;

                            // If local modification or true conflict detected, warn user
                            if let Some(change_type) = change_type {
                                match change_type {
                                    ChangeType::LocalModification | ChangeType::TrueConflict => {
                                        // Show warning
                                        let change_label = match change_type {
                                            ChangeType::LocalModification => "Local modification",
                                            ChangeType::TrueConflict => {
                                                "Conflict (both local and source modified)"
                                            }
                                            _ => unreachable!(),
                                        };

                                        println!(
                                            "\n{} {}",
                                            "⚠".yellow(),
                                            change_label.yellow().bold()
                                        );
                                        println!("  File: {}", entry.path().bright_white());
                                        println!(
                                            "  {}",
                                            "This file has been modified locally.".yellow()
                                        );
                                        println!(
                                            "  {}",
                                            "Applying will overwrite your local changes.".yellow()
                                        );

                                        // Ask for confirmation
                                        use dialoguer::{Confirm, theme::ColorfulTheme};
                                        let theme = ColorfulTheme::default();

                                        Confirm::with_theme(&theme)
                                            .with_prompt("Continue and overwrite local changes?")
                                            .default(false)
                                            .interact()
                                            .context("Failed to read user input")?
                                    }
                                    ChangeType::SourceUpdate => {
                                        // Source update only, safe to apply
                                        true
                                    }
                                }
                            } else {
                                // No change detected by change_type, but needs_update said yes
                                // This can happen for new files or permission changes
                                true
                            }
                        }
                    };

                    if should_apply {
                        match apply_target_entry(entry, &dest_path, &identities) {
                            Ok(_) => {
                                debug!(path = %entry.path(), "Applied entry successfully");

                                // Save state to database after successful application
                                // If state save fails, treat the entire operation as failed
                                match save_entry_state_to_db(entry) {
                                    Ok(_) => {
                                        print_success_entry(entry, show_icons);
                                        stats.record_success(entry);
                                    }
                                    Err(e) => {
                                        warn!(path = %entry.path(), error = %e, "Failed to save state to database");
                                        print_error_entry(entry, &e, show_icons);
                                        stats.record_failure();
                                    }
                                }
                            }
                            Err(e) => {
                                warn!(path = %entry.path(), error = %e, "Failed to apply entry");
                                print_error_entry(entry, &e, show_icons);
                                stats.record_failure();
                            }
                        }
                    }
                }
            }
        } else {
            // Parallel processing for non-interactive mode
            // First, pre-scan for local modifications and get confirmations
            use std::collections::HashSet;
            let mut confirmed_paths = HashSet::new();
            let mut has_warnings = false;

            for entry in &entries_to_apply {
                let dest_path = dest_abs.join(entry.path());

                // Skip if file doesn't need update
                if !needs_update(entry, &dest_path, &identities) {
                    continue;
                }

                // Load last written state to detect change type
                let last_written_hash = get_last_written_hash(entry);

                // Detect type of change
                if let Ok(Some(change_type)) = ConflictHandler::detect_change_type(
                    entry,
                    dest_abs,
                    last_written_hash.as_deref(),
                    &identities,
                ) {
                    match change_type {
                        ChangeType::LocalModification | ChangeType::TrueConflict => {
                            has_warnings = true;

                            // Show warning
                            let change_label = match change_type {
                                ChangeType::LocalModification => "Local modification",
                                ChangeType::TrueConflict => {
                                    "Conflict (both local and source modified)"
                                }
                                _ => unreachable!(),
                            };

                            println!("\n{} {}", "⚠".yellow(), change_label.yellow().bold());
                            println!("  File: {}", entry.path().bright_white());
                            println!("  {}", "This file has been modified locally.".yellow());
                            println!(
                                "  {}",
                                "Applying will overwrite your local changes.".yellow()
                            );

                            // Ask for confirmation
                            use dialoguer::{Confirm, theme::ColorfulTheme};
                            let theme = ColorfulTheme::default();

                            let confirmed = Confirm::with_theme(&theme)
                                .with_prompt("Continue and overwrite local changes?")
                                .default(false)
                                .interact()
                                .context("Failed to read user input")?;

                            if confirmed {
                                confirmed_paths.insert(entry.path().to_string());
                            }
                        }
                        ChangeType::SourceUpdate => {
                            // Source update only, safe to apply
                            confirmed_paths.insert(entry.path().to_string());
                        }
                    }
                } else {
                    // No change detected or error - apply by default
                    confirmed_paths.insert(entry.path().to_string());
                }
            }

            if has_warnings {
                println!();
            }

            // Now process confirmed files in parallel
            let results: Vec<Result<()>> = entries_to_apply
                .par_iter()
                .filter(|entry| confirmed_paths.contains(&entry.path().to_string()))
                .map(|entry| {
                    let dest_path = dest_abs.join(entry.path());

                    // Skip if file doesn't need update
                    if !needs_update(entry, &dest_path, &identities) {
                        debug!(path = %entry.path(), "File is already up to date, skipping");
                        return Ok(());
                    }

                    match apply_target_entry(entry, &dest_path, &identities) {
                        Ok(_) => {
                            debug!(path = %entry.path(), "Applied entry successfully");

                            // Save state to database after successful application
                            // If state save fails, treat the entire operation as failed
                            match save_entry_state_to_db(entry) {
                                Ok(_) => {
                                    print_success_entry(entry, show_icons);
                                    stats.record_success(entry);
                                    Ok(())
                                }
                                Err(e) => {
                                    warn!(path = %entry.path(), error = %e, "Failed to save state to database");
                                    print_error_entry(entry, &e, show_icons);
                                    stats.record_failure();
                                    Err(e)
                                }
                            }
                        }
                        Err(e) => {
                            warn!(path = %entry.path(), error = %e, "Failed to apply entry");
                            print_error_entry(entry, &e, show_icons);
                            stats.record_failure();
                            Err(e)
                        }
                    }
                })
                .collect();

            // Check for any errors in parallel execution
            for result in results {
                result?;
            }
        }

        // Return stats instead of printing here
        // The caller (lib.rs) will print the summary after hooks complete

        let failed_count = stats.failed();
        if failed_count > 0 {
            return Err(anyhow::anyhow!("Failed to apply {} entries", failed_count).into());
        }

        Ok(stats.snapshot())
    }
}

/// Entry type filter for apply command
#[derive(Debug, Clone, Copy, PartialEq)]
enum EntryType {
    Files,
    Dirs,
    Symlinks,
    Templates,
    Encrypted,
}

impl std::str::FromStr for EntryType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "files" | "file" => Ok(EntryType::Files),
            "dirs" | "dir" | "directories" => Ok(EntryType::Dirs),
            "symlinks" | "symlink" => Ok(EntryType::Symlinks),
            "templates" | "template" => Ok(EntryType::Templates),
            "encrypted" | "encrypt" => Ok(EntryType::Encrypted),
            _ => anyhow::bail!(
                "Invalid entry type: {}. Valid types: files, dirs, symlinks, templates, encrypted",
                s
            ),
        }
    }
}
/// Check if a target entry needs to be updated at the destination
///
/// Returns true if:
/// - The destination file/directory doesn't exist
/// - The content differs from the target
/// - The permissions differ from the target
///
/// NOTE: This function should NOT be used alone to determine if a file needs updating.
/// Use `detect_change_type` instead for proper three-way comparison.
/// This function is only called after `detect_change_type` returns None.
fn needs_update(
    entry: &TargetEntry,
    dest_path: &AbsPath,
    identities: &[guisu_crypto::Identity],
) -> bool {
    match entry {
        TargetEntry::File { content, mode, .. } => {
            // If file doesn't exist, it needs to be created
            if !dest_path.as_path().exists() {
                return true;
            }

            // Decrypt inline age values in target content before comparing
            // This matches the behavior in detect_change_type and apply_target_entry
            let target_content_decrypted =
                decrypt_inline_age_values(content, identities).unwrap_or_else(|_| content.to_vec());

            // Check if content differs
            if let Ok(existing_content) = fs::read(dest_path.as_path()) {
                if existing_content != target_content_decrypted {
                    return true;
                }
            } else {
                // Can't read file, assume it needs update
                return true;
            }

            // Check if permissions differ (Unix only)
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Some(target_mode) = mode
                    && let Ok(metadata) = fs::metadata(dest_path.as_path())
                {
                    let current_mode = metadata.permissions().mode() & 0o777;
                    if current_mode != *target_mode {
                        return true;
                    }
                }
            }

            // Content and permissions match, no update needed
            false
        }
        TargetEntry::Directory { mode, .. } => {
            // If directory doesn't exist, it needs to be created
            if !dest_path.as_path().exists() {
                return true;
            }

            // Check if it's actually a directory
            if !dest_path.as_path().is_dir() {
                return true;
            }

            // Check if permissions differ (Unix only)
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Some(target_mode) = mode
                    && let Ok(metadata) = fs::metadata(dest_path.as_path())
                {
                    let current_mode = metadata.permissions().mode() & 0o777;
                    if current_mode != *target_mode {
                        return true;
                    }
                }
            }

            // Directory exists with correct permissions
            false
        }
        TargetEntry::Symlink { target, .. } => {
            // If symlink doesn't exist, it needs to be created
            if !dest_path.as_path().exists() {
                return true;
            }

            // Check if it's actually a symlink
            if !dest_path.as_path().is_symlink() {
                return true;
            }

            // Check if symlink target differs
            if let Ok(existing_target) = fs::read_link(dest_path.as_path()) {
                if existing_target != target.as_path() {
                    return true;
                }
            } else {
                // Can't read symlink, assume it needs update
                return true;
            }

            // Symlink exists with correct target
            false
        }
        TargetEntry::Remove { .. } => {
            // Always needs update if file exists
            dest_path.as_path().exists()
        }
    }
}

/// Apply a single target entry to the destination
fn apply_target_entry(
    entry: &TargetEntry,
    dest_path: &AbsPath,
    identities: &[guisu_crypto::Identity],
) -> Result<()> {
    match entry {
        TargetEntry::File { content, mode, .. } => {
            // Ensure parent directory exists
            if let Some(parent) = dest_path.as_path().parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create parent directory: {:?}", parent))?;
            }

            // Check if file exists and save its permissions
            #[cfg(unix)]
            let existing_mode = if dest_path.as_path().exists() {
                use std::os::unix::fs::PermissionsExt;
                fs::metadata(dest_path.as_path())
                    .ok()
                    .map(|m| m.permissions().mode())
            } else {
                None
            };

            // Decrypt inline age values before writing to destination
            // This allows source files to contain age:... encrypted values
            // but destination files get plaintext (for applications to use)
            let final_content = decrypt_inline_age_values(content, identities)?;

            // Write file with atomic permission setting to avoid TOCTOU race condition
            #[cfg(unix)]
            {
                use std::io::Write;
                use std::os::unix::fs::OpenOptionsExt;

                // Determine permissions to use
                // - If source has mode, use it (source is authoritative)
                // - Otherwise, preserve existing permissions if file existed
                // - Default to 0o600 (owner read/write only) for security
                let mode_to_use = mode.or(existing_mode).unwrap_or(0o600);

                // Create file with permissions atomically (no TOCTOU window)
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .mode(mode_to_use)
                    .open(dest_path.as_path())
                    .with_context(|| format!("Failed to create file: {:?}", dest_path))?;

                file.write_all(&final_content)
                    .with_context(|| format!("Failed to write file content: {:?}", dest_path))?;
            }

            #[cfg(not(unix))]
            {
                // On non-Unix systems, use standard write (no mode support)
                fs::write(dest_path.as_path(), &final_content)
                    .with_context(|| format!("Failed to write file: {:?}", dest_path))?;
            }

            Ok(())
        }

        TargetEntry::Directory { mode, .. } => {
            // Create directory
            fs::create_dir_all(dest_path.as_path())
                .with_context(|| format!("Failed to create directory: {:?}", dest_path))?;

            // Set permissions
            #[cfg(unix)]
            if let Some(mode) = mode {
                use std::os::unix::fs::PermissionsExt;
                let permissions = fs::Permissions::from_mode(*mode);
                fs::set_permissions(dest_path.as_path(), permissions)
                    .with_context(|| format!("Failed to set permissions: {:?}", dest_path))?;
            }

            Ok(())
        }

        TargetEntry::Symlink { target, .. } => {
            // Ensure parent directory exists
            if let Some(parent) = dest_path.as_path().parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create parent directory: {:?}", parent))?;
            }

            // Remove existing symlink/file if it exists
            if dest_path.as_path().exists() || dest_path.as_path().is_symlink() {
                if dest_path.as_path().is_dir() && !dest_path.as_path().is_symlink() {
                    fs::remove_dir_all(dest_path.as_path()).with_context(|| {
                        format!("Failed to remove existing directory: {:?}", dest_path)
                    })?;
                } else {
                    fs::remove_file(dest_path.as_path()).with_context(|| {
                        format!("Failed to remove existing file/symlink: {:?}", dest_path)
                    })?;
                }
            }

            // Create symlink
            #[cfg(unix)]
            {
                use std::os::unix::fs::symlink;
                symlink(target, dest_path.as_path())
                    .with_context(|| format!("Failed to create symlink: {:?}", dest_path))?;
            }

            #[cfg(windows)]
            {
                use std::os::windows::fs::symlink_file;
                symlink_file(target, dest_path.as_path())
                    .with_context(|| format!("Failed to create symlink: {:?}", dest_path))?;
            }

            Ok(())
        }

        TargetEntry::Remove { .. } => {
            // Handle removal entries (not used in apply, but included for completeness)
            if dest_path.as_path().exists() {
                if dest_path.as_path().is_dir() {
                    fs::remove_dir_all(dest_path.as_path())
                        .with_context(|| format!("Failed to remove directory: {:?}", dest_path))?;
                } else {
                    fs::remove_file(dest_path.as_path())
                        .with_context(|| format!("Failed to remove file: {:?}", dest_path))?;
                }
            }
            Ok(())
        }
    }
}

/// Save entry state to database
fn save_entry_state_to_db(entry: &TargetEntry) -> Result<()> {
    // Only save state for files (directories and symlinks don't need content state)
    if let TargetEntry::File { content, mode, .. } = entry {
        let path = entry.path().to_string();
        guisu_engine::database::save_entry_state(&path, content, *mode)
            .with_context(|| format!("Failed to save state for {}", path))?;
    }
    Ok(())
}

impl ApplyStats {
    fn record_success(&self, entry: &TargetEntry) {
        match entry {
            TargetEntry::File { .. } => self.inc_files(),
            TargetEntry::Directory { .. } => self.inc_directories(),
            TargetEntry::Symlink { .. } => self.inc_symlinks(),
            TargetEntry::Remove { .. } => {}
        }
    }

    fn record_failure(&self) {
        self.inc_failed();
    }

    fn record_dry_run(&self, entry: &TargetEntry) {
        // Same as success for counting purposes
        self.record_success(entry);
    }
}

/// Print a dry-run entry
fn print_dry_run_entry(entry: &TargetEntry, use_nerd_fonts: bool) {
    use lscolors::{LsColors, Style};

    let lscolors = LsColors::from_env().unwrap_or_default();
    let path = entry.path();
    let display_path = format!("~/{}", path);

    // Get file icon
    let (is_directory, is_symlink) = match entry {
        TargetEntry::File { .. } => (false, false),
        TargetEntry::Directory { .. } => (true, false),
        TargetEntry::Symlink { .. } => (false, true),
        TargetEntry::Remove { .. } => (false, false),
    };

    let icon_info = crate::ui::icons::FileIconInfo {
        path: display_path.as_str(),
        is_directory,
        is_symlink,
    };
    let icon = crate::ui::icons::icon_for_file(&icon_info, use_nerd_fonts);

    // Get color style
    let file_style = lscolors
        .style_for_path(&display_path)
        .map(Style::to_nu_ansi_term_style)
        .unwrap_or_default();

    let styled_icon = file_style.paint(icon);
    let styled_path = file_style.paint(&display_path);

    println!("  {} {} {}", "[dry-run]".dimmed(), styled_icon, styled_path);
}

/// Print a successful entry
fn print_success_entry(entry: &TargetEntry, use_nerd_fonts: bool) {
    use lscolors::{LsColors, Style};

    let lscolors = LsColors::from_env().unwrap_or_default();
    let path = entry.path();
    let display_path = format!("~/{}", path);

    // Get file icon
    let (is_directory, is_symlink) = match entry {
        TargetEntry::File { .. } => (false, false),
        TargetEntry::Directory { .. } => (true, false),
        TargetEntry::Symlink { .. } => (false, true),
        TargetEntry::Remove { .. } => (false, false),
    };

    let icon_info = crate::ui::icons::FileIconInfo {
        path: display_path.as_str(),
        is_directory,
        is_symlink,
    };
    let icon = crate::ui::icons::icon_for_file(&icon_info, use_nerd_fonts);

    // Get color style
    let file_style = lscolors
        .style_for_path(&display_path)
        .map(Style::to_nu_ansi_term_style)
        .unwrap_or_default();

    let styled_icon = file_style.paint(icon);
    let styled_path = file_style.paint(&display_path);

    println!("  {} {} {}", "✓".bright_green(), styled_icon, styled_path);
}

/// Print an error entry
fn print_error_entry(entry: &TargetEntry, error: &anyhow::Error, use_nerd_fonts: bool) {
    use lscolors::{LsColors, Style};

    let lscolors = LsColors::from_env().unwrap_or_default();
    let path = entry.path();
    let display_path = format!("~/{}", path);

    // Get file icon
    let (is_directory, is_symlink) = match entry {
        TargetEntry::File { .. } => (false, false),
        TargetEntry::Directory { .. } => (true, false),
        TargetEntry::Symlink { .. } => (false, true),
        TargetEntry::Remove { .. } => (false, false),
    };

    let icon_info = crate::ui::icons::FileIconInfo {
        path: display_path.as_str(),
        is_directory,
        is_symlink,
    };
    let icon = crate::ui::icons::icon_for_file(&icon_info, use_nerd_fonts);

    // Get color style
    let file_style = lscolors
        .style_for_path(&display_path)
        .map(Style::to_nu_ansi_term_style)
        .unwrap_or_default();

    let styled_icon = file_style.paint(icon);
    let styled_path = file_style.paint(&display_path);

    println!(
        "  {} {} {} - {}",
        "✗".bright_red(),
        styled_icon,
        styled_path,
        error.to_string().red()
    );
}

/// Detect configuration drift for files
///
/// Returns a list of file paths where:
/// 1. The user has modified the file locally (actual != last_written)
/// 2. The source has also been updated (target != last_written)
///
/// This indicates potential conflict where both local and source changes exist.
fn detect_config_drift(entries: &[&TargetEntry], dest_abs: &AbsPath) -> Result<Vec<String>> {
    use sha2::{Digest, Sha256};

    // Parallel processing of drift detection (3x SHA256 per file = CPU-intensive)
    let drift_files: Vec<String> = entries
        .par_iter()
        .filter_map(|entry| {
            // Only check files
            let target_content = match entry {
                TargetEntry::File { content, .. } => content,
                _ => return None,
            };

            let dest_path = dest_abs.join(entry.path());

            // Skip if destination doesn't exist
            if !dest_path.as_path().exists() {
                return None;
            }

            // Get last written state from database
            let path_str = entry.path().to_string();
            let last_written_state = match guisu_engine::database::get_entry_state(&path_str) {
                Ok(Some(state)) => state,
                Ok(None) => return None, // No previous state, can't detect drift
                Err(e) => {
                    warn!(path = %path_str, error = %e, "Failed to read entry state");
                    return None;
                }
            };

            // Read actual content from destination
            let actual_content = match fs::read(dest_path.as_path()) {
                Ok(content) => content,
                Err(e) => {
                    warn!(path = %path_str, error = %e, "Failed to read destination file");
                    return None;
                }
            };

            // Compute actual content hash
            let mut hasher = Sha256::new();
            hasher.update(&actual_content);
            let actual_hash = hasher.finalize().to_vec();

            // Compute target content hash
            let mut hasher = Sha256::new();
            hasher.update(target_content);
            let target_hash = hasher.finalize().to_vec();

            // Check for drift:
            // 1. actual != last_written (user modified)
            // 2. target != last_written (source updated)
            // 3. target != actual (they're different)
            //
            // Use constant-time comparison for hashes to prevent timing side-channel attacks
            let user_modified = !bool::from(actual_hash.ct_eq(&last_written_state.content_hash));
            let source_updated = !bool::from(target_hash.ct_eq(&last_written_state.content_hash));
            let contents_differ = target_content != &actual_content;

            if user_modified && source_updated && contents_differ {
                Some(path_str)
            } else {
                None
            }
        })
        .collect();

    Ok(drift_files)
}

/// Decrypt inline age: encrypted values in file content
///
/// This function scans the content for age:base64(...) patterns and decrypts them,
/// allowing source files to store encrypted secrets while destination files get plaintext.
///
/// This enables the workflow:
/// - Source: password: age:YWdlLWVuY3J5cHRpb24...
/// - Destination: password: my-secret-password
///
/// If decryption fails or no identities are available, returns the original content unchanged.
fn decrypt_inline_age_values(
    content: &[u8],
    identities: &[guisu_crypto::Identity],
) -> Result<Vec<u8>> {
    // Convert to string (if not valid UTF-8, return original)
    let content_str = match String::from_utf8(content.to_vec()) {
        Ok(s) => s,
        Err(_) => return Ok(content.to_vec()), // Binary file, return as-is
    };

    // Check if content contains age: prefix (quick check before decrypting)
    if !content_str.contains("age:") {
        return Ok(content.to_vec()); // No encrypted values, return as-is
    }

    // If no identities available, return original content
    if identities.is_empty() {
        return Ok(content.to_vec());
    }

    // Decrypt all inline age values
    match guisu_crypto::decrypt_file_content(&content_str, identities) {
        Ok(decrypted) => Ok(decrypted.into_bytes()),
        Err(e) => {
            // Log the error with context
            warn!(
                "Failed to decrypt inline age values in file. \
                 Content will be written with encrypted age: values intact. \
                 Applications may not be able to use these values. \
                 Error: {}",
                e
            );

            // Return original content with encrypted values
            // This allows the file to be applied, but the application
            // will see "age:..." strings instead of plaintext
            //
            // NOTE: Future enhancement - add config option to fail loudly
            // See CLAUDE.md: "Encryption Failure Handling Configuration"
            Ok(content.to_vec())
        }
    }
}
