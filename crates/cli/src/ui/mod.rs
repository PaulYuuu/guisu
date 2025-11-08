//! Terminal UI components for guisu
//!
//! This crate provides all terminal user interface components including:
//! - Conflict resolution prompts
//! - Diff viewers
//! - Text editors
//! - Progress indicators
//! - Icons and themes

pub mod diffviewer;
pub mod editor;
pub mod icons;
pub mod interactive_diff;
pub mod merge;
pub mod preview;
pub mod progress;
pub mod prompt;
pub mod theme;

pub use diffviewer::{DiffFormat, DiffViewer};
pub use editor::{open_for_merge, open_in_editor};
pub use icons::{FileIconInfo, Icons, StatusIcon};
pub use interactive_diff::{FileDiff, FileStatus, InteractiveDiffViewer};
pub use merge::MergeResult;
pub use preview::{ChangePreview, ChangeSummary};
pub use progress::{create_progress_bar, create_spinner};
pub use prompt::{ConflictAction, ConflictPrompt};
pub use theme::Theme;
