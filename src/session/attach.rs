//! Attach client: connects to a session daemon's socket, bridges the local
//! terminal to the agent's PTY, and detaches on Ctrl-\ leaving the agent
//! running.

use crate::protocol::{
    encode_resize, read_frame, write_frame, C2D_DETACH, C2D_RESIZE, C2D_STDIN, D2C_EXIT, D2C_OUTPUT,
};
use crate::session::Session;
use anyhow::{Context, Result};
use std::io::{IsTerminal, Read, Write};
use std::path::Path;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;
use tokio::sync::mpsc;

/// Ctrl-\ (FS). Chosen because coding agents rarely bind it, unlike Ctrl-b/Ctrl-a.
const DETACH_BYTE: u8 = 0x1c;

pub enum Outcome {
    Detached,
    Exited(i32),
    ConnectionClosed,
}

struct RawModeGuard {
    active: bool,
}

impl RawModeGuard {
    fn new(enable: bool) -> Result<Self> {
        if enable {
            crossterm::terminal::enable_raw_mode()?;
        }
        Ok(Self { active: enable })
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = crossterm::terminal::disable_raw_mode();
        }
    }
}

pub async fn run(session: &Session, sock_path: &Path) -> Result<Outcome> {
    let stream = UnixStream::connect(sock_path)
        .await
        .with_context(|| format!("Could not connect to session {}", session.id))?;
    let (mut rd, mut wr) = stream.into_split();

    let interactive = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();

    if interactive {
        let (cols, rows) = crossterm::terminal::size()?;
        write_frame(&mut wr, C2D_RESIZE, &encode_resize(cols, rows)).await?;
        eprintln!(
            "[acta] attached to session {} ({}) — detach: Ctrl-\\",
            session.id, session.name
        );
    }

    let _raw = RawModeGuard::new(interactive)?;

    // Blocking stdin reader feeding the async loop.
    let (stdin_tx, mut stdin_rx) = mpsc::channel::<Vec<u8>>(64);
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin();
        let mut buf = [0u8; 4096];
        loop {
            match stdin.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if stdin_tx.blocking_send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let mut sigwinch =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::window_change())?;
    let mut stdout = tokio::io::stdout();
    let mut stdin_open = true;

    let outcome = loop {
        tokio::select! {
            frame = read_frame(&mut rd) => match frame? {
                None => break Outcome::ConnectionClosed,
                Some(frame) => match frame.kind {
                    D2C_OUTPUT => {
                        stdout.write_all(&frame.payload).await?;
                        stdout.flush().await?;
                    }
                    D2C_EXIT => {
                        let code = String::from_utf8_lossy(&frame.payload)
                            .parse::<i32>()
                            .unwrap_or(-1);
                        break Outcome::Exited(code);
                    }
                    _ => {}
                },
            },
            chunk = stdin_rx.recv(), if stdin_open => match chunk {
                None => stdin_open = false,
                Some(bytes) => {
                    if interactive {
                        if let Some(pos) = bytes.iter().position(|&b| b == DETACH_BYTE) {
                            if pos > 0 {
                                write_frame(&mut wr, C2D_STDIN, &bytes[..pos]).await?;
                            }
                            let _ = write_frame(&mut wr, C2D_DETACH, &[]).await;
                            break Outcome::Detached;
                        }
                    }
                    write_frame(&mut wr, C2D_STDIN, &bytes).await?;
                }
            },
            _ = sigwinch.recv(), if interactive => {
                let (cols, rows) = crossterm::terminal::size()?;
                write_frame(&mut wr, C2D_RESIZE, &encode_resize(cols, rows)).await?;
            }
        }
    };

    // Make sure raw output doesn't leave the cursor mid-line.
    drop(_raw);
    if interactive {
        let mut out = std::io::stdout();
        let _ = out.write_all(b"\r\n");
        let _ = out.flush();
    }
    Ok(outcome)
}
