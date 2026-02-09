use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use sovereign_db::surreal::SurrealGraphDB;
use sovereign_db::GraphDB;

/// Auto-commit threshold: commit after this many edits.
const EDIT_THRESHOLD: u32 = 50;
/// Auto-commit threshold: commit after this many seconds since last commit.
const TIME_THRESHOLD_SECS: u64 = 300; // 5 minutes

/// Tracks document edits and commits automatically based on adaptive frequency.
///
/// Policy (from spec):
/// - High activity: commit after 50 edits OR 5 minutes since last commit
/// - Low activity: commit on context switch (document close) or session end
pub struct AutoCommitEngine {
    db: Arc<SurrealGraphDB>,
    edit_counts: HashMap<String, u32>,
    last_commit_times: HashMap<String, Instant>,
}

impl AutoCommitEngine {
    pub fn new(db: Arc<SurrealGraphDB>) -> Self {
        Self {
            db,
            edit_counts: HashMap::new(),
            last_commit_times: HashMap::new(),
        }
    }

    /// Record an edit for a document. Called on each save.
    pub fn record_edit(&mut self, doc_id: &str) {
        *self.edit_counts.entry(doc_id.to_string()).or_insert(0) += 1;
    }

    /// Check all tracked documents and commit any that exceed thresholds.
    pub async fn check_and_commit(&mut self) {
        let now = Instant::now();
        let doc_ids: Vec<String> = self.edit_counts.keys().cloned().collect();

        for doc_id in doc_ids {
            let count = *self.edit_counts.get(&doc_id).unwrap_or(&0);
            if count == 0 {
                continue;
            }

            let last = self.last_commit_times.get(&doc_id).copied();
            let elapsed = last.map(|t| now.duration_since(t).as_secs()).unwrap_or(u64::MAX);

            if count >= EDIT_THRESHOLD || elapsed >= TIME_THRESHOLD_SECS {
                let msg = format!("Auto-commit: {} edits", count);
                match self.db.commit_document(&doc_id, &msg).await {
                    Ok(commit) => {
                        tracing::info!(
                            "Auto-committed {}: {} ({})",
                            doc_id,
                            msg,
                            commit.id_string().unwrap_or_default()
                        );
                        self.edit_counts.insert(doc_id.clone(), 0);
                        self.last_commit_times.insert(doc_id, now);
                    }
                    Err(e) => {
                        tracing::error!("Auto-commit failed for {}: {e}", doc_id);
                    }
                }
            }
        }
    }

    /// Force-commit a specific document (e.g., on close or context switch).
    pub async fn commit_on_close(&mut self, doc_id: &str) {
        let count = self.edit_counts.get(doc_id).copied().unwrap_or(0);
        if count == 0 {
            return;
        }

        let msg = format!("Auto-commit on close: {} edits", count);
        match self.db.commit_document(doc_id, &msg).await {
            Ok(commit) => {
                tracing::info!(
                    "Committed on close {}: {} ({})",
                    doc_id,
                    msg,
                    commit.id_string().unwrap_or_default()
                );
                self.edit_counts.insert(doc_id.to_string(), 0);
                self.last_commit_times.insert(doc_id.to_string(), Instant::now());
            }
            Err(e) => {
                tracing::error!("Commit on close failed for {}: {e}", doc_id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sovereign_db::schema::Document;
    use sovereign_db::surreal::StorageMode;

    async fn setup() -> (Arc<SurrealGraphDB>, String) {
        let db = SurrealGraphDB::new(StorageMode::Memory).await.unwrap();
        db.connect().await.unwrap();
        db.init_schema().await.unwrap();

        let doc = Document::new("Test".into(), "thread:t".into(), true);
        let created = db.create_document(doc).await.unwrap();
        let doc_id = created.id_string().unwrap();
        (Arc::new(db), doc_id)
    }

    #[tokio::test]
    async fn no_commit_when_no_edits() {
        let (db, doc_id) = setup().await;
        let mut engine = AutoCommitEngine::new(db.clone());
        engine.check_and_commit().await;

        let commits = db.list_document_commits(&doc_id).await.unwrap();
        assert_eq!(commits.len(), 0);
    }

    #[tokio::test]
    async fn commit_after_threshold_edits() {
        let (db, doc_id) = setup().await;
        let mut engine = AutoCommitEngine::new(db.clone());

        for _ in 0..EDIT_THRESHOLD {
            engine.record_edit(&doc_id);
        }
        engine.check_and_commit().await;

        let commits = db.list_document_commits(&doc_id).await.unwrap();
        assert_eq!(commits.len(), 1);
        assert!(commits[0].message.contains("Auto-commit"));
    }

    #[tokio::test]
    async fn commit_on_close_flushes() {
        let (db, doc_id) = setup().await;
        let mut engine = AutoCommitEngine::new(db.clone());

        engine.record_edit(&doc_id);
        engine.record_edit(&doc_id);
        engine.commit_on_close(&doc_id).await;

        let commits = db.list_document_commits(&doc_id).await.unwrap();
        assert_eq!(commits.len(), 1);
        assert!(commits[0].message.contains("on close"));
    }

    #[tokio::test]
    async fn no_commit_on_close_without_edits() {
        let (db, doc_id) = setup().await;
        let mut engine = AutoCommitEngine::new(db.clone());
        engine.commit_on_close(&doc_id).await;

        let commits = db.list_document_commits(&doc_id).await.unwrap();
        assert_eq!(commits.len(), 0);
    }
}
