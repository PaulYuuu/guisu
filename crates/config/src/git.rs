//! Git operations abstraction layer
//!
//! This module provides a unified interface for Git operations, supporting both:
//! - Built-in git via git2 (libgit2) - default
//! - External git command - fallback or when explicitly configured
//!
//! The abstraction allows switching between implementations based on configuration
//! or availability, similar to chezmoi's approach.

use crate::Result;
use std::path::Path;

/// Helper function to convert git2 errors to guisu_core errors
#[inline]
fn git_err(e: git2::Error) -> guisu_core::Error {
    guisu_core::Error::Message(format!("Git error: {}", e))
}

/// Git provider trait defining all git operations needed by guisu
pub trait GitProvider {
    /// Clone a repository from URL to target path
    fn clone(
        &self,
        url: &str,
        target: &Path,
        depth: Option<usize>,
        branch: Option<&str>,
        recurse_submodules: bool,
    ) -> Result<()>;

    /// Fetch updates from remote
    fn fetch(&self, repo_path: &Path, remote: &str) -> Result<()>;

    /// Perform fast-forward merge
    fn fast_forward(&self, repo_path: &Path) -> Result<usize>;

    /// Perform rebase
    fn rebase(&self, repo_path: &Path) -> Result<()>;

    /// Check if repository is up to date
    fn is_up_to_date(&self, repo_path: &Path) -> Result<bool>;

    /// Get repository status (has uncommitted changes, etc.)
    fn status(&self, repo_path: &Path) -> Result<GitStatus>;

    /// Get current branch name
    fn current_branch(&self, repo_path: &Path) -> Result<String>;
}

/// Git repository status
#[derive(Debug, Clone)]
pub struct GitStatus {
    pub has_uncommitted_changes: bool,
    pub has_untracked_files: bool,
    pub branch: String,
}

/// Type alias for progress callback function
/// Arguments: (current, total, percentage)
type ProgressCallback = Box<dyn Fn(usize, usize, f64) + Send + Sync>;

/// Git provider implementation using git2 (libgit2)
pub struct Git2Provider {
    progress_callback: Option<ProgressCallback>,
}

impl Git2Provider {
    pub fn new() -> Self {
        Self {
            progress_callback: None,
        }
    }

    /// Set progress callback for clone/fetch operations
    pub fn with_progress<F>(mut self, callback: F) -> Self
    where
        F: Fn(usize, usize, f64) + Send + Sync + 'static,
    {
        self.progress_callback = Some(Box::new(callback));
        self
    }
}

impl Default for Git2Provider {
    fn default() -> Self {
        Self::new()
    }
}

impl GitProvider for Git2Provider {
    fn clone(
        &self,
        url: &str,
        target: &Path,
        depth: Option<usize>,
        branch: Option<&str>,
        recurse_submodules: bool,
    ) -> Result<()> {
        use git2::{FetchOptions, RemoteCallbacks, build::RepoBuilder};

        // Set up callbacks for progress reporting
        let mut callbacks = RemoteCallbacks::new();
        if let Some(ref progress_fn) = self.progress_callback {
            #[allow(noop_method_call)]
            let progress_fn = progress_fn.clone();
            callbacks.transfer_progress(move |stats| {
                let received = stats.received_objects();
                let total = stats.total_objects();
                let bytes_mb = stats.received_bytes() as f64 / 1_048_576.0;
                progress_fn(received, total, bytes_mb);
                true
            });
        }

        // Configure fetch options
        let mut fetch_options = FetchOptions::new();
        fetch_options.remote_callbacks(callbacks);

        if let Some(d) = depth {
            fetch_options.depth(d as i32);
        }

        // Build and execute clone
        let mut builder = RepoBuilder::new();
        builder.fetch_options(fetch_options);

        if let Some(b) = branch {
            builder.branch(b);
        }

        let repo = builder.clone(url, target)
            .map_err(|e| guisu_core::Error::Message(
                format!(
                    "Failed to clone repository from {}. Check the URL and your network connection. Error: {}",
                    url, e
                )
            ))?;

        // Initialize submodules if requested
        if recurse_submodules {
            init_submodules_recursive(&repo, target)?;
        }

        Ok(())
    }

    fn fetch(&self, repo_path: &Path, remote: &str) -> Result<()> {
        use git2::{AutotagOption, FetchOptions, RemoteCallbacks, Repository};

        let repo = Repository::open(repo_path).map_err(git_err)?;
        let mut remote = repo.find_remote(remote).map_err(git_err)?;

        let mut callbacks = RemoteCallbacks::new();
        if let Some(ref progress_fn) = self.progress_callback {
            #[allow(noop_method_call)]
            let progress_fn = progress_fn.clone();
            callbacks.transfer_progress(move |stats| {
                let received = stats.received_objects();
                let total = stats.total_objects();
                let bytes_mb = stats.received_bytes() as f64 / 1_048_576.0;
                progress_fn(received, total, bytes_mb);
                true
            });
        }

        let mut fetch_options = FetchOptions::new();
        fetch_options.remote_callbacks(callbacks);
        fetch_options.download_tags(AutotagOption::Auto);

        remote
            .fetch(&["HEAD"], Some(&mut fetch_options), None)
            .map_err(git_err)?;

        Ok(())
    }

