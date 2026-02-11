//! Week 7 cross-phase interface definitions.
//!
//! These are signature-only stubs defining the contracts between phases.
//! Phase 2 (Canvas), Phase 3 (AI), and Phase 4 (Skills) will implement them.

use async_trait::async_trait;

use crate::security::{BubbleVisualState, ProposedAction};

/// Controls the spatial canvas viewport and highlights.
/// Implemented in Phase 2 by sovereign-canvas.
pub trait CanvasController: Send + Sync {
    fn navigate_to_document(&self, doc_id: &str);
    fn highlight_card(&self, doc_id: &str, highlight: bool);
    fn zoom_to_thread(&self, thread_id: &str);
    fn get_viewport(&self) -> Viewport;
}

/// Current canvas viewport state.
#[derive(Debug, Clone)]
pub struct Viewport {
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
    pub width: f64,
    pub height: f64,
}

/// Events emitted by the AI orchestrator.
/// Phase 3 produces these; the UI and skills consume them.
#[derive(Debug, Clone)]
pub enum OrchestratorEvent {
    DocumentOpened { doc_id: String },
    SearchResults { query: String, doc_ids: Vec<String> },
    ActionProposed { proposal: ProposedAction },
    ActionExecuted { action: String, success: bool },
    ActionRejected { action: String, reason: String },
    InjectionDetected { source: String, pattern: String },
    BubbleState(BubbleVisualState),
    ThreadCreated { thread_id: String, name: String },
    ThreadRenamed { thread_id: String, name: String },
    ThreadDeleted { thread_id: String },
    DocumentMoved { doc_id: String, new_thread_id: String },
    ThreadMerged { target_id: String, source_id: String },
    ThreadSplit { new_thread_id: String, name: String, doc_ids: Vec<String> },
    AdoptionStarted { doc_id: String },
    MilestoneCreated { milestone_id: String, title: String, thread_id: String },
    MilestonesListed { thread_id: String, milestones: Vec<MilestoneSummary> },
    Suggestion { text: String, action: String },
    VersionHistory { doc_id: String, commits: Vec<CommitSummary> },
    SkillResult { skill: String, action: String, kind: String, data: String },
    // P2P sync events
    SyncStatus { peer_id: String, status: String },
    SyncConflict { doc_id: String, description: String },
    DeviceDiscovered { device_id: String, device_name: String },
    DevicePaired { device_id: String },
    // Guardian events
    GuardianEnrolled { guardian_id: String, name: String },
    GuardianDropped { guardian_id: String, reason: String },
    ShardRotated { epoch: u32 },
    RecoveryInitiated { request_id: String },
    RecoveryCompleted,
    RecoveryAborted { reason: String },
    // Encryption events
    EncryptionEnabled,
    MigrationProgress { encrypted: u32, total: u32 },
    MigrationComplete,
    // Document creation
    DocumentCreated { doc_id: String, title: String, thread_id: String },
    // Communications events
    NewMessagesReceived { channel: String, count: u32, conversation_id: String },
    MessageSent { channel: String, message_id: String },
    CommsSyncComplete { channel: String, new_messages: u32 },
    CommsSyncError { channel: String, error: String },
    ContactCreated { contact_id: String, name: String },
}

/// Lightweight milestone summary for milestone events.
#[derive(Debug, Clone)]
pub struct MilestoneSummary {
    pub id: String,
    pub title: String,
    pub timestamp: String,
    pub description: String,
}

/// Lightweight commit summary for version history events.
#[derive(Debug, Clone)]
pub struct CommitSummary {
    pub id: String,
    pub message: String,
    pub timestamp: String,
}

/// Feedback events sent from the UI back to the orchestrator
/// when a user accepts or dismisses a proactive suggestion.
#[derive(Debug, Clone)]
pub enum FeedbackEvent {
    SuggestionAccepted { action: String },
    SuggestionDismissed { action: String },
}

/// Parsed user intent from the AI router.
#[derive(Debug, Clone)]
pub struct UserIntent {
    pub action: String,
    pub target: Option<String>,
    pub confidence: f32,
    pub entities: Vec<(String, String)>,
    /// Whether this intent originated from user input (Control) or document content (Data).
    pub origin: crate::security::Plane,
}

/// Events emitted by the canvas/skills system for document actions.
#[derive(Debug, Clone)]
pub enum SkillEvent {
    OpenDocument { doc_id: String },
    DocumentClosed { doc_id: String },
}

/// Commands for controlling the voice pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceCommand {
    StartListening,
    StopListening,
}

/// Voice activation mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceMode {
    WakeWord,
    PushToTalk,
}

/// Backend for loading and running LLM inference.
/// Implemented in Phase 3 by sovereign-ai.
///
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_command_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<VoiceCommand>();
    }

    #[test]
    fn voice_mode_variants() {
        assert_ne!(VoiceMode::WakeWord, VoiceMode::PushToTalk);
    }

    #[test]
    fn suggestion_event_clone() {
        let event = OrchestratorEvent::Suggestion {
            text: "Try organizing your docs".into(),
            action: "create_thread".into(),
        };
        let cloned = event.clone();
        match cloned {
            OrchestratorEvent::Suggestion { text, action } => {
                assert_eq!(text, "Try organizing your docs");
                assert_eq!(action, "create_thread");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn skill_event_is_send_and_clone() {
        fn assert_send<T: Send>() {}
        fn assert_clone<T: Clone>() {}
        assert_send::<SkillEvent>();
        assert_clone::<SkillEvent>();

        let event = SkillEvent::OpenDocument {
            doc_id: "document:abc".into(),
        };
        let cloned = event.clone();
        match cloned {
            SkillEvent::OpenDocument { doc_id } => assert_eq!(doc_id, "document:abc"),
            SkillEvent::DocumentClosed { .. } => panic!("wrong variant"),
        }
    }
}

#[async_trait]
pub trait ModelBackend: Send + Sync {
    async fn load(&mut self, model_path: &str, n_gpu_layers: i32) -> anyhow::Result<()>;
    async fn generate(&self, prompt: &str, max_tokens: u32) -> anyhow::Result<String>;
    async fn unload(&mut self) -> anyhow::Result<()>;
}
