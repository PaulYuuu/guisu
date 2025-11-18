//! Update command implementation
//!
//! Pull the latest changes from the source repository and optionally apply them.

use anyhow::{Context, Result, anyhow};
use clap::Args;
use git2::{AnnotatedCommit, AutotagOption, FetchOptions, RemoteCallbacks, Repository};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;
use tracing::{debug, info, warn};

use crate::command::Command;
use crate::common::RuntimeContext;
use guisu_config::Config;

/// Update command
#[derive(Args)]
pub struct UpdateCommand {
    /// Apply changes after pulling (default: true)
    #[arg(short, long, default_value_t = true)]
    pub apply: bool,

    /// Use rebase instead of merge when branches have diverged
    #[arg(short, long)]
    pub rebase: bool,
}

impl Command for UpdateCommand {
    type Output = ();
    fn execute(&self, context: &RuntimeContext) -> crate::error::Result<()> {
        run_impl(
            context.source_dir(),
            context.dest_dir().as_path(),
            self.apply,
            self.rebase,
            &context.config,
        )
        .map_err(Into::into)
    }
}

/// Run the update command implementation
///
/// Pulls the latest changes from the remote repository and optionally applies them.
fn run_impl(
    source_dir: &Path,
    dest_dir: &Path,
    apply: bool,
    rebase: bool,
    config: &Config,
) -> Result<()> {
    let dotfiles_dir = config.dotfiles_dir(source_dir);

    info!(path = %source_dir.display(), "Updating repository");
    println!("Updating from: {}", source_dir.display());

    // Verify source directory exists
    if !source_dir.exists() {
        return Err(anyhow!(
            "Source directory does not exist: {}",
            source_dir.display()
        ));
    }

    // Open the repository
    let repo = Repository::open(source_dir).with_context(|| {
        format!(
            "Failed to open git repository at {}. Did you initialize with 'guisu init'?",
            source_dir.display()
        )
    })?;

    // Get the remote (default to "origin")
    let mut remote = repo
        .find_remote("origin")
        .with_context(|| "No remote 'origin' found. Make sure this repository was cloned.")?;

    // Set up progress bar for fetch
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
            let percentage = (received as f64 / total as f64 * 100.0) as u64;
            progress_bar.set_position(percentage);

            if stats.received_bytes() > 0 {
                let mb = stats.received_bytes() as f64 / 1_048_576.0;
                progress_bar.set_message(format!("{}/{} objects ({:.2} MiB)", received, total, mb));
            } else {
                progress_bar.set_message(format!("{}/{} objects", received, total));
            }
        }

        true
    });

    // Configure fetch options with callbacks
    let mut fetch_options = FetchOptions::new();
    fetch_options.remote_callbacks(callbacks);
    fetch_options.download_tags(AutotagOption::Auto);

    // Fetch from remote
    debug!("Fetching from remote origin");
    progress_bar.set_message("Fetching updates...");

    remote
        .fetch(&["HEAD"], Some(&mut fetch_options), None)
        .with_context(|| "Failed to fetch from remote. Check your network connection.")?;

    progress_bar.finish_and_clear();

    // Get FETCH_HEAD to analyze what was fetched
    let fetch_head = repo
        .find_reference("FETCH_HEAD")
        .context("Failed to find FETCH_HEAD after fetch")?;

    let fetch_commit = repo
        .reference_to_annotated_commit(&fetch_head)
        .context("Failed to get fetch commit")?;

    // Analyze the fetch
    let analysis = repo
        .merge_analysis(&[&fetch_commit])
        .context("Failed to analyze merge")?;

    // Handle different merge scenarios
    if analysis.0.is_up_to_date() {
        info!("Already up to date");
        println!("✓ Already up to date");
        return Ok(());
    }

    if analysis.0.is_fast_forward() {
        // Fast-forward merge
        debug!("Performing fast-forward merge");
        perform_fast_forward(&repo, &fetch_commit)
            .context("Failed to perform fast-forward merge")?;

        let commit_count = count_new_commits(&repo, &fetch_commit)?;
        info!(commits = commit_count, "Successfully updated");
        println!(
            "✓ Updated successfully ({} new commit{})",
            commit_count,
            if commit_count == 1 { "" } else { "s" }
        );
    } else if analysis.0.is_normal() {
        // Normal merge required - branches have diverged
        if rebase {
            // Perform rebase
            debug!("Performing rebase");
            println!("Branches have diverged. Rebasing local changes...");

            perform_rebase(&repo, &fetch_commit).context("Failed to perform rebase")?;

            let commit_count = count_new_commits(&repo, &fetch_commit)?;
            info!(commits = commit_count, "Successfully rebased and updated");
            println!(
                "✓ Rebased successfully ({} new commit{})",
                commit_count,
                if commit_count == 1 { "" } else { "s" }
            );
        } else {
            warn!("Manual merge required");
            return Err(anyhow!(
                "Cannot fast-forward. Your local repository has diverged from the remote.\n\
                Please resolve this manually:\n\
                  cd {}\n\
                  git pull --rebase\n\
                \n\
                Or use: guisu update --rebase",
                source_dir.display()
            ));
        }
    } else {
        return Err(anyhow!(
            "Unknown merge state. Please update manually:\n\
              cd {}\n\
              git pull",
            source_dir.display()
        ));
    }

    // Apply changes if requested
    if apply {
        println!("\nApplying changes...");

        // Create ApplyCommand with default options (all files)
        let apply_cmd = crate::cmd::apply::ApplyCommand {
            files: vec![],
            dry_run: false,
            force: false,
            interactive: false,
            include: vec![],
            exclude: vec![],
        };

        // Create RuntimeContext and execute
        let context = crate::common::RuntimeContext::new(config.clone(), &dotfiles_dir, dest_dir)?;
        apply_cmd
            .execute(&context)
            .context("Failed to apply changes")?;
    } else {
        println!("\nTo apply these changes, run:");
        println!("  guisu apply");
    }

    Ok(())
}

