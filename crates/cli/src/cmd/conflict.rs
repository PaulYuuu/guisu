//! Conflict resolution for apply command
//!
//! Handles three-state comparison (target, lastWritten, actual) and interactive prompts using ratatui.

use anyhow::{Context, Result, anyhow};
use guisu_core::path::AbsPath;
use guisu_engine::entry::TargetEntry;
use owo_colors::OwoColorize;
use sha2::{Digest, Sha256};
use std::fs;

use crate::ui::{
    ChangePreview, ChangeSummary, ConflictAction, ConflictPrompt, DiffFormat, DiffViewer,
};
use guisu_config::Config;

/// Type of change detected
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeType {
    /// Only destination was modified locally
    LocalModification,
    /// Only source was updated
    SourceUpdate,
    /// Both source and destination were modified (true conflict)
    TrueConflict,
}

/// Result of three-way comparison
///
/// Used by both status and apply commands to ensure consistent behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreeWayComparisonResult {
    /// No changes detected (all three states match)
    NoChange,
    /// Only source changed (can safely update)
    SourceChanged,
    /// Only destination changed (local modification)
    DestinationChanged,
    /// Both changed to different values (conflict)
    BothChanged,
    /// Both changed but converged to same value (no action needed)
    Converged,
}

impl From<ThreeWayComparisonResult> for Option<ChangeType> {
    fn from(result: ThreeWayComparisonResult) -> Self {
        match result {
            ThreeWayComparisonResult::NoChange | ThreeWayComparisonResult::Converged => None,
            ThreeWayComparisonResult::SourceChanged => Some(ChangeType::SourceUpdate),
            ThreeWayComparisonResult::DestinationChanged => Some(ChangeType::LocalModification),
            ThreeWayComparisonResult::BothChanged => Some(ChangeType::TrueConflict),
        }
    }
}

/// Perform three-way comparison of content hashes
///
/// This is the canonical implementation used by both `status` and `apply` commands
/// to ensure consistent behavior when detecting changes.
///
/// # Arguments
///
/// * `source_hash` - Hash of processed source content (templates rendered, files decrypted)
/// * `dest_hash` - Hash of actual destination file content
/// * `base_hash` - Optional hash of last written content (from database)
///
/// # Returns
///
/// Returns the comparison result indicating which states changed
pub fn compare_three_way(
    source_hash: &[u8],
    dest_hash: &[u8],
    base_hash: Option<&[u8]>,
) -> ThreeWayComparisonResult {
    use subtle::ConstantTimeEq;

    if let Some(base) = base_hash {
        // Three-way comparison with base state
        let source_changed = !bool::from(source_hash.ct_eq(base));
        let dest_changed = !bool::from(dest_hash.ct_eq(base));

        match (source_changed, dest_changed) {
            (false, false) => ThreeWayComparisonResult::NoChange,
            (true, false) => ThreeWayComparisonResult::SourceChanged,
            (false, true) => ThreeWayComparisonResult::DestinationChanged,
            (true, true) => {
                // Both changed - check if they converged to same content
                if bool::from(source_hash.ct_eq(dest_hash)) {
                    ThreeWayComparisonResult::Converged
                } else {
                    ThreeWayComparisonResult::BothChanged
                }
            }
        }
    } else {
        // No base state - two-way comparison
        if bool::from(source_hash.ct_eq(dest_hash)) {
            ThreeWayComparisonResult::NoChange
        } else {
            // Content differs, but we can't tell who changed
            // Conservatively treat as source update (safer to update)
            ThreeWayComparisonResult::SourceChanged
        }
    }
}

/// Conflict handler that manages three-state comparison and user interaction
pub struct ConflictHandler {
    /// Whether to override all remaining conflicts
    override_all: bool,
    /// Configuration (shared, unused but kept for future use)
    _config: std::sync::Arc<Config>,
    /// Diff viewer
    diff_viewer: DiffViewer,
    /// Age identities for decrypting inline age values
    identities: std::sync::Arc<Vec<guisu_crypto::Identity>>,
}

impl ConflictHandler {
    /// Create a new conflict handler
    pub fn new(
        config: std::sync::Arc<Config>,
        identities: std::sync::Arc<Vec<guisu_crypto::Identity>>,
    ) -> Self {
        let diff_format = config.ui.diff_format.parse().unwrap_or(DiffFormat::Unified);
        let diff_viewer = DiffViewer::new(diff_format, config.ui.context_lines);

        Self {
            override_all: false,
            _config: config,
            diff_viewer,
            identities,
        }
    }

