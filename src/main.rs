mod cli;
mod clipboard;
mod config;
mod git;
mod protocol;
mod session;
mod tui;
mod workspace;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing/logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "acta=info".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    // Parse CLI arguments
    let cli = cli::Cli::parse();

    // Execute command
    cli.execute().await
}
