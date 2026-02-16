use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub plugins: HashMap<String, PluginConfig>,

    #[serde(default)]
    pub settings: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        let mut plugins = HashMap::new();

        // Default plugins
        plugins.insert(
            "claude".to_string(),
            PluginConfig {
                command: "claude".to_string(),
                args: vec![],
                env: HashMap::from([("ANTHROPIC_API_KEY".to_string(), "${ANTHROPIC_API_KEY}".to_string())]),
            },
        );

        plugins.insert(
            "opencode".to_string(),
            PluginConfig {
                command: "opencode".to_string(),
                args: vec![],
                env: HashMap::new(),
            },
        );

        plugins.insert(
            "cursor".to_string(),
            PluginConfig {
                command: "cursor".to_string(),
                args: vec![],
                env: HashMap::new(),
            },
        );

        Self {
            plugins,
            settings: HashMap::new(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = Self::get_config_path()?;

        if !path.exists() {
            // Create default config
            let config = Self::default();
            config.save()?;
            return Ok(config);
        }

        let contents = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config from {}", path.display()))?;

        let config: Config = serde_yaml::from_str(&contents)
            .with_context(|| format!("Failed to parse config from {}", path.display()))?;

        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::get_config_path()?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .context("Failed to create config directory")?;
        }

        let contents = serde_yaml::to_string(self)
            .context("Failed to serialize config")?;

        fs::write(&path, contents)
            .with_context(|| format!("Failed to write config to {}", path.display()))?;

        Ok(())
    }

    pub fn get_config_path() -> Result<PathBuf> {
        let config_dir = dirs::home_dir()
            .context("Could not determine home directory")?
            .join(".config")
            .join("acta");

        Ok(config_dir.join("config.yaml"))
    }

    pub fn get(&self, key: &str) -> Option<&String> {
        self.settings.get(key)
    }

    pub fn set(&mut self, key: String, value: String) {
        self.settings.insert(key, value);
    }

    pub fn get_plugin(&self, name: &str) -> Option<&PluginConfig> {
        self.plugins.get(name)
    }

    pub fn register_plugin(&mut self, name: String, config: PluginConfig) {
        self.plugins.insert(name, config);
    }

    pub fn remove_plugin(&mut self, name: &str) -> Option<PluginConfig> {
        self.plugins.remove(name)
    }
}
