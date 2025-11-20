//! Diff command implementation
//!
//! Show differences between source and destination states.

use anyhow::{Context, Result};
use clap::Args;
use guisu_core::path::AbsPath;
use guisu_engine::adapters::crypto::CryptoDecryptorAdapter;
use guisu_engine::adapters::template::TemplateRendererAdapter;
use guisu_engine::entry::TargetEntry;
use guisu_engine::processor::ContentProcessor;
use guisu_engine::state::{SourceState, TargetState};
use owo_colors::OwoColorize;
use rayon::prelude::*;
use similar::{ChangeTag, TextDiff};
use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::{Command as ProcessCommand, Stdio};
use std::sync::Arc;
use tracing::{debug, warn};

use crate::command::Command;
use crate::common::RuntimeContext;
use crate::stats::DiffStats;
use guisu_config::Config;

/// Diff command
#[derive(Args)]
pub struct DiffCommand {
    /// Specific files to diff (all if not specified)
    pub files: Vec<PathBuf>,

    /// Use pager for output
    #[arg(long)]
    pub pager: bool,

    /// Interactive diff viewer
    #[arg(short, long)]
    pub interactive: bool,
}

impl Command for DiffCommand {
    type Output = ();
    fn execute(&self, context: &RuntimeContext) -> crate::error::Result<()> {
        run_impl(
            context.source_dir(),
            context.dest_dir().as_path(),
            &self.files,
            self.pager,
            self.interactive,
            &context.config,
        )
        .map_err(Into::into)
    }
}

