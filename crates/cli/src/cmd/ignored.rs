//! Ignored command operations
//!
//! This module provides commands for viewing ignored files and patterns:
//! - list: List files that are ignored on the current platform
//! - show: Show ignore rules for the current platform

use anyhow::{Context, Result};
use guisu_config::IgnoreMatcher;
use guisu_config::IgnoresConfig;
use guisu_core::platform::CURRENT_PLATFORM;
use lscolors::{LsColors, Style};
use std::io::IsTerminal;
use std::path::Path;

use guisu_config::Config;

/// Run ignored list command
///
/// Lists all files in the source directory that would be ignored on the current platform.
/// Shows the target file paths (after removing .j2, .age suffixes).
/// This includes entries ignored by:
/// - Global patterns from global section
/// - Platform-specific patterns from `<platform>` section
pub fn run_list(source_dir: &Path, config: &Config) -> Result<()> {
    let platform = CURRENT_PLATFORM.os;
    use guisu_engine::entry::SourceEntry;
    use guisu_engine::state::SourceState;
    use owo_colors::OwoColorize;

    // Detect if output is to a terminal for icon auto mode
    let is_tty = std::io::stdout().is_terminal();
    let show_icons = config.ui.icons.should_show_icons(is_tty);

    // Resolve dotfiles directory (handles root_entry and canonicalization)
    // Note: We don't need dest_dir for this command, but we pass home_dir for consistency
    let paths = crate::common::ResolvedPaths::resolve(
        source_dir,
        &dirs::home_dir().unwrap_or_default(),
        config,
    )?;
    let source_abs = &paths.dotfiles_dir;

    // Load ignore patterns from source_dir/.guisu/ignores.toml
    // Use dotfiles_dir as the match root so patterns match relative to the dotfiles directory
    let matcher = IgnoreMatcher::from_ignores_toml(source_dir)
        .context("Failed to load ignore patterns from .guisu/ignores.toml")?;

    // Read ALL source files (without filtering by ignore patterns)
    let source_state =
        SourceState::read(source_abs.to_owned()).context("Failed to read source state")?;

    // Collect all ignored files with their target paths
    let mut ignored_files = Vec::new();

    for entry in source_state.entries() {
        // Only process files
        if let SourceEntry::File { .. } = entry {
            // Get the target path (after processing .j2, .age, etc.)
            let target_path = entry.target_path();

            // Check if target path or any parent directory is ignored
            // Use relative path for matching (patterns are relative paths)
            let mut is_ignored = false;

            // Check the file itself (use relative path)
            if matcher.is_ignored(target_path.as_path()) {
                is_ignored = true;
            }

            // Check each parent directory (use relative paths)
            if !is_ignored {
                let mut current = target_path.as_path();
                while let Some(parent) = current.parent() {
                    // Stop at root (empty path)
                    if parent.as_os_str().is_empty() {
                        break;
                    }

                    if matcher.is_ignored(parent) {
                        is_ignored = true;
                        break;
                    }

                    current = parent;
                }
            }

            if is_ignored {
                ignored_files.push(target_path.to_string());
            }
        }
    }

    // Display results
    if ignored_files.is_empty() {
        println!("{}\n", format!("Ignored 0 files on {}", platform).dimmed());
    } else {
        let count = ignored_files.len();

        println!(
            "{} {} {} {}\n",
            "Ignored".bright_white(),
            count.to_string().bright_cyan(),
            "files on".bright_white(),
            platform.bright_cyan()
        );

        // Sort for consistent output
        ignored_files.sort();

        // Initialize lscolors from environment for file coloring
        let lscolors = LsColors::from_env().unwrap_or_default();

        for file in &ignored_files {
            // Add ~/ prefix to path
            let display_path = format!("~/{}", file);

            // Create FileIconInfo to get icon
            let icon_info = crate::ui::icons::FileIconInfo {
                path: display_path.as_str(),
                is_directory: false,
                is_symlink: false,
            };

            let icon = crate::ui::icons::icon_for_file(&icon_info, show_icons);

            // Get color style for file based on its extension/type
            let file_style = lscolors
                .style_for_path(&display_path)
                .map(Style::to_nu_ansi_term_style)
                .unwrap_or_default();

            // Apply style to both icon and path
            let styled_icon = file_style.paint(icon);
            let styled_path = file_style.paint(&display_path);

            println!("  {} {}", styled_icon, styled_path);
        }
    }

    Ok(())
}

/// Run ignored show command
///
/// Shows the ignore rules that apply to the current platform.
/// This reads from .guisu/ignores.toml in the source directory.
pub fn run_show(source_dir: &Path, _config: &Config, show_all: bool) -> Result<()> {
    use owo_colors::OwoColorize;

    let platform = CURRENT_PLATFORM.os;

    // Load ignore config from source_dir/.guisu/ignores.toml
    // Note: ignores.toml is always in source_dir, not in dotfiles_dir
    // (dotfiles_dir might be source_dir/root_entry if root_entry is set)
    let ignores_config =
        IgnoresConfig::load(source_dir).context("Failed to load .guisu/ignores.toml")?;

    // Helper to display a section
    let display_section = |section_name: &str, patterns: &[String], is_current: bool| {
        if is_current {
            println!("{}:", format!("{} (current)", section_name).bright_white());
        } else {
            println!("{}:", section_name.bright_white());
        }

        if patterns.is_empty() {
            println!("  {}", "(none)".dimmed());
        } else {
            for pattern in patterns {
                println!("  {}", pattern);
            }
        }
    };

    if show_all {
        // Show all platforms
        println!(
            "{} {}\n",
            "Current platform:".bright_white(),
            platform.bright_cyan()
        );

        // Global patterns
        display_section("global", &ignores_config.global, false);
        println!();

        // All platform sections
        display_section("darwin", &ignores_config.darwin, platform == "darwin");
        println!();
        display_section("linux", &ignores_config.linux, platform == "linux");
        println!();
        display_section("windows", &ignores_config.windows, platform == "windows");
    } else {
        // Show only current platform
        println!(
            "{} {}\n",
            "Platform:".bright_white(),
            platform.bright_cyan()
        );

        // Show global patterns if any
        if !ignores_config.global.is_empty() {
            display_section("global", &ignores_config.global, false);
            println!();
        }

        // Show platform-specific patterns
        let empty_vec = Vec::new();
        let platform_patterns = match platform {
            "darwin" => &ignores_config.darwin,
            "linux" => &ignores_config.linux,
            "windows" => &ignores_config.windows,
            _ => &empty_vec,
        };

        display_section(platform, platform_patterns, true);
    }

    Ok(())
}
