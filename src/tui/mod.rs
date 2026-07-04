//! Interactive session picker with vim motions (j/k/gg/G, Enter attaches).

use crate::session::{Session, SessionManager};
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame, Terminal,
};
use std::io;

struct App {
    sessions: Vec<Session>,
    selected: usize,
    pending_g: bool,
}

enum Action {
    Quit,
    Attach(u32),
}

impl App {
    fn new() -> Result<Self> {
        let manager = SessionManager::new()?;
        Ok(Self {
            sessions: manager.list()?,
            selected: 0,
            pending_g: false,
        })
    }

    fn refresh(&mut self) -> Result<()> {
        let manager = SessionManager::new()?;
        self.sessions = manager.list()?;
        if !self.sessions.is_empty() {
            self.selected = self.selected.min(self.sessions.len() - 1);
        } else {
            self.selected = 0;
        }
        Ok(())
    }

    fn select_next(&mut self) {
        if !self.sessions.is_empty() {
            self.selected = (self.selected + 1) % self.sessions.len();
        }
    }

    fn select_previous(&mut self) {
        if !self.sessions.is_empty() {
            self.selected = self
                .selected
                .checked_sub(1)
                .unwrap_or(self.sessions.len() - 1);
        }
    }
}

pub async fn run() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new()?;
    let action = run_app(&mut terminal, &mut app).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    match action? {
        Action::Quit => Ok(()),
        Action::Attach(id) => crate::cli::attach_session(id).await,
    }
}

async fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<Action> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            let was_pending_g = app.pending_g;
            app.pending_g = false;
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(Action::Quit),
                KeyCode::Down | KeyCode::Char('j') => app.select_next(),
                KeyCode::Up | KeyCode::Char('k') => app.select_previous(),
                KeyCode::Char('g') => {
                    if was_pending_g {
                        app.selected = 0;
                    } else {
                        app.pending_g = true;
                    }
                }
                KeyCode::Char('G') => {
                    app.selected = app.sessions.len().saturating_sub(1);
                }
                KeyCode::Char('r') => app.refresh()?,
                KeyCode::Enter => {
                    if let Some(session) = app.sessions.get(app.selected) {
                        return Ok(Action::Attach(session.id));
                    }
                }
                _ => {}
            }
        }
    }
}

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(f.area());

    let header = Paragraph::new("Acta — sessions")
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(header, chunks[0]);

    let items: Vec<ListItem> = app
        .sessions
        .iter()
        .enumerate()
        .map(|(i, session)| {
            let content = format!(
                "{:<4} {:<20} {:<12} {:<12} {}",
                session.id,
                session.name,
                session.agent,
                session.status.label(),
                session.cwd.display()
            );
            let style = if i == app.selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(content).style(style)
        })
        .collect();

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(format!(
        "Sessions ({}/{})",
        if app.sessions.is_empty() {
            0
        } else {
            app.selected + 1
        },
        app.sessions.len()
    )));
    f.render_widget(list, chunks[1]);

    let footer = Paragraph::new("j/k move · gg/G top/bottom · Enter attach · r refresh · q quit")
        .style(Style::default().fg(Color::Gray))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(footer, chunks[2]);
}
