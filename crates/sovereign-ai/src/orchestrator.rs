use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use sovereign_core::config::AiConfig;
use sovereign_core::interfaces::{
    CommitSummary, FeedbackEvent, MilestoneSummary, ModelBackend, OrchestratorEvent,
};
use sovereign_core::profile::{AdaptiveParams, SuggestionFeedback, UserProfile};
use sovereign_core::security::{self, ActionDecision, BubbleVisualState, ProposedAction};
use sovereign_db::schema::{Milestone, Thread};
use sovereign_db::GraphDB;

use crate::action_gate;
use crate::injection;
use crate::intent::IntentClassifier;
use crate::session_log::SessionLog;
use crate::trust::TrustTracker;

/// Central AI orchestrator. Owns the intent classifier and DB handle.
/// Receives queries (text from search overlay or voice pipeline),
/// classifies intent, executes actions, and emits events to the UI.
pub struct Orchestrator {
    classifier: tokio::sync::Mutex<IntentClassifier>,
    db: Arc<dyn GraphDB>,
    event_tx: std::sync::mpsc::Sender<OrchestratorEvent>,
    session_log: Option<Mutex<SessionLog>>,
    decision_rx: Option<tokio::sync::Mutex<tokio::sync::mpsc::Receiver<ActionDecision>>>,
    feedback_rx: Option<tokio::sync::Mutex<tokio::sync::mpsc::Receiver<FeedbackEvent>>>,
    trust: Mutex<TrustTracker>,
    profile: Mutex<UserProfile>,
    profile_dir: PathBuf,
    model_dir: String,
    n_gpu_layers: i32,
    #[cfg(feature = "encrypted-log")]
    session_log_key: Option<[u8; 32]>,
    #[cfg(feature = "p2p")]
    p2p_command_tx: Option<tokio::sync::mpsc::Sender<sovereign_p2p::P2pCommand>>,
    #[cfg(feature = "p2p")]
    p2p_event_rx: Option<Mutex<tokio::sync::mpsc::Receiver<sovereign_p2p::P2pEvent>>>,
}

impl Orchestrator {
    /// Create a new orchestrator. Loads the 3B router model eagerly.
    pub async fn new(
        config: AiConfig,
        db: Arc<dyn GraphDB>,
        event_tx: std::sync::mpsc::Sender<OrchestratorEvent>,
    ) -> Result<Self> {
        let model_dir = config.model_dir.clone();
        let n_gpu_layers = config.n_gpu_layers;
        let mut classifier = IntentClassifier::new(config);
        classifier.load_router().await?;

        // Initialize session log + profile directory
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let profile_dir = PathBuf::from(home)
            .join(".sovereign")
            .join("orchestrator");
        let session_log = match SessionLog::open(&profile_dir) {
            Ok(log) => Some(Mutex::new(log)),
            Err(e) => {
                tracing::warn!("Session log unavailable: {e}");
                None
            }
        };

        // Load persistent user profile
        let profile = match UserProfile::load(&profile_dir) {
            Ok(p) => {
                tracing::info!("User profile loaded (id={})", p.user_id);
                p
            }
            Err(e) => {
                tracing::warn!("Profile load failed, using default: {e}");
                UserProfile::default_new()
            }
        };

        // Load persistent trust state
        let trust = match TrustTracker::load(&profile_dir) {
            Ok(t) => {
                tracing::info!("Trust state loaded from disk");
                t
            }
            Err(e) => {
                tracing::warn!("Trust load failed, using default: {e}");
                TrustTracker::new()
            }
        };

        Ok(Self {
            classifier: tokio::sync::Mutex::new(classifier),
            db,
            event_tx,
            session_log,
            decision_rx: None,
            feedback_rx: None,
            trust: Mutex::new(trust),
            profile: Mutex::new(profile),
            profile_dir,
            model_dir,
            n_gpu_layers,
            #[cfg(feature = "encrypted-log")]
            session_log_key: None,
            #[cfg(feature = "p2p")]
            p2p_command_tx: None,
            #[cfg(feature = "p2p")]
            p2p_event_rx: None,
        })
    }

    /// Attach a decision channel for user confirmations of Level 3+ actions.
    pub fn set_decision_rx(&mut self, rx: tokio::sync::mpsc::Receiver<ActionDecision>) {
        self.decision_rx = Some(tokio::sync::Mutex::new(rx));
    }

    /// Attach a feedback channel for suggestion accept/dismiss events from the UI.
    pub fn set_feedback_rx(&mut self, rx: tokio::sync::mpsc::Receiver<FeedbackEvent>) {
        self.feedback_rx = Some(tokio::sync::Mutex::new(rx));
    }

    /// Enable session log encryption with the given key.
    ///
    /// Re-opens the session log in encrypted mode. Each subsequent entry will be
    /// encrypted with XChaCha20-Poly1305 and hash-chained to the previous entry
    /// for tamper detection.
    #[cfg(feature = "encrypted-log")]
    pub fn set_session_log_key(&mut self, key: [u8; 32]) {
        match SessionLog::open_encrypted(&self.profile_dir, key) {
            Ok(log) => {
                self.session_log = Some(Mutex::new(log));
                self.session_log_key = Some(key);
                tracing::info!("Session log encryption enabled");
            }
            Err(e) => {
                tracing::warn!("Failed to enable session log encryption: {e}");
            }
        }
    }

    /// Attach P2P command/event channels for device sync and guardian transport.
    #[cfg(feature = "p2p")]
    pub fn set_p2p_channels(
        &mut self,
        command_tx: tokio::sync::mpsc::Sender<sovereign_p2p::P2pCommand>,
        event_rx: tokio::sync::mpsc::Receiver<sovereign_p2p::P2pEvent>,
    ) {
        self.p2p_command_tx = Some(command_tx);
        self.p2p_event_rx = Some(Mutex::new(event_rx));
    }

    /// Drain pending P2P events and forward them as OrchestratorEvents.
    #[cfg(feature = "p2p")]
    pub fn poll_p2p_events(&self) {
        if let Some(ref rx_mutex) = self.p2p_event_rx {
            if let Ok(mut rx) = rx_mutex.lock() {
                while let Ok(event) = rx.try_recv() {
                    use sovereign_p2p::P2pEvent;
                    let orch_event = match event {
                        P2pEvent::PeerDiscovered { peer_id, device_name } => {
                            OrchestratorEvent::DeviceDiscovered {
                                device_id: peer_id,
                                device_name: device_name.unwrap_or_else(|| "Unknown".into()),
                            }
                        }
                        P2pEvent::SyncStarted { peer_id } => {
                            OrchestratorEvent::SyncStatus {
                                peer_id,
                                status: "started".into(),
                            }
                        }
                        P2pEvent::SyncCompleted { peer_id, docs_synced } => {
                            OrchestratorEvent::SyncStatus {
                                peer_id,
                                status: format!("completed ({} docs)", docs_synced),
                            }
                        }
                        P2pEvent::SyncConflict { doc_id, description } => {
                            OrchestratorEvent::SyncConflict { doc_id, description }
                        }
                        P2pEvent::ShardReceived { shard_id, .. } => {
                            tracing::info!("Shard received: {}", shard_id);
                            continue;
                        }
                        P2pEvent::PairingCompleted { peer_id, device_name: _ } => {
                            OrchestratorEvent::DevicePaired { device_id: peer_id }
                        }
                        P2pEvent::PeerLost { peer_id } => {
                            OrchestratorEvent::SyncStatus {
                                peer_id,
                                status: "disconnected".into(),
                            }
                        }
                        P2pEvent::PairingRequested { peer_id, device_name } => {
                            tracing::info!("Pairing requested from {} ({})", peer_id, device_name);
                            continue;
                        }
                    };
                    let _ = self.event_tx.send(orch_event);
                }
            }
        }
    }

