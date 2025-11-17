//! Hook management commands
//!
//! This module provides commands for managing and executing hooks.
//! Hooks are executed before and after applying dotfiles.

use anyhow::{Context, Result};
use guisu_config::hooks::{HookLoader, HookRunner, HookStage, TemplateRenderer};
use guisu_core::platform::CURRENT_PLATFORM;
use guisu_engine::state::{HookStatePersistence, RedbPersistentState};
use owo_colors::OwoColorize;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use crate::ui::icons::StatusIcon;
use guisu_config::Config;
use guisu_engine::database;

/// Run hooks
pub fn run_hooks(
    source_dir: &Path,
    config: &Config,
    skip_confirm: bool,
    hook_filter: Option<&str>,
) -> Result<()> {
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

    let mut collections = loader.load().context("Failed to load hooks")?;

    // Filter hooks if a specific hook name is provided
    if let Some(filter_name) = hook_filter {
        collections.pre.retain(|h| h.name == filter_name);
        collections.post.retain(|h| h.name == filter_name);

        if collections.is_empty() {
            println!("{}", format!("Hook '{}' not found.", filter_name).yellow());
            return Ok(());
        }

        println!(
            "{} Running hook: {}",
            StatusIcon::Hook.get(use_nerd_fonts),
            filter_name.cyan()
        );
    }

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

    // Load persistent state for hook execution tracking
    let db_path = database::get_db_path()?;
    let db = RedbPersistentState::new(&db_path).context("Failed to open database")?;
    let persistence = HookStatePersistence::new(&db);
    let mut state = persistence.load()?;

    // Create template renderer
    let renderer = create_template_engine(source_dir, config)?;

    // Create hook runner with builder pattern (recommended API)
    let runner = HookRunner::builder(&collections, source_dir)
        .template_renderer(renderer)
        .persistent_state(state.once_executed.clone(), state.onchange_hashes.clone())
        .build();

    // Run hooks in stages
    println!("\n{}", "Running pre hooks...".bold());
    runner
        .run_stage(HookStage::Pre)
        .context("Pre hooks failed")?;

    println!("\n{}", "Running post hooks...".bold());
    runner
        .run_stage(HookStage::Post)
        .context("Post hooks failed")?;

    // Get newly executed hooks and merge with state
    for hook_name in runner.get_once_executed() {
        state.mark_executed_once(hook_name);
    }
    for (hook_name, content_hash) in runner.get_onchange_hashes() {
        state.update_onchange_hash(hook_name, content_hash);
    }

    // Update state in database
    let hooks_dir = source_dir.join(".guisu/hooks");
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

/// Show detailed information about a specific hook
pub fn run_show(source_dir: &Path, config: &Config, hook_name: &str) -> Result<()> {
    let is_tty = std::io::stdout().is_terminal();
    let use_nerd_fonts = config.ui.icons.should_show_icons(is_tty);

    // Load hooks using HookLoader
    let loader = HookLoader::new(source_dir);

    if !loader.exists() {
        println!("{}", "No hooks directory found.".yellow());
        return Ok(());
    }

    let collections = loader.load().context("Failed to load hooks")?;
    let platform = CURRENT_PLATFORM.os;

    // Search for the hook in both pre and post collections
    let hook = collections
        .pre
        .iter()
        .chain(collections.post.iter())
        .find(|h| h.name == hook_name);

    if let Some(hook) = hook {
        // Determine stage
        let stage = if collections.pre.iter().any(|h| h.name == hook_name) {
            "pre"
        } else {
            "post"
        };

        println!();
        println!("{} {}", "Hook:".bold(), hook.name.cyan());
        println!("{} {}", "Stage:".bold(), stage);
        println!("{} {}", "Order:".bold(), hook.order);
        println!("{} {:?}", "Mode:".bold(), hook.mode);

        // Platform information
        if hook.platforms.is_empty() {
            println!("{} All platforms", "Platforms:".bold());
        } else {
            println!(
                "{} {}",
                "Platforms:".bold(),
                hook.platforms
                    .iter()
                    .map(|p| format!("{:?}", p))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }

        // Check if should run on current platform
        if hook.should_run_on(platform) {
            println!(
                "{} {} {}",
                "Status:".bold(),
                StatusIcon::Success.get(use_nerd_fonts),
                "Will run on this platform".green()
            );
        } else {
            println!(
                "{} {} {}",
                "Status:".bold(),
                StatusIcon::Warning.get(use_nerd_fonts),
                "Skipped on this platform".dimmed()
            );
        }

        // Command or script
        if let Some(ref cmd) = hook.cmd {
            println!("{} {}", "Command:".bold(), cmd);
        } else if let Some(ref script) = hook.script {
            println!("{} {}", "Script:".bold(), script);

            // Try to read and display script content
            // Script path is relative to source_dir (already includes .guisu/hooks/stage/)
            let script_path = source_dir.join(script);

            if script_path.exists()
                && let Ok(content) = std::fs::read_to_string(&script_path)
            {
                println!("\n{}", "Script content:".bold());
                println!("{}", "─".repeat(60).dimmed());
                println!("{}", content.dimmed());
                println!("{}", "─".repeat(60).dimmed());
            }
        }

        // Working directory
        if let Some(ref working_dir) = hook.working_dir {
            println!("{} {}", "Working directory:".bold(), working_dir);
        }

        // Environment variables
        if !hook.env.is_empty() {
            println!("\n{}", "Environment variables:".bold());
            for (key, value) in &hook.env {
                println!("  {} = {}", key.cyan(), value);
            }
        }

        // Timeout
        println!("{} {} seconds", "Timeout:".bold(), hook.timeout_secs);

        println!();
    } else {
        println!(
            "{} {}",
            StatusIcon::Warning.get(use_nerd_fonts),
            format!("Hook '{}' not found.", hook_name).yellow()
        );
        println!(
            "\nUse {} to list all available hooks.",
            "guisu hooks list".cyan()
        );
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

    // Create template renderer
    let renderer = create_template_engine(source_dir, config)?;

    // Create hook runner with builder pattern and run pre hooks
    let runner = HookRunner::builder(&collections, source_dir)
        .template_renderer(renderer)
        .build();
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

    // Create template renderer
    let renderer = create_template_engine(source_dir, config)?;

    // Create hook runner with builder pattern and run post hooks
    let runner = HookRunner::builder(&collections, source_dir)
        .template_renderer(renderer)
        .build();
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

/// Create a template renderer closure for hooks
fn create_template_engine(source_dir: &Path, config: &Config) -> Result<impl TemplateRenderer> {
    use guisu_template::{TemplateContext, TemplateEngine};

    // Load age identities for encryption support in templates
    let identities = config.age_identities().unwrap_or_else(|_| Vec::new());

    // Get template directory path
    let template_dir = source_dir.join(".guisu/templates");
    let template_dir = if template_dir.exists() {
        Some(template_dir)
    } else {
        None
    };

    // Create template engine with identities and template directory
    let engine = TemplateEngine::with_identities_and_template_dir(identities, template_dir);

    // Get destination directory (use home_dir as default if not configured)
    let dst_dir = config
        .general
        .dst_dir
        .clone()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("~"));

    // Create template context with guisu info
    let context = TemplateContext::new().with_guisu_info(
        source_dir.display().to_string(),
        dst_dir.display().to_string(),
        config.general.root_entry.display().to_string(),
    );

    // Return a closure that captures both engine and context
    // No Box needed - the closure implements TemplateRenderer directly
    Ok(move |content: &str| {
        engine
            .render_str(content, &context)
            .map_err(|e| guisu_core::Error::Message(format!("Template rendering error: {}", e)))
    })
}
