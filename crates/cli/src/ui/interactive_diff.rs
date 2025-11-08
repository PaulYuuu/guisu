//! Interactive diff viewer using ratatui
//!
//! Provides a TUI for navigating and viewing file diffs with features like:
//! - File list navigation
//! - Scrollable diff view
//! - Hunk jumping
//! - Multiple diff formats

use anyhow::{Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};
use similar::{ChangeTag, TextDiff};
use std::io;

use crate::ui::icons::Icons;

/// A single diff line
#[derive(Debug, Clone)]
pub enum DiffLine {
    /// Context line (unchanged)
    Context {
        line_num: Option<usize>,
        content: String,
    },
    /// Added line
    Add {
        line_num: Option<usize>,
        content: String,
    },
    /// Removed line
    Remove {
        line_num: Option<usize>,
        content: String,
    },
    /// Hunk header
    Header {
        old_range: (usize, usize),
        new_range: (usize, usize),
    },
}

/// A hunk of changes in a file
#[derive(Debug, Clone)]
pub struct Hunk {
    /// Old file line range (start, length)
    pub old_range: (usize, usize),
    /// New file line range (start, length)
    pub new_range: (usize, usize),
    /// Lines in this hunk
    pub lines: Vec<DiffLine>,
}

/// Diff information for a single file
#[derive(Debug, Clone)]
pub struct FileDiff {
    /// Relative path to the file
    pub path: String,
    /// File status (added, modified, deleted)
    pub status: FileStatus,
    /// Hunks of changes
    pub hunks: Vec<Hunk>,
    /// Old content (for reference)
    pub old_content: String,
    /// New content (for reference)
    pub new_content: String,
}

/// File change status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    /// File is new
    Added,
    /// File is modified
    Modified,
    /// File is deleted
    Deleted,
}

impl FileDiff {
    /// Create a new FileDiff from old and new content
    pub fn new(path: String, old_content: String, new_content: String, status: FileStatus) -> Self {
        let hunks = Self::compute_hunks(&old_content, &new_content, 3);

        Self {
            path,
            status,
            hunks,
            old_content,
            new_content,
        }
    }

    /// Compute hunks from old and new content
    fn compute_hunks(old_content: &str, new_content: &str, context_lines: usize) -> Vec<Hunk> {
        let diff = TextDiff::from_lines(old_content, new_content);
        let mut hunks = Vec::new();

        for group in diff.grouped_ops(context_lines) {
            if group.is_empty() {
                continue;
            }

            let first = &group[0];
            let last = &group[group.len() - 1];

            let old_start = first.old_range().start + 1;
            let old_len = last.old_range().end - first.old_range().start;
            let new_start = first.new_range().start + 1;
            let new_len = last.new_range().end - first.new_range().start;

            let mut lines = Vec::new();

            // Add hunk header
            lines.push(DiffLine::Header {
                old_range: (old_start, old_len),
                new_range: (new_start, new_len),
            });

            // Add diff lines
            let mut old_line_num = old_start;
            let mut new_line_num = new_start;

            for op in &group {
                for change in diff.iter_changes(op) {
                    let content = change.value().trim_end_matches('\n').to_string();

                    match change.tag() {
                        ChangeTag::Equal => {
                            lines.push(DiffLine::Context {
                                line_num: Some(new_line_num),
                                content,
                            });
                            old_line_num += 1;
                            new_line_num += 1;
                        }
                        ChangeTag::Insert => {
                            lines.push(DiffLine::Add {
                                line_num: Some(new_line_num),
                                content,
                            });
                            new_line_num += 1;
                        }
                        ChangeTag::Delete => {
                            lines.push(DiffLine::Remove {
                                line_num: Some(old_line_num),
                                content,
                            });
                            old_line_num += 1;
                        }
                    }
                }
            }

            hunks.push(Hunk {
                old_range: (old_start, old_len),
                new_range: (new_start, new_len),
                lines,
            });
        }

        hunks
    }

    /// Get total number of diff lines (for scrolling)
    pub fn total_lines(&self) -> usize {
        self.hunks.iter().map(|h| h.lines.len()).sum()
    }
}

/// Interactive diff viewer state
pub struct InteractiveDiffViewer {
    /// All file diffs
    files: Vec<FileDiff>,
    /// Currently selected file index
    selected_file: usize,
    /// File list scroll state
    file_list_state: ListState,
    /// Diff view scroll offset
    diff_scroll: usize,
    /// Show help
    show_help: bool,
}

