use crate::session::SessionManager;
use anyhow::{Context, Result};
use std::io::Write;

pub async fn execute(session: String, tail: Option<usize>) -> Result<()> {
    let manager = SessionManager::new()?;
    let session = manager.resolve(&session)?;
    let log_path = manager.log_path(session.id);
    let contents = std::fs::read(&log_path)
        .with_context(|| format!("No output logged yet at {}", log_path.display()))?;

    let mut stdout = std::io::stdout().lock();
    match tail {
        None => stdout.write_all(&contents)?,
        Some(n) => {
            let text = String::from_utf8_lossy(&contents);
            let lines: Vec<&str> = text.lines().collect();
            let start = lines.len().saturating_sub(n);
            for line in &lines[start..] {
                writeln!(stdout, "{line}")?;
            }
        }
    }
    Ok(())
}
