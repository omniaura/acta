mod commands;

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};

/// Acta — a terminal multiplexer for agentic coding.
///
/// Detachable agent sessions (exit, SSH away, the agent keeps working),
/// a FIFO clipboard queue for agent-to-human handoff, and multi-repo
/// workspaces backed by plain clones instead of git worktrees.
#[derive(Parser, Debug)]
#[command(name = "acta")]
#[command(author, version, about)]
pub struct Cli {
    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Create a new detached agent session
    New {
        /// Agent to run: a configured plugin (claude, opencode, codex, …)
        /// or any raw command
        agent: String,

        /// Session name (defaults to <agent>-<id>)
        #[arg(short, long)]
        name: Option<String>,

        /// Working directory for the agent (defaults to the current dir)
        #[arg(long)]
        cwd: Option<String>,

        /// Run inside a workspace clone (looks up clones/<repo> via acta.yaml)
        #[arg(short, long)]
        repo: Option<String>,

        /// Attach to the session immediately after starting it
        #[arg(short, long)]
        attach: bool,

        /// Additional arguments passed to the agent
        #[arg(last = true)]
        args: Vec<String>,
    },

    /// List sessions
    #[command(alias = "ls")]
    List,

    /// Attach to a running session (detach with Ctrl-\)
    Attach {
        /// Session ID or name
        session: String,
    },

    /// How to detach (must be done from inside an attached session)
    Detach,

    /// Terminate a session's agent
    Kill {
        /// Session ID or name
        session: String,

        /// SIGKILL the agent and its daemon instead of SIGTERM
        #[arg(short, long)]
        force: bool,
    },

    /// Print a session's terminal output log
    Logs {
        /// Session ID or name
        session: String,

        /// Only print the last N lines
        #[arg(short = 'n', long)]
        tail: Option<usize>,
    },

    /// Remove records of exited/failed sessions
    Clean,

    /// Agent-to-human clipboard queue (alias: cb)
    #[command(visible_alias = "cb")]
    Clipboard {
        #[command(subcommand)]
        command: commands::clipboard::ClipboardCommands,
    },

    /// Multi-repo workspace: virtual worktrees via plain clones (alias: ws)
    #[command(visible_alias = "ws")]
    Workspace {
        #[command(subcommand)]
        command: commands::workspace::WorkspaceCommands,
    },

    /// Open the interactive session picker TUI
    Tui,

    /// Manage configuration
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },

    /// Manage agent plugins
    Plugin {
        #[command(subcommand)]
        command: PluginCommands,
    },

    /// Generate shell completions
    Completions {
        /// Shell to generate for
        shell: clap_complete::Shell,
    },

    /// Internal: session daemon entry point
    #[command(name = "__sessiond", hide = true)]
    Sessiond {
        /// Session id
        id: u32,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigCommands {
    /// List current configuration
    List,

    /// Get a configuration value
    Get {
        /// Configuration key
        key: String,
    },

    /// Set a configuration value
    Set {
        /// Configuration key
        key: String,
        /// Configuration value
        value: String,
    },

    /// Show configuration file path
    Path,
}

#[derive(Subcommand, Debug)]
enum PluginCommands {
    /// List available plugins
    List,

    /// Register a new plugin
    Register {
        /// Plugin name
        name: String,
        /// Command to execute
        command: String,
    },

    /// Remove a plugin
    Remove {
        /// Plugin name
        name: String,
    },
}

/// Attach to a session by id — used by the TUI after it restores the terminal.
pub async fn attach_session(id: u32) -> Result<()> {
    commands::attach::execute(id.to_string()).await
}

impl Cli {
    pub async fn execute(self) -> Result<()> {
        match self.command {
            Commands::New {
                agent,
                name,
                cwd,
                repo,
                attach,
                args,
            } => commands::new::execute(agent, name, cwd, repo, attach, args).await,
            Commands::List => commands::list::execute().await,
            Commands::Tui => crate::tui::run().await,
            Commands::Attach { session } => commands::attach::execute(session).await,
            Commands::Detach => commands::detach::execute().await,
            Commands::Kill { session, force } => commands::kill::execute(session, force).await,
            Commands::Logs { session, tail } => commands::logs::execute(session, tail).await,
            Commands::Clean => commands::clean::execute().await,
            Commands::Clipboard { command } => commands::clipboard::execute(command).await,
            Commands::Workspace { command } => commands::workspace::execute(command).await,
            Commands::Config { command } => match command {
                ConfigCommands::List => commands::config::list().await,
                ConfigCommands::Get { key } => commands::config::get(key).await,
                ConfigCommands::Set { key, value } => commands::config::set(key, value).await,
                ConfigCommands::Path => commands::config::path().await,
            },
            Commands::Plugin { command } => match command {
                PluginCommands::List => commands::plugin::list().await,
                PluginCommands::Register { name, command } => {
                    commands::plugin::register(name, command).await
                }
                PluginCommands::Remove { name } => commands::plugin::remove(name).await,
            },
            Commands::Completions { shell } => {
                let mut cmd = Cli::command();
                let name = cmd.get_name().to_string();
                clap_complete::generate(shell, &mut cmd, name, &mut std::io::stdout());
                Ok(())
            }
            Commands::Sessiond { id } => crate::session::daemon::run(id).await,
        }
    }
}