/// Run the diff command implementation
fn run_impl(
    source_dir: &Path,
    dest_dir: &Path,
    files: &[PathBuf],
    pager: bool,
    interactive: bool,
    config: &Config,
) -> Result<()> {
    // Resolve all paths (handles root_entry and canonicalization)
    let paths = crate::common::ResolvedPaths::resolve(source_dir, dest_dir, config)?;
    let source_abs = &paths.dotfiles_dir;
    let dest_abs = &paths.dest_dir;

    // Get .guisu directory and platform name for loading variables and ignore patterns
    let guisu_dir = source_dir.join(".guisu");
    let platform_name = guisu_core::platform::CURRENT_PLATFORM.os;

    // Load metadata for create-once tracking
    let metadata =
        guisu_engine::state::Metadata::load(source_dir).context("Failed to load metadata")?;

    // Create ignore matcher from .guisu/ignores.toml
    // Use dotfiles_dir as the match root so patterns match relative to the dotfiles directory
    let ignore_matcher = guisu_config::IgnoreMatcher::from_ignores_toml(source_dir)
        .context("Failed to load ignore patterns from .guisu/ignores.toml")?;

    // Read source state
    let source_state =
        SourceState::read(source_abs.to_owned()).context("Failed to read source state")?;

    if source_state.is_empty() {
        println!("No files to diff. Add files with: guisu add <file>");
        return Ok(());
    }

    // Load age identities for decryption
    let identities = std::sync::Arc::new(config.age_identities().unwrap_or_default());

    // Track if we've already shown a decryption error message
    let shown_decryption_error = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    // Load variables from .guisu/variables/ directory
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
    // If no identity is available, encrypted files won't be decrypted
    // Use Arc to share identity without unnecessary cloning
    let identity_arc = if let Some(first) = identities.first() {
        Arc::new(first.clone())
    } else {
        // Create a dummy identity - encrypted files will fail to decrypt gracefully
        Arc::new(guisu_crypto::Identity::generate())
    };
    let decryptor = CryptoDecryptorAdapter::from_arc(identity_arc);
    let renderer = TemplateRendererAdapter::new(template_engine);
    let processor = ContentProcessor::new(decryptor, renderer);

    // Build filter paths if specific files requested
    let filter_paths = if !files.is_empty() {
        Some(crate::build_filter_paths(files, dest_abs)?)
    } else {
        None
    };

    // Build target state (processes templates and decrypts files)
    // Only process files that we're going to diff to avoid template errors in unrelated files
    // Create template context with system variables and guisu info
    let working_tree = guisu_engine::git::find_working_tree(source_dir)
        .unwrap_or_else(|| source_dir.to_path_buf());
    let template_context = guisu_template::TemplateContext::new()
        .with_variables(all_variables)
        .with_guisu_info(
            source_abs.to_string(),
            working_tree.display().to_string(),
            dest_abs.to_string(),
            config.general.root_entry.display().to_string(),
        );
    let context =
        serde_json::to_value(&template_context).context("Failed to serialize template context")?;
    let mut target_state = TargetState::new();

    for source_entry in source_state.entries() {
        let target_path = source_entry.target_path();

        // Skip if file is ignored
        if ignore_matcher.is_ignored(target_path.as_path()) {
            continue;
        }

        // If filtering, skip entries not in the filter
        if let Some(ref filter) = filter_paths
            && !filter.contains(target_path)
        {
            continue;
        }

        // Process this entry manually to handle errors gracefully
        use guisu_engine::entry::SourceEntry;
        match source_entry {
            SourceEntry::File {
                source_path,
                target_path,
                attributes,
            } => {
                let abs_source_path = source_state.source_file_path(source_path);
                match processor.process_file(&abs_source_path, attributes, &context) {
                    Ok(mut content) => {
                        // Decrypt inline age: values (sops-like behavior)
                        if !identities.is_empty()
                            && let Ok(content_str) = String::from_utf8(content.clone())
                            && content_str.contains("age:")
                            && let Ok(decrypted) =
                                guisu_crypto::decrypt_file_content(&content_str, &identities)
                        {
                            content = decrypted.into_bytes();
                        }

                        let mode = attributes.mode();
                        target_state.add(TargetEntry::File {
                            path: target_path.clone(),
                            content,
                            mode,
                        });
                    }
                    Err(e) => {
                        // Only show detailed error message once for decryption failures
                        let error_msg = e.to_string();
                        if error_msg.contains("Decryption failed") {
                            if !shown_decryption_error
                                .swap(true, std::sync::atomic::Ordering::Relaxed)
                            {
                                // First decryption error - check if it's a missing identity file
                                if identities.is_empty() {
                                    if let Some(ref identity_path) = config.age.identity {
                                        eprintln!(
                                            "{} Decryption failed - {:?}: no such file or directory",
                                            "Error:".red().bold(),
                                            identity_path
                                        );
                                    } else if let Some(ref identities_paths) = config.age.identities
                                    {
                                        if let Some(first_path) = identities_paths.first() {
                                            eprintln!(
                                                "{} Decryption failed - {:?}: no such file or directory",
                                                "Error:".red().bold(),
                                                first_path
                                            );
                                        } else {
                                            eprintln!(
                                                "{} No age identity configured",
                                                "Error:".red().bold()
                                            );
                                        }
                                    } else {
                                        eprintln!(
                                            "{} No age identity configured",
                                            "Error:".red().bold()
                                        );
                                    }
                                } else {
                                    // Show first decryption error
                                    warn!(
                                        "Decryption failed for {}: {}",
                                        target_path.as_path().display(),
                                        e
                                    );
                                }
                            }
                            // Subsequent decryption errors are silently counted
                        } else {
                            // Non-decryption errors are still shown
                            warn!(
                                "Failed to process {}: {}",
                                target_path.as_path().display(),
                                e
                            );
                        }
                    }
                }
            }
            SourceEntry::Directory {
                source_path: _,
                target_path,
                attributes,
            } => {
                let mode = attributes.mode();
                target_state.add(TargetEntry::Directory {
                    path: target_path.clone(),
                    mode,
                });
            }
            SourceEntry::Symlink {
                source_path: _,
                target_path,
                link_target,
            } => {
                target_state.add(TargetEntry::Symlink {
                    path: target_path.clone(),
                    target: link_target.clone(),
                });
            }
        }
    }

    // Use thread-safe stats for parallel processing
    let stats = Arc::new(DiffStats::new());

    // Parallel diff of target entries
    let diff_outputs: Vec<String> = target_state
        .entries()
        .par_bridge()
        .filter_map(|entry| {
            // Skip directories, symlinks, and remove entries - only diff files
            if !matches!(entry, TargetEntry::File { .. }) {
                return None;
            }

            let target_path = entry.path();

            // Skip if filtering and this file is not in the filter
            if let Some(ref filter) = filter_paths
                && !filter.iter().any(|p| p == target_path)
            {
                return None;
            }
            let path_str = target_path.to_string();

            // Skip create-once files that already exist at destination (silently)
            if metadata.is_create_once(&path_str) {
                let dest_path = dest_abs.join(target_path);
                if dest_path.as_path().exists() {
                    debug!(
                        path = %path_str,
                        "Skipping create-once file that already exists in diff"
                    );
                    return None;
                }
            }

            match diff_target_entry(entry, dest_abs, &stats) {
                Ok(entry_diff) => {
                    if !entry_diff.is_empty() {
                        Some(entry_diff)
                    } else {
                        None
                    }
                }
                Err(e) => {
                    // Track error
                    stats.inc_errors();

                    // Debug log for verbose mode
                    debug!(path = %target_path, error = %e, "Failed to diff file");

                    // Show path with root_entry prefix for better context
                    let display_path =
                        format!("{}/{}", config.general.root_entry.display(), target_path);
                    warn!("Error processing {}: {}", display_path, e);
                    None
                }
            }
        })
        .collect();

    // If interactive mode is enabled, use the interactive diff viewer
    if interactive {
        use crate::ui::{FileDiff, FileStatus, InteractiveDiffViewer};

        // Build FileDiff structures from target state
        let file_diffs: Vec<FileDiff> = target_state
            .entries()
            .filter_map(|entry| {
                if !matches!(entry, TargetEntry::File { .. }) {
                    return None;
                }

                let target_path = entry.path();
                let path_str = target_path.to_string();

                // Skip if filtering and this file is not in the filter
                if let Some(ref filter) = filter_paths
                    && !filter.iter().any(|p| p == target_path)
                {
                    return None;
                }

                // Skip create-once files that already exist at destination
                if metadata.is_create_once(&path_str) {
                    let dest_path = dest_abs.join(target_path);
                    if dest_path.as_path().exists() {
                        return None;
                    }
                }

                if let TargetEntry::File { content, .. } = entry {
                    let dest_path = dest_abs.join(target_path);

                    // Determine file status and content
                    let (status, old_content, new_content) = if !dest_path.as_path().exists() {
                        (
                            FileStatus::Added,
                            String::new(),
                            String::from_utf8_lossy(content).to_string(),
                        )
                    } else if let Ok(dest_content) = fs::read(dest_path.as_path()) {
                        if is_binary(content) || is_binary(&dest_content) {
                            // Skip binary files in interactive mode
                            return None;
                        }
                        (
                            FileStatus::Modified,
                            String::from_utf8_lossy(&dest_content).to_string(),
                            String::from_utf8_lossy(content).to_string(),
                        )
                    } else {
                        return None;
                    };

                    // Only include files that have actual changes
                    if status == FileStatus::Modified && old_content == new_content {
                        return None;
                    }

                    Some(FileDiff::new(path_str, old_content, new_content, status))
                } else {
                    None
                }
            })
            .collect();

        if file_diffs.is_empty() {
            println!("No differences to display");
        } else {
            let mut viewer = InteractiveDiffViewer::new(file_diffs);
            viewer.run()?;
        }

        return Ok(());
    }

    // Check and display hooks status first
    print_hooks_status(source_dir, config)?;

    // Join all diff outputs
    let diff_output = diff_outputs.join("\n");

    // Print diff output (no message if no differences)
    if !diff_output.is_empty() {
        if pager {
            maybe_use_pager(&diff_output, config)?;
        } else {
            print_colored_diff(&diff_output);
        }
    }

    // Print statistics at the end
    println!();
    print_stats(&stats);

    Ok(())
}

