//! Status command implementation
//!
//! Show status of managed files with multiple output formats.

use anyhow::{Context, Result};
use guisu_core::path::{AbsPath, RelPath};
use guisu_engine::adapters::crypto::CryptoDecryptorAdapter;
use guisu_engine::adapters::template::TemplateRendererAdapter;
use guisu_engine::entry::{EntryKind, SourceEntry, TargetEntry};
use guisu_engine::processor::ContentProcessor;
use guisu_engine::state::{DestinationState, SourceState, TargetState};
use guisu_engine::system::RealSystem;
use owo_colors::OwoColorize;
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::fs;
use std::io::IsTerminal;
use std::path::Path;
use std::sync::Arc;
use tracing::debug;

use crate::cmd::conflict::{ThreeWayComparisonResult, compare_three_way};
use crate::ui::icons::{FileIconInfo, icon_for_file};
use guisu_config::Config;
use lscolors::{LsColors, Style};
use nu_ansi_term::Style as AnsiStyle;

/// Output format for status command
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutputFormat {
    Simple,
    Tree,
}

impl std::str::FromStr for OutputFormat {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "simple" => Ok(OutputFormat::Simple),
            "tree" => Ok(OutputFormat::Tree),
            _ => anyhow::bail!("Invalid output format: {}. Use 'simple' or 'tree'", s),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum FileStatus {
    /// File exists in source but not in dest (潜在的，dst 不存在)
    Latent,
    /// Destination is ahead of source (本地修改)
    Ahead,
    /// Source is ahead of destination (源端修改)
    Behind,
    /// Both have changes (双方都有改动)
    Conflict,
    /// Files are in steady state (完全一致、稳定)
    Steady,
}

impl FileStatus {
    fn label(&self) -> &str {
        match self {
            FileStatus::Latent => "[L]",
            FileStatus::Ahead => "[A]",
            FileStatus::Behind => "[B]",
            FileStatus::Conflict => "[C]",
            FileStatus::Steady => "[S]",
        }
    }

    fn full_name(&self) -> &str {
        match self {
            FileStatus::Latent => "[L]atent",
            FileStatus::Ahead => "[A]head",
            FileStatus::Behind => "[B]ehind",
            FileStatus::Conflict => "[C]onflict",
            FileStatus::Steady => "[S]teady",
        }
    }

