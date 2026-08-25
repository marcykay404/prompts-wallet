use std::io::{Stdout, Write, stdout};

use anyhow::Result;
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{DisableBracketedPaste, EnableBracketedPaste},
    execute,
    terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode, size},
};
use ratatui::{
    Frame, Terminal, TerminalOptions, Viewport,
    backend::CrosstermBackend,
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::{
    app::{App, NewField, Screen},
    usage::UsageEntry,
};

type InlineTerminal = Terminal<CrosstermBackend<Stdout>>;

pub struct InlineUi {
    terminal: Option<InlineTerminal>,
    viewport_index: usize,
    height: u16,
}

impl InlineUi {
    const HEIGHTS: [u16; 3] = [10, 20, 40];

    pub fn new(preferred_height: u16) -> Result<Self> {
        let viewport_index = Self::HEIGHTS
            .iter()
            .position(|height| *height >= preferred_height)
            .unwrap_or(Self::HEIGHTS.len() - 1);
        let height = clamp_height(Self::HEIGHTS[viewport_index]);
        enable_raw_mode()?;
        if let Err(error) = execute!(stdout(), EnableBracketedPaste, Hide) {
            let _ = disable_raw_mode();
            return Err(error.into());
        }
        let terminal = match create_terminal(height) {
            Ok(terminal) => terminal,
            Err(error) => {
                let _ = disable_raw_mode();
                let _ = execute!(stdout(), DisableBracketedPaste, Show);
                return Err(error);
            }
        };
        Ok(Self {
            terminal: Some(terminal),
            viewport_index,
            height,
        })
    }

    pub fn draw(&mut self, app: &App) -> Result<()> {
        if let Some(terminal) = self.terminal.as_mut() {
            terminal.draw(|frame| draw_app(frame, app))?;
        }
        Ok(())
    }

    pub fn cycle_height(&mut self) -> Result<()> {
        let anchor = self.suspend()?;
        self.viewport_index = (self.viewport_index + 1) % Self::HEIGHTS.len();
        self.height = clamp_height(Self::HEIGHTS[self.viewport_index]);
        self.resume(anchor)
    }

    pub fn suspend(&mut self) -> Result<u16> {
        let Some(mut terminal) = self.terminal.take() else {
            return Ok(0);
        };
        let area = terminal.get_frame().area();
        let cursor_result = terminal.show_cursor();
        let flush_result = terminal.flush();
        drop(terminal);

        let mut output = stdout();
        let cleanup_result = execute!(
            output,
            MoveTo(0, area.y),
            Clear(ClearType::FromCursorDown),
            DisableBracketedPaste,
            Show
        );
        let output_result = output.flush();
        let raw_result = disable_raw_mode();
        cursor_result?;
        flush_result?;
        cleanup_result?;
        output_result?;
        raw_result?;
        Ok(area.y)
    }

    pub fn resume(&mut self, anchor: u16) -> Result<()> {
        let (_, terminal_height) = size()?;
        let anchor = anchor.min(terminal_height.saturating_sub(1));
        self.height = clamp_height(Self::HEIGHTS[self.viewport_index]);
        let mut output = stdout();
        execute!(
            output,
            MoveTo(0, anchor),
            Clear(ClearType::FromCursorDown),
            EnableBracketedPaste,
            Hide
        )?;
        output.flush()?;
        enable_raw_mode()?;
        self.terminal = Some(create_terminal(self.height)?);
        Ok(())
    }

    pub fn finish(&mut self) -> Result<()> {
        self.suspend().map(|_| ())
    }
}

impl Drop for InlineUi {
    fn drop(&mut self) {
        let _ = self.suspend();
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), DisableBracketedPaste, Show);
    }
}

fn clamp_height(requested: u16) -> u16 {
    let terminal_height = size().map(|(_, height)| height).unwrap_or(requested);
    requested.min(terminal_height.saturating_sub(1).max(3))
}

fn create_terminal(height: u16) -> Result<InlineTerminal> {
    Ok(Terminal::with_options(
        CrosstermBackend::new(stdout()),
        TerminalOptions {
            viewport: Viewport::Inline(height),
        },
    )?)
}