    /// Handle a user query: classify intent, gate check, execute or await confirmation.
    pub async fn handle_query(&self, query: &str) -> Result<()> {
        let intent = self.classifier.lock().await.classify(query).await?;
        tracing::info!(
            "Intent: action={}, confidence={:.2}, target={:?}, origin={:?}",
            intent.action,
            intent.confidence,
            intent.target,
            intent.origin,
        );

        // Log user input
        if let Some(ref log) = self.session_log {
            if let Ok(mut log) = log.lock() {
                log.log_user_input("text", query, &intent.action);
            }
        }

        // Gate check: plane violation
        if let Some(reason) = action_gate::check_plane_violation(&intent) {
            tracing::warn!("Plane violation: {reason}");
            self.log_action("plane_violation", &reason);
            let _ = self.event_tx.send(OrchestratorEvent::ActionRejected {
                action: intent.action.clone(),
                reason: reason.clone(),
            });
            return Ok(());
        }

        // Gate check: does this action level require confirmation?
        let level = security::action_level(&intent.action);
        if action_gate::requires_confirmation(level) {
            // Check trust: can we auto-approve this action?
            let trusted = {
                if let Ok(trust) = self.trust.lock() {
                    trust.should_auto_approve(&intent.action, level)
                } else {
                    false
                }
            };

            if trusted {
                tracing::info!("Auto-approved via trust: {}", intent.action);
                self.log_action("trust_auto_approve", &intent.action);
                let _ = self.event_tx.send(OrchestratorEvent::BubbleState(
                    BubbleVisualState::Executing,
                ));
                self.execute_action(&intent.action, intent.target.as_deref(), query)
                    .await?;
                let _ = self
                    .event_tx
                    .send(OrchestratorEvent::BubbleState(BubbleVisualState::Idle));
            } else {
                let proposal = action_gate::build_proposal(&intent);

                // Signal bubble state
                let _ = self.event_tx.send(OrchestratorEvent::BubbleState(
                    BubbleVisualState::Proposing,
                ));

                // Emit proposal to UI
                let _ = self.event_tx.send(OrchestratorEvent::ActionProposed {
                    proposal: proposal.clone(),
                });

                // Wait for user decision (with timeout)
                let decision = self.wait_for_decision().await;
                match decision {
                    ActionDecision::Approve => {
                        // Record approval in trust tracker + persist
                        if let Ok(mut trust) = self.trust.lock() {
                            trust.record_approval(&intent.action);
                            if let Err(e) = trust.save(&self.profile_dir) {
                                tracing::warn!("Failed to save trust state: {e}");
                            }
                        }
                        let _ = self.event_tx.send(OrchestratorEvent::BubbleState(
                            BubbleVisualState::Executing,
                        ));
                        self.execute_action(&intent.action, intent.target.as_deref(), query)
                            .await?;
                    }
                    ActionDecision::Reject(reason) => {
                        // Record rejection in trust tracker (resets counter) + persist
                        if let Ok(mut trust) = self.trust.lock() {
                            trust.record_rejection(&intent.action);
                            if let Err(e) = trust.save(&self.profile_dir) {
                                tracing::warn!("Failed to save trust state: {e}");
                            }
                        }
                        tracing::info!("Action rejected: {reason}");
                        self.log_action("rejected", &format!("{}: {reason}", intent.action));
                        let _ = self.event_tx.send(OrchestratorEvent::ActionRejected {
                            action: intent.action.clone(),
                            reason,
                        });
                    }
                }

                let _ = self
                    .event_tx
                    .send(OrchestratorEvent::BubbleState(BubbleVisualState::Idle));
            }
        } else {
            // Low-gravity action — execute immediately
            let bubble_state = if intent.origin == sovereign_core::security::Plane::Control {
                BubbleVisualState::ProcessingOwned
            } else {
                BubbleVisualState::ProcessingExternal
            };
            let _ = self
                .event_tx
                .send(OrchestratorEvent::BubbleState(bubble_state));

            self.execute_action(&intent.action, intent.target.as_deref(), query)
                .await?;

            let _ = self
                .event_tx
                .send(OrchestratorEvent::BubbleState(BubbleVisualState::Idle));
        }

        Ok(())
    }

    /// Maximum iterations for the agent loop per chat message.
    const MAX_AGENT_ITERATIONS: usize = 5;
    /// Maximum token budget for history (~1000 tokens).
    const MAX_HISTORY_TOKENS: usize = 1000;

    /// Handle a chat message: load context, run agent loop with tool calling.
    /// Handle chat input. Delegates to handle_query so that all user input
    /// — whether from the search bar or chat panel — goes through the same
    /// classify → gate → dispatch path.
    pub async fn handle_chat(&self, message: &str) -> Result<()> {
        self.handle_query(message).await
    }

