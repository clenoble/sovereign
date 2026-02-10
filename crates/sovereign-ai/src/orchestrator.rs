use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use sovereign_core::config::AiConfig;
use sovereign_core::interfaces::{CommitSummary, ModelBackend, OrchestratorEvent};
use sovereign_core::security::{self, ActionDecision, BubbleVisualState, ProposedAction};
use sovereign_db::schema::Thread;
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
    trust: Mutex<TrustTracker>,
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

        // Initialize session log
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let log_dir = std::path::PathBuf::from(home)
            .join(".sovereign")
            .join("orchestrator");
        let session_log = match SessionLog::open(&log_dir) {
            Ok(log) => Some(Mutex::new(log)),
            Err(e) => {
                tracing::warn!("Session log unavailable: {e}");
                None
            }
        };

        Ok(Self {
            classifier,
            db,
            event_tx,
            session_log,
            decision_rx: None,
            trust: Mutex::new(TrustTracker::new()),
            model_dir,
            n_gpu_layers,
        })
    }

    /// Attach a decision channel for user confirmations of Level 3+ actions.
    pub fn set_decision_rx(&mut self, rx: mpsc::Receiver<ActionDecision>) {
        self.decision_rx = Some(Mutex::new(rx));
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
                        // Record approval in trust tracker
                        if let Ok(mut trust) = self.trust.lock() {
                            trust.record_approval(&intent.action);
                        }
                        let _ = self.event_tx.send(OrchestratorEvent::BubbleState(
                            BubbleVisualState::Executing,
                        ));
                        self.execute_action(&intent.action, intent.target.as_deref(), query)
                            .await?;
                    }
                    ActionDecision::Reject(reason) => {
                        // Record rejection in trust tracker (resets counter)
                        if let Ok(mut trust) = self.trust.lock() {
                            trust.record_rejection(&intent.action);
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
}
