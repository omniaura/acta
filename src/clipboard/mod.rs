pub mod daemon;
pub mod protocol;
pub mod state;

use crate::clipboard::protocol::{Request, Response};
use anyhow::{anyhow, Context, Result};
use std::env;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::process::Command;
use tokio::time::{sleep, Duration};

#[derive(Debug, Clone)]
pub struct ClipboardPaths {
    pub root_dir: PathBuf,
    pub socket_path: PathBuf,
    pub state_path: PathBuf,
}

impl ClipboardPaths {
    pub fn new() -> Result<Self> {
        let root_dir = env::var_os("ACTA_HOME")
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|path| path.join(".acta")))
            .context("Could not determine Acta home directory")?;

        Ok(Self {
            socket_path: root_dir.join("clipboard.sock"),
            state_path: root_dir.join("clipboard-queue.json"),
            root_dir,
        })
    }

    pub fn ensure_parent_dirs(&self) -> Result<()> {
        std::fs::create_dir_all(&self.root_dir)
            .with_context(|| format!("Failed to create {}", self.root_dir.display()))
    }
}

pub struct ClipboardClient {
    paths: ClipboardPaths,
}

impl ClipboardClient {
    pub fn new() -> Result<Self> {
        Ok(Self {
            paths: ClipboardPaths::new()?,
        })
    }

    pub async fn request(&self, request: Request) -> Result<Response> {
        self.ensure_daemon_running().await?;
        let mut stream = UnixStream::connect(&self.paths.socket_path)
            .await
            .with_context(|| format!("Failed to connect to {}", self.paths.socket_path.display()))?;

        let payload = serde_json::to_vec(&request).context("Failed to encode clipboard request")?;
        stream.write_all(&payload).await.context("Failed to send request")?;
        stream.write_all(b"\n").await.context("Failed to terminate request")?;
        stream.flush().await.context("Failed to flush request")?;

        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .await
            .context("Failed to read clipboard response")?;

        let response: Response = serde_json::from_str(line.trim_end())
            .context("Failed to parse clipboard response")?;

        if response.ok {
            Ok(response)
        } else {
            Err(anyhow!(response.message.unwrap_or_else(|| "clipboard request failed".into())))
        }
    }

    async fn ensure_daemon_running(&self) -> Result<()> {
        if self.is_daemon_ready().await {
            return Ok(());
        }

        if self.paths.socket_path.exists() {
            let _ = std::fs::remove_file(&self.paths.socket_path);
        }

        self.paths.ensure_parent_dirs()?;
        let current_exe = env::current_exe().context("Failed to resolve current acta binary")?;
        Command::new(current_exe)
            .arg("clipboard")
            .arg("daemon")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("Failed to start clipboard daemon")?;

        for _ in 0..40 {
            if self.is_daemon_ready().await {
                return Ok(());
            }
            sleep(Duration::from_millis(25)).await;
        }

        Err(anyhow!("clipboard daemon did not become ready"))
    }

    async fn is_daemon_ready(&self) -> bool {
        let Ok(mut stream) = UnixStream::connect(&self.paths.socket_path).await else {
            return false;
        };

        let Ok(payload) = serde_json::to_vec(&Request::Ping) else {
            return false;
        };

        if stream.write_all(&payload).await.is_err() || stream.write_all(b"\n").await.is_err() {
            return false;
        }

        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        if reader.read_line(&mut line).await.is_err() {
            return false;
        }

        serde_json::from_str::<Response>(line.trim_end())
            .map(|response| response.ok)
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::ClipboardPaths;

    #[test]
    fn honors_acta_home_override() {
        let original = std::env::var_os("ACTA_HOME");
        std::env::set_var("ACTA_HOME", "/tmp/acta-test-home");

        let paths = ClipboardPaths::new().unwrap();
        assert_eq!(paths.root_dir, std::path::PathBuf::from("/tmp/acta-test-home"));
        assert_eq!(paths.socket_path, std::path::PathBuf::from("/tmp/acta-test-home/clipboard.sock"));

        match original {
            Some(value) => std::env::set_var("ACTA_HOME", value),
            None => std::env::remove_var("ACTA_HOME"),
        }
    }
}
