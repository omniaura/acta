use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub name: Option<String>,
    pub agent: String,
    pub worktree_path: PathBuf,
    pub status: SessionStatus,
    pub created_at: SystemTime,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SessionStatus {
    Running,
    Stopped,
    Failed,
}

pub struct SessionManager {
    state_dir: PathBuf,
    sessions: HashMap<String, Session>,
}

impl SessionManager {
    pub fn new() -> Result<Self> {
        let state_dir = Self::get_state_dir()?;
        fs::create_dir_all(&state_dir)
            .context("Failed to create state directory")?;

        let sessions = Self::load_sessions(&state_dir)?;

        Ok(Self {
            state_dir,
            sessions,
        })
    }

    fn get_state_dir() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .context("Could not determine home directory")?;
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
        let session: Session = serde_yaml::from_str(&contents)?;
        Ok(session)
    }

    fn save_session(&self, session: &Session) -> Result<()> {
        let path = self.state_dir.join(format!("{}.json", session.id));
        let contents = serde_yaml::to_string(session)?;
        fs::write(path, contents)?;
        Ok(())
    }

    pub fn create_session(
        &mut self,
        agent: String,
        name: Option<String>,
        args: Vec<String>,
    ) -> Result<Session> {
        let id = Uuid::new_v4().to_string();
        let worktree_path = PathBuf::from(format!(".acta/sessions/{}", id));

        let session = Session {
            id: id.clone(),
            name,
            agent,
            worktree_path,
            status: SessionStatus::Running,
            created_at: SystemTime::now(),
            args,
        };

        self.sessions.insert(id.clone(), session.clone());
        self.save_session(&session)?;

        Ok(session)
    }

    pub fn get_session(&self, id_or_name: &str) -> Option<&Session> {
        // Try by ID first
        if let Some(session) = self.sessions.get(id_or_name) {
            return Some(session);
        }

        // Try by name
        self.sessions.values().find(|s| {
            s.name.as_ref().map(|n| n == id_or_name).unwrap_or(false)
        })
    }

    pub fn list_sessions(&self) -> Vec<&Session> {
        let mut sessions: Vec<&Session> = self.sessions.values().collect();
        sessions.sort_by_key(|s| s.created_at);
        sessions
    }

    pub fn kill_session(&mut self, id_or_name: &str) -> Result<()> {
        let session = self
            .get_session(id_or_name)
            .context("Session not found")?;
        let id = session.id.clone();

        self.sessions.remove(&id);

        let path = self.state_dir.join(format!("{}.json", id));
        if path.exists() {
            fs::remove_file(path)?;
        }

        Ok(())
    }

    pub fn update_status(&mut self, id: &str, status: SessionStatus) -> Result<()> {
        let session = self
            .sessions
            .get_mut(id)
            .context("Session not found")?;

        session.status = status.clone();

        // Clone the session to avoid borrow checker issues
        let session_clone = session.clone();
        self.save_session(&session_clone)?;

        Ok(())
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new().expect("Failed to create SessionManager")
    }
}
