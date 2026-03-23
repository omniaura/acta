use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs;
use std::path::Path;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ClipboardQueue {
    items: VecDeque<String>,
}

impl ClipboardQueue {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let contents = fs::read_to_string(path)
            .with_context(|| format!("Failed to read clipboard state from {}", path.display()))?;

        serde_json::from_str(&contents)
            .with_context(|| format!("Failed to parse clipboard state from {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create {}", parent.display()))?;
        }

        let contents = serde_json::to_string_pretty(self)
            .context("Failed to serialize clipboard state")?;
        fs::write(path, contents)
            .with_context(|| format!("Failed to write clipboard state to {}", path.display()))?;
        Ok(())
    }

    pub fn push(&mut self, content: String) {
        self.items.push_back(content);
    }

    pub fn peek(&self) -> Option<&String> {
        self.items.front()
    }

    pub fn pop(&mut self) -> Option<String> {
        self.items.pop_front()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }
}

#[cfg(test)]
mod tests {
    use super::ClipboardQueue;
    use std::fs;
    use std::path::PathBuf;
    use uuid::Uuid;

    fn temp_path() -> PathBuf {
        std::env::temp_dir().join(format!("acta-clipboard-{}.json", Uuid::new_v4()))
    }

    #[test]
    fn preserves_fifo_order_across_persistence() {
        let path = temp_path();

        let mut queue = ClipboardQueue::default();
        queue.push("first".into());
        queue.push("second".into());
        queue.save(&path).unwrap();

        let mut restored = ClipboardQueue::load(&path).unwrap();
        assert_eq!(restored.pop().as_deref(), Some("first"));
        assert_eq!(restored.pop().as_deref(), Some("second"));

        fs::remove_file(path).unwrap();
    }
}
