//! Init command implementation
//!
//! Initialize a new guisu source directory or clone from GitHub.

use anyhow::{Context, Result, anyhow};
use git2::{FetchOptions, RemoteCallbacks, Repository, SubmoduleUpdateOptions};
use indicatif::{ProgressBar, ProgressStyle};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

/// Run the init command
///
/// Returns the path to the initialized source directory if successful
///
/// # Errors
///
/// Returns an error if:
/// - The target directory cannot be determined
/// - Git cloning fails
/// - Local directory initialization fails
///
/// # Panics
///
/// Panics if `path_or_repo` is `None` when `is_clone` is `true`.
/// This should never happen due to the logic in `determine_init_target`.
pub fn run(
    path_or_repo: Option<&str>,
    custom_source: Option<&Path>,
    depth: Option<usize>,
    branch: Option<&str>,
    use_ssh: bool,
    recurse_submodules: bool,
) -> Result<Option<PathBuf>> {
    let (target_path, is_clone) = determine_init_target(path_or_repo, custom_source)?;
    debug!(path = %target_path.display(), is_clone, "Initializing guisu");

    if is_clone {
        // Clone from GitHub
        let repo_url = path_or_repo.expect("path_or_repo is Some when is_clone is true");
        clone_from_github(
            repo_url,
            &target_path,
            depth,
            branch,
            use_ssh,
            recurse_submodules,
        )?;
        println!("\nGuisu dotfiles cloned successfully!");
        println!("Source directory: {}", target_path.display());
        println!("\nNext steps:");
        println!("  1. Review files: guisu status");
        println!("  2. Apply changes: guisu apply");
        return Ok(Some(target_path));
    }

    // Initialize local directory
    initialize_local_directory(&target_path)?;
    Ok(Some(target_path))
}

/// Determine the target path and whether we're cloning from GitHub
fn determine_init_target(
    path_or_repo: Option<&str>,
    custom_source: Option<&Path>,
) -> Result<(PathBuf, bool)> {
    match path_or_repo {
        None => {
            // Default: use custom source or XDG data directory
            let target = custom_source
                .map(std::path::Path::to_path_buf)
                .or_else(guisu_config::dirs::data_dir)
                .ok_or_else(|| anyhow!("Could not determine data directory"))?;
            Ok((target, false))
        }
        Some(input) => {
            // Check if it looks like a GitHub reference
            if is_github_reference(input) {
                // Use custom source or XDG data directory for cloned repos
                let target = custom_source
                    .map(std::path::Path::to_path_buf)
                    .or_else(guisu_config::dirs::data_dir)
                    .ok_or_else(|| anyhow!("Could not determine data directory"))?;
                Ok((target, true))
            } else {
                // Explicit local path (overrides custom_source)
                Ok((PathBuf::from(input), false))
            }
        }
    }
}

/// Check if the input looks like a GitHub reference (username or owner/repo)
fn is_github_reference(input: &str) -> bool {
    // Don't treat paths as GitHub references
    if input.starts_with('/') || input.starts_with('.') || input.contains('\\') {
        return false;
    }

    // Check if it looks like username or owner/repo
    // Simple heuristic: contains only alphanumeric, dash, underscore, and at most one slash
    let slash_count = input.matches('/').count();
    if slash_count > 1 {
        return false;
    }

    // Check if all characters are valid GitHub username/repo characters
    input
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '/')
}

