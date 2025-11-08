//! Thread-safe statistics tracking for parallel operations

use std::sync::atomic::{AtomicUsize, Ordering};

/// Thread-safe statistics for apply operations
#[derive(Debug, Default)]
pub struct ApplyStats {
    files: AtomicUsize,
    directories: AtomicUsize,
    symlinks: AtomicUsize,
    failed: AtomicUsize,
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
        self.files.load(Ordering::Relaxed)
    }

    pub fn directories(&self) -> usize {
        self.directories.load(Ordering::Relaxed)
    }

    pub fn symlinks(&self) -> usize {
        self.symlinks.load(Ordering::Relaxed)
    }

    pub fn failed(&self) -> usize {
        self.failed.load(Ordering::Relaxed)
    }

    pub fn total(&self) -> usize {
        self.files() + self.directories() + self.symlinks()
    }
}

/// Thread-safe statistics for diff operations
#[derive(Debug, Default)]
pub struct DiffStats {
    added: AtomicUsize,
    modified: AtomicUsize,
    unchanged: AtomicUsize,
    errors: AtomicUsize,
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
        self.added.load(Ordering::Relaxed)
    }

    pub fn modified(&self) -> usize {
        self.modified.load(Ordering::Relaxed)
    }

    pub fn unchanged(&self) -> usize {
        self.unchanged.load(Ordering::Relaxed)
    }

    pub fn errors(&self) -> usize {
        self.errors.load(Ordering::Relaxed)
    }

    pub fn total(&self) -> usize {
        self.added() + self.modified() + self.unchanged()
    }
}

/// Thread-safe statistics for status operations
#[derive(Debug, Default)]
pub struct StatusStats {
    total: AtomicUsize,
    modified: AtomicUsize,
    added: AtomicUsize,
    removed: AtomicUsize,
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
        self.total.load(Ordering::Relaxed)
    }

    pub fn modified(&self) -> usize {
        self.modified.load(Ordering::Relaxed)
    }

    pub fn added(&self) -> usize {
        self.added.load(Ordering::Relaxed)
    }

    pub fn removed(&self) -> usize {
        self.removed.load(Ordering::Relaxed)
    }
}