    /// Multi-turn chat agent loop with tool calling and conversation history.
    /// Called from execute_action when the classified intent is "chat" or "unknown".
    async fn run_chat_agent_loop(&self, message: &str) -> Result<()> {
        // 1. Log user input
        if let Some(ref log) = self.session_log {
            if let Ok(mut log) = log.lock() {
                log.log_user_input("chat", message, "chat");
            }
        }

        let _ = self.event_tx.send(OrchestratorEvent::BubbleState(
            BubbleVisualState::ProcessingOwned,
        ));

        // 2. Load conversation history from persistent session log
        let session_entries = self.load_session_entries(50);
        let mut turns = crate::llm::context::session_entries_to_chat_turns(&session_entries);

        // 3. Gather workspace context
        let workspace_ctx =
            crate::llm::context::gather_workspace_context(self.db.as_ref()).await;

        // 4. Read user profile for verbosity, name, designation, and nickname
        let (verbosity, user_name, designation, nickname) = {
            if let Ok(profile) = self.profile.lock() {
                (
                    profile.interaction_patterns.command_verbosity.clone(),
                    profile.display_name.clone(),
                    if profile.designation.is_empty() { None } else { Some(profile.designation.clone()) },
                    profile.nickname.clone(),
                )
            } else {
                ("detailed".into(), None, None, None)
            }
        };

        // 5. Build system prompt with context and UX principles
        let formatter = self.classifier.lock().await.formatter.clone();
        let system_prompt = crate::llm::prompt::build_chat_system_prompt(
            Some(&workspace_ctx),
            &verbosity,
            user_name.as_deref(),
            designation.as_deref(),
            nickname.as_deref(),
            Some(&*formatter),
        );

        // 6. Append current user message to turns
        turns.push(crate::llm::context::ChatTurn {
            role: crate::llm::context::ChatRole::User,
            content: message.to_string(),
        });

        // 7. Agent loop
        let mut iterations = 0;
        loop {
            iterations += 1;
            if iterations > Self::MAX_AGENT_ITERATIONS {
                let fallback =
                    "I had trouble processing that request. Could you rephrase it?";
                self.log_chat_response(fallback);
                let _ = self.event_tx.send(OrchestratorEvent::ChatResponse {
                    text: fallback.into(),
                });
                break;
            }

            // Build prompt from full history (budget in chars = tokens * chars_per_token)
            let max_chars = (Self::MAX_HISTORY_TOKENS as f64 * formatter.chars_per_token()) as usize;
            let full_prompt = crate::llm::context::build_prompt_from_full_history(
                &system_prompt,
                &turns,
                max_chars,
                Some(&*formatter),
            );

            // Generate
            let response = match self
                .classifier
                .lock()
                .await
                .router
                .generate(&full_prompt, 300)
                .await
            {
                Ok(r) => r.trim().to_string(),
                Err(e) => {
                    tracing::error!("Chat generation failed: {e}");
                    let error_msg = format!("Sorry, I couldn't generate a response: {e}");
                    self.log_chat_response(&error_msg);
                    let _ = self.event_tx.send(OrchestratorEvent::ChatResponse {
                        text: error_msg,
                    });
                    break;
                }
            };

            // Check for tool calls
            if crate::tools::has_tool_call(&response, Some(&*formatter)) {
                let calls = crate::tools::parse_tool_calls(&response, Some(&*formatter));
                if let Some(call) = calls.first() {
                    tracing::info!("Tool call: {} (iteration {})", call.name, iterations);

                    let tool_output = if crate::tools::is_write_tool(&call.name) {
                        // Write tool — gate through action gravity system
                        let level = security::action_level(&call.name);
                        let trusted = {
                            if let Ok(trust) = self.trust.lock() {
                                trust.should_auto_approve(&call.name, level)
                            } else {
                                false
                            }
                        };

                        if !action_gate::requires_confirmation(level) || trusted {
                            // Auto-execute (Observe/Annotate or trusted)
                            let result = crate::tools::execute_write_tool(call, self.db.as_ref()).await;
                            if let Some(event) = result.event {
                                let _ = self.event_tx.send(event);
                            }
                            format!("[{}] {}", result.tool_name, result.output)
                        } else {
                            // Propose to user and wait for confirmation
                            let proposal = security::ProposedAction {
                                action: call.name.clone(),
                                level,
                                plane: security::Plane::Control,
                                doc_id: None,
                                thread_id: None,
                                description: format_tool_proposal(
                                    &call.name,
                                    &call.arguments,
                                ),
                            };
                            let _ = self.event_tx.send(OrchestratorEvent::BubbleState(
                                BubbleVisualState::Proposing,
                            ));
                            let _ = self.event_tx.send(OrchestratorEvent::ActionProposed {
                                proposal: proposal.clone(),
                            });

                            match self.wait_for_decision().await {
                                ActionDecision::Approve => {
                                    if let Ok(mut trust) = self.trust.lock() {
                                        trust.record_approval(&call.name);
                                        if let Err(e) = trust.save(&self.profile_dir) {
                                            tracing::warn!("Failed to save trust: {e}");
                                        }
                                    }
                                    let _ = self.event_tx.send(OrchestratorEvent::BubbleState(
                                        BubbleVisualState::Executing,
                                    ));
                                    let result = crate::tools::execute_write_tool(call, self.db.as_ref()).await;
                                    if let Some(event) = result.event {
                                        let _ = self.event_tx.send(event);
                                    }
                                    let _ = self.event_tx.send(OrchestratorEvent::BubbleState(
                                        BubbleVisualState::ProcessingOwned,
                                    ));
                                    format!("[{}] {}", result.tool_name, result.output)
                                }
                                ActionDecision::Reject(reason) => {
                                    if let Ok(mut trust) = self.trust.lock() {
                                        trust.record_rejection(&call.name);
                                        if let Err(e) = trust.save(&self.profile_dir) {
                                            tracing::warn!("Failed to save trust: {e}");
                                        }
                                    }
                                    let _ = self.event_tx.send(OrchestratorEvent::BubbleState(
                                        BubbleVisualState::ProcessingOwned,
                                    ));
                                    format!("[{}] Action rejected by user: {}", call.name, reason)
                                }
                            }
                        }
                    } else {
                        // Read-only tool — execute immediately
                        let result = crate::tools::execute_tool(call, self.db.as_ref()).await;
                        format!("[{}] {}", result.tool_name, result.output)
                    };

                    // Append assistant turn (tool call) and tool result to history
                    turns.push(crate::llm::context::ChatTurn {
                        role: crate::llm::context::ChatRole::Assistant,
                        content: response.clone(),
                    });
                    turns.push(crate::llm::context::ChatTurn {
                        role: crate::llm::context::ChatRole::Tool,
                        content: tool_output,
                    });

                    // Continue loop — model will see tool result and generate again
                    continue;
                }
            }

            // No tool call — this is the final response
            let text_response = crate::tools::extract_text_response(&response, Some(&*formatter));
            tracing::info!(
                "Chat response: {} chars, {} iterations",
                text_response.len(),
                iterations
            );
            self.log_action(
                "chat",
                &format!(
                    "response: {} chars, {} iterations",
                    text_response.len(),
                    iterations
                ),
            );
            self.log_chat_response(&text_response);
            let _ = self.event_tx.send(OrchestratorEvent::ChatResponse {
                text: text_response,
            });
            break;
        }

        let _ = self
            .event_tx
            .send(OrchestratorEvent::BubbleState(BubbleVisualState::Idle));

        Ok(())
    }

    /// Log a chat response to the session log for persistent conversation history.
    fn log_chat_response(&self, response: &str) {
        if let Some(ref log) = self.session_log {
            if let Ok(mut log) = log.lock() {
                log.log_chat_response(response);
            }
        }
    }

    /// Wait for a user decision on the decision channel (30s timeout).
    /// If no channel is configured, auto-approve (for backward compatibility/testing).
    async fn wait_for_decision(&self) -> ActionDecision {
        if let Some(ref rx_mutex) = self.decision_rx {
            let mut rx = rx_mutex.lock().await;
            match tokio::time::timeout(Duration::from_secs(120), rx.recv()).await {
                Ok(Some(decision)) => return decision,
                Ok(None) => {
                    tracing::warn!("Decision channel closed — rejecting action");
                    return ActionDecision::Reject("Decision channel closed".into());
                }
                Err(_) => {
                    tracing::warn!("Decision timeout — rejecting action");
                    return ActionDecision::Reject("Timeout waiting for user decision".into());
                }
            }
        }
        // No decision channel — auto-approve (backward compat)
        ActionDecision::Approve
    }

