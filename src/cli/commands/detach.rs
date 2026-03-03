use anyhow::Result;

pub async fn execute() -> Result<()> {
    println!("Detach with Ctrl+B then d while attached to a session.");
    println!("\nThis command is only meaningful inside an attached session.");
    Ok(())
}