    fn color_str(&self, text: &str) -> String {
        match self {
            FileStatus::Latent => text.bright_green().to_string(), // 绿色：待部署
            FileStatus::Behind => text.bright_yellow().to_string(), // 黄色：需要更新
            FileStatus::Ahead => text.bright_cyan().to_string(),   // 青色：本地改动
            FileStatus::Conflict => text.bright_red().to_string(), // 红色：冲突
            FileStatus::Steady => text.bright_blue().to_string(),  // 蓝色：稳定
        }
    }
}

/// Complete file information for display
#[derive(Debug, Clone)]
struct FileInfo {
    path: String,
    status: FileStatus,
    file_type: char,
}

impl FileInfo {
    fn status_str(&self) -> String {
        let label = self.status.label();
        self.status.color_str(label)
    }
}

/// Run the status command
pub fn run(
    source_dir: &Path,
    dest_dir: &Path,
    config: &Config,
    files: &[std::path::PathBuf],
    show_all: bool,
    output_format: OutputFormat,
) -> Result<()> {
    // Open database for reading previous state
    guisu_engine::database::open_db().context("Failed to open state database")?;

    // Initialize lscolors from environment
    let lscolors = LsColors::from_env().unwrap_or_default();
    // Get the actual dotfiles directory
    let dotfiles_dir = config.dotfiles_dir(source_dir);
    let source_abs = AbsPath::new(fs::canonicalize(&dotfiles_dir)?)?;
    let dest_abs = AbsPath::new(fs::canonicalize(dest_dir)?)?;

    // Load metadata for create-once tracking
    let metadata =
        guisu_engine::state::Metadata::load(source_dir).context("Failed to load metadata")?;

    // Create ignore matcher from .guisu/ignores.toml
    // Use dotfiles_dir as the match root so patterns match relative to the dotfiles directory
    let ignore_matcher = guisu_config::IgnoreMatcher::from_ignores_toml(source_dir)
        .context("Failed to load ignore patterns from .guisu/ignores.toml")?;

    // Read source state with ignore matcher from config
    let source_state =
        SourceState::read(source_abs.clone()).context("Failed to read source state")?;

    if source_state.is_empty() {
        println!("No files managed yet.");
        return Ok(());
    }

    // Load age identities for decryption
    let identities = std::sync::Arc::new(config.age_identities().unwrap_or_default());

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
    for (key, value) in &config.variables {
        all_variables.insert(key.clone(), value.clone());
    }

    // Create template engine with identities, template directory, and bitwarden provider
    let template_engine =
        crate::create_template_engine(source_dir, Arc::clone(&identities), config);

    // Create content processor with real decryptor and renderer
    // Use Arc to share identity without unnecessary cloning
    let identity_arc = if let Some(first) = identities.first() {
        Arc::new(first.clone())
    } else {
        Arc::new(guisu_crypto::Identity::generate())
    };
    let decryptor = CryptoDecryptorAdapter::from_arc(identity_arc);
    let renderer = TemplateRendererAdapter::new(template_engine);
    let processor = ContentProcessor::new(decryptor, renderer);

    // Build filter paths if specific files were requested
    let filter_paths = if !files.is_empty() {
        let paths = crate::build_filter_paths(files, &dest_abs)?;
        // Check if any files match
        let has_matches = source_state
            .entries()
            .any(|entry| paths.iter().any(|p| p == entry.target_path()));

        if !has_matches {
            println!("No matching files found.");
            return Ok(());
        }
        Some(paths)
    } else {
        None
    };

    // Build target state (processes templates and decrypts files)
    // Process files one by one to handle errors gracefully
    // Create template context with system variables and guisu info
    let template_context = guisu_template::TemplateContext::new()
        .with_variables(all_variables)
        .with_guisu_info(
            source_abs.to_string(),
            dest_abs.to_string(),
            config.general.root_entry.display().to_string(),
        );
    let context =
        serde_json::to_value(&template_context).context("Failed to serialize template context")?;
    let mut target_state = TargetState::new();

    for source_entry in source_state.entries() {
        let target_path = source_entry.target_path();

        // If filtering, skip entries not in the filter
        if let Some(ref filter) = filter_paths
            && !filter.iter().any(|p| p == target_path)
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
                        // Skip files with processing errors (e.g., template syntax errors)
                        debug!(
                            "Warning: Failed to process {}: {}",
                            target_path.as_path().display(),
                            e
                        );
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

    // Read destination state
    let dest_abs_clone = dest_abs.clone();
    let mut dest_state = DestinationState::new(dest_abs);
    let system = RealSystem;

    // Collect file information
    let file_infos = collect_file_info(CollectParams {
        source_state: &source_state,
        target_state: &target_state,
        dest_state: &mut dest_state,
        system: &system,
        dest_root: &dest_abs_clone,
        metadata: &metadata,
        filter_paths: filter_paths.as_ref(),
        ignore_matcher: &ignore_matcher,
    })?;

    // Check if we're viewing a single file (don't show summary header)
    let is_single_file = !files.is_empty() && files.len() == 1;

    // Detect if output is to a terminal for icon auto mode
    let is_tty = std::io::stdout().is_terminal();
    let show_icons = config.ui.icons.should_show_icons(is_tty);

    // Render output based on format
    match output_format {
        OutputFormat::Simple => {
            render_simple(&file_infos, show_all, is_single_file, &lscolors, show_icons)
        }
        OutputFormat::Tree => {
            render_tree(&file_infos, show_all, is_single_file, &lscolors, show_icons)
        }
    }

    // Check and display hooks status
    print_hooks_status(source_dir)?;

    Ok(())
}

/// Parameters for collecting file information
struct CollectParams<'a> {
    source_state: &'a SourceState,
    target_state: &'a TargetState,
    dest_state: &'a mut DestinationState,
    system: &'a RealSystem,
    dest_root: &'a AbsPath,
    metadata: &'a guisu_engine::state::Metadata,
    filter_paths: Option<&'a Vec<RelPath>>,
    ignore_matcher: &'a guisu_config::IgnoreMatcher,
}

/// Collect file information from source and destination states
fn collect_file_info(params: CollectParams) -> Result<Vec<FileInfo>> {
    use std::sync::Mutex;

    // Wrap dest_state in a Mutex for thread-safe access during parallel processing
    // The cache mutations are serialized, but hash computation (CPU-intensive) is still parallel
    let dest_state_mutex = Mutex::new(params.dest_state);

    // Use parallel processing for file info collection
    let files: Vec<FileInfo> = params
        .source_state
        .entries()
        .par_bridge()
        .filter_map(|entry| {
            let target_path = entry.target_path();

            // Skip if filtering and this file is not in the filter
            if let Some(filter) = params.filter_paths
                && !filter.iter().any(|p| p == target_path)
            {
                return None;
            }

            // Skip if file is ignored
            if params.ignore_matcher.is_ignored(target_path.as_path()) {
                return None;
            }

            let path_str = target_path.to_string();

            // Read destination entry (thread-safe via mutex)
            let dest_entry = {
                let mut dest_state = dest_state_mutex.lock().unwrap();
                match dest_state
                    .read(target_path, params.system)
                    .context("Failed to read destination state")
                {
                    Ok(entry) => entry.clone(), // Clone to release the lock quickly
                    Err(e) => {
                        debug!(
                            "Failed to read destination state for {}: {}",
                            target_path.as_path().display(),
                            e
                        );
                        return None;
                    }
                }
            };

            // Handle create-once files that already exist - show as Steady
            if params.metadata.is_create_once(&path_str) && dest_entry.kind != EntryKind::Missing {
                // Determine file type
                let file_type = match entry {
                    SourceEntry::File { .. } => 'F',
                    SourceEntry::Directory { .. } => 'D',
                    SourceEntry::Symlink { .. } => 'L',
                };

                // Build full destination path and format for display (same as normal files)
                let full_dest_path = params.dest_root.join(target_path);
                let display_path = if let Some(home_dir) = dirs::home_dir() {
                    // If path is under home, show as ~/relative/path
                    if let Ok(rel_path) = full_dest_path.as_path().strip_prefix(&home_dir) {
                        format!("~/{}", rel_path.display())
                    } else {
                        full_dest_path.as_path().display().to_string()
                    }
                } else {
                    full_dest_path.as_path().display().to_string()
                };

                return Some(FileInfo {
                    path: display_path,
                    status: FileStatus::Steady,
                    file_type,
                });
            }

            // Determine file type
            let file_type = match entry {
                SourceEntry::File { .. } => 'F',
                SourceEntry::Directory { .. } => 'D',
                SourceEntry::Symlink { .. } => 'L',
            };

            // Determine status based on three-way comparison (Base, Source, Destination)
            let status = if dest_entry.kind == EntryKind::Missing {
                // Destination doesn't exist → Latent
                FileStatus::Latent
            } else {
                // Destination exists, do three-way comparison
                // Use target_state which has processed content (decrypted + rendered)
                let target_entry = match params.target_state.get(target_path) {
                    Some(entry) => entry,
                    None => {
                        // Target entry not found (likely due to template processing error)
                        // Skip this file
                        debug!(
                            "Skipping {}: target entry not found in target state",
                            target_path.as_path().display()
                        );
                        return None;
                    }
                };

                // Get the base state from database (last applied state)
                let base_state = guisu_engine::database::get_entry_state(&path_str)
                    .ok()
                    .flatten();

                match target_entry {
                    TargetEntry::File { content, mode, .. } => {
                        // Compute hashes for three-way comparison
                        use guisu_engine::state::hash_data;
                        let source_hash = hash_data(content);
                        let dest_hash = dest_entry.content.as_ref().map(|c| hash_data(c));

                        // Check mode matches
                        let mode_matches = if let Some(expected_mode) = mode {
                            dest_entry.mode == Some(*expected_mode)
                        } else {
                            true
                        };

                        // Use unified three-way comparison
                        let dest_hash_vec = dest_hash.unwrap_or_default();
                        let base_hash = base_state.as_ref().map(|s| s.content_hash.as_slice());

                        let comparison_result =
                            compare_three_way(&source_hash, &dest_hash_vec, base_hash);

                        // Map comparison result to file status
                        match comparison_result {
                            ThreeWayComparisonResult::NoChange
                            | ThreeWayComparisonResult::Converged => {
                                if mode_matches {
                                    FileStatus::Steady
                                } else {
                                    FileStatus::Behind // Mode changed
                                }
                            }
                            ThreeWayComparisonResult::SourceChanged => FileStatus::Behind,
                            ThreeWayComparisonResult::DestinationChanged => FileStatus::Ahead,
                            ThreeWayComparisonResult::BothChanged => FileStatus::Conflict,
                        }
                    }
                    TargetEntry::Directory { mode, .. } => {
                        if let Some(expected_mode) = mode {
                            if dest_entry.mode == Some(*expected_mode) {
                                FileStatus::Steady
                            } else {
                                FileStatus::Behind
                            }
                        } else {
                            FileStatus::Steady
                        }
                    }
                    TargetEntry::Symlink { target, .. } => {
                        if dest_entry.link_target.as_ref() == Some(target) {
                            FileStatus::Steady
                        } else {
                            FileStatus::Behind
                        }
                    }
                    TargetEntry::Remove { .. } => {
                        // Remove entries should not be in status
                        FileStatus::Behind
                    }
                }
            };

            // Build full destination path and format for display
            let full_dest_path = params.dest_root.join(target_path);
            let display_path = if let Some(home_dir) = dirs::home_dir() {
                // If path is under home, show as ~/relative/path
                if let Ok(rel_path) = full_dest_path.as_path().strip_prefix(&home_dir) {
                    format!("~/{}", rel_path.display())
                } else {
                    full_dest_path.as_path().display().to_string()
                }
            } else {
                full_dest_path.as_path().display().to_string()
            };

            Some(FileInfo {
                path: display_path,
                status,
                file_type,
            })
        })
        .collect();

    // Sort files by path for consistent output
    // Note: Parallel collect doesn't preserve order, so we sort after
    let mut files = files;
    files.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(files)
}

/// Format status line with counts and labels
fn format_status_line(items: &[(usize, FileStatus)]) -> String {
    items
        .iter()
        .map(|(count, status)| {
            format!(
                "{} {}",
                status.color_str(&count.to_string()).bold(),
                status.color_str(status.full_name())
            )
        })
        .collect::<Vec<_>>()
        .join(&format!(" {} ", "|".dimmed()))
}

/// Render simple format (default)
fn render_simple(
    files: &[FileInfo],
    show_all: bool,
    is_single_file: bool,
    lscolors: &LsColors,
    use_nerd_fonts: bool,
) {
    // Group files by status (exclude directories from display)
    let mut latent: Vec<_> = files
        .iter()
        .filter(|f| f.status == FileStatus::Latent && f.file_type != 'D')
        .collect();
    latent.sort_by(|a, b| a.path.cmp(&b.path));

    let mut behind: Vec<_> = files
        .iter()
        .filter(|f| f.status == FileStatus::Behind && f.file_type != 'D')
        .collect();
    behind.sort_by(|a, b| a.path.cmp(&b.path));

    let mut ahead: Vec<_> = files
        .iter()
        .filter(|f| f.status == FileStatus::Ahead && f.file_type != 'D')
        .collect();
    ahead.sort_by(|a, b| a.path.cmp(&b.path));

    let mut conflict: Vec<_> = files
        .iter()
        .filter(|f| f.status == FileStatus::Conflict && f.file_type != 'D')
        .collect();
    conflict.sort_by(|a, b| a.path.cmp(&b.path));

    let mut steady: Vec<_> = files
        .iter()
        .filter(|f| f.status == FileStatus::Steady && f.file_type != 'D')
        .collect();
    steady.sort_by(|a, b| a.path.cmp(&b.path));

    // Print header with status counts (inline abbreviations)
    // Skip header for single file view
    if !is_single_file {
        println!();
    }

    if !is_single_file && show_all {
        let status_items = vec![
            (latent.len(), FileStatus::Latent),
            (ahead.len(), FileStatus::Ahead),
            (behind.len(), FileStatus::Behind),
            (conflict.len(), FileStatus::Conflict),
            (steady.len(), FileStatus::Steady),
        ];
        println!("  {}", format_status_line(&status_items));
    } else if !is_single_file {
        let status_items = vec![
            (latent.len(), FileStatus::Latent),
            (ahead.len(), FileStatus::Ahead),
            (behind.len(), FileStatus::Behind),
            (conflict.len(), FileStatus::Conflict),
        ];
        println!("  {}", format_status_line(&status_items));
    }

    if !is_single_file {
        println!();
    }

    // Show latent files (to deploy)
    for file in &latent {
        let icon = get_file_icon_for_info(file, use_nerd_fonts);
        let file_style = get_file_style(file, lscolors);
        println!(
            "  {}  {} {}",
            file.status_str().bold(),
            file_style.paint(icon),
            file_style.paint(&file.path),
        );
    }

    // Show ahead files (local changes)
    for file in &ahead {
        let icon = get_file_icon_for_info(file, use_nerd_fonts);
        let file_style = get_file_style(file, lscolors);
        println!(
            "  {}  {} {}",
            file.status_str().bold(),
            file_style.paint(icon),
            file_style.paint(&file.path),
        );
    }

    // Show behind files (need update from source)
    for file in &behind {
        let icon = get_file_icon_for_info(file, use_nerd_fonts);
        let file_style = get_file_style(file, lscolors);
        println!(
            "  {}  {} {}",
            file.status_str().bold(),
            file_style.paint(icon),
            file_style.paint(&file.path),
        );
    }

    // Show conflict files
    for file in &conflict {
        let icon = get_file_icon_for_info(file, use_nerd_fonts);
        let file_style = get_file_style(file, lscolors);
        println!(
            "  {}  {} {}",
            file.status_str().bold(),
            file_style.paint(icon),
            file_style.paint(&file.path),
        );
    }

    // Show steady files (if --all is specified OR viewing a single file)
    if show_all || is_single_file {
        for file in &steady {
            let icon = get_file_icon_for_info(file, use_nerd_fonts);
            let file_style = get_file_style(file, lscolors).dimmed();
            println!(
                "  {}  {} {}",
                file.status_str(),
                file_style.paint(icon),
                file_style.paint(&file.path),
            );
        }
    }

    if !is_single_file
        && (!latent.is_empty()
            || !ahead.is_empty()
            || !behind.is_empty()
            || !conflict.is_empty()
            || show_all)
    {
        println!();
    }
}

/// Tree node for nested directory structure
#[derive(Debug)]
enum TreeNode<'a> {
    File(&'a FileInfo),
    Directory {
        children: BTreeMap<String, TreeNode<'a>>,
    },
}

/// Get icon for file using the new icon system
fn get_file_icon_for_info(file: &FileInfo, use_nerd_fonts: bool) -> &'static str {
    let info = FileIconInfo {
        path: &file.path,
        is_directory: file.file_type == 'D',
        is_symlink: file.file_type == 'L',
    };

