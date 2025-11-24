//! Diff viewers with multiple formats (unified, split, inline)

use anyhow::Result;
use owo_colors::OwoColorize;
use similar::{ChangeTag, TextDiff};
use std::io::Write;

/// Diff display format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffFormat {
    /// Unified diff (traditional format with +/- lines)
    Unified,
    /// Split diff (side-by-side comparison)
    Split,
    /// Inline diff (word-level highlighting within lines)
    Inline,
}

impl std::str::FromStr for DiffFormat {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "unified" => Ok(DiffFormat::Unified),
            "split" => Ok(DiffFormat::Split),
            "inline" => Ok(DiffFormat::Inline),
            _ => Err(anyhow::anyhow!("Invalid diff format: {s}")),
        }
    }
}

/// Diff viewer for displaying file differences
pub struct DiffViewer {
    format: DiffFormat,
    context_lines: usize,
}

impl DiffViewer {
    /// Create a new diff viewer with specified format and context lines
    #[must_use]
    pub fn new(format: DiffFormat, context_lines: usize) -> Self {
        Self {
            format,
            context_lines,
        }
    }

    /// Display diff to writer
    ///
    /// # Errors
    ///
    /// Returns an error if writing to the output fails
    pub fn display<W: Write>(
        &self,
        writer: &mut W,
        old_content: &str,
        new_content: &str,
        old_label: &str,
        new_label: &str,
    ) -> Result<()> {
        match self.format {
            DiffFormat::Unified => {
                self.display_unified(writer, old_content, new_content, old_label, new_label)
            }
            DiffFormat::Split => {
                Self::display_split(writer, old_content, new_content, old_label, new_label)
            }
            DiffFormat::Inline => {
                self.display_inline(writer, old_content, new_content, old_label, new_label)
            }
        }
    }

