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
            _ => Err(anyhow::anyhow!("Invalid diff format: {}", s)),
        }
    }
}

/// Diff viewer for displaying file differences
pub struct DiffViewer {
    format: DiffFormat,
    context_lines: usize,
}

impl DiffViewer {
    pub fn new(format: DiffFormat, context_lines: usize) -> Self {
        Self {
            format,
            context_lines,
        }
    }

    /// Display diff to writer
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
                self.display_split(writer, old_content, new_content, old_label, new_label)
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

        writeln!(writer, "{}", format!("--- {}", old_label).bold())?;
        writeln!(writer, "{}", format!("+++ {}", new_label).bold())?;

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
                    format!(
                        "@@ -{},{} +{},{} @@",
                        old_start, old_len, new_start, new_len
                    )
                    .cyan()
                )?;
            }

            // Write changes
            for op in group {
                for change in diff.iter_changes(op) {
                    match change.tag() {
                        ChangeTag::Delete => {
                            let line = format!("-{}", change.value()).red().to_string();
                            write!(writer, "{}", line)?;
                        }
                        ChangeTag::Insert => {
                            let line = format!("+{}", change.value()).green().to_string();
                            write!(writer, "{}", line)?;
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

    /// Display split (side-by-side) diff format
    fn display_split<W: Write>(
        &self,
        writer: &mut W,
        old_content: &str,
        new_content: &str,
        old_label: &str,
        new_label: &str,
    ) -> Result<()> {
        let diff = TextDiff::from_lines(old_content, new_content);

        // Get terminal width or use default
        let term_width = terminal_size::terminal_size()
            .map(|(terminal_size::Width(w), _)| w as usize)
            .unwrap_or(120);

        let column_width = (term_width - 4) / 2; // 4 for divider and padding

        // Header
        let header_left = format!("{:width$}", old_label, width = column_width);
        let header_right = format!("{:width$}", new_label, width = column_width);
        writeln!(writer, "{} │ {}", header_left.bold(), header_right.bold())?;
        writeln!(writer, "{}", "─".repeat(term_width))?;

        // Build side-by-side view
        let old_lines: Vec<_> = old_content.lines().collect();
        let new_lines: Vec<_> = new_content.lines().collect();

        let mut old_idx = 0;
        let mut new_idx = 0;

        for op in diff.ops() {
            let old_range = op.old_range();
            let new_range = op.new_range();

            match op.tag() {
                similar::DiffTag::Equal => {
                    // Show both sides (same content)
                    for i in 0..(old_range.end - old_range.start) {
                        let line = old_lines.get(old_idx + i).unwrap_or(&"");
                        let truncated = truncate_line(line, column_width);
                        writeln!(
                            writer,
                            "{:width$} │ {:width$}",
                            truncated,
                            truncated,
                            width = column_width
                        )?;
                    }
                    old_idx += old_range.end - old_range.start;
                    new_idx += new_range.end - new_range.start;
                }
                similar::DiffTag::Delete => {
                    // Show on left side only
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
                    old_idx += old_range.end - old_range.start;
                }
                similar::DiffTag::Insert => {
                    // Show on right side only
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
                    new_idx += new_range.end - new_range.start;
                }
                similar::DiffTag::Replace => {
                    // Show both with highlighting
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

        writeln!(writer, "{}", format!("--- {}", old_label).bold())?;
        writeln!(writer, "{}", format!("+++ {}", new_label).bold())?;

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
        format!("{:width$}", line, width = max_width)
    } else {
        let truncated = &line[..max_width.saturating_sub(3)];
        format!("{}...", truncated)
    }
}
