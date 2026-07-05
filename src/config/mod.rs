use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

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
                env: HashMap::from([(
                    "ANTHROPIC_API_KEY".to_string(),
                    "${ANTHROPIC_API_KEY}".to_string(),
                )]),
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
                command: "cursor-agent".to_string(),
                args: vec![],
                env: HashMap::new(),
            },
        );

        plugins.insert(
            "codex".to_string(),
            PluginConfig {
                command: "codex".to_string(),
                args: vec![],
                env: HashMap::new(),
            },
        );

        plugins.insert(
            "gemini".to_string(),
            PluginConfig {
                command: "gemini".to_string(),
                args: vec![],
                env: HashMap::new(),
            },
        );

        plugins.insert(
            "aider".to_string(),
            PluginConfig {
                command: "aider".to_string(),
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
            fs::create_dir_all(parent).context("Failed to create config directory")?;
        }

        let contents = serde_yaml::to_string(self).context("Failed to serialize config")?;

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

/// Expand `${VAR}` references from the environment. Returns `None` when the
/// value is a single unset variable reference, so plugin env entries like
/// `ANTHROPIC_API_KEY: "${ANTHROPIC_API_KEY}"` don't override an inherited
/// value with an empty string.
pub fn expand_env_value(value: &str) -> Option<String> {
    let mut out = String::new();
    let mut rest = value;
    let mut had_unset = false;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            out.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let var = &after[..end];
        match std::env::var(var) {
            Ok(val) => out.push_str(&val),
            Err(_) => had_unset = true,
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    if out.is_empty() && had_unset {
        None
    } else {
        Some(out)
    }
}
