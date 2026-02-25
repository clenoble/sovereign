use std::sync::Arc;

use sha2::{Digest, Sha256};
use sovereign_db::schema::{Commit, Document};
use sovereign_db::GraphDB;

use crate::error::{P2pError, P2pResult};
use crate::protocol::manifest::{DocumentManifestEntry, SyncManifest};
use crate::protocol::sync::EncryptedCommit;

/// Middleware between the P2P networking layer and the database.
///
/// Owns an `Arc<dyn GraphDB>` and exposes async methods for building
/// manifests, fetching/applying commits, and checking commit ancestry.
pub struct SyncService {
    db: Arc<dyn GraphDB>,
    device_id: String,
}

impl SyncService {
    pub fn new(db: Arc<dyn GraphDB>, device_id: String) -> Self {
        Self { db, device_id }
    }

    /// Build a SyncManifest from all documents in the database.
    pub async fn build_manifest(&self) -> P2pResult<SyncManifest> {
        let docs = self
            .db
            .list_documents(None)
            .await
            .map_err(|e| P2pError::SyncError(format!("failed to list documents: {e}")))?;

        let mut manifest = SyncManifest::new(self.device_id.clone());

        for doc in &docs {
            let doc_id = match doc.id_string() {
                Some(id) => id,
                None => continue,
            };

            let commits = self
                .db
                .list_document_commits(&doc_id)
                .await
                .unwrap_or_default();

            manifest.entries.push(DocumentManifestEntry {
                doc_id,
                head_commit: doc.head_commit.clone(),
                commit_count: commits.len() as u32,
                content_hash: content_hash(&doc.content),
                modified_at: doc.modified_at.to_rfc3339(),
            });
        }

        Ok(manifest)
    }

    /// Check if `potential_ancestor` is in the commit chain of `doc_id`.
    ///
    /// Walks the chain from `descendant` backwards through parent_commit links.
    /// Returns true if `potential_ancestor` is found in the chain.
    pub async fn is_ancestor(
        &self,
        doc_id: &str,
        potential_ancestor: &str,
        descendant: &str,
    ) -> bool {
        let commits = match self.db.list_document_commits(doc_id).await {
            Ok(c) => c,
            Err(_) => return false,
        };

        // Build a map from commit_id -> parent_commit for fast lookup
        let parent_map: std::collections::HashMap<String, Option<String>> = commits
            .iter()
            .filter_map(|c| {
                c.id_string()
                    .map(|id| (id, c.parent_commit.clone()))
            })
            .collect();

        // Walk from descendant backwards
        let mut current = Some(descendant.to_string());
        while let Some(ref commit_id) = current {
            if commit_id == potential_ancestor {
                return true;
            }
            current = parent_map
                .get(commit_id)
                .and_then(|parent| parent.clone());
        }

        false
    }

    /// Retrieve commits by their IDs, packaged as EncryptedCommit for transport.
    ///
    /// For Phase 1, snapshots are sent as plaintext base64 (no pair-key encryption).
    pub async fn get_commits(&self, commit_ids: &[String]) -> P2pResult<Vec<EncryptedCommit>> {
        let mut result = Vec::with_capacity(commit_ids.len());

        for commit_id in commit_ids {
            let commit = self
                .db
                .get_commit(commit_id)
                .await
                .map_err(|e| P2pError::SyncError(format!("failed to get commit {commit_id}: {e}")))?;

            result.push(commit_to_transport(&commit));
        }

        Ok(result)
    }

    /// Retrieve all commits for a document that are descendants of `since_commit`.
    ///
    /// If `since_commit` is None, returns all commits for the document.
    pub async fn get_commits_since(
        &self,
        doc_id: &str,
        since_commit: Option<&str>,
    ) -> P2pResult<Vec<EncryptedCommit>> {
        let commits = self
            .db
            .list_document_commits(doc_id)
            .await
            .map_err(|e| P2pError::SyncError(format!("failed to list commits for {doc_id}: {e}")))?;

        match since_commit {
            None => Ok(commits.iter().map(commit_to_transport).collect()),
            Some(since) => {
                // Commits are returned most-recent-first. Collect until we hit `since`.
                let mut result = Vec::new();
                for c in &commits {
                    if c.id_string().as_deref() == Some(since) {
                        break;
                    }
                    result.push(commit_to_transport(c));
                }
                Ok(result)
            }
        }
    }