/// Diff a single target entry against destination
fn diff_target_entry(entry: &TargetEntry, dest_abs: &AbsPath, stats: &DiffStats) -> Result<String> {
    let target_path = entry.path();
    let dest_path = dest_abs.join(target_path);

    // Only process File entries
    let (source_content, source_mode) = match entry {
        TargetEntry::File { content, mode, .. } => (content.clone(), *mode),
        _ => return Ok(String::new()),
    };

    // Check if destination exists
    if !dest_path.as_path().exists() {
        stats.inc_added();
        return format_new_file(target_path.as_path(), &source_content, source_mode);
    }

    // Get destination content and mode
    let dest_content = fs::read(dest_path.as_path())
        .with_context(|| format!("Failed to read destination file: {}", dest_path))?;

    #[cfg(unix)]
    let dest_mode = {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(dest_path.as_path())
            .ok()
            .map(|m| m.permissions().mode())
    };
    #[cfg(not(unix))]
    let dest_mode: Option<u32> = None;

    // Check if mode differs (compare only permission bits, not file type)
    let mode_differs = if let Some(src_mode) = source_mode {
        if let Some(dst_mode) = dest_mode {
            // Mask to get only permission bits (lower 12 bits)
            const PERM_MASK: u32 = 0o7777;
            (src_mode & PERM_MASK) != (dst_mode & PERM_MASK)
        } else {
            true // dest doesn't have mode
        }
    } else {
        false // source doesn't specify mode
    };

    // Check if binary
    if is_binary(&source_content) || is_binary(&dest_content) {
        if source_content != dest_content || mode_differs {
            stats.inc_modified();
            let mut output = String::new();
            if mode_differs {
                output.push_str(&format_mode_diff(dest_mode, source_mode));
            }
            output.push_str(&format!(
                "{} {} differ\n",
                "Binary files".bold(),
                target_path.as_path().display().to_string().cyan()
            ));
            return Ok(output);
        }
        stats.inc_unchanged();
        return Ok(String::new());
    }

    // Generate text diff
    let source_str = String::from_utf8_lossy(&source_content);
    let dest_str = String::from_utf8_lossy(&dest_content);
    let content_differs = source_str != dest_str;

    if !content_differs && !mode_differs {
        stats.inc_unchanged();
        return Ok(String::new());
    }

    stats.inc_modified();
    generate_unified_diff(
        &dest_str,
        &source_str,
        &format!("a/{}", target_path),
        &format!("b/{}", target_path),
        dest_mode,
        source_mode,
    )
}

