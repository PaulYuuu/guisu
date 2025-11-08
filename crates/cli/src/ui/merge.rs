//! Three-way merge implementation

use anyhow::Result;
use similar::{ChangeTag, TextDiff};

/// Result of a three-way merge operation
#[derive(Debug)]
pub enum MergeResult {
    /// Merge completed successfully without conflicts
    Success(String),
    /// Merge has conflicts that need manual resolution
    Conflicts(String),
}

impl MergeResult {
    /// Check if merge has conflicts
    pub fn has_conflicts(&self) -> bool {
        matches!(self, MergeResult::Conflicts(_))
    }

    /// Get the content (either clean or with conflict markers)
    pub fn content(&self) -> &str {
        match self {
            MergeResult::Success(c) | MergeResult::Conflicts(c) => c,
        }
    }
}

/// Perform three-way merge
///
/// # Arguments
/// * `base` - Common ancestor (last synchronized version from database)
/// * `local` - Current destination file content
/// * `remote` - Source file content
///
/// # Returns
/// `MergeResult::Success` if merge completed cleanly
/// `MergeResult::Conflicts` if there are conflicts (includes conflict markers)
pub fn three_way_merge(base: &str, local: &str, remote: &str) -> Result<MergeResult> {
    // Compute diffs from base to local and base to remote
    let _diff_local = TextDiff::from_lines(base, local);
    let _diff_remote = TextDiff::from_lines(base, remote);

    // Build line-by-line merge
    let base_lines: Vec<&str> = base.lines().collect();
    let local_lines: Vec<&str> = local.lines().collect();
    let remote_lines: Vec<&str> = remote.lines().collect();

    let mut result = Vec::new();
    let mut has_conflicts = false;

    // Track position in base, local, and remote
    let mut base_idx = 0;
    let mut local_idx = 0;
    let mut remote_idx = 0;

    // Simple merge algorithm:
    // For each line in base, check if it was changed in local or remote
    // - If unchanged in both: keep original
    // - If changed in one: use the changed version
    // - If changed in both differently: conflict

    while base_idx < base_lines.len()
        || local_idx < local_lines.len()
        || remote_idx < remote_lines.len()
    {
        let base_line = base_lines.get(base_idx).copied();
        let local_line = local_lines.get(local_idx).copied();
        let remote_line = remote_lines.get(remote_idx).copied();

        match (base_line, local_line, remote_line) {
            (Some(b), Some(l), Some(r)) if b == l && b == r => {
                // No changes in either
                result.push(b.to_string());
                base_idx += 1;
                local_idx += 1;
                remote_idx += 1;
            }
            (Some(b), Some(l), Some(r)) if b == l && b != r => {
                // Changed in remote only
                result.push(r.to_string());
                base_idx += 1;
                local_idx += 1;
                remote_idx += 1;
            }
            (Some(b), Some(l), Some(r)) if b != l && b == r => {
                // Changed in local only
                result.push(l.to_string());
                base_idx += 1;
                local_idx += 1;
                remote_idx += 1;
            }
            (Some(b), Some(l), Some(r)) if b != l && b != r => {
                // Changed in both - check if they agree
                if l == r {
                    // Same change in both
                    result.push(l.to_string());
                    base_idx += 1;
                    local_idx += 1;
                    remote_idx += 1;
                } else {
                    // Conflict: different changes
                    has_conflicts = true;
                    result.push("<<<<<<< LOCAL (destination)".to_string());
                    result.push(l.to_string());
                    result.push("=======".to_string());
                    result.push(r.to_string());
                    result.push(">>>>>>> REMOTE (source)".to_string());
                    base_idx += 1;
                    local_idx += 1;
                    remote_idx += 1;
                }
            }
            (None, Some(l), Some(r)) => {
                // Lines added in both - check if they're the same
                if l == r {
                    result.push(l.to_string());
                    local_idx += 1;
                    remote_idx += 1;
                } else {
                    // Conflict: different additions
                    has_conflicts = true;
                    result.push("<<<<<<< LOCAL (destination)".to_string());
                    result.push(l.to_string());
                    result.push("=======".to_string());
                    result.push(r.to_string());
                    result.push(">>>>>>> REMOTE (source)".to_string());
                    local_idx += 1;
                    remote_idx += 1;
                }
            }
            (None, Some(l), None) => {
                // Added in local only
                result.push(l.to_string());
                local_idx += 1;
            }
            (None, None, Some(r)) => {
                // Added in remote only
                result.push(r.to_string());
                remote_idx += 1;
            }
            (Some(_), None, Some(r)) => {
                // Deleted in local, maybe modified in remote
                // For now, use remote version
                result.push(r.to_string());
                base_idx += 1;
                remote_idx += 1;
            }
            (Some(_), Some(l), None) => {
                // Deleted in remote, maybe modified in local
                // For now, use local version
                result.push(l.to_string());
                base_idx += 1;
                local_idx += 1;
            }
            (Some(_), None, None) => {
                // Deleted in both
                base_idx += 1;
            }
            (None, None, None) => {
                // Should not happen
                break;
            }
            _ => {
                // Handle remaining cases
                if let Some(l) = local_line {
                    result.push(l.to_string());
                    local_idx += 1;
                } else if let Some(r) = remote_line {
                    result.push(r.to_string());
                    remote_idx += 1;
                } else {
                    break;
                }
            }
        }
    }

    let merged = result.join("\n");

    if has_conflicts {
        Ok(MergeResult::Conflicts(merged))
    } else {
        Ok(MergeResult::Success(merged))
    }
}

/// Simpler two-way merge (no base available)
///
/// This is less sophisticated and more likely to produce conflicts
pub fn two_way_merge(local: &str, remote: &str) -> Result<MergeResult> {
    // Without a base, we can only detect identical lines
    // Everything else is a potential conflict
    let diff = TextDiff::from_lines(local, remote);

    let mut result = Vec::new();
    let mut has_conflicts = false;
    let mut in_conflict = false;
    let mut conflict_local = Vec::new();
    let mut conflict_remote = Vec::new();

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Equal => {
                if in_conflict {
                    // End of conflict region
                    has_conflicts = true;
                    result.push("<<<<<<< LOCAL (destination)".to_string());
                    result.extend(conflict_local.drain(..).map(|s: &str| s.to_string()));
                    result.push("=======".to_string());
                    result.extend(conflict_remote.drain(..).map(|s: &str| s.to_string()));
                    result.push(">>>>>>> REMOTE (source)".to_string());
                    in_conflict = false;
                }
                result.push(change.value().trim_end_matches('\n').to_string());
            }
            ChangeTag::Delete => {
                in_conflict = true;
                conflict_local.push(change.value().trim_end_matches('\n'));
            }
            ChangeTag::Insert => {
                in_conflict = true;
                conflict_remote.push(change.value().trim_end_matches('\n'));
            }
        }
    }

    // Handle any remaining conflict
    if in_conflict {
        has_conflicts = true;
        result.push("<<<<<<< LOCAL (destination)".to_string());
        result.extend(conflict_local.drain(..).map(|s: &str| s.to_string()));
        result.push("=======".to_string());
        result.extend(conflict_remote.drain(..).map(|s: &str| s.to_string()));
        result.push(">>>>>>> REMOTE (source)".to_string());
    }

    let merged = result.join("\n");

    if has_conflicts {
        Ok(MergeResult::Conflicts(merged))
    } else {
        Ok(MergeResult::Success(merged))
    }
}