    /// Apply received commits to the local database (fast-forward merge).
    ///
    /// For each commit: creates the document if it doesn't exist, then applies
    /// the snapshot as a new local commit. Returns the number of documents updated.
    pub async fn apply_commits(&self, commits: Vec<EncryptedCommit>) -> P2pResult<u32> {
        let mut docs_updated = std::collections::HashSet::new();

        // Sort commits by timestamp so we apply them in order
        let mut sorted = commits;
        sorted.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        for ec in &sorted {
            let snapshot = transport_to_snapshot(ec);

            // Check if document exists
            let doc_exists = self.db.get_document(&ec.document_id).await.is_ok();

            if !doc_exists {
                // Create the document from the snapshot
                let doc = Document::new(
                    snapshot.title.clone(),
                    "default".to_string(), // Will be in a default thread
                    false,                 // External until adopted
                );
                let created = self
                    .db
                    .create_document(doc)
                    .await
                    .map_err(|e| P2pError::SyncError(format!("failed to create doc: {e}")))?;

                let created_id = created
                    .id_string()
                    .ok_or_else(|| P2pError::SyncError("created doc has no ID".into()))?;

                // Update content from snapshot
                self.db
                    .update_document(&created_id, Some(&snapshot.title), Some(&snapshot.content))
                    .await
                    .map_err(|e| P2pError::SyncError(format!("failed to update doc: {e}")))?;

                // Create a commit to record this
                self.db
                    .commit_document(&created_id, &ec.message)
                    .await
                    .map_err(|e| P2pError::SyncError(format!("failed to commit doc: {e}")))?;

                docs_updated.insert(created_id);
            } else {
                // Update existing document with snapshot content
                self.db
                    .update_document(
                        &ec.document_id,
                        Some(&snapshot.title),
                        Some(&snapshot.content),
                    )
                    .await
                    .map_err(|e| P2pError::SyncError(format!("failed to update doc: {e}")))?;

                // Create a local commit recording this sync
                self.db
                    .commit_document(&ec.document_id, &format!("sync: {}", ec.message))
                    .await
                    .map_err(|e| P2pError::SyncError(format!("failed to commit doc: {e}")))?;

                docs_updated.insert(ec.document_id.clone());
            }
        }

        Ok(docs_updated.len() as u32)
    }

    /// Get the device ID for this sync service.
    pub fn device_id(&self) -> &str {
        &self.device_id
    }
}

/// Compute a SHA-256 hash of document content.
pub fn content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Convert a DB Commit to the transport format.
fn commit_to_transport(commit: &Commit) -> EncryptedCommit {
    // For Phase 1: snapshot is plaintext base64, no encryption
    let snapshot_json = serde_json::to_string(&commit.snapshot).unwrap_or_default();
    let snapshot_b64 =
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &snapshot_json);

    EncryptedCommit {
        commit_id: commit.id_string().unwrap_or_default(),
        document_id: commit.document_id.clone(),
        parent_commit: commit.parent_commit.clone(),
        encrypted_snapshot: snapshot_b64,
        nonce: String::new(), // Empty nonce = plaintext marker
        message: commit.message.clone(),
        timestamp: commit.timestamp.to_rfc3339(),
    }
}

