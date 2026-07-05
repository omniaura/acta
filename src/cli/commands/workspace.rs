use crate::clipboard::{copy_to_system_clipboard, ClipboardStore, CopyOutcome};
use crate::git;
use crate::workspace::{self, repo_name_from_url, RepoSpec, SyncAction, Workspace};
use anyhow::{bail, Context, Result};
use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum WorkspaceCommands {
    /// Create an acta.yaml manifest + gitignored clones/ dir here
    Init {
        /// Workspace name (defaults to the directory name)
        name: Option<String>,
    },

    /// Add a repo to the manifest
    Add {
        /// Git URL (ssh or https)
        url: String,

        /// Clone directory name (defaults to the repo name from the URL)
        #[arg(short, long)]
        name: Option<String>,

        /// Branch to clone
        #[arg(short, long)]
        branch: Option<String>,

        /// Clone it immediately
        #[arg(short, long)]
        sync: bool,
    },

    /// Clone missing repos and fast-forward existing ones (in parallel)
    Sync {
        /// Only this repo
        #[arg(short, long)]
        repo: Option<String>,

        /// Clone missing repos but don't pull existing ones
        #[arg(long)]
        no_pull: bool,
    },

    /// Show branch/dirty/ahead-behind for every clone
    Status {
        /// Only this repo
        #[arg(short, long)]
        repo: Option<String>,
    },

    /// Run a shell command in every clone
    Run {
        /// Only this repo
        #[arg(short, long)]
        repo: Option<String>,

        /// The command (joined and passed to `sh -c`)
        #[arg(required = true, last = true)]
        command: Vec<String>,
    },

    /// Re-run the setup commands from the manifest
    Setup {
        /// Only this repo
        #[arg(short, long)]
        repo: Option<String>,
    },

    /// Emit <acta-context clone="…"> blocks for an LLM prompt
    Context {
        /// Only this repo
        #[arg(short, long)]
        repo: Option<String>,

        /// Include staged+unstaged diffs
        #[arg(short, long)]
        diff: bool,

        /// Push the context onto the acta clipboard queue instead of printing
        #[arg(long)]
        clip: bool,
    },
}

