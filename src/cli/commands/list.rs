use crate::session::SessionManager;
use anyhow::Result;
use std::time::{SystemTime, UNIX_EPOCH};

pub async fn execute() -> Result<()> {
    let manager = SessionManager::new()?;
    let sessions = manager.list()?;

    if sessions.is_empty() {
        println!("No sessions");
        println!("\n💡 Create one with: acta new <agent>");
        return Ok(());
    }

    if let Ok(env_mode) = std::env::var("ACTA_ENV") {
        println!("🔒 ACTA_ENV={env_mode}\n");
    }

    println!(
        "{:<4} {:<20} {:<12} {:<12} {:<8} {:<10} CWD",
        "ID", "NAME", "AGENT", "STATUS", "PID", "CREATED"
    );
    for session in &sessions {
        println!(
            "{:<4} {:<20} {:<12} {:<12} {:<8} {:<10} {}",
            session.id,
            truncate(&session.name, 20),
            truncate(&session.agent, 12),
            session.status.label(),
            session
                .child_pid
                .map(|p| p.to_string())
                .unwrap_or_else(|| "-".into()),
            age(session.created_at),
            session.cwd.display(),
        );
    }

    println!("\n💡 attach <id> · logs <id> · kill <id> · clean");
    Ok(())
}

fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        value.to_string()
    } else {
        let cut: String = value.chars().take(max.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}

fn age(created_at: u64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let delta = now.saturating_sub(created_at);
    match delta {
        0..=59 => format!("{delta}s ago"),
        60..=3599 => format!("{}m ago", delta / 60),
        3600..=86399 => format!("{}h ago", delta / 3600),
        _ => format!("{}d ago", delta / 86400),
    }
}
