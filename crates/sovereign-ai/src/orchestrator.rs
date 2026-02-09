use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use sovereign_core::config::AiConfig;
use sovereign_core::interfaces::{CommitSummary, OrchestratorEvent};
use sovereign_db::schema::Thread;
use sovereign_db::surreal::SurrealGraphDB;
use sovereign_db::GraphDB;

use crate::intent::IntentClassifier;
use crate::session_log::SessionLog;

/// Central AI orchestrator. Owns the intent classifier and DB handle.
/// Receives queries (text from search overlay or voice pipeline),
/// classifies intent, executes actions, and emits events to the UI.
pub struct Orchestrator {
    classifier: IntentClassifier,
    db: Arc<SurrealGraphDB>,
    event_tx: mpsc::Sender<OrchestratorEvent>,
    session_log: Option<Mutex<SessionLog>>,
}

impl Orchestrator {
    /// Create a new orchestrator. Loads the 3B router model eagerly.
    pub async fn new(
        config: AiConfig,
        db: Arc<SurrealGraphDB>,
        event_tx: mpsc::Sender<OrchestratorEvent>,
    ) -> Result<Self> {
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
        })
    }

    /// Handle a user query: classify intent, execute, emit events.
    pub async fn handle_query(&self, query: &str) -> Result<()> {
        let intent = self.classifier.classify(query).await?;
        tracing::info!(
            "Intent: action={}, confidence={:.2}, target={:?}",
            intent.action,
            intent.confidence,
            intent.target
        );

        // Log user input
        if let Some(ref log) = self.session_log {
            if let Ok(mut log) = log.lock() {
                log.log_user_input("text", query, &intent.action);
            }
        }

        match intent.action.as_str() {
            "search" => {
                let search_term = intent
                    .target
                    .as_deref()
                    .unwrap_or(query)
                    .to_lowercase();

                let docs = self.db.list_documents(None).await?;
                let matches: Vec<String> = docs
                    .iter()
                    .filter(|d| d.title.to_lowercase().contains(&search_term))
                    .filter_map(|d| d.id_string())
                    .collect();

                tracing::info!("Search '{}': {} matches", search_term, matches.len());
                self.log_action("search", &format!("{} matches for '{}'", matches.len(), search_term));
                let _ = self.event_tx.send(OrchestratorEvent::SearchResults {
                    query: query.into(),
                    doc_ids: matches,
                });
            }
            "open" | "navigate" => {
                if let Some(target) = &intent.target {
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
                let name = intent
                    .target
                    .as_deref()
                    .unwrap_or("New Thread")
                    .to_string();
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
                if let Some(target) = &intent.target {
                    // Expect entities with ("new_name", "...") or parse "X to Y" from target
                    let (old_name, new_name) = parse_rename_target(target);
                    let threads = self.db.list_threads().await?;
                    if let Some(thread) = threads
                        .iter()
                        .find(|t| t.name.to_lowercase().contains(&old_name.to_lowercase()))
                    {
                        if let Some(tid) = thread.id_string() {
                            match self.db.update_thread(&tid, Some(&new_name), None).await {
                                Ok(_) => {
                                    tracing::info!("Thread renamed: {} → {}", old_name, new_name);
                                    let _ = self.event_tx.send(OrchestratorEvent::ThreadRenamed {
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
                if let Some(target) = &intent.target {
                    let threads = self.db.list_threads().await?;
                    if let Some(thread) = threads
                        .iter()
                        .find(|t| t.name.to_lowercase().contains(&target.to_lowercase()))
                    {
                        if let Some(tid) = thread.id_string() {
                            match self.db.delete_thread(&tid).await {
                                Ok(()) => {
                                    tracing::info!("Thread deleted: {} ({})", target, tid);
                                    let _ = self.event_tx.send(OrchestratorEvent::ThreadDeleted {
                                        thread_id: tid,
                                    });
                                }
                                Err(e) => tracing::error!("Failed to delete thread: {e}"),
                            }
                        }
                    }
                }
            }
            "move_document" => {
                // Expect target like "DocTitle to ThreadName" or entities
                if let Some(target) = &intent.target {
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
                                let _ = self.event_tx.send(OrchestratorEvent::DocumentMoved {
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
                if let Some(target) = &intent.target {
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
                            tracing::info!("History for {}: {} commits", target, summaries.len());
                            self.log_action("history", &format!("{} commits for {}", summaries.len(), target));
                            let _ = self.event_tx.send(OrchestratorEvent::VersionHistory {
                                doc_id,
                                commits: summaries,
                            });
                        }
                    }
                }
            }
            "restore" => {
                // Expects entities with commit_id, or target as "DocTitle to commit:xyz"
                if let Some(target) = &intent.target {
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
                                        tracing::info!("Restored {} to {}", doc_name, commit_ref);
                                        self.log_action("restore", &format!("{} to {}", doc_name, commit_ref));
                                        let _ = self.event_tx.send(
                                            OrchestratorEvent::DocumentOpened { doc_id },
                                        );
                                    }
                                    Err(e) => tracing::error!("Restore failed: {e}"),
                                }
                            } else {
                                // No specific commit — restore to latest (previous) commit
                                let commits = self.db.list_document_commits(&doc_id).await?;
                                if commits.len() >= 2 {
                                    let prev = commits[1].id_string().unwrap_or_default();
                                    match self.db.restore_document(&doc_id, &prev).await {
                                        Ok(_) => {
                                            tracing::info!("Restored {} to previous version", doc_name);
                                            self.log_action("restore", &format!("{} to previous", doc_name));
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
                let _ = self.event_tx.send(OrchestratorEvent::ActionProposed {
                    description: format!("Unhandled intent: {}", intent.action),
                    action: intent.action,
                });
            }
        }

        Ok(())
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
