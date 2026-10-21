//! Cross-phase interface definitions.
//!
//! Traits and types defining the contracts between crates:
//! sovereign-ai (ModelBackend, OrchestratorEvent) and
//! sovereign-skills (SkillEvent).

use async_trait::async_trait;

use crate::security::{BubbleVisualState, ProposedAction};

/// Events emitted by the AI orchestrator and consumed by the UI and skills.
#[derive(Debug, Clone)]
pub enum OrchestratorEvent {
    DocumentOpened { doc_id: String },
    SearchResults { query: String, doc_ids: Vec<String> },
    ActionProposed { proposal: ProposedAction },
    ActionExecuted { action: String, success: bool },
    ActionRejected { action: String, reason: String },
    InjectionDetected {
        source: String,
        pattern: String,
        indicators: Vec<String>,
        severity: u8,
    },
    /// A high-severity injection was found in agent-loop tool output. The loop is
    /// PAUSED; the UI must present the choice and send an `InjectionDecision` on
    /// the injection-decision channel. Fails closed to `Redact` if no response.
    /// (INJECTION-002 — decision 2026-07-18.)
    InjectionDecisionRequested {
        source: String,
        pattern: String,
        indicators: Vec<String>,
        severity: u8,
        /// A bounded preview of the flagged content so the user can judge.
        preview: String,
    },
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
    DevicePaired { device_id: String, device_name: String },
    /// A P3.1 pairing handshake attempt failed. `offer_dead` means the
    /// offer self-destructed (expired or attempts exhausted) and the
    /// pairing QR must be regenerated.
    PairingFailed { reason: String, offer_dead: bool },
    /// A paired device runs an older version: documents are not exchanged
    /// with it until it is updated; other data still syncs (C-WIRE 12).
    /// `device_name` is empty when the device is not in the paired list.
    PeerNeedsUpdate { peer_id: String, device_name: String },
    /// The alerts on changes from paired devices may have changed (one was
    /// raised by the background model verdict, C-WIRE 13): the UI re-reads
    /// them. Alerts raised during sync are re-read on sync completion.
    SyncAlertsChanged,
    /// The one-time migration to document format v2 has completed in this
    /// persona's store (C-DOC adoption step 6, M2): the notice to show, once.
    /// `text` is `sovereign_docs::MigrationNotice::text()`; it names no
    /// document.
    DocumentsMigrated { text: String },
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
    // Chat response from LLM
    ChatResponse { text: String },
    // Web browsing events
    BrowserNavigated { url: String, title: String },
    BrowserContentExtracted { url: String, title: String, text: String },
    ReliabilityAssessed { doc_id: String, classification: String, score: f32 },
    // Memory consolidation events
    LinkSuggested {
        suggestion_id: String,
        from_doc_id: String,
        from_title: String,
        to_doc_id: String,
        to_title: String,
        relation_type: String,
        strength: f32,
        rationale: String,
    },
    LinkSuggestionResolved { suggestion_id: String, accepted: bool },
    /// Toggle a frontend UI panel. `name` is one of:
    /// "pii_dashboard", "models", "inbox", "browser", "settings".
    OpenPanel { name: String },
    /// A chat turn handed to the agent loop has ended (spec ch. 08, *Action Gravity*:
    /// a chat turn always ends with an explicit outcome). Emitted once per chat turn,
    /// in place of `ActionExecuted`.
    ChatTurnEnded { outcome: ChatOutcome },
}

/// How a chat turn ended: the writes that ran, and how the turn itself closed.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatOutcome {
    pub writes: Vec<WriteResult>,
    pub end: ChatEnd,
}

/// How the turn closed, apart from its writes.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatEnd {
    /// The assistant gave its answer.
    Completed,
    /// Generation failed; the reason is shown to the user.
    GenerationFailed(String),
    /// The user chose to stop the turn on flagged content in tool output.
    StoppedForInjection,
    /// The turn reached the round limit without a final answer. Not a failure: the
    /// assistant stopped after `steps` tool steps, and the trace of the turn was saved
    /// (or could not be) — spec ch. 08, *Chat agent loop*.
    RoundLimit { steps: u8, trace: TraceLink },
}

