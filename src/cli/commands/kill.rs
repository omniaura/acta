use anyhow::Result;
use tracing::info;
use crate::session::SessionManager;

pub async fn execute(session: String, force: bool) -> Result<()> {
    info!("Killing session: {} (force: {})", session, force);

    let mut manager = SessionManager::new()?;

    // Check if session exists
    let session_info = manager
        .get_session(&session)
        .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session))?;

    let id = session_info.id.clone();
    let agent = session_info.agent.clone();

    println!("💀 Killing {} session '{}'...", agent, session);
    if force {
        println!("⚠️  Force kill enabled - skipping cleanup");
    }

    manager.kill_session(&id)?;

    println!("✅ Session terminated");

    Ok(())
}
