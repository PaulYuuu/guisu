//! Change preview and summary utilities

use similar::{ChangeTag, TextDiff};

/// Summary of changes between two files
#[derive(Debug)]
pub struct ChangeSummary {
    pub lines_added: usize,
    pub lines_removed: usize,
    pub lines_modified: usize,
}

impl ChangeSummary {
    /// Calculate change summary from a text diff
    pub fn from_diff<'a, 'b>(diff: &TextDiff<'a, 'b, 'a, str>) -> Self {
        let mut lines_added = 0;
        let mut lines_removed = 0;

        for op in diff.ops() {
            for change in diff.iter_changes(op) {
                match change.tag() {
                    ChangeTag::Insert => lines_added += 1,
                    ChangeTag::Delete => lines_removed += 1,
                    ChangeTag::Equal => {}
                }
            }
        }

        // Calculate modified lines (pairs of adjacent delete/insert)
        // This is a simple heuristic
        let modifications = lines_added.min(lines_removed);
        let lines_modified = modifications;
        let lines_added = lines_added - modifications;
        let lines_removed = lines_removed - modifications;

        Self {
            lines_added,
            lines_removed,
            lines_modified,
        }
    }

    /// Create summary from two text contents
    pub fn from_texts(old: &str, new: &str) -> Self {
        let diff = TextDiff::from_lines(old, new);
        Self::from_diff(&diff)
    }
}

/// Preview of changes with limited lines
#[derive(Debug)]
pub struct ChangePreview {
    pub lines: Vec<PreviewLine>,
    pub truncated: bool,
}

#[derive(Debug)]
pub struct PreviewLine {
    pub tag: PreviewTag,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewTag {
    Add,
    Remove,
    Context,
}

impl ChangePreview {
    /// Generate preview from text diff
    pub fn from_diff<'a, 'b>(diff: &TextDiff<'a, 'b, 'a, str>, max_lines: usize) -> Self {
        let mut lines = Vec::new();
        let mut total_lines = 0;

        for op in diff.ops() {
            if lines.len() >= max_lines {
                break;
            }

            for change in diff.iter_changes(op) {
                total_lines += 1;
                if lines.len() >= max_lines {
                    break;
                }

                let tag = match change.tag() {
                    ChangeTag::Insert => PreviewTag::Add,
                    ChangeTag::Delete => PreviewTag::Remove,
                    ChangeTag::Equal => PreviewTag::Context,
                };

                let content = change.value().trim_end_matches('\n').to_string();

                lines.push(PreviewLine { tag, content });
            }
        }

        let truncated = total_lines > max_lines;

        Self { lines, truncated }
    }

    /// Generate preview from two text contents
    pub fn from_texts(old: &str, new: &str, max_lines: usize) -> Self {
        let diff = TextDiff::from_lines(old, new);
        Self::from_diff(&diff, max_lines)
    }

    /// Get lines with change markers
    pub fn lines_with_markers(&self) -> Vec<String> {
        use crate::ui::icons::Icons;
        self.lines
            .iter()
            .map(|line| {
                let marker = match line.tag {
                    PreviewTag::Add => Icons::ACTION_ADD,
                    PreviewTag::Remove => Icons::ACTION_REMOVE,
                    PreviewTag::Context => " ",
                };
                format!("{} {}", marker, line.content)
            })
            .collect()
    }
}