pub async fn execute(command: WorkspaceCommands) -> Result<()> {
    match command {
        WorkspaceCommands::Init { name } => {
            let dir = std::env::current_dir()?;
            let name = name.unwrap_or_else(|| {
                dir.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "workspace".into())
            });
            let manifest = workspace::init(&dir, &name)?;
            println!("✅ Workspace '{name}' initialized");
            println!("   Manifest: {}", manifest.display());
            println!("   clones/ created and gitignored");
            println!("\n💡 `acta ws add <git-url>` then `acta ws sync`");
        }
        WorkspaceCommands::Add {
            url,
            name,
            branch,
            sync,
        } => {
            let mut ws = Workspace::discover_from_cwd()?;
            let name = match name.or_else(|| repo_name_from_url(&url)) {
                Some(name) => name,
                None => bail!("Could not derive a repo name from '{url}' — pass --name"),
            };
            if ws.manifest.repos.iter().any(|r| r.name == name) {
                bail!("Repo '{name}' is already in the manifest");
            }
            ws.manifest.repos.push(RepoSpec {
                name: name.clone(),
                url,
                branch,
                setup: Vec::new(),
            });
            ws.save()?;
            println!("✅ Added '{name}' to acta.yaml");
            if sync {
                run_sync(&ws, Some(&name), false).await?;
            } else {
                println!("💡 `acta ws sync` to clone it");
            }
        }
        WorkspaceCommands::Sync { repo, no_pull } => {
            let ws = Workspace::discover_from_cwd()?;
            if ws.manifest.repos.is_empty() {
                bail!("No repos in acta.yaml yet — `acta ws add <git-url>` first");
            }
            run_sync(&ws, repo.as_deref(), no_pull).await?;
        }
        WorkspaceCommands::Status { repo } => {
            let ws = Workspace::discover_from_cwd()?;
            println!(
                "Workspace: {} ({})",
                ws.manifest.workspace.name,
                ws.root.display()
            );
            println!(
                "{:<20} {:<24} {:<8} {:<12} SYNCED",
                "REPO", "BRANCH", "DIRTY", "AHEAD/BEHIND"
            );
            for spec in ws.select_repos(repo.as_deref())? {
                let path = ws.clone_path(spec);
                if !path.join(".git").exists() {
                    println!(
                        "{:<20} {:<24} {:<8} {:<12} ✗ (run `acta ws sync`)",
                        spec.name, "-", "-", "-"
                    );
                    continue;
                }
                let branch = git::current_branch(&path)
                    .await
                    .unwrap_or_else(|_| "?".into());
                let dirty = git::dirty_count(&path).await.unwrap_or(0);
                let ab = match git::ahead_behind(&path).await.unwrap_or(None) {
                    Some((ahead, behind)) => format!("+{ahead}/-{behind}"),
                    None => "no upstream".into(),
                };
                println!("{:<20} {:<24} {:<8} {:<12} ✓", spec.name, branch, dirty, ab);
            }
        }
        WorkspaceCommands::Run { repo, command } => {
            let ws = Workspace::discover_from_cwd()?;
            let cmdline = command.join(" ");
            let mut failures = Vec::new();
            for spec in ws.select_repos(repo.as_deref())? {
                let path = ws.clone_path(spec);
                if !path.join(".git").exists() {
                    println!("── {} ── skipped (not synced)", spec.name);
                    continue;
                }
                println!("── {} ── $ {cmdline}", spec.name);
                let status = tokio::process::Command::new("sh")
                    .arg("-c")
                    .arg(&cmdline)
                    .current_dir(&path)
                    .status()
                    .await
                    .context("Failed to run shell")?;
                if !status.success() {
                    failures.push(spec.name.clone());
                }
            }
            if !failures.is_empty() {
                bail!("Command failed in: {}", failures.join(", "));
            }
        }
        WorkspaceCommands::Setup { repo } => {
            let ws = Workspace::discover_from_cwd()?;
            for spec in ws.select_repos(repo.as_deref())? {
                if !ws.clone_path(spec).join(".git").exists() {
                    println!("── {} ── skipped (not synced)", spec.name);
                    continue;
                }
                if spec.setup.is_empty() {
                    println!("── {} ── no setup commands", spec.name);
                    continue;
                }
                println!(
                    "── {} ── running {} setup command(s)",
                    spec.name,
                    spec.setup.len()
                );
                workspace::run_setup(&ws, spec).await?;
            }
            println!("✅ Setup complete");
        }
        WorkspaceCommands::Context { repo, diff, clip } => {
            let ws = Workspace::discover_from_cwd()?;
            let context = workspace::context(&ws, repo.as_deref(), diff).await?;
            if clip {
                let store = ClipboardStore::new()?;
                let item = store.push(
                    context.clone(),
                    Some(format!("workspace context: {}", ws.manifest.workspace.name)),
                    false,
                )?;
                // Also try to land it on the system clipboard right away.
                match copy_to_system_clipboard(&context) {
                    CopyOutcome::Unavailable => {
                        println!("📋 Context queued as clipboard item #{}", item.id)
                    }
                    _ => println!(
                        "📋 Context copied to clipboard and queued as item #{}",
                        item.id
                    ),
                }
            } else {
                print!("{context}");
            }
        }
    }
    Ok(())
}

async fn run_sync(ws: &Workspace, only: Option<&str>, no_pull: bool) -> Result<()> {
    println!("Syncing into {} …", ws.clones_dir().display());
    let results = workspace::sync(ws, only, no_pull).await?;
    let mut had_error = false;
    for result in &results {
        let action = match &result.action {
            SyncAction::Cloned => "cloned",
            SyncAction::Pulled => "pulled",
            SyncAction::Skipped(reason) => reason.as_str(),
        };
        match &result.error {
            Some(error) => {
                had_error = true;
                println!("  ✗ {:<20} {action}: {error}", result.repo);
            }
            None => {
                let links = if result.linked.is_empty() {
                    String::new()
                } else {
                    format!(" (linked: {})", result.linked.join(", "))
                };
                println!("  ✓ {:<20} {action}{links}", result.repo);
            }
        }
    }
    if had_error {
        bail!("Some repos failed to sync");
    }
    Ok(())
}
