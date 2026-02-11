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
    #[serde(default)]
    pub voice: VoiceConfig,
    #[serde(default)]
    pub crypto: CryptoConfig,
    #[serde(default)]
    pub p2p: P2pConfig,
    #[serde(default)]
    pub comms: CommsAppConfig,
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

#[derive(Debug, Clone, Deserialize)]
pub struct VoiceConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_wake_word_model")]
    pub wake_word_model: String,
    #[serde(default = "default_whisper_model")]
    pub whisper_model: String,
    #[serde(default = "default_piper_binary")]
    pub piper_binary: String,
    #[serde(default)]
    pub piper_model: String,
    #[serde(default)]
    pub piper_config: String,
}

fn default_wake_word_model() -> String {
    "models/sovereign.rpw".into()
}
fn default_whisper_model() -> String {
    "models/ggml-large-v3-turbo.bin".into()
}
fn default_piper_binary() -> String {
    "piper".into()
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            wake_word_model: default_wake_word_model(),
            whisper_model: default_whisper_model(),
            piper_binary: default_piper_binary(),
            piper_model: String::new(),
            piper_config: String::new(),
        }
    }
}

/// Encryption-at-rest configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct CryptoConfig {
    /// Whether encryption is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Days before automatic key rotation (0 = disabled).
    #[serde(default = "default_key_rotation_days")]
    pub key_rotation_days: u32,
    /// Commits before automatic key rotation (0 = disabled).
    #[serde(default = "default_key_rotation_commits")]
    pub key_rotation_commits: u32,
}

fn default_key_rotation_days() -> u32 {
    90
}
fn default_key_rotation_commits() -> u32 {
    100
}

impl Default for CryptoConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            key_rotation_days: default_key_rotation_days(),
            key_rotation_commits: default_key_rotation_commits(),
        }
    }
}

/// P2P networking configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct P2pConfig {
    /// Whether P2P networking is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Port to listen on (0 = random).
    #[serde(default)]
    pub listen_port: u16,
    /// Optional rendezvous server address for WAN discovery.
    #[serde(default)]
    pub rendezvous_server: Option<String>,
    /// Human-readable device name shown to peers.
    #[serde(default = "default_device_name")]
    pub device_name: String,
}

fn default_device_name() -> String {
    "Sovereign Device".into()
}

impl Default for P2pConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            listen_port: 0,
            rendezvous_server: None,
            device_name: default_device_name(),
        }
    }
}

/// Communications configuration (email, messaging, etc.)
#[derive(Debug, Clone, Deserialize)]
pub struct CommsAppConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_comms_poll_interval")]
    pub poll_interval_secs: u64,
}

fn default_comms_poll_interval() -> u64 {
    300
}

impl Default for CommsAppConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            poll_interval_secs: default_comms_poll_interval(),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            database: DatabaseConfig::default(),
            ui: UiConfig::default(),
            ai: AiConfig::default(),
            voice: VoiceConfig::default(),
            crypto: CryptoConfig::default(),
            p2p: P2pConfig::default(),
            comms: CommsAppConfig::default(),
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
