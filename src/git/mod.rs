use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Find the root of the current git repository.
pub fn find_repo_root() -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("Failed to run git")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Not a git repository: {}", stderr.trim());
    }

    let path = String::from_utf8(output.stdout)
        .context("Invalid UTF-8 in git output")?
        .trim()
        .to_string();

    Ok(PathBuf::from(path))
}

/// Create a new git worktree with a new branch.
pub fn create_worktree(repo_root: &Path, session_id: &str) -> Result<PathBuf> {
    let worktree_path = repo_root.join(".acta").join("worktrees").join(session_id);
    let branch_name = format!("acta/{}", session_id);

    std::fs::create_dir_all(worktree_path.parent().unwrap())
        .context("Failed to create .acta/worktrees directory")?;

    ensure_gitignore(repo_root)?;

    let output = Command::new("git")
        .current_dir(repo_root)
        .args([
            "worktree",
            "add",
            "-b",
            &branch_name,
            worktree_path.to_str().unwrap(),
        ])
        .output()
        .context("Failed to run git worktree add")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git worktree add failed: {}", stderr.trim());
    }

    Ok(worktree_path)
}

/// Remove a git worktree and its branch.
pub fn remove_worktree(repo_root: &Path, worktree_path: &Path, session_id: &str) -> Result<()> {
    let output = Command::new("git")
        .current_dir(repo_root)
        .args([
            "worktree",
            "remove",
            "--force",
            worktree_path.to_str().unwrap(),
        ])
        .output()
        .context("Failed to run git worktree remove")?;

    if !output.status.success() {
        if worktree_path.exists() {
            std::fs::remove_dir_all(worktree_path).ok();
        }
        Command::new("git")
            .current_dir(repo_root)
            .args(["worktree", "prune"])
            .output()
            .ok();
    }

    let branch_name = format!("acta/{}", session_id);
    Command::new("git")
        .current_dir(repo_root)
        .args(["branch", "-D", &branch_name])
        .output()
        .ok();

    Ok(())
}

fn ensure_gitignore(repo_root: &Path) -> Result<()> {
    let gitignore_path = repo_root.join(".gitignore");

    if gitignore_path.exists() {
        let contents = std::fs::read_to_string(&gitignore_path)?;
        if contents
            .lines()
            .any(|line| line.trim() == ".acta/" || line.trim() == ".acta")
        {
            return Ok(());
        }
        let mut contents = contents;
        if !contents.ends_with('\n') {
            contents.push('\n');
        }
        contents.push_str(".acta/\n");
        std::fs::write(&gitignore_path, contents)?;
    } else {
        std::fs::write(&gitignore_path, ".acta/\n")?;
    }

    Ok(())
}
