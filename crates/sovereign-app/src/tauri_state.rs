use std::sync::{Arc, Mutex};

use sovereign_core::config::AppConfig;
use sovereign_core::interfaces::OrchestratorEvent;
use sovereign_core::security::ActionDecision;
use sovereign_core::interfaces::FeedbackEvent;
use sovereign_db::surreal::SurrealGraphDB;

/// Runtime model assignment (router + reasoning filenames).
pub struct ModelAssignments {
    pub router: String,
    pub reasoning: String,
}

/// Shared application state managed by Tauri.
///
/// All Tauri commands receive an immutable reference to this via
/// `tauri::State<'_, AppState>`. Interior mutability is handled by
/// Arc + Mutex on the individual subsystems.
pub struct AppState {
    pub db: Arc<SurrealGraphDB>,
    pub orchestrator: Option<Arc<sovereign_ai::Orchestrator>>,
    pub config: AppConfig,
    pub skill_registry: Arc<sovereign_skills::SkillRegistry>,
    pub skill_db: Arc<dyn sovereign_skills::SkillDbAccess>,
    /// LLM access for skills that declare `Capability::LlmInference`.
    /// `None` when the orchestrator failed to initialize (model load error).
    pub skill_llm: Option<Arc<dyn sovereign_skills::SkillLlmAccess>>,
    pub decision_tx: tokio::sync::mpsc::Sender<ActionDecision>,
    pub feedback_tx: tokio::sync::mpsc::Sender<FeedbackEvent>,
    /// Sender end for orchestrator events (for forwarding to Tauri events).
    pub orch_tx: std::sync::mpsc::Sender<OrchestratorEvent>,
    /// Current UI theme ("dark" or "light").
    pub theme: Mutex<String>,
    /// Auto-commit engine for document edits.
    pub autocommit: Arc<tokio::sync::Mutex<sovereign_ai::AutoCommitEngine>>,
    /// Current model assignments (router + reasoning filenames).
    pub model_assignments: Mutex<ModelAssignments>,
    /// User profile directory path (~/.sovereign).
    pub profile_dir: std::path::PathBuf,
    /// User-scoped key for PII vault, body_raw, and session-log encryption.
    /// None until the user completes onboarding or logs in (post-login the
    /// lock holds Some). Same value on every paired device, so encrypted
    /// at-rest data is portable across the user's devices.
    ///
    /// Wrapped in tokio::sync::RwLock so it can be installed by Tauri
    /// commands (validate_password, complete_onboarding) after AppState
    /// is constructed. Reads return a snapshot Arc clone so the guard
    /// is dropped immediately.
    #[cfg(feature = "encryption")]
    pub account_key: tokio::sync::RwLock<Option<Arc<sovereign_crypto::account_key::AccountKey>>>,
    /// Per-device key used solely for libp2p identity derivation (PeerId).
    /// Different on every device. Installed alongside `account_key` in the
    /// post-authentication flow; consumed by P2P startup.
    #[cfg(feature = "encryption")]
    pub p2p_identity_key: tokio::sync::RwLock<Option<Arc<sovereign_crypto::device_key::DeviceKey>>>,
    /// Whisper STT engine for mobile voice-to-text (Web Audio API → Whisper).
    /// Populated when voice-stt feature is enabled and whisper model exists.
    /// Desktop uses the cpal-based VoicePipeline instead.
    #[cfg(feature = "voice-stt")]
    pub stt_engine: Option<Arc<tokio::sync::Mutex<sovereign_ai::voice::stt::SttEngine>>>,
}

#[cfg(feature = "encryption")]
impl AppState {
    /// Snapshot the account key. Cheap (one Arc::clone) and the read
    /// guard is dropped before returning, so callers can hold the
    /// result without blocking other readers / writers.
    pub async fn account_key(&self) -> Option<Arc<sovereign_crypto::account_key::AccountKey>> {
        self.account_key.read().await.clone()
    }

    /// Install a freshly-derived account key (called post-authentication).
    pub async fn set_account_key(&self, key: Arc<sovereign_crypto::account_key::AccountKey>) {
        *self.account_key.write().await = Some(key);
    }

    /// Snapshot the per-device libp2p identity key.
    pub async fn p2p_identity_key(&self) -> Option<Arc<sovereign_crypto::device_key::DeviceKey>> {
        self.p2p_identity_key.read().await.clone()
    }

    /// Install the per-device libp2p identity key (called post-authentication).
    pub async fn set_p2p_identity_key(&self, key: Arc<sovereign_crypto::device_key::DeviceKey>) {
        *self.p2p_identity_key.write().await = Some(key);
    }
}