    fn fast_forward(&self, repo_path: &Path) -> Result<usize> {
        use git2::Repository;

        let repo = Repository::open(repo_path).map_err(git_err)?;

        // Get FETCH_HEAD
        let fetch_head = repo.find_reference("FETCH_HEAD").map_err(git_err)?;
        let fetch_commit = repo
            .reference_to_annotated_commit(&fetch_head)
            .map_err(git_err)?;

        // Get the commit object
        let commit_id = fetch_commit.id();

        // Update HEAD reference
        let reference = repo.find_reference("HEAD").map_err(git_err)?;
        let ref_name = reference
            .name()
            .ok_or_else(|| guisu_core::Error::Message("Invalid reference name".to_string()))?;

        repo.reference(
            ref_name,
            commit_id,
            true,
            &format!("guisu update: fast-forward to {}", commit_id),
        )
        .map_err(git_err)?;

        // Checkout updated HEAD
        repo.checkout_head(Some(
            git2::build::CheckoutBuilder::new()
                .force()
                .remove_untracked(false),
        ))
        .map_err(git_err)?;

        // Count new commits
        count_new_commits(&repo, &fetch_commit)
    }

    fn rebase(&self, repo_path: &Path) -> Result<()> {
        use git2::{RebaseOptions, Repository};

        let repo = Repository::open(repo_path).map_err(git_err)?;

        // Get FETCH_HEAD and HEAD
        let fetch_head = repo.find_reference("FETCH_HEAD").map_err(git_err)?;
        let fetch_commit = repo
            .reference_to_annotated_commit(&fetch_head)
            .map_err(git_err)?;

        let head = repo.head().map_err(git_err)?;
        let head_commit_obj = head.peel_to_commit().map_err(git_err)?;
        let head_commit = repo
            .find_annotated_commit(head_commit_obj.id())
            .map_err(git_err)?;

        // Initialize and perform rebase
        let mut rebase_options = RebaseOptions::new();
        let mut rebase = repo
            .rebase(
                Some(&head_commit),
                Some(&fetch_commit),
                None,
                Some(&mut rebase_options),
            )
            .map_err(git_err)?;

        while let Some(op) = rebase.next() {
            op.map_err(git_err)?;
            rebase
                .commit(None, &repo.signature().map_err(git_err)?, None)
                .map_err(git_err)?;
        }

        rebase.finish(None).map_err(git_err)?;
        Ok(())
    }

    fn is_up_to_date(&self, repo_path: &Path) -> Result<bool> {
        use git2::Repository;

        let repo = Repository::open(repo_path).map_err(git_err)?;
        let fetch_head = repo.find_reference("FETCH_HEAD").map_err(git_err)?;
        let fetch_commit = repo
            .reference_to_annotated_commit(&fetch_head)
            .map_err(git_err)?;

        let analysis = repo.merge_analysis(&[&fetch_commit]).map_err(git_err)?;
        Ok(analysis.0.is_up_to_date())
    }

    fn status(&self, repo_path: &Path) -> Result<GitStatus> {
        use git2::Repository;

        let repo = Repository::open(repo_path).map_err(git_err)?;
        let statuses = repo.statuses(None).map_err(git_err)?;

        let has_uncommitted_changes = statuses
            .iter()
            .any(|s| s.status().is_index_modified() || s.status().is_wt_modified());

        let has_untracked_files = statuses.iter().any(|s| s.status().is_wt_new());

        let head = repo.head().map_err(git_err)?;
        let branch = head
            .shorthand()
            .ok_or_else(|| guisu_core::Error::Message("Failed to get branch name".to_string()))?
            .to_string();

        Ok(GitStatus {
            has_uncommitted_changes,
            has_untracked_files,
            branch,
        })
    }

    fn current_branch(&self, repo_path: &Path) -> Result<String> {
        use git2::Repository;

        let repo = Repository::open(repo_path).map_err(git_err)?;
        let head = repo.head().map_err(git_err)?;
        let branch = head
            .shorthand()
            .ok_or_else(|| {
                guisu_core::Error::Message("Not on a branch (detached HEAD)".to_string())
            })?
            .to_string();
        Ok(branch)
    }
}

/// Helper function to recursively initialize submodules
fn init_submodules_recursive(repo: &git2::Repository, repo_path: &Path) -> Result<()> {
    use git2::{FetchOptions, RemoteCallbacks, Repository, SubmoduleUpdateOptions};

    let submodules = repo.submodules().map_err(git_err)?;

    for mut submodule in submodules {
        let _name = submodule.name().unwrap_or("<unnamed>");
        let path = submodule.path().to_path_buf();

        // Initialize the submodule
        submodule.init(false).map_err(git_err)?;

        // Update the submodule
        let mut update_options = SubmoduleUpdateOptions::new();
        let mut fetch_options = FetchOptions::new();
        let mut callbacks = RemoteCallbacks::new();
        callbacks.transfer_progress(|_| true);
        fetch_options.remote_callbacks(callbacks);
        update_options.fetch(fetch_options);

        submodule
            .update(true, Some(&mut update_options))
            .map_err(git_err)?;

        // Recursively initialize nested submodules
        let submodule_path = repo_path.join(&path);
        if let Ok(sub_repo) = Repository::open(&submodule_path) {
            init_submodules_recursive(&sub_repo, &submodule_path)?;
        }
    }

    Ok(())
}

/// Helper function to count new commits
fn count_new_commits(repo: &git2::Repository, new_commit: &git2::AnnotatedCommit) -> Result<usize> {
    let head = repo.head().map_err(git_err)?;
    let head_commit = head.peel_to_commit().map_err(git_err)?;
    let new_commit_obj = repo.find_commit(new_commit.id()).map_err(git_err)?;

    let mut revwalk = repo.revwalk().map_err(git_err)?;
    revwalk.push(new_commit_obj.id()).map_err(git_err)?;
    revwalk.hide(head_commit.id()).map_err(git_err)?;

    Ok(revwalk.count())
}

/// Create git provider (uses git2)
pub fn create_provider(_use_builtin: &crate::config::AutoBool) -> Box<dyn GitProvider> {
    Box::new(Git2Provider::new())
}
