use std::sync::{Arc, Mutex};

use sovereign_core::config::AppConfig;
use sovereign_core::interfaces::OrchestratorEvent;
use sovereign_core::security::ActionDecision;
use sovereign_core::interfaces::FeedbackEvent;
use sovereign_db::layered::LayeredGraphDB;

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
/// In-flight Guardian **Access** Recovery secrets, held only for the duration
/// of a running app process (RECOVERY-001). The user sets the NEW passphrase up
/// front (`start_access_recovery`); we derive the share-sealing key from it and
/// keep both here so `access_recovery_poll`/`finalize` can unseal without a
/// re-prompt. Nothing here is persisted — after an app restart during the
/// multi-day recovery window the user calls `resume_access_recovery` with the
/// same passphrase to re-derive the seal key and rehydrate this slot. Cleared on
/// `finalize`/`cancel`. The passphrase is `Zeroizing` and the whole struct
/// wipes the seal key on drop.
#[cfg(feature = "encryption")]
pub struct RecoverySession {
    /// AEAD key sealing the in-progress shares at rest. Re-derived from the
    /// passphrase + the stored per-recovery salt; never itself persisted.
    pub seal_key: [u8; 32],
    /// The new passphrase, re-created into `auth.store` at `finalize`.
    pub new_passphrase: sovereign_crypto::zeroize::Zeroizing<String>,
}

#[cfg(feature = "encryption")]
impl Drop for RecoverySession {
    fn drop(&mut self) {
        use sovereign_crypto::zeroize::Zeroize;
        self.seal_key.zeroize();
        // new_passphrase (Zeroizing) wipes itself on drop.
    }
}

