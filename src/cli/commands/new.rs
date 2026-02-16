use anyhow::Result;
use tracing::info;
use crate::session::SessionManager;

pub async fn execute(agent: String, name: Option<String>, args: Vec<String>) -> Result<()> {
    info!(
        "Creating new {} session{}",
        agent,
        name.as_ref()
            .map(|n| format!(" named '{}'", n))
            .unwrap_or_default()
    );

    let mut manager = SessionManager::new()?;
    let session = manager.create_session(agent.clone(), name.clone(), args)?;

    println!("✅ Created {} session", agent);
    println!("   ID: {}", session.id);
    if let Some(name) = &session.name {
        println!("   Name: {}", name);
    }
    println!("   Worktree: {}", session.worktree_path.display());
    println!("   Status: {:?}", session.status);

    println!("\n💡 Session created! Use 'acta attach {}' to connect", session.id);

    Ok(())
}
