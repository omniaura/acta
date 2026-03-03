use anyhow::Result;
use tracing::info;
use crate::session::{Session, SessionManager, SessionStatus};
use crate::{config::Config, git};
use std::time::SystemTime;

pub async fn execute(agent: String, name: Option<String>, args: Vec<String>) -> Result<()> {
    info!(
        "Creating new {} session{}",
        agent,
        name.as_ref()
            .map(|n| format!(" named '{}'", n))
            .unwrap_or_default()
    );

    let config = Config::load()?;
    if config.get_plugin(&agent).is_none() {
        anyhow::bail!(
            "Unknown agent '{}'. Register it with 'acta plugin register {} <command>'",
            agent,
            agent
        );
    }

    let mut manager = SessionManager::new()?;
    let session_id = SessionManager::next_session_id();
    let worktree = git::create_worktree(&session_id, &agent, name.as_deref())?;

    let session = Session {
        id: session_id,
        name,
        agent: agent.clone(),
        worktree_path: worktree.path,
        branch: worktree.branch,
        repo_root: worktree.repo_root,
        status: SessionStatus::Stopped,
        created_at: SystemTime::now(),
        args,
    };

    manager.register_session(session.clone())?;

    println!("✅ Created {} session", agent);
    println!("   ID: {}", session.id);
    if let Some(name) = &session.name {
        println!("   Name: {}", name);
    }
    println!("   Branch: {}", session.branch);
    println!("   Worktree: {}", session.worktree_path.display());
    println!("   Status: {:?}", session.status);

    println!("\n💡 Session created! Use 'acta attach {}' to start the agent", session.id);

    Ok(())
}