/// Format mode diff header
fn format_mode_diff(old_mode: Option<u32>, new_mode: Option<u32>) -> String {
    const S_IFREG: u32 = 0o100000; // Regular file type bit
    const DEFAULT_MODE: u32 = 0o644; // Default file permissions

    // Ensure both modes include file type bits for consistent display
    let old_mode_full = old_mode.unwrap_or(DEFAULT_MODE | S_IFREG);
    let new_mode_full = new_mode
        .map(|m| {
            // If mode only has permission bits (< octal 10000), add file type
            if m < 0o10000 { m | S_IFREG } else { m }
        })
        .unwrap_or(DEFAULT_MODE | S_IFREG);

    format!(
        "old mode {:06o}\nnew mode {:06o}\n",
        old_mode_full, new_mode_full
    )
}

/// Check if content is binary
fn is_binary(content: &[u8]) -> bool {
    // Simple heuristic: check for null bytes in first 8KB
    content.iter().take(8000).any(|&b| b == 0)
}

/// Generate unified diff using the similar crate
fn generate_unified_diff(
    old: &str,
    new: &str,
    old_path: &str,
    new_path: &str,
    old_mode: Option<u32>,
    new_mode: Option<u32>,
) -> Result<String> {
    let diff = TextDiff::from_lines(old, new);
    let mut output = String::new();

    // Add mode diff if permission bits differ (compare only permission bits, not file type)
    const PERM_MASK: u32 = 0o7777;
    let mode_differs = match (old_mode, new_mode) {
        (Some(old), Some(new)) => (old & PERM_MASK) != (new & PERM_MASK),
        (None, Some(_)) | (Some(_), None) => true,
        (None, None) => false,
    };

    if mode_differs {
        output.push_str(&format_mode_diff(old_mode, new_mode));
    }

    output.push_str(&format!("--- {}\n", old_path));
    output.push_str(&format!("+++ {}\n", new_path));

    for (idx, group) in diff.grouped_ops(3).iter().enumerate() {
        if idx > 0 {
            output.push_str("---\n");
        }

        let mut hunk_header = String::from("@@");
        if let Some(first) = group.first()
            && let Some(last) = group.last()
        {
            let old_start = first.old_range().start + 1;
            let old_len = last.old_range().end - first.old_range().start;
            let new_start = first.new_range().start + 1;
            let new_len = last.new_range().end - first.new_range().start;
            hunk_header.push_str(&format!(
                " -{},{} +{},{} ",
                old_start, old_len, new_start, new_len
            ));
        }
        hunk_header.push_str("@@\n");
        output.push_str(&hunk_header);

        for op in group {
            for change in diff.iter_changes(op) {
                let sign = match change.tag() {
                    ChangeTag::Delete => "-",
                    ChangeTag::Insert => "+",
                    ChangeTag::Equal => " ",
                };
                output.push_str(sign);
                output.push_str(change.value());
                if !change.value().ends_with('\n') {
                    output.push('\n');
                }
            }
        }
    }

    Ok(output)
}

/// Format a new file for diff output
fn format_new_file(path: &Path, content: &[u8], mode: Option<u32>) -> Result<String> {
    let content_str = String::from_utf8_lossy(content);
    let mut output = String::new();

    // Add mode if present
    if let Some(m) = mode {
        output.push_str(&format!("new file mode {:06o}\n", m));
    }

    output.push_str("--- /dev/null\n");
    output.push_str(&format!("+++ b/{}\n", path.display()));
    output.push_str("@@ -0,0 +1");

    let line_count = content_str.lines().count();
    output.push_str(&format!(",{} @@\n", line_count));

    for line in content_str.lines() {
        output.push_str(&format!("+{}\n", line));
    }

    Ok(output)
}