/// Perform a fast-forward merge
fn perform_fast_forward(repo: &Repository, fetch_commit: &AnnotatedCommit) -> Result<()> {
    // Get the commit object
    let commit_id = fetch_commit.id();

    // Update HEAD to point to the new commit
    let refname = "refs/heads/main";
    let refname_alt = "refs/heads/master";

    // Try to find the current branch reference
    let reference = repo
        .find_reference("HEAD")
        .context("Failed to find HEAD reference")?;

    let reference = if reference.is_branch() {
        reference
    } else {
        // Detached HEAD, try to find main or master branch
        repo.find_reference(refname)
            .or_else(|_| repo.find_reference(refname_alt))
            .context("Failed to find main/master branch")?
    };

    let ref_name = reference
        .name()
        .ok_or_else(|| anyhow!("Invalid reference name"))?;

    // Update the reference to point to the new commit
    repo.reference(
        ref_name,
        commit_id,
        true, // force
        &format!("guisu update: fast-forward to {}", commit_id),
    )
    .context("Failed to update reference")?;

    // Update the working directory
    repo.checkout_head(Some(
        git2::build::CheckoutBuilder::new()
            .force()
            .remove_untracked(false),
    ))
    .context("Failed to checkout HEAD")?;

    debug!(commit = %commit_id, "Fast-forward complete");
    Ok(())
}

/// Perform a rebase operation
fn perform_rebase(repo: &Repository, fetch_commit: &AnnotatedCommit) -> Result<()> {
    use git2::RebaseOptions;

    // Get the current HEAD
    let head = repo.head().context("Failed to get HEAD")?;
    let head_commit_obj = head.peel_to_commit().context("Failed to get HEAD commit")?;
    let head_commit = repo
        .find_annotated_commit(head_commit_obj.id())
        .context("Failed to convert HEAD commit to annotated commit")?;

    // Get current branch name for updating later
    let branch_name = head
        .shorthand()
        .ok_or_else(|| anyhow!("Failed to get branch name"))?
        .to_string();

    debug!(branch = %branch_name, "Starting rebase");

    // Initialize rebase
    let mut rebase_options = RebaseOptions::new();
    let mut rebase = repo
        .rebase(
            Some(&head_commit), // branch to rebase (our local commits)
            Some(fetch_commit), // upstream (their commits)
            None,               // onto (use upstream)
            Some(&mut rebase_options),
        )
        .context("Failed to initialize rebase")?;

    // Process each rebase operation
    while let Some(op) = rebase.next() {
        let operation = op.context("Failed to get rebase operation")?;

        // Get the commit being rebased
        let commit_id = operation.id();
        debug!(commit = %commit_id, "Rebasing commit");

        // Perform the rebase operation (apply the commit)
        rebase
            .commit(None, &repo.signature()?, None)
            .with_context(|| format!("Failed to apply commit {} during rebase", commit_id))?;
    }

    // Finish the rebase
    rebase.finish(None).context("Failed to finish rebase")?;

    debug!("Rebase complete");
    Ok(())
}

/// Count how many new commits were pulled
fn count_new_commits(repo: &Repository, new_commit: &AnnotatedCommit) -> Result<usize> {
    // Get HEAD commit
    let head = repo.head().context("Failed to get HEAD")?;
    let head_commit = head
        .peel_to_commit()
        .context("Failed to peel HEAD to commit")?;

    // Get the new commit
    let new_commit_obj = repo
        .find_commit(new_commit.id())
        .context("Failed to find new commit")?;

    // Count commits between old HEAD and new commit
    let mut revwalk = repo.revwalk().context("Failed to create revwalk")?;
    revwalk
        .push(new_commit_obj.id())
        .context("Failed to push new commit")?;
    revwalk
        .hide(head_commit.id())
        .context("Failed to hide old commit")?;

    let count = revwalk.count();
    Ok(count)
}
