//! Acta Workspaces: virtual worktrees for multi-repo work.
//!
//! A workspace is one orchestrator repo holding shared agent config and an
//! `acta.yaml` manifest. `acta ws sync` clones every listed repo into a
//! gitignored `clones/` directory — full independent clones, so you get the
//! parallel-worktree experience with none of the worktree locking/branch
//! headaches. Shared files (skills, CLAUDE.md, .claude/…) can be symlinked
//! into every clone, and `acta ws context` formats cross-repo state into
//! `<acta-context clone="…">` blocks ready for an LLM prompt.

use crate::git;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const MANIFEST_FILE: &str = "acta.yaml";
pub const DEFAULT_CLONES_DIR: &str = "clones";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    #[serde(default)]
    pub workspace: WorkspaceSection,
    #[serde(default)]
    pub repos: Vec<RepoSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSection {
    #[serde(default)]
    pub name: String,
    /// Directory (relative to the workspace root) that clones live in.
    #[serde(default = "default_clones_dir")]
    pub clones_dir: String,
    /// Paths in the workspace root symlinked into every clone — shared
    /// skills, CLAUDE.md, .claude/, agent config — without polluting the
    /// sibling repos' git history (clones_dir is gitignored).
    #[serde(default)]
    pub links: Vec<String>,
}

impl Default for WorkspaceSection {
    fn default() -> Self {
        Self {
            name: String::new(),
            clones_dir: default_clones_dir(),
            links: Vec::new(),
        }
    }
}

