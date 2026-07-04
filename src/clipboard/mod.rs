//! Acta Clipboard: a stateful FIFO queue for agent-to-human handoff.
//!
//! Agents `acta cb push` snippets; the human runs `acta cb next` to pop each
//! one straight onto the system clipboard — paste, `next`, paste, `next` —
//! eliminating multiline copy/paste friction ("heredoc hell"). Queue state
//! persists in `~/.acta/clipboard.json`, guarded by an advisory lock so many
//! concurrent agents can push safely.

use anyhow::{Context, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipItem {
    pub id: u64,
    pub content: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub sensitive: bool,
    pub created_at: u64,
    /// Session name or "human" — who pushed this item.
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Queue {
    #[serde(default)]
    pub next_id: u64,
    #[serde(default)]
    pub items: Vec<ClipItem>,
}

pub struct ClipboardStore {
    path: PathBuf,
    lock_path: PathBuf,
}

impl ClipboardStore {
    pub fn new() -> Result<Self> {
        let dir = dirs::home_dir()
            .context("Could not determine home directory")?
            .join(".acta");
        fs::create_dir_all(&dir)?;
        Ok(Self {
            path: dir.join("clipboard.json"),
            lock_path: dir.join("clipboard.lock"),
        })
    }

    fn lock(&self) -> Result<nix::fcntl::Flock<File>> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&self.lock_path)?;
        nix::fcntl::Flock::lock(file, nix::fcntl::FlockArg::LockExclusive)
            .map_err(|(_, errno)| anyhow::anyhow!("Failed to lock clipboard state: {errno}"))
    }

    fn load(&self) -> Result<Queue> {
        if !self.path.exists() {
            return Ok(Queue::default());
        }
        let contents = fs::read_to_string(&self.path)?;
        Ok(serde_json::from_str(&contents).unwrap_or_default())
    }

    fn save(&self, queue: &Queue) -> Result<()> {
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(queue)?)?;
        fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    pub fn push(
        &self,
        content: String,
        description: Option<String>,
        sensitive: bool,
    ) -> Result<ClipItem> {
        let _lock = self.lock()?;
        let mut queue = self.load()?;
        queue.next_id += 1;
        let item = ClipItem {
            id: queue.next_id,
            content,
            description,
            sensitive,
            created_at: now(),
            source: std::env::var("ACTA_SESSION_NAME").ok(),
        };
        queue.items.push(item.clone());
        self.save(&queue)?;
        Ok(item)
    }

    /// Pop the head of the queue. Returns the popped item and the new head.
    pub fn next(&self) -> Result<(Option<ClipItem>, Option<ClipItem>)> {
        let _lock = self.lock()?;
        let mut queue = self.load()?;
        if queue.items.is_empty() {
            return Ok((None, None));
        }
        let item = queue.items.remove(0);
        let upcoming = queue.items.first().cloned();
        self.save(&queue)?;
        Ok((Some(item), upcoming))
    }

    pub fn peek(&self) -> Result<Option<ClipItem>> {
        let _lock = self.lock()?;
        Ok(self.load()?.items.first().cloned())
    }

    pub fn list(&self) -> Result<Vec<ClipItem>> {
        let _lock = self.lock()?;
        Ok(self.load()?.items)
    }

    /// Drop the head without copying it anywhere.
    pub fn skip(&self) -> Result<Option<ClipItem>> {
        let _lock = self.lock()?;
        let mut queue = self.load()?;
        if queue.items.is_empty() {
            return Ok(None);
        }
        let item = queue.items.remove(0);
        self.save(&queue)?;
        Ok(Some(item))
    }

    /// Empty the queue. Returns how many items were dropped.
    pub fn clear(&self) -> Result<usize> {
        let _lock = self.lock()?;
        let mut queue = self.load()?;
        let count = queue.items.len();
        queue.items.clear();
        self.save(&queue)?;
        Ok(count)
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Where a snippet ended up when we "copied" it.
pub enum CopyOutcome {
    /// A native clipboard tool accepted the content.
    Native(&'static str),
    /// Emitted an OSC 52 escape to the terminal (works over SSH in
    /// supporting terminals: iTerm2, WezTerm, kitty, Ghostty, tmux…).
    Osc52,
    /// No clipboard path available; caller should print the content.
    Unavailable,
}

/// Copy `content` to the system clipboard, trying native tools first and
/// falling back to OSC 52 so `acta cb next` works over plain SSH.
pub fn copy_to_system_clipboard(content: &str) -> CopyOutcome {
    let candidates: &[(&str, &[&str])] = &[
        ("pbcopy", &[]),
        ("wl-copy", &[]),
        ("xclip", &["-selection", "clipboard"]),
        ("xsel", &["--input", "--clipboard"]),
    ];
    for (tool, args) in candidates {
        if which(tool) {
            if let Ok(mut child) = Command::new(tool)
                .args(*args)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                let wrote = child
                    .stdin
                    .take()
                    .map(|mut stdin| stdin.write_all(content.as_bytes()).is_ok())
                    .unwrap_or(false);
                if wrote && matches!(child.wait().map(|s| s.success()), Ok(true)) {
                    return CopyOutcome::Native(tool);
                }
            }
        }
    }

    // OSC 52: ESC ] 52 ; c ; <base64> BEL — written to the tty so it reaches
    // the terminal emulator even when stdout is redirected.
    if let Ok(mut tty) = OpenOptions::new().write(true).open("/dev/tty") {
        let encoded = base64::engine::general_purpose::STANDARD.encode(content.as_bytes());
        let seq = format!("\x1b]52;c;{encoded}\x07");
        if tty.write_all(seq.as_bytes()).is_ok() && tty.flush().is_ok() {
            return CopyOutcome::Osc52;
        }
    }
    CopyOutcome::Unavailable
}

/// Overwrite the system clipboard (used by `clear --sensitive`).
pub fn wipe_system_clipboard() -> CopyOutcome {
    copy_to_system_clipboard("")
}

fn which(tool: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                let candidate = dir.join(tool);
                candidate.is_file()
            })
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> (tempfile::TempDir, ClipboardStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = ClipboardStore {
            path: dir.path().join("clipboard.json"),
            lock_path: dir.path().join("clipboard.lock"),
        };
        (dir, store)
    }

    #[test]
    fn fifo_order() {
        let (_dir, store) = test_store();
        store.push("one".into(), None, false).unwrap();
        store
            .push("two".into(), Some("second".into()), false)
            .unwrap();
        store.push("three".into(), None, true).unwrap();

        assert_eq!(store.list().unwrap().len(), 3);
        let (popped, upcoming) = store.next().unwrap();
        assert_eq!(popped.unwrap().content, "one");
        assert_eq!(upcoming.unwrap().content, "two");

        let skipped = store.skip().unwrap().unwrap();
        assert_eq!(skipped.content, "two");

        assert_eq!(store.peek().unwrap().unwrap().content, "three");
        assert_eq!(store.clear().unwrap(), 1);
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn empty_next() {
        let (_dir, store) = test_store();
        let (popped, upcoming) = store.next().unwrap();
        assert!(popped.is_none());
        assert!(upcoming.is_none());
    }

    #[test]
    fn ids_are_monotonic_across_pops() {
        let (_dir, store) = test_store();
        store.push("a".into(), None, false).unwrap();
        store.next().unwrap();
        let item = store.push("b".into(), None, false).unwrap();
        assert_eq!(item.id, 2);
    }
}
