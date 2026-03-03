use anyhow::{Context, Result};
use crossterm::terminal;
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

/// Attach to a running session via its Unix socket.
///
/// Enters raw terminal mode, bridges stdin/stdout to the socket,
/// and handles the detach escape sequence (Ctrl+B then d).
pub async fn attach(socket_path: &Path) -> Result<()> {
    let mut stream = UnixStream::connect(socket_path)
        .await
        .with_context(|| format!("Failed to connect to session at {}", socket_path.display()))?;

    // Get terminal size
    let (cols, rows) = terminal::size().unwrap_or((80, 24));

    // Send 4-byte size header
    let header = [
        (cols >> 8) as u8,
        (cols & 0xff) as u8,
        (rows >> 8) as u8,
        (rows & 0xff) as u8,
    ];
    stream.write_all(&header).await?;

    // Enter raw terminal mode
    terminal::enable_raw_mode()?;
    let _guard = RawModeGuard;

    let (mut sock_reader, mut sock_writer) = stream.into_split();

    // stdin → socket (with detach escape handling)
    let stdin_task = tokio::spawn(async move {
        let mut stdin = tokio::io::stdin();
        let mut buf = [0u8; 4096];
        let mut prefix_mode = false;

        loop {
            let n = match stdin.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => n,
                Err(_) => break,
            };

            let mut output = Vec::with_capacity(n);
            let mut detach = false;

            for i in 0..n {
                if prefix_mode {
                    if buf[i] == b'd' {
                        detach = true;
                        break;
                    }
                    // Not 'd' — forward the buffered Ctrl+B and this byte
                    output.push(0x02);
                    output.push(buf[i]);
                    prefix_mode = false;
                } else if buf[i] == 0x02 {
                    // Ctrl+B pressed
                    prefix_mode = true;
                } else {
                    output.push(buf[i]);
                }
            }

            if detach {
                break;
            }

            if !output.is_empty() {
                if sock_writer.write_all(&output).await.is_err() {
                    break;
                }
            }
        }
    });

    // socket → stdout
    let stdout_task = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        let mut buf = [0u8; 8192];

        loop {
            let n = match sock_reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => n,
                Err(_) => break,
            };

            if stdout.write_all(&buf[..n]).await.is_err() {
                break;
            }
            let _ = stdout.flush().await;
        }
    });

    // Wait for either direction to close
    tokio::select! {
        _ = stdin_task => {}
        _ = stdout_task => {}
    }

    // Guard's Drop will restore terminal
    Ok(())
}

/// Wait for a socket file to appear, with timeout.
pub async fn wait_for_socket(socket_path: &Path, timeout_secs: u64) -> Result<()> {
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs);

    while start.elapsed() < timeout {
        if socket_path.exists() {
            // Give the daemon a moment to start listening
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            return Ok(());
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }

    anyhow::bail!(
        "Timed out waiting for daemon socket at {}",
        socket_path.display()
    );
}

/// RAII guard that restores terminal mode on drop.
struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        // Print newline so the shell prompt starts clean
        eprintln!();
    }
}
