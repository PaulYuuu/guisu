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

use crate::cmd::conflict::ChangeType;
use crate::ui::icons::Icons;
use crate::ui::preview::{ChangePreview, ChangeSummary};
use crate::ui::theme::Theme;

/// Conflict action options
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictAction {
    Diff,
    Override,
    Skip,
    AllSkip,
    AllOverride,
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

    fn label(&self) -> String {
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
                    KeyCode::Char('q') => return Ok(ConflictAction::Quit),
                    KeyCode::Esc => return Ok(ConflictAction::Quit),
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