    /// Detect the type of change for the given entry
    ///
    /// Returns:
    /// - `None` if no change detected
    /// - `Some(LocalModification)` if only destination was modified
    /// - `Some(SourceUpdate)` if only source was updated
    /// - `Some(TrueConflict)` if both were modified
    ///
    /// # Arguments
    ///
    /// * `entry` - The target entry to check
    /// * `dest_abs` - Absolute path to the destination directory
    /// * `last_written_hash` - Last written content hash from database (if available)
    ///
    /// # Returns
    ///
    /// Returns the type of change detected, or None if file is up to date
    pub fn detect_change_type(
        entry: &TargetEntry,
        dest_abs: &AbsPath,
        last_written_hash: Option<&[u8]>,
        identities: &[guisu_crypto::Identity],
    ) -> Result<Option<ChangeType>> {
        // Only check files
        let target_content = match entry {
            TargetEntry::File { content, .. } => content,
            _ => return Ok(None),
        };

        let dest_path = dest_abs.join(entry.path());

        // If destination doesn't exist, no conflict (will be created)
        if !dest_path.as_path().exists() {
            return Ok(None);
        }

        // Read actual content
        let actual_content = fs::read(dest_path.as_path())
            .with_context(|| format!("Failed to read destination file: {}", dest_path))?;

        // Decrypt inline age: values in target_content before hashing (to match status behavior)
        let target_content_decrypted = if !identities.is_empty() {
            if let Ok(content_str) = String::from_utf8(target_content.to_vec()) {
                if content_str.contains("age:") {
                    if let Ok(decrypted) =
                        guisu_crypto::decrypt_file_content(&content_str, identities)
                    {
                        decrypted.into_bytes()
                    } else {
                        target_content.to_vec()
                    }
                } else {
                    target_content.to_vec()
                }
            } else {
                target_content.to_vec()
            }
        } else {
            target_content.to_vec()
        };

        // Compute hashes for three-way comparison
        let mut hasher = Sha256::new();
        hasher.update(&target_content_decrypted);
        let target_hash = hasher.finalize().to_vec();

        let mut hasher = Sha256::new();
        hasher.update(&actual_content);
        let actual_hash = hasher.finalize().to_vec();

        // Use the unified three-way comparison function
        let result = compare_three_way(&target_hash, &actual_hash, last_written_hash);
        Ok(result.into())
    }

    /// Check if a conflict exists (for backward compatibility)
    pub fn has_conflict(
        entry: &TargetEntry,
        dest_abs: &AbsPath,
        last_written_hash: Option<&[u8]>,
        identities: &[guisu_crypto::Identity],
    ) -> Result<bool> {
        Ok(Self::detect_change_type(entry, dest_abs, last_written_hash, identities)?.is_some())
    }