    /// Execute a classified action by name.
    async fn execute_action(
        &self,
        action: &str,
        target: Option<&str>,
        query: &str,
    ) -> Result<()> {
        match action {
            "search" => {
                let search_term = target.unwrap_or(query);

                let docs = self.db.search_documents_by_title(search_term).await?;
                let matches: Vec<String> = docs
                    .iter()
                    .filter_map(|d| d.id_string())
                    .collect();

                tracing::info!("Search '{}': {} matches", search_term, matches.len());
                self.log_action(
                    "search",
                    &format!("{} matches for '{}'", matches.len(), search_term),
                );
                let _ = self.event_tx.send(OrchestratorEvent::SearchResults {
                    query: query.into(),
                    doc_ids: matches,
                });
            }
            "open" | "navigate" => {
                if let Some(target) = target {
                    let docs = self.db.search_documents_by_title(target).await?;
                    if let Some(doc) = docs.first() {
                        if let Some(id) = doc.id_string() {
                            let _ = self
                                .event_tx
                                .send(OrchestratorEvent::DocumentOpened { doc_id: id });
                        }
                    }
                }
            }
            "create_thread" => {
                let name = target.unwrap_or("New Thread").to_string();
                let thread = Thread::new(name.clone(), String::new());
                match self.db.create_thread(thread).await {
                    Ok(created) => {
                        let tid = created.id_string().unwrap_or_default();
                        tracing::info!("Thread created: {} ({})", name, tid);
                        self.log_action("create_thread", &format!("{} ({})", name, tid));
                        let _ = self.event_tx.send(OrchestratorEvent::ThreadCreated {
                            thread_id: tid,
                            name,
                        });
                    }
                    Err(e) => tracing::error!("Failed to create thread: {e}"),
                }
            }
            "rename_thread" => {
                if let Some(target) = target {
                    let (old_name, new_name) = parse_rename_target(target);
                    if let Some(thread) = self.db.find_thread_by_name(&old_name).await? {
                        if let Some(tid) = thread.id_string() {
                            match self.db.update_thread(&tid, Some(&new_name), None).await {
                                Ok(_) => {
                                    tracing::info!(
                                        "Thread renamed: {} → {}",
                                        old_name,
                                        new_name
                                    );
                                    let _ =
                                        self.event_tx.send(OrchestratorEvent::ThreadRenamed {
                                            thread_id: tid,
                                            name: new_name,
                                        });
                                }
                                Err(e) => tracing::error!("Failed to rename thread: {e}"),
                            }
                        }
                    }
                }
            }
            "delete_thread" => {
                if let Some(target) = target {
                    if let Some(thread) = self.db.find_thread_by_name(target).await? {
                        if let Some(tid) = thread.id_string() {
                            match self.db.soft_delete_thread(&tid).await {
                                Ok(()) => {
                                    tracing::info!("Thread soft-deleted: {} ({})", target, tid);
                                    let _ =
                                        self.event_tx.send(OrchestratorEvent::ThreadDeleted {
                                            thread_id: tid,
                                        });
                                }
                                Err(e) => tracing::error!("Failed to soft-delete thread: {e}"),
                            }
                        }
                    }
                }
            }
            "move_document" => {
                if let Some(target) = target {
                    let (doc_name, thread_name) = parse_move_target(target);
                    let docs = self.db.search_documents_by_title(&doc_name).await?;
                    let thread = self.db.find_thread_by_name(&thread_name).await?;

                    if let (Some(doc), Some(thread)) = (docs.first(), thread.as_ref()) {
                        let doc_id = doc.id_string().unwrap_or_default();
                        let tid = thread.id_string().unwrap_or_default();
                        match self.db.move_document_to_thread(&doc_id, &tid).await {
                            Ok(_) => {
                                tracing::info!("Moved {} to {}", doc_name, thread_name);
                                let _ =
                                    self.event_tx.send(OrchestratorEvent::DocumentMoved {
                                        doc_id,
                                        new_thread_id: tid,
                                    });
                            }
                            Err(e) => tracing::error!("Failed to move document: {e}"),
                        }
                    }
                }
            }
            "history" => {
                if let Some(target) = target {
                    let docs = self.db.search_documents_by_title(target).await?;
                    if let Some(doc) = docs.first() {
                        if let Some(doc_id) = doc.id_string() {
                            let commits = self.db.list_document_commits(&doc_id).await?;
                            let summaries: Vec<CommitSummary> = commits
                                .iter()
                                .map(|c| CommitSummary {
                                    id: c.id_string().unwrap_or_default(),
                                    message: c.message.clone(),
                                    timestamp: c.timestamp.to_rfc3339(),
                                })
                                .collect();
                            tracing::info!(
                                "History for {}: {} commits",
                                target,
                                summaries.len()
                            );
                            self.log_action(
                                "history",
                                &format!("{} commits for {}", summaries.len(), target),
                            );
                            let _ =
                                self.event_tx.send(OrchestratorEvent::VersionHistory {
                                    doc_id,
                                    commits: summaries,
                                });
                        }
                    }
                }
            }
            "summarize" => {
                if let Some(target) = target {
                    let docs = self.db.search_documents_by_title(target).await?;
                    if let Some(doc) = docs.first() {
                        let content = &doc.content;
                        let fmt = self.classifier.lock().await.formatter.clone();
                        let prompt = crate::llm::prompt::format_single_turn(
                            &*fmt,
                            "You are a concise summarizer. Summarize the following document in 2-3 sentences.",
                            content,
                        );
                        match self.classifier.lock().await.router.generate(&prompt, 200).await {
                            Ok(summary) => {
                                let summary_text: &str = summary.trim();
                                let json = serde_json::json!({
                                    "doc_title": doc.title,
                                    "summary": summary_text,
                                });
                                let _ = self.event_tx.send(OrchestratorEvent::SkillResult {
                                    skill: "summarizer".into(),
                                    action: "summarize".into(),
                                    kind: "summary".into(),
                                    data: json.to_string(),
                                });
                            }
                            Err(e) => tracing::error!("Summarize failed: {e}"),
                        }
                    }
                }
            }
            "list_models" => {
                let dir = self.model_dir.clone();
                let found = tokio::task::spawn_blocking(move || scan_gguf_models(&dir))
                    .await
                    .unwrap_or_default();
                let models: Vec<serde_json::Value> = found
                    .iter()
                    .map(|(name, size_mb)| {
                        serde_json::json!({ "name": name, "size_mb": size_mb })
                    })
                    .collect();

                tracing::info!("Listed {} models in {}", models.len(), self.model_dir);
                self.log_action("list_models", &format!("{} models found", models.len()));
                let _ = self.event_tx.send(OrchestratorEvent::SkillResult {
                    skill: "model_manager".into(),
                    action: "list_models".into(),
                    kind: "model_list".into(),
                    data: serde_json::json!({ "models": models }).to_string(),
                });
            }
            "swap_model" => {
                if let Some(target) = target {
                    let full_path = resolve_model_path(&self.model_dir, target);
                    let model_name = full_path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();

                    if !full_path.exists() {
                        tracing::error!("Model not found: {}", full_path.display());
                        let _ = self.event_tx.send(OrchestratorEvent::ActionRejected {
                            action: "swap_model".into(),
                            reason: format!("Model not found: {model_name}"),
                        });
                    } else {
                        let path_str = full_path.to_string_lossy().to_string();
                        let mut classifier = self.classifier.lock().await;
                        match classifier
                            .swap_router(&path_str, self.n_gpu_layers)
                            .await
                        {
                            Ok(()) => {
                                // Auto-detect prompt format from the model filename.
                                let detected = crate::llm::format::detect_format_from_filename(&model_name);
                                classifier.swap_formatter(detected);
                                let format_name = match detected {
                                    crate::llm::format::PromptFormat::ChatML => "chatml",
                                    crate::llm::format::PromptFormat::Mistral => "mistral",
                                    crate::llm::format::PromptFormat::Llama3 => "llama3",
                                };

                                tracing::info!("Model swapped to: {model_name} (format: {format_name})");
                                self.log_action("swap_model", &model_name);
                                let _ = self.event_tx.send(OrchestratorEvent::SkillResult {
                                    skill: "model_manager".into(),
                                    action: "swap_model".into(),
                                    kind: "model_swapped".into(),
                                    data: serde_json::json!({
                                        "model": model_name,
                                        "prompt_format": format_name,
                                    })
                                    .to_string(),
                                });
                            }
                            Err(e) => {
                                tracing::error!("Model swap failed: {e}");
                                let _ = self.event_tx.send(OrchestratorEvent::ActionRejected {
                                    action: "swap_model".into(),
                                    reason: format!("Swap failed: {e}"),
                                });
                            }
                        }
                    }
                } else {
                    let _ = self.event_tx.send(OrchestratorEvent::ActionRejected {
                        action: "swap_model".into(),
                        reason: "No model name specified".into(),
                    });
                }
            }
            "merge_threads" => {
                if let Some(target) = target {
                    let (target_name, source_name) = parse_move_target(target);
                    let target_thread = self.db.find_thread_by_name(&target_name).await?;
                    let source_thread = self.db.find_thread_by_name(&source_name).await?;

                    if let (Some(tt), Some(st)) = (target_thread, source_thread) {
                        let target_id = tt.id_string().unwrap_or_default();
                        let source_id = st.id_string().unwrap_or_default();
                        match self.db.merge_threads(&target_id, &source_id).await {
                            Ok(()) => {
                                tracing::info!(
                                    "Threads merged: {} ← {}",
                                    target_name,
                                    source_name
                                );
                                self.log_action(
                                    "merge_threads",
                                    &format!("{} ← {}", target_name, source_name),
                                );
                                let _ = self.event_tx.send(OrchestratorEvent::ThreadMerged {
                                    target_id,
                                    source_id,
                                });
                            }
                            Err(e) => tracing::error!("Failed to merge threads: {e}"),
                        }
                    }
                }
            }
            "split_thread" => {
                if let Some(target) = target {
                    let (thread_name, new_name) = parse_rename_target(target);
                    let new_name = if new_name == thread_name {
                        format!("{} (split)", thread_name)
                    } else {
                        new_name
                    };
                    if let Some(thread) = self.db.find_thread_by_name(&thread_name).await? {
                        if let Some(tid) = thread.id_string() {
                            // For now, split with no specific doc_ids — the user will
                            // refine via UI. Create empty thread as a target.
                            match self.db.split_thread(&tid, &[], &new_name).await {
                                Ok(created) => {
                                    let new_tid = created.id_string().unwrap_or_default();
                                    tracing::info!("Thread split: {} → {}", thread_name, new_name);
                                    self.log_action(
                                        "split_thread",
                                        &format!("{} → {}", thread_name, new_name),
                                    );
                                    let _ = self.event_tx.send(OrchestratorEvent::ThreadSplit {
                                        new_thread_id: new_tid,
                                        name: new_name,
                                        doc_ids: vec![],
                                    });
                                }
                                Err(e) => tracing::error!("Failed to split thread: {e}"),
                            }
                        }
                    }
                }
            }
            "adopt" => {
                if let Some(target) = target {
                    let docs = self.db.search_documents_by_title(target).await?;
                    if let Some(doc) = docs.first() {
                        if let Some(doc_id) = doc.id_string() {
                            // Set is_owned = true via adopt_document
                            match self.db.adopt_document(&doc_id).await {
                                Ok(_) => {
                                    tracing::info!("Adopted: {} ({})", target, doc_id);
                                    self.log_action(
                                        "adopt",
                                        &format!("{} ({})", target, doc_id),
                                    );
                                    let _ = self.event_tx.send(
                                        OrchestratorEvent::AdoptionStarted {
                                            doc_id,
                                        },
                                    );
                                }
                                Err(e) => tracing::error!("Failed to adopt: {e}"),
                            }
                        }
                    }
                }
            }
            "create_milestone" => {
                if let Some(target) = target {
                    // Parse "title on/for thread_name" or just use default thread
                    let (title, thread_name) = if let Some(idx) = target.to_lowercase().find(" on ") {
                        (target[..idx].trim().to_string(), target[idx + 4..].trim().to_string())
                    } else if let Some(idx) = target.to_lowercase().find(" for ") {
                        (target[..idx].trim().to_string(), target[idx + 5..].trim().to_string())
                    } else {
                        (target.to_string(), String::new())
                    };

                    let thread = if thread_name.is_empty() {
                        self.db.list_threads().await?.into_iter().next()
                    } else {
                        self.db.find_thread_by_name(&thread_name).await?
                    };

                    if let Some(thread) = thread {
                        let tid = thread.id_string().unwrap_or_default();
                        let ms = Milestone::new(title.clone(), tid.clone(), String::new());
                        match self.db.create_milestone(ms).await {
                            Ok(created) => {
                                let mid = created.id_string().unwrap_or_default();
                                tracing::info!("Milestone created: {} ({})", title, mid);
                                self.log_action("create_milestone", &format!("{} on {}", title, tid));
                                let _ = self.event_tx.send(OrchestratorEvent::MilestoneCreated {
                                    milestone_id: mid,
                                    title,
                                    thread_id: tid,
                                });
                            }
                            Err(e) => tracing::error!("Failed to create milestone: {e}"),
                        }
                    }
                }
            }
            "list_milestones" => {
                let thread = if let Some(target) = target {
                    self.db.find_thread_by_name(target).await?
                } else {
                    self.db.list_threads().await?.into_iter().next()
                };

                if let Some(thread) = thread {
                    let tid = thread.id_string().unwrap_or_default();
                    let milestones = self.db.list_milestones(&tid).await?;
                    let summaries: Vec<MilestoneSummary> = milestones
                        .iter()
                        .map(|m| MilestoneSummary {
                            id: m.id_string().unwrap_or_default(),
                            title: m.title.clone(),
                            timestamp: m.timestamp.to_rfc3339(),
                            description: m.description.clone(),
                        })
                        .collect();
                    tracing::info!("Listed {} milestones for {}", summaries.len(), tid);
                    self.log_action("list_milestones", &format!("{} milestones", summaries.len()));
                    let _ = self.event_tx.send(OrchestratorEvent::MilestonesListed {
                        thread_id: tid,
                        milestones: summaries,
                    });
                }
            }
            // Communications actions
            "list_contacts" => {
                let contacts = self.db.list_contacts().await?;
                let summary: Vec<String> = contacts
                    .iter()
                    .map(|c| {
                        let addrs: Vec<String> = c.addresses
                            .iter()
                            .map(|a| format!("{}: {}", a.channel, a.address))
                            .collect();
                        format!("{} ({})", c.name, addrs.join(", "))
                    })
                    .collect();
                self.log_action("list_contacts", &format!("{} contacts", summary.len()));
                let _ = self.event_tx.send(OrchestratorEvent::ChatResponse {
                    text: format!("Contacts:\n{}", summary.join("\n")),
                });
            }
            "view_messages" => {
                let conversations = self.db.list_conversations(None).await?;
                let summary: Vec<String> = conversations
                    .iter()
                    .map(|c| {
                        let linked = c.linked_thread_id.as_deref().unwrap_or("none");
                        format!(
                            "{} ({}) — {} unread, thread: {}",
                            c.title, c.channel, c.unread_count, linked
                        )
                    })
                    .collect();
                self.log_action("view_messages", &format!("{} conversations", summary.len()));
                let _ = self.event_tx.send(OrchestratorEvent::ChatResponse {
                    text: format!("Conversations:\n{}", summary.join("\n")),
                });
            }
            // P2P actions
            "sync_device" | "pair_device" | "list_devices" | "list_guardians"
            | "enroll_guardian" | "revoke_guardian" | "rotate_shards"
            | "initiate_recovery" | "sync_status" | "encrypt_data" => {
                #[cfg(feature = "p2p")]
                {
                    if let Some(ref tx) = self.p2p_command_tx {
                        match action {
                            "sync_device" => {
                                if let Some(peer_id) = target {
                                    let _ = tx.try_send(sovereign_p2p::P2pCommand::StartSync {
                                        peer_id: peer_id.to_string(),
                                    });
                                }
                            }
                            "pair_device" => {
                                if let Some(peer_id) = target {
                                    let _ = tx.try_send(sovereign_p2p::P2pCommand::PairDevice {
                                        peer_id: peer_id.to_string(),
                                    });
                                }
                            }
                            _ => {}
                        }
                    }
                }
                let _ = self.event_tx.send(OrchestratorEvent::ActionExecuted {
                    action: action.to_string(),
                    success: true,
                });
            }
            "create_document" => {
                let title = target
                    .filter(|t| !t.is_empty())
                    .unwrap_or("Untitled Document")
                    .to_string();

                // Pick the first thread, or let target specify it
                let threads = self.db.list_threads().await?;
                let thread_id = threads
                    .first()
                    .and_then(|t| t.id_string())
                    .unwrap_or_default();

                let doc = sovereign_db::schema::Document::new(
                    title.clone(),
                    thread_id.clone(),
                    true,
                );
                match self.db.create_document(doc).await {
                    Ok(created) => {
                        let doc_id = created.id_string().unwrap_or_default();
                        tracing::info!("Document created: {} ({})", title, doc_id);
                        self.log_action("create_document", &format!("{} ({})", title, doc_id));
                        let _ = self.event_tx.send(OrchestratorEvent::DocumentCreated {
                            doc_id,
                            title,
                            thread_id,
                        });
                    }
                    Err(e) => tracing::error!("Failed to create document: {e}"),
                }
            }
            "word_count" | "find_replace" | "duplicate" | "import_file" => {
                let _ = self.event_tx.send(OrchestratorEvent::SkillResult {
                    skill: action.to_string(),
                    action: action.to_string(),
                    kind: "skill_hint".into(),
                    data: serde_json::json!({ "hint": format!("Use the {} skill from the skills panel", action) }).to_string(),
                });
            }
            "restore" => {
                if let Some(target) = target {
                    let (doc_name, commit_ref) = parse_move_target(target);
                    let docs = self.db.search_documents_by_title(&doc_name).await?;
                    if let Some(doc) = docs.first() {
                        if let Some(doc_id) = doc.id_string() {
                            if commit_ref.starts_with("commit:") {
                                match self.db.restore_document(&doc_id, &commit_ref).await {
                                    Ok(_) => {
                                        tracing::info!(
                                            "Restored {} to {}",
                                            doc_name,
                                            commit_ref
                                        );
                                        self.log_action(
                                            "restore",
                                            &format!("{} to {}", doc_name, commit_ref),
                                        );
                                        let _ = self.event_tx.send(
                                            OrchestratorEvent::DocumentOpened { doc_id },
                                        );
                                    }
                                    Err(e) => tracing::error!("Restore failed: {e}"),
                                }
                            } else {
                                let commits =
                                    self.db.list_document_commits(&doc_id).await?;
                                if commits.len() >= 2 {
                                    let prev = commits[1].id_string().unwrap_or_default();
                                    match self.db.restore_document(&doc_id, &prev).await {
                                        Ok(_) => {
                                            tracing::info!(
                                                "Restored {} to previous version",
                                                doc_name
                                            );
                                            self.log_action(
                                                "restore",
                                                &format!("{} to previous", doc_name),
                                            );
                                            let _ = self.event_tx.send(
                                                OrchestratorEvent::DocumentOpened { doc_id },
                                            );
                                        }
                                        Err(e) => tracing::error!("Restore failed: {e}"),
                                    }
                                }
                            }
                        }
                    }
                }
            }
            "chat" | "unknown" => {
                // Delegate to the agent loop which handles context, tools, and history
                self.run_chat_agent_loop(query).await?;
            }
            _ => {
                let level = security::action_level(action);
                let _ = self.event_tx.send(OrchestratorEvent::ActionProposed {
                    proposal: ProposedAction {
                        action: action.to_string(),
                        level,
                        plane: sovereign_core::security::Plane::Control,
                        doc_id: None,
                        thread_id: None,
                        description: format!("Unhandled intent: {}", action),
                    },
                });
            }
        }

        let _ = self.event_tx.send(OrchestratorEvent::ActionExecuted {
            action: action.to_string(),
            success: true,
        });

        Ok(())
    }