    icon_for_file(&info, use_nerd_fonts)
}

/// Get ANSI style for file based on its type and attributes
fn get_file_style(file: &FileInfo, lscolors: &LsColors) -> AnsiStyle {
    // Get style from lscolors based on file path and extension
    let style = lscolors.style_for_path(&file.path);

    // Convert to nu_ansi_term::Style
    style.map(Style::to_nu_ansi_term_style).unwrap_or_default()
}

/// Build nested tree structure from file list
fn build_tree<'a>(files: &[&'a FileInfo]) -> BTreeMap<String, TreeNode<'a>> {
    let mut root: BTreeMap<String, TreeNode> = BTreeMap::new();

    for file in files {
        let path_parts: Vec<&str> = file.path.split('/').collect();

        let mut current = &mut root;

        // Navigate/create directories
        for (i, &part) in path_parts.iter().enumerate() {
            if i == path_parts.len() - 1 {
                // Last part - this is the file
                current.insert(part.to_string(), TreeNode::File(file));
            } else {
                // Directory part
                current =
                    match current
                        .entry(part.to_string())
                        .or_insert_with(|| TreeNode::Directory {
                            children: BTreeMap::new(),
                        }) {
                        TreeNode::Directory { children } => children,
                        _ => unreachable!(),
                    };
            }
        }
    }

    root
}