/// Print colored diff output
fn print_colored_diff(diff: &str) {
    for line in diff.lines() {
        if line.starts_with("---") || line.starts_with("+++") {
            println!("{}", line.bold());
        } else if line.starts_with("@@") {
            println!("{}", line.cyan());
        } else if line.starts_with('+') {
            println!("{}", line.green());
        } else if line.starts_with('-') {
            println!("{}", line.red());
        } else {
            println!("{}", line);
        }
    }
}

/// Use pager for output if available
fn maybe_use_pager(output: &str, _config: &Config) -> Result<()> {
    // Try to use pager from environment
    let pager = env::var("PAGER").unwrap_or_else(|_| {
        #[cfg(unix)]
        {
            "less -R".to_string()
        }
        #[cfg(windows)]
        {
            "more".to_string()
        }
    });

    let mut parts = pager.split_whitespace();
    let cmd = parts.next().unwrap_or("less");
    let args: Vec<_> = parts.collect();

    match ProcessCommand::new(cmd)
        .args(&args)
        .stdin(Stdio::piped())
        .spawn()
    {
        Ok(mut child) => {
            if let Some(mut stdin) = child.stdin.take() {
                // Write colored output
                for line in output.lines() {
                    let colored_line = if line.starts_with("---") || line.starts_with("+++") {
                        format!("{}\n", line.bold())
                    } else if line.starts_with("@@") {
                        format!("{}\n", line.cyan())
                    } else if line.starts_with('+') {
                        format!("{}\n", line.green())
                    } else if line.starts_with('-') {
                        format!("{}\n", line.red())
                    } else {
                        format!("{}\n", line)
                    };
                    let _ = stdin.write_all(colored_line.as_bytes());
                }
            }
            child.wait()?;
        }
        Err(_) => {
            // Fallback to direct print if pager fails
            print_colored_diff(output);
        }
    }

    Ok(())
}

/// Print statistics summary
fn print_stats(stats: &DiffStats) {
    let added = stats.added();
    let modified = stats.modified();
    let unchanged = stats.unchanged();
    let errors = stats.errors();

    if added == 0 && modified == 0 && errors == 0 {
        return;
    }

    println!("{}", "Summary:".bold());
    if added > 0 {
        println!(
            "  {} {} to be added",
            added.to_string().green(),
            if added == 1 { "file" } else { "files" }
        );
    }
    if modified > 0 {
        println!(
            "  {} {} to be modified",
            modified.to_string().yellow(),
            if modified == 1 { "file" } else { "files" }
        );
    }
    if unchanged > 0 {
        println!(
            "  {} {} unchanged",
            unchanged.to_string().dimmed(),
            if unchanged == 1 { "file" } else { "files" }
        );
    }
    if errors > 0 {
        println!(
            "  {} {} with errors (check warnings above)",
            errors.to_string().red(),
            if errors == 1 { "file" } else { "files" }
        );

        // Show unified help message for decryption errors
        println!("\n{}", "Common fixes for decryption errors:".yellow());
        println!("  1. Ensure you're using the correct age identity");
        println!("  2. Check if files were encrypted with a different key");
        println!("  3. Re-encrypt if needed:  guisu edit <file>");
    }
}

/// Render hook template content
fn render_hook_template(source_dir: &Path, content: &str, config: &Config) -> Result<String> {
    use guisu_template::TemplateContext;
    use std::sync::Arc;

    // Load age identities for encryption support in templates
    let identities = Arc::new(config.age_identities().unwrap_or_else(|_| Vec::new()));

    // Create template engine with bitwarden provider support
    let engine = crate::create_template_engine(source_dir, identities, config);

    // Get destination directory
    let dst_dir = config
        .general
        .dst_dir
        .clone()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| std::path::PathBuf::from("~"));

    // Create template context with guisu info and all variables
    let working_tree = guisu_engine::git::find_working_tree(source_dir)
        .unwrap_or_else(|| source_dir.to_path_buf());
    let context = TemplateContext::new()
        .with_guisu_info(
            crate::path_to_string(source_dir),
            crate::path_to_string(&working_tree),
            crate::path_to_string(&dst_dir),
            crate::path_to_string(&config.general.root_entry),
        )
        .with_loaded_variables(source_dir, config)
        .map_err(|e| anyhow::anyhow!("Failed to load variables: {}", e))?;

    // Render template
    engine
        .render_str(content, &context)
        .map_err(|e| anyhow::anyhow!("Template rendering error: {}", e))
}

