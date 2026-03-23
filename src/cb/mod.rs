use crate::clipboard::protocol::Request;
use crate::clipboard::ClipboardClient;
use anyhow::{Context, Result};
use std::io::IsTerminal;
use tokio::io::AsyncReadExt;

pub async fn push(text: Option<String>, stdin: bool) -> Result<()> {
    let content = resolve_push_content(text, stdin).await?;
    let client = ClipboardClient::new()?;
    let response = client.request(Request::Push { content }).await?;
    println!("Queued snippet. Pending items: {}", response.queue_len);
    Ok(())
}

pub async fn next() -> Result<()> {
    let client = ClipboardClient::new()?;
    let response = client.request(Request::Next).await?;

    if let Some(content) = response.content {
        println!("Copied next snippet to clipboard.");
        println!("Remaining items: {}", response.queue_len);
        println!("---");
        print!("{}", content);
        if !content.ends_with('\n') {
            println!();
        }
    } else {
        println!("Clipboard queue is empty.");
    }

    Ok(())
}

pub async fn status() -> Result<()> {
    let client = ClipboardClient::new()?;
    let response = client.request(Request::Status).await?;
    println!("Clipboard daemon PID: {}", response.daemon_pid);
    println!("Queued snippets: {}", response.queue_len);
    Ok(())
}

pub async fn daemon() -> Result<()> {
    let paths = crate::clipboard::ClipboardPaths::new()?;
    crate::clipboard::daemon::run(paths).await
}

async fn resolve_push_content(text: Option<String>, stdin: bool) -> Result<String> {
    match text {
        Some(text) if !stdin => Ok(text),
        Some(_) => Err(anyhow::anyhow!("Choose either positional text or --stdin, not both")),
        None => {
            if !stdin && std::io::stdin().is_terminal() {
                return Err(anyhow::anyhow!(
                    "No snippet provided. Pass text directly or pipe raw content with --stdin"
                ));
            }

            let mut buffer = String::new();
            tokio::io::stdin()
                .read_to_string(&mut buffer)
                .await
                .context("Failed to read snippet from stdin")?;
            Ok(buffer)
        }
    }
}
