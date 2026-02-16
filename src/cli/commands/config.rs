use anyhow::Result;
use tracing::info;
use crate::config::Config;

pub async fn list() -> Result<()> {
    info!("Listing configuration");

    let config = Config::load()?;

    println!("Configuration:");
    println!("==============\n");

    println!("Plugins:");
    for (name, plugin) in &config.plugins {
        println!("  {} -> {}", name, plugin.command);
    }

    println!("\nSettings:");
    if config.settings.is_empty() {
        println!("  (none)");
    } else {
        for (key, value) in &config.settings {
            println!("  {} = {}", key, value);
        }
    }

    Ok(())
}

pub async fn get(key: String) -> Result<()> {
    info!("Getting config key: {}", key);

    let config = Config::load()?;

    if let Some(value) = config.get(&key) {
        println!("{} = {}", key, value);
    } else {
        println!("Config key '{}' not found", key);
    }

    Ok(())
}

pub async fn set(key: String, value: String) -> Result<()> {
    info!("Setting config key '{}' to '{}'", key, value);

    let mut config = Config::load()?;
    config.set(key.clone(), value.clone());
    config.save()?;

    println!("✅ Set '{}' = '{}'", key, value);

    Ok(())
}

pub async fn path() -> Result<()> {
    info!("Showing config path");

    let path = Config::get_config_path()?;
    println!("Config file: {}", path.display());

    Ok(())
}
