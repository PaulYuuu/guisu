use anyhow::{Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};
use std::io;

use crate::conflict::ChangeType;
use crate::ui::icons::Icons;
use crate::ui::preview::{ChangePreview, ChangeSummary};
use crate::ui::theme::Theme;

/// Conflict action options
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictAction {
    /// Show full differences between files
    Diff,
    /// Override destination with source changes
    Override,
    /// Skip this file and keep destination as-is
    Skip,
    /// Skip all remaining files
    AllSkip,
    /// Override all remaining files with source
    AllOverride,
    /// Quit the apply operation
    Quit,
}

impl ConflictAction {
    fn all_actions() -> Vec<Self> {
        vec![
            Self::Diff,
            Self::Override,
            Self::Skip,
            Self::AllSkip,
            Self::AllOverride,
            Self::Quit,
        ]
    }

    fn label(self) -> String {
        match self {
            Self::Diff => "Diff - show full differences".to_string(),
            Self::Override => "Override - apply source changes".to_string(),
            Self::Skip => "Skip - keep destination as-is".to_string(),
            Self::AllSkip => "All Skip - keep all remaining files as-is".to_string(),
            Self::AllOverride => "All Override - apply source for all remaining".to_string(),
            Self::Quit => "Quit - exit apply operation".to_string(),
        }
    }
}

/// Interactive conflict resolution prompt using ratatui
pub struct ConflictPrompt {
    file_path: String,
    summary: ChangeSummary,
    preview: ChangePreview,
    theme: Theme,
    list_state: ListState,
    actions: Vec<ConflictAction>,
    /// Scroll offset for preview area
    preview_scroll: u16,
    /// Type of change detected
    change_type: ChangeType,
}