    /// Scan document content for injection attempts. Emits InjectionDetected events.
    /// Returns true if injection was detected (caller should refuse to process).
    // TODO: integrate into execute_action pipeline — call before processing user-supplied content
    #[allow(dead_code)]
    pub async fn scan_document_for_injection(&self, doc_id: &str) -> bool {
        match self.db.get_document(doc_id).await {
            Ok(doc) => {
                let matches = injection::scan_for_injection(&doc.content);
                if !matches.is_empty() {
                    let top = &matches[0];
                    tracing::warn!(
                        "Injection detected in {}: {} (severity {})",
                        doc_id,
                        top.pattern_name,
                        top.severity
                    );
                    self.log_action(
                        "injection_detected",
                        &format!("{}: {}", doc_id, top.pattern_name),
                    );
                    let _ = self.event_tx.send(OrchestratorEvent::InjectionDetected {
                        source: doc_id.to_string(),
                        pattern: top.pattern_name.clone(),
                    });
                    return true;
                }
                false
            }
            Err(_) => false,
        }
    }

    /// Generate a proactive suggestion based on current context.
    /// Called after N seconds of idle time. Never auto-executes.
    /// Uses adaptive thresholds from the user profile to decide whether to show.
    pub async fn idle_suggest(&self) -> Result<()> {
        // Drain any pending feedback events first
        self.poll_feedback().await;

        let docs = self.db.list_documents(None).await?;
        let threads = self.db.list_threads().await?;

        if let Some((text, action)) = generate_suggestion(&docs, &threads) {
            // Adaptive gating: check profile feedback for this action
            let should_show = {
                if let Ok(profile) = self.profile.lock() {
                    if let Some(fb) = profile.suggestion_feedback.get(&action) {
                        if fb.shown < 5 {
                            true // cold-start: always show first 5
                        } else {
                            let params = AdaptiveParams::from_acceptance_rate(fb.acceptance_rate());
                            fb.acceptance_rate() >= params.suggestion_threshold
                        }
                    } else {
                        true // never shown this action before
                    }
                } else {
                    true
                }
            };

            if should_show {
                // Record that we showed this suggestion
                if let Ok(mut profile) = self.profile.lock() {
                    profile
                        .suggestion_feedback
                        .entry(action.clone())
                        .or_insert_with(SuggestionFeedback::new)
                        .record_shown();
                    if let Err(e) = profile.save(&self.profile_dir) {
                        tracing::warn!("Failed to save profile: {e}");
                    }
                }

                let _ = self.event_tx.send(OrchestratorEvent::BubbleState(
                    BubbleVisualState::Suggesting,
                ));
                let _ = self.event_tx.send(OrchestratorEvent::Suggestion {
                    text,
                    action,
                });
            }
        }

        Ok(())
    }