fn default_clones_dir() -> String {
    DEFAULT_CLONES_DIR.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoSpec {
    pub name: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Shell commands run inside the clone after it is first created
    /// (and on demand via `acta ws setup`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub setup: Vec<String>,
}

pub struct Workspace {
    pub root: PathBuf,
    pub manifest: Manifest,
}

impl Workspace {
    /// Walk up from `start` looking for an `acta.yaml`.
    pub fn discover(start: &Path) -> Result<Self> {
        let mut dir = Some(start.to_path_buf());
        while let Some(current) = dir {
            let manifest_path = current.join(MANIFEST_FILE);
            if manifest_path.is_file() {
                return Self::load(&current);
            }
            dir = current.parent().map(|p| p.to_path_buf());
        }
        bail!(
            "No {MANIFEST_FILE} found in {} or any parent — run `acta workspace init` first",
            start.display()
        );
    }

    pub fn discover_from_cwd() -> Result<Self> {
        Self::discover(&std::env::current_dir()?)
    }

    pub fn load(root: &Path) -> Result<Self> {
        let manifest_path = root.join(MANIFEST_FILE);
        let contents = fs::read_to_string(&manifest_path)
            .with_context(|| format!("Failed to read {}", manifest_path.display()))?;
        let manifest: Manifest = serde_yaml::from_str(&contents)
            .with_context(|| format!("Failed to parse {}", manifest_path.display()))?;
        Ok(Self {
            root: root.to_path_buf(),
            manifest,
        })
    }

    pub fn save(&self) -> Result<()> {
        let manifest_path = self.root.join(MANIFEST_FILE);
        fs::write(&manifest_path, serde_yaml::to_string(&self.manifest)?)
            .with_context(|| format!("Failed to write {}", manifest_path.display()))?;
        Ok(())
    }

    pub fn clones_dir(&self) -> PathBuf {
        self.root.join(&self.manifest.workspace.clones_dir)
    }

    pub fn clone_path(&self, repo: &RepoSpec) -> PathBuf {
        self.clones_dir().join(&repo.name)
    }

    pub fn repo(&self, name: &str) -> Result<&RepoSpec> {
        self.manifest
            .repos
            .iter()
            .find(|r| r.name == name)
            .with_context(|| format!("No repo named '{name}' in {MANIFEST_FILE}"))
    }

    /// Repos filtered by an optional name; errors if the name is unknown.
    pub fn select_repos(&self, only: Option<&str>) -> Result<Vec<&RepoSpec>> {
        match only {
            Some(name) => Ok(vec![self.repo(name)?]),
            None => Ok(self.manifest.repos.iter().collect()),
        }
    }

    /// Symlink each configured shared path into the clone (idempotent).
    pub fn apply_links(&self, repo: &RepoSpec) -> Result<Vec<String>> {
        let clone_path = self.clone_path(repo);
        let mut applied = Vec::new();
        for link in &self.manifest.workspace.links {
            let source = self.root.join(link);
            if !source.exists() {
                continue;
            }
            let target = clone_path.join(link);
            if target.exists() || target.symlink_metadata().is_ok() {
                continue; // don't clobber anything the clone already has
            }
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            std::os::unix::fs::symlink(&source, &target).with_context(|| {
                format!(
                    "Failed to link {} -> {}",
                    target.display(),
                    source.display()
                )
            })?;
            exclude_in_clone(&clone_path, link)?;
            applied.push(link.clone());
        }
        Ok(applied)
    }
}

/// Keep shared links out of the clone's `git status` via .git/info/exclude
/// (local-only, never committed to the cloned repo).
fn exclude_in_clone(clone_path: &Path, link: &str) -> Result<()> {
    let info_dir = clone_path.join(".git").join("info");
    if !clone_path.join(".git").exists() {
        return Ok(());
    }
    fs::create_dir_all(&info_dir)?;
    let exclude = info_dir.join("exclude");
    let entry = format!("/{}", link.trim_start_matches('/'));
    let existing = fs::read_to_string(&exclude).unwrap_or_default();
    if existing.lines().any(|l| l.trim() == entry) {
        return Ok(());
    }
    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(&entry);
    updated.push('\n');
    fs::write(&exclude, updated)?;
    Ok(())
}

/// Extract a repo name from a git URL (`git@host:org/name.git` or https).
pub fn repo_name_from_url(url: &str) -> Option<String> {
    let trimmed = url.trim_end_matches('/').trim_end_matches(".git");
    let last = trimmed.rsplit(['/', ':']).next()?;
    if last.is_empty() {
        None
    } else {
        Some(last.to_string())
    }
}

/// Create a workspace skeleton in `dir`.
pub fn init(dir: &Path, name: &str) -> Result<PathBuf> {
    let manifest_path = dir.join(MANIFEST_FILE);
    if manifest_path.exists() {
        bail!("{} already exists", manifest_path.display());
    }
    let template = format!(
        "# Acta workspace manifest — see `acta workspace --help`\n\
         workspace:\n\
         \x20 name: {name}\n\
         \x20 clones_dir: {DEFAULT_CLONES_DIR}\n\
         \x20 # Shared paths symlinked into every clone (skills, agent config):\n\
         \x20 links: []\n\
         #   - CLAUDE.md\n\
         #   - .claude\n\
         repos: []\n\
         # - name: my-repo\n\
         #   url: git@github.com:me/my-repo.git\n\
         #   branch: main\n\
         #   setup:\n\
         #     - npm install\n"
    );
    fs::write(&manifest_path, template)?;
    fs::create_dir_all(dir.join(DEFAULT_CLONES_DIR))?;
    ensure_gitignored(dir, DEFAULT_CLONES_DIR)?;
    Ok(manifest_path)
}

/// Make sure `clones/` never leaks into the orchestrator repo's history.
pub fn ensure_gitignored(dir: &Path, entry: &str) -> Result<bool> {
    let gitignore = dir.join(".gitignore");
    let entry_line = format!("{}/", entry.trim_end_matches('/'));
    let existing = fs::read_to_string(&gitignore).unwrap_or_default();
    let already = existing
        .lines()
        .map(str::trim)
        .any(|l| l == entry_line || l == entry);
    if already {
        return Ok(false);
    }
    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(&entry_line);
    updated.push('\n');
    fs::write(&gitignore, updated)?;
    Ok(true)
}

pub struct SyncResult {
    pub repo: String,
    pub action: SyncAction,
    pub linked: Vec<String>,
    pub error: Option<String>,
}

pub enum SyncAction {
    Cloned,
    Pulled,
    Skipped(String),
}

/// Clone missing repos / fast-forward existing ones, in parallel.
pub async fn sync(ws: &Workspace, only: Option<&str>, no_pull: bool) -> Result<Vec<SyncResult>> {
    let clones_dir = ws.clones_dir();
    fs::create_dir_all(&clones_dir)?;
    ensure_gitignored(&ws.root, &ws.manifest.workspace.clones_dir)?;

    let repos = ws.select_repos(only)?;
    let mut joinset = tokio::task::JoinSet::new();
    for repo in repos {
        let repo = repo.clone();
        let clones_dir = clones_dir.clone();
        joinset.spawn(async move {
            let clone_path = clones_dir.join(&repo.name);
            let fresh = !clone_path.join(".git").exists();
            let action = if fresh {
                match git::clone(&clones_dir, &repo.url, &repo.name, repo.branch.as_deref()).await {
                    Ok(()) => SyncAction::Cloned,
                    Err(e) => {
                        return (
                            repo,
                            SyncAction::Skipped("clone failed".into()),
                            Some(e.to_string()),
                        );
                    }
                }
            } else if no_pull {
                SyncAction::Skipped("exists".into())
            } else {
                match git::pull_ff_only(&clone_path).await {
                    Ok(_) => SyncAction::Pulled,
                    Err(e) => {
                        return (
                            repo,
                            SyncAction::Skipped("pull failed".into()),
                            Some(e.to_string()),
                        );
                    }
                }
            };
            (repo, action, None)
        });
    }

    let mut results = Vec::new();
    while let Some(joined) = joinset.join_next().await {
        let (repo, action, error) = joined?;
        // Fresh clones get setup commands + shared links.
        let mut linked = Vec::new();
        if error.is_none() {
            linked = ws.apply_links(&repo).unwrap_or_default();
        }
        let mut error = error;
        if matches!(action, SyncAction::Cloned) {
            if let Err(e) = run_setup(ws, &repo).await {
                error = Some(format!("setup failed: {e}"));
            }
        }
        results.push(SyncResult {
            repo: repo.name,
            action,
            linked,
            error,
        });
    }
    results.sort_by(|a, b| a.repo.cmp(&b.repo));
    Ok(results)
}

pub async fn run_setup(ws: &Workspace, repo: &RepoSpec) -> Result<()> {
    let clone_path = ws.clone_path(repo);
    for command in &repo.setup {
        let status = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(&clone_path)
            .status()
            .await?;
        if !status.success() {
            bail!(
                "'{command}' exited with {status} in {}",
                clone_path.display()
            );
        }
    }
    Ok(())
}

/// Build `<acta-context clone="…">` blocks for an LLM prompt.
pub async fn context(ws: &Workspace, only: Option<&str>, include_diff: bool) -> Result<String> {
    let mut out = String::new();
    for repo in ws.select_repos(only)? {
        let clone_path = ws.clone_path(repo);
        if !clone_path.join(".git").exists() {
            continue;
        }
        let status = git::short_status(&clone_path).await.unwrap_or_default();
        out.push_str(&format!("<acta-context clone=\"{}\">\n", repo.name));
        out.push_str(&format!("# git status --short --branch\n{status}"));
        if include_diff {
            let diff = git::diff(&clone_path).await.unwrap_or_default();
            if !diff.trim().is_empty() {
                out.push_str(&format!("\n# git diff (staged + unstaged)\n{diff}"));
            }
        }
        out.push_str("</acta-context>\n\n");
    }
    if out.is_empty() {
        bail!("No synced clones found — run `acta ws sync` first");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_names_from_urls() {
        assert_eq!(
            repo_name_from_url("git@github.com:omniaura/acta.git").as_deref(),
            Some("acta")
        );
        assert_eq!(
            repo_name_from_url("https://github.com/omniaura/acta").as_deref(),
            Some("acta")
        );
        assert_eq!(
            repo_name_from_url("https://github.com/omniaura/acta.git/").as_deref(),
            Some("acta")
        );
    }

    #[test]
    fn init_creates_manifest_and_gitignore() {
        let dir = tempfile::tempdir().unwrap();
        init(dir.path(), "test-ws").unwrap();
        let ws = Workspace::load(dir.path()).unwrap();
        assert_eq!(ws.manifest.workspace.name, "test-ws");
        assert!(dir.path().join(DEFAULT_CLONES_DIR).is_dir());
        let gitignore = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(gitignore.contains("clones/"));
        // idempotent gitignore
        assert!(!ensure_gitignored(dir.path(), DEFAULT_CLONES_DIR).unwrap());
    }
}
