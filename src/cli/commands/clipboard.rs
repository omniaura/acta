use anyhow::Result;

pub async fn push(text: Option<String>, stdin: bool) -> Result<()> {
    crate::cb::push(text, stdin).await
}

pub async fn next() -> Result<()> {
    crate::cb::next().await
}

pub async fn status() -> Result<()> {
    crate::cb::status().await
}

pub async fn daemon() -> Result<()> {
    crate::cb::daemon().await
}