/// Convert a transport EncryptedCommit back to a DocumentSnapshot.
fn transport_to_snapshot(ec: &EncryptedCommit) -> sovereign_db::schema::DocumentSnapshot {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&ec.encrypted_snapshot)
        .unwrap_or_default();

    serde_json::from_slice(&bytes).unwrap_or(sovereign_db::schema::DocumentSnapshot {
        document_id: ec.document_id.clone(),
        title: String::new(),
        content: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sovereign_db::mock::MockGraphDB;
    use sovereign_db::schema::{Document, Thread};

    fn mock_sync_service() -> (Arc<MockGraphDB>, SyncService) {
        let db = Arc::new(MockGraphDB::new());
        let svc = SyncService::new(db.clone(), "device-1".into());
        (db, svc)
    }

    #[tokio::test]
    async fn build_manifest_includes_all_docs() {
        let (db, svc) = mock_sync_service();
        let t = db.create_thread(Thread::new("T".into(), "".into())).await.unwrap();
        let tid = t.id_string().unwrap();

        db.create_document(Document::new("Doc A".into(), tid.clone(), true)).await.unwrap();
        db.create_document(Document::new("Doc B".into(), tid.clone(), true)).await.unwrap();

        let manifest = svc.build_manifest().await.unwrap();
        assert_eq!(manifest.device_id, "device-1");
        assert_eq!(manifest.entries.len(), 2);
    }

    #[tokio::test]
    async fn build_manifest_empty_db() {
        let (_db, svc) = mock_sync_service();
        let manifest = svc.build_manifest().await.unwrap();
        assert!(manifest.entries.is_empty());
    }

    #[tokio::test]
    async fn is_ancestor_true_for_parent() {
        let (db, svc) = mock_sync_service();
        let t = db.create_thread(Thread::new("T".into(), "".into())).await.unwrap();
        let tid = t.id_string().unwrap();

        let doc = db.create_document(Document::new("Doc".into(), tid, true)).await.unwrap();
        let doc_id = doc.id_string().unwrap();

        let c1 = db.commit_document(&doc_id, "first").await.unwrap();
        let c1_id = c1.id_string().unwrap();
        let c2 = db.commit_document(&doc_id, "second").await.unwrap();
        let c2_id = c2.id_string().unwrap();

        assert!(svc.is_ancestor(&doc_id, &c1_id, &c2_id).await);
    }

    #[tokio::test]
    async fn is_ancestor_false_for_unrelated() {
        let (db, svc) = mock_sync_service();
        let t = db.create_thread(Thread::new("T".into(), "".into())).await.unwrap();
        let tid = t.id_string().unwrap();

        let doc = db.create_document(Document::new("Doc".into(), tid, true)).await.unwrap();
        let doc_id = doc.id_string().unwrap();

        let c1 = db.commit_document(&doc_id, "first").await.unwrap();
        let c1_id = c1.id_string().unwrap();

        // c1 is not an ancestor of itself in the parent chain (it IS itself, not its ancestor)
        // A commit with no parent should return false for a non-existent ancestor
        assert!(!svc.is_ancestor(&doc_id, "commit:nonexistent", &c1_id).await);
    }

    #[tokio::test]
    async fn get_commits_retrieves_by_id() {
        let (db, svc) = mock_sync_service();
        let t = db.create_thread(Thread::new("T".into(), "".into())).await.unwrap();
        let tid = t.id_string().unwrap();

        let doc = db.create_document(Document::new("Doc".into(), tid, true)).await.unwrap();
        let doc_id = doc.id_string().unwrap();

        let c = db.commit_document(&doc_id, "snapshot").await.unwrap();
        let cid = c.id_string().unwrap();

        let result = svc.get_commits(&[cid.clone()]).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].commit_id, cid);
        assert_eq!(result[0].message, "snapshot");
    }

    #[tokio::test]
    async fn get_commits_since_filters_correctly() {
        let (db, svc) = mock_sync_service();
        let t = db.create_thread(Thread::new("T".into(), "".into())).await.unwrap();
        let tid = t.id_string().unwrap();

        let doc = db.create_document(Document::new("Doc".into(), tid, true)).await.unwrap();
        let doc_id = doc.id_string().unwrap();

        let c1 = db.commit_document(&doc_id, "first").await.unwrap();
        let c1_id = c1.id_string().unwrap();
        db.commit_document(&doc_id, "second").await.unwrap();
        db.commit_document(&doc_id, "third").await.unwrap();

        // Get commits since c1 — should return only newer commits (second, third)
        let result = svc.get_commits_since(&doc_id, Some(&c1_id)).await.unwrap();
        assert_eq!(result.len(), 2);

        // Get all commits (no since)
        let all = svc.get_commits_since(&doc_id, None).await.unwrap();
        assert_eq!(all.len(), 3);
    }

    #[tokio::test]
    async fn apply_commits_creates_new_doc() {
        let (db, svc) = mock_sync_service();

        let ec = EncryptedCommit {
            commit_id: "commit:remote1".into(),
            document_id: "document:remote_doc".into(),
            parent_commit: None,
            encrypted_snapshot: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                serde_json::to_string(&sovereign_db::schema::DocumentSnapshot {
                    document_id: "document:remote_doc".into(),
                    title: "Remote Doc".into(),
                    content: "synced content".into(),
                }).unwrap(),
            ),
            nonce: String::new(),
            message: "remote commit".into(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        };

        let count = svc.apply_commits(vec![ec]).await.unwrap();
        assert_eq!(count, 1);

        // Verify a document was created
        let docs = db.list_documents(None).await.unwrap();
        assert_eq!(docs.len(), 1);
    }

    #[tokio::test]
    async fn apply_commits_updates_existing_doc() {
        let (db, svc) = mock_sync_service();
        let t = db.create_thread(Thread::new("T".into(), "".into())).await.unwrap();
        let tid = t.id_string().unwrap();

        let doc = db.create_document(Document::new("Existing".into(), tid, true)).await.unwrap();
        let doc_id = doc.id_string().unwrap();

        let ec = EncryptedCommit {
            commit_id: "commit:sync1".into(),
            document_id: doc_id.clone(),
            parent_commit: None,
            encrypted_snapshot: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                serde_json::to_string(&sovereign_db::schema::DocumentSnapshot {
                    document_id: doc_id.clone(),
                    title: "Updated Title".into(),
                    content: "updated content".into(),
                }).unwrap(),
            ),
            nonce: String::new(),
            message: "sync update".into(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        };

        let count = svc.apply_commits(vec![ec]).await.unwrap();
        assert_eq!(count, 1);

        // Verify the document was updated
        let updated = db.get_document(&doc_id).await.unwrap();
        assert_eq!(updated.title, "Updated Title");
        assert_eq!(updated.content, "updated content");
    }

    #[test]
    fn content_hash_deterministic() {
        let h1 = content_hash(r#"{"body":"hello","images":[]}"#);
        let h2 = content_hash(r#"{"body":"hello","images":[]}"#);
        assert_eq!(h1, h2);
    }

    #[test]
    fn content_hash_differs_for_different_content() {
        let h1 = content_hash("hello");
        let h2 = content_hash("world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn commit_transport_roundtrip() {
        use sovereign_db::schema::DocumentSnapshot;

        let commit = Commit {
            id: None,
            document_id: "document:abc".into(),
            parent_commit: None,
            message: "initial".into(),
            timestamp: chrono::Utc::now(),
            snapshot: DocumentSnapshot {
                document_id: "document:abc".into(),
                title: "Test Doc".into(),
                content: r#"{"body":"hello","images":[]}"#.into(),
            },
        };

        let transport = commit_to_transport(&commit);
        assert_eq!(transport.document_id, "document:abc");
        assert_eq!(transport.message, "initial");
        assert!(transport.nonce.is_empty()); // plaintext marker

        let snapshot = transport_to_snapshot(&transport);
        assert_eq!(snapshot.title, "Test Doc");
        assert_eq!(snapshot.content, r#"{"body":"hello","images":[]}"#);
    }
}
