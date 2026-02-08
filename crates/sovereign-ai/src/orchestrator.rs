use std::sync::mpsc;
use std::sync::Arc;

use anyhow::Result;
use sovereign_core::config::AiConfig;
use sovereign_core::interfaces::OrchestratorEvent;
use sovereign_db::surreal::SurrealGraphDB;
use sovereign_db::GraphDB;

use crate::intent::IntentClassifier;

/// Central AI orchestrator. Owns the intent classifier and DB handle.
/// Receives queries (text from search overlay or voice pipeline),
/// classifies intent, executes actions, and emits events to the UI.
pub struct Orchestrator {
    classifier: IntentClassifier,
    db: Arc<SurrealGraphDB>,
    event_tx: mpsc::Sender<OrchestratorEvent>,
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

        Ok(Self {
            classifier,
            db,
            event_tx,
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
                let _ = self.event_tx.send(OrchestratorEvent::SearchResults {
                    query: query.into(),
                    doc_ids: matches,
                });
            }
            "open" | "navigate" => {
                if let Some(target) = &intent.target {
                    // Try to find the document by title
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
            _ => {
                let _ = self.event_tx.send(OrchestratorEvent::ActionProposed {
                    description: format!("Unhandled intent: {}", intent.action),
                    action: intent.action,
                });
            }
        }

        Ok(())
    }
}