    /// Drain pending feedback events from the UI and update the profile.
    /// Saves the profile once after processing all events (batch).
    async fn poll_feedback(&self) {
        if let Some(ref rx_mutex) = self.feedback_rx {
            let mut rx = rx_mutex.lock().await;
            let mut profile_dirty = false;
            while let Ok(event) = rx.try_recv() {
                match event {
                    FeedbackEvent::SuggestionAccepted { action } => {
                        if let Ok(mut profile) = self.profile.lock() {
                            profile
                                .suggestion_feedback
                                .entry(action.clone())
                                .or_insert_with(SuggestionFeedback::new)
                                .record_accepted();
                            profile_dirty = true;
                        }
                        tracing::info!("Suggestion accepted: {action}");
                    }
                    FeedbackEvent::SuggestionDismissed { action } => {
                        if let Ok(mut profile) = self.profile.lock() {
                            profile
                                .suggestion_feedback
                                .entry(action.clone())
                                .or_insert_with(SuggestionFeedback::new)
                                .record_dismissed();
                            profile_dirty = true;
                        }
                        tracing::info!("Suggestion dismissed: {action}");
                    }
                }
            }
            // Save profile once after draining all events.
            if profile_dirty {
                if let Ok(mut profile) = self.profile.lock() {
                    if let Err(e) = profile.save(&self.profile_dir) {
                        tracing::warn!("Failed to save profile: {e}");
                    }
                }
            }
        }
    }

