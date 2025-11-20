//! Thread-safe statistics tracking for parallel operations

use std::sync::atomic::{AtomicU32, Ordering};

/// Thread-safe statistics for apply operations
///
/// Uses AtomicU32 instead of AtomicUsize to save memory (4 bytes vs 8 bytes on 64-bit systems).
/// File counts are unlikely to exceed u32::MAX (~4.3 billion).
#[derive(Debug, Default)]
pub struct ApplyStats {
    files: AtomicU32,
    directories: AtomicU32,
    symlinks: AtomicU32,
    failed: AtomicU32,
}

impl ApplyStats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn inc_files(&self) {
        self.files.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_directories(&self) {
        self.directories.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_symlinks(&self) {
        self.symlinks.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_failed(&self) {
        self.failed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn files(&self) -> usize {
        self.files.load(Ordering::Relaxed) as usize
    }

    pub fn directories(&self) -> usize {
        self.directories.load(Ordering::Relaxed) as usize
    }

    pub fn symlinks(&self) -> usize {
        self.symlinks.load(Ordering::Relaxed) as usize
    }

    pub fn failed(&self) -> usize {
        self.failed.load(Ordering::Relaxed) as usize
    }

    pub fn total(&self) -> usize {
        self.files() + self.directories() + self.symlinks()
    }

    /// Create a snapshot of current stats
    ///
    /// This is needed because ApplyStats uses atomics and cannot be cloned directly
    pub fn snapshot(&self) -> Self {
        Self {
            files: AtomicU32::new(self.files.load(Ordering::Relaxed)),
            directories: AtomicU32::new(self.directories.load(Ordering::Relaxed)),
            symlinks: AtomicU32::new(self.symlinks.load(Ordering::Relaxed)),
            failed: AtomicU32::new(self.failed.load(Ordering::Relaxed)),
        }
    }

    pub fn print_summary(&self, dry_run: bool) {
        use owo_colors::OwoColorize;

        let total = self.total();
        let failed = self.failed();
        let files = self.files();
        let directories = self.directories();
        let symlinks = self.symlinks();

        if dry_run {
            println!(
                "{} {} would be applied",
                "●".bright_green(),
                total.to_string().bright_white().bold()
            );
        } else if failed > 0 {
            println!(
                "{} {} | {} {}",
                "●".bright_green(),
                total.to_string().bright_green().bold(),
                "●".bright_red(),
                failed.to_string().bright_red().bold(),
            );
        } else {
            println!(
                "{} {} applied",
                "●".bright_green(),
                total.to_string().bright_green().bold()
            );
        }

        // Show breakdown if more than just files
        if directories > 0 || symlinks > 0 {
            let mut parts = Vec::new();
            if files > 0 {
                parts.push(format!("{} files", files));
            }
            if directories > 0 {
                parts.push(format!("{} directories", directories));
            }
            if symlinks > 0 {
                parts.push(format!("{} symlinks", symlinks));
            }
            println!("  {}", parts.join(", ").dimmed());
        }
    }
}

/// Thread-safe statistics for diff operations
///
/// Uses AtomicU32 instead of AtomicUsize to save memory (4 bytes vs 8 bytes on 64-bit systems).
#[derive(Debug, Default)]
pub struct DiffStats {
    added: AtomicU32,
    modified: AtomicU32,
    unchanged: AtomicU32,
    errors: AtomicU32,
}

impl DiffStats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn inc_added(&self) {
        self.added.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_modified(&self) {
        self.modified.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_unchanged(&self) {
        self.unchanged.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_errors(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn added(&self) -> usize {
        self.added.load(Ordering::Relaxed) as usize
    }

    pub fn modified(&self) -> usize {
        self.modified.load(Ordering::Relaxed) as usize
    }

    pub fn unchanged(&self) -> usize {
        self.unchanged.load(Ordering::Relaxed) as usize
    }

    pub fn errors(&self) -> usize {
        self.errors.load(Ordering::Relaxed) as usize
    }

    pub fn total(&self) -> usize {
        self.added() + self.modified() + self.unchanged()
    }
}

/// Thread-safe statistics for status operations
///
/// Uses AtomicU32 instead of AtomicUsize to save memory (4 bytes vs 8 bytes on 64-bit systems).
#[derive(Debug, Default)]
pub struct StatusStats {
    total: AtomicU32,
    modified: AtomicU32,
    added: AtomicU32,
    removed: AtomicU32,
}

impl StatusStats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn inc_total(&self) {
        self.total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_modified(&self) {
        self.modified.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_added(&self) {
        self.added.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_removed(&self) {
        self.removed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn total(&self) -> usize {
        self.total.load(Ordering::Relaxed) as usize
    }

    pub fn modified(&self) -> usize {
        self.modified.load(Ordering::Relaxed) as usize
    }

    pub fn added(&self) -> usize {
        self.added.load(Ordering::Relaxed) as usize
    }

    pub fn removed(&self) -> usize {
        self.removed.load(Ordering::Relaxed) as usize
    }
}
