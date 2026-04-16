use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub api: ApiConfig,
    pub poll: PollConfig,
    pub ui: UiConfig,
    pub groups: Vec<GroupConfig>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            api: ApiConfig::default(),
            poll: PollConfig::default(),
            ui: UiConfig::default(),
            groups: vec![
                GroupConfig {
                    id: "group-1".to_string(),
                    name: "BTC 主监控".to_string(),
                    symbol: "BTCUSDT".to_string(),
                    periods: vec![
                        "15".to_string(),
                        "60".to_string(),
                        "4D".to_string(),
                        "W".to_string(),
                    ],
                    signal_types: vec!["vegas".to_string()],
                    enabled: true,
                },
                GroupConfig {
                    id: "group-2".to_string(),
                    name: "ETH 主监控".to_string(),
                    symbol: "ETHUSDT".to_string(),
                    periods: vec![
                        "15".to_string(),
                        "60".to_string(),
                        "4D".to_string(),
                        "W".to_string(),
                    ],
                    signal_types: vec!["divMacd".to_string()],
                    enabled: true,
                },
            ],
        }
    }
}

impl AppConfig {
    pub fn load_or_create() -> Result<(Self, PathBuf)> {
        let path = config_path()?;
        if path.exists() {
            let raw = fs::read_to_string(&path)
                .with_context(|| format!("failed to read config file: {}", path.display()))?;
            let parsed: Self = serde_yaml::from_str(&raw)
                .with_context(|| format!("failed to parse yaml: {}", path.display()))?;
            return Ok((parsed, path));
        }

        let cfg = Self::default();
        cfg.save_to(&path)?;
        Ok((cfg, path))
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create config dir: {}", parent.display()))?;
        }
        let yaml = serde_yaml::to_string(self).context("failed to serialize config into yaml")?;
        fs::write(path, yaml)
            .with_context(|| format!("failed to write config file: {}", path.display()))?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    pub base_url: String,
    pub api_key: String,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            base_url: "https://example.com".to_string(),
            api_key: "replace-with-real-api-key".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollConfig {
    pub interval_secs: u64,
    pub page_size: u32,
}

impl Default for PollConfig {
    fn default() -> Self {
        Self {
            interval_secs: 8,
            page_size: 100,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WidgetConfig {
    pub visible: bool,
    pub x: f32,
    pub y: f32,
    pub size: f32,
}

impl Default for WidgetConfig {
    fn default() -> Self {
        Self {
            visible: true,
            x: 32.0,
            y: 32.0,
            size: 56.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub edge_mode: bool,
    pub edge_width: f32,
    pub always_on_top: bool,
    pub notifications: bool,
    pub sound: bool,
    pub widget: WidgetConfig,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            edge_mode: false,
            edge_width: 240.0,
            always_on_top: true,
            notifications: true,
            sound: false,
            widget: WidgetConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupConfig {
    pub id: String,
    pub name: String,
    pub symbol: String,
    pub periods: Vec<String>,
    pub signal_types: Vec<String>,
    pub enabled: bool,
}

fn config_path() -> Result<PathBuf> {
    if let Ok(explicit) = std::env::var("SIGNAL_DESK_CONFIG") {
        let path = PathBuf::from(explicit);
        if path.is_absolute() {
            return Ok(path);
        }
        let cwd = std::env::current_dir().context("unable to get current working dir")?;
        return Ok(cwd.join(path));
    }

    let cwd = std::env::current_dir().context("unable to get current working dir")?;
    let local = cwd.join("config.yaml");
    if local.exists() || cwd.join("config.yaml.example").exists() {
        return Ok(local);
    }

    let base = match dirs::config_dir() {
        Some(path) => path,
        None => cwd,
    };
    Ok(base.join("signal-desk-v2").join("config.yaml"))
}
