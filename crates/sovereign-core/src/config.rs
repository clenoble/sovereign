use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub vision: VisionConfig,
    #[serde(default)]
    pub crypto: CryptoConfig,
    #[serde(default)]
    pub p2p: P2pConfig,
    #[serde(default)]
    pub comms: CommsAppConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VoiceConfig {
    pub enabled: bool,
    /// Audio source: "cpal" (PC mic, default) or "jiminy" (Reachy Mini mic via WebSocket).
    pub voice_source: String,
    /// WebSocket URL for Jiminy audio (only used when voice_source = "jiminy").
    pub jiminy_ws_url: String,
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
            voice_source: "cpal".into(),
            jiminy_ws_url: "ws://127.0.0.1:9100/ws/audio".into(),
            wake_word_model: "models/sovereign.rpw".into(),
            whisper_model: "models/ggml-large-v3-turbo.bin".into(),
            piper_binary: "piper".into(),
            piper_model: String::new(),
            piper_config: String::new(),
        }
    }
}

/// Vision integration: the jiminy-vision service (gestures + windowed VLM scene
/// understanding) that feeds the orchestrator context and the camera UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VisionConfig {
    /// Whether the vision integration is active (poller + camera UI).
    pub enabled: bool,
    /// How long the windowed VLM scene-understanding stays open, in seconds.
    pub window_seconds: f64,
    /// Camera source for jiminy-vision: "webcam" (dev/desktop) or "robot".
    pub camera_source: String,
    /// Base URL of the jiminy-vision service.
    pub vision_url: String,
}

impl Default for VisionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            window_seconds: 300.0,
            camera_source: "webcam".into(),
            vision_url: "http://127.0.0.1:9101".into(),
        }
    }
}

/// Encryption-at-rest configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Whether mDNS LAN peer discovery is enabled (P2P-006). Default true.
    /// Set false to disable multicast discovery on untrusted/shared networks.
    #[serde(default = "default_true")]
    pub enable_mdns: bool,
    /// When true, suppress auto-sync triggers while the device reports
    /// cellular connectivity (Phase 4.2). Defaults to true on Android,
    /// false on desktop. The actual gating happens in
    /// `sovereign_p2p::ConnectivityState::allows_auto_sync`.
    pub wifi_only: bool,
    /// Opt-in to hosting other users' encrypted backup fragments and
    /// guardian key shards (P4.2). Off by default.
    pub backup_host_enabled: bool,
    /// Per-owner storage quota for hosted backup fragments, in MiB.
    pub backup_quota_mb: u64,
}

fn default_true() -> bool {
    true
}

impl Default for P2pConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            listen_port: 0,
            rendezvous_server: None,
            device_name: "Sovereign Device".into(),
            enable_mdns: true,
            wifi_only: cfg!(target_os = "android"),
            backup_host_enabled: false,
            backup_quota_mb: 64,
        }
    }
}

/// Communications configuration (email, messaging, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
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
            vision: VisionConfig::default(),
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

    /// Load config with fallback chain: explicit path → CWD → project root → hardcoded defaults.
    /// Relative paths in `ai.model_dir` are resolved against the directory that contained the
    /// config file (or the project root for hardcoded defaults).
    pub fn load_or_default(explicit_path: Option<&Path>) -> Self {
        let mut cfg = None;
        let mut config_root: Option<std::path::PathBuf> = None;

        if let Some(path) = explicit_path {
            match Self::load(path) {
                Ok(c) => {
                    config_root = path.parent().and_then(|p| p.parent()).map(|p| p.to_path_buf());
                    cfg = Some(c);
                }
                Err(e) => {
                    tracing::warn!("Failed to load config from {}: {e}", path.display());
                }
            }
        }

        if cfg.is_none() {
            // Search for config/default.toml relative to CWD, then project root
            let candidates = Self::config_search_paths();
            for path in &candidates {
                if path.exists() {
                    match Self::load(path) {
                        Ok(c) => {
                            // config_root = grandparent of config file (project root)
                            config_root =
                                path.parent().and_then(|p| p.parent()).map(|p| p.to_path_buf());
                            tracing::info!("Loaded config from {}", path.display());
                            cfg = Some(c);
                            break;
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to load config from {}: {e}",
                                path.display()
                            );
                        }
                    }
                }
            }
        }

        let root = config_root.unwrap_or_else(|| Self::project_root());

        let mut cfg = cfg.unwrap_or_else(|| {
            tracing::info!("Using hardcoded default configuration");
            Self::default()
        });

        // Resolve relative model_dir against the project root
        let model_path = Path::new(&cfg.ai.model_dir);
        if model_path.is_relative() {
            let resolved = root.join(model_path);
            cfg.ai.model_dir = resolved.to_string_lossy().into_owned();
            tracing::info!("Resolved model_dir to {}", cfg.ai.model_dir);
        }

        cfg
    }

    /// Candidate paths to search for `config/default.toml`.
    fn config_search_paths() -> Vec<std::path::PathBuf> {
        // INSTALLER-003: search ONLY the workspace/exe-anchored location (see
        // project_root), never a bare CWD-relative `config/default.toml`. On a
        // shipped build the latter let an attacker who controls the working
        // directory plant a config that the app would silently load.
        let project = Self::project_root();
        vec![project.join("config/default.toml")]
    }

    /// Best-effort project root: compile-time workspace root (dev), then the
    /// running executable's directory (shipped), then CWD as a last resort.
    fn project_root() -> std::path::PathBuf {
        // 1. Dev/workspace: CARGO_MANIFEST_DIR is crates/sovereign-app/ at
        //    compile time → up 2 levels. Only trust it if it actually holds the
        //    workspace layout (absent on an end-user machine).
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        if let Some(root) = manifest.parent().and_then(|p| p.parent()) {
            if root.join("config").is_dir() || root.join("models").is_dir() {
                return root.to_path_buf();
            }
        }
        // 2. INSTALLER-003: shipped build → anchor to the EXECUTABLE's directory,
        //    NOT the current working directory, so a process launched from an
        //    attacker-controlled CWD can't plant a config/model that gets loaded.
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                return dir.to_path_buf();
            }
        }
        // 3. Last resort (both of the above failed — very rare): CWD.
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vision_config_defaults() {
        let v = VisionConfig::default();
        assert!(v.enabled);
        assert_eq!(v.window_seconds, 300.0);
        assert_eq!(v.camera_source, "webcam");
        assert_eq!(v.vision_url, "http://127.0.0.1:9101");
    }

    #[test]
    fn app_config_default_includes_vision() {
        assert_eq!(AppConfig::default().vision.window_seconds, 300.0);
    }
}