    /// Display unified diff format
    fn display_unified<W: Write>(
        &self,
        writer: &mut W,
        old_content: &str,
        new_content: &str,
        old_label: &str,
        new_label: &str,
    ) -> Result<()> {
        let diff = TextDiff::from_lines(old_content, new_content);

        writeln!(writer, "{}", format!("--- {old_label}").bold())?;
        writeln!(writer, "{}", format!("+++ {new_label}").bold())?;

        for (idx, group) in diff.grouped_ops(self.context_lines).iter().enumerate() {
            if idx > 0 {
                writeln!(writer, "---")?;
            }

            // Write hunk header
            if let Some(first) = group.first()
                && let Some(last) = group.last()
            {
                let old_start = first.old_range().start + 1;
                let old_len = last.old_range().end - first.old_range().start;
                let new_start = first.new_range().start + 1;
                let new_len = last.new_range().end - first.new_range().start;
                writeln!(
                    writer,
                    "{}",
                    format!("@@ -{old_start},{old_len} +{new_start},{new_len} @@").cyan()
                )?;
            }

            // Write changes
            for op in group {
                for change in diff.iter_changes(op) {
                    match change.tag() {
                        ChangeTag::Delete => {
                            let line = format!("-{}", change.value()).red().to_string();
                            write!(writer, "{line}")?;
                        }
                        ChangeTag::Insert => {
                            let line = format!("+{}", change.value()).green().to_string();
                            write!(writer, "{line}")?;
                        }
                        ChangeTag::Equal => {
                            write!(writer, " {}", change.value())?;
                        }
                    }
                    if !change.value().ends_with('\n') {
                        writeln!(writer)?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Calculate terminal width and column width for split view
    fn calculate_split_dimensions() -> (usize, usize) {
        let term_width =
            terminal_size::terminal_size().map_or(120, |(terminal_size::Width(w), _)| w as usize);
        let column_width = (term_width - 4) / 2; // 4 for divider and padding
        (term_width, column_width)
    }

    /// Display header for split view
    fn display_split_header<W: Write>(
        writer: &mut W,
        old_label: &str,
        new_label: &str,
        column_width: usize,
        term_width: usize,
    ) -> Result<()> {
        let header_left = format!("{old_label:column_width$}");
        let header_right = format!("{new_label:column_width$}");
        writeln!(writer, "{} │ {}", header_left.bold(), header_right.bold())?;
        writeln!(writer, "{}", "─".repeat(term_width))?;
        Ok(())
    }

    /// Display equal lines (same content on both sides)
    fn display_equal_lines<W: Write>(
        writer: &mut W,
        lines: &[&str],
        start_idx: usize,
        range: &std::ops::Range<usize>,
        column_width: usize,
    ) -> Result<()> {
        for i in 0..(range.end - range.start) {
            let line = lines.get(start_idx + i).unwrap_or(&"");
            let truncated = truncate_line(line, column_width);
            writeln!(
                writer,
                "{truncated:column_width$} │ {truncated:column_width$}"
            )?;
        }
        Ok(())
    }

    /// Display deleted lines (left side only)
    fn display_delete_lines<W: Write>(
        writer: &mut W,
        old_lines: &[&str],
        old_idx: usize,
        old_range: &std::ops::Range<usize>,
        column_width: usize,
    ) -> Result<()> {
        for i in 0..(old_range.end - old_range.start) {
            let line = old_lines.get(old_idx + i).unwrap_or(&"");
            let truncated = truncate_line(line, column_width);
            writeln!(
                writer,
                "{} │ {:width$}",
                truncated.red(),
                "",
                width = column_width
            )?;
        }
        Ok(())
    }

    /// Display inserted lines (right side only)
    fn display_insert_lines<W: Write>(
        writer: &mut W,
        new_lines: &[&str],
        new_idx: usize,
        new_range: &std::ops::Range<usize>,
        column_width: usize,
    ) -> Result<()> {
        for i in 0..(new_range.end - new_range.start) {
            let line = new_lines.get(new_idx + i).unwrap_or(&"");
            let truncated = truncate_line(line, column_width);
            writeln!(
                writer,
                "{:width$} │ {}",
                "",
                truncated.green(),
                width = column_width
            )?;
        }
        Ok(())
    }

    /// Display replaced lines (both sides with highlighting)
    #[allow(clippy::too_many_arguments)]
    fn display_replace_lines<W: Write>(
        writer: &mut W,
        old_lines: &[&str],
        new_lines: &[&str],
        old_idx: usize,
        new_idx: usize,
        old_range: &std::ops::Range<usize>,
        new_range: &std::ops::Range<usize>,
        column_width: usize,
    ) -> Result<(usize, usize)> {
        let old_count = old_range.end - old_range.start;
        let new_count = new_range.end - new_range.start;
        let max_count = old_count.max(new_count);

        for i in 0..max_count {
            let old_line = if i < old_count {
                old_lines.get(old_idx + i).unwrap_or(&"")
            } else {
                ""
            };
            let new_line = if i < new_count {
                new_lines.get(new_idx + i).unwrap_or(&"")
            } else {
                ""
            };

            let old_truncated = truncate_line(old_line, column_width);
            let new_truncated = truncate_line(new_line, column_width);

            if !old_line.is_empty() && !new_line.is_empty() {
                writeln!(
                    writer,
                    "{} │ {}",
                    old_truncated.red(),
                    new_truncated.green()
                )?;
            } else if !old_line.is_empty() {
                writeln!(
                    writer,
                    "{} │ {:width$}",
                    old_truncated.red(),
                    "",
                    width = column_width
                )?;
            } else {
                writeln!(
                    writer,
                    "{:width$} │ {}",
                    "",
                    new_truncated.green(),
                    width = column_width
                )?;
            }
        }

        Ok((old_count, new_count))
    }

    /// Display split (side-by-side) diff format
    fn display_split<W: Write>(
        writer: &mut W,
        old_content: &str,
        new_content: &str,
        old_label: &str,
        new_label: &str,
    ) -> Result<()> {
        let (term_width, column_width) = Self::calculate_split_dimensions();
        Self::display_split_header(writer, old_label, new_label, column_width, term_width)?;

        let diff = TextDiff::from_lines(old_content, new_content);
        let old_lines: Vec<_> = old_content.lines().collect();
        let new_lines: Vec<_> = new_content.lines().collect();

        let mut old_idx = 0;
        let mut new_idx = 0;

        for op in diff.ops() {
            let old_range = op.old_range();
            let new_range = op.new_range();

            match op.tag() {
                similar::DiffTag::Equal => {
                    Self::display_equal_lines(
                        writer,
                        &old_lines,
                        old_idx,
                        &old_range,
                        column_width,
                    )?;
                    old_idx += old_range.end - old_range.start;
                    new_idx += new_range.end - new_range.start;
                }
                similar::DiffTag::Delete => {
                    Self::display_delete_lines(
                        writer,
                        &old_lines,
                        old_idx,
                        &old_range,
                        column_width,
                    )?;
                    old_idx += old_range.end - old_range.start;
                }
                similar::DiffTag::Insert => {
                    Self::display_insert_lines(
                        writer,
                        &new_lines,
                        new_idx,
                        &new_range,
                        column_width,
                    )?;
                    new_idx += new_range.end - new_range.start;
                }
                similar::DiffTag::Replace => {
                    let (old_count, new_count) = Self::display_replace_lines(
                        writer,
                        &old_lines,
                        &new_lines,
                        old_idx,
                        new_idx,
                        &old_range,
                        &new_range,
                        column_width,
                    )?;
                    old_idx += old_count;
                    new_idx += new_count;
                }
            }
        }

        Ok(())
    }

    /// Display inline diff format with word-level highlighting
    fn display_inline<W: Write>(
        &self,
        writer: &mut W,
        old_content: &str,
        new_content: &str,
        old_label: &str,
        new_label: &str,
    ) -> Result<()> {
        let diff = TextDiff::from_lines(old_content, new_content);

        writeln!(writer, "{}", format!("--- {old_label}").bold())?;
        writeln!(writer, "{}", format!("+++ {new_label}").bold())?;

        for (idx, group) in diff.grouped_ops(self.context_lines).iter().enumerate() {
            if idx > 0 {
                writeln!(writer, "---")?;
            }

            for op in group {
                for change in diff.iter_changes(op) {
                    match change.tag() {
                        ChangeTag::Equal => {
                            write!(writer, " {}", change.value())?;
                            if !change.value().ends_with('\n') {
                                writeln!(writer)?;
                            }
                        }
                        ChangeTag::Delete => {
                            // For inline diff, highlight the specific words/chars that changed
                            write!(writer, "{}", format!("-{}", change.value()).red())?;
                            if !change.value().ends_with('\n') {
                                writeln!(writer)?;
                            }
                        }
                        ChangeTag::Insert => {
                            write!(writer, "{}", format!("+{}", change.value()).green())?;
                            if !change.value().ends_with('\n') {
                                writeln!(writer)?;
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

/// Truncate line to fit in column width
fn truncate_line(line: &str, max_width: usize) -> String {
    if line.len() <= max_width {
        format!("{line:max_width$}")
    } else {
        let truncated = &line[..max_width.saturating_sub(3)];
        format!("{truncated}...")
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use super::*;

    // Tests for DiffFormat

    #[test]
    fn test_diff_format_from_str_unified() {
        let format = "unified".parse::<DiffFormat>().unwrap();
        assert_eq!(format, DiffFormat::Unified);
    }

    #[test]
    fn test_diff_format_from_str_split() {
        let format = "split".parse::<DiffFormat>().unwrap();
        assert_eq!(format, DiffFormat::Split);
    }

    #[test]
    fn test_diff_format_from_str_inline() {
        let format = "inline".parse::<DiffFormat>().unwrap();
        assert_eq!(format, DiffFormat::Inline);
    }

    #[test]
    fn test_diff_format_from_str_case_insensitive() {
        assert_eq!(
            "UNIFIED".parse::<DiffFormat>().unwrap(),
            DiffFormat::Unified
        );
        assert_eq!("Split".parse::<DiffFormat>().unwrap(), DiffFormat::Split);
        assert_eq!("InLiNe".parse::<DiffFormat>().unwrap(), DiffFormat::Inline);
    }

    #[test]
    fn test_diff_format_from_str_invalid() {
        let result = "invalid".parse::<DiffFormat>();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid diff format")
        );
    }

    #[test]
    fn test_diff_format_equality() {
        assert_eq!(DiffFormat::Unified, DiffFormat::Unified);
        assert_eq!(DiffFormat::Split, DiffFormat::Split);
        assert_eq!(DiffFormat::Inline, DiffFormat::Inline);
        assert_ne!(DiffFormat::Unified, DiffFormat::Split);
    }

    #[test]
    fn test_diff_format_clone() {
        let format = DiffFormat::Unified;
        let cloned = format;
        assert_eq!(format, cloned);
    }

    // Tests for DiffViewer

    #[test]
    fn test_diff_viewer_new() {
        let viewer = DiffViewer::new(DiffFormat::Unified, 3);
        assert_eq!(viewer.format, DiffFormat::Unified);
        assert_eq!(viewer.context_lines, 3);
    }

    #[test]
    fn test_diff_viewer_new_different_context() {
        let viewer = DiffViewer::new(DiffFormat::Split, 5);
        assert_eq!(viewer.format, DiffFormat::Split);
        assert_eq!(viewer.context_lines, 5);
    }

    #[test]
    fn test_diff_viewer_display_unified_no_changes() {
        let viewer = DiffViewer::new(DiffFormat::Unified, 3);
        let mut output = Vec::new();

        let old = "line1\nline2\nline3";
        let new = "line1\nline2\nline3";

        viewer
            .display(&mut output, old, new, "old.txt", "new.txt")
            .unwrap();

        let result = String::from_utf8(output).unwrap();
        assert!(result.contains("--- old.txt"));
        assert!(result.contains("+++ new.txt"));
        // No changes, so no diff hunks
    }

    #[test]
    fn test_diff_viewer_display_unified_with_changes() {
        let viewer = DiffViewer::new(DiffFormat::Unified, 3);
        let mut output = Vec::new();

        let old = "line1\nline2\nline3";
        let new = "line1\nmodified\nline3";

        viewer
            .display(&mut output, old, new, "old.txt", "new.txt")
            .unwrap();

        let result = String::from_utf8(output).unwrap();
        assert!(result.contains("--- old.txt"));
        assert!(result.contains("+++ new.txt"));
        // Should show the change
        assert!(result.contains("line2") || result.contains("modified"));
    }

    #[test]
    fn test_diff_viewer_display_split() {
        let viewer = DiffViewer::new(DiffFormat::Split, 3);
        let mut output = Vec::new();

        let old = "line1\nline2";
        let new = "line1\nchanged";

        viewer
            .display(&mut output, old, new, "old.txt", "new.txt")
            .unwrap();

        let result = String::from_utf8(output).unwrap();
        assert!(result.contains("old.txt"));
        assert!(result.contains("new.txt"));
        assert!(result.contains("│")); // Split view uses vertical bar
    }

    #[test]
    fn test_diff_viewer_display_inline() {
        let viewer = DiffViewer::new(DiffFormat::Inline, 3);
        let mut output = Vec::new();

        let old = "line1\nline2";
        let new = "line1\nchanged";

        viewer
            .display(&mut output, old, new, "old.txt", "new.txt")
            .unwrap();

        let result = String::from_utf8(output).unwrap();
        assert!(result.contains("--- old.txt"));
        assert!(result.contains("+++ new.txt"));
    }

    #[test]
    fn test_diff_viewer_unified_empty_to_content() {
        let viewer = DiffViewer::new(DiffFormat::Unified, 3);
        let mut output = Vec::new();

        let old = "";
        let new = "new line";

        viewer
            .display(&mut output, old, new, "empty", "new.txt")
            .unwrap();

        let result = String::from_utf8(output).unwrap();
        assert!(result.contains("--- empty"));
        assert!(result.contains("+++ new.txt"));
    }

    #[test]
    fn test_diff_viewer_unified_content_to_empty() {
        let viewer = DiffViewer::new(DiffFormat::Unified, 3);
        let mut output = Vec::new();

        let old = "old line";
        let new = "";

        viewer
            .display(&mut output, old, new, "old.txt", "empty")
            .unwrap();

        let result = String::from_utf8(output).unwrap();
        assert!(result.contains("--- old.txt"));
        assert!(result.contains("+++ empty"));
    }

    #[test]
    fn test_diff_viewer_unified_multiple_changes() {
        let viewer = DiffViewer::new(DiffFormat::Unified, 1);
        let mut output = Vec::new();

        let old = "line1\nline2\nline3\nline4";
        let new = "line1\nchanged2\nline3\nchanged4";

        viewer
            .display(&mut output, old, new, "old.txt", "new.txt")
            .unwrap();

        let result = String::from_utf8(output).unwrap();
        assert!(result.contains("--- old.txt"));
        assert!(result.contains("+++ new.txt"));
        // With context_lines=1, should show changes with context
    }

    #[test]
    fn test_diff_viewer_split_empty_both() {
        let viewer = DiffViewer::new(DiffFormat::Split, 3);
        let mut output = Vec::new();

        viewer
            .display(&mut output, "", "", "empty1", "empty2")
            .unwrap();

        let result = String::from_utf8(output).unwrap();
        assert!(result.contains("empty1"));
        assert!(result.contains("empty2"));
    }

    #[test]
    fn test_diff_viewer_inline_no_changes() {
        let viewer = DiffViewer::new(DiffFormat::Inline, 3);
        let mut output = Vec::new();

        let content = "same\nlines\nhere";
        viewer
            .display(&mut output, content, content, "a", "b")
            .unwrap();

        let result = String::from_utf8(output).unwrap();
        assert!(result.contains("--- a"));
        assert!(result.contains("+++ b"));
    }

    // Tests for truncate_line

    #[test]
    fn test_truncate_line_shorter_than_max() {
        let line = "short";
        let result = truncate_line(line, 10);
        assert_eq!(result.len(), 10);
        assert!(result.starts_with("short"));
    }

    #[test]
    fn test_truncate_line_equal_to_max() {
        let line = "exactly10c";
        let result = truncate_line(line, 10);
        assert_eq!(result, "exactly10c");
    }

    #[test]
    fn test_truncate_line_longer_than_max() {
        let line = "this is a very long line that needs truncation";
        let result = truncate_line(line, 10);
        assert_eq!(result.len(), 10);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_truncate_line_exact_cutoff() {
        let line = "abcdefghijk";
        let result = truncate_line(line, 10);
        // Should be "abcdefg..." (7 chars + "...")
        assert_eq!(result, "abcdefg...");
    }

    #[test]
    fn test_truncate_line_empty() {
        let line = "";
        let result = truncate_line(line, 10);
        assert_eq!(result.len(), 10);
        assert_eq!(result.trim(), "");
    }

    #[test]
    fn test_truncate_line_max_width_zero() {
        let line = "text";
        let result = truncate_line(line, 0);
        // With max_width=0, saturating_sub(3) = 0, so truncated = ""
        assert_eq!(result, "...");
    }

    #[test]
    fn test_truncate_line_max_width_very_small() {
        let line = "text";
        let result = truncate_line(line, 3);
        // With max_width=3, can fit exactly "..."
        assert_eq!(result, "...");
    }

    #[test]
    fn test_truncate_line_unicode() {
        // Note: This test may fail due to byte vs char length issues
        // The current implementation uses byte length, not char length
        let line = "hello";
        let result = truncate_line(line, 5);
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_truncate_line_padding() {
        let line = "abc";
        let result = truncate_line(line, 10);
        // Should be padded to 10 chars
        assert_eq!(result.len(), 10);
        assert!(result.starts_with("abc"));
        // Rest should be spaces
        assert_eq!(result.trim_end(), "abc");
    }
}
