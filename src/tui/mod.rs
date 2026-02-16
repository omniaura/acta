use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
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
use crate::session::SessionManager;

pub struct App {
    manager: SessionManager,
    selected: usize,
}

impl App {
    pub fn new() -> Result<Self> {
        Ok(Self {
            manager: SessionManager::new()?,
            selected: 0,
        })
    }

    fn select_next(&mut self) {
        let sessions = self.manager.list_sessions();
        if !sessions.is_empty() {
            self.selected = (self.selected + 1) % sessions.len();
        }
    }

    fn select_previous(&mut self) {
        let sessions = self.manager.list_sessions();
        if !sessions.is_empty() {
            if self.selected > 0 {
                self.selected -= 1;
            } else {
                self.selected = sessions.len() - 1;
            }
        }
    }
}

pub async fn run() -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut app = App::new()?;

    // Run app
    let res = run_app(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Error: {}", err);
    }

    Ok(())
}

async fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => return Ok(()),
                KeyCode::Down | KeyCode::Char('j') => app.select_next(),
                KeyCode::Up | KeyCode::Char('k') => app.select_previous(),
                _ => {}
            }
        }
    }
}

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(f.area());

    // Header
    let header = Paragraph::new("Acta - Agentic Terminal Multiplexer")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL).title("Header"));
    f.render_widget(header, chunks[0]);

    // Sessions list
    let sessions = app.manager.list_sessions();
    let items: Vec<ListItem> = sessions
        .iter()
        .enumerate()
        .map(|(i, session)| {
            let content = format!(
                "{} {} [{}] {}",
                &session.id[..8],
                session.agent,
                format!("{:?}", session.status),
                session.name.as_deref().unwrap_or("-")
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

    let sessions_list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(format!(
            "Sessions ({}/{})",
            app.selected + 1,
            sessions.len().max(1)
        )));

    f.render_widget(sessions_list, chunks[1]);

    // Footer
    let footer = Paragraph::new("q: quit | ↑/k: up | ↓/j: down | Enter: attach (not implemented)")
        .style(Style::default().fg(Color::Gray))
        .block(Block::default().borders(Borders::ALL).title("Help"));
    f.render_widget(footer, chunks[2]);
}
