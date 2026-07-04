use crate::session::attach::{self, Outcome};
use crate::session::{SessionManager, SessionStatus};
use anyhow::{bail, Result};

pub async fn execute(session: String) -> Result<()> {
    let manager = SessionManager::new()?;
    let session = manager.resolve(&session)?;

    match &session.status {
        SessionStatus::Running | SessionStatus::Starting => {}
        SessionStatus::Exited(code) => {
            bail!(
                "Session {} already exited with code {code} — see `acta logs {}`",
                session.id,
                session.id
            )
        }
        SessionStatus::Failed(reason) => bail!("Session {} failed: {reason}", session.id),
    }

    let sock_path = manager.sock_path(session.id);
    match attach::run(&session, &sock_path).await? {
        Outcome::Detached => {
            println!(
                "[acta] detached from session {} — it keeps running",
                session.id
            );
        }
        Outcome::Exited(code) => {
            println!("[acta] session {} exited with code {code}", session.id);
        }
        Outcome::ConnectionClosed => {
            println!("[acta] connection to session {} closed", session.id);
        }
    }
    Ok(())
}
