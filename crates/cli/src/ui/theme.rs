//! Theme and color definitions for ratatui UI

use ratatui::style::{Color, Modifier, Style};

/// UI theme with predefined colors and styles
#[derive(Debug, Clone)]
pub struct Theme {
    // General UI
    /// Border style for UI elements
    pub border: Style,
    /// Title text style
    pub title: Style,
    /// Normal text style
    pub text: Style,
    /// Highlighted text style
    pub highlight: Style,
    /// Selected item style
    pub selected: Style,

    // Diff colors
    /// Style for added lines in diffs
    pub diff_add: Style,
    /// Style for removed lines in diffs
    pub diff_remove: Style,
    /// Style for modified lines in diffs
    pub diff_modify: Style,
}

impl Theme {
    /// Create a new theme
    #[must_use]
    pub fn new() -> Self {
        Self {
            // General UI
            border: Style::default().fg(Color::Cyan),
            title: Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            text: Style::default().fg(Color::White),
            highlight: Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
            selected: Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),

            // Diff colors
            diff_add: Style::default().fg(Color::Green),
            diff_remove: Style::default().fg(Color::Red),
            diff_modify: Style::default().fg(Color::Yellow),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use super::*;

    #[test]
    fn test_theme_new() {
        let theme = Theme::new();

        // Verify all styles are initialized (not panicking)
        assert_eq!(theme.border.fg, Some(Color::Cyan));
        assert_eq!(theme.title.fg, Some(Color::Cyan));
        assert_eq!(theme.text.fg, Some(Color::White));
        assert_eq!(theme.highlight.fg, Some(Color::Yellow));
        assert_eq!(theme.selected.fg, Some(Color::Black));
        assert_eq!(theme.selected.bg, Some(Color::Cyan));
        assert_eq!(theme.diff_add.fg, Some(Color::Green));
        assert_eq!(theme.diff_remove.fg, Some(Color::Red));
        assert_eq!(theme.diff_modify.fg, Some(Color::Yellow));
    }

    #[test]
    fn test_theme_default() {
        let theme = Theme::default();

        // Default should be same as new()
        assert_eq!(theme.border.fg, Some(Color::Cyan));
        assert_eq!(theme.title.fg, Some(Color::Cyan));
        assert_eq!(theme.text.fg, Some(Color::White));
    }

    #[test]
    fn test_theme_title_has_bold_modifier() {
        let theme = Theme::new();

        // Title should have BOLD modifier
        assert!(theme.title.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn test_theme_highlight_has_bold_modifier() {
        let theme = Theme::new();

        // Highlight should have BOLD modifier
        assert!(theme.highlight.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn test_theme_selected_has_bold_modifier() {
        let theme = Theme::new();

        // Selected should have BOLD modifier
        assert!(theme.selected.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn test_theme_selected_has_background() {
        let theme = Theme::new();

        // Selected should have background color
        assert_eq!(theme.selected.bg, Some(Color::Cyan));
    }

    #[test]
    fn test_theme_clone() {
        let theme = Theme::new();
        let cloned = theme.clone();

        // Verify all fields are cloned correctly
        assert_eq!(theme.border.fg, cloned.border.fg);
        assert_eq!(theme.title.fg, cloned.title.fg);
        assert_eq!(theme.text.fg, cloned.text.fg);
        assert_eq!(theme.highlight.fg, cloned.highlight.fg);
        assert_eq!(theme.selected.fg, cloned.selected.fg);
        assert_eq!(theme.selected.bg, cloned.selected.bg);
        assert_eq!(theme.diff_add.fg, cloned.diff_add.fg);
        assert_eq!(theme.diff_remove.fg, cloned.diff_remove.fg);
        assert_eq!(theme.diff_modify.fg, cloned.diff_modify.fg);
    }

    #[test]
    fn test_theme_diff_colors_distinct() {
        let theme = Theme::new();

        // Verify diff colors are different from each other
        assert_ne!(theme.diff_add.fg, theme.diff_remove.fg);
        assert_ne!(theme.diff_add.fg, theme.diff_modify.fg);
        assert_ne!(theme.diff_remove.fg, theme.diff_modify.fg);
    }

    #[test]
    fn test_theme_general_ui_colors() {
        let theme = Theme::new();

        // Verify general UI colors are set
        assert!(theme.border.fg.is_some());
        assert!(theme.title.fg.is_some());
        assert!(theme.text.fg.is_some());
        assert!(theme.highlight.fg.is_some());
        assert!(theme.selected.fg.is_some());
    }

    #[test]
    fn test_theme_border_style() {
        let theme = Theme::new();

        // Border should be cyan with no modifiers
        assert_eq!(theme.border.fg, Some(Color::Cyan));
        assert_eq!(theme.border.add_modifier, Modifier::empty());
    }

    #[test]
    fn test_theme_text_style() {
        let theme = Theme::new();

        // Text should be white with no modifiers
        assert_eq!(theme.text.fg, Some(Color::White));
        assert_eq!(theme.text.add_modifier, Modifier::empty());
    }

    #[test]
    fn test_theme_diff_add_style() {
        let theme = Theme::new();

        // Diff add should be green
        assert_eq!(theme.diff_add.fg, Some(Color::Green));
    }

    #[test]
    fn test_theme_diff_remove_style() {
        let theme = Theme::new();

        // Diff remove should be red
        assert_eq!(theme.diff_remove.fg, Some(Color::Red));
    }

    #[test]
    fn test_theme_diff_modify_style() {
        let theme = Theme::new();

        // Diff modify should be yellow
        assert_eq!(theme.diff_modify.fg, Some(Color::Yellow));
    }

    #[test]
    fn test_theme_multiple_instances_independent() {
        let theme1 = Theme::new();
        let theme2 = Theme::new();

        // Verify they have same values but are independent instances
        assert_eq!(theme1.border.fg, theme2.border.fg);
        assert_eq!(theme1.title.fg, theme2.title.fg);
    }
}