/// Render tree node recursively
fn render_tree_node(
    node: &TreeNode,
    name: &str,
    prefix: &str,
    is_last: bool,
    lscolors: &LsColors,
    use_nerd_fonts: bool,
) {
    let connector = if is_last { "└─" } else { "├─" };
    let new_prefix = if is_last { "  " } else { "│ " };

    match node {
        TreeNode::File(file) => {
            let icon = get_file_icon_for_info(file, use_nerd_fonts);
            let file_style = get_file_style(file, lscolors);

            println!(
                "{}{} {}  {} {}",
                prefix.dimmed(),
                connector.dimmed(),
                file.status_str().bold(),
                file_style.paint(icon),
                file_style.paint(name),
            );
        }
        TreeNode::Directory { children } => {
            // Print directory
            println!(
                "{}{}  {}",
                prefix.dimmed(),
                connector.dimmed(),
                name.bright_cyan().bold()
            );

            // Print children
            let child_count = children.len();
            for (idx, (child_name, child_node)) in children.iter().enumerate() {
                let is_last_child = idx == child_count - 1;
                render_tree_node(
                    child_node,
                    child_name,
                    &format!("{}{}", prefix, new_prefix),
                    is_last_child,
                    lscolors,
                    use_nerd_fonts,
                );
            }
        }
    }
}