impl InteractiveDiffViewer {
    /// Create a new interactive diff viewer
    pub fn new(files: Vec<FileDiff>) -> Self {
        let mut file_list_state = ListState::default();
        if !files.is_empty() {
            file_list_state.select(Some(0));
        }

        Self {
            files,
            selected_file: 0,
            file_list_state,
            diff_scroll: 0,
            show_help: false,
        }
    }

    /// Run the interactive diff viewer
    pub fn run(&mut self) -> Result<()> {
        // Setup terminal
        enable_raw_mode().context("Failed to enable raw mode")?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
            .context("Failed to enter alternate screen")?;

        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend).context("Failed to create terminal")?;

        // Run app
        let res = self.run_app(&mut terminal);

        // Restore terminal
        disable_raw_mode().context("Failed to disable raw mode")?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )
        .context("Failed to leave alternate screen")?;
        terminal.show_cursor().context("Failed to show cursor")?;

        res
    }

    /// Main application loop
    fn run_app<B: ratatui::backend::Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<()> {
        loop {
            terminal.draw(|f| self.render(f))?;

            if let Event::Key(key) = event::read()? {
                // Check for Ctrl+C
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                    break;
                }

                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('?') => self.show_help = !self.show_help,
                    // Up/Down arrows and j/k switch between files (like vim)
                    KeyCode::Down | KeyCode::Char('j') if !self.show_help => self.next_file(),
                    KeyCode::Up | KeyCode::Char('k') if !self.show_help => self.prev_file(),
                    // Ctrl+F/B for full page scroll (vim-like)
                    KeyCode::Char('f')
                        if key.modifiers.contains(KeyModifiers::CONTROL) && !self.show_help =>
                    {
                        self.page_down_full()
                    }
                    KeyCode::Char('b')
                        if key.modifiers.contains(KeyModifiers::CONTROL) && !self.show_help =>
                    {
                        self.page_up_full()
                    }
                    // Ctrl+D/U for half page scroll (vim-like)
                    KeyCode::Char('d')
                        if key.modifiers.contains(KeyModifiers::CONTROL) && !self.show_help =>
                    {
                        self.page_down()
                    }
                    KeyCode::Char('u')
                        if key.modifiers.contains(KeyModifiers::CONTROL) && !self.show_help =>
                    {
                        self.page_up()
                    }
                    // PageUp/PageDown for full page scroll
                    KeyCode::PageDown if !self.show_help => self.page_down_full(),
                    KeyCode::PageUp if !self.show_help => self.page_up_full(),
                    // d/u for half page scroll
                    KeyCode::Char('d') if !self.show_help => self.page_down(),
                    KeyCode::Char('u') if !self.show_help => self.page_up(),
                    // n/N for next/previous hunk
                    KeyCode::Char('n') if !self.show_help => self.next_hunk(),
                    KeyCode::Char('N') if !self.show_help => self.prev_hunk(),
                    // Tab still works for next/previous file
                    KeyCode::Tab if !self.show_help => self.next_file(),
                    KeyCode::BackTab if !self.show_help => self.prev_file(),
                    // Home/End go to top/bottom of current file
                    KeyCode::Home if !self.show_help => self.scroll_to_top(),
                    KeyCode::End if !self.show_help => self.scroll_to_bottom(),
                    _ => {}
                }
            }
        }

        Ok(())
    }

    /// Render the UI
    fn render(&self, frame: &mut Frame) {
        if self.show_help {
            self.render_help(frame);
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(30), // File list
                Constraint::Percentage(70), // Diff view
            ])
            .split(frame.area());

        self.render_file_list(frame, chunks[0]);
        self.render_diff_view(frame, chunks[1]);
    }

    /// Render file list
    fn render_file_list(&self, frame: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self
            .files
            .iter()
            .map(|file| {
                let (icon, color) = match file.status {
                    FileStatus::Added => (Icons::ACTION_ADD, Color::Green),
                    FileStatus::Modified => (Icons::ACTION_MODIFY, Color::Yellow),
                    FileStatus::Deleted => (Icons::ACTION_REMOVE, Color::Red),
                };

                ListItem::new(Line::from(vec![
                    Span::styled(icon, Style::default().fg(color)),
                    Span::raw(format!(" {}", file.path)),
                ]))
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .title(format!(
                        " Files ({}/{}) ",
                        self.selected_file + 1,
                        self.files.len()
                    ))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("❯ ");

        let mut state = self.file_list_state.clone();
        frame.render_stateful_widget(list, area, &mut state);
    }

    /// Render diff view
    fn render_diff_view(&self, frame: &mut Frame, area: Rect) {
        if let Some(file) = self.files.get(self.selected_file) {
            let title = format!(" {} ", file.path);

            let mut lines = Vec::new();

            for hunk in &file.hunks {
                for diff_line in &hunk.lines {
                    let line = match diff_line {
                        DiffLine::Header {
                            old_range,
                            new_range,
                        } => Line::from(vec![Span::styled(
                            format!(
                                "@@ -{},{} +{},{} @@",
                                old_range.0, old_range.1, new_range.0, new_range.1
                            ),
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        )]),
                        DiffLine::Add { line_num, content } => {
                            let num_str = line_num
                                .map(|n| format!("{:4} ", n))
                                .unwrap_or_else(|| "     ".to_string());
                            Line::from(vec![
                                Span::styled(num_str, Style::default().fg(Color::DarkGray)),
                                Span::styled(
                                    "+",
                                    Style::default()
                                        .fg(Color::Green)
                                        .add_modifier(Modifier::BOLD),
                                ),
                                Span::styled(content.clone(), Style::default().fg(Color::Green)),
                            ])
                        }
                        DiffLine::Remove { line_num, content } => {
                            let num_str = line_num
                                .map(|n| format!("{:4} ", n))
                                .unwrap_or_else(|| "     ".to_string());
                            Line::from(vec![
                                Span::styled(num_str, Style::default().fg(Color::DarkGray)),
                                Span::styled(
                                    "-",
                                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                                ),
                                Span::styled(content.clone(), Style::default().fg(Color::Red)),
                            ])
                        }
                        DiffLine::Context { line_num, content } => {
                            let num_str = line_num
                                .map(|n| format!("{:4} ", n))
                                .unwrap_or_else(|| "     ".to_string());
                            Line::from(vec![
                                Span::styled(num_str, Style::default().fg(Color::DarkGray)),
                                Span::raw(" "),
                                Span::raw(content.clone()),
                            ])
                        }
                    };
                    lines.push(line);
                }

                // Add blank line between hunks
                lines.push(Line::from(""));
            }

            let paragraph = Paragraph::new(lines)
                .block(
                    Block::default()
                        .title(title)
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Cyan)),
                )
                .scroll((self.diff_scroll as u16, 0));

            frame.render_widget(paragraph, area);
        } else {
            let text = Text::from("No files to display");
            let paragraph = Paragraph::new(text)
                .block(Block::default().borders(Borders::ALL))
                .alignment(Alignment::Center);
            frame.render_widget(paragraph, area);
        }
    }

    /// Render help screen
    fn render_help(&self, frame: &mut Frame) {
        let help_text = vec![
            Line::from(vec![Span::styled(
                "Interactive Diff Viewer",
                Style::default().add_modifier(Modifier::BOLD),
            )]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Files:",
                Style::default().add_modifier(Modifier::UNDERLINED),
            )]),
            Line::from(vec![
                Span::styled("  j/↓       ", Style::default().fg(Color::Cyan)),
                Span::raw("Next file"),
            ]),
            Line::from(vec![
                Span::styled("  k/↑       ", Style::default().fg(Color::Cyan)),
                Span::raw("Previous file"),
            ]),
            Line::from(vec![
                Span::styled("  Tab       ", Style::default().fg(Color::Cyan)),
                Span::raw("Next file"),
            ]),
            Line::from(vec![
                Span::styled("  Shift+Tab ", Style::default().fg(Color::Cyan)),
                Span::raw("Previous file"),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Scroll Content:",
                Style::default().add_modifier(Modifier::UNDERLINED),
            )]),
            Line::from(vec![
                Span::styled("  Ctrl+F    ", Style::default().fg(Color::Cyan)),
                Span::raw("Scroll down full page"),
            ]),
            Line::from(vec![
                Span::styled("  Ctrl+B    ", Style::default().fg(Color::Cyan)),
                Span::raw("Scroll up full page"),
            ]),
            Line::from(vec![
                Span::styled("  Ctrl+D/d  ", Style::default().fg(Color::Cyan)),
                Span::raw("Scroll down half page"),
            ]),
            Line::from(vec![
                Span::styled("  Ctrl+U/u  ", Style::default().fg(Color::Cyan)),
                Span::raw("Scroll up half page"),
            ]),
            Line::from(vec![
                Span::styled("  PageDown  ", Style::default().fg(Color::Cyan)),
                Span::raw("Scroll down full page"),
            ]),
            Line::from(vec![
                Span::styled("  PageUp    ", Style::default().fg(Color::Cyan)),
                Span::raw("Scroll up full page"),
            ]),
            Line::from(vec![
                Span::styled("  Home      ", Style::default().fg(Color::Cyan)),
                Span::raw("Go to top of file"),
            ]),
            Line::from(vec![
                Span::styled("  End       ", Style::default().fg(Color::Cyan)),
                Span::raw("Go to bottom of file"),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Hunks:",
                Style::default().add_modifier(Modifier::UNDERLINED),
            )]),
            Line::from(vec![
                Span::styled("  n         ", Style::default().fg(Color::Cyan)),
                Span::raw("Next hunk"),
            ]),
            Line::from(vec![
                Span::styled("  N         ", Style::default().fg(Color::Cyan)),
                Span::raw("Previous hunk"),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Other:",
                Style::default().add_modifier(Modifier::UNDERLINED),
            )]),
            Line::from(vec![
                Span::styled("  ?         ", Style::default().fg(Color::Cyan)),
                Span::raw("Toggle help"),
            ]),
            Line::from(vec![
                Span::styled("  q/Esc     ", Style::default().fg(Color::Cyan)),
                Span::raw("Quit"),
            ]),
            Line::from(vec![
                Span::styled("  Ctrl+C    ", Style::default().fg(Color::Cyan)),
                Span::raw("Force quit"),
            ]),
        ];

        let paragraph = Paragraph::new(help_text)
            .block(
                Block::default()
                    .title(" Help (Press ? to close) ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow)),
            )
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: false });

        let area = centered_rect(60, 80, frame.area());
        frame.render_widget(ratatui::widgets::Clear, area);
        frame.render_widget(paragraph, area);
    }

    // Navigation methods
    fn page_down(&mut self) {
        // Half page scroll (vim Ctrl+D)
        if let Some(file) = self.files.get(self.selected_file) {
            let max_scroll = file.total_lines().saturating_sub(1);
            self.diff_scroll = (self.diff_scroll + 10).min(max_scroll);
        }
    }

    fn page_up(&mut self) {
        // Half page scroll (vim Ctrl+U)
        self.diff_scroll = self.diff_scroll.saturating_sub(10);
    }

    fn page_down_full(&mut self) {
        // Full page scroll (vim Ctrl+F)
        if let Some(file) = self.files.get(self.selected_file) {
            let max_scroll = file.total_lines().saturating_sub(1);
            self.diff_scroll = (self.diff_scroll + 20).min(max_scroll);
        }
    }

    fn page_up_full(&mut self) {
        // Full page scroll (vim Ctrl+B)
        self.diff_scroll = self.diff_scroll.saturating_sub(20);
    }

    fn scroll_to_top(&mut self) {
        self.diff_scroll = 0;
    }

    fn scroll_to_bottom(&mut self) {
        if let Some(file) = self.files.get(self.selected_file) {
            let max_scroll = file.total_lines().saturating_sub(1);
            self.diff_scroll = max_scroll;
        }
    }

    fn next_hunk(&mut self) {
        if let Some(file) = self.files.get(self.selected_file) {
            let mut current_line = 0;
            for hunk in &file.hunks {
                if current_line > self.diff_scroll {
                    self.diff_scroll = current_line;
                    return;
                }
                current_line += hunk.lines.len();
            }
        }
    }

    fn prev_hunk(&mut self) {
        if let Some(file) = self.files.get(self.selected_file) {
            let mut prev_hunk_start = 0;
            let mut current_line = 0;

            for hunk in &file.hunks {
                if current_line >= self.diff_scroll {
                    self.diff_scroll = prev_hunk_start;
                    return;
                }
                prev_hunk_start = current_line;
                current_line += hunk.lines.len();
            }
        }
    }

    fn next_file(&mut self) {
        if !self.files.is_empty() {
            // Cycle to first file if at the end
            if self.selected_file < self.files.len() - 1 {
                self.selected_file += 1;
            } else {
                self.selected_file = 0;
            }
            self.file_list_state.select(Some(self.selected_file));
            self.diff_scroll = 0;
        }
    }

    fn prev_file(&mut self) {
        if !self.files.is_empty() {
            // Cycle to last file if at the beginning
            if self.selected_file > 0 {
                self.selected_file -= 1;
            } else {
                self.selected_file = self.files.len() - 1;
            }
            self.file_list_state.select(Some(self.selected_file));
            self.diff_scroll = 0;
        }
    }
}

/// Helper function to create a centered rect
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
