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
#[serde(default)]
pub struct DatabaseConfig {
    pub mode: String,
    pub path: String,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            mode: "persistent".into(),
            path: "data/sovereign.db".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub theme: String,
    pub default_width: i32,
    pub default_height: i32,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            default_width: 1280,
            default_height: 720,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AiConfig {
    pub model_dir: String,
    pub router_model: String,
    pub reasoning_model: String,
    pub n_gpu_layers: i32,
    pub n_ctx: u32,
    /// Prompt format: "chatml" (default), "mistral", "llama3".
    pub prompt_format: String,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            model_dir: "models".into(),
            router_model: String::new(),
            reasoning_model: String::new(),
            n_gpu_layers: 99,
            n_ctx: 4096,
            prompt_format: "chatml".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct VoiceConfig {
    pub enabled: bool,
    pub wake_word_model: String,
    pub whisper_model: String,
    pub piper_binary: String,
    pub piper_model: String,
    pub piper_config: String,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            wake_word_model: "models/sovereign.rpw".into(),
            whisper_model: "models/ggml-large-v3-turbo.bin".into(),
            piper_binary: "piper".into(),
            piper_model: String::new(),
            piper_config: String::new(),
        }
    }
}

/// Encryption-at-rest configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CryptoConfig {
    /// Whether encryption is enabled.
    pub enabled: bool,
    /// Days before automatic key rotation (0 = disabled).
    pub key_rotation_days: u32,
    /// Commits before automatic key rotation (0 = disabled).
    pub key_rotation_commits: u32,
    /// Whether keystroke dynamics are enabled for login.
    pub keystroke_enabled: bool,
    /// Minutes before re-authentication is required after keystroke anomaly.
    pub keystroke_reauth_minutes: u32,
    /// Maximum failed login attempts before lockout.
    pub max_login_attempts: u32,
    /// Seconds the account is locked after exceeding max attempts.
    pub lockout_seconds: u32,
}

impl Default for CryptoConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            key_rotation_days: 90,
            key_rotation_commits: 100,
            keystroke_enabled: true,
            keystroke_reauth_minutes: 30,
            max_login_attempts: 10,
            lockout_seconds: 300,
        }
    }
}

/// P2P networking configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct P2pConfig {
    /// Whether P2P networking is enabled.
    pub enabled: bool,
    /// Port to listen on (0 = random).
    pub listen_port: u16,
    /// Optional rendezvous server address for WAN discovery.
    pub rendezvous_server: Option<String>,
    /// Human-readable device name shown to peers.
    pub device_name: String,
}

impl Default for P2pConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            listen_port: 0,
            rendezvous_server: None,
            device_name: "Sovereign Device".into(),
        }
    }
}

/// Communications configuration (email, messaging, etc.)
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CommsAppConfig {
    pub enabled: bool,
    pub poll_interval_secs: u64,
}

impl Default for CommsAppConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            poll_interval_secs: 300,
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
