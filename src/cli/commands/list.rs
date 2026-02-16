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
    println!("{:<8} {:<15} {:<10} {:<10}", "ID", "Agent", "Status", "Name");
    println!("{}", "-".repeat(60));

    for session in sessions {
        let short_id = &session.id[..8];
        let name = session.name.as_deref().unwrap_or("-");
        println!(
            "{:<8} {:<15} {:<10} {:<10}",
            short_id,
            session.agent,
            format!("{:?}", session.status),
            name
        );
    }

    println!("\n💡 Use 'acta attach <id>' to connect to a session");

    Ok(())
}
