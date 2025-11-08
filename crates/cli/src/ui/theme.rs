//! Theme and color definitions for ratatui UI

use ratatui::style::{Color, Modifier, Style};

/// UI theme with predefined colors and styles
#[derive(Debug, Clone)]
pub struct Theme {
    // General UI
    pub border: Style,
    pub title: Style,
    pub text: Style,
    pub highlight: Style,
    pub selected: Style,

    // Diff colors
    pub diff_add: Style,
    pub diff_remove: Style,
    pub diff_modify: Style,
}

impl Theme {
    /// Create a new theme
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
