use anyhow::Result;
use std::process::Stdio;
use tracing::info;

use crate::git;
use crate::session::{generate_session_id, SessionManager};

pub async fn execute(
    agent: String,
    name: Option<String>,
    detach: bool,
    args: Vec<String>,
) -> Result<()> {
    info!("Creating new {} session", agent);

    // Find git repo root
    let repo_root = git::find_repo_root()?;

    // Generate session ID
    let session_id = name.clone().unwrap_or_else(|| generate_session_id(&agent));

    // Create git worktree
    eprintln!("Creating worktree for session '{}'...", session_id);
    let worktree_path = git::create_worktree(&repo_root, &session_id)?;
    eprintln!(
        "  Worktree: {}",
        worktree_path
            .strip_prefix(&repo_root)
            .unwrap_or(&worktree_path)
            .display()
    );

    // Create session metadata
    let mut manager = SessionManager::new()?;
    let session = manager.create_session(
        session_id.clone(),
        agent.clone(),
        name,
        worktree_path.clone(),
        repo_root.clone(),
        args,
    )?;

    // Get path to our own binary for spawning the daemon
    let exe = std::env::current_exe()?;
    let log_path = session.log_path();

    // Spawn daemon process
    let log_file = std::fs::File::create(&log_path)?;
    let log_err = log_file.try_clone()?;

    let _daemon = std::process::Command::new(&exe)
        .args(["daemon", "--session-id", &session_id])
        .stdin(Stdio::null())
        .stdout(log_file)
        .stderr(log_err)
        .spawn()?;

    eprintln!("  Agent: {}", agent);
    eprintln!("  Session: {}", session_id);

    if detach {
        let socket_path = session.socket_path();
        match crate::client::wait_for_socket(&socket_path, 5).await {
            Ok(_) => eprintln!("\n  Session started in background"),
            Err(_) => {
                eprintln!("\n  Session started (daemon may still be initializing)");
                eprintln!("  Check logs: {}", log_path.display());
            }
        }
        eprintln!("  Attach with: acta attach {}", session_id);
        return Ok(());
    }

    // Wait for daemon socket to appear
    let socket_path = session.socket_path();
    eprintln!("\nStarting agent...");
    crate::client::wait_for_socket(&socket_path, 10)
        .await
        .map_err(|e| {
            eprintln!("Daemon failed to start. Check logs: {}", log_path.display());
            e
        })?;

    // Attach to session
    eprintln!("Attached. (Ctrl+B d to detach)\n");
    crate::client::attach(&socket_path).await?;

    eprintln!("[detached from session '{}']", session_id);

    Ok(())
}
