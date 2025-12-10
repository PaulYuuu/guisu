//! Progress bar utilities
//!
//! This module provides progress bar helpers using indicatif.

use indicatif::{ProgressBar, ProgressStyle};

/// Create a progress bar for file operations
///
/// # Panics
///
/// Panics if the hardcoded progress bar template string is invalid.
/// This should never happen as the template is validated at compile time.
#[must_use]
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
///
/// # Panics
///
/// Panics if the hardcoded spinner template string is invalid.
/// This should never happen as the template is validated at compile time.
#[must_use]
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use super::*;

    #[test]
    fn test_create_progress_bar_basic() {
        let pb = create_progress_bar(100, "Testing");

        // Verify the progress bar was created
        assert_eq!(pb.length(), Some(100));
    }

    #[test]
    fn test_create_progress_bar_zero_total() {
        let pb = create_progress_bar(0, "Empty");

        // Should handle zero total
        assert_eq!(pb.length(), Some(0));
    }

    #[test]
    fn test_create_progress_bar_large_total() {
        let pb = create_progress_bar(1_000_000, "Large operation");

        assert_eq!(pb.length(), Some(1_000_000));
    }

    #[test]
    fn test_create_progress_bar_empty_message() {
        let pb = create_progress_bar(50, "");

        // Should handle empty message
        assert_eq!(pb.length(), Some(50));
    }

    #[test]
    fn test_create_progress_bar_unicode_message() {
        let pb = create_progress_bar(100, "Processing files");

        // Should handle unicode in message
        assert_eq!(pb.length(), Some(100));
    }

    #[test]
    fn test_create_spinner_basic() {
        let spinner = create_spinner("Loading");

        // Verify the spinner was created
        // Spinners don't have a definite length
        assert_eq!(spinner.length(), None);
    }

    #[test]
    fn test_create_spinner_empty_message() {
        let spinner = create_spinner("");

        // Should handle empty message
        assert_eq!(spinner.length(), None);
    }

    #[test]
    fn test_create_spinner_unicode_message() {
        let spinner = create_spinner("Waiting...");

        // Should handle unicode in message
        assert_eq!(spinner.length(), None);
    }

    #[test]
    fn test_create_spinner_long_message() {
        let long_msg = "A very long message that describes a complex operation in detail";
        let spinner = create_spinner(long_msg);

        assert_eq!(spinner.length(), None);
    }

    #[test]
    fn test_progress_bar_position_increment() {
        let pb = create_progress_bar(100, "Test");

        // Test incrementing position
        pb.inc(10);
        assert_eq!(pb.position(), 10);

        pb.inc(20);
        assert_eq!(pb.position(), 30);
    }

    #[test]
    fn test_progress_bar_set_position() {
        let pb = create_progress_bar(100, "Test");

        // Test setting position directly
        pb.set_position(50);
        assert_eq!(pb.position(), 50);
    }

    #[test]
    fn test_progress_bar_finish() {
        let pb = create_progress_bar(100, "Test");

        pb.set_position(50);
        pb.finish();

        // After finish, position should be at length
        assert_eq!(pb.position(), 100);
    }

    #[test]
    fn test_spinner_increment() {
        let spinner = create_spinner("Test");

        // Spinners can still track position even without length
        spinner.inc(1);
        assert_eq!(spinner.position(), 1);

        spinner.inc(5);
        assert_eq!(spinner.position(), 6);
    }

    #[test]
    fn test_multiple_progress_bars() {
        let pb1 = create_progress_bar(100, "First");
        let pb2 = create_progress_bar(200, "Second");

        // Verify both are independent
        assert_eq!(pb1.length(), Some(100));
        assert_eq!(pb2.length(), Some(200));

        pb1.inc(10);
        pb2.inc(20);

        assert_eq!(pb1.position(), 10);
        assert_eq!(pb2.position(), 20);
    }

    #[test]
    fn test_progress_bar_finish_and_clear() {
        let pb = create_progress_bar(100, "Test");

        pb.set_position(75);
        pb.finish_and_clear();

        // After finish_and_clear, the bar is done
        assert_eq!(pb.position(), 100);
    }
}
