use anyhow::Result;
use tracing::info;

use crate::session::SessionManager;

pub async fn execute() -> Result<()> {
    info!("Listing active sessions");

    let manager = SessionManager::new()?;
    let sessions = manager.list_sessions();

    if sessions.is_empty() {
        println!("No sessions");
        println!("\n  Create one with: acta new <agent>");
        return Ok(());
    }

    println!(
        "{:<16} {:<12} {:<10} {:<10} {}",
        "ID", "Agent", "Status", "PID", "Worktree"
    );
    println!("{}", "\u{2500}".repeat(70));

    for session in &sessions {
        let status = session.effective_status();
        let pid_str = session
            .pid
            .filter(|_| session.is_alive())
            .map(|p| p.to_string())
            .unwrap_or_else(|| "-".to_string());

        let worktree = session
            .worktree_path
            .strip_prefix(&session.repo_path)
            .unwrap_or(&session.worktree_path)
            .display()
            .to_string();

        println!(
            "{:<16} {:<12} {:<10} {:<10} {}",
            session.id, session.agent, status, pid_str, worktree
        );
    }

    let running = sessions.iter().filter(|s| s.is_alive()).count();

    println!(
        "\n  {} session(s), {} running",
        sessions.len(),
        running
    );
    println!("  Attach: acta attach <id>");

    Ok(())
}