/// Where the trace of a turn that reached the round limit went.
#[derive(Debug, Clone, PartialEq)]
pub enum TraceLink {
    /// Saved as a Sovereign document in the thread "Assistant traces". The outcome names
    /// the thread and this exact `title`; nothing opens by itself (spec v2.0.39).
    Saved { doc_id: String, title: String },
    /// Could not be saved; the reason is shown and no document is named.
    NotSaved { reason: String },
}

/// One write tool the turn ran, or proposed, and what came of it.
#[derive(Debug, Clone, PartialEq)]
pub struct WriteResult {
    pub tool: String,
    pub outcome: WriteOutcome,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WriteOutcome {
    Succeeded,
    Failed(String),
    /// Proposed to the user and declined (or not confirmed in time).
    Declined,
}

impl ChatOutcome {
    /// The one line every UI shows for the turn. Never empty; says "Done" only of a
    /// write that succeeded, names every write, and gives each failure's reason.
    pub fn summary_line(&self) -> String {
        let mut parts: Vec<String> = self
            .writes
            .iter()
            .map(|w| match &w.outcome {
                WriteOutcome::Succeeded => format!("✓ Done: {}", w.tool),
                WriteOutcome::Failed(reason) => format!("✗ Failed: {} ({reason})", w.tool),
                WriteOutcome::Declined => format!("Declined: {}", w.tool),
            })
            .collect();
        match &self.end {
            ChatEnd::Completed => {
                if parts.is_empty() {
                    parts.push("Answered (no changes made)".to_string());
                }
            }
            ChatEnd::GenerationFailed(reason) => {
                parts.push(format!("✗ Reply failed: {reason}"));
            }
            ChatEnd::StoppedForInjection => {
                parts.push("Stopped: flagged content in tool output".to_string());
            }
            ChatEnd::RoundLimit { steps, trace } => {
                parts.push(format!(
                    "Stopped after {steps} steps without a final answer — rephrase or split the request"
                ));
                parts.push(match trace {
                    TraceLink::Saved { title, .. } => {
                        format!("Trace saved in \"Assistant traces\": {title}")
                    }
                    TraceLink::NotSaved { reason } => format!("Trace not saved: {reason}"),
                });
            }
        }
        parts.join(" · ")
    }

    /// The document holding the trace of this turn, when the turn reached the round
    /// limit and the trace was saved. `None` for every other turn end.
    pub fn trace_doc_id(&self) -> Option<&str> {
        match &self.end {
            ChatEnd::RoundLimit { trace: TraceLink::Saved { doc_id, .. }, .. } => Some(doc_id),
            _ => None,
        }
    }

    /// The exact title of that saved trace, under the same condition as
    /// [`Self::trace_doc_id`].
    pub fn trace_title(&self) -> Option<&str> {
        match &self.end {
            ChatEnd::RoundLimit { trace: TraceLink::Saved { title, .. }, .. } => Some(title),
            _ => None,
        }
    }
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

/// Events from the voice pipeline to the UI.
#[derive(Debug, Clone)]
pub enum VoiceEvent {
    WakeWordDetected,
    ListeningStarted,
    TranscriptionReady(String),
    ListeningStopped,
    TtsSpeaking(String),
    TtsDone,
}

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

/// Backend for loading and running LLM inference.
#[async_trait]
pub trait ModelBackend: Send + Sync {
    async fn load(&mut self, model_path: &str, n_gpu_layers: i32) -> anyhow::Result<()>;
    async fn generate(&self, prompt: &str, max_tokens: u32) -> anyhow::Result<String>;
    async fn unload(&mut self) -> anyhow::Result<()>;
}

#[cfg(test)]
#[path = "chat_outcome_tests.rs"]
mod chat_outcome_tests;

#[cfg(test)]
#[path = "chat_round_limit_tests.rs"]
mod chat_round_limit_tests;
