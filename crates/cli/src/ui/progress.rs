//! Progress bar utilities
//!
//! This module provides progress bar helpers using indicatif.

use indicatif::{ProgressBar, ProgressStyle};

/// Create a progress bar for file operations
pub fn create_progress_bar(total: u64, message: &str) -> ProgressBar {
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{msg} [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
            .expect("progress bar template is valid")
            .progress_chars("#>-"),
    );
    pb.set_message(message.to_string());
    pb
}

/// Create a spinner for indeterminate operations
pub fn create_spinner(message: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner} {msg}")
            .expect("spinner template is valid"),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(std::time::Duration::from_millis(100));
    pb
}