/// Render tree format
fn render_tree(
    files: &[FileInfo],
    show_all: bool,
    is_single_file: bool,
    lscolors: &LsColors,
    use_nerd_fonts: bool,
) {
    let latent = files
        .iter()
        .filter(|f| f.status == FileStatus::Latent)
        .count();
    let ahead = files
        .iter()
        .filter(|f| f.status == FileStatus::Ahead)
        .count();
    let behind = files
        .iter()
        .filter(|f| f.status == FileStatus::Behind)
        .count();
    let conflict = files
        .iter()
        .filter(|f| f.status == FileStatus::Conflict)
        .count();
    let steady = files
        .iter()
        .filter(|f| f.status == FileStatus::Steady)
        .count();

    // Print header with status counts (inline abbreviations)
    // Skip header for single file view
    if !is_single_file {
        println!();
    }

    if !is_single_file && show_all {
        let status_items = vec![
            (latent, FileStatus::Latent),
            (ahead, FileStatus::Ahead),
            (behind, FileStatus::Behind),
            (conflict, FileStatus::Conflict),
            (steady, FileStatus::Steady),
        ];
        println!("  {}", format_status_line(&status_items));
    } else if !is_single_file {
        let status_items = vec![
            (latent, FileStatus::Latent),
            (ahead, FileStatus::Ahead),
            (behind, FileStatus::Behind),
            (conflict, FileStatus::Conflict),
        ];
        println!("  {}", format_status_line(&status_items));
    }

    if !is_single_file {
        println!();
    }

    // Filter files
    let filtered_files: Vec<&FileInfo> = files
        .iter()
        .filter(|f| {
            // Filter by show_all (but always show steady files in single file mode)
            if !show_all && !is_single_file && f.status == FileStatus::Steady {
                return false;
            }
            // Only show actual files, not directory entries
            f.file_type != 'D'
        })
        .collect();

    if filtered_files.is_empty() {
        if !is_single_file {
            println!("  {}", "No files to show".dimmed());
            println!();
        }
        return;
    }

    // Build and render tree
    let tree = build_tree(&filtered_files);

    if !is_single_file {
        println!("   {}", ".".bright_cyan().bold());
    }
    let node_count = tree.len();
    for (idx, (name, node)) in tree.iter().enumerate() {
        let is_last = idx == node_count - 1;
        render_tree_node(node, name, "  ", is_last, lscolors, use_nerd_fonts);
    }

    if !is_single_file {
        println!();
    }
}