pub fn draw_app(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    let title = app
        .notice
        .as_deref()
        .map(|notice| format!(" Prompt Wallet — {notice} "))
        .unwrap_or_else(|| " Prompt Wallet ".into());
    let block = Block::default().title(title).borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    match app.screen {
        Screen::Home => draw_home(frame, inner, app),
        Screen::Search => draw_search(frame, inner, app),
        Screen::Variables => draw_variables(frame, inner, app),
        Screen::Preview => draw_preview(frame, inner, app),
        Screen::NewPrompt => draw_new_prompt(frame, inner, app),
        Screen::Help => draw_help(frame, inner, app),
    }
}

fn draw_home(frame: &mut Frame<'_>, area: ratatui::layout::Rect, app: &App) {
    let [body, footer] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);
    let indices = app.home_indices();
    if indices.is_empty() {
        frame.render_widget(
            Paragraph::new("No prompts yet. Press n to create your first prompt."),
            body,
        );
    } else {
        let items: Vec<_> = indices
            .iter()
            .enumerate()
            .map(|(position, index)| {
                let prompt = &app.prompts[*index];
                let UsageEntry { use_count, .. } = app.usage.entry(&prompt.metadata.id);
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("{}  ", position + 1),
                        Style::default().fg(Color::Cyan),
                    ),
                    Span::raw(prompt.metadata.title.clone()),
                    Span::styled(
                        format!("  used {use_count}"),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]))
            })
            .collect();
        let mut state = ListState::default().with_selected(Some(app.home_selected));
        frame.render_stateful_widget(
            List::new(items)
                .highlight_symbol("› ")
                .highlight_style(Style::default().add_modifier(Modifier::BOLD)),
            body,
            &mut state,
        );
    }
    frame.render_widget(
        Paragraph::new("↑↓ select  Enter/1–5 open  s search  n new  e edit  v size  ? help  q quit")
            .style(Style::default().fg(Color::DarkGray)),
        footer,
    );
}

fn draw_search(frame: &mut Frame<'_>, area: ratatui::layout::Rect, app: &App) {
    let [query, results, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Search: ", Style::default().fg(Color::Cyan)),
            Span::raw(&app.search_query),
            Span::styled("▌", Style::default().fg(Color::Cyan)),
        ])),
        query,
    );
    let items: Vec<_> = app
        .search_hits
        .iter()
        .map(|hit| {
            let prompt = &app.prompts[hit.index];
            let usage = app.usage.entry(&prompt.metadata.id);
            ListItem::new(format!(
                "{}  used {}",
                prompt.metadata.title, usage.use_count
            ))
        })
        .collect();
    let mut state = ListState::default().with_selected(Some(app.search_selected));
    frame.render_stateful_widget(
        List::new(items)
            .highlight_symbol("› ")
            .highlight_style(Style::default().add_modifier(Modifier::BOLD)),
        results,
        &mut state,
    );
    frame.render_widget(
        Paragraph::new("type to search  ↑↓ select  Enter open  Ctrl-E edit  Alt-V size  Esc home")
            .style(Style::default().fg(Color::DarkGray)),
        footer,
    );
}

fn draw_variables(frame: &mut Frame<'_>, area: ratatui::layout::Rect, app: &App) {
    let [title, fields, input, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(2),
        Constraint::Length(1),
    ])
    .areas(area);
    let prompt_title = app
        .selected_prompt()
        .map(|prompt| prompt.metadata.title.as_str())
        .unwrap_or("Prompt");
    frame.render_widget(
        Paragraph::new(prompt_title).style(Style::default().add_modifier(Modifier::BOLD)),
        title,
    );
    let lines: Vec<_> = app
        .variable_names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let marker = if index == app.variable_index {
                "›"
            } else {
                " "
            };
            let value = app
                .variable_values
                .get(name)
                .map(|value| value.replace('\n', "↵"))
                .unwrap_or_default();
            Line::from(format!("{marker} {name}: {value}"))
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), fields);
    let current = app
        .variable_names
        .get(app.variable_index)
        .map(String::as_str)
        .unwrap_or("value");
    frame.render_widget(
        Paragraph::new(app.variable_input.as_str())
            .block(Block::default().title(current).borders(Borders::ALL)),
        input,
    );
    frame.render_widget(
        Paragraph::new("Enter next  Shift-Tab previous  Ctrl-R render  Alt-V size  Esc back")
            .style(Style::default().fg(Color::DarkGray)),
        footer,
    );
}

