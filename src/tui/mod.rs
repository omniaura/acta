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

    fn refresh(&mut self) {
        if let Ok(m) = SessionManager::new() {
            self.manager = m;
        }
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

    fn selected_session_id(&self) -> Option<String> {
        let sessions = self.manager.list_sessions();
        sessions.get(self.selected).map(|s| s.id.clone())
    }
}

pub async fn run() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new()?;
    let res = run_app(&mut terminal, &mut app).await;

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

async fn run_app<B: Backend + io::Write>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        if event::poll(std::time::Duration::from_secs(2))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Down | KeyCode::Char('j') => app.select_next(),
                    KeyCode::Up | KeyCode::Char('k') => app.select_previous(),
                    KeyCode::Char('r') => app.refresh(),
                    KeyCode::Enter => {
                        if let Some(id) = app.selected_session_id() {
                            disable_raw_mode()?;
                            execute!(
                                terminal.backend_mut(),
                                LeaveAlternateScreen,
                                DisableMouseCapture
                            )?;
                            terminal.show_cursor()?;

                            let sessions = app.manager.list_sessions();
                            if let Some(sess) = sessions.iter().find(|s| s.id == id) {
                                if sess.is_alive() {
                                    let socket_path = sess.socket_path();
                                    eprintln!(
                                        "Attaching to '{}'... (Ctrl+B d to detach)\n",
                                        id
                                    );
                                    let _ = crate::client::attach(&socket_path).await;
                                    eprintln!("[detached from '{}']", id);
                                }
                            }

                            enable_raw_mode()?;
                            execute!(
                                terminal.backend_mut(),
                                EnterAlternateScreen,
                                EnableMouseCapture
                            )?;
                            app.refresh();
                        }
                    }
                    _ => {}
                }
            }
        } else {
            app.refresh();
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

    let header = Paragraph::new("Acta \u{2014} Agentic Terminal Multiplexer")
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(header, chunks[0]);

    let sessions = app.manager.list_sessions();
    let items: Vec<ListItem> = sessions
        .iter()
        .enumerate()
        .map(|(i, session)| {
            let status = session.effective_status();
            let status_color = match status {
                crate::session::SessionStatus::Running => Color::Green,
                crate::session::SessionStatus::Starting => Color::Yellow,
                crate::session::SessionStatus::Stopped => Color::Red,
                crate::session::SessionStatus::Failed => Color::Red,
            };

            let pid_str = session
                .pid
                .filter(|_| session.is_alive())
                .map(|p| format!("PID {}", p))
                .unwrap_or_default();

            let content = format!(
                " {} | {:<10} | {:<10} | {}",
                session.id, session.agent, status, pid_str,
            );

            let style = if i == app.selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(status_color)
            };

            ListItem::new(content).style(style)
        })
        .collect();

    let count = sessions.len();
    let sessions_list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" Sessions ({}) ", count)),
    );
    f.render_widget(sessions_list, chunks[1]);

    let footer = Paragraph::new("q:quit  j/k:navigate  Enter:attach  r:refresh")
        .style(Style::default().fg(Color::DarkGray))
        .block(Block::default().borders(Borders::ALL).title(" Keys "));
    f.render_widget(footer, chunks[2]);
}