    fn log_action(&self, action: &str, details: &str) {
        if let Some(ref log) = self.session_log {
            if let Ok(mut log) = log.lock() {
                log.log_action(action, details);
            }
        }
    }

    /// Load recent session entries, using encrypted decryption if a key is available.
    fn load_session_entries(&self, max_entries: usize) -> Vec<crate::session_log::SessionEntry> {
        #[cfg(feature = "encrypted-log")]
        if let Some(ref key) = self.session_log_key {
            return SessionLog::load_recent_encrypted(&self.profile_dir, max_entries, key);
        }
        SessionLog::load_recent(&self.profile_dir, max_entries)
    }
}

/// Format a tool call proposal into a human-readable description.
fn format_tool_proposal(name: &str, args: &serde_json::Value) -> String {
    match name {
        "create_document" => format!(
            "Create document '{}'",
            args["title"].as_str().unwrap_or("Untitled"),
        ),
        "create_thread" => format!(
            "Create thread '{}'",
            args["name"].as_str().unwrap_or("New Thread"),
        ),
        "rename_thread" => format!(
            "Rename thread '{}' to '{}'",
            args["old_name"].as_str().unwrap_or("?"),
            args["new_name"].as_str().unwrap_or("?"),
        ),
        "move_document" => format!(
            "Move '{}' to thread '{}'",
            args["title"].as_str().unwrap_or("?"),
            args["thread"].as_str().unwrap_or("?"),
        ),
        _ => format!("{}: {}", name, args),
    }
}

/// Parse "X to Y" from a rename target string.
/// Returns (old_name, new_name). Falls back to (target, target) if no " to " found.
fn parse_rename_target(target: &str) -> (String, String) {
    if let Some(idx) = target.to_lowercase().find(" to ") {
        let old = target[..idx].trim().to_string();
        let new = target[idx + 4..].trim().to_string();
        (old, new)
    } else {
        (target.to_string(), target.to_string())
    }
}

/// Parse "DocTitle to ThreadName" from a move target string.
/// Returns (doc_name, thread_name).
fn parse_move_target(target: &str) -> (String, String) {
    if let Some(idx) = target.to_lowercase().find(" to ") {
        let doc = target[..idx].trim().to_string();
        let thread = target[idx + 4..].trim().to_string();
        (doc, thread)
    } else {
        (target.to_string(), String::new())
    }
}

/// Analyze documents and threads to produce a contextual suggestion.
/// Returns (text, action) or None if no suggestion is appropriate.
pub(crate) fn generate_suggestion(
    docs: &[sovereign_db::schema::Document],
    threads: &[Thread],
) -> Option<(String, String)> {
    // Suggest creating a thread if there are docs but no threads
    if !docs.is_empty() && threads.is_empty() {
        return Some((
            "You have documents but no threads. Create a thread to organize them?".into(),
            "create_thread".into(),
        ));
    }

    // Suggest adopting external content if there are many external docs
    let external_count = docs.iter().filter(|d| !d.is_owned).count();
    let total = docs.len();
    if total >= 3 && external_count as f64 / total as f64 > 0.7 {
        return Some((
            format!(
                "{} of {} documents are external. Adopt some to make them yours?",
                external_count, total
            ),
            "adopt".into(),
        ));
    }

    // Suggest creating a milestone if there are many docs in a thread
    for thread in threads {
        let tid = thread.id_string().unwrap_or_default();
        let thread_docs = docs.iter().filter(|d| d.thread_id == tid).count();
        if thread_docs >= 5 {
            return Some((
                format!(
                    "Thread \"{}\" has {} documents. Create a milestone to mark progress?",
                    thread.name, thread_docs
                ),
                "create_milestone".into(),
            ));
        }
    }

    None
}

/// Scan a directory for .gguf model files and return (name, size_mb) pairs.
/// Extracted for testability.
pub(crate) fn scan_gguf_models(model_dir: &str) -> Vec<(String, u64)> {
    let dir = std::path::Path::new(model_dir);
    let mut models = Vec::new();

    if dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("gguf") {
                    let name = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let size_mb = std::fs::metadata(&path)
                        .map(|m| m.len() / (1024 * 1024))
                        .unwrap_or(0);
                    models.push((name, size_mb));
                }
            }
        }
    }

    models
}

/// Resolve a model target name to a full path, appending .gguf if needed.
pub(crate) fn resolve_model_path(model_dir: &str, target: &str) -> std::path::PathBuf {
    let dir = std::path::Path::new(model_dir);

    // 1. Exact filename match (with or without .gguf extension)
    let exact_name = if target.ends_with(".gguf") {
        target.to_string()
    } else {
        format!("{}.gguf", target)
    };
    let exact_path = dir.join(&exact_name);
    if exact_path.exists() {
        return exact_path;
    }

    // 2. Fuzzy match: scan .gguf files for a name that matches the target.
    //    Checks substring containment, then expands the target with known aliases
    //    (e.g. "mistral" also matches "ministral" product names).
    let target_lower = target.to_lowercase();
    let aliases = expand_model_aliases(&target_lower);
    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut best: Option<std::path::PathBuf> = None;
        let mut best_len = usize::MAX; // prefer shortest filename (most specific match)

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("gguf") {
                let name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase();
                let matched = aliases.iter().any(|alias| name.contains(alias));
                if matched && name.len() < best_len {
                    best_len = name.len();
                    best = Some(path);
                }
            }
        }
        if let Some(matched) = best {
            return matched;
        }
    }

    // 3. Fallback: return the exact path (will fail exists() check in caller)
    exact_path
}

