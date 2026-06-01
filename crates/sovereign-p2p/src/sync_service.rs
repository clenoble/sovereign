use std::sync::Arc;

use sha2::{Digest, Sha256};
use sovereign_db::schema::{
    Commit, Document, Entity, PiiRecord, ShareRecord, Thread,
};
use sovereign_db::GraphDB;

use crate::error::{P2pError, P2pResult};
use crate::protocol::manifest::{
    DocumentManifestEntry, EntityManifestEntry, PiiRecordManifestEntry,
    ShareRecordManifestEntry, SyncManifest, ThreadManifestEntry,
};
use crate::protocol::sync::{EncryptedCommit, EncryptedRow, SyncTable};

/// Maximum tolerated clock skew into the future for a remote row's LWW
/// timestamp (P2P-003, cheap mitigation). Timestamps are attacker-influenced,
/// so a far-future value would otherwise silently win last-writer-wins. We
/// reject any remote row whose timestamp is more than this far ahead of our
/// own clock. The deep fix (signed monotonic counters) is deferred.
const MAX_FUTURE_SKEW: chrono::Duration = chrono::Duration::hours(24);

/// Middleware between the P2P networking layer and the database.
///
/// Owns an `Arc<dyn GraphDB>` and exposes async methods for building
/// manifests, fetching/applying commits, and checking commit ancestry.
pub struct SyncService {
    db: Arc<dyn GraphDB>,
    device_id: String,
    /// Per-account key that AEAD-seals every sync envelope on the wire
    /// (P2P-002). Derived from the shared AccountKey
    /// ([`AccountKey::derive_transport_key`]), so every paired device has
    /// the same one and can decrypt the others' rows/commits/manifests.
    transport_key: [u8; 32],
}

impl SyncService {
    pub fn new(db: Arc<dyn GraphDB>, device_id: String, transport_key: [u8; 32]) -> Self {
        Self {
            db,
            device_id,
            transport_key,
        }
    }

    /// The per-account sync transport key (P2P-002). Used by the node to
    /// encrypt/decrypt the manifest envelope.
    pub fn transport_key(&self) -> &[u8; 32] {
        &self.transport_key
    }

    /// Build a SyncManifest from all syncable tables in the database.
    /// Documents track via commit chain; threads/entities/pii_records/
    /// share_records use the row-level last-writer-wins protocol.
    pub async fn build_manifest(&self) -> P2pResult<SyncManifest> {
        let mut manifest = SyncManifest::new(self.device_id.clone());

        // --- Documents (commit-chain tracked) ---
        let docs = self
            .db
            .list_documents(None)
            .await
            .map_err(|e| P2pError::SyncError(format!("failed to list documents: {e}")))?;
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
            manifest.documents.push(DocumentManifestEntry {
                doc_id,
                head_commit: doc.head_commit.clone(),
                commit_count: commits.len() as u32,
                content_hash: content_hash(&doc.content),
                modified_at: doc.modified_at.to_rfc3339(),
                deleted_at: doc.deleted_at.clone(),
            });
        }

        // --- Threads ---
        let threads = self
            .db
            .list_threads()
            .await
            .map_err(|e| P2pError::SyncError(format!("failed to list threads: {e}")))?;
        for t in &threads {
            let id = match t.id_string() {
                Some(id) => id,
                None => continue,
            };
            manifest.threads.push(ThreadManifestEntry {
                thread_id: id,
                modified_at: t.modified_at.to_rfc3339(),
                content_hash: hash_thread(t),
                deleted_at: t.deleted_at.clone(),
            });
        }

        // --- Entities ---
        let entities = self
            .db
            .list_entities()
            .await
            .map_err(|e| P2pError::SyncError(format!("failed to list entities: {e}")))?;
        for e in &entities {
            let id = match e.id_string() {
                Some(id) => id,
                None => continue,
            };
            manifest.entities.push(EntityManifestEntry {
                entity_id: id,
                modified_at: e.modified_at.to_rfc3339(),
                content_hash: hash_entity(e),
                deleted_at: e.deleted_at.clone(),
            });
        }