/// Check and print hooks status
fn print_hooks_status(source_dir: &Path) -> Result<()> {
    use guisu_engine::database;
    use guisu_engine::hooks::HookLoader;
    use guisu_engine::state::{HookStatePersistence, RedbPersistentState};

    let hooks_dir = source_dir.join(".guisu/hooks");

    // Check if hooks directory exists
    if !hooks_dir.exists() {
        return Ok(());
    }

    // Load hooks
    let loader = HookLoader::new(source_dir);
    let collections = match loader.load() {
        Ok(c) => c,
        Err(_) => return Ok(()), // Silently skip if hooks fail to load
    };

    if collections.is_empty() {
        return Ok(());
    }

    // Load state from database
    let db_path = match database::get_db_path() {
        Ok(p) => p,
        Err(_) => return Ok(()), // Silently skip if can't get db path
    };

    let db = match RedbPersistentState::new(&db_path) {
        Ok(d) => d,
        Err(_) => return Ok(()), // Silently skip if can't open db
    };

    let persistence = HookStatePersistence::new(&db);
    let state = match persistence.load() {
        Ok(s) => s,
        Err(_) => return Ok(()), // Silently skip if can't load state
    };

    let has_changed = state.has_changed(&hooks_dir)?;

    // Only show message if hooks have changed
    if has_changed {
        println!();
        println!(
            "{} {}",
            "Hooks:".bold(),
            "Hooks have changed since last execution".yellow()
        );
        println!("  Run {} to execute hooks", "guisu hooks run".cyan());
    }

    Ok(())
}
