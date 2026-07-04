use crate::session::{SessionManager, SessionStatus};
use anyhow::Result;

pub async fn execute() -> Result<()> {
    let manager = SessionManager::new()?;
    let mut removed = 0;
    for session in manager.list()? {
        if matches!(
            session.status,
            SessionStatus::Exited(_) | SessionStatus::Failed(_)
        ) {
            manager.remove(session.id)?;
            println!(
                "🧹 Removed session {} ({}) — {}",
                session.id,
                session.name,
                session.status.label()
            );
            removed += 1;
        }
    }
    if removed == 0 {
        println!("Nothing to clean");
    }
    Ok(())
}
