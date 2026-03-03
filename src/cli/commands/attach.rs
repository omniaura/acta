use anyhow::{Context, Result};
use tracing::info;
use std::process::Command;
use crate::config::Config;
use crate::session::{SessionManager, SessionStatus};

pub async fn execute(session: String) -> Result<()> {
    info!("Attaching to session: {}", session);

    let mut manager = SessionManager::new()?;
    let session_info = manager
        .find_session(&session)
        .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session))?;

    let config = Config::load()?;
    let plugin = config
        .get_plugin(&session_info.agent)
        .with_context(|| format!("No plugin registered for agent '{}'", session_info.agent))?;

    let mut cmd = Command::new(&plugin.command);
    cmd.args(&plugin.args)
        .args(&session_info.args)
        .current_dir(&session_info.worktree_path);

    for (key, value) in &plugin.env {
        cmd.env(key, expand_env(value));
    }

    manager.update_status(&session_info.id, SessionStatus::Running)?;

    println!("🔗 Starting '{}' in {}", session_info.agent, session_info.worktree_path.display());
    println!("   Command: {} {}", plugin.command, session_info.args.join(" "));

    let status = match cmd.status() {
        Ok(status) => status,
        Err(err) => {
            manager.update_status(&session_info.id, SessionStatus::Failed)?;
            return Err(err).with_context(|| {
                format!(
                    "Failed to start '{}' (is it installed and in PATH?)",
                    plugin.command
                )
            });
        }
    };

    if status.success() {
        manager.update_status(&session_info.id, SessionStatus::Stopped)?;
        println!("✅ Session command exited cleanly");
    } else {
        manager.update_status(&session_info.id, SessionStatus::Failed)?;
        anyhow::bail!("Session exited with status {}", status);
    }

    Ok(())
}

fn expand_env(value: &str) -> String {
    if value.starts_with("${") && value.ends_with('}') {
        let key = &value[2..value.len() - 1];
        return std::env::var(key).unwrap_or_default();
    }

    value.to_string()
}