/// Clone a repository from GitHub
fn clone_from_github(
    repo_ref: &str,
    target_path: &Path,
    depth: Option<usize>,
    branch: Option<&str>,
    use_ssh: bool,
    recurse_submodules: bool,
) -> Result<()> {
    // Build the full repository URL
    let repo_url = if use_ssh {
        // Use SSH URL format
        if repo_ref.contains('/') {
            format!("git@github.com:{repo_ref}.git")
        } else {
            format!("git@github.com:{repo_ref}/dotfiles.git")
        }
    } else {
        // Use HTTPS URL format (default)
        if repo_ref.contains('/') {
            format!("https://github.com/{repo_ref}.git")
        } else {
            format!("https://github.com/{repo_ref}/dotfiles.git")
        }
    };

    info!(
        url = %repo_url,
        path = %target_path.display(),
        depth = ?depth,
        branch = ?branch,
        use_ssh,
        recurse_submodules,
        "Cloning repository"
    );
    println!("Cloning from: {repo_url}");
    if let Some(d) = depth {
        println!("  Depth: {} commit{}", d, if d == 1 { "" } else { "s" });
    }
    if let Some(b) = branch {
        println!("  Branch: {b}");
    }
    if recurse_submodules {
        println!("  Recurse submodules: yes");
    }
    println!("Target directory: {}", target_path.display());

    // Remove target directory if it exists and is not empty
    if target_path.exists() && target_path.read_dir()?.next().is_some() {
        return Err(anyhow!(
            "Target directory is not empty: {}",
            target_path.display()
        ));
    }

    // Set up progress bar
    let progress_bar = ProgressBar::new(100);
    progress_bar.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}% {msg}")
            .expect("Invalid progress bar template")
            .progress_chars("#>-"),
    );

    // Set up git2 callbacks for progress reporting
    let mut callbacks = RemoteCallbacks::new();
    callbacks.transfer_progress(|stats| {
        let received = stats.received_objects();
        let total = stats.total_objects();

        if total > 0 {
            #[allow(
                clippy::cast_precision_loss,
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss
            )]
            let percentage = (received as f64 / total as f64 * 100.0) as u64;
            progress_bar.set_position(percentage);

            if stats.received_bytes() > 0 {
                #[allow(clippy::cast_precision_loss)]
                let mb = stats.received_bytes() as f64 / 1_048_576.0;
                progress_bar.set_message(format!("{received}/{total} objects ({mb:.2} MiB)"));
            } else {
                progress_bar.set_message(format!("{received}/{total} objects"));
            }
        }

        true
    });

    // Configure fetch options with callbacks
    let mut fetch_options = FetchOptions::new();
    fetch_options.remote_callbacks(callbacks);

    // Set depth for shallow clone if specified
    if let Some(depth_value) = depth {
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        fetch_options.depth(depth_value as i32);
    }

    // Clone the repository
    debug!(url = %repo_url, path = %target_path.display(), "Starting git clone");
    let mut builder = git2::build::RepoBuilder::new();
    builder.fetch_options(fetch_options);

    // Set branch if specified
    if let Some(branch_name) = branch {
        builder.branch(branch_name);
    }

    let repo = builder
        .clone(&repo_url, target_path)
        .with_context(|| {
            progress_bar.finish_and_clear();
            format!(
                "Failed to clone repository from {repo_url}. Make sure the repository exists and you have access."
            )
        })?;

    progress_bar.finish_with_message("Clone complete");
    info!(path = %target_path.display(), "Repository cloned successfully");

    // Initialize submodules recursively if requested
    if recurse_submodules {
        debug!("Initializing submodules recursively");
        println!("\nInitializing submodules...");

        init_submodules_recursive(&repo, target_path)?;

        info!("Submodules initialized successfully");
        println!("✓ Submodules initialized");
    }

    Ok(())
}

/// Initialize submodules recursively using git2
fn init_submodules_recursive(repo: &Repository, repo_path: &Path) -> Result<()> {
    let submodules = repo.submodules().context("Failed to get submodules")?;

    if submodules.is_empty() {
        debug!("No submodules found");
        return Ok(());
    }

    debug!(count = submodules.len(), "Found submodules");

    for mut submodule in submodules {
        let name = submodule.name().unwrap_or("<unnamed>").to_string();
        let path = submodule.path().to_path_buf();
        debug!(name = %name, path = %path.display(), "Initializing submodule");

        // Initialize the submodule
        submodule
            .init(false)
            .with_context(|| format!("Failed to initialize submodule '{name}'"))?;

        // Update the submodule (clone and checkout)
        let mut update_options = SubmoduleUpdateOptions::new();

        // Set up fetch options with callbacks
        let mut fetch_options = FetchOptions::new();
        let mut callbacks = RemoteCallbacks::new();
        callbacks.transfer_progress(|_| true);
        fetch_options.remote_callbacks(callbacks);

        update_options.fetch(fetch_options);

        submodule
            .update(true, Some(&mut update_options))
            .with_context(|| format!("Failed to update submodule '{name}'"))?;

        println!("  Submodule: {name}");

        // Recursively initialize submodules of this submodule
        let submodule_path = repo_path.join(&path);
        if let Ok(sub_repo) = Repository::open(&submodule_path) {
            init_submodules_recursive(&sub_repo, &submodule_path)?;
        }
    }

    Ok(())
}

/// Initialize a local directory
fn initialize_local_directory(path: &Path) -> Result<()> {
    info!(path = %path.display(), "Initializing source directory");
    println!("Initializing guisu source directory at: {}", path.display());

    // Create the directory if it doesn't exist
    if path.exists() {
        debug!(path = %path.display(), "Directory already exists");
        println!("Directory already exists: {}", path.display());
    } else {
        fs::create_dir_all(path)
            .with_context(|| format!("Failed to create directory: {}", path.display()))?;
        debug!(path = %path.display(), "Created directory");
        println!("Created directory: {}", path.display());
    }

    println!("\nGuisu source directory initialized successfully!");
    println!("\nNext steps:");
    println!("  1. Add files: guisu add ~/.bashrc");
    println!("  2. Apply changes: guisu apply");

    Ok(())
}
