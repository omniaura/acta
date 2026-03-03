use anyhow::Result;
use tracing::info;
use crate::session::SessionManager;

pub async fn execute() -> Result<()> {
    info!("Listing active sessions");

    let manager = SessionManager::new()?;
    let sessions = manager.list_sessions();

    if sessions.is_empty() {
        println!("No active sessions");
        println!("\n💡 Create a new session with: acta new <agent>");
        return Ok(());
    }

    println!("Active Sessions:");
    println!("================\n");
    println!("{:<8} {:<10} {:<10} {:<18} {:<16}", "ID", "Agent", "Status", "Name", "Branch");
    println!("{}", "-".repeat(90));

    for session in sessions {
        let short_id = &session.id[..8];
        let name = session.name.as_deref().unwrap_or("-");
        let branch = if session.branch.is_empty() {
            "-"
        } else {
            &session.branch
        };
        println!(
            "{:<8} {:<10} {:<10} {:<18} {:<16}",
            short_id,
            session.agent,
            format!("{:?}", session.status),
            name,
            branch
        );
        println!("{:<8} worktree: {}", "", session.worktree_path.display());
    }

    println!("\n💡 Use 'acta attach <id>' to connect to a session");

    Ok(())
}