/// Print hook diff with content changes
fn print_hook_diff(
    source_dir: &Path,
    current: &guisu_engine::hooks::Hook,
    previous: Option<&guisu_engine::hooks::Hook>,
    stage: &str,
    platform: &str,
    config: &Config,
) -> Result<()> {
    use owo_colors::OwoColorize;

    let is_active = current.should_run_on(platform);

    match previous {
        None => {
            // New hook (added) - show full content
            println!();
            println!("  {} hook: {}", stage, current.name.green());

            if let Some(ref cmd) = current.cmd {
                println!("    {} cmd:", "+".bold());
                for line in cmd.lines() {
                    println!("      + {}", line.green());
                }
            } else if let Some(ref script) = current.script {
                // Display script path without .j2 suffix
                let display_script = script.strip_suffix(".j2").unwrap_or(script);
                println!("    {} script: {}", "+".bold(), display_script);

                // Use stored script content or read from file
                let raw_content = if let Some(ref stored_content) = current.script_content {
                    Some(stored_content.clone())
                } else {
                    // Fallback: read from file (script path is relative to source_dir)
                    let script_path = source_dir.join(script);
                    std::fs::read_to_string(&script_path).ok()
                };

                if let Some(raw_content) = raw_content {
                    // Render template if it's a .j2 file
                    let content = if script.ends_with(".j2") {
                        match render_hook_template(source_dir, &raw_content, config) {
                            Ok(rendered) => rendered,
                            Err(e) => {
                                tracing::warn!("Failed to render hook template: {}", e);
                                raw_content // Fallback to raw content
                            }
                        }
                    } else {
                        raw_content
                    };

                    for line in content.lines() {
                        println!("      + {}", line.green());
                    }
                }
            }
        }
        Some(prev) => {
            // Modified hook - show unified diff
            println!();
            println!("  {} hook: {}", stage, current.name.yellow());

            // Compare cmd
            if current.cmd != prev.cmd {
                match (&prev.cmd, &current.cmd) {
                    (Some(old_cmd), Some(new_cmd)) => {
                        println!("    {} cmd changed:", "~".yellow().bold());
                        let diff_output = generate_text_diff(old_cmd, new_cmd)?;
                        for line in diff_output.lines() {
                            println!("      {}", line);
                        }
                    }
                    (Some(old_cmd), None) => {
                        println!("    {} cmd removed:", "-".red().bold());
                        for line in old_cmd.lines() {
                            println!("      - {}", line.red());
                        }
                    }
                    (None, Some(new_cmd)) => {
                        println!("    {} cmd added:", "+".green().bold());
                        for line in new_cmd.lines() {
                            println!("      + {}", line.green());
                        }
                    }
                    _ => {}
                }
            }

            // Compare script
            if current.script != prev.script {
                match (&prev.script, &current.script) {
                    (Some(old_script), Some(new_script)) => {
                        if old_script != new_script {
                            // Different script files - show both contents
                            let display_old = old_script.strip_suffix(".j2").unwrap_or(old_script);
                            let display_new = new_script.strip_suffix(".j2").unwrap_or(new_script);
                            println!(
                                "    {} script: {} -> {}",
                                "~".yellow().bold(),
                                display_old.red(),
                                display_new.green()
                            );

                            // Try to show content diff if available
                            if let (Some(old_content), Some(new_content)) =
                                (&prev.script_content, &current.script_content)
                            {
                                // Render templates if needed
                                let old_rendered = if old_script.ends_with(".j2") {
                                    render_hook_template(source_dir, old_content, config)
                                        .unwrap_or_else(|_| old_content.clone())
                                } else {
                                    old_content.clone()
                                };
                                let new_rendered = if new_script.ends_with(".j2") {
                                    render_hook_template(source_dir, new_content, config)
                                        .unwrap_or_else(|_| new_content.clone())
                                } else {
                                    new_content.clone()
                                };

                                let diff_output = generate_text_diff(&old_rendered, &new_rendered)?;
                                for line in diff_output.lines() {
                                    println!("      {}", line);
                                }
                            }
                        }
                    }
                    (Some(old_script), None) => {
                        let display_old = old_script.strip_suffix(".j2").unwrap_or(old_script);
                        println!(
                            "    {} script removed: {}",
                            "-".red().bold(),
                            display_old.red()
                        );
                        if let Some(old_content) = &prev.script_content {
                            // Render if template
                            let rendered = if old_script.ends_with(".j2") {
                                render_hook_template(source_dir, old_content, config)
                                    .unwrap_or_else(|_| old_content.clone())
                            } else {
                                old_content.clone()
                            };
                            for line in rendered.lines() {
                                println!("      - {}", line.red());
                            }
                        }
                    }
                    (None, Some(new_script)) => {
                        let display_new = new_script.strip_suffix(".j2").unwrap_or(new_script);
                        println!(
                            "    {} script added: {}",
                            "+".green().bold(),
                            display_new.green()
                        );

                        if let Some(new_content) = &current.script_content {
                            // Render if template
                            let rendered = if new_script.ends_with(".j2") {
                                render_hook_template(source_dir, new_content, config)
                                    .unwrap_or_else(|_| new_content.clone())
                            } else {
                                new_content.clone()
                            };
                            for line in rendered.lines() {
                                println!("      + {}", line.green());
                            }
                        }
                    }
                    _ => {}
                }
            } else if current.script.is_some() {
                // Same script path - check if content changed
                if let (Some(old_content), Some(new_content)) =
                    (&prev.script_content, &current.script_content)
                    && old_content != new_content
                {
                    let script = current.script.as_ref().unwrap();
                    let display_script = script.strip_suffix(".j2").unwrap_or(script);
                    println!(
                        "    {} script content changed: {}",
                        "~".yellow().bold(),
                        display_script
                    );

                    // Render templates before diffing
                    let old_rendered = if script.ends_with(".j2") {
                        render_hook_template(source_dir, old_content, config)
                            .unwrap_or_else(|_| old_content.clone())
                    } else {
                        old_content.clone()
                    };
                    let new_rendered = if script.ends_with(".j2") {
                        render_hook_template(source_dir, new_content, config)
                            .unwrap_or_else(|_| new_content.clone())
                    } else {
                        new_content.clone()
                    };

                    let diff_output = generate_text_diff(&old_rendered, &new_rendered)?;
                    for line in diff_output.lines() {
                        println!("      {}", line);
                    }
                }
            }

            // Show other changes
            if current.order != prev.order {
                println!(
                    "    {} order: {} -> {}",
                    "~".yellow(),
                    prev.order.to_string().red(),
                    current.order.to_string().green()
                );
            }
            if current.mode != prev.mode {
                println!(
                    "    {} mode: {:?} -> {:?}",
                    "~".yellow(),
                    format!("{:?}", prev.mode).red(),
                    format!("{:?}", current.mode).green()
                );
            }
        }
    }

    if !is_active {
        println!("    {} (skipped on this platform)", "ℹ".dimmed());
    }

    Ok(())
}

