use crate::session::{pid_alive, SessionManager, SessionStatus};
use anyhow::Result;
use std::time::Duration;

pub async fn execute(session: String, force: bool) -> Result<()> {
    let manager = SessionManager::new()?;
    let session = manager.resolve(&session)?;

    if !matches!(
        session.status,
        SessionStatus::Running | SessionStatus::Starting
    ) {
        // Already dead — just clean up the record.
        manager.remove(session.id)?;
        println!(
            "🧹 Removed finished session {} ({})",
            session.id, session.name
        );
        return Ok(());
    }

    manager.kill(&session, force)?;

    // Give the daemon a moment to reap the agent and record the exit.
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let alive = session.child_pid.map(pid_alive).unwrap_or(false);
        if !alive {
            break;
        }
    }
    manager.remove(session.id)?;
    println!("💀 Killed session {} ({})", session.id, session.name);
    Ok(())
}
