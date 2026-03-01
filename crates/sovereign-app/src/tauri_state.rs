use std::sync::{Arc, Mutex};

use sovereign_core::config::AppConfig;
use sovereign_core::interfaces::OrchestratorEvent;
use sovereign_core::security::ActionDecision;
use sovereign_core::interfaces::FeedbackEvent;
use sovereign_db::surreal::SurrealGraphDB;

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
    pub decision_tx: tokio::sync::mpsc::Sender<ActionDecision>,
    pub feedback_tx: tokio::sync::mpsc::Sender<FeedbackEvent>,
    /// Sender end for orchestrator events (for forwarding to Tauri events).
    pub orch_tx: std::sync::mpsc::Sender<OrchestratorEvent>,
    /// Current UI theme ("dark" or "light").
    pub theme: Mutex<String>,
}