/// Generate unified diff for text content
fn generate_text_diff(old: &str, new: &str) -> Result<String> {
    use similar::{ChangeTag, TextDiff};

    let diff = TextDiff::from_lines(old, new);
    let mut output = String::new();

    for change in diff.iter_all_changes() {
        let (sign, line_colored) = match change.tag() {
            ChangeTag::Delete => ("-", format!("{}", change.value().red())),
            ChangeTag::Insert => ("+", format!("{}", change.value().green())),
            ChangeTag::Equal => (" ", change.value().to_string()),
        };

        output.push_str(&format!("{} {}", sign, line_colored));
        if !change.value().ends_with('\n') {
            output.push('\n');
        }
    }

    Ok(output)
}

/// Print removed hook with content
fn print_removed_hook(
    source_dir: &Path,
    hook: &guisu_engine::hooks::Hook,
    stage: &str,
    platform: &str,
    config: &Config,
) -> Result<()> {
    use owo_colors::OwoColorize;

    let is_active = hook.should_run_on(platform);

    println!();
    println!("  {} hook: {}", stage, hook.name.red());

    if let Some(ref cmd) = hook.cmd {
        println!("    {} cmd:", "-".red().bold());
        for line in cmd.lines() {
            println!("      {} {}", "-".red(), line.red());
        }
    } else if let Some(ref script) = hook.script {
        let display_script = script.strip_suffix(".j2").unwrap_or(script);
        println!("    {} script: {}", "-".red().bold(), display_script.red());

        // Use stored script content if available
        if let Some(ref content) = hook.script_content {
            // Render template if needed
            let rendered = if script.ends_with(".j2") {
                render_hook_template(source_dir, content, config)
                    .unwrap_or_else(|_| content.clone())
            } else {
                content.clone()
            };

            for line in rendered.lines() {
                println!("      {} {}", "-".red(), line.red());
            }
        }
    }

    if !is_active {
        println!("    {} (skipped on this platform)", "ℹ".dimmed());
    }

    Ok(())
}

