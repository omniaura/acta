use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub name: Option<String>,
    pub agent: String,
    pub worktree_path: PathBuf,
    pub repo_path: PathBuf,
    pub status: SessionStatus,
    pub pid: Option<u32>,
    pub created_at: u64,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SessionStatus {
    Starting,
    Running,
    Stopped,
    Failed,
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionStatus::Starting => write!(f, "Starting"),
            SessionStatus::Running => write!(f, "Running"),
            SessionStatus::Stopped => write!(f, "Stopped"),
            SessionStatus::Failed => write!(f, "Failed"),
        }
    }
}

impl Session {
    pub fn is_alive(&self) -> bool {
        if let Some(pid) = self.pid {
            unsafe { libc::kill(pid as i32, 0) == 0 }
        } else {
            false
        }
    }

    pub fn socket_path(&self) -> PathBuf {
        SessionManager::get_state_dir()
            .unwrap()
            .join(format!("{}.sock", self.id))
    }

    pub fn log_path(&self) -> PathBuf {
        SessionManager::get_state_dir()
            .unwrap()
            .join(format!("{}.log", self.id))
    }

    pub fn effective_status(&self) -> SessionStatus {
        match self.status {
            SessionStatus::Running | SessionStatus::Starting => {
                if self.is_alive() {
                    self.status.clone()
                } else {
                    SessionStatus::Stopped
                }
            }
            _ => self.status.clone(),
        }
    }
}

pub struct SessionManager {
    state_dir: PathBuf,
    sessions: HashMap<String, Session>,
}

impl SessionManager {
    pub fn new() -> Result<Self> {
        let state_dir = Self::get_state_dir()?;
        fs::create_dir_all(&state_dir).context("Failed to create state directory")?;

        let sessions = Self::load_sessions(&state_dir)?;

        Ok(Self {
            state_dir,
            sessions,
        })
    }

    pub fn get_state_dir() -> Result<PathBuf> {
        let home = dirs::home_dir().context("Could not determine home directory")?;
        Ok(home.join(".acta").join("sessions"))
    }

    fn load_sessions(state_dir: &Path) -> Result<HashMap<String, Session>> {
        let mut sessions = HashMap::new();

        if !state_dir.exists() {
            return Ok(sessions);
        }

        for entry in fs::read_dir(state_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(session) = Self::load_session(&path) {
                    sessions.insert(session.id.clone(), session);
                }
            }
        }

        Ok(sessions)
    }

    fn load_session(path: &Path) -> Result<Session> {
        let contents = fs::read_to_string(path)?;
        let session: Session = serde_json::from_str(&contents)?;
        Ok(session)
    }

    fn save_session(&self, session: &Session) -> Result<()> {
        let path = self.state_dir.join(format!("{}.json", session.id));
        let contents = serde_json::to_string_pretty(session)?;
        fs::write(path, contents)?;
        Ok(())
    }

    pub fn create_session(
        &mut self,
        id: String,
        agent: String,
        name: Option<String>,
        worktree_path: PathBuf,
        repo_path: PathBuf,
        args: Vec<String>,
    ) -> Result<Session> {
        if self.sessions.contains_key(&id) {
            anyhow::bail!("Session '{}' already exists", id);
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let session = Session {
            id: id.clone(),
            name,
            agent,
            worktree_path,
            repo_path,
            status: SessionStatus::Starting,
            pid: None,
            created_at: now,
            args,
        };

        self.sessions.insert(id.clone(), session.clone());
        self.save_session(&session)?;

        Ok(session)
    }

    pub fn get_session(&self, id_or_name: &str) -> Option<&Session> {
        if let Some(session) = self.sessions.get(id_or_name) {
            return Some(session);
        }

        self.sessions
            .values()
            .find(|s| s.name.as_ref().map(|n| n == id_or_name).unwrap_or(false))
    }

    pub fn list_sessions(&self) -> Vec<&Session> {
        let mut sessions: Vec<&Session> = self.sessions.values().collect();
        sessions.sort_by_key(|s| s.created_at);
        sessions
    }

    pub fn remove_session(&mut self, id: &str) -> Result<()> {
        self.sessions.remove(id);

        let json_path = self.state_dir.join(format!("{}.json", id));
        if json_path.exists() {
            fs::remove_file(json_path)?;
        }

        let sock_path = self.state_dir.join(format!("{}.sock", id));
        if sock_path.exists() {
            fs::remove_file(sock_path)?;
        }

        let log_path = self.state_dir.join(format!("{}.log", id));
        if log_path.exists() {
            fs::remove_file(log_path)?;
        }

        Ok(())
    }

    pub fn update_session(&mut self, session: Session) -> Result<()> {
        self.sessions.insert(session.id.clone(), session.clone());
        self.save_session(&session)?;
        Ok(())
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new().expect("Failed to create SessionManager")
    }
}

pub fn generate_session_id(agent: &str) -> String {
    let short = &uuid::Uuid::new_v4().to_string()[..6];
    format!("{}-{}", agent, short)
}
