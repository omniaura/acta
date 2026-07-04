use crate::config::{Config, PluginConfig};
use anyhow::Result;
use std::collections::HashMap;
use tracing::info;

pub async fn list() -> Result<()> {
    info!("Listing plugins");

    let config = Config::load()?;

    println!("Available Plugins:");
    println!("==================\n");

    if config.plugins.is_empty() {
        println!("No plugins registered");
        return Ok(());
    }

    for (name, plugin) in &config.plugins {
        println!("  {}", name);
        println!("    Command: {}", plugin.command);
        if !plugin.args.is_empty() {
            println!("    Args: {}", plugin.args.join(" "));
        }
        if !plugin.env.is_empty() {
            println!("    Env vars: {}", plugin.env.len());
        }
        println!();
    }

    Ok(())
}

pub async fn register(name: String, command: String) -> Result<()> {
    info!("Registering plugin '{}' with command '{}'", name, command);

    let mut config = Config::load()?;

    let plugin = PluginConfig {
        command: command.clone(),
        args: vec![],
        env: HashMap::new(),
    };

    config.register_plugin(name.clone(), plugin);
    config.save()?;

    println!("✅ Registered plugin '{}' -> '{}'", name, command);
    println!("   Edit ~/.config/acta/config.yaml to add args or env vars");

    Ok(())
}

pub async fn remove(name: String) -> Result<()> {
    info!("Removing plugin '{}'", name);

    let mut config = Config::load()?;

    if config.remove_plugin(&name).is_some() {
        config.save()?;
        println!("🗑️  Removed plugin '{}'", name);
    } else {
        println!("⚠️  Plugin '{}' not found", name);
    }

    Ok(())
}