        // --- PII records ---
        let pii_records = self
            .db
            .list_pii_records(None, None, None)
            .await
            .map_err(|e| P2pError::SyncError(format!("failed to list pii_records: {e}")))?;
        for r in &pii_records {
            let id = match r.id_string() {
                Some(id) => id,
                None => continue,
            };
            manifest.pii_records.push(PiiRecordManifestEntry {
                record_id: id,
                discovered_at: r.discovered_at.to_rfc3339(),
                content_hash: hash_pii_record(r),
                deleted_at: r.deleted_at.clone(),
            });
        }

        // --- Share records (append-only) ---
        let share_records = self
            .db
            .list_all_share_records()
            .await
            .map_err(|e| P2pError::SyncError(format!("failed to list share_records: {e}")))?;
        for s in &share_records {
            let id = match s.id_string() {
                Some(id) => id,
                None => continue,
            };
            manifest.share_records.push(ShareRecordManifestEntry {
                record_id: id,
                shared_at: s.shared_at.to_rfc3339(),
                content_hash: hash_share_record(s),
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

            result.push(commit_to_transport(&commit, &self.transport_key)?);
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
            None => commits
                .iter()
                .map(|c| commit_to_transport(c, &self.transport_key))
                .collect(),
            Some(since) => {
                // Commits are returned most-recent-first. Collect until we hit `since`.
                let mut result = Vec::new();
                for c in &commits {
                    if c.id_string().as_deref() == Some(since) {
                        break;
                    }
                    result.push(commit_to_transport(c, &self.transport_key)?);
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
            let snapshot = transport_to_snapshot(ec, &self.transport_key)?;

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

    // ----- Row-level sync (non-document tables, Phase 3 v0.0.5) -----

    /// Fetch rows from the local DB and package them as `EncryptedRow`s
    /// for transport. Phase 3 ships with plaintext-marker rows (empty
    /// nonce) like the existing `EncryptedCommit` track; pair-key
    /// envelope encryption is wired in alongside the orchestrator's
    /// post-login p2p start (Phase 3.6 / v0.0.5.x).
    pub async fn get_rows(
        &self,
        table: SyncTable,
        ids: &[String],
    ) -> P2pResult<Vec<EncryptedRow>> {
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            let row = match table {
                SyncTable::Thread => match self.db.get_thread(id).await {
                    Ok(t) => row_from_thread(&t, &self.transport_key)?,
                    Err(_) => continue,
                },
                SyncTable::Entity => match self.db.get_entity(id).await {
                    Ok(e) => row_from_entity(&e, &self.transport_key)?,
                    Err(_) => continue,
                },
                SyncTable::PiiRecord => match self.db.get_pii_record(id).await {
                    Ok(r) => row_from_pii_record(&r, &self.transport_key)?,
                    Err(_) => continue,
                },
                SyncTable::ShareRecord => match self.db.get_share_record(id).await {
                    Ok(s) => row_from_share_record(&s, &self.transport_key)?,
                    Err(_) => continue,
                },
            };
            out.push(row);
        }
        Ok(out)
    }

    /// Apply rows received from a peer using last-writer-wins. For tables
    /// without an `update_*` method (entity, pii_record), v0.0.5 only
    /// creates rows that don't yet exist locally; updates from remote
    /// are dropped with a tracing::debug. Returns (written, skipped).
    pub async fn apply_rows(
        &self,
        table: SyncTable,
        rows: Vec<EncryptedRow>,
    ) -> P2pResult<(u32, u32)> {
        let mut written = 0u32;
        let mut skipped = 0u32;
        for row in rows {
            let result = match table {
                SyncTable::Thread => self.apply_thread_row(&row).await,
                SyncTable::Entity => self.apply_entity_row(&row).await,
                SyncTable::PiiRecord => self.apply_pii_record_row(&row).await,
                SyncTable::ShareRecord => self.apply_share_record_row(&row).await,
            };
            match result {
                Ok(true) => written += 1,
                Ok(false) => skipped += 1,
                Err(e) => {
                    tracing::warn!(
                        "apply_rows({:?}) row {} failed: {}",
                        table,
                        row.id,
                        e
                    );
                    skipped += 1;
                }
            }
        }
        Ok((written, skipped))
    }

    async fn apply_thread_row(&self, row: &EncryptedRow) -> P2pResult<bool> {
        let remote: Thread = decode_row_inner(row, &self.transport_key)?;

        // P2P-003: reject far-future timestamps before they can win LWW.
        if remote.modified_at > chrono::Utc::now() + MAX_FUTURE_SKEW {
            tracing::warn!(
                "rejecting thread row {}: modified_at {} is beyond max future skew",
                row.id,
                remote.modified_at
            );
            return Ok(false);
        }

        match self.db.get_thread(&row.id).await {
            Ok(local) => {
                if remote.modified_at > local.modified_at {
                    // Best-effort surfacing: a remote row is overwriting an
                    // existing local row via LWW (P2P-003).
                    tracing::warn!(
                        "thread row {} overwriting local copy via LWW (remote {} > local {})",
                        row.id,
                        remote.modified_at,
                        local.modified_at
                    );
                    self.db
                        .update_thread(
                            &row.id,
                            Some(&remote.name),
                            Some(&remote.description),
                        )
                        .await
                        .map_err(|e| P2pError::SyncError(format!("update_thread: {e}")))?;
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Err(_) => {
                // Thread missing locally — create it. The mock + surreal
                // path expect to mint an ID, so we recreate the record
                // and rely on a future reconciliation pass to align
                // auto-minted IDs with the remote ID for non-document
                // tables. Documented limitation for v0.0.5.
                let _ = self
                    .db
                    .create_thread(remote)
                    .await
                    .map_err(|e| P2pError::SyncError(format!("create_thread: {e}")))?;
                Ok(true)
            }
        }
    }

    async fn apply_entity_row(&self, row: &EncryptedRow) -> P2pResult<bool> {
        let remote: Entity = decode_row_inner(row, &self.transport_key)?;
        match self.db.get_entity(&row.id).await {
            Ok(_local) => {
                // No update_entity in v0.0.5; LWW updates dropped.
                tracing::debug!(
                    "apply_entity_row: skip update for {} (entity LWW updates land in v0.0.6)",
                    row.id
                );
                Ok(false)
            }
            Err(_) => {
                let _ = self
                    .db
                    .create_entity(remote)
                    .await
                    .map_err(|e| P2pError::SyncError(format!("create_entity: {e}")))?;
                Ok(true)
            }
        }
    }

    async fn apply_pii_record_row(&self, row: &EncryptedRow) -> P2pResult<bool> {
        let remote: PiiRecord = decode_row_inner(row, &self.transport_key)?;

        // P2P-003: reject far-future timestamps before they can win LWW.
        if remote.discovered_at > chrono::Utc::now() + MAX_FUTURE_SKEW {
            tracing::warn!(
                "rejecting pii record {}: discovered_at {} is beyond max future skew",
                row.id,
                remote.discovered_at
            );
            return Ok(false);
        }

        match self.db.get_pii_record(&row.id).await {
            Ok(local) => {
                // LWW on discovered_at: if remote is newer, refresh the
                // encrypted value (the only field we have an update
                // method for). Other field changes ride along in v0.0.6.
                if remote.discovered_at > local.discovered_at {
                    // Best-effort surfacing: remote overwriting a local row.
                    tracing::warn!(
                        "pii record {} overwriting local copy via LWW (remote {} > local {})",
                        row.id,
                        remote.discovered_at,
                        local.discovered_at
                    );
                    self.db
                        .update_pii_record_value(
                            &row.id,
                            &remote.value_encrypted,
                            &remote.value_nonce,
                        )
                        .await
                        .map_err(|e| {
                            P2pError::SyncError(format!("update_pii_record_value: {e}"))
                        })?;
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Err(_) => {
                let _ = self
                    .db
                    .create_pii_record(remote)
                    .await
                    .map_err(|e| P2pError::SyncError(format!("create_pii_record: {e}")))?;
                Ok(true)
            }
        }
    }

    async fn apply_share_record_row(&self, row: &EncryptedRow) -> P2pResult<bool> {
        let remote: ShareRecord = decode_row_inner(row, &self.transport_key)?;
        // Append-only: skip if already present, otherwise create.
        if self.db.get_share_record(&row.id).await.is_ok() {
            return Ok(false);
        }
        let _ = self
            .db
            .create_share_record(remote)
            .await
            .map_err(|e| P2pError::SyncError(format!("create_share_record: {e}")))?;
        Ok(true)
    }

    /// Get the device ID for this sync service.
    pub fn device_id(&self) -> &str {
        &self.device_id
    }
}

// --- Row encode/decode helpers (Phase 3 plaintext-marker shape) ---

fn row_from_thread(t: &Thread, key: &[u8; 32]) -> P2pResult<EncryptedRow> {
    encode_row(t.id_string().unwrap_or_default(), t, t.modified_at.to_rfc3339(), t.deleted_at.clone(), key)
}

fn row_from_entity(e: &Entity, key: &[u8; 32]) -> P2pResult<EncryptedRow> {
    encode_row(e.id_string().unwrap_or_default(), e, e.modified_at.to_rfc3339(), e.deleted_at.clone(), key)
}

fn row_from_pii_record(r: &PiiRecord, key: &[u8; 32]) -> P2pResult<EncryptedRow> {
    encode_row(r.id_string().unwrap_or_default(), r, r.discovered_at.to_rfc3339(), r.deleted_at.clone(), key)
}

fn row_from_share_record(s: &ShareRecord, key: &[u8; 32]) -> P2pResult<EncryptedRow> {
    encode_row(s.id_string().unwrap_or_default(), s, s.shared_at.to_rfc3339(), None, key)
}

/// AEAD-seal a row's JSON under the transport key (P2P-002). The body is
/// XChaCha20-Poly1305 ciphertext + a real random nonce; only `id`,
/// `modified_at`, and the soft-delete marker stay in the clear (the
/// receiver needs them for LWW without decrypting).
fn encode_row<T: serde::Serialize>(
    id: String,
    row: &T,
    modified_at: String,
    deleted_at: Option<String>,
    key: &[u8; 32],
) -> P2pResult<EncryptedRow> {
    use base64::Engine;
    let json = serde_json::to_vec(row)
        .map_err(|e| P2pError::SyncError(format!("row serialize: {e}")))?;
    let (ciphertext, nonce) = sovereign_crypto::aead::encrypt(&json, key)
        .map_err(|e| P2pError::SyncError(format!("row encrypt: {e}")))?;
    Ok(EncryptedRow {
        id,
        ciphertext: base64::engine::general_purpose::STANDARD.encode(&ciphertext),
        nonce: base64::engine::general_purpose::STANDARD.encode(nonce),
        modified_at,
        deleted_at,
    })
}

fn decode_row_inner<T: serde::de::DeserializeOwned>(
    row: &EncryptedRow,
    key: &[u8; 32],
) -> P2pResult<T> {
    use base64::Engine;
    // P2P-002: encryption is mandatory. An empty nonce is the old
    // plaintext-marker shape — reject it rather than trust unsealed data.
    if row.nonce.is_empty() {
        return Err(P2pError::SyncError(
            "rejecting plaintext sync row (empty nonce); transport encryption is mandatory".into(),
        ));
    }
    let ciphertext = base64::engine::general_purpose::STANDARD
        .decode(&row.ciphertext)
        .map_err(|e| P2pError::SyncError(format!("row base64 ct: {e}")))?;
    let nonce_bytes = base64::engine::general_purpose::STANDARD
        .decode(&row.nonce)
        .map_err(|e| P2pError::SyncError(format!("row base64 nonce: {e}")))?;
    if nonce_bytes.len() != 24 {
        return Err(P2pError::SyncError(format!(
            "row nonce wrong length: {}",
            nonce_bytes.len()
        )));
    }
    let mut nonce = [0u8; 24];
    nonce.copy_from_slice(&nonce_bytes);
    let plaintext = sovereign_crypto::aead::decrypt(&ciphertext, &nonce, key)
        .map_err(|e| P2pError::SyncError(format!("row decrypt: {e}")))?;
    serde_json::from_slice(&plaintext)
        .map_err(|e| P2pError::SyncError(format!("row decode: {e}")))
}

/// Compute a SHA-256 hash of document content.
pub fn content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// SHA-256 of a row's syncable fields. Excludes timestamps used for
/// LWW so two devices that converged on the same value via different
/// histories produce the same hash and short-circuit the diff to
/// "in sync".
fn hash_thread(t: &Thread) -> String {
    let mut h = Sha256::new();
    h.update(t.name.as_bytes());
    h.update(t.description.as_bytes());
    if let Some(ref d) = t.deleted_at {
        h.update(b"|deleted:");
        h.update(d.as_bytes());
    }
    format!("{:x}", h.finalize())
}

fn hash_entity(e: &Entity) -> String {
    let mut h = Sha256::new();
    h.update(e.name.as_bytes());
    // Stable serialization for collections — sort first.
    let mut domains = e.domains.clone();
    domains.sort();
    for d in &domains {
        h.update(b"|d:");
        h.update(d.as_bytes());
    }
    h.update(b"|kind:");
    h.update(format!("{:?}", e.kind).as_bytes());
    h.update(b"|owned:");
    h.update(if e.is_owned { b"1" as &[u8] } else { b"0" });
    if let Some(ref d) = e.deleted_at {
        h.update(b"|deleted:");
        h.update(d.as_bytes());
    }
    format!("{:x}", h.finalize())
}

fn hash_pii_record(r: &PiiRecord) -> String {
    let mut h = Sha256::new();
    h.update(format!("{:?}", r.kind).as_bytes());
    h.update(b"|enc:");
    h.update(r.value_encrypted.as_bytes());
    h.update(b"|nonce:");
    h.update(r.value_nonce.as_bytes());
    h.update(b"|entity:");
    h.update(r.entity_id.as_deref().unwrap_or("").as_bytes());
    h.update(b"|review:");
    h.update(format!("{:?}", r.review_state).as_bytes());
    if let Some(ref d) = r.deleted_at {
        h.update(b"|deleted:");
        h.update(d.as_bytes());
    }
    format!("{:x}", h.finalize())
}

fn hash_share_record(s: &ShareRecord) -> String {
    let mut h = Sha256::new();
    h.update(s.pii_record_id.as_bytes());
    h.update(b"|to:");
    h.update(s.to_entity_id.as_bytes());
    h.update(b"|chan:");
    h.update(format!("{:?}", s.channel).as_bytes());
    format!("{:x}", h.finalize())
}

/// Convert a DB Commit to the transport format, AEAD-sealing the snapshot
/// under the transport key (P2P-002). `message`/`timestamp`/parent stay in
/// the clear as commit-chain metadata; the document title + content (the
/// snapshot) are encrypted.
fn commit_to_transport(commit: &Commit, key: &[u8; 32]) -> P2pResult<EncryptedCommit> {
    use base64::Engine;
    let snapshot_json = serde_json::to_vec(&commit.snapshot)
        .map_err(|e| P2pError::SyncError(format!("snapshot serialize: {e}")))?;
    let (ciphertext, nonce) = sovereign_crypto::aead::encrypt(&snapshot_json, key)
        .map_err(|e| P2pError::SyncError(format!("snapshot encrypt: {e}")))?;
    Ok(EncryptedCommit {
        commit_id: commit.id_string().unwrap_or_default(),
        document_id: commit.document_id.clone(),
        parent_commit: commit.parent_commit.clone(),
        encrypted_snapshot: base64::engine::general_purpose::STANDARD.encode(&ciphertext),
        nonce: base64::engine::general_purpose::STANDARD.encode(nonce),
        message: commit.message.clone(),
        timestamp: commit.timestamp.to_rfc3339(),
    })
}

/// Decrypt a transport EncryptedCommit back to a DocumentSnapshot.
/// Rejects the old empty-nonce plaintext shape (P2P-002).
fn transport_to_snapshot(
    ec: &EncryptedCommit,
    key: &[u8; 32],
) -> P2pResult<sovereign_db::schema::DocumentSnapshot> {
    use base64::Engine;
    if ec.nonce.is_empty() {
        return Err(P2pError::SyncError(
            "rejecting plaintext commit snapshot (empty nonce); transport encryption is mandatory"
                .into(),
        ));
    }
    let ciphertext = base64::engine::general_purpose::STANDARD
        .decode(&ec.encrypted_snapshot)
        .map_err(|e| P2pError::SyncError(format!("snapshot base64 ct: {e}")))?;
    let nonce_bytes = base64::engine::general_purpose::STANDARD
        .decode(&ec.nonce)
        .map_err(|e| P2pError::SyncError(format!("snapshot base64 nonce: {e}")))?;
    if nonce_bytes.len() != 24 {
        return Err(P2pError::SyncError(format!(
            "snapshot nonce wrong length: {}",
            nonce_bytes.len()
        )));
    }
    let mut nonce = [0u8; 24];
    nonce.copy_from_slice(&nonce_bytes);
    let plaintext = sovereign_crypto::aead::decrypt(&ciphertext, &nonce, key)
        .map_err(|e| P2pError::SyncError(format!("snapshot decrypt: {e}")))?;
    serde_json::from_slice(&plaintext)
        .map_err(|e| P2pError::SyncError(format!("snapshot decode: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sovereign_db::mock::MockGraphDB;
    use sovereign_db::schema::{Document, Thread};

    fn mock_sync_service() -> (Arc<MockGraphDB>, SyncService) {
        let db = Arc::new(MockGraphDB::new());
        let svc = SyncService::new(db.clone(), "device-1".into(), [7u8; 32]);
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
        assert_eq!(manifest.documents.len(), 2);
    }

    #[tokio::test]
    async fn build_manifest_empty_db() {
        let (_db, svc) = mock_sync_service();
        let manifest = svc.build_manifest().await.unwrap();
        assert!(manifest.documents.is_empty());
        assert!(manifest.threads.is_empty());
        assert!(manifest.entities.is_empty());
        assert!(manifest.pii_records.is_empty());
        assert!(manifest.share_records.is_empty());
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

        // Seal the commit under the mock service's transport key ([7u8;32])
        // so apply_commits can decrypt it (P2P-002).
        let ec = commit_to_transport(
            &Commit {
                id: None,
                document_id: "document:remote_doc".into(),
                parent_commit: None,
                message: "remote commit".into(),
                timestamp: chrono::Utc::now(),
                snapshot: sovereign_db::schema::DocumentSnapshot {
                    document_id: "document:remote_doc".into(),
                    title: "Remote Doc".into(),
                    content: "synced content".into(),
                },
            },
            &[7u8; 32],
        )
        .unwrap();

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

        let ec = commit_to_transport(
            &Commit {
                id: None,
                document_id: doc_id.clone(),
                parent_commit: None,
                message: "sync update".into(),
                timestamp: chrono::Utc::now(),
                snapshot: sovereign_db::schema::DocumentSnapshot {
                    document_id: doc_id.clone(),
                    title: "Updated Title".into(),
                    content: "updated content".into(),
                },
            },
            &[7u8; 32],
        )
        .unwrap();

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

        let key = [9u8; 32];
        let transport = commit_to_transport(&commit, &key).unwrap();
        assert_eq!(transport.document_id, "document:abc");
        assert_eq!(transport.message, "initial");
        assert!(!transport.nonce.is_empty(), "snapshot is AEAD-sealed (P2P-002)");

        let snapshot = transport_to_snapshot(&transport, &key).unwrap();
        assert_eq!(snapshot.title, "Test Doc");
        assert_eq!(snapshot.content, r#"{"body":"hello","images":[]}"#);

        // Wrong key cannot decrypt; plaintext (empty nonce) is refused.
        assert!(transport_to_snapshot(&transport, &[0u8; 32]).is_err());
        let mut plain = transport.clone();
        plain.nonce = String::new();
        assert!(transport_to_snapshot(&plain, &key).is_err());
    }

    #[test]
    fn row_encrypt_roundtrip_hides_plaintext_and_rejects_unsealed() {
        // P2P-002: a row's body is AEAD-sealed on the wire — the plaintext
        // never appears in the ciphertext, a wrong key can't open it, and an
        // empty-nonce (plaintext-marker) row is refused.
        use base64::Engine;
        let key = [5u8; 32];
        let t = Thread::new("Secret-Name".into(), "desc".into());
        let row = encode_row(
            "thread:1".into(),
            &t,
            "2026-01-01T00:00:00Z".into(),
            None,
            &key,
        )
        .unwrap();
        assert!(!row.nonce.is_empty(), "row must carry a real nonce");
        let ct = base64::engine::general_purpose::STANDARD
            .decode(&row.ciphertext)
            .unwrap();
        assert!(
            !String::from_utf8_lossy(&ct).contains("Secret-Name"),
            "row name must not appear in the ciphertext"
        );

        let back: Thread = decode_row_inner(&row, &key).unwrap();
        assert_eq!(back.name, "Secret-Name");

        assert!(
            decode_row_inner::<Thread>(&row, &[6u8; 32]).is_err(),
            "wrong key must fail to decrypt"
        );

        let mut plain = row.clone();
        plain.nonce = String::new();
        assert!(
            decode_row_inner::<Thread>(&plain, &key).is_err(),
            "plaintext-marker row must be rejected"
        );
    }

    // ── P2P-003: future-skew clock-forge mitigation ──────────────────────

    #[tokio::test]
    async fn apply_thread_row_rejects_far_future_timestamp() {
        let (db, svc) = mock_sync_service();
        let local = db
            .create_thread(Thread::new("Original".into(), "orig desc".into()))
            .await
            .unwrap();
        let tid = local.id_string().unwrap();

        // Forge a remote row whose modified_at is 48h in the future.
        let mut forged = db.get_thread(&tid).await.unwrap();
        forged.name = "Forged".into();
        forged.modified_at = chrono::Utc::now() + chrono::Duration::hours(48);
        let row = row_from_thread(&forged, &[7u8; 32]).unwrap();

        let applied = svc.apply_thread_row(&row).await.unwrap();
        assert!(!applied, "far-future row must be rejected (skipped)");

        // Local row must be unchanged.
        let after = db.get_thread(&tid).await.unwrap();
        assert_eq!(after.name, "Original");
    }

    #[tokio::test]
    async fn apply_thread_row_accepts_normal_newer_timestamp() {
        let (db, svc) = mock_sync_service();
        let local = db
            .create_thread(Thread::new("Original".into(), "orig desc".into()))
            .await
            .unwrap();
        let tid = local.id_string().unwrap();

        // A slightly-newer remote row (within skew) still wins LWW.
        let mut newer = db.get_thread(&tid).await.unwrap();
        newer.name = "Updated".into();
        newer.modified_at = chrono::Utc::now() + chrono::Duration::seconds(5);
        let row = row_from_thread(&newer, &[7u8; 32]).unwrap();

        let applied = svc.apply_thread_row(&row).await.unwrap();
        assert!(applied, "slightly-newer row should apply");
        let after = db.get_thread(&tid).await.unwrap();
        assert_eq!(after.name, "Updated");
    }

    #[tokio::test]
    async fn apply_pii_record_row_rejects_far_future_timestamp() {
        use sovereign_db::schema::{PiiKind, ReviewState};

        let (db, svc) = mock_sync_service();
        let local = db
            .create_pii_record(PiiRecord {
                id: None,
                kind: PiiKind::Email,
                value_encrypted: "ENC_ORIGINAL".into(),
                value_nonce: "NONCE_ORIGINAL".into(),
                label: None,
                entity_id: None,
                stored_secret: true,
                confidence: 1.0,
                sources: vec![],
                discovered_at: chrono::Utc::now(),
                last_revealed_at: None,
                use_count: 0,
                review_state: ReviewState::Confirmed,
                deleted_at: None,
            })
            .await
            .unwrap();
        let pid = local.id_string().unwrap();

        // Forge a remote row 48h in the future trying to overwrite the value.
        let mut forged = db.get_pii_record(&pid).await.unwrap();
        forged.value_encrypted = "ENC_FORGED".into();
        forged.discovered_at = chrono::Utc::now() + chrono::Duration::hours(48);
        let row = row_from_pii_record(&forged, &[7u8; 32]).unwrap();

        let applied = svc.apply_pii_record_row(&row).await.unwrap();
        assert!(!applied, "far-future pii row must be rejected (skipped)");

        let after = db.get_pii_record(&pid).await.unwrap();
        assert_eq!(after.value_encrypted, "ENC_ORIGINAL");
    }
}
