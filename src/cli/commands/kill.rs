use anyhow::Result;
use tracing::info;
use crate::session::SessionManager;
use crate::git;

pub async fn execute(session: String, force: bool) -> Result<()> {
    info!("Killing session: {} (force: {})", session, force);

    let mut manager = SessionManager::new()?;

    let session_info = manager
        .find_session(&session)
        .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session))?;

    let id = session_info.id.clone();
    let agent = session_info.agent.clone();

    println!("💀 Killing {} session '{}'...", agent, session);
    if force {
        println!("⚠️  Force kill enabled - forcing worktree removal");
    }

    if !session_info.repo_root.as_os_str().is_empty() && !session_info.branch.is_empty() {
        git::remove_worktree(
            &session_info.repo_root,
            &session_info.worktree_path,
            &session_info.branch,
            force,
        )?;
    }
    manager.kill_session(&id)?;

    println!("✅ Session terminated");

    Ok(())
}
