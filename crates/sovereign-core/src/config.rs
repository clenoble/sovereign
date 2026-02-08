use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub ai: AiConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    #[serde(default = "default_db_mode")]
    pub mode: String,
    #[serde(default = "default_db_path")]
    pub path: String,
}

fn default_db_mode() -> String {
    "persistent".into()
}
fn default_db_path() -> String {
    "data/sovereign.db".into()
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            mode: default_db_mode(),
            path: default_db_path(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct UiConfig {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_width")]
    pub default_width: i32,
    #[serde(default = "default_height")]
    pub default_height: i32,
}

fn default_theme() -> String {
    "dark".into()
}
fn default_width() -> i32 {
    1280
}
fn default_height() -> i32 {
    720
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            default_width: default_width(),
            default_height: default_height(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AiConfig {
    #[serde(default = "default_model_dir")]
    pub model_dir: String,
    #[serde(default)]
    pub router_model: String,
    #[serde(default)]
    pub reasoning_model: String,
    #[serde(default = "default_n_gpu_layers")]
    pub n_gpu_layers: i32,
    #[serde(default = "default_n_ctx")]
    pub n_ctx: u32,
}

fn default_model_dir() -> String {
    "models".into()
}
fn default_n_gpu_layers() -> i32 {
    99
}
fn default_n_ctx() -> u32 {
    4096
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            model_dir: default_model_dir(),
            router_model: String::new(),
            reasoning_model: String::new(),
            n_gpu_layers: default_n_gpu_layers(),
            n_ctx: default_n_ctx(),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            database: DatabaseConfig::default(),
            ui: UiConfig::default(),
            ai: AiConfig::default(),
        }
    }
}

impl AppConfig {
    /// Load configuration from a TOML file.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: AppConfig = toml::from_str(&content)?;
        Ok(config)
    }

    /// Load config with fallback chain: explicit path → ./config/default.toml → hardcoded defaults.
    pub fn load_or_default(explicit_path: Option<&Path>) -> Self {
        if let Some(path) = explicit_path {
            match Self::load(path) {
                Ok(cfg) => return cfg,
                Err(e) => {
                    tracing::warn!("Failed to load config from {}: {e}", path.display());
                }
            }
        }

        let default_path = Path::new("config/default.toml");
        if default_path.exists() {
            match Self::load(default_path) {
                Ok(cfg) => return cfg,
                Err(e) => {
                    tracing::warn!("Failed to load default config: {e}");
                }
            }
        }

        tracing::info!("Using hardcoded default configuration");
        Self::default()
    }
}