fn draw_preview(frame: &mut Frame<'_>, area: ratatui::layout::Rect, app: &App) {
    let [body, footer] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);
    frame.render_widget(
        Paragraph::new(app.rendered_prompt().unwrap_or_default())
            .wrap(Wrap { trim: false })
            .scroll((app.preview_scroll, 0)),
        body,
    );
    frame.render_widget(
        Paragraph::new(
            "↑↓ scroll  c copy  C copy+exit  p print  e edit  b back  v size  ? help  q quit",
        )
        .style(Style::default().fg(Color::DarkGray)),
        footer,
    );
}

fn draw_help(frame: &mut Frame<'_>, area: ratatui::layout::Rect, app: &App) {
    let [body, footer] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);
    frame.render_widget(
        Paragraph::new(app.help_text())
            .wrap(Wrap { trim: false })
            .scroll((app.help_scroll, 0)),
        body,
    );
    frame.render_widget(
        Paragraph::new("↑↓ scroll  Esc/q/? close").style(Style::default().fg(Color::DarkGray)),
        footer,
    );
}

fn draw_new_prompt(frame: &mut Frame<'_>, area: ratatui::layout::Rect, app: &App) {
    let [title, tags, help, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(area);
    let active = Style::default().fg(Color::Cyan);
    frame.render_widget(
        Paragraph::new(app.new_title.as_str()).block(
            Block::default()
                .title("Title")
                .borders(Borders::ALL)
                .border_style(if app.new_field == NewField::Title {
                    active
                } else {
                    Style::default()
                }),
        ),
        title,
    );
    frame.render_widget(
        Paragraph::new(app.new_tags.as_str()).block(
            Block::default()
                .title("Tags, comma-separated")
                .borders(Borders::ALL)
                .border_style(if app.new_field == NewField::Tags {
                    active
                } else {
                    Style::default()
                }),
        ),
        tags,
    );
    frame.render_widget(
        Paragraph::new("After these fields, pwt opens the draft in your configured editor."),
        help,
    );
    frame.render_widget(
        Paragraph::new("Enter/Tab continue  Shift-Tab previous  Alt-V size  Esc cancel")
            .style(Style::default().fg(Color::DarkGray)),
        footer,
    );
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use ratatui::{Terminal, backend::TestBackend};
    use uuid::Uuid;

    use super::*;
    use crate::{
        model::{Prompt, PromptMetadata},
        storage::{Vault, WalletPaths},
        usage::UsageStore,
    };

    fn render_with_height(app: &App, height: u16) -> String {
        let backend = TestBackend::new(100, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw_app(frame, app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    fn render(app: &App) -> String {
        render_with_height(app, 10)
    }

    #[test]
    fn compact_home_render_contains_prompt_and_keyboard_help() {
        let prompt = Prompt {
            metadata: PromptMetadata {
                id: Uuid::new_v4(),
                title: "Security review".into(),
                tags: vec![],
                aliases: vec![],
            },
            body: "Review this code.".into(),
            path: PathBuf::new(),
        };
        let app = App::new(
            Vault {
                prompts: vec![prompt],
                warnings: vec![],
            },
            UsageStore::default(),
            WalletPaths::from_root("/test-root"),
        );

        let rendered = render(&app);

        assert!(rendered.contains("Security review"));
        assert!(rendered.contains("s search"));
        assert!(rendered.contains("v size"));
        assert!(rendered.contains("? help"));
    }

    #[test]
    fn help_screen_shows_explanation_and_file_locations() {
        let mut app = App::new(
            Vault::default(),
            UsageStore::default(),
            WalletPaths::from_root("/test-root"),
        );
        app.screen = Screen::Help;

        // The help screen is long; render at the wallet's largest inline
        // viewport (see InlineUi::HEIGHTS) so the whole text is on screen
        // instead of testing scroll behavior pixel-by-pixel.
        let rendered = render_with_height(&app, 40);

        assert!(rendered.contains("Search"));
        assert!(rendered.contains("/test-root/prompts"));
        assert!(rendered.contains("close"));
    }
}