pub struct AppState {
    /// `LayeredGraphDB` indirection so the DB can be swapped from raw
    /// `SurrealGraphDB` to `EncryptedGraphDB` at login. Trait calls go through
    /// the current inner via an atomic Arc swap; consumers don't observe the
    /// transition. See `crates/sovereign-db/src/layered.rs`.
    pub db: Arc<LayeredGraphDB>,
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
    /// Content KEK (Feature 1): the random key that wraps document keys.
    /// Retained post-login so recovery-setup can seal it (with the
    /// AccountKey) into the guardian Recovery Bundle. Same on every paired
    /// device; None until login/onboarding.
    #[cfg(feature = "encryption")]
    pub kek: tokio::sync::RwLock<Option<Arc<sovereign_crypto::kek::Kek>>>,
    /// In-flight Guardian Access Recovery secrets (RECOVERY-001), held only
    /// while an app process is running a recovery. `None` outside recovery;
    /// populated by `start`/`resume`, cleared by `finalize`/`cancel`. Not
    /// persisted — see [`RecoverySession`].
    #[cfg(feature = "encryption")]
    pub recovery_session: tokio::sync::RwLock<Option<RecoverySession>>,
    /// Per-device key used solely for libp2p identity derivation (PeerId).
    /// Different on every device. Installed alongside `account_key` in the
    /// post-authentication flow; consumed by P2P startup.
    #[cfg(feature = "encryption")]
    pub p2p_identity_key: tokio::sync::RwLock<Option<Arc<sovereign_crypto::device_key::DeviceKey>>>,
    /// Pending pairing payload + PIN, generated by `generate_pair_qr` on
    /// the existing device and held until the new device redeems it (or
    /// the TTL expires). Single-use; cleared after a successful consume
    /// on the existing device's side or when a new pairing is generated.
    #[cfg(feature = "encryption")]
    pub pending_pairing:
        tokio::sync::RwLock<Option<(sovereign_crypto::pair_payload::PairPayload, String)>>,
    /// The document service (C-DOC) over the persona's encrypted store: every
    /// document save, skill write-back and restore goes through it (adoption
    /// step 2). Built by `install_session`; `None` while locked.
    #[cfg(feature = "encryption")]
    pub docs: tokio::sync::RwLock<Option<Arc<sovereign_docs::DocumentService>>>,
    /// The one-time migration to document format v2 started at the last
    /// unlock (C-DOC adoption step 6): a later unlock replaces it, stopping a
    /// run still under way over the previous persona's store (it resumes at
    /// that store's next unlock, M3).
    #[cfg(feature = "encryption")]
    pub migration_task: std::sync::Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    /// One editing session per open document (the document panel / reader),
    /// keyed by document id: created at its first save, ended by
    /// `close_document`. See [`crate::doc_saves`].
    pub edit_sessions: std::sync::Mutex<std::collections::HashMap<String, crate::doc_saves::SessionState>>,
    /// P2P command sender for queueing `StartSync` / `PairDevice` /
    /// `Shutdown` from Tauri commands and the periodic poll task.
    /// `None` until `install_session` runs the post-login P2P startup.
    #[cfg(feature = "p2p")]
    pub p2p_command_tx:
        tokio::sync::RwLock<Option<tokio::sync::mpsc::Sender<sovereign_p2p::P2pCommand>>>,
    /// In-memory paired devices, persisted under
    /// `~/.sovereign/crypto/paired_devices.json`. `None` until
    /// `install_session` loads (or initialises) the manager. Arc-shared
    /// with the sync_startup event translator, which persists the new
    /// device record when the P3.1 handshake emits `PairingCompleted`.
    #[cfg(feature = "p2p")]
    pub pairing_manager:
        Arc<tokio::sync::RwLock<Option<sovereign_p2p::pairing::PairingManager>>>,
    /// PR2 (spec ch. 05; v0.0.13, item C3): the question the source side
    /// holds after a device joined under a name a paired device has, until
    /// she answers (`answer_same_name`). Shared with the event translator.
    /// `None` when nothing is asked. A field, not a process-wide static, so
    /// that each test's state holds its own question (the test author added
    /// it to the `AppState` literals of the earlier test files).
    #[cfg(feature = "p2p")]
    pub same_name: Arc<std::sync::Mutex<Option<crate::tauri_commands::pairing::SameNameQuestionDto>>>,
    /// Concrete listen multiaddrs the P2P swarm reported (interface-
    /// expanded). Collected by the event translator; pairing offers use
    /// them as dial hints so the new device doesn't depend on mDNS.
    #[cfg(feature = "p2p")]
    pub p2p_listen_addrs: Arc<std::sync::RwLock<Vec<String>>>,
    /// Opt-in backup host store (P4.2): fragments + guardian shards we
    /// hold for other users. `None` until P2P startup, or when
    /// `p2p.backup_host_enabled` is off. Shared with the node.
    #[cfg(feature = "p2p")]
    pub backup_host:
        tokio::sync::RwLock<Option<std::sync::Arc<sovereign_p2p::BackupHost>>>,
    /// Latest connectivity state reported by the Android plugin (Phase
    /// 4.2). Shared with the sync_startup event translator + periodic
    /// poll so they can suppress auto-sync on Cellular/Offline when
    /// `config.p2p.wifi_only` is true. Defaults to `Unknown`; desktop
    /// builds leave it there (wifi_only is false on desktop, so the
    /// gate is permissive).
    #[cfg(feature = "p2p")]
    pub connectivity:
        std::sync::Arc<std::sync::atomic::AtomicU8>,
    /// Whisper STT engine for mobile voice-to-text (Web Audio API → Whisper).
    /// Populated when voice-stt feature is enabled and whisper model exists.
    /// Desktop uses the cpal-based VoicePipeline instead.
    #[cfg(feature = "voice-stt")]
    pub stt_engine: Option<Arc<tokio::sync::Mutex<sovereign_ai::voice::stt::SttEngine>>>,
}

/// IPC-005: data commands may only be invoked from the trusted main
/// webview. The embedded `browser` webview renders arbitrary external
/// pages; even while the session is unlocked it must never reach data
/// commands through the IPC bridge.
pub fn require_main_webview(caller: &tauri::Webview) -> Result<(), String> {
    if caller.label() == "main" {
        Ok(())
    } else {
        Err(format!("Command not available to webview '{}'", caller.label()))
    }
}