    /// Prompt user for action when a change is detected
    ///
    /// # Arguments
    ///
    /// * `entry` - The target entry with the change
    /// * `dest_abs` - Absolute path to the destination directory
    /// * `last_written_content` - Last written content from database (for merge)
    /// * `change_type` - Type of change detected
    ///
    /// # Returns
    ///
    /// Returns the user's chosen action
    pub fn prompt_action(
        &mut self,
        entry: &TargetEntry,
        dest_abs: &AbsPath,
        _last_written_content: Option<&[u8]>,
        change_type: ChangeType,
    ) -> Result<ConflictAction> {
        // If override_all is set, always return Override
        if self.override_all {
            return Ok(ConflictAction::Override);
        }

        let (target_content, _target_mode) = match entry {
            TargetEntry::File { content, mode, .. } => (content, mode),
            _ => return Err(anyhow!("Cannot handle conflict for non-file entry")),
        };

        // Decrypt inline age: values in target_content before displaying
        let target_content = self.decrypt_inline_age_values(target_content)?;

        let dest_path = dest_abs.join(entry.path());
        let actual_content = fs::read(dest_path.as_path())
            .with_context(|| format!("Failed to read destination file: {}", dest_path))?;

        // Check if binary
        if is_binary(&target_content) || is_binary(&actual_content) {
            // Use appropriate messaging based on change type
            let title = match change_type {
                ChangeType::LocalModification => "Local modification:",
                ChangeType::SourceUpdate => "Source updated:",
                ChangeType::TrueConflict => "Conflict:",
            };

            println!(
                "\n{} {} (binary file)",
                title.yellow().bold(),
                entry.path().bright_white()
            );
            println!("{}", "Binary files cannot be merged or previewed.".dimmed());
            println!("Choose Override to use source version, or Skip to keep destination.\n");

            // Simple prompt for binary files
            return self.simple_prompt(entry);
        }

        // Generate change summary and preview
        let target_str = String::from_utf8_lossy(&target_content);
        let actual_str = String::from_utf8_lossy(&actual_content);

        let summary = ChangeSummary::from_texts(&actual_str, &target_str);
        // Show complete diff content (no line limit) for better review
        let preview = ChangePreview::from_texts(&actual_str, &target_str, usize::MAX);

        // Create and run interactive prompt with change type info
        let mut prompt =
            ConflictPrompt::new(entry.path().to_string(), summary, preview, change_type);

        loop {
            let action = prompt.run()?;

            match action {
                ConflictAction::Diff => {
                    // Show full diff
                    self.show_diff(entry, dest_abs)?;
                    println!("\nPress Enter to continue...");
                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input)?;
                    // Continue prompting
                }
                ConflictAction::AllOverride => {
                    self.override_all = true;
                    return Ok(ConflictAction::Override);
                }
                other => return Ok(other),
            }
        }
    }

    /// Simple prompt for binary files (no preview/merge available)
    fn simple_prompt(&self, _entry: &TargetEntry) -> Result<ConflictAction> {
        use dialoguer::{Select, theme::ColorfulTheme};

        let options = vec![
            "Override - apply source changes",
            "Skip - keep destination as-is",
            "All Override - apply source for all remaining",
            "All Skip - keep all remaining as-is",
            "Quit - exit operation",
        ];

        let theme = ColorfulTheme::default();
        let selection = Select::with_theme(&theme)
            .with_prompt("Binary file - choose action")
            .items(&options)
            .default(0)
            .interact()
            .context("Failed to read user input")?;

        match selection {
            0 => Ok(ConflictAction::Override),
            1 => Ok(ConflictAction::Skip),
            2 => Ok(ConflictAction::AllOverride),
            3 => Ok(ConflictAction::AllSkip),
            4 => Ok(ConflictAction::Quit),
            _ => unreachable!(),
        }
    }

    /// Show a diff between target and actual states
    fn show_diff(&self, entry: &TargetEntry, dest_abs: &AbsPath) -> Result<()> {
        let (target_content, target_mode) = match entry {
            TargetEntry::File { content, mode, .. } => (content, mode),
            _ => return Ok(()),
        };

        let dest_path = dest_abs.join(entry.path());
        let actual_content = fs::read(dest_path.as_path())
            .with_context(|| format!("Failed to read destination file: {}", dest_path))?;

        // Get actual mode
        #[cfg(unix)]
        let actual_mode = {
            use std::os::unix::fs::PermissionsExt;
            fs::metadata(dest_path.as_path())
                .ok()
                .map(|m| m.permissions().mode())
        };
        #[cfg(not(unix))]
        let actual_mode: Option<u32> = None;

        // Decrypt inline age: values in target_content before displaying
        let target_content = self.decrypt_inline_age_values(target_content)?;

        // Check if binary
        if is_binary(&target_content) || is_binary(&actual_content) {
            println!("{}", "Binary files differ".bold());
            return Ok(());
        }

        // Generate text diff using new diff viewer
        let target_str = String::from_utf8_lossy(&target_content);
        let actual_str = String::from_utf8_lossy(&actual_content);

        let mut stdout = std::io::stdout();
        self.diff_viewer.display(
            &mut stdout,
            &actual_str,
            &target_str,
            "actual (destination)",
            "target (source)",
        )?;

        // Show mode diff if applicable
        #[cfg(unix)]
        if let (Some(target_m), Some(actual_m)) = (target_mode, actual_mode) {
            const PERM_MASK: u32 = 0o7777;
            if (target_m & PERM_MASK) != (actual_m & PERM_MASK) {
                println!();
                println!("old mode {:06o}", actual_m);
                println!("new mode {:06o}", target_m);
            }
        }

        Ok(())
    }

    /// Decrypt inline age: encrypted values in file content
    ///
    /// This function scans the content for age:base64(...) patterns and decrypts them,
    /// allowing source files to store encrypted secrets while previews show plaintext.
    ///
    /// If decryption fails or no identities are available, returns the original content unchanged.
    fn decrypt_inline_age_values(&self, content: &[u8]) -> Result<Vec<u8>> {
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
        if self.identities.is_empty() {
            return Ok(content.to_vec());
        }

        // Decrypt all inline age values
        match guisu_crypto::decrypt_file_content(&content_str, &self.identities) {
            Ok(decrypted) => Ok(decrypted.into_bytes()),
            Err(_) => {
                // If decryption fails, return original content
                // This allows the preview to show encrypted values
                Ok(content.to_vec())
            }
        }
    }
}

/// Check if content is binary
fn is_binary(content: &[u8]) -> bool {
    // Simple heuristic: check for null bytes in first 8KB
    content.iter().take(8000).any(|&b| b == 0)
}
