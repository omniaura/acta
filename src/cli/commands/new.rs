use crate::config::{expand_env_value, Config};
use crate::session::{Session, SessionManager, SessionStatus};
use crate::workspace::Workspace;
use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

pub async fn execute(
    agent: String,
    name: Option<String>,
    cwd: Option<String>,
    repo: Option<String>,
    attach: bool,
    extra_args: Vec<String>,
) -> Result<()> {
    let config = Config::load()?;

    // A configured plugin wins; otherwise treat the agent as a raw command
    // so `acta new bash` or `acta new ./my-agent.sh` just works.
    let (command, mut args, env) = match config.get_plugin(&agent) {
        Some(plugin) => {
            let mut env = HashMap::new();
            for (key, value) in &plugin.env {
                if let Some(expanded) = expand_env_value(value) {
                    env.insert(key.clone(), expanded);
                }
            }
            (plugin.command.clone(), plugin.args.clone(), env)
        }
        None => (agent.clone(), Vec::new(), HashMap::new()),
    };
    args.extend(extra_args);

    let cwd = resolve_cwd(cwd, repo)?;
    if !cwd.is_dir() {
        bail!("Working directory {} does not exist", cwd.display());
    }

    let manager = SessionManager::new()?;
    let session = manager.allocate(Session {
        id: 0,
        name: name.unwrap_or_default(),
        agent: agent.clone(),
        command,
        args,
        env,
        cwd,
        status: SessionStatus::Starting,
        created_at: Session::created_at_now(),
        daemon_pid: None,
        child_pid: None,
    })?;

    spawn_daemon(&manager, &session)?;
    let session = wait_until_running(&manager, session.id).await?;

    println!("✅ Session {} ({}) started", session.id, session.name);
    println!(
        "   Agent: {} — pid {}",
        session.agent,
        session.child_pid.unwrap_or(0)
    );
    println!("   Cwd:   {}", session.cwd.display());
    if let Ok(env_mode) = std::env::var("ACTA_ENV") {
        println!("   Env:   ACTA_ENV={env_mode}");
    }

    if attach {
        super::attach::execute(session.id.to_string()).await
    } else {
        println!(
            "\n💡 `acta attach {}` to connect — it keeps running when you detach or log out",
            session.id
        );
        Ok(())
    }
}

fn resolve_cwd(cwd: Option<String>, repo: Option<String>) -> Result<PathBuf> {
    if let Some(repo) = repo {
        let ws = Workspace::discover_from_cwd()?;
        let spec = ws.repo(&repo)?;
        let path = ws.clone_path(spec);
        if !path.join(".git").exists() {
            bail!(
                "Clone for '{repo}' not found at {} — run `acta ws sync` first",
                path.display()
            );
        }
        return Ok(path);
    }
    match cwd {
        Some(dir) => Ok(PathBuf::from(expand_home(&dir))),
        None => Ok(std::env::current_dir()?),
    }
}

fn expand_home(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).to_string_lossy().into_owned();
        }
    }
    path.to_string()
}

fn spawn_daemon(manager: &SessionManager, session: &Session) -> Result<()> {
    let exe = std::env::current_exe().context("Could not locate the acta binary")?;
    let daemon_log = std::fs::File::create(manager.daemon_log_path(session.id))?;
    std::process::Command::new(exe)
        .arg("__sessiond")
        .arg(session.id.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::from(daemon_log.try_clone()?))
        .stderr(Stdio::from(daemon_log))
        .spawn()
        .context("Failed to spawn session daemon")?;
    Ok(())
}

async fn wait_until_running(manager: &SessionManager, id: u32) -> Result<Session> {
    for _ in 0..30 {
        let session = manager.load(id)?;
        match &session.status {
            SessionStatus::Running => return Ok(session),
            SessionStatus::Exited(code) => {
                bail!("Agent exited immediately with code {code} — check `acta logs {id}`")
            }
            SessionStatus::Failed(reason) => {
                let daemon_log =
                    std::fs::read_to_string(manager.daemon_log_path(id)).unwrap_or_default();
                manager.remove(id).ok();
                bail!("Session failed to start: {reason}\n{daemon_log}");
            }
            SessionStatus::Starting => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    }
    let daemon_log = std::fs::read_to_string(manager.daemon_log_path(id)).unwrap_or_default();
    manager.remove(id).ok();
    bail!("Session daemon did not come up within 3s\n{daemon_log}");
}
