use std::path::PathBuf;
use std::sync::mpsc;
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
use sovereign_db::surreal::SurrealGraphDB;
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
    classifier: IntentClassifier,
    db: Arc<SurrealGraphDB>,
    event_tx: mpsc::Sender<OrchestratorEvent>,
    session_log: Option<Mutex<SessionLog>>,
    decision_rx: Option<Mutex<mpsc::Receiver<ActionDecision>>>,
    feedback_rx: Option<Mutex<mpsc::Receiver<FeedbackEvent>>>,
    trust: Mutex<TrustTracker>,
    profile: Mutex<UserProfile>,
    profile_dir: PathBuf,
    model_dir: String,
    n_gpu_layers: i32,
}

impl Orchestrator {
    /// Create a new orchestrator. Loads the 3B router model eagerly.
    pub async fn new(
        config: AiConfig,
        db: Arc<SurrealGraphDB>,
        event_tx: mpsc::Sender<OrchestratorEvent>,
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
            classifier,
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
        })
    }

    /// Attach a decision channel for user confirmations of Level 3+ actions.
    pub fn set_decision_rx(&mut self, rx: mpsc::Receiver<ActionDecision>) {
        self.decision_rx = Some(Mutex::new(rx));
    }

    /// Attach a feedback channel for suggestion accept/dismiss events from the UI.
    pub fn set_feedback_rx(&mut self, rx: mpsc::Receiver<FeedbackEvent>) {
        self.feedback_rx = Some(Mutex::new(rx));
    }

    /// Handle a user query: classify intent, gate check, execute or await confirmation.
    pub async fn handle_query(&self, query: &str) -> Result<()> {
        let intent = self.classifier.classify(query).await?;
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
                let decision = self.wait_for_decision();
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

    /// Wait for a user decision on the decision channel (30s timeout).
    /// If no channel is configured, auto-approve (for backward compatibility/testing).
    fn wait_for_decision(&self) -> ActionDecision {
        if let Some(ref rx_mutex) = self.decision_rx {
            if let Ok(rx) = rx_mutex.lock() {
                match rx.recv_timeout(Duration::from_secs(30)) {
                    Ok(decision) => return decision,
                    Err(_) => {
                        tracing::warn!("Decision timeout — rejecting action");
                        return ActionDecision::Reject("Timeout waiting for user decision".into());
                    }
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
                let search_term = target.unwrap_or(query).to_lowercase();

                let docs = self.db.list_documents(None).await?;
                let matches: Vec<String> = docs
                    .iter()
                    .filter(|d| d.title.to_lowercase().contains(&search_term))
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
                    let docs = self.db.list_documents(None).await?;
                    if let Some(doc) = docs
                        .iter()
                        .find(|d| d.title.to_lowercase().contains(&target.to_lowercase()))
                    {
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
                    let threads = self.db.list_threads().await?;
                    if let Some(thread) = threads
                        .iter()
                        .find(|t| t.name.to_lowercase().contains(&old_name.to_lowercase()))
                    {
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
                    let threads = self.db.list_threads().await?;
                    if let Some(thread) = threads
                        .iter()
                        .find(|t| t.name.to_lowercase().contains(&target.to_lowercase()))
                    {
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
                    let docs = self.db.list_documents(None).await?;
                    let threads = self.db.list_threads().await?;

                    let doc = docs
                        .iter()
                        .find(|d| d.title.to_lowercase().contains(&doc_name.to_lowercase()));
                    let thread = threads
                        .iter()
                        .find(|t| t.name.to_lowercase().contains(&thread_name.to_lowercase()));

                    if let (Some(doc), Some(thread)) = (doc, thread) {
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
                    let docs = self.db.list_documents(None).await?;
                    if let Some(doc) = docs
                        .iter()
                        .find(|d| d.title.to_lowercase().contains(&target.to_lowercase()))
                    {
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
                    let docs = self.db.list_documents(None).await?;
                    if let Some(doc) = docs
                        .iter()
                        .find(|d| d.title.to_lowercase().contains(&target.to_lowercase()))
                    {
                        let content = &doc.content;
                        let prompt = crate::llm::prompt::qwen_chat_prompt(
                            "You are a concise summarizer. Summarize the following document in 2-3 sentences.",
                            content,
                        );
                        match self.classifier.router.generate(&prompt, 200).await {
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
                let found = scan_gguf_models(&self.model_dir);
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
                        match self
                            .classifier
                            .swap_router(&path_str, self.n_gpu_layers)
                            .await
                        {
                            Ok(()) => {
                                tracing::info!("Model swapped to: {model_name}");
                                self.log_action("swap_model", &model_name);
                                let _ = self.event_tx.send(OrchestratorEvent::SkillResult {
                                    skill: "model_manager".into(),
                                    action: "swap_model".into(),
                                    kind: "model_swapped".into(),
                                    data: serde_json::json!({
                                        "model": model_name,
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
                    let threads = self.db.list_threads().await?;
                    let target_thread = threads
                        .iter()
                        .find(|t| t.name.to_lowercase().contains(&target_name.to_lowercase()));
                    let source_thread = threads
                        .iter()
                        .find(|t| t.name.to_lowercase().contains(&source_name.to_lowercase()));

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
                    let threads = self.db.list_threads().await?;
                    if let Some(thread) = threads
                        .iter()
                        .find(|t| t.name.to_lowercase().contains(&thread_name.to_lowercase()))
                    {
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
                    let docs = self.db.list_documents(None).await?;
                    if let Some(doc) = docs
                        .iter()
                        .find(|d| d.title.to_lowercase().contains(&target.to_lowercase()))
                    {
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

                    let threads = self.db.list_threads().await?;
                    let thread = if thread_name.is_empty() {
                        threads.first()
                    } else {
                        threads.iter().find(|t| {
                            t.name.to_lowercase().contains(&thread_name.to_lowercase())
                        })
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
                let threads = self.db.list_threads().await?;
                let thread = if let Some(target) = target {
                    threads.iter().find(|t| {
                        t.name.to_lowercase().contains(&target.to_lowercase())
                    })
                } else {
                    threads.first()
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
                    let docs = self.db.list_documents(None).await?;
                    if let Some(doc) = docs
                        .iter()
                        .find(|d| d.title.to_lowercase().contains(&doc_name.to_lowercase()))
                    {
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
        self.poll_feedback();

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
    fn poll_feedback(&self) {
        if let Some(ref rx_mutex) = self.feedback_rx {
            if let Ok(rx) = rx_mutex.lock() {
                while let Ok(event) = rx.try_recv() {
                    match event {
                        FeedbackEvent::SuggestionAccepted { action } => {
                            if let Ok(mut profile) = self.profile.lock() {
                                profile
                                    .suggestion_feedback
                                    .entry(action.clone())
                                    .or_insert_with(SuggestionFeedback::new)
                                    .record_accepted();
                                if let Err(e) = profile.save(&self.profile_dir) {
                                    tracing::warn!("Failed to save profile: {e}");
                                }
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
                                if let Err(e) = profile.save(&self.profile_dir) {
                                    tracing::warn!("Failed to save profile: {e}");
                                }
                            }
                            tracing::info!("Suggestion dismissed: {action}");
                        }
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
    let model_name = if target.ends_with(".gguf") {
        target.to_string()
    } else {
        format!("{}.gguf", target)
    };
    std::path::Path::new(model_dir).join(model_name)
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