// Without the encryption feature there is no login/account-key concept, so
// the session check is a no-op (keeps the ~65 gated commands compiling in
// all feature configurations). The shipped builds always enable encryption.
// The caller (webview) check still applies in every configuration.
#[cfg(not(feature = "encryption"))]
impl AppState {
    pub async fn require_unlocked(&self, caller: &tauri::Webview) -> Result<(), String> {
        require_main_webview(caller)?;
        self.require_session_unlocked().await
    }

    pub async fn require_session_unlocked(&self) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(feature = "encryption")]
impl AppState {
    /// Snapshot the account key. Cheap (one Arc::clone) and the read
    /// guard is dropped before returning, so callers can hold the
    /// result without blocking other readers / writers.
    pub async fn account_key(&self) -> Option<Arc<sovereign_crypto::account_key::AccountKey>> {
        self.account_key.read().await.clone()
    }

    /// Authorization gate for data/IPC commands (IPC-004 + IPC-005).
    /// Rejects unless the caller is the trusted main webview AND a session
    /// is unlocked — i.e. `install_session` has installed the account key
    /// and swapped in the EncryptedGraphDB. Bootstrap commands (onboarding,
    /// auth, theme, connectivity, voice, paired-onboarding) must NOT call
    /// this; every other command must call it first so the plaintext
    /// bootstrap DB is never readable pre-login.
    pub async fn require_unlocked(&self, caller: &tauri::Webview) -> Result<(), String> {
        require_main_webview(caller)?;
        self.require_session_unlocked().await
    }

    /// Caller-agnostic session check (IPC-004 only). For internal callers
    /// with no webview, and for the one command legitimately invoked FROM
    /// the browser webview (`__browser_form_extracted`), which enforces its
    /// own caller restriction instead.
    pub async fn require_session_unlocked(&self) -> Result<(), String> {
        if self.account_key.read().await.is_some() {
            Ok(())
        } else {
            Err("Not authenticated: log in first.".to_string())
        }
    }

    /// Install a freshly-derived account key (called post-authentication).
    pub async fn set_account_key(&self, key: Arc<sovereign_crypto::account_key::AccountKey>) {
        *self.account_key.write().await = Some(key);
    }

    /// Snapshot the content KEK (Feature 1 recovery setup).
    #[cfg(feature = "encryption")]
    pub async fn kek(&self) -> Option<Arc<sovereign_crypto::kek::Kek>> {
        self.kek.read().await.clone()
    }

    /// Install the content KEK (called post-authentication, alongside the
    /// account key).
    #[cfg(feature = "encryption")]
    pub async fn set_kek(&self, key: Arc<sovereign_crypto::kek::Kek>) {
        *self.kek.write().await = Some(key);
    }

    /// Install the in-flight access-recovery secrets (RECOVERY-001).
    #[cfg(feature = "encryption")]
    pub async fn set_recovery_session(&self, session: RecoverySession) {
        *self.recovery_session.write().await = Some(session);
    }

    /// Snapshot the share-sealing key of the in-flight recovery, if one is open.
    /// Copy — the guard is dropped immediately.
    #[cfg(feature = "encryption")]
    pub async fn recovery_seal_key(&self) -> Option<[u8; 32]> {
        self.recovery_session.read().await.as_ref().map(|s| s.seal_key)
    }

    /// Snapshot the new passphrase of the in-flight recovery, if one is open.
    #[cfg(feature = "encryption")]
    pub async fn recovery_new_passphrase(
        &self,
    ) -> Option<sovereign_crypto::zeroize::Zeroizing<String>> {
        self.recovery_session
            .read()
            .await
            .as_ref()
            .map(|s| s.new_passphrase.clone())
    }

