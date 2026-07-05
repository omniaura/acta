//! Thin wrappers around the system `git` binary.
//!
//! Acta shells out to `git` instead of linking libgit2 so it inherits the
//! user's auth setup (SSH agent, credential helpers) with zero native deps.

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

pub async fn run(dir: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .await
        .context("Failed to execute git — is it installed?")?;
    if !output.status.success() {
        bail!(
            "git {} failed in {}:\n{}",
            args.join(" "),
            dir.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub async fn clone(parent: &Path, url: &str, name: &str, branch: Option<&str>) -> Result<()> {
    let mut args = vec!["clone"];
    if let Some(branch) = branch {
        args.extend(["--branch", branch]);
    }
    args.extend([url, name]);
    run(parent, &args).await?;
    Ok(())
}

pub async fn pull_ff_only(dir: &Path) -> Result<String> {
    run(dir, &["pull", "--ff-only"]).await
}

pub async fn current_branch(dir: &Path) -> Result<String> {
    Ok(run(dir, &["rev-parse", "--abbrev-ref", "HEAD"])
        .await?
        .trim()
        .to_string())
}

/// Number of modified/untracked paths in the working tree.
pub async fn dirty_count(dir: &Path) -> Result<usize> {
    let out = run(dir, &["status", "--porcelain"]).await?;
    Ok(out.lines().filter(|l| !l.trim().is_empty()).count())
}

/// (ahead, behind) relative to the upstream branch, if one is configured.
pub async fn ahead_behind(dir: &Path) -> Result<Option<(u32, u32)>> {
    let out = match run(
        dir,
        &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"],
    )
    .await
    {
        Ok(out) => out,
        Err(_) => return Ok(None), // no upstream configured
    };
    let mut parts = out.split_whitespace();
    let ahead = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let behind = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    Ok(Some((ahead, behind)))
}

pub async fn diff(dir: &Path) -> Result<String> {
    // Include staged changes too — agents often stage as they go.
    let staged = run(dir, &["diff", "--cached"]).await?;
    let unstaged = run(dir, &["diff"]).await?;
    Ok(format!("{staged}{unstaged}"))
}

pub async fn short_status(dir: &Path) -> Result<String> {
    run(dir, &["status", "--short", "--branch"]).await
}
