use anyhow::Result;

pub async fn execute() -> Result<()> {
    if let Ok(session) = std::env::var("ACTA_SESSION") {
        println!("You are inside acta session {session}.");
    }
    println!("Detach from an attached session with Ctrl-\\ — the agent keeps running.");
    println!("Re-attach any time with `acta attach <id>`, even from a new SSH login.");
    Ok(())
}