impl ConflictPrompt {
    /// Create a new conflict prompt
    #[must_use]
    pub fn new(
        file_path: String,
        summary: ChangeSummary,
        preview: ChangePreview,
        change_type: ChangeType,
    ) -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0)); // Select first item by default

        Self {
            file_path,
            summary,
            preview,
            theme: Theme::new(),
            list_state,
            actions: ConflictAction::all_actions(),
            preview_scroll: 0,
            change_type,
        }
    }

    /// Run the interactive prompt and return the selected action
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Terminal setup fails (raw mode, alternate screen)
    /// - Event handling fails
    /// - Terminal restoration fails
    pub fn run(&mut self) -> Result<ConflictAction> {
        // Setup terminal
        enable_raw_mode().context("Failed to enable raw mode")?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
            .context("Failed to setup terminal")?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend).context("Failed to create terminal")?;

        // Run event loop
        let result = self.run_event_loop(&mut terminal);

        // Restore terminal
        disable_raw_mode().context("Failed to disable raw mode")?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )
        .context("Failed to restore terminal")?;
        terminal.show_cursor().context("Failed to show cursor")?;

        result
    }

    /// Event loop for the TUI
    fn run_event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<ConflictAction> {
        loop {
            terminal.draw(|f| self.render(f))?;

            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => self.previous(),
                    KeyCode::Down | KeyCode::Char('j') => self.next(),
                    KeyCode::PageUp => self.scroll_preview_up(),
                    KeyCode::PageDown => self.scroll_preview_down(),
                    KeyCode::Enter => return Ok(self.get_selected_action()),
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(ConflictAction::Quit),
                    _ => {}
                }
            }
        }
    }

    /// Render the UI
    fn render(&mut self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Title
                Constraint::Min(10),    // Preview
                Constraint::Length(12), // Actions
                Constraint::Length(3),  // Help
            ])
            .split(frame.area());

        // Render title
        self.render_title(frame, chunks[0]);

        // Render preview
        self.render_preview(frame, chunks[1]);

        // Render actions
        self.render_actions(frame, chunks[2]);

        // Render help
        self.render_help(frame, chunks[3]);
    }

    /// Render title section
    fn render_title(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        // Use appropriate title based on change type
        let prefix = match self.change_type {
            ChangeType::LocalModification => "Local modification",
            ChangeType::SourceUpdate => "Source updated",
            ChangeType::TrueConflict => "Conflict",
        };

        let title = format!("{}: {}", prefix, self.file_path);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.theme.border)
            .title(title)
            .title_style(self.theme.title);

        frame.render_widget(block, area);
    }

    /// Render preview section
    fn render_preview(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let mut text = Vec::new();

        // Change summary
        text.push(Line::from(vec![Span::styled(
            "Change Summary:",
            self.theme.highlight,
        )]));

        if self.summary.lines_added > 0 {
            text.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(Icons::ACTION_ADD, self.theme.diff_add),
                Span::styled(
                    format!(" {} lines added", self.summary.lines_added),
                    self.theme.diff_add,
                ),
            ]));
        }

        if self.summary.lines_removed > 0 {
            text.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(Icons::ACTION_REMOVE, self.theme.diff_remove),
                Span::styled(
                    format!(" {} lines removed", self.summary.lines_removed),
                    self.theme.diff_remove,
                ),
            ]));
        }

        if self.summary.lines_modified > 0 {
            text.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(Icons::ACTION_MODIFY, self.theme.diff_modify),
                Span::styled(
                    format!(" {} lines modified", self.summary.lines_modified),
                    self.theme.diff_modify,
                ),
            ]));
        }

        text.push(Line::from(""));

        // Preview
        text.push(Line::from(vec![Span::styled(
            "Preview:",
            self.theme.highlight,
        )]));

        for line in self.preview.lines_with_markers() {
            let style = if line.starts_with(Icons::ACTION_ADD) {
                self.theme.diff_add
            } else if line.starts_with(Icons::ACTION_REMOVE) {
                self.theme.diff_remove
            } else if line.starts_with(Icons::ACTION_MODIFY) {
                self.theme.diff_modify
            } else {
                self.theme.text
            };

            text.push(Line::from(vec![Span::raw("  "), Span::styled(line, style)]));
        }

        if self.preview.truncated {
            text.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("...", self.theme.text),
            ]));
        }

        let paragraph = Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(self.theme.border)
                    .title("Changes (Page Up/Down to scroll)")
                    .title_style(self.theme.title),
            )
            .wrap(Wrap { trim: false })
            .scroll((self.preview_scroll, 0));

        frame.render_widget(paragraph, area);
    }

    /// Render actions list
    fn render_actions(&mut self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let items: Vec<ListItem> = self
            .actions
            .iter()
            .map(|action| {
                let label = action.label();
                ListItem::new(label)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(self.theme.border)
                    .title("Actions")
                    .title_style(self.theme.title),
            )
            .highlight_style(self.theme.selected)
            .highlight_symbol(">> ");

        frame.render_stateful_widget(list, area, &mut self.list_state);
    }

    /// Render help section
    fn render_help(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let help_text = vec![Line::from(vec![
            Span::styled("Navigation: ", self.theme.highlight),
            Span::raw("↑↓/jk to move, "),
            Span::styled("PgUp/PgDn", self.theme.highlight),
            Span::raw(" to scroll, "),
            Span::styled("Enter", self.theme.highlight),
            Span::raw(" to select, "),
            Span::styled("q/Esc", self.theme.highlight),
            Span::raw(" to quit"),
        ])];

        let paragraph = Paragraph::new(help_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(self.theme.border),
            )
            .alignment(Alignment::Center);

        frame.render_widget(paragraph, area);
    }

    /// Move to previous item
    fn previous(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.actions.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    /// Move to next item
    fn next(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) => (i + 1) % self.actions.len(),
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    /// Get the currently selected action
    fn get_selected_action(&self) -> ConflictAction {
        let idx = self.list_state.selected().unwrap_or(0);
        self.actions[idx]
    }

    /// Scroll preview up
    fn scroll_preview_up(&mut self) {
        self.preview_scroll = self.preview_scroll.saturating_sub(5);
    }

    /// Scroll preview down
    fn scroll_preview_down(&mut self) {
        self.preview_scroll = self.preview_scroll.saturating_add(5);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use super::*;
    use crate::ui::preview::{PreviewLine, PreviewTag};

    // Tests for ConflictAction

    #[test]
    fn test_conflict_action_all_actions() {
        let actions = ConflictAction::all_actions();
        assert_eq!(actions.len(), 6);
        assert_eq!(actions[0], ConflictAction::Diff);
        assert_eq!(actions[1], ConflictAction::Override);
        assert_eq!(actions[2], ConflictAction::Skip);
        assert_eq!(actions[3], ConflictAction::AllSkip);
        assert_eq!(actions[4], ConflictAction::AllOverride);
        assert_eq!(actions[5], ConflictAction::Quit);
    }

    #[test]
    fn test_conflict_action_label_diff() {
        let action = ConflictAction::Diff;
        assert_eq!(action.label(), "Diff - show full differences");
    }

    #[test]
    fn test_conflict_action_label_override() {
        let action = ConflictAction::Override;
        assert_eq!(action.label(), "Override - apply source changes");
    }

    #[test]
    fn test_conflict_action_label_skip() {
        let action = ConflictAction::Skip;
        assert_eq!(action.label(), "Skip - keep destination as-is");
    }

    #[test]
    fn test_conflict_action_label_all_skip() {
        let action = ConflictAction::AllSkip;
        assert_eq!(action.label(), "All Skip - keep all remaining files as-is");
    }

    #[test]
    fn test_conflict_action_label_all_override() {
        let action = ConflictAction::AllOverride;
        assert_eq!(
            action.label(),
            "All Override - apply source for all remaining"
        );
    }

    #[test]
    fn test_conflict_action_label_quit() {
        let action = ConflictAction::Quit;
        assert_eq!(action.label(), "Quit - exit apply operation");
    }

    #[test]
    fn test_conflict_action_equality() {
        assert_eq!(ConflictAction::Diff, ConflictAction::Diff);
        assert_eq!(ConflictAction::Override, ConflictAction::Override);
        assert_ne!(ConflictAction::Diff, ConflictAction::Override);
    }

    #[test]
    fn test_conflict_action_clone() {
        let action = ConflictAction::Diff;
        let cloned = action;
        assert_eq!(action, cloned);
    }

    #[test]
    fn test_conflict_action_copy() {
        let action = ConflictAction::Skip;
        let copied = action;
        // After copy, original should still be usable
        assert_eq!(action, ConflictAction::Skip);
        assert_eq!(copied, ConflictAction::Skip);
    }

    // Tests for ConflictPrompt

    fn create_test_prompt() -> ConflictPrompt {
        let summary = ChangeSummary {
            lines_added: 2,
            lines_removed: 1,
            lines_modified: 3,
        };

        let preview = ChangePreview {
            lines: vec![
                PreviewLine {
                    tag: PreviewTag::Context,
                    content: "line1".to_string(),
                },
                PreviewLine {
                    tag: PreviewTag::Add,
                    content: "line2".to_string(),
                },
                PreviewLine {
                    tag: PreviewTag::Remove,
                    content: "line3".to_string(),
                },
            ],
            truncated: false,
        };

        ConflictPrompt::new(
            "test/file.txt".to_string(),
            summary,
            preview,
            ChangeType::TrueConflict,
        )
    }

    #[test]
    fn test_conflict_prompt_new() {
        let prompt = create_test_prompt();

        assert_eq!(prompt.file_path, "test/file.txt");
        assert_eq!(prompt.summary.lines_added, 2);
        assert_eq!(prompt.summary.lines_removed, 1);
        assert_eq!(prompt.summary.lines_modified, 3);
        assert_eq!(prompt.preview.lines.len(), 3);
        assert!(!prompt.preview.truncated);
        assert_eq!(prompt.actions.len(), 6);
        assert_eq!(prompt.preview_scroll, 0);
        assert_eq!(prompt.change_type, ChangeType::TrueConflict);
    }

    #[test]
    fn test_conflict_prompt_new_selects_first_action() {
        let prompt = create_test_prompt();
        assert_eq!(prompt.list_state.selected(), Some(0));
    }

    #[test]
    fn test_conflict_prompt_get_selected_action_default() {
        let prompt = create_test_prompt();
        assert_eq!(prompt.get_selected_action(), ConflictAction::Diff);
    }

    #[test]
    fn test_conflict_prompt_next() {
        let mut prompt = create_test_prompt();

        // Start at 0
        assert_eq!(prompt.list_state.selected(), Some(0));

        // Move to 1
        prompt.next();
        assert_eq!(prompt.list_state.selected(), Some(1));
        assert_eq!(prompt.get_selected_action(), ConflictAction::Override);

        // Move to 2
        prompt.next();
        assert_eq!(prompt.list_state.selected(), Some(2));
        assert_eq!(prompt.get_selected_action(), ConflictAction::Skip);
    }

    #[test]
    fn test_conflict_prompt_next_wraps_around() {
        let mut prompt = create_test_prompt();

        // Move to last item
        for _ in 0..5 {
            prompt.next();
        }
        assert_eq!(prompt.list_state.selected(), Some(5));
        assert_eq!(prompt.get_selected_action(), ConflictAction::Quit);

        // Next should wrap to 0
        prompt.next();
        assert_eq!(prompt.list_state.selected(), Some(0));
        assert_eq!(prompt.get_selected_action(), ConflictAction::Diff);
    }

    #[test]
    fn test_conflict_prompt_previous() {
        let mut prompt = create_test_prompt();

        // Move to second item first
        prompt.next();
        assert_eq!(prompt.list_state.selected(), Some(1));

        // Move back to first
        prompt.previous();
        assert_eq!(prompt.list_state.selected(), Some(0));
        assert_eq!(prompt.get_selected_action(), ConflictAction::Diff);
    }

    #[test]
    fn test_conflict_prompt_previous_wraps_around() {
        let mut prompt = create_test_prompt();

        // Start at 0
        assert_eq!(prompt.list_state.selected(), Some(0));

        // Previous should wrap to last item
        prompt.previous();
        assert_eq!(prompt.list_state.selected(), Some(5));
        assert_eq!(prompt.get_selected_action(), ConflictAction::Quit);
    }

    #[test]
    fn test_conflict_prompt_scroll_preview_down() {
        let mut prompt = create_test_prompt();

        assert_eq!(prompt.preview_scroll, 0);

        prompt.scroll_preview_down();
        assert_eq!(prompt.preview_scroll, 5);

        prompt.scroll_preview_down();
        assert_eq!(prompt.preview_scroll, 10);
    }

    #[test]
    fn test_conflict_prompt_scroll_preview_up() {
        let mut prompt = create_test_prompt();

        // Scroll down first
        prompt.scroll_preview_down();
        prompt.scroll_preview_down();
        assert_eq!(prompt.preview_scroll, 10);

        // Scroll up
        prompt.scroll_preview_up();
        assert_eq!(prompt.preview_scroll, 5);

        prompt.scroll_preview_up();
        assert_eq!(prompt.preview_scroll, 0);
    }

    #[test]
    fn test_conflict_prompt_scroll_preview_up_saturates_at_zero() {
        let mut prompt = create_test_prompt();

        assert_eq!(prompt.preview_scroll, 0);

        // Scrolling up at 0 should stay at 0 (saturating_sub)
        prompt.scroll_preview_up();
        assert_eq!(prompt.preview_scroll, 0);

        prompt.scroll_preview_up();
        assert_eq!(prompt.preview_scroll, 0);
    }

    #[test]
    fn test_conflict_prompt_scroll_preview_down_large_values() {
        let mut prompt = create_test_prompt();

        // Scroll down many times
        for _ in 0..100 {
            prompt.scroll_preview_down();
        }

        // Should be 500 (100 * 5)
        assert_eq!(prompt.preview_scroll, 500);
    }

    #[test]
    fn test_conflict_prompt_navigation_sequence() {
        let mut prompt = create_test_prompt();

        // Test a navigation sequence
        prompt.next(); // 0 -> 1 (Override)
        prompt.next(); // 1 -> 2 (Skip)
        prompt.next(); // 2 -> 3 (AllSkip)
        assert_eq!(prompt.get_selected_action(), ConflictAction::AllSkip);

        prompt.previous(); // 3 -> 2 (Skip)
        assert_eq!(prompt.get_selected_action(), ConflictAction::Skip);

        prompt.previous(); // 2 -> 1 (Override)
        prompt.previous(); // 1 -> 0 (Diff)
        assert_eq!(prompt.get_selected_action(), ConflictAction::Diff);
    }

    #[test]
    fn test_conflict_prompt_with_different_change_types() {
        // Test LocalModification
        let prompt1 = ConflictPrompt::new(
            "file1.txt".to_string(),
            ChangeSummary {
                lines_added: 1,
                lines_removed: 0,
                lines_modified: 0,
            },
            ChangePreview {
                lines: vec![PreviewLine {
                    tag: PreviewTag::Add,
                    content: "new line".to_string(),
                }],
                truncated: false,
            },
            ChangeType::LocalModification,
        );
        assert_eq!(prompt1.change_type, ChangeType::LocalModification);

        // Test SourceUpdate
        let prompt2 = ConflictPrompt::new(
            "file2.txt".to_string(),
            ChangeSummary {
                lines_added: 1,
                lines_removed: 0,
                lines_modified: 0,
            },
            ChangePreview {
                lines: vec![PreviewLine {
                    tag: PreviewTag::Add,
                    content: "new line".to_string(),
                }],
                truncated: false,
            },
            ChangeType::SourceUpdate,
        );
        assert_eq!(prompt2.change_type, ChangeType::SourceUpdate);

        // Test TrueConflict
        let prompt3 = ConflictPrompt::new(
            "file3.txt".to_string(),
            ChangeSummary {
                lines_added: 1,
                lines_removed: 0,
                lines_modified: 0,
            },
            ChangePreview {
                lines: vec![PreviewLine {
                    tag: PreviewTag::Add,
                    content: "new line".to_string(),
                }],
                truncated: false,
            },
            ChangeType::TrueConflict,
        );
        assert_eq!(prompt3.change_type, ChangeType::TrueConflict);
    }

    #[test]
    fn test_conflict_prompt_with_empty_preview() {
        let summary = ChangeSummary {
            lines_added: 0,
            lines_removed: 0,
            lines_modified: 0,
        };

        let preview = ChangePreview {
            lines: vec![],
            truncated: false,
        };

        let prompt = ConflictPrompt::new(
            "empty.txt".to_string(),
            summary,
            preview,
            ChangeType::TrueConflict,
        );

        assert_eq!(prompt.preview.lines.len(), 0);
        assert_eq!(prompt.summary.lines_added, 0);
        assert_eq!(prompt.summary.lines_removed, 0);
        assert_eq!(prompt.summary.lines_modified, 0);
    }

    #[test]
    fn test_conflict_prompt_with_truncated_preview() {
        let summary = ChangeSummary {
            lines_added: 100,
            lines_removed: 50,
            lines_modified: 25,
        };

        let preview = ChangePreview {
            lines: vec![
                PreviewLine {
                    tag: PreviewTag::Add,
                    content: "line1".to_string(),
                },
                PreviewLine {
                    tag: PreviewTag::Remove,
                    content: "line2".to_string(),
                },
            ],
            truncated: true,
        };

        let prompt = ConflictPrompt::new(
            "large.txt".to_string(),
            summary,
            preview,
            ChangeType::TrueConflict,
        );

        assert!(prompt.preview.truncated);
        assert_eq!(prompt.summary.lines_added, 100);
        assert_eq!(prompt.summary.lines_removed, 50);
        assert_eq!(prompt.summary.lines_modified, 25);
    }
}