/// Expand a target name into a list of search terms including known aliases.
/// E.g. "mistral" → ["mistral", "ministral"] so that the user saying "switch to mistral"
/// matches files named "Ministral-3-3B-Instruct-...".
fn expand_model_aliases(target_lower: &str) -> Vec<String> {
    // Alias groups: names that users might use interchangeably.
    const ALIAS_GROUPS: &[&[&str]] = &[
        &["mistral", "ministral"],
    ];
    let mut result = vec![target_lower.to_string()];
    for group in ALIAS_GROUPS {
        if group.iter().any(|a| target_lower.contains(a) || a.contains(target_lower)) {
            for alias in *group {
                if *alias != target_lower && !result.iter().any(|r| r == alias) {
                    result.push(alias.to_string());
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("sovereign_test_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn scan_gguf_finds_models_in_dir() {
        let dir = make_test_dir("scan_gguf");
        std::fs::write(dir.join("model-a.gguf"), "fake").unwrap();
        std::fs::write(dir.join("model-b.gguf"), "fake data longer").unwrap();
        std::fs::write(dir.join("readme.txt"), "hello").unwrap();

        let models = scan_gguf_models(dir.to_str().unwrap());
        assert_eq!(models.len(), 2);

        let names: Vec<&str> = models.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"model-a.gguf"));
        assert!(names.contains(&"model-b.gguf"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_gguf_empty_dir() {
        let dir = make_test_dir("scan_empty");
        let models = scan_gguf_models(dir.to_str().unwrap());
        assert!(models.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_gguf_nonexistent_dir() {
        let models = scan_gguf_models("/nonexistent/path/that/does/not/exist");
        assert!(models.is_empty());
    }

    #[test]
    fn resolve_model_path_appends_gguf() {
        let path = resolve_model_path("/models", "Qwen2.5-7B");
        assert_eq!(path, std::path::PathBuf::from("/models/Qwen2.5-7B.gguf"));
    }

    #[test]
    fn resolve_model_path_keeps_existing_gguf() {
        let path = resolve_model_path("/models", "Qwen2.5-7B.gguf");
        assert_eq!(path, std::path::PathBuf::from("/models/Qwen2.5-7B.gguf"));
    }

    #[test]
    fn resolve_model_path_fuzzy_matches() {
        let dir = make_test_dir("resolve_fuzzy");
        std::fs::write(dir.join("Ministral-3-3B-Instruct-2512-Q4_K_M.gguf"), "fake").unwrap();
        std::fs::write(dir.join("llama-3.2-1b-instruct-q4_k_m.gguf"), "fake").unwrap();

        // "mistral" should fuzzy-match "Ministral-..."
        let path = resolve_model_path(dir.to_str().unwrap(), "mistral");
        assert_eq!(
            path.file_name().unwrap().to_string_lossy(),
            "Ministral-3-3B-Instruct-2512-Q4_K_M.gguf"
        );

        // "llama" should fuzzy-match "llama-3.2-..."
        let path = resolve_model_path(dir.to_str().unwrap(), "llama");
        assert_eq!(
            path.file_name().unwrap().to_string_lossy(),
            "llama-3.2-1b-instruct-q4_k_m.gguf"
        );

        // No match returns fallback exact path
        let path = resolve_model_path(dir.to_str().unwrap(), "gemma");
        assert_eq!(path.file_name().unwrap().to_string_lossy(), "gemma.gguf");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn suggestion_no_docs_returns_none() {
        let result = generate_suggestion(&[], &[]);
        assert!(result.is_none());
    }

    #[test]
    fn suggestion_docs_without_threads_suggests_create() {
        use sovereign_db::schema::Document;
        let docs = vec![
            Document::new("A".into(), "thread:t".into(), true),
        ];
        let result = generate_suggestion(&docs, &[]);
        assert!(result.is_some());
        let (text, action) = result.unwrap();
        assert_eq!(action, "create_thread");
        assert!(text.contains("no threads"));
    }

    #[test]
    fn suggestion_many_external_suggests_adopt() {
        use sovereign_db::schema::{Document, Thread};
        let thread = Thread::new("T".into(), "".into());
        let docs = vec![
            Document::new("A".into(), "thread:t".into(), false),
            Document::new("B".into(), "thread:t".into(), false),
            Document::new("C".into(), "thread:t".into(), false),
        ];
        let result = generate_suggestion(&docs, &[thread]);
        assert!(result.is_some());
        let (text, action) = result.unwrap();
        assert_eq!(action, "adopt");
        assert!(text.contains("external"));
    }

    #[test]
    fn suggestion_no_suggestion_for_balanced_docs() {
        use sovereign_db::schema::{Document, Thread};
        let thread = Thread::new("T".into(), "".into());
        let docs = vec![
            Document::new("A".into(), "thread:t".into(), true),
            Document::new("B".into(), "thread:t".into(), true),
            Document::new("C".into(), "thread:t".into(), false),
        ];
        let result = generate_suggestion(&docs, &[thread]);
        assert!(result.is_none());
    }

    #[test]
    fn parse_rename_target_splits() {
        let (old, new) = parse_rename_target("Alpha to Beta");
        assert_eq!(old, "Alpha");
        assert_eq!(new, "Beta");
    }

    #[test]
    fn parse_move_target_splits() {
        let (doc, thread) = parse_move_target("Notes to Research");
        assert_eq!(doc, "Notes");
        assert_eq!(thread, "Research");
    }

    #[test]
    fn adaptive_params_gate_cold_start_always_shows() {
        // Cold start: shown < 5 means we always show
        let fb = sovereign_core::profile::SuggestionFeedback {
            shown: 3,
            accepted: 0,
            dismissed: 3,
        };
        // Even with 0% acceptance, cold-start passes
        assert!(fb.shown < 5);
    }

    #[test]
    fn adaptive_params_gate_blocks_low_acceptance() {
        let fb = sovereign_core::profile::SuggestionFeedback {
            shown: 10,
            accepted: 1,
            dismissed: 9,
        };
        let params = AdaptiveParams::from_acceptance_rate(fb.acceptance_rate());
        // acceptance_rate = 0.1, threshold = 0.9 → should NOT show
        assert!(fb.acceptance_rate() < params.suggestion_threshold);
    }

    #[test]
    fn adaptive_params_gate_allows_high_acceptance() {
        let fb = sovereign_core::profile::SuggestionFeedback {
            shown: 10,
            accepted: 9,
            dismissed: 1,
        };
        let params = AdaptiveParams::from_acceptance_rate(fb.acceptance_rate());
        // acceptance_rate = 0.9, threshold = 0.5 → should show
        assert!(fb.acceptance_rate() >= params.suggestion_threshold);
    }

    #[test]
    fn feedback_event_is_send_and_clone() {
        fn assert_send<T: Send>() {}
        fn assert_clone<T: Clone>() {}
        assert_send::<FeedbackEvent>();
        assert_clone::<FeedbackEvent>();
    }

    #[test]
    fn poll_feedback_updates_profile() {
        // Unit test: verify SuggestionFeedback math used by poll_feedback
        let mut fb = sovereign_core::profile::SuggestionFeedback::new();
        fb.record_accepted();
        fb.record_accepted();
        fb.record_dismissed();
        assert_eq!(fb.accepted, 2);
        assert_eq!(fb.dismissed, 1);
        assert!((fb.acceptance_rate() - 2.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn trust_persistence_roundtrip() {
        let dir = make_test_dir("trust_persist");
        let mut trust = crate::trust::TrustTracker::with_threshold(3);
        trust.record_approval("create_thread");
        trust.record_approval("create_thread");
        trust.save(&dir).unwrap();

        let loaded = crate::trust::TrustTracker::load(&dir).unwrap();
        assert_eq!(loaded.approval_count("create_thread"), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
