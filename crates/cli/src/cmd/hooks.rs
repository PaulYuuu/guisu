//! Hook management commands
//!
//! This module provides commands for managing and executing hooks.
//! Hooks are executed before and after applying dotfiles.

use anyhow::{Context, Result};
use guisu_config::hooks::{HookLoader, HookRunner, HookStage};
use guisu_core::platform::CURRENT_PLATFORM;
use guisu_engine::state::{HookStatePersistence, RedbPersistentState};
use owo_colors::OwoColorize;
use std::io::IsTerminal;
use std::path::Path;

use crate::ui::icons::StatusIcon;
use guisu_config::Config;
use guisu_engine::database;

/// Run hooks
pub fn run_hooks(source_dir: &Path, config: &Config, skip_confirm: bool) -> Result<()> {
    let is_tty = std::io::stdout().is_terminal();
    let use_nerd_fonts = config.ui.icons.should_show_icons(is_tty);
    // Load hooks using HookLoader
    let loader = HookLoader::new(source_dir);

    if !loader.exists() {
        println!("{}", "No hooks directory found.".yellow());
        println!("Create .guisu/hooks/pre/ and .guisu/hooks/post/ directories to get started.");
        println!("\nExample structure:");
        println!(
            "{}",
            r#"
.guisu/hooks/
  pre/
    01-setup.sh          # Script to run before applying
    02-install.toml      # Hook configuration
  post/
    01-cleanup.sh        # Script to run after applying
    99-notify.toml       # Notification hook
"#
            .dimmed()
        );
        return Ok(());
    }

    let collections = loader.load().context("Failed to load hooks")?;

    if collections.is_empty() {
        println!("{}", "No hooks configured.".yellow());
        return Ok(());
    }

    let platform = CURRENT_PLATFORM.os;
    let total_hooks = collections.total();

    println!(
        "{} Hooks directory: {}",
        StatusIcon::Hook.get(use_nerd_fonts),
        source_dir.join(".guisu/hooks").display().cyan()
    );
    println!("Platform: {}", platform.cyan());
    println!("Total hooks: {}", total_hooks);
    println!("  Pre hooks: {}", collections.pre.len());
    println!("  Post hooks: {}", collections.post.len());

    // Confirm unless --yes is specified
    if !skip_confirm {
        use dialoguer::{Confirm, theme::ColorfulTheme};

        let confirmed = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("Run hooks?")
            .default(true)
            .interact()?;

        if !confirmed {
            println!("Cancelled.");
            return Ok(());
        }
    }

    // Create hook runner
    let runner = HookRunner::new(&collections, source_dir);

    // Run hooks in stages
    println!("\n{}", "Running pre hooks...".bold());
    runner
        .run_stage(HookStage::Pre)
        .context("Pre hooks failed")?;

    println!("\n{}", "Running post hooks...".bold());
    runner
        .run_stage(HookStage::Post)
        .context("Post hooks failed")?;

    // Update state in database
    let db_path = database::get_db_path()?;
    let db = RedbPersistentState::new(&db_path).context("Failed to open database")?;
    let persistence = HookStatePersistence::new(&db);

    // For directory-based hooks, we compute a hash of the directory structure
    let hooks_dir = source_dir.join(".guisu/hooks");
    let mut state = persistence.load()?;
    state
        .update(&hooks_dir)
        .context("Failed to update hook state")?;

    persistence
        .save(&state)
        .context("Failed to save hook state")?;

    println!(
        "\n{} {}",
        StatusIcon::Success.get(use_nerd_fonts),
        "All hooks completed!".green().bold()
    );

    Ok(())
}

/// List configured hooks
pub fn run_list(source_dir: &Path, _config: &Config, format: &str) -> Result<()> {
    // Load hooks using HookLoader
    let loader = HookLoader::new(source_dir);

    if !loader.exists() {
        println!("{}", "No hooks directory found.".yellow());
        return Ok(());
    }

    let collections = loader.load().context("Failed to load hooks")?;

    let platform = CURRENT_PLATFORM.os;

    match format {
        "json" => {
            // JSON output
            let json = serde_json::json!({
                "hooks_dir": source_dir.join(".guisu/hooks"),
                "platform": platform,
                "hooks": {
                    "pre": collections.pre,
                    "post": collections.post,
                },
            });
            println!("{}", serde_json::to_string_pretty(&json)?);
        }
        _ => {
            // Simple output
            println!(
                "Hooks directory: {}",
                source_dir.join(".guisu/hooks").display().cyan()
            );
            println!("Platform: {}", platform.cyan());
            println!();

            println!("{} ({} hooks)", "Pre hooks:".bold(), collections.pre.len());
            for hook in &collections.pre {
                if hook.should_run_on(platform) {
                    println!("  • {} (order: {})", hook.name.green(), hook.order);
                } else {
                    println!(
                        "  • {} (order: {}) {}",
                        hook.name.dimmed(),
                        hook.order,
                        "[skipped]".dimmed()
                    );
                }
            }

            println!(
                "\n{} ({} hooks)",
                "Post hooks:".bold(),
                collections.post.len()
            );
            for hook in &collections.post {
                if hook.should_run_on(platform) {
                    println!("  • {} (order: {})", hook.name.green(), hook.order);
                } else {
                    println!(
                        "  • {} (order: {}) {}",
                        hook.name.dimmed(),
                        hook.order,
                        "[skipped]".dimmed()
                    );
                }
            }
        }
    }

    Ok(())
}

