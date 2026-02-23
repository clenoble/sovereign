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
