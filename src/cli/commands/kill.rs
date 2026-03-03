use anyhow::{Context, Result};
use tracing::info;

use crate::git;
use crate::session::SessionManager;

pub async fn execute(session: String, clean: bool) -> Result<()> {
    info!("Killing session: {} (clean: {})", session, clean);

    let mut manager = SessionManager::new()?;

    let sess = manager
        .get_session(&session)
        .context(format!("Session '{}' not found", session))?
        .clone();

    // Kill the daemon process if alive
    if let Some(pid) = sess.pid {
        if sess.is_alive() {
            eprintln!("Stopping {} session '{}'...", sess.agent, sess.id);
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }

            // Wait briefly for graceful shutdown
            for _ in 0..20 {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                if !sess.is_alive() {
                    break;
                }
            }

            // Force kill if still alive
            if sess.is_alive() {
                unsafe {
                    libc::kill(pid as i32, libc::SIGKILL);
                }
            }
        }
    }

    // Clean up worktree if requested
    if clean {
        eprintln!("Removing worktree...");
        git::remove_worktree(&sess.repo_path, &sess.worktree_path, &sess.id)?;
        eprintln!("  Removed: {}", sess.worktree_path.display());
    }

    // Remove session metadata
    manager.remove_session(&sess.id)?;

    eprintln!("Session '{}' terminated", sess.id);

    if !clean && sess.worktree_path.exists() {
        eprintln!(
            "\nWorktree preserved at: {}",
            sess.worktree_path.display()
        );
        eprintln!("  Use 'acta kill {} --clean' to also remove the worktree", sess.id);
    }

    Ok(())
}