    /// Drop the in-flight recovery secrets (on finalize/cancel). The removed
    /// `RecoverySession` zeroizes on drop.
    #[cfg(feature = "encryption")]
    pub async fn clear_recovery_session(&self) {
        *self.recovery_session.write().await = None;
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

/// The persona of the session open now: see [`AppState::session_persona`].
/// Process wide for the reason given at [`AppState::set_p2p_without_dns`]: a
/// field would break the `AppState` literals of the tests, which the
/// implementer does not edit. One `AppState` per process.
static SESSION_PERSONA: std::sync::Mutex<Option<sovereign_core::auth::PersonaKind>> = std::sync::Mutex::new(None);

impl AppState {
    /// The persona whose session is open now, `None` before the first login
    /// (C-AREA, spec ch. 07: what a session reads and writes follows it).
    pub fn session_persona(&self) -> Option<sovereign_core::auth::PersonaKind> {
        SESSION_PERSONA.lock().map(|g| *g).unwrap_or(None)
    }

    /// Records the persona of the session being opened (at login, by
    /// `install_session`).
    pub(crate) fn set_session_persona(&self, persona: Option<sovereign_core::auth::PersonaKind>) {
        if let Ok(mut g) = SESSION_PERSONA.lock() {
            *g = persona;
        }
    }

    /// C-SUP (spec ch. 05, *The node's supervision*): the node ended without
    /// the session asking for it. The supervisor's `on_stop` calls this, the
    /// app's counterpart of the shell's `App::on_sync_stopped`: the state
    /// knows sync stopped (so `node_refusal` answers with the stop's text), and
    /// the `sync-stopped` event goes to the screens. Why it ended goes to the
    /// log.
    ///
    /// *Signature by the C-SUP test author (lot LQ); the body is the
    /// implementer's.*
    #[cfg(feature = "p2p")]
    // The supervisor's `on_stop` runs the same body (`sync_startup::node_stopped`)
    // through the event channel it holds: the closure cannot hold the `AppState`.
    pub fn on_sync_stopped(&self, end: sovereign_p2p::supervision::NodeEnd) {
        crate::sync_startup::node_stopped(&self.orch_tx, &self.profile_dir, end);
    }

    /// C-SUP, DEL-0019 (the Fable maille, by delegation of Céline): the words
    /// a command that needs the node answers when no node runs. While sync is
    /// stopped (the node ended without the session asking), the stop's notice
    /// text, « Sync stopped unexpectedly. Nothing syncs until you restart it. »,
    /// so the screen shows the stop's notice again with « Restart sync »; when
    /// sync was never started, `when_never_started`, the command's own
    /// sentence, as before.
    ///
    /// *Signature by the C-SUP test author (lot LQ); the body is the
    /// implementer's.*
    pub fn node_refusal(&self, when_never_started: &str) -> String {
        #[cfg(feature = "p2p")]
        if crate::sync_startup::sync_stopped(&self.profile_dir) {
            return crate::sync_startup::SYNC_STOPPED_TEXT.to_string();
        }
        when_never_started.to_string()
    }

    /// Whether the open session is the duress persona's.
    #[cfg_attr(not(feature = "p2p"), allow(dead_code))] // its callers are the p2p build's
    pub fn is_duress_session(&self) -> bool {
        self.session_persona() == Some(sovereign_core::auth::PersonaKind::Duress)
    }

    /// The directory of the open persona's identity profile on this host
    /// (`user_profile.json`: designation, nickname, display name), read and
    /// written only after login (C-AREA AR2, « user_profile.json: into the
    /// area »). The primary keeps the historical location, the data directory,
    /// so an existing profile is the primary's (ch. 07, *Migration*); the
    /// duress persona's is in its own state directory, under `identity/`
    /// (the phone keeps no other directory of the persona's own).
    ///
    /// **No session, no identity directory** (C-AREA AR2, v2.0.87: « A process
    /// with no session has no identity directory »): `None` before any login,
    /// and after a failed one.
    pub fn identity_dir(&self) -> Option<std::path::PathBuf> {
        self.session_persona().map(|p| persona_identity_dir(&self.profile_dir, Some(p)))
    }
}

/// See [`AppState::identity_dir`]: the identity profile's directory of
/// `persona` under the data directory `base` (`None`: the primary's).
pub fn persona_identity_dir(
    base: &std::path::Path,
    persona: Option<sovereign_core::auth::PersonaKind>,
) -> std::path::PathBuf {
    match persona {
        Some(sovereign_core::auth::PersonaKind::Duress) => {
            sovereign_core::auth::orchestrator_state_dir(base, sovereign_core::auth::PersonaKind::Duress).join("identity")
        }
        _ => base.to_path_buf(),
    }
}

/// See [`AppState::set_p2p_without_dns`] (v0.0.12 item 12).
#[cfg(feature = "p2p")]
static P2P_WITHOUT_DNS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(feature = "p2p")]
impl AppState {
    /// Snapshot the P2P command channel. Cheap (one mpsc::Sender clone).
    pub async fn p2p_command_tx(
        &self,
    ) -> Option<tokio::sync::mpsc::Sender<sovereign_p2p::P2pCommand>> {
        // C-SUP: a node that ended by itself takes no command; its channel is
        // dropped by the next stop or restart (`stop_p2p_node`).
        if crate::sync_startup::sync_stopped(&self.profile_dir) {
            return None;
        }
        self.p2p_command_tx.read().await.clone()
    }

