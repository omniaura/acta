mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};

/// Acta - A terminal multiplexer for agentic coding
#[derive(Parser, Debug)]
#[command(name = "acta")]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Create a new agent session
    New {
        /// Agent type (claude, opencode, cursor)
        agent: String,

        /// Optional session name
        #[arg(short, long)]
        name: Option<String>,

        /// Additional arguments to pass to the agent
        #[arg(last = true)]
        args: Vec<String>,
    },

    /// List active sessions
    #[command(alias = "ls")]
    List,

    /// Attach to a session
    Attach {
        /// Session ID or name
        session: String,
    },

    /// Detach from current session
    Detach,

    /// Kill a session
    Kill {
        /// Session ID or name
        session: String,

        /// Force kill without cleanup
        #[arg(short, long)]
        force: bool,
    },

    /// Manage configuration
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },

    /// Manage plugins
    Plugin {
        #[command(subcommand)]
        command: PluginCommands,
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

impl Cli {
    pub async fn execute(self) -> Result<()> {
        match self.command {
            Commands::New { agent, name, args } => {
                commands::new::execute(agent, name, args).await
            }
            Commands::List => commands::list::execute().await,
            Commands::Attach { session } => commands::attach::execute(session).await,
            Commands::Detach => commands::detach::execute().await,
            Commands::Kill { session, force } => commands::kill::execute(session, force).await,
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
        }
    }
}
