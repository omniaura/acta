use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    pub path: PathBuf,
    pub branch: String,
    pub repo_root: PathBuf,
}

pub fn create_worktree(session_id: &str, agent: &str, name: Option<&str>) -> Result<WorktreeInfo> {
    let repo_root = repo_root()?;
    let short_id = &session_id[..8.min(session_id.len())];
    let branch = build_branch_name(short_id, agent, name);
    let worktree_path = repo_root.join(".acta").join("worktrees").join(short_id);

    if let Some(parent) = worktree_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }

    let output = Command::new("git")
        .arg("worktree")
        .arg("add")
        .arg("-b")
        .arg(&branch)
        .arg(&worktree_path)
        .arg("HEAD")
        .current_dir(&repo_root)
        .output()
        .context("Failed to execute git worktree add")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to create worktree: {}", stderr.trim());
    }

    Ok(WorktreeInfo {
        path: worktree_path,
        branch,
        repo_root,
    })
}

pub fn remove_worktree(repo_root: &Path, worktree_path: &Path, branch: &str, force: bool) -> Result<()> {
    let mut worktree_remove = Command::new("git");
    worktree_remove
        .arg("worktree")
        .arg("remove");

    if force {
        worktree_remove.arg("--force");
    }

    let output = worktree_remove
        .arg(worktree_path)
        .current_dir(repo_root)
        .output()
        .context("Failed to execute git worktree remove")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("is not a working tree") && !stderr.contains("No such file") {
            bail!("Failed to remove worktree: {}", stderr.trim());
        }
    }

    let branch_output = Command::new("git")
        .arg("branch")
        .arg("-D")
        .arg(branch)
        .current_dir(repo_root)
        .output()
        .context("Failed to execute git branch -D")?;

    if !branch_output.status.success() {
        let stderr = String::from_utf8_lossy(&branch_output.stderr);
        if !stderr.contains("not found") && !stderr.contains("not fully merged") {
            bail!("Failed to remove branch '{}': {}", branch, stderr.trim());
        }
    }

    Ok(())
}

fn repo_root() -> Result<PathBuf> {
    let output = Command::new("git")
        .arg("rev-parse")
        .arg("--show-toplevel")
        .output()
        .context("Failed to execute git rev-parse --show-toplevel")?;

    if !output.status.success() {
        bail!("acta must run inside a git repository");
    }

    let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(PathBuf::from(root))
}

fn build_branch_name(short_id: &str, agent: &str, name: Option<&str>) -> String {
    let sanitized_agent = sanitize_for_branch(agent);
    let base = name
        .map(sanitize_for_branch)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "session".to_string());

    format!("acta/{}/{}/{}", sanitized_agent, base, short_id)
}

fn sanitize_for_branch(input: &str) -> String {
    input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}