    /// Install the P2P command channel after the post-login node start.
    pub async fn set_p2p_command_tx(
        &self,
        tx: tokio::sync::mpsc::Sender<sovereign_p2p::P2pCommand>,
    ) {
        *self.p2p_command_tx.write().await = Some(tx);
    }

    /// Record whether the running P2P node was built WITHOUT the DNS
    /// transport (v0.0.12 item 12: no system resolver config, as on Android).
    /// Set by `start_p2p_node` before the node runs.
    ///
    /// Kept in a process-wide flag rather than an `AppState` field: a new
    /// field would break the full `AppState` literal of
    /// `lot1_onboarding_tests.rs` under the `p2p` feature, a test file the
    /// implementer does not edit. One `AppState` per process, so the scope is
    /// the same.
    pub fn set_p2p_without_dns(&self, without_dns: bool) {
        P2P_WITHOUT_DNS.store(without_dns, std::sync::atomic::Ordering::Relaxed);
    }

    /// True while the P2P node runs without the DNS transport (item 12, P4).
    /// False when the node is not running, whatever was recorded.
    pub async fn p2p_without_dns(&self) -> bool {
        self.p2p_command_tx().await.is_some()
            && P2P_WITHOUT_DNS.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Snapshot the current connectivity state. Lock-free atomic read.
    pub fn connectivity_state(&self) -> sovereign_p2p::ConnectivityState {
        connectivity_from_u8(
            self.connectivity.load(std::sync::atomic::Ordering::Relaxed),
        )
    }

    /// Update the connectivity state. Called by the
    /// `set_connectivity_state` Tauri command, which the Android plugin
    /// invokes on every network transition.
    pub fn set_connectivity_state(&self, state: sovereign_p2p::ConnectivityState) {
        self.connectivity
            .store(connectivity_to_u8(state), std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(feature = "p2p")]
fn connectivity_to_u8(state: sovereign_p2p::ConnectivityState) -> u8 {
    use sovereign_p2p::ConnectivityState;
    match state {
        ConnectivityState::Unknown => 0,
        ConnectivityState::Wifi => 1,
        ConnectivityState::Cellular => 2,
        ConnectivityState::Offline => 3,
    }
}

#[cfg(feature = "p2p")]
fn connectivity_from_u8(byte: u8) -> sovereign_p2p::ConnectivityState {
    use sovereign_p2p::ConnectivityState;
    match byte {
        1 => ConnectivityState::Wifi,
        2 => ConnectivityState::Cellular,
        3 => ConnectivityState::Offline,
        _ => ConnectivityState::Unknown,
    }
}

impl AppState {
    /// The document service, once a session is unlocked.
    pub async fn docs(&self) -> Result<Arc<sovereign_docs::DocumentService>, String> {
        #[cfg(feature = "encryption")]
        {
            self.docs
                .read()
                .await
                .clone()
                .ok_or_else(|| "The workspace is locked: log in to change documents.".to_string())
        }
        #[cfg(not(feature = "encryption"))]
        {
            Err("Changing documents needs the encryption build.".to_string())
        }
    }
}
