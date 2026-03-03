use anyhow::Result;
use tracing::info;

pub async fn execute() -> Result<()> {
    info!("Detaching from current session");

    println!("📤 Detach is handled by your terminal multiplexer/session manager.");
    println!("   If you launched with 'acta attach', press Ctrl+C to return to Acta.");

    Ok(())
}
