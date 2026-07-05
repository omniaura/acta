//! Session metadata and lifecycle management.
//!
//! Each session is backed by a detached daemon process (`acta __sessiond`)
//! that owns the agent's PTY and serves attach clients over a unix socket.
//! Metadata lives in `~/.acta/sessions/<id>.json`; the socket and raw output
//! log sit next to it. Because the daemon runs in its own session (setsid),
//! agents keep working after you detach, close your terminal, or drop SSH.

pub mod attach;
pub mod daemon;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SessionStatus {
    Starting,
    Running,
    Exited(i32),
    Failed(String),
}

impl SessionStatus {
    pub fn label(&self) -> String {
        match self {
            SessionStatus::Starting => "starting".into(),
            SessionStatus::Running => "running".into(),
            SessionStatus::Exited(code) => format!("exited({code})"),
            SessionStatus::Failed(_) => "failed".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: u32,
    pub name: String,
    pub agent: String,
    /// Resolved program the daemon executes.
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub cwd: PathBuf,
    pub status: SessionStatus,
    pub created_at: u64,
    #[serde(default)]
    pub daemon_pid: Option<u32>,
    #[serde(default)]
    pub child_pid: Option<u32>,
}

impl Session {
    pub fn created_at_now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

pub struct SessionManager {
    state_dir: PathBuf,
}

impl SessionManager {
    pub fn new() -> Result<Self> {
        let state_dir = state_dir()?;
        fs::create_dir_all(&state_dir).context("Failed to create session state directory")?;
        Ok(Self { state_dir })
    }

    pub fn meta_path(&self, id: u32) -> PathBuf {
        self.state_dir.join(format!("{id}.json"))
    }

    pub fn sock_path(&self, id: u32) -> PathBuf {
        self.state_dir.join(format!("{id}.sock"))
    }

    pub fn log_path(&self, id: u32) -> PathBuf {
        self.state_dir.join(format!("{id}.log"))
    }

    pub fn daemon_log_path(&self, id: u32) -> PathBuf {
        self.state_dir.join(format!("{id}.daemon.log"))
    }

    /// Allocate the next free session id by exclusively creating its
    /// metadata file — atomic even with concurrent `acta new` invocations.
    pub fn allocate(&self, mut session: Session) -> Result<Session> {
        let existing_max = self.list()?.iter().map(|s| s.id).max().unwrap_or(0);
        let mut id = existing_max + 1;
        loop {
            session.id = id;
            let path = self.meta_path(id);
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(_) => break,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    id += 1;
                    continue;
                }
                Err(e) => return Err(e).context("Failed to create session metadata"),
            }
        }
        if session.name.is_empty() {
            session.name = format!("{}-{}", session.agent, session.id);
        }
        self.save(&session)?;
        Ok(session)
    }

    pub fn save(&self, session: &Session) -> Result<()> {
        let path = self.meta_path(session.id);
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(session)?)?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }

    pub fn load(&self, id: u32) -> Result<Session> {
        let contents = fs::read_to_string(self.meta_path(id))
            .with_context(|| format!("Session {id} not found"))?;
        Ok(serde_json::from_str(&contents)?)
    }

    /// List sessions, reconciling stale "running" states whose daemon died.
    pub fn list(&self) -> Result<Vec<Session>> {
        let mut sessions = Vec::new();
        if !self.state_dir.exists() {
            return Ok(sessions);
        }
        for entry in fs::read_dir(&self.state_dir)? {
            let path = entry?.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let Ok(contents) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(mut session) = serde_json::from_str::<Session>(&contents) else {
                continue;
            };
            if self.reconcile(&mut session) {
                let _ = self.save(&session);
            }
            sessions.push(session);
        }
        sessions.sort_by_key(|s| s.id);
        Ok(sessions)
    }

    /// Returns true if the status was corrected.
    fn reconcile(&self, session: &mut Session) -> bool {
        if !matches!(
            session.status,
            SessionStatus::Running | SessionStatus::Starting
        ) {
            return false;
        }
        if let Some(pid) = session.daemon_pid {
            if pid_alive(pid) {
                return false;
            }
        }
        session.status = SessionStatus::Failed("daemon died".into());
        true
    }

    /// Resolve a session by numeric id or by name.
    pub fn resolve(&self, id_or_name: &str) -> Result<Session> {
        if let Ok(id) = id_or_name.parse::<u32>() {
            if let Ok(session) = self.load(id) {
                return Ok(session);
            }
        }
        let sessions = self.list()?;
        sessions
            .into_iter()
            .find(|s| s.name == id_or_name)
            .with_context(|| format!("No session matching '{id_or_name}'"))
    }

    /// Remove every on-disk trace of a session.
    pub fn remove(&self, id: u32) -> Result<()> {
        for path in [
            self.meta_path(id),
            self.sock_path(id),
            self.log_path(id),
            self.daemon_log_path(id),
        ] {
            if path.exists() {
                fs::remove_file(&path)
                    .with_context(|| format!("Failed to remove {}", path.display()))?;
            }
        }
        Ok(())
    }

    pub fn kill(&self, session: &Session, force: bool) -> Result<()> {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;

        if !matches!(
            session.status,
            SessionStatus::Running | SessionStatus::Starting
        ) {
            bail!("Session {} is not running", session.id);
        }
        let signal = if force {
            Signal::SIGKILL
        } else {
            Signal::SIGTERM
        };
        let mut signalled = false;
        if let Some(pid) = session.child_pid {
            if pid_alive(pid) {
                kill(Pid::from_raw(pid as i32), signal).ok();
                signalled = true;
            }
        }
        if force {
            if let Some(pid) = session.daemon_pid {
                if pid_alive(pid) {
                    kill(Pid::from_raw(pid as i32), Signal::SIGKILL).ok();
                    signalled = true;
                }
            }
        }
        if !signalled {
            bail!("Session {} has no live process to signal", session.id);
        }
        Ok(())
    }
}

pub fn state_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    Ok(home.join(".acta").join("sessions"))
}

pub fn pid_alive(pid: u32) -> bool {
    use nix::sys::signal::kill;
    use nix::unistd::Pid;
    kill(Pid::from_raw(pid as i32), None).is_ok()
}
