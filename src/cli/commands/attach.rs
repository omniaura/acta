use anyhow::{Context, Result};
use tracing::info;

use crate::session::SessionManager;

pub async fn execute(session: String) -> Result<()> {
    info!("Attaching to session: {}", session);

    let manager = SessionManager::new()?;
    let sess = manager
        .get_session(&session)
        .context(format!("Session '{}' not found", session))?;

    let effective = sess.effective_status();
    if effective != crate::session::SessionStatus::Running {
        anyhow::bail!(
            "Session '{}' is not running (status: {})",
            session,
            effective
        );
    }

    let socket_path = sess.socket_path();
    if !socket_path.exists() {
        anyhow::bail!(
            "Session '{}' daemon is not running (no socket found). \
             The agent may have exited.",
            session
        );
    }

    eprintln!("Attaching to '{}' ({})... (Ctrl+B d to detach)\n", sess.id, sess.agent);
    crate::client::attach(&socket_path).await?;

    eprintln!("[detached from session '{}']", sess.id);

    Ok(())
}
