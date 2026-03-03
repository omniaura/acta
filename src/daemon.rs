use anyhow::{Context, Result};
use std::io;
use std::os::unix::io::{FromRawFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::Stdio;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::sync::broadcast;
use tracing::{error, info, warn};

use crate::config::Config;
use crate::session::{SessionManager, SessionStatus};

/// Run the per-session daemon.
///
/// This creates a PTY, spawns the agent process, listens on a Unix socket,
/// and bridges I/O between connected clients and the PTY.
pub async fn run(session_id: String) -> Result<()> {
    // Load session metadata
    let mut manager = SessionManager::new()?;
    let session = manager
        .get_session(&session_id)
        .context("Session not found")?
        .clone();

    let socket_path = session.socket_path();
    let worktree_path = session.worktree_path.clone();

    // Resolve agent command from config
    let config = Config::load().unwrap_or_default();
    let (agent_cmd, agent_args) = if let Some(plugin) = config.plugins.get(&session.agent) {
        let mut args = plugin.args.clone();
        args.extend(session.args.clone());
        (plugin.command.clone(), args)
    } else {
        (session.agent.clone(), session.args.clone())
    };

    // For interactive agents, add common flags
    if agent_cmd == "claude" && !agent_args.iter().any(|a| a == "--no-input") {
        // Claude Code runs interactively by default
    }

    info!(
        "Starting daemon for session {} (agent: {}, worktree: {})",
        session_id,
        agent_cmd,
        worktree_path.display()
    );

    // Create PTY pair
    let (master_fd, slave_fd) = create_pty(80, 24)?;
    info!("Created PTY pair: master={}, slave={}", master_fd, slave_fd);

    // Spawn agent process
    let slave_in = unsafe { libc::dup(slave_fd) };
    let slave_out = unsafe { libc::dup(slave_fd) };
    let master_for_child = master_fd;

    let mut child: std::process::Child = unsafe {
        let result = std::process::Command::new(&agent_cmd)
            .args(&agent_args)
            .current_dir(&worktree_path)
            .stdin(Stdio::from_raw_fd(slave_in))
            .stdout(Stdio::from_raw_fd(slave_out))
            .stderr(Stdio::from_raw_fd(slave_fd))
            .pre_exec(move || {
                libc::close(master_for_child);
                if libc::setsid() < 0 {
                    return Err(io::Error::last_os_error());
                }
                if libc::ioctl(0, libc::TIOCSCTTY as libc::c_ulong, 0i32) < 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            })
            .spawn();

        match result {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to spawn agent: {}", e);
                let mut session = session.clone();
                session.status = SessionStatus::Failed;
                manager.update_session(session)?;
                return Err(e.into());
            }
        }
    };

    let child_pid = child.id();
    info!("Agent spawned with PID {}", child_pid);

    // Update session with PID and Running status
    let mut session = session.clone();
    session.pid = Some(child_pid);
    session.status = SessionStatus::Running;
    manager.update_session(session)?;

    // Clean up stale socket
    if socket_path.exists() {
        std::fs::remove_file(&socket_path).ok();
    }

    // Create Unix socket listener
    let listener = UnixListener::bind(&socket_path)
        .with_context(|| format!("Failed to bind socket at {}", socket_path.display()))?;
    info!("Listening on {}", socket_path.display());

    // Broadcast channel for PTY output
    let (tx, _) = broadcast::channel::<Vec<u8>>(512);

    // PTY reader thread: continuously reads from master and broadcasts
    let tx_clone = tx.clone();
    let read_fd = master_fd;
    let reader_handle = std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            let n = unsafe { libc::read(read_fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
            if n <= 0 {
                break;
            }
            let _ = tx_clone.send(buf[..n as usize].to_vec());
        }
    });

    // Set up SIGTERM handler
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;

    // Main loop: accept clients and handle I/O
    loop {
        tokio::select! {
            // Accept new client
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((stream, _)) => {
                        info!("Client connected");
                        handle_client(stream, master_fd, &tx).await;
                        info!("Client disconnected");
                    }
                    Err(e) => {
                        warn!("Accept error: {}", e);
                    }
                }
            }

            // SIGTERM: graceful shutdown
            _ = sigterm.recv() => {
                info!("Received SIGTERM, shutting down");
                break;
            }

            // Check if child exited (poll periodically)
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(500)) => {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        info!("Agent exited with status: {:?}", status);
                        break;
                    }
                    Ok(None) => {} // Still running
                    Err(e) => {
                        error!("Error checking child status: {}", e);
                        break;
                    }
                }
            }
        }
    }

    // Cleanup
    info!("Daemon shutting down");

    // Kill agent if still running
    let _ = child.kill();
    let _ = child.wait();

    // Close master PTY
    unsafe { libc::close(master_fd) };

    // Wait for reader thread
    let _ = reader_handle.join();

    // Remove socket
    std::fs::remove_file(&socket_path).ok();

    // Update session status
    let mut manager = SessionManager::new()?;
    if let Some(session) = manager.get_session(&session_id) {
        let mut session = session.clone();
        session.status = SessionStatus::Stopped;
        session.pid = None;
        manager.update_session(session)?;
    }

    Ok(())
}

/// Handle a single client connection.
async fn handle_client(
    mut stream: tokio::net::UnixStream,
    master_fd: RawFd,
    tx: &broadcast::Sender<Vec<u8>>,
) {
    // Read 4-byte size header: [cols_hi, cols_lo, rows_hi, rows_lo]
    let mut header = [0u8; 4];
    if stream.read_exact(&mut header).await.is_err() {
        return;
    }

    let cols = u16::from_be_bytes([header[0], header[1]]);
    let rows = u16::from_be_bytes([header[2], header[3]]);
    set_pty_size(master_fd, cols, rows);

    let (mut sock_reader, mut sock_writer) = stream.into_split();

    // Subscribe to PTY output
    let mut rx = tx.subscribe();

    // Forward PTY output → client
    let fwd_task = tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(data) => {
                    if sock_writer.write_all(&data).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("Client lagged by {} messages", n);
                    // Continue receiving
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // Forward client input → PTY
    let write_fd = master_fd;
    let input_task = tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        loop {
            match sock_reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let written =
                        unsafe { libc::write(write_fd, buf.as_ptr() as *const libc::c_void, n) };
                    if written < 0 {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Wait for either direction to close
    tokio::select! {
        _ = fwd_task => {}
        _ = input_task => {}
    }
}

/// Create a PTY pair with initial window size.
fn create_pty(cols: u16, rows: u16) -> Result<(RawFd, RawFd)> {
    let mut master: libc::c_int = 0;
    let mut slave: libc::c_int = 0;
    let ws = libc::winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    let ret = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &ws as *const libc::winsize,
        )
    };

    if ret < 0 {
        anyhow::bail!(
            "openpty failed: {}",
            io::Error::last_os_error()
        );
    }

    Ok((master, slave))
}

/// Set the window size on a PTY master.
fn set_pty_size(master_fd: RawFd, cols: u16, rows: u16) {
    let ws = libc::winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    unsafe {
        libc::ioctl(master_fd, libc::TIOCSWINSZ as libc::c_ulong, &ws);
    }
}
