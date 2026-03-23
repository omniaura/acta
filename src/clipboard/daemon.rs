use crate::clipboard::protocol::{Request, Response};
use crate::clipboard::state::ClipboardQueue;
use crate::clipboard::ClipboardPaths;
use anyhow::{Context, Result};
use arboard::Clipboard;
use std::process;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Mutex;

pub async fn run(paths: ClipboardPaths) -> Result<()> {
    paths.ensure_parent_dirs()?;

    if paths.socket_path.exists() {
        let _ = std::fs::remove_file(&paths.socket_path);
    }

    let listener = UnixListener::bind(&paths.socket_path)
        .with_context(|| format!("Failed to bind {}", paths.socket_path.display()))?;

    let queue = ClipboardQueue::load(&paths.state_path)?;
    let state = Arc::new(Mutex::new(DaemonState {
        queue,
        state_path: paths.state_path.clone(),
    }));

    loop {
        let (stream, _) = listener.accept().await.context("Failed to accept UDS connection")?;
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            if let Err(error) = handle_connection(stream, state).await {
                tracing::error!(?error, "clipboard daemon request failed");
            }
        });
    }
}

struct DaemonState {
    queue: ClipboardQueue,
    state_path: std::path::PathBuf,
}

impl DaemonState {
    fn persist(&self) -> Result<()> {
        self.queue.save(&self.state_path)
    }
}

async fn handle_connection(stream: UnixStream, state: Arc<Mutex<DaemonState>>) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .await
        .context("Failed to read request")?;

    let request: Request = serde_json::from_str(line.trim_end())
        .context("Failed to parse clipboard request")?;

    let response = handle_request(request, state).await;
    let payload = serde_json::to_vec(&response).context("Failed to encode clipboard response")?;
    writer.write_all(&payload).await.context("Failed to write response")?;
    writer.write_all(b"\n").await.context("Failed to terminate response")?;
    writer.flush().await.context("Failed to flush response")?;

    Ok(())
}

async fn handle_request(request: Request, state: Arc<Mutex<DaemonState>>) -> Response {
    let daemon_pid = process::id();

    match request {
        Request::Ping => {
            let state = state.lock().await;
            Response::ok(state.queue.len(), daemon_pid)
        }
        Request::Status => {
            let state = state.lock().await;
            Response::ok(state.queue.len(), daemon_pid).with_message("clipboard daemon ready")
        }
        Request::Push { content } => {
            let mut state = state.lock().await;
            state.queue.push(content);
            if let Err(error) = state.persist() {
                return Response::error(error.to_string(), state.queue.len(), daemon_pid);
            }

            Response::ok(state.queue.len(), daemon_pid).with_message("queued clipboard snippet")
        }
        Request::Next => {
            let mut state = state.lock().await;
            let Some(content) = state.queue.peek().cloned() else {
                return Response::ok(0, daemon_pid).with_message("clipboard queue is empty");
            };

            match copy_to_clipboard(&content) {
                Ok(()) => {
                    state.queue.pop();
                    if let Err(error) = state.persist() {
                        return Response::error(error.to_string(), state.queue.len(), daemon_pid);
                    }

                    Response::ok(state.queue.len(), daemon_pid)
                        .with_content(Some(content))
                        .with_message("copied next snippet to clipboard")
                }
                Err(error) => Response::error(error.to_string(), state.queue.len(), daemon_pid),
            }
        }
    }
}

fn copy_to_clipboard(content: &str) -> Result<()> {
    let mut clipboard = Clipboard::new().context("Failed to open system clipboard")?;
    clipboard
        .set_text(content.to_string())
        .context("Failed to update system clipboard")?;
    Ok(())
}