/// Check and print hooks status
/// Returns true if any hooks were displayed
fn print_hooks_status(source_dir: &Path, config: &Config) -> Result<bool> {
    use guisu_engine::database;
    use guisu_engine::hooks::HookLoader;
    use guisu_engine::state::{HookStatePersistence, RedbPersistentState};

    let hooks_dir = source_dir.join(".guisu/hooks");

    // Check if hooks directory exists
    if !hooks_dir.exists() {
        return Ok(false);
    }

    // Load hooks
    let loader = HookLoader::new(source_dir);
    let collections = match loader.load() {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!("Failed to load hooks: {}", e);
            return Ok(false);
        }
    };

    if collections.is_empty() {
        return Ok(false);
    }

    // Load state from database
    let db_path = match database::get_db_path() {
        Ok(p) => p,
        Err(e) => {
            tracing::debug!("Failed to get db path: {}", e);
            return Ok(false);
        }
    };

    let db = match RedbPersistentState::new(&db_path) {
        Ok(d) => d,
        Err(e) => {
            tracing::debug!("Failed to open db: {}", e);
            return Ok(false);
        }
    };

    let persistence = HookStatePersistence::new(&db);
    let state = match persistence.load() {
        Ok(s) => s,
        Err(e) => {
            tracing::debug!("Failed to load state: {}", e);
            return Ok(false);
        }
    };

    let has_changed = match state.has_changed(&hooks_dir) {
        Ok(changed) => changed,
        Err(e) => {
            tracing::debug!("Failed to check if hooks changed: {}", e);
            return Ok(false);
        }
    };

    // Only show message if hooks have changed
    if has_changed {
        use guisu_core::platform::CURRENT_PLATFORM;
        use std::collections::HashSet;

        // Compare with last collections if available
        if let Some(ref last) = state.last_collections {
            let platform = CURRENT_PLATFORM.os;

            // Pre hooks comparison
            if !collections.pre.is_empty() || !last.pre.is_empty() {
                let last_names: HashSet<_> = last.pre.iter().map(|h| h.name.as_str()).collect();
                let current_names: HashSet<_> =
                    collections.pre.iter().map(|h| h.name.as_str()).collect();

                // New hooks
                for hook in &collections.pre {
                    if !last_names.contains(hook.name.as_str()) {
                        print_hook_diff(source_dir, hook, None, "pre", platform, config)?;
                    }
                }

                // Removed hooks
                for hook in &last.pre {
                    if !current_names.contains(hook.name.as_str()) {
                        print_removed_hook(source_dir, hook, "pre", platform, config)?;
                    }
                }

                // Modified hooks
                for hook in &collections.pre {
                    if let Some(last_hook) = last.pre.iter().find(|h| h.name == hook.name)
                        && (hook.order != last_hook.order
                            || hook.mode != last_hook.mode
                            || hook.cmd != last_hook.cmd
                            || hook.script != last_hook.script)
                    {
                        print_hook_diff(
                            source_dir,
                            hook,
                            Some(last_hook),
                            "pre",
                            platform,
                            config,
                        )?;
                    }
                }
            }

            // Post hooks comparison
            if !collections.post.is_empty() || !last.post.is_empty() {
                let last_names: HashSet<_> = last.post.iter().map(|h| h.name.as_str()).collect();
                let current_names: HashSet<_> =
                    collections.post.iter().map(|h| h.name.as_str()).collect();

                // New hooks
                for hook in &collections.post {
                    if !last_names.contains(hook.name.as_str()) {
                        print_hook_diff(source_dir, hook, None, "post", platform, config)?;
                    }
                }

                // Removed hooks
                for hook in &last.post {
                    if !current_names.contains(hook.name.as_str()) {
                        print_removed_hook(source_dir, hook, "post", platform, config)?;
                    }
                }

                // Modified hooks
                for hook in &collections.post {
                    if let Some(last_hook) = last.post.iter().find(|h| h.name == hook.name)
                        && (hook.order != last_hook.order
                            || hook.mode != last_hook.mode
                            || hook.cmd != last_hook.cmd
                            || hook.script != last_hook.script)
                    {
                        print_hook_diff(
                            source_dir,
                            hook,
                            Some(last_hook),
                            "post",
                            platform,
                            config,
                        )?;
                    }
                }
            }
        } else {
            // No last collections, show all current hooks as new
            let platform = CURRENT_PLATFORM.os;

            // Show all pre hooks as new (added)
            for hook in &collections.pre {
                print_hook_diff(source_dir, hook, None, "pre", platform, config)?;
            }

            // Show all post hooks as new (added)
            for hook in &collections.post {
                print_hook_diff(source_dir, hook, None, "post", platform, config)?;
            }
        }

        // Add blank line after hooks
        println!();

        return Ok(true);
    }

    Ok(false)
}