/// Check hook configuration status
pub fn run_check(source_dir: &Path, config: &Config, format: &str) -> Result<()> {
    let is_tty = std::io::stdout().is_terminal();
    let use_nerd_fonts = config.ui.icons.should_show_icons(is_tty);
    // Load hooks using HookLoader
    let loader = HookLoader::new(source_dir);

    if !loader.exists() {
        println!("{}", "No hooks directory found.".yellow());
        return Ok(());
    }

    let collections = loader.load().context("Failed to load hooks")?;

    // Load state from database
    let db_path = database::get_db_path()?;
    let db = RedbPersistentState::new(&db_path).context("Failed to open database")?;
    let persistence = HookStatePersistence::new(&db);
    let state = persistence.load()?;

    let hooks_dir = source_dir.join(".guisu/hooks");
    let has_changed = state.has_changed(&hooks_dir)?;
    let platform = CURRENT_PLATFORM.os;

    match format {
        "json" => {
            let json = serde_json::json!({
                "hooks_dir": hooks_dir,
                "platform": platform,
                "has_changed": has_changed,
                "last_executed": state.last_executed,
                "total_hooks": collections.total(),
            });
            println!("{}", serde_json::to_string_pretty(&json)?);
        }
        _ => {
            println!("Hooks directory: {}", hooks_dir.display().cyan());
            println!("Platform: {}", platform.cyan());
            println!();

            if has_changed {
                println!(
                    "{} {}",
                    StatusIcon::Warning.get(use_nerd_fonts),
                    "Hooks have changed since last execution".yellow().bold()
                );
                println!("Run {} to execute hooks", "guisu hooks run".cyan());
            } else {
                println!(
                    "{} {}",
                    StatusIcon::Success.get(use_nerd_fonts),
                    "Hooks are up to date".green()
                );
            }

            let total_hooks = collections.total();

            println!("\nTotal hooks: {}", total_hooks);
            println!("  Pre: {}", collections.pre.len());
            println!("  Post: {}", collections.post.len());
        }
    }

    Ok(())
}

/// Handle hooks during apply (auto-run if hooks changed)
pub fn handle_hooks_pre(source_dir: &Path, config: &Config) -> Result<()> {
    let is_tty = std::io::stdout().is_terminal();
    let use_nerd_fonts = config.ui.icons.should_show_icons(is_tty);
    // Load hooks using HookLoader
    let loader = HookLoader::new(source_dir);

    if !loader.exists() {
        tracing::debug!("No hooks directory found, skipping");
        return Ok(());
    }

    let collections = loader.load().context("Failed to load hooks")?;

    if collections.pre.is_empty() {
        tracing::debug!("No pre hooks configured, skipping");
        return Ok(());
    }

    println!(
        "{} Running pre-apply hooks...",
        StatusIcon::Hook.get(use_nerd_fonts)
    );

    // Create hook runner and run pre hooks
    let runner = HookRunner::new(&collections, source_dir);
    runner.run_stage(HookStage::Pre)?;

    println!(
        "{} {}",
        StatusIcon::Success.get(use_nerd_fonts),
        "Pre hooks completed!".green()
    );

    Ok(())
}

/// Handle hooks after apply
pub fn handle_hooks_post(source_dir: &Path, config: &Config) -> Result<()> {
    let is_tty = std::io::stdout().is_terminal();
    let use_nerd_fonts = config.ui.icons.should_show_icons(is_tty);
    // Load hooks using HookLoader
    let loader = HookLoader::new(source_dir);

    if !loader.exists() {
        tracing::debug!("No hooks directory found, skipping");
        return Ok(());
    }

    let collections = loader.load().context("Failed to load hooks")?;

    if collections.post.is_empty() {
        tracing::debug!("No post hooks configured, skipping");
        return Ok(());
    }

    println!(
        "{} Running post-apply hooks...",
        StatusIcon::Hook.get(use_nerd_fonts)
    );

    // Create hook runner and run post hooks
    let runner = HookRunner::new(&collections, source_dir);
    runner.run_stage(HookStage::Post)?;

    // Update state in database
    let db_path = database::get_db_path()?;
    let db = RedbPersistentState::new(&db_path).context("Failed to open database")?;
    let persistence = HookStatePersistence::new(&db);

    let hooks_dir = source_dir.join(".guisu/hooks");
    let mut state = persistence.load()?;
    state.update(&hooks_dir)?;
    persistence.save(&state)?;

    println!(
        "{} {}",
        StatusIcon::Success.get(use_nerd_fonts),
        "Post hooks completed!".green()
    );

    Ok(())
}
