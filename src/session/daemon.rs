//! The per-session daemon (`acta __sessiond <id>`).
//!
//! Owns the agent's PTY, keeps an in-memory scrollback, appends raw output
//! to the session log, and serves attach clients over a unix socket. It runs
//! in its own kernel session (setsid), so the agent keeps working when every
//! client detaches or the launching terminal/SSH connection goes away.

use crate::protocol::{
    decode_resize, read_frame, write_frame, C2D_DETACH, C2D_KILL, C2D_RESIZE, C2D_STDIN, D2C_EXIT,
    D2C_OUTPUT,
};
use crate::session::{SessionManager, SessionStatus};
use anyhow::{Context, Result};
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::collections::VecDeque;
use std::fs;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::broadcast;

/// Bytes of scrollback replayed to a freshly attached client.
const SCROLLBACK_CAP: usize = 256 * 1024;

#[derive(Clone, Debug)]
enum Event {
    Output(Vec<u8>),
    Exit(i32),
}

struct Shared {
    scrollback: Mutex<VecDeque<u8>>,
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    child_pid: Option<u32>,
}

pub async fn run(id: u32) -> Result<()> {
    let manager = SessionManager::new()?;
    let mut session = manager.load(id)?;

    // Detach from the controlling terminal so the launching shell/SSH
    // connection dying never delivers SIGHUP to us or the agent.
    let _ = nix::unistd::setsid();

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .context("Failed to open PTY")?;

    let mut cmd = CommandBuilder::new(&session.command);
    cmd.args(&session.args);
    cmd.cwd(&session.cwd);
    for (key, value) in &session.env {
        cmd.env(key, value);
    }
    cmd.env("ACTA_SESSION", session.id.to_string());
    cmd.env("ACTA_SESSION_NAME", &session.name);

    let child = pair
        .slave
        .spawn_command(cmd)
        .with_context(|| format!("Failed to spawn agent command '{}'", session.command))?;
    drop(pair.slave);

    session.daemon_pid = Some(std::process::id());
    session.child_pid = child.process_id();
    session.status = SessionStatus::Running;
    manager.save(&session)?;

    let reader = pair
        .master
        .try_clone_reader()
        .context("Failed to clone PTY reader")?;
    let writer = pair
        .master
        .take_writer()
        .context("Failed to take PTY writer")?;

    let shared = Arc::new(Shared {
        scrollback: Mutex::new(VecDeque::with_capacity(SCROLLBACK_CAP)),
        writer: Mutex::new(writer),
        master: Mutex::new(pair.master),
        child_pid: child.process_id(),
    });

    let (events, _) = broadcast::channel::<Event>(4096);

    spawn_pty_reader(reader, shared.clone(), events.clone(), manager.log_path(id));
    let mut exit_rx = spawn_child_waiter(child);

    let sock_path = manager.sock_path(id);
    let _ = fs::remove_file(&sock_path);
    let listener = UnixListener::bind(&sock_path)
        .with_context(|| format!("Failed to bind {}", sock_path.display()))?;

    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;

    let exit_code = loop {
        tokio::select! {
            accepted = listener.accept() => {
                if let Ok((stream, _)) = accepted {
                    tokio::spawn(handle_client(stream, shared.clone(), events.clone()));
                }
            }
            code = &mut exit_rx => {
                break code.unwrap_or(-1);
            }
            _ = sigterm.recv() => signal_child(&shared, nix::sys::signal::Signal::SIGTERM),
            _ = sigint.recv() => signal_child(&shared, nix::sys::signal::Signal::SIGTERM),
        }
    };

    // Let the PTY reader drain any final output before announcing the exit.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let _ = events.send(Event::Exit(exit_code));
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let mut session = manager.load(id)?;
    session.status = SessionStatus::Exited(exit_code);
    manager.save(&session)?;
    let _ = fs::remove_file(&sock_path);
    Ok(())
}

fn spawn_pty_reader(
    mut reader: Box<dyn Read + Send>,
    shared: Arc<Shared>,
    events: broadcast::Sender<Event>,
    log_path: std::path::PathBuf,
) {
    std::thread::spawn(move || {
        let mut log = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .ok();
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let chunk = &buf[..n];
                    if let Some(log) = log.as_mut() {
                        let _ = log.write_all(chunk);
                    }
                    {
                        let mut sb = shared.scrollback.lock().unwrap();
                        sb.extend(chunk.iter().copied());
                        while sb.len() > SCROLLBACK_CAP {
                            sb.pop_front();
                        }
                    }
                    let _ = events.send(Event::Output(chunk.to_vec()));
                }
            }
        }
    });
}

fn spawn_child_waiter(
    mut child: Box<dyn portable_pty::Child + Send + Sync>,
) -> tokio::sync::oneshot::Receiver<i32> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::task::spawn_blocking(move || {
        let code = child
            .wait()
            .map(|status| status.exit_code() as i32)
            .unwrap_or(-1);
        let _ = tx.send(code);
    });
    rx
}

fn signal_child(shared: &Shared, signal: nix::sys::signal::Signal) {
    if let Some(pid) = shared.child_pid {
        let _ = nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), signal);
    }
}

async fn handle_client(
    stream: UnixStream,
    shared: Arc<Shared>,
    events: broadcast::Sender<Event>,
) -> Result<()> {
    let mut rx = events.subscribe();
    let (mut rd, mut wr) = stream.into_split();

    // Replay scrollback so a re-attaching client sees where the agent is.
    let snapshot: Vec<u8> = {
        let sb = shared.scrollback.lock().unwrap();
        sb.iter().copied().collect()
    };
    if !snapshot.is_empty() {
        write_frame(&mut wr, D2C_OUTPUT, &snapshot).await?;
    }

    loop {
        tokio::select! {
            event = rx.recv() => match event {
                Ok(Event::Output(chunk)) => {
                    write_frame(&mut wr, D2C_OUTPUT, &chunk).await?;
                }
                Ok(Event::Exit(code)) => {
                    let _ = write_frame(&mut wr, D2C_EXIT, code.to_string().as_bytes()).await;
                    break;
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            },
            frame = read_frame(&mut rd) => match frame? {
                None => break, // client went away
                Some(frame) => match frame.kind {
                    C2D_STDIN => {
                        let mut writer = shared.writer.lock().unwrap();
                        writer.write_all(&frame.payload)?;
                        writer.flush()?;
                    }
                    C2D_RESIZE => {
                        if let Some((cols, rows)) = decode_resize(&frame.payload) {
                            let master = shared.master.lock().unwrap();
                            let _ = master.resize(PtySize {
                                rows,
                                cols,
                                pixel_width: 0,
                                pixel_height: 0,
                            });
                        }
                    }
                    C2D_DETACH => break,
                    C2D_KILL => signal_child(&shared, nix::sys::signal::Signal::SIGTERM),
                    _ => {}
                },
            },
        }
    }
    Ok(())
}
