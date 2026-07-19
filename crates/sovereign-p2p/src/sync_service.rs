use std::sync::{Arc, Mutex};

use libp2p::identity::{Keypair, PublicKey};
use libp2p::PeerId;
use sha2::{Digest, Sha256};
use sovereign_db::schema::{
    Contact, Conversation, Document, Entity, Message, Milestone, PiiRecord, RelatedTo,
    ShareRecord, SuggestedLink, Thread,
};
#[cfg(test)]
use sovereign_db::schema::Commit;
use sovereign_db::GraphDB;

use crate::error::{P2pError, P2pResult};
use crate::protocol::manifest::{
    DocumentManifestEntry, EntityManifestEntry, PiiRecordManifestEntry, RowManifestEntry,
    ShareRecordManifestEntry, SyncManifest, ThreadManifestEntry,
};
use crate::protocol::sync::{EncryptedCommit, EncryptedRow, SyncTable};
use crate::version_store::{RowVersion, VersionStore};

/// Maximum tolerated clock skew into the future for a remote row's LWW
/// timestamp (P2P-003, cheap mitigation). Timestamps are attacker-influenced,
/// so a far-future value would otherwise silently win last-writer-wins. We
/// reject any remote row whose timestamp is more than this far ahead of our
/// own clock. The deep fix (signed monotonic counters) is deferred.
const MAX_FUTURE_SKEW: chrono::Duration = chrono::Duration::hours(24);

/// Maximum amount a remote row's Lamport `version_counter` may exceed our
/// local high-water (`VersionStore::own_counter`) in a single apply.
///
/// `p2p-no-per-doc-authz`: the envelope signature only proves the *sender*
/// signed the row, and a paired peer freely chooses `version_counter` and
/// signs it with its own key — so neither the signature nor the
/// `version_device == sender` gate stops a forged-high counter (e.g.
/// `u64::MAX`) from winning every `(counter, device_id)` LWW race and
/// silently overwriting any record cluster-wide. We bound the trusted
/// advance: an honest device's counter is a Lamport clock that bumps by one
/// per local edit and merges to the max it has seen, so it never leaps this
/// far ahead of what we've already synced. This kills the instant-win forge;
/// the non-destructive apply path (preserve prior version + LLM audit +
/// surface) covers the residual bounded-overwrite case.
const MAX_COUNTER_ADVANCE: u64 = 1_000_000;

/// Middleware between the P2P networking layer and the database.
///
/// Owns an `Arc<dyn GraphDB>` and exposes async methods for building
/// manifests, fetching/applying commits, and checking commit ancestry.
pub struct SyncService {
    db: Arc<dyn GraphDB>,
    device_id: String,
    /// Per-account key that AEAD-seals the **manifest** envelope (P2P-002).
    /// Derived from the shared AccountKey
    /// ([`AccountKey::derive_transport_key`]), so every paired device has
    /// the same one. Row/commit envelopes use the per-pair keys instead
    /// (P1.4 / P2P-005).
    transport_key: [u8; 32],
    /// This device's libp2p identity keypair. Every outgoing row envelope
    /// is Ed25519-signed with it (P1.3 / P2P-003); receivers verify
    /// against the public key embedded in the sender's PeerId.
    keypair: Keypair,
    /// Per-device Lamport clock + per-row version map (P1.3). Guards the
    /// row-level LWW so resolution orders by signed `(counter, device_id)`
    /// stamps instead of forgeable wall-clock timestamps. std Mutex —
    /// never held across an await.
    versions: Mutex<VersionStore>,
    /// `peer_id → pair key` (P1.4 / P2P-005): the AEAD key sealing every
    /// row/commit envelope exchanged with that specific peer. Populated
    /// from the PairingManager at startup and on pairing changes via
    /// [`Self::set_pair_keys`]. A peer with no entry fails CLOSED — no
    /// data can be sealed for or unsealed from it.
    pair_keys: std::sync::RwLock<std::collections::HashMap<String, [u8; 32]>>,
}

impl SyncService {
    pub fn new(
        db: Arc<dyn GraphDB>,
        device_id: String,
        transport_key: [u8; 32],
        keypair: Keypair,
        versions: VersionStore,
    ) -> Self {
        Self {
            db,
            device_id,
            transport_key,
            keypair,
            versions: Mutex::new(versions),
            pair_keys: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    /// The per-account sync transport key (P2P-002). Used by the node to
    /// encrypt/decrypt the manifest envelope.
    pub fn transport_key(&self) -> &[u8; 32] {
        &self.transport_key
    }

    /// Replace the per-pair key map (P1.4). Called at startup from the
    /// persisted PairingManager and whenever a device is paired/unpaired.
    pub fn set_pair_keys(&self, keys: std::collections::HashMap<String, [u8; 32]>) {
        *self.pair_keys.write().expect("pair key lock poisoned") = keys;
    }

    /// Install a single pair key without disturbing the rest of the map.
    /// Used by the P3.1 pairing handshake so a freshly paired device can
    /// sync immediately, before the app has persisted the pairing list.
    pub fn add_pair_key(&self, peer_id: String, key: [u8; 32]) {
        self.pair_keys
            .write()
            .expect("pair key lock poisoned")
            .insert(peer_id, key);
    }

    /// The sealing key for envelopes exchanged with `peer`. Fails closed:
    /// without a pair key no row or commit can leave for — or be accepted
    /// from — that peer.
    fn pair_key_for(&self, peer: &PeerId) -> P2pResult<[u8; 32]> {
        self.pair_keys
            .read()
            .expect("pair key lock poisoned")
            .get(&peer.to_string())
            .copied()
            .ok_or_else(|| {
                P2pError::SyncError(format!(
                    "no pair key for peer {peer}; refusing to seal/unseal sync data (P2P-005)"
                ))
            })
    }

    /// Build a SyncManifest from all syncable tables in the database.
    /// Documents track via commit chain; threads/entities/pii_records/
    /// share_records use the row-level last-writer-wins protocol.
    ///
    /// H-p2p1: soft-deleted rows are INCLUDED (via the `*_including_deleted`
    /// sync-scope lists) with their LWW timestamp advanced to the deletion
    /// time. A tombstone omitted from the manifest looks merely absent, so a
    /// peer still holding the row live re-pushes it and resurrects deleted
    /// data — including deleted PII.
    pub async fn build_manifest(&self) -> P2pResult<SyncManifest> {
        let mut manifest = SyncManifest::new(self.device_id.clone());

        // --- Documents (commit-chain tracked) ---
        let docs = self
            .db
            .list_documents_including_deleted()
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
            // The deleted state must be part of the content identity, or two
            // copies with equal content but different deleted state read as
            // "in sync" and the deletion never propagates.
            let mut ch = content_hash(&doc.content);
            if doc.deleted_at.is_some() {
                ch.push_str(":tombstone");
            }
            manifest.documents.push(DocumentManifestEntry {
                doc_id,
                head_commit: doc.head_commit.clone(),
                commit_count: commits.len() as u32,
                content_hash: ch,
                modified_at: lww_ts(doc.modified_at.to_rfc3339(), &doc.deleted_at),
                deleted_at: doc.deleted_at.clone(),
            });
        }

        // --- Threads ---
        let threads = self
            .db
            .list_threads_including_deleted()
            .await
            .map_err(|e| P2pError::SyncError(format!("failed to list threads: {e}")))?;
        for t in &threads {
            let id = match t.id_string() {
                Some(id) => id,
                None => continue,
            };
            manifest.threads.push(ThreadManifestEntry {
                thread_id: id,
                modified_at: lww_ts(t.modified_at.to_rfc3339(), &t.deleted_at),
                content_hash: hash_thread(t),
                deleted_at: t.deleted_at.clone(),
            });
        }

        // --- Entities ---
        let entities = self
            .db
            .list_entities_including_deleted()
            .await
            .map_err(|e| P2pError::SyncError(format!("failed to list entities: {e}")))?;
        for e in &entities {
            let id = match e.id_string() {
                Some(id) => id,
                None => continue,
            };
            manifest.entities.push(EntityManifestEntry {
                entity_id: id,
                modified_at: lww_ts(e.modified_at.to_rfc3339(), &e.deleted_at),
                content_hash: hash_entity(e),
                deleted_at: e.deleted_at.clone(),
            });
        }

        // --- PII records ---
        let pii_records = self
            .db
            .list_pii_records_including_deleted()
            .await
            .map_err(|e| P2pError::SyncError(format!("failed to list pii_records: {e}")))?;
        for r in &pii_records {
            let id = match r.id_string() {
                Some(id) => id,
                None => continue,
            };
            manifest.pii_records.push(PiiRecordManifestEntry {
                record_id: id,
                discovered_at: lww_ts(r.discovered_at.to_rfc3339(), &r.deleted_at),
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

        // --- P2 tables (generic row entries) ---

        let contacts = self
            .db
            .list_contacts_including_deleted()
            .await
            .map_err(|e| P2pError::SyncError(format!("failed to list contacts: {e}")))?;
        for c in &contacts {
            let Some(id) = c.id_string() else { continue };
            manifest.contacts.push(RowManifestEntry {
                id,
                modified_at: lww_ts(c.modified_at.to_rfc3339(), &c.deleted_at),
                content_hash: hash_contact(c),
                deleted_at: c.deleted_at.clone(),
            });
        }

        let messages = self
            .db
            .list_messages_including_deleted()
            .await
            .map_err(|e| P2pError::SyncError(format!("failed to list messages: {e}")))?;
        for m in &messages {
            let Some(id) = m.id_string() else { continue };
            manifest.messages.push(RowManifestEntry {
                id,
                modified_at: lww_ts(m.created_at.to_rfc3339(), &m.deleted_at),
                content_hash: hash_message(m),
                deleted_at: m.deleted_at.clone(),
            });
        }

        let conversations = self
            .db
            .list_conversations_including_deleted()
            .await
            .map_err(|e| P2pError::SyncError(format!("failed to list conversations: {e}")))?;
        for v in &conversations {
            let Some(id) = v.id_string() else { continue };
            manifest.conversations.push(RowManifestEntry {
                id,
                modified_at: lww_ts(
                    v.last_message_at.unwrap_or(v.created_at).to_rfc3339(),
                    &v.deleted_at,
                ),
                content_hash: hash_conversation(v),
                deleted_at: v.deleted_at.clone(),
            });
        }

        let milestones = self
            .db
            .list_all_milestones()
            .await
            .map_err(|e| P2pError::SyncError(format!("failed to list milestones: {e}")))?;
        for ms in &milestones {
            let Some(id) = ms.id_string() else { continue };
            manifest.milestones.push(RowManifestEntry {
                id,
                modified_at: ms.timestamp.to_rfc3339(),
                content_hash: hash_milestone(ms),
                deleted_at: None,
            });
        }

        let relationships = self
            .db
            .list_all_relationships()
            .await
            .map_err(|e| P2pError::SyncError(format!("failed to list relationships: {e}")))?;
        for r in &relationships {
            let Some(id) = r.id_string() else { continue };
            manifest.relationships.push(RowManifestEntry {
                id,
                modified_at: r.created_at.to_rfc3339(),
                content_hash: hash_relationship(r),
                deleted_at: None,
            });
        }

        let suggested_links = self
            .db
            .list_all_suggested_links()
            .await
            .map_err(|e| P2pError::SyncError(format!("failed to list suggested_links: {e}")))?;
        for l in &suggested_links {
            let Some(id) = l.id_string() else { continue };
            manifest.suggested_links.push(RowManifestEntry {
                id,
                modified_at: l.resolved_at.unwrap_or(l.created_at).to_rfc3339(),
                content_hash: hash_suggested_link(l),
                deleted_at: None,
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

    /// Package the current plaintext state of the documents owning the
    /// given head-commit ids, for transport to `peer`.
    ///
    /// Plaintext-on-sync-boundary model: content is read decrypted (the db
    /// handle is the encrypting layer), then the snapshot is AEAD-sealed
    /// under the pair key for that peer (P1.4 / P2P-005). The receiver
    /// re-encrypts under its own local keys. `commit_ids` are the remote
    /// manifest's per-document head commits; each maps back to its document.
    pub async fn get_commits(
        &self,
        commit_ids: &[String],
        peer: &PeerId,
    ) -> P2pResult<Vec<EncryptedCommit>> {
        let key = self.pair_key_for(peer)?;
        let mut result = Vec::with_capacity(commit_ids.len());
        for commit_id in commit_ids {
            let commit = self
                .db
                .get_commit(commit_id)
                .await
                .map_err(|e| P2pError::SyncError(format!("failed to get commit {commit_id}: {e}")))?;
            result.push(
                self.seal_document_state(&commit.document_id, Some(commit_id), &key)
                    .await?,
            );
        }
        Ok(result)
    }

    /// Package the current plaintext state of a single document for
    /// transport to `peer`. `since_commit` is accepted for protocol
    /// compatibility but is unused — documents sync as content-LWW
    /// (current state), not as a replayed commit chain (which can't cross
    /// the per-device at-rest key boundary). Per-device commit history
    /// stays local.
    pub async fn get_commits_since(
        &self,
        doc_id: &str,
        _since_commit: Option<&str>,
        peer: &PeerId,
    ) -> P2pResult<Vec<EncryptedCommit>> {
        let key = self.pair_key_for(peer)?;
        Ok(vec![self.seal_document_state(doc_id, None, &key).await?])
    }

    /// Read a document's current decrypted state and seal it as an
    /// `EncryptedCommit` under the given pair key.
    async fn seal_document_state(
        &self,
        doc_id: &str,
        head_commit: Option<&str>,
        key: &[u8; 32],
    ) -> P2pResult<EncryptedCommit> {
        let doc = self
            .db
            .get_document(doc_id)
            .await
            .map_err(|e| P2pError::SyncError(format!("get_document {doc_id}: {e}")))?;
        let snapshot = sovereign_db::schema::DocumentSnapshot {
            document_id: doc_id.to_string(),
            title: doc.title,
            content: doc.content,
            thread_id: doc.thread_id,
            // Transport snapshots carry PLAINTEXT state (sealed under the
            // pair key, re-encrypted by the receiver under its own keys) —
            // the at-rest nonce fields (C1) don't apply here.
            content_nonce: None,
            title_nonce: None,
            title_token_hashes: Vec::new(),
            // H-p2p1: the deleted state travels with the snapshot so the
            // receiving device converges on the deletion.
            deleted_at: doc.deleted_at.clone(),
        };
        let mut commit = seal_snapshot(
            head_commit.unwrap_or("").to_string(),
            doc_id.to_string(),
            doc.modified_at.to_rfc3339(),
            snapshot,
            key,
        )?;
        // AUTOCOMMIT-001 / P2P-001: stamp our authoring identity (PeerId) and
        // sign the envelope so the receiver can verify authorship — a paired
        // peer can't forge a document update "from" another device.
        commit.version_device = self.device_id.clone();
        sign_commit(&mut commit, &self.keypair)?;
        Ok(commit)
    }

    /// Apply received document states to the local database (content-LWW).
    ///
    /// The envelopes must unseal under the pair key shared with `sender`
    /// (P1.4 / P2P-005) — a peer we hold no pair key for is refused. The
    /// manifest diff already decided direction (we only receive docs the
    /// remote is newer on, or that we lack), so each received state is
    /// upserted: a missing document is created **under its origin id**
    /// (`create_document_with_id`) so it doesn't duplicate on the next sync;
    /// an existing one is overwritten with the newer content. Plaintext
    /// content is re-encrypted at rest by the db's encrypting layer. Returns
    /// the number of documents written.
    pub async fn apply_commits(
        &self,
        commits: Vec<EncryptedCommit>,
        sender: &PeerId,
    ) -> P2pResult<u32> {
        let key = self.pair_key_for(sender)?;
        let sender_key = match public_key_from_peer_id(sender) {
            Some(k) => k,
            None => {
                return Err(P2pError::SyncError(format!(
                    "cannot extract a public key from sender peer id {sender}; rejecting commits"
                )));
            }
        };
        let sender_id = sender.to_string();
        let mut docs_updated = std::collections::HashSet::new();

        // Apply in timestamp order so the newest state lands last.
        let mut sorted = commits;
        sorted.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        for ec in &sorted {
            // AUTOCOMMIT-001 / P2P-001: the envelope must carry a valid Ed25519
            // signature by the sender AND claim the sender as its author. This
            // stops a paired peer from forging a document update "from" another
            // device, and rejects unsigned (pre-signing) commits.
            if !verify_commit(ec, &sender_key) {
                tracing::warn!(
                    "rejecting commit {} for doc {}: missing or invalid signature",
                    ec.commit_id,
                    ec.document_id
                );
                continue;
            }
            if ec.version_device != sender_id {
                tracing::warn!(
                    "rejecting commit {} for doc {}: version_device '{}' != verified sender (authorship forgery)",
                    ec.commit_id,
                    ec.document_id,
                    ec.version_device
                );
                continue;
            }
            // P2P-001: also reject a commit whose timestamp is implausibly far in
            // the future (or unparseable) — bounds clock-forge LWW wins. Content
            // integrity + authorship are covered by the signature above; this is
            // belt-and-suspenders for the modified_at content-LWW resolution.
            match chrono::DateTime::parse_from_rfc3339(&ec.timestamp) {
                Ok(ts) if ts.with_timezone(&chrono::Utc) <= chrono::Utc::now() + MAX_FUTURE_SKEW => {}
                _ => {
                    tracing::warn!(
                        "rejecting commit {} for doc {}: timestamp '{}' is unparseable or beyond max future skew",
                        ec.commit_id,
                        ec.document_id,
                        ec.timestamp
                    );
                    continue;
                }
            }

            let snapshot = transport_to_snapshot(ec, &key)?;

            if let Ok(local) = self.db.get_document(&ec.document_id).await {
                // p2p-no-per-doc-authz: a paired peer's overwrite is
                // NON-DESTRUCTIVE. Skip identical re-syncs (the common case —
                // every round re-pushes); otherwise snapshot the current local
                // version as a commit FIRST so it stays restorable, apply the
                // peer's content, then flag the doc so the change is surfaced
                // for review on next open instead of silently replacing it.
                let content_changed =
                    local.title != snapshot.title || local.content != snapshot.content;
                let remote_deleted = snapshot.deleted_at.is_some();
                let locally_deleted = local.deleted_at.is_some();
                if content_changed {
                    let prior_commit = self
                        .db
                        .commit_document(&ec.document_id, "pre-sync snapshot (peer overwrite)")
                        .await
                        .ok()
                        .and_then(|c| c.id_string());
                    self.db
                        .update_document(
                            &ec.document_id,
                            Some(&snapshot.title),
                            Some(&snapshot.content),
                        )
                        .await
                        .map_err(|e| P2pError::SyncError(format!("failed to update doc: {e}")))?;
                    // Best-effort: a failure to flag must not abort the sync,
                    // but the content is already preserved in the commit above.
                    if let Err(e) = self
                        .db
                        .set_document_peer_review(
                            &ec.document_id,
                            &sender_id,
                            prior_commit.as_deref(),
                        )
                        .await
                    {
                        tracing::warn!(
                            "failed to flag doc {} for peer review: {e}",
                            ec.document_id
                        );
                    }
                    docs_updated.insert(ec.document_id.clone());
                }
                // H-p2p1: converge on the deleted state. Deletion is
                // NON-DESTRUCTIVE too — soft-delete only (trash, restorable),
                // preceded by a pre-delete commit, and flagged for review
                // like any other peer overwrite.
                if remote_deleted && !locally_deleted {
                    let prior_commit = self
                        .db
                        .commit_document(&ec.document_id, "pre-sync snapshot (peer delete)")
                        .await
                        .ok()
                        .and_then(|c| c.id_string());
                    self.db
                        .soft_delete_document(&ec.document_id)
                        .await
                        .map_err(|e| P2pError::SyncError(format!("peer soft-delete: {e}")))?;
                    if let Err(e) = self
                        .db
                        .set_document_peer_review(
                            &ec.document_id,
                            &sender_id,
                            prior_commit.as_deref(),
                        )
                        .await
                    {
                        tracing::warn!(
                            "failed to flag doc {} deletion for peer review: {e}",
                            ec.document_id
                        );
                    }
                    docs_updated.insert(ec.document_id.clone());
                } else if !remote_deleted && locally_deleted {
                    // The winning remote state is live (e.g. the user restored
                    // the doc from trash on the other device) — un-delete.
                    self.db
                        .restore_soft_deleted_document(&ec.document_id)
                        .await
                        .map_err(|e| P2pError::SyncError(format!("peer un-delete: {e}")))?;
                    docs_updated.insert(ec.document_id.clone());
                }
            } else {
                // Recreate the document under its ORIGIN id so both devices
                // agree on the identity (no duplication on re-sync).
                let id = sovereign_db::schema::raw_to_thing(&ec.document_id).ok_or_else(|| {
                    P2pError::SyncError(format!("bad document id {}", ec.document_id))
                })?;
                // A document synced from a paired device is the user's OWN
                // content (same account / AccountKey), so it's owned — not
                // external. Marking it `false` here made it render with the
                // external (parallelogram) provenance cue on the receiver.
                // Thread membership now rides in the sealed snapshot; older
                // commits predate the field and decode to "" → fall back to
                // "default" so they still land somewhere renderable.
                let thread_id = if snapshot.thread_id.is_empty() {
                    "default".to_string()
                } else {
                    snapshot.thread_id.clone()
                };
                let mut doc = Document::new(snapshot.title.clone(), thread_id, true);
                doc.id = Some(id);
                doc.content = snapshot.content.clone();
                // H-p2p1: a snapshot of a deleted document lands as a
                // tombstone, not as a live resurrection.
                doc.deleted_at = snapshot.deleted_at.clone();
                self.db
                    .create_document_with_id(doc)
                    .await
                    .map_err(|e| P2pError::SyncError(format!("failed to create doc: {e}")))?;
                docs_updated.insert(ec.document_id.clone());
            }
        }

        Ok(docs_updated.len() as u32)
    }

    // ----- Row-level sync (non-document tables, Phase 3 v0.0.5) -----

    /// Fetch rows from the local DB and package them as `EncryptedRow`s
    /// for transport to `peer`: AEAD-sealed under the pair key (P1.4 /
    /// P2P-005), stamped with this device's Lamport version for the row
    /// (P1.3 — a locally-edited row gets the next counter), and
    /// Ed25519-signed with the device identity key.
    pub async fn get_rows(
        &self,
        table: SyncTable,
        ids: &[String],
        peer: &PeerId,
    ) -> P2pResult<Vec<EncryptedRow>> {
        let key = self.pair_key_for(peer)?;
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            let (mut row, content_hash) = match table {
                SyncTable::Thread => match self.db.get_thread(id).await {
                    Ok(t) => (row_from_thread(&t, &key)?, hash_thread(&t)),
                    Err(_) => continue,
                },
                SyncTable::Entity => match self.db.get_entity(id).await {
                    Ok(e) => (row_from_entity(&e, &key)?, hash_entity(&e)),
                    Err(_) => continue,
                },
                SyncTable::PiiRecord => match self.db.get_pii_record(id).await {
                    Ok(r) => (
                        row_from_pii_record(&r, &key)?,
                        hash_pii_record(&r),
                    ),
                    Err(_) => continue,
                },
                SyncTable::ShareRecord => match self.db.get_share_record(id).await {
                    Ok(s) => (
                        row_from_share_record(&s, &key)?,
                        hash_share_record(&s),
                    ),
                    Err(_) => continue,
                },
                SyncTable::Contact => match self.db.get_contact(id).await {
                    Ok(c) => (row_from_contact(&c, &key)?, hash_contact(&c)),
                    Err(_) => continue,
                },
                SyncTable::Message => match self.db.get_message(id).await {
                    Ok(m) => (row_from_message(&m, &key)?, hash_message(&m)),
                    Err(_) => continue,
                },
                SyncTable::Conversation => match self.db.get_conversation(id).await {
                    Ok(v) => (row_from_conversation(&v, &key)?, hash_conversation(&v)),
                    Err(_) => continue,
                },
                SyncTable::Milestone => match self.db.get_milestone(id).await {
                    Ok(ms) => (row_from_milestone(&ms, &key)?, hash_milestone(&ms)),
                    Err(_) => continue,
                },
                SyncTable::Relationship => match self.db.get_relationship(id).await {
                    Ok(r) => (row_from_relationship(&r, &key)?, hash_relationship(&r)),
                    Err(_) => continue,
                },
                SyncTable::SuggestedLink => match self.db.get_suggested_link(id).await {
                    Ok(l) => (row_from_suggested_link(&l, &key)?, hash_suggested_link(&l)),
                    Err(_) => continue,
                },
            };
            let version = self
                .versions
                .lock()
                .expect("version store lock poisoned")
                .current_version(&row.id, &content_hash, &self.device_id);
            row.version_counter = version.counter;
            row.version_device = version.device_id;
            sign_row(&mut row, table, &self.keypair)?;
            out.push(row);
        }
        // Persist any freshly minted counters BEFORE the rows leave this
        // device — a stamp that's on the wire must never be re-mintable
        // for different content after a restart.
        self.versions
            .lock()
            .expect("version store lock poisoned")
            .save()?;
        Ok(out)
    }

    /// Apply rows received from a peer.
    ///
    /// Every row must unseal under the pair key shared with `sender`
    /// (P1.4 / P2P-005) and carry a valid Ed25519 signature by `sender`'s
    /// identity key (P1.3 / P2P-003) — unsigned or mis-signed rows are
    /// skipped, never applied. Conflict resolution orders by the signed
    /// `(version_counter, version_device)` stamp, not by `modified_at`;
    /// the 24h future-skew bound on timestamps stays as belt-and-
    /// suspenders.
    pub async fn apply_rows(
        &self,
        table: SyncTable,
        rows: Vec<EncryptedRow>,
        sender: &PeerId,
    ) -> P2pResult<RowApplyReport> {
        let key = self.pair_key_for(sender)?;
        let sender_key = match public_key_from_peer_id(sender) {
            Some(k) => k,
            None => {
                return Err(P2pError::SyncError(format!(
                    "cannot extract a public key from sender peer id {sender}; rejecting rows"
                )));
            }
        };
        let sender_id = sender.to_string();
        let mut written = 0u32;
        let mut skipped = 0u32;
        let mut review_flagged = 0u32;
        for row in rows {
            if !verify_row(&row, table, &sender_key) {
                tracing::warn!(
                    "rejecting row {} from {}: missing or invalid envelope signature (P2P-003)",
                    row.id,
                    sender
                );
                skipped += 1;
                continue;
            }
            // P2P-001: the signature only proves the SENDER signed this envelope
            // — it does NOT stop the sender from claiming authorship by a
            // different (ghost) device with an inflated counter, which would win
            // the (counter, device) LWW race and overwrite any row on every
            // paired device. Bind authorship to the verified sender: the row's
            // version_device MUST equal the sender's peer id (our version
            // identity IS the PeerId). This holds for the pairwise sync we do
            // today; multi-hop relay would instead require the ORIGINAL author's
            // signature over the row, verified against their pinned key.
            if row.version_device != sender_id {
                tracing::warn!(
                    "rejecting row {} from {}: version_device '{}' != verified sender (P2P-001 authorship forgery)",
                    row.id,
                    sender,
                    row.version_device
                );
                skipped += 1;
                continue;
            }
            // p2p-no-per-doc-authz: rows have no commit history, so before a
            // peer overwrite REPLACES an existing row we capture its prior
            // serialized state. We only stash it (sealed) if the apply actually
            // overwrote (Ok(true) AND the row pre-existed) — a fresh create has
            // nothing to recover, and a lost LWW race never touched the row.
            let prior = self.capture_prior_row(table, &row.id).await;
            let result = match table {
                SyncTable::Thread => self.apply_thread_row(&row, &key).await,
                SyncTable::Entity => self.apply_entity_row(&row, &key).await,
                SyncTable::PiiRecord => self.apply_pii_record_row(&row, &key).await,
                SyncTable::ShareRecord => self.apply_share_record_row(&row, &key).await,
                SyncTable::Contact => self.apply_contact_row(&row, &key).await,
                SyncTable::Message => self.apply_message_row(&row, &key).await,
                SyncTable::Conversation => self.apply_conversation_row(&row, &key).await,
                SyncTable::Milestone => self.apply_milestone_row(&row, &key).await,
                SyncTable::Relationship => self.apply_relationship_row(&row, &key).await,
                SyncTable::SuggestedLink => self.apply_suggested_link_row(&row, &key).await,
            };
            match result {
                Ok(true) => {
                    written += 1;
                    if let Some(prior_json) = prior {
                        self.stash_prior_row(&row.id, table, &prior_json, &sender_id).await;
                        review_flagged += 1;
                    }
                }
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
        // Best-effort persistence of merged counters/applied versions. The
        // rows are already in the DB; failing to persist only means some
        // stamps are re-evaluated (idempotently) on the next sync.
        if let Err(e) = self
            .versions
            .lock()
            .expect("version store lock poisoned")
            .save()
        {
            tracing::warn!("version store save after apply_rows failed: {e}");
        }
        Ok(RowApplyReport {
            written,
            skipped,
            review_flagged,
        })
    }

    /// Decide whether a remote row version beats the local one, stamping
    /// the local row lazily if it was edited since its last stamp. Returns
    /// `Some(remote_version)` to apply (the caller records it via
    /// [`Self::record_row_applied`] after the DB write succeeds), `None`
    /// to skip. Never holds the version lock across an await.
    fn resolve_row_version(
        &self,
        row_id: &str,
        remote_counter: u64,
        remote_device: &str,
        remote_hash: String,
        local_hash: &str,
    ) -> Option<RowVersion> {
        let remote = RowVersion {
            counter: remote_counter,
            device_id: remote_device.to_string(),
            content_hash: remote_hash,
        };
        let mut versions = self.versions.lock().expect("version store lock poisoned");
        // p2p-no-per-doc-authz: bound how far a remote counter may jump beyond
        // our Lamport high-water. A paired peer signs its OWN counter, so the
        // signature/authorship gates can't stop a forged-high value (u64::MAX)
        // from winning the LWW race and overwriting any row. An honest device
        // never leaps this far ahead of what we've already synced.
        let high_water = versions.own_counter();
        if remote.counter > high_water.saturating_add(MAX_COUNTER_ADVANCE) {
            tracing::warn!(
                "rejecting row {} from {}: version_counter {} exceeds high-water {} + cap {} (forged-counter overwrite attempt, p2p-no-per-doc-authz)",
                row_id,
                remote.device_id,
                remote.counter,
                high_water,
                MAX_COUNTER_ADVANCE
            );
            return None;
        }
        let local = versions.current_version(row_id, local_hash, &self.device_id);
        if remote.ordering_key() > local.ordering_key() {
            Some(remote)
        } else {
            if remote.ordering_key() < local.ordering_key() {
                tracing::debug!(
                    "row {} from {} loses version race ({} < {}); possible stale or replayed row",
                    row_id,
                    remote.device_id,
                    remote.counter,
                    local.counter
                );
            }
            None
        }
    }

    /// Record that `version` was applied to `row_id` (merges the Lamport
    /// clock). Called after the DB write succeeded.
    fn record_row_applied(&self, row_id: &str, version: RowVersion) {
        self.versions
            .lock()
            .expect("version store lock poisoned")
            .record_applied(row_id, version);
    }

    /// Capture the current local copy of an overwritable row as serialized
    /// JSON, BEFORE a peer overwrite replaces it (p2p-no-per-doc-authz row
    /// shadow-recovery). Returns `None` if the row doesn't exist locally (a
    /// fresh create — nothing to preserve) or for the append-only tables
    /// (ShareRecord / Milestone / Relationship), which the apply path never
    /// overwrites. SuggestedLink IS overwritable (status/resolved_at) — C2.
    async fn capture_prior_row(&self, table: SyncTable, row_id: &str) -> Option<Vec<u8>> {
        match table {
            SyncTable::Thread => self.db.get_thread(row_id).await.ok().and_then(|r| serde_json::to_vec(&r).ok()),
            SyncTable::Entity => self.db.get_entity(row_id).await.ok().and_then(|r| serde_json::to_vec(&r).ok()),
            SyncTable::PiiRecord => self.db.get_pii_record(row_id).await.ok().and_then(|r| serde_json::to_vec(&r).ok()),
            SyncTable::Contact => self.db.get_contact(row_id).await.ok().and_then(|r| serde_json::to_vec(&r).ok()),
            SyncTable::Message => self.db.get_message(row_id).await.ok().and_then(|r| serde_json::to_vec(&r).ok()),
            SyncTable::Conversation => self.db.get_conversation(row_id).await.ok().and_then(|r| serde_json::to_vec(&r).ok()),
            SyncTable::SuggestedLink => self.db.get_suggested_link(row_id).await.ok().and_then(|r| serde_json::to_vec(&r).ok()),
            SyncTable::ShareRecord
            | SyncTable::Milestone
            | SyncTable::Relationship => None,
        }
    }

    /// Seal a captured prior row (under the account transport key, so it is
    /// never plaintext at rest) and stash it as a recovery entry. Best-effort:
    /// a failure here must not abort the sync — the row is already updated, and
    /// losing the shadow only forfeits one-click restore, never correctness.
    async fn stash_prior_row(&self, row_id: &str, table: SyncTable, prior_json: &[u8], peer: &str) {
        use base64::Engine;
        let (ct, nonce) = match sovereign_crypto::aead::encrypt(prior_json, &self.transport_key) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("row-recovery seal {row_id} failed: {e}");
                return;
            }
        };
        let ct_b64 = base64::engine::general_purpose::STANDARD.encode(&ct);
        let nonce_b64 = base64::engine::general_purpose::STANDARD.encode(nonce);
        if let Err(e) = self
            .db
            .stash_row_recovery(row_id, table.as_str(), &ct_b64, &nonce_b64, peer)
            .await
        {
            tracing::warn!("row-recovery stash {row_id} failed: {e}");
        }
    }

    /// Re-apply the preserved prior value of a row recovery, undoing a peer
    /// overwrite (p2p-no-per-doc-authz). Unseals the stashed prior (under the
    /// account transport key — this is the only place that key lives), restores
    /// the record's content fields via the per-table setter, and marks the
    /// recovery reviewed. Restore for the content-bearing sensitive tables
    /// (PII vault, threads, contacts) is wired; other tables return an error
    /// (their prior is still preserved and inspectable for manual recovery).
    pub async fn restore_row_recovery(&self, recovery_id: &str) -> P2pResult<()> {
        use base64::Engine;
        let rec = self
            .db
            .get_row_recovery(recovery_id)
            .await
            .map_err(|e| P2pError::SyncError(format!("get_row_recovery {recovery_id}: {e}")))?;
        let ct = base64::engine::general_purpose::STANDARD
            .decode(&rec.prior_ciphertext)
            .map_err(|e| P2pError::SyncError(format!("recovery base64 ct: {e}")))?;
        let nonce_v = base64::engine::general_purpose::STANDARD
            .decode(&rec.prior_nonce)
            .map_err(|e| P2pError::SyncError(format!("recovery base64 nonce: {e}")))?;
        if nonce_v.len() != 24 {
            return Err(P2pError::SyncError(format!(
                "recovery nonce wrong length: {}",
                nonce_v.len()
            )));
        }
        let mut nonce = [0u8; 24];
        nonce.copy_from_slice(&nonce_v);
        let prior_json = sovereign_crypto::aead::decrypt(&ct, &nonce, &self.transport_key)
            .map_err(|e| P2pError::SyncError(format!("recovery unseal: {e}")))?;

        match rec.table.as_str() {
            "thread" => {
                let t: Thread = serde_json::from_slice(&prior_json)
                    .map_err(|e| P2pError::SyncError(format!("recovery decode thread: {e}")))?;
                self.db
                    .update_thread(&rec.row_id, Some(&t.name), Some(&t.description))
                    .await
                    .map_err(|e| P2pError::SyncError(format!("restore thread: {e}")))?;
            }
            "contact" => {
                let c: Contact = serde_json::from_slice(&prior_json)
                    .map_err(|e| P2pError::SyncError(format!("recovery decode contact: {e}")))?;
                self.db
                    .update_contact(&rec.row_id, Some(&c.name), Some(&c.notes), c.avatar.as_deref())
                    .await
                    .map_err(|e| P2pError::SyncError(format!("restore contact: {e}")))?;
            }
            "pii_record" => {
                let p: PiiRecord = serde_json::from_slice(&prior_json)
                    .map_err(|e| P2pError::SyncError(format!("recovery decode pii: {e}")))?;
                self.db
                    .update_pii_record_value(&rec.row_id, &p.value_encrypted, &p.value_nonce)
                    .await
                    .map_err(|e| P2pError::SyncError(format!("restore pii value: {e}")))?;
            }
            "entity" => {
                let en: Entity = serde_json::from_slice(&prior_json)
                    .map_err(|e| P2pError::SyncError(format!("recovery decode entity: {e}")))?;
                self.db
                    .update_entity(
                        &rec.row_id,
                        Some(&en.name),
                        Some(en.kind.clone()),
                        Some(en.domains.clone()),
                        Some(en.contact_ids.clone()),
                        Some(&en.notes),
                        Some(en.is_owned),
                        Some(en.deleted_at.clone()),
                    )
                    .await
                    .map_err(|e| P2pError::SyncError(format!("restore entity: {e}")))?;
            }
            "message" => {
                let m: Message = serde_json::from_slice(&prior_json)
                    .map_err(|e| P2pError::SyncError(format!("recovery decode message: {e}")))?;
                // The mutable message fields are the body (canonical PII
                // rewrite) and the read status — restore both.
                self.db
                    .update_message_body(&rec.row_id, &m.body, m.body_html.as_deref())
                    .await
                    .map_err(|e| P2pError::SyncError(format!("restore message body: {e}")))?;
                self.db
                    .update_message_read_status(&rec.row_id, m.read_status.clone())
                    .await
                    .map_err(|e| P2pError::SyncError(format!("restore message status: {e}")))?;
            }
            "conversation" => {
                let v: Conversation = serde_json::from_slice(&prior_json)
                    .map_err(|e| P2pError::SyncError(format!("recovery decode conversation: {e}")))?;
                // Restore the fields the apply path can overwrite: unread
                // count, last_message_at, thread link.
                self.db
                    .update_conversation_unread(&rec.row_id, v.unread_count)
                    .await
                    .map_err(|e| P2pError::SyncError(format!("restore conversation unread: {e}")))?;
                if let Some(at) = v.last_message_at {
                    self.db
                        .update_conversation_last_message_at(&rec.row_id, at)
                        .await
                        .map_err(|e| {
                            P2pError::SyncError(format!("restore conversation last_message_at: {e}"))
                        })?;
                }
                if let Some(tid) = v.linked_thread_id.as_deref() {
                    self.db
                        .link_conversation_to_thread(&rec.row_id, tid)
                        .await
                        .map_err(|e| {
                            P2pError::SyncError(format!("restore conversation thread link: {e}"))
                        })?;
                }
            }
            "suggested_link" => {
                let l: SuggestedLink = serde_json::from_slice(&prior_json)
                    .map_err(|e| P2pError::SyncError(format!("recovery decode suggested_link: {e}")))?;
                // Status + resolved_at are the only peer-overwritable fields.
                self.db
                    .set_suggested_link_status(&rec.row_id, l.status.clone(), l.resolved_at)
                    .await
                    .map_err(|e| P2pError::SyncError(format!("restore suggested_link: {e}")))?;
            }
            other => {
                return Err(P2pError::SyncError(format!(
                    "restore not yet supported for table '{other}'; the prior value is preserved \
                     in recovery {recovery_id} and can be inspected manually"
                )));
            }
        }

        self.db
            .resolve_row_recovery(recovery_id)
            .await
            .map_err(|e| P2pError::SyncError(format!("resolve_row_recovery: {e}")))?;
        Ok(())
    }

    async fn apply_thread_row(&self, row: &EncryptedRow, key: &[u8; 32]) -> P2pResult<bool> {
        let remote: Thread = decode_row_inner(row, key)?;

        // P2P-003 belt-and-suspenders: far-future timestamps are rejected
        // even though resolution no longer trusts them (display ordering
        // and the manifest diff still read them).
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
                // P1.3: resolution by signed (counter, device_id) stamp.
                let winner = self.resolve_row_version(
                    &row.id,
                    row.version_counter,
                    &row.version_device,
                    hash_thread(&remote),
                    &hash_thread(&local),
                );
                let Some(version) = winner else {
                    return Ok(false);
                };
                tracing::warn!(
                    "thread row {} overwriting local copy (remote version {}@{} wins)",
                    row.id,
                    version.counter,
                    version.device_id
                );
                self.db
                    .update_thread(
                        &row.id,
                        Some(&remote.name),
                        Some(&remote.description),
                    )
                    .await
                    .map_err(|e| P2pError::SyncError(format!("update_thread: {e}")))?;
                // H-p2p1: converge on the winner's deleted state (soft-delete
                // only — restorable from trash on this device too).
                if remote.deleted_at.is_some() && local.deleted_at.is_none() {
                    self.db
                        .soft_delete_thread(&row.id)
                        .await
                        .map_err(|e| P2pError::SyncError(format!("soft_delete_thread: {e}")))?;
                } else if remote.deleted_at.is_none() && local.deleted_at.is_some() {
                    self.db
                        .restore_soft_deleted_thread(&row.id)
                        .await
                        .map_err(|e| P2pError::SyncError(format!("restore_soft_deleted_thread: {e}")))?;
                }
                self.record_row_applied(&row.id, version);
                Ok(true)
            }
            Err(_) => {
                // Thread missing locally — create it UNDER ITS ORIGIN ID
                // (P2): a minted local id would make the remote id look
                // forever-missing and re-fetch the row every sync round.
                let version = RowVersion {
                    counter: row.version_counter,
                    device_id: row.version_device.clone(),
                    content_hash: hash_thread(&remote),
                };
                let mut remote = remote;
                remote.id = Some(envelope_thing(&row.id)?);
                let inserted = self
                    .db
                    .create_thread_with_id(remote)
                    .await
                    .map_err(|e| P2pError::SyncError(format!("create_thread_with_id: {e}")))?;
                self.record_row_applied(&row.id, version);
                Ok(inserted)
            }
        }
    }

    async fn apply_entity_row(&self, row: &EncryptedRow, key: &[u8; 32]) -> P2pResult<bool> {
        let remote: Entity = decode_row_inner(row, key)?;

        // P2P-003 belt-and-suspenders (see apply_thread_row).
        if remote.modified_at > chrono::Utc::now() + MAX_FUTURE_SKEW {
            tracing::warn!(
                "rejecting entity row {}: modified_at {} is beyond max future skew",
                row.id,
                remote.modified_at
            );
            return Ok(false);
        }

        match self.db.get_entity(&row.id).await {
            Ok(local) => {
                let winner = self.resolve_row_version(
                    &row.id,
                    row.version_counter,
                    &row.version_device,
                    hash_entity(&remote),
                    &hash_entity(&local),
                );
                let Some(version) = winner else {
                    return Ok(false);
                };
                tracing::warn!(
                    "entity row {} overwriting local copy (remote version {}@{} wins)",
                    row.id,
                    version.counter,
                    version.device_id
                );
                self.db
                    .update_entity(
                        &row.id,
                        Some(&remote.name),
                        Some(remote.kind.clone()),
                        Some(remote.domains.clone()),
                        Some(remote.contact_ids.clone()),
                        Some(&remote.notes),
                        Some(remote.is_owned),
                        Some(remote.deleted_at.clone()),
                    )
                    .await
                    .map_err(|e| P2pError::SyncError(format!("update_entity: {e}")))?;
                self.record_row_applied(&row.id, version);
                Ok(true)
            }
            Err(_) => {
                let version = RowVersion {
                    counter: row.version_counter,
                    device_id: row.version_device.clone(),
                    content_hash: hash_entity(&remote),
                };
                let mut remote = remote;
                remote.id = Some(envelope_thing(&row.id)?);
                let inserted = self
                    .db
                    .create_entity_with_id(remote)
                    .await
                    .map_err(|e| P2pError::SyncError(format!("create_entity_with_id: {e}")))?;
                self.record_row_applied(&row.id, version);
                Ok(inserted)
            }
        }
    }

    async fn apply_pii_record_row(&self, row: &EncryptedRow, key: &[u8; 32]) -> P2pResult<bool> {
        let remote: PiiRecord = decode_row_inner(row, key)?;

        // P2P-003 belt-and-suspenders (see apply_thread_row).
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
                let winner = self.resolve_row_version(
                    &row.id,
                    row.version_counter,
                    &row.version_device,
                    hash_pii_record(&remote),
                    &hash_pii_record(&local),
                );
                let Some(version) = winner else {
                    return Ok(false);
                };
                // Refresh the encrypted value (the only field we have an
                // update method for). Other field changes ride along in
                // a later pass.
                tracing::warn!(
                    "pii record {} overwriting local copy (remote version {}@{} wins)",
                    row.id,
                    version.counter,
                    version.device_id
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
                // H-p2p1: a redacted (soft-deleted) PII record must not stay
                // live on other devices — the review's headline resurrection
                // case. Forward-only: PiiRecord has no restore method, so an
                // un-delete does not propagate (the redact action is L5 and
                // deliberate; resurrecting PII by sync would be worse).
                if remote.deleted_at.is_some() && local.deleted_at.is_none() {
                    self.db
                        .soft_delete_pii_record(&row.id)
                        .await
                        .map_err(|e| P2pError::SyncError(format!("soft_delete_pii_record: {e}")))?;
                }
                self.record_row_applied(&row.id, version);
                Ok(true)
            }
            Err(_) => {
                let version = RowVersion {
                    counter: row.version_counter,
                    device_id: row.version_device.clone(),
                    content_hash: hash_pii_record(&remote),
                };
                let mut remote = remote;
                remote.id = Some(envelope_thing(&row.id)?);
                let inserted = self
                    .db
                    .create_pii_record_with_id(remote)
                    .await
                    .map_err(|e| P2pError::SyncError(format!("create_pii_record_with_id: {e}")))?;
                self.record_row_applied(&row.id, version);
                Ok(inserted)
            }
        }
    }

    async fn apply_share_record_row(&self, row: &EncryptedRow, key: &[u8; 32]) -> P2pResult<bool> {
        let mut remote: ShareRecord = decode_row_inner(row, key)?;
        // Append-only: skip if already present, otherwise create under
        // its origin id (P2 — see apply_thread_row).
        if self.db.get_share_record(&row.id).await.is_ok() {
            return Ok(false);
        }
        remote.id = Some(envelope_thing(&row.id)?);
        let inserted = self
            .db
            .create_share_record_with_id(remote)
            .await
            .map_err(|e| P2pError::SyncError(format!("create_share_record_with_id: {e}")))?;
        Ok(inserted)
    }

    // ----- P2 table apply fns -----

    async fn apply_contact_row(&self, row: &EncryptedRow, key: &[u8; 32]) -> P2pResult<bool> {
        let remote: Contact = decode_row_inner(row, key)?;

        if remote.modified_at > chrono::Utc::now() + MAX_FUTURE_SKEW {
            tracing::warn!(
                "rejecting contact row {}: modified_at beyond max future skew",
                row.id
            );
            return Ok(false);
        }

        match self.db.get_contact(&row.id).await {
            Ok(local) => {
                let winner = self.resolve_row_version(
                    &row.id,
                    row.version_counter,
                    &row.version_device,
                    hash_contact(&remote),
                    &hash_contact(&local),
                );
                let Some(version) = winner else {
                    return Ok(false);
                };
                self.db
                    .update_contact(
                        &row.id,
                        Some(&remote.name),
                        Some(&remote.notes),
                        remote.avatar.as_deref(),
                    )
                    .await
                    .map_err(|e| P2pError::SyncError(format!("update_contact: {e}")))?;
                // Addresses: append any the local copy lacks (no removal
                // path exists; removals don't propagate yet).
                for addr in &remote.addresses {
                    let have = local
                        .addresses
                        .iter()
                        .any(|a| a.address == addr.address && a.channel == addr.channel);
                    if !have {
                        self.db
                            .add_contact_address(&row.id, addr.clone())
                            .await
                            .map_err(|e| {
                                P2pError::SyncError(format!("add_contact_address: {e}"))
                            })?;
                    }
                }
                // Soft-delete propagation.
                if remote.deleted_at.is_some() && local.deleted_at.is_none() {
                    self.db
                        .soft_delete_contact(&row.id)
                        .await
                        .map_err(|e| P2pError::SyncError(format!("soft_delete_contact: {e}")))?;
                }
                self.record_row_applied(&row.id, version);
                Ok(true)
            }
            Err(_) => {
                let version = RowVersion {
                    counter: row.version_counter,
                    device_id: row.version_device.clone(),
                    content_hash: hash_contact(&remote),
                };
                let mut remote = remote;
                remote.id = Some(envelope_thing(&row.id)?);
                let inserted = self
                    .db
                    .create_contact_with_id(remote)
                    .await
                    .map_err(|e| P2pError::SyncError(format!("create_contact_with_id: {e}")))?;
                self.record_row_applied(&row.id, version);
                Ok(inserted)
            }
        }
    }

    async fn apply_message_row(&self, row: &EncryptedRow, key: &[u8; 32]) -> P2pResult<bool> {
        let remote: Message = decode_row_inner(row, key)?;

        if remote.created_at > chrono::Utc::now() + MAX_FUTURE_SKEW {
            tracing::warn!(
                "rejecting message row {}: created_at beyond max future skew",
                row.id
            );
            return Ok(false);
        }

        match self.db.get_message(&row.id).await {
            Ok(local) => {
                let winner = self.resolve_row_version(
                    &row.id,
                    row.version_counter,
                    &row.version_device,
                    hash_message(&remote),
                    &hash_message(&local),
                );
                let Some(version) = winner else {
                    return Ok(false);
                };
                // Messages are near-immutable; the mutable parts are the
                // read status and the canonical body (PII tokenization
                // rewrites it post-ingest).
                if remote.read_status != local.read_status {
                    self.db
                        .update_message_read_status(&row.id, remote.read_status.clone())
                        .await
                        .map_err(|e| {
                            P2pError::SyncError(format!("update_message_read_status: {e}"))
                        })?;
                }
                if remote.body != local.body || remote.body_html != local.body_html {
                    self.db
                        .update_message_body(&row.id, &remote.body, remote.body_html.as_deref())
                        .await
                        .map_err(|e| P2pError::SyncError(format!("update_message_body: {e}")))?;
                }
                self.record_row_applied(&row.id, version);
                Ok(true)
            }
            Err(_) => {
                let version = RowVersion {
                    counter: row.version_counter,
                    device_id: row.version_device.clone(),
                    content_hash: hash_message(&remote),
                };
                let mut remote = remote;
                remote.id = Some(envelope_thing(&row.id)?);
                let inserted = self
                    .db
                    .create_message_with_id(remote)
                    .await
                    .map_err(|e| P2pError::SyncError(format!("create_message_with_id: {e}")))?;
                self.record_row_applied(&row.id, version);
                Ok(inserted)
            }
        }
    }

    async fn apply_conversation_row(&self, row: &EncryptedRow, key: &[u8; 32]) -> P2pResult<bool> {
        let remote: Conversation = decode_row_inner(row, key)?;

        if remote.created_at > chrono::Utc::now() + MAX_FUTURE_SKEW {
            tracing::warn!(
                "rejecting conversation row {}: created_at beyond max future skew",
                row.id
            );
            return Ok(false);
        }

        match self.db.get_conversation(&row.id).await {
            Ok(local) => {
                let winner = self.resolve_row_version(
                    &row.id,
                    row.version_counter,
                    &row.version_device,
                    hash_conversation(&remote),
                    &hash_conversation(&local),
                );
                let Some(version) = winner else {
                    return Ok(false);
                };
                // Mutable fields with dedicated setters: unread count,
                // last_message_at, thread link. (Title changes have no
                // update path yet — deferred.)
                if remote.unread_count != local.unread_count {
                    self.db
                        .update_conversation_unread(&row.id, remote.unread_count)
                        .await
                        .map_err(|e| {
                            P2pError::SyncError(format!("update_conversation_unread: {e}"))
                        })?;
                }
                if let Some(at) = remote.last_message_at {
                    if local.last_message_at.map_or(true, |l| at > l) {
                        self.db
                            .update_conversation_last_message_at(&row.id, at)
                            .await
                            .map_err(|e| {
                                P2pError::SyncError(format!(
                                    "update_conversation_last_message_at: {e}"
                                ))
                            })?;
                    }
                }
                if let Some(ref tid) = remote.linked_thread_id {
                    if local.linked_thread_id.as_deref() != Some(tid.as_str()) {
                        self.db
                            .link_conversation_to_thread(&row.id, tid)
                            .await
                            .map_err(|e| {
                                P2pError::SyncError(format!("link_conversation_to_thread: {e}"))
                            })?;
                    }
                }
                self.record_row_applied(&row.id, version);
                Ok(true)
            }
            Err(_) => {
                let version = RowVersion {
                    counter: row.version_counter,
                    device_id: row.version_device.clone(),
                    content_hash: hash_conversation(&remote),
                };
                let mut remote = remote;
                remote.id = Some(envelope_thing(&row.id)?);
                let inserted = self
                    .db
                    .create_conversation_with_id(remote)
                    .await
                    .map_err(|e| {
                        P2pError::SyncError(format!("create_conversation_with_id: {e}"))
                    })?;
                self.record_row_applied(&row.id, version);
                Ok(inserted)
            }
        }
    }

    async fn apply_milestone_row(&self, row: &EncryptedRow, key: &[u8; 32]) -> P2pResult<bool> {
        let mut remote: Milestone = decode_row_inner(row, key)?;

        if remote.timestamp > chrono::Utc::now() + MAX_FUTURE_SKEW {
            tracing::warn!(
                "rejecting milestone row {}: timestamp beyond max future skew",
                row.id
            );
            return Ok(false);
        }

        // Append-only (milestones are immutable; deletions are hard
        // deletes and don't propagate yet).
        if self.db.get_milestone(&row.id).await.is_ok() {
            return Ok(false);
        }
        remote.id = Some(envelope_thing(&row.id)?);
        let inserted = self
            .db
            .create_milestone_with_id(remote)
            .await
            .map_err(|e| P2pError::SyncError(format!("create_milestone_with_id: {e}")))?;
        Ok(inserted)
    }

    async fn apply_relationship_row(&self, row: &EncryptedRow, key: &[u8; 32]) -> P2pResult<bool> {
        let mut remote: RelatedTo = decode_row_inner(row, key)?;

        if remote.created_at > chrono::Utc::now() + MAX_FUTURE_SKEW {
            tracing::warn!(
                "rejecting relationship row {}: created_at beyond max future skew",
                row.id
            );
            return Ok(false);
        }

        // Append-only edge.
        if self.db.get_relationship(&row.id).await.is_ok() {
            return Ok(false);
        }
        remote.id = Some(envelope_thing(&row.id)?);
        let inserted = self
            .db
            .create_relationship_with_id(remote)
            .await
            .map_err(|e| P2pError::SyncError(format!("create_relationship_with_id: {e}")))?;
        Ok(inserted)
    }

    async fn apply_suggested_link_row(&self, row: &EncryptedRow, key: &[u8; 32]) -> P2pResult<bool> {
        let remote: SuggestedLink = decode_row_inner(row, key)?;

        if remote.created_at > chrono::Utc::now() + MAX_FUTURE_SKEW {
            tracing::warn!(
                "rejecting suggested link row {}: created_at beyond max future skew",
                row.id
            );
            return Ok(false);
        }

        match self.db.get_suggested_link(&row.id).await {
            Ok(local) => {
                let winner = self.resolve_row_version(
                    &row.id,
                    row.version_counter,
                    &row.version_device,
                    hash_suggested_link(&remote),
                    &hash_suggested_link(&local),
                );
                let Some(version) = winner else {
                    return Ok(false);
                };
                // The only mutable bits are status + resolved_at. Raw
                // setter — the accepted-link promotion to a related_to
                // edge synced separately as its own row.
                self.db
                    .set_suggested_link_status(
                        &row.id,
                        remote.status.clone(),
                        remote.resolved_at,
                    )
                    .await
                    .map_err(|e| {
                        P2pError::SyncError(format!("set_suggested_link_status: {e}"))
                    })?;
                self.record_row_applied(&row.id, version);
                Ok(true)
            }
            Err(_) => {
                let version = RowVersion {
                    counter: row.version_counter,
                    device_id: row.version_device.clone(),
                    content_hash: hash_suggested_link(&remote),
                };
                let mut remote = remote;
                remote.id = Some(envelope_thing(&row.id)?);
                let inserted = self
                    .db
                    .create_suggested_link_with_id(remote)
                    .await
                    .map_err(|e| {
                        P2pError::SyncError(format!("create_suggested_link_with_id: {e}"))
                    })?;
                self.record_row_applied(&row.id, version);
                Ok(inserted)
            }
        }
    }

    /// Get the device ID for this sync service.
    pub fn device_id(&self) -> &str {
        &self.device_id
    }
}

/// Parse the (signed) envelope id into a Thing for an id-preserving
/// create. The envelope id is covered by the row signature, so it is the
/// authoritative identity — the embedded `id` inside the decrypted body
/// is overwritten with it.
fn envelope_thing(raw_id: &str) -> P2pResult<sovereign_db::schema::Thing> {
    sovereign_db::schema::raw_to_thing(raw_id)
        .ok_or_else(|| P2pError::SyncError(format!("malformed row id '{raw_id}'")))
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

fn row_from_contact(c: &Contact, key: &[u8; 32]) -> P2pResult<EncryptedRow> {
    encode_row(c.id_string().unwrap_or_default(), c, c.modified_at.to_rfc3339(), c.deleted_at.clone(), key)
}

fn row_from_message(m: &Message, key: &[u8; 32]) -> P2pResult<EncryptedRow> {
    encode_row(m.id_string().unwrap_or_default(), m, m.created_at.to_rfc3339(), m.deleted_at.clone(), key)
}

fn row_from_conversation(v: &Conversation, key: &[u8; 32]) -> P2pResult<EncryptedRow> {
    let lww = v.last_message_at.unwrap_or(v.created_at).to_rfc3339();
    encode_row(v.id_string().unwrap_or_default(), v, lww, v.deleted_at.clone(), key)
}

fn row_from_milestone(ms: &Milestone, key: &[u8; 32]) -> P2pResult<EncryptedRow> {
    encode_row(ms.id_string().unwrap_or_default(), ms, ms.timestamp.to_rfc3339(), None, key)
}

fn row_from_relationship(r: &RelatedTo, key: &[u8; 32]) -> P2pResult<EncryptedRow> {
    encode_row(r.id_string().unwrap_or_default(), r, r.created_at.to_rfc3339(), None, key)
}

fn row_from_suggested_link(l: &SuggestedLink, key: &[u8; 32]) -> P2pResult<EncryptedRow> {
    let lww = l.resolved_at.unwrap_or(l.created_at).to_rfc3339();
    encode_row(l.id_string().unwrap_or_default(), l, lww, None, key)
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
        version_counter: 0,
        version_device: String::new(),
        signature: String::new(),
    })
}

// --- Row envelope signing (P1.3 / P2P-003 deep fix) ---

/// Canonical byte encoding of a row envelope for signing. Every field is
/// length-prefixed (no delimiter ambiguity), the table is bound in (a row
/// can't be replayed into another table), and the version stamp is covered
/// (a relay can't inflate someone else's counter).
fn row_sig_bytes(table: SyncTable, row: &EncryptedRow) -> Vec<u8> {
    let mut out = Vec::with_capacity(128 + row.ciphertext.len());
    out.extend_from_slice(b"sovereign-row-sig:v1");
    for field in [
        table.as_str(),
        row.id.as_str(),
        row.ciphertext.as_str(),
        row.nonce.as_str(),
        row.modified_at.as_str(),
        row.deleted_at.as_deref().unwrap_or(""),
        row.version_device.as_str(),
    ] {
        out.extend_from_slice(&(field.len() as u32).to_le_bytes());
        out.extend_from_slice(field.as_bytes());
    }
    // Disambiguate deleted_at: None vs Some("").
    out.push(row.deleted_at.is_some() as u8);
    out.extend_from_slice(&row.version_counter.to_le_bytes());
    out
}

/// Sign a row envelope with the sending device's identity keypair. The
/// version fields must already be set.
fn sign_row(row: &mut EncryptedRow, table: SyncTable, keypair: &Keypair) -> P2pResult<()> {
    use base64::Engine;
    let sig = keypair
        .sign(&row_sig_bytes(table, row))
        .map_err(|e| P2pError::SyncError(format!("row sign: {e}")))?;
    row.signature = base64::engine::general_purpose::STANDARD.encode(sig);
    Ok(())
}

/// Verify a row envelope's signature against the sender's public key.
/// An absent signature fails (pre-P1.3 rows are rejected, like the
/// empty-nonce plaintext shape under P2P-002).
fn verify_row(row: &EncryptedRow, table: SyncTable, sender_key: &PublicKey) -> bool {
    use base64::Engine;
    if row.signature.is_empty() {
        return false;
    }
    let Ok(sig) = base64::engine::general_purpose::STANDARD.decode(&row.signature) else {
        return false;
    };
    sender_key.verify(&row_sig_bytes(table, row), &sig)
}

// --- Document-commit envelope signing (AUTOCOMMIT-001 / P2P-001 doc path) ---

/// Canonical byte encoding of a commit envelope for signing. Binds the
/// document id, the sealed snapshot (ciphertext + nonce), the chain metadata,
/// and the author device — so a peer can't swap content, re-parent, or claim
/// authorship by a different device. (Like rows, this signs over the
/// pair-key ciphertext, so verification is pairwise: author == sender.)
fn commit_sig_bytes(commit: &EncryptedCommit) -> Vec<u8> {
    let mut out = Vec::with_capacity(128 + commit.encrypted_snapshot.len());
    out.extend_from_slice(b"sovereign-commit-sig:v1");
    for field in [
        commit.document_id.as_str(),
        commit.commit_id.as_str(),
        commit.parent_commit.as_deref().unwrap_or(""),
        commit.encrypted_snapshot.as_str(),
        commit.nonce.as_str(),
        commit.message.as_str(),
        commit.timestamp.as_str(),
        commit.version_device.as_str(),
    ] {
        out.extend_from_slice(&(field.len() as u32).to_le_bytes());
        out.extend_from_slice(field.as_bytes());
    }
    // Disambiguate parent_commit None vs Some("").
    out.push(commit.parent_commit.is_some() as u8);
    out
}

/// Sign a commit envelope with the sending device's identity keypair.
/// `version_device` must already be set.
fn sign_commit(commit: &mut EncryptedCommit, keypair: &Keypair) -> P2pResult<()> {
    use base64::Engine;
    let sig = keypair
        .sign(&commit_sig_bytes(commit))
        .map_err(|e| P2pError::SyncError(format!("commit sign: {e}")))?;
    commit.signature = base64::engine::general_purpose::STANDARD.encode(sig);
    Ok(())
}

/// Verify a commit envelope's signature against the sender's public key.
/// An absent signature fails (pre-signing commits are rejected).
fn verify_commit(commit: &EncryptedCommit, sender_key: &PublicKey) -> bool {
    use base64::Engine;
    if commit.signature.is_empty() {
        return false;
    }
    let Ok(sig) = base64::engine::general_purpose::STANDARD.decode(&commit.signature) else {
        return false;
    };
    sender_key.verify(&commit_sig_bytes(commit), &sig)
}

/// Extract the Ed25519 public key embedded in a libp2p PeerId. Ed25519
/// peer ids use an identity multihash whose digest IS the protobuf-encoded
/// public key; non-inline (sha256) peer ids return None and the sender's
/// rows are rejected.
pub fn public_key_from_peer_id(peer_id: &PeerId) -> Option<PublicKey> {
    let mh: &libp2p::multihash::Multihash<64> = peer_id.as_ref();
    const MULTIHASH_IDENTITY_CODE: u64 = 0;
    if mh.code() != MULTIHASH_IDENTITY_CODE {
        return None;
    }
    PublicKey::try_decode_protobuf(mh.digest()).ok()
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

/// Outcome of applying one batch of peer rows (returned by
/// [`SyncService::apply_rows`]).
#[derive(Debug, Clone, Copy, Default)]
pub struct RowApplyReport {
    pub written: u32,
    pub skipped: u32,
    /// Rows whose prior local value was stashed for review — a peer
    /// overwrote an existing row. Surfaced as a [`crate::P2pEvent`] so the
    /// UI can badge the review panel instead of the change staying
    /// invisible until the user happens to open it (review C2).
    pub review_flagged: u32,
}

/// Effective LWW timestamp for a manifest entry: the deletion time when it
/// is the latest state change. A soft-delete doesn't bump `modified_at`, so
/// without this a tombstone always loses the manifest diff to the peer's
/// live copy and the deletion never propagates (H-p2p1). Both values are
/// RFC3339 UTC strings, so lexicographic comparison is chronological.
fn lww_ts(modified: String, deleted_at: &Option<String>) -> String {
    match deleted_at {
        Some(d) if *d > modified => d.clone(),
        _ => modified,
    }
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

// ----- P2 table hashes. Same convention as above: cover the synced
// fields, exclude the table's LWW timestamp. -----

fn hash_contact(c: &Contact) -> String {
    let mut h = Sha256::new();
    h.update(c.name.as_bytes());
    h.update(b"|notes:");
    h.update(c.notes.as_bytes());
    h.update(b"|avatar:");
    h.update(c.avatar.as_deref().unwrap_or("").as_bytes());
    h.update(b"|entity:");
    h.update(c.entity_id.as_deref().unwrap_or("").as_bytes());
    h.update(b"|owned:");
    h.update(if c.is_owned { b"1" as &[u8] } else { b"0" });
    // Addresses order-insensitively (devices may append in different order).
    let mut addrs: Vec<String> = c
        .addresses
        .iter()
        .map(|a| format!("{}@{}", a.channel, a.address))
        .collect();
    addrs.sort();
    for a in &addrs {
        h.update(b"|a:");
        h.update(a.as_bytes());
    }
    if let Some(ref d) = c.deleted_at {
        h.update(b"|deleted:");
        h.update(d.as_bytes());
    }
    format!("{:x}", h.finalize())
}

fn hash_message(m: &Message) -> String {
    let mut h = Sha256::new();
    h.update(m.conversation_id.as_bytes());
    h.update(b"|chan:");
    h.update(m.channel.to_string().as_bytes());
    h.update(b"|dir:");
    h.update(format!("{:?}", m.direction).as_bytes());
    h.update(b"|from:");
    h.update(m.from_contact_id.as_bytes());
    for to in &m.to_contact_ids {
        h.update(b"|to:");
        h.update(to.as_bytes());
    }
    h.update(b"|subj:");
    h.update(m.subject.as_deref().unwrap_or("").as_bytes());
    h.update(b"|body:");
    h.update(m.body.as_bytes());
    h.update(b"|read:");
    h.update(format!("{:?}", m.read_status).as_bytes());
    h.update(b"|ext:");
    h.update(m.external_id.as_deref().unwrap_or("").as_bytes());
    if let Some(ref d) = m.deleted_at {
        h.update(b"|deleted:");
        h.update(d.as_bytes());
    }
    format!("{:x}", h.finalize())
}

fn hash_conversation(v: &Conversation) -> String {
    let mut h = Sha256::new();
    h.update(v.title.as_bytes());
    h.update(b"|chan:");
    h.update(v.channel.to_string().as_bytes());
    let mut parts = v.participant_contact_ids.clone();
    parts.sort();
    for p in &parts {
        h.update(b"|p:");
        h.update(p.as_bytes());
    }
    h.update(b"|unread:");
    h.update(v.unread_count.to_string().as_bytes());
    h.update(b"|thread:");
    h.update(v.linked_thread_id.as_deref().unwrap_or("").as_bytes());
    if let Some(ref d) = v.deleted_at {
        h.update(b"|deleted:");
        h.update(d.as_bytes());
    }
    format!("{:x}", h.finalize())
}

fn hash_milestone(ms: &Milestone) -> String {
    let mut h = Sha256::new();
    h.update(ms.title.as_bytes());
    h.update(b"|thread:");
    h.update(ms.thread_id.as_bytes());
    h.update(b"|desc:");
    h.update(ms.description.as_bytes());
    format!("{:x}", h.finalize())
}

fn hash_relationship(r: &RelatedTo) -> String {
    let mut h = Sha256::new();
    h.update(r.in_.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default().as_bytes());
    h.update(b"|out:");
    h.update(r.out.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default().as_bytes());
    h.update(b"|type:");
    h.update(r.relation_type.to_string().as_bytes());
    h.update(b"|strength:");
    h.update(r.strength.to_bits().to_le_bytes());
    format!("{:x}", h.finalize())
}

fn hash_suggested_link(l: &SuggestedLink) -> String {
    let mut h = Sha256::new();
    h.update(l.in_.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default().as_bytes());
    h.update(b"|out:");
    h.update(l.out.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default().as_bytes());
    h.update(b"|type:");
    h.update(l.relation_type.to_string().as_bytes());
    h.update(b"|strength:");
    h.update(l.strength.to_bits().to_le_bytes());
    h.update(b"|rationale:");
    h.update(l.rationale.as_bytes());
    h.update(b"|source:");
    h.update(format!("{:?}", l.source).as_bytes());
    h.update(b"|status:");
    h.update(format!("{:?}", l.status).as_bytes());
    format!("{:x}", h.finalize())
}

/// AEAD-seal a (plaintext) document snapshot under the transport key for
/// the wire. `commit_id`/`document_id`/`timestamp` stay in the clear as
/// metadata; the title + content are sealed (P2P-002).
fn seal_snapshot(
    commit_id: String,
    document_id: String,
    timestamp: String,
    snapshot: sovereign_db::schema::DocumentSnapshot,
    key: &[u8; 32],
) -> P2pResult<EncryptedCommit> {
    use base64::Engine;
    let snapshot_json = serde_json::to_vec(&snapshot)
        .map_err(|e| P2pError::SyncError(format!("snapshot serialize: {e}")))?;
    let (ciphertext, nonce) = sovereign_crypto::aead::encrypt(&snapshot_json, key)
        .map_err(|e| P2pError::SyncError(format!("snapshot encrypt: {e}")))?;
    Ok(EncryptedCommit {
        commit_id,
        document_id,
        parent_commit: None,
        encrypted_snapshot: base64::engine::general_purpose::STANDARD.encode(&ciphertext),
        nonce: base64::engine::general_purpose::STANDARD.encode(nonce),
        message: "sync".to_string(),
        timestamp,
        version_device: String::new(),
        signature: String::new(),
    })
}

/// Convert a DB Commit to the transport format, AEAD-sealing the snapshot
/// under the transport key (P2P-002). `message`/`timestamp`/parent stay in
/// the clear as commit-chain metadata; the document title + content (the
/// snapshot) are encrypted.
#[cfg(test)]
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
        version_device: String::new(),
        signature: String::new(),
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

    /// The pair key shared between test peers (P1.4). Same value the mock
    /// services use as their transport key, so rows/commits hand-sealed
    /// with `[7u8; 32]` in older tests decode under it unchanged.
    const TEST_PAIR_KEY: [u8; 32] = [7u8; 32];

    fn test_keypair(seed: u8) -> Keypair {
        Keypair::ed25519_from_bytes([seed; 32]).expect("32-byte seed")
    }

    /// Register `peer` on `svc` with TEST_PAIR_KEY so the per-pair
    /// seal/unseal paths resolve a key for it (P1.4 fails closed without).
    fn register_peer(svc: &SyncService, peer: &PeerId) {
        let mut m = std::collections::HashMap::new();
        m.insert(peer.to_string(), TEST_PAIR_KEY);
        svc.set_pair_keys(m);
    }

    /// A throwaway peer id for tests that only need SOME paired remote.
    fn remote_peer() -> PeerId {
        test_keypair(0xD9).public().to_peer_id()
    }

    fn mock_sync_service() -> (Arc<MockGraphDB>, SyncService) {
        mock_sync_service_with(0xE1, "device-1")
    }

    fn mock_sync_service_with(seed: u8, _device: &str) -> (Arc<MockGraphDB>, SyncService) {
        let db = Arc::new(MockGraphDB::new());
        // P2P-001: the version/authorship identity must be the verifiable
        // PeerId (apply_rows rejects rows whose version_device != sender peer
        // id), so derive it from the same keypair the service signs with.
        let peer_id = test_keypair(seed).public().to_peer_id().to_string();
        let svc = SyncService::new(
            db.clone(),
            peer_id,
            [7u8; 32],
            test_keypair(seed),
            VersionStore::ephemeral(),
        );
        (db, svc)
    }

    /// Stamp a hand-built row with a version, the way a remote device's
    /// `get_rows` would. Tests that call `apply_*_row` directly bypass the
    /// signature gate (that's `apply_rows`' job) but still need a stamp to
    /// win version resolution.
    fn stamp(mut row: EncryptedRow, counter: u64, device: &str) -> EncryptedRow {
        row.version_counter = counter;
        row.version_device = device.into();
        row
    }

    /// Sign a transport commit as the device whose keypair seed is `seed` —
    /// its PeerId must be the `sender` passed to apply_commits (AUTOCOMMIT-001).
    fn sign_commit_as(mut ec: EncryptedCommit, seed: u8) -> EncryptedCommit {
        ec.version_device = test_keypair(seed).public().to_peer_id().to_string();
        sign_commit(&mut ec, &test_keypair(seed)).unwrap();
        ec
    }

    #[tokio::test]
    async fn build_manifest_includes_all_docs() {
        let (db, svc) = mock_sync_service();
        let t = db.create_thread(Thread::new("T".into(), "".into())).await.unwrap();
        let tid = t.id_string().unwrap();

        db.create_document(Document::new("Doc A".into(), tid.clone(), true)).await.unwrap();
        db.create_document(Document::new("Doc B".into(), tid.clone(), true)).await.unwrap();

        let manifest = svc.build_manifest().await.unwrap();
        // P2P-001: the manifest identity is the service's verifiable PeerId.
        assert_eq!(manifest.device_id, svc.device_id());
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
    async fn get_commits_seals_current_document_state() {
        // Content-LWW model: get_commits returns the document's CURRENT
        // decrypted state sealed for transport, keyed by the head commit id.
        let (db, svc) = mock_sync_service();
        let t = db.create_thread(Thread::new("T".into(), "".into())).await.unwrap();
        let tid = t.id_string().unwrap();

        let doc = db.create_document(Document::new("Doc".into(), tid, true)).await.unwrap();
        let doc_id = doc.id_string().unwrap();
        db.update_document(&doc_id, Some("Doc"), Some("current body")).await.unwrap();

        let c = db.commit_document(&doc_id, "snapshot").await.unwrap();
        let cid = c.id_string().unwrap();

        let peer = remote_peer();
        register_peer(&svc, &peer);
        let result = svc.get_commits(&[cid.clone()], &peer).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].commit_id, cid);
        assert_eq!(result[0].document_id, doc_id);

        // The sealed snapshot decrypts back under the PAIR key (P1.4).
        let snap = transport_to_snapshot(&result[0], &TEST_PAIR_KEY).unwrap();
        assert_eq!(snap.content, "current body");

        // Without a pair key for the peer, nothing can be sealed for it.
        svc.set_pair_keys(std::collections::HashMap::new());
        assert!(
            svc.get_commits(&[cid], &peer).await.is_err(),
            "missing pair key must fail closed (P2P-005)"
        );
    }

    #[tokio::test]
    async fn get_commits_since_returns_current_state() {
        // Documents sync as current state, not a replayed chain — one entry.
        let (db, svc) = mock_sync_service();
        let t = db.create_thread(Thread::new("T".into(), "".into())).await.unwrap();
        let tid = t.id_string().unwrap();

        let doc = db.create_document(Document::new("Doc".into(), tid, true)).await.unwrap();
        let doc_id = doc.id_string().unwrap();

        db.commit_document(&doc_id, "first").await.unwrap();
        db.commit_document(&doc_id, "second").await.unwrap();

        let peer = remote_peer();
        register_peer(&svc, &peer);
        let result = svc.get_commits_since(&doc_id, None, &peer).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].document_id, doc_id);
    }

    #[tokio::test]
    async fn apply_commits_preserves_document_id() {
        // A document synced from a peer must land under its ORIGIN id so it
        // doesn't duplicate on the next manifest exchange (P2P-004).
        let (db, svc) = mock_sync_service();
        let peer = remote_peer();
        register_peer(&svc, &peer);
        let ec = seal_snapshot(
            "commit:remote_head".into(),
            "document:origin_abc".into(),
            "2026-02-01T00:00:00Z".into(),
            sovereign_db::schema::DocumentSnapshot {
                document_id: "document:origin_abc".into(),
                title: "Shared".into(),
                content: "shared body".into(),
                thread_id: "default".into(),
                ..Default::default()
            },
            &TEST_PAIR_KEY,
        )
        .unwrap();

        let n = svc.apply_commits(vec![sign_commit_as(ec, 0xD9)], &peer).await.unwrap();
        assert_eq!(n, 1);

        // Stored under the origin id, content intact.
        let got = db.get_document("document:origin_abc").await.unwrap();
        assert_eq!(got.content, "shared body");
        assert_eq!(got.title, "Shared");

        // Re-applying the same state is idempotent — still one document.
        let ec2 = seal_snapshot(
            "commit:remote_head".into(),
            "document:origin_abc".into(),
            "2026-02-02T00:00:00Z".into(),
            sovereign_db::schema::DocumentSnapshot {
                document_id: "document:origin_abc".into(),
                title: "Shared".into(),
                content: "shared body v2".into(),
                thread_id: "default".into(),
                ..Default::default()
            },
            &TEST_PAIR_KEY,
        )
        .unwrap();
        svc.apply_commits(vec![sign_commit_as(ec2, 0xD9)], &peer).await.unwrap();
        assert_eq!(db.list_documents(None).await.unwrap().len(), 1);
        assert_eq!(
            db.get_document("document:origin_abc").await.unwrap().content,
            "shared body v2"
        );
    }

    #[tokio::test]
    async fn apply_commits_creates_new_doc() {
        let (db, svc) = mock_sync_service();
        let peer = remote_peer();
        register_peer(&svc, &peer);

        // Seal the commit under the pair key shared with the test peer
        // so apply_commits can decrypt it (P2P-002 / P1.4).
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
                    thread_id: "default".into(),
                    ..Default::default()
                },
                signature: None,
            },
            &[7u8; 32],
        )
        .unwrap();

        let count = svc.apply_commits(vec![sign_commit_as(ec, 0xD9)], &peer).await.unwrap();
        assert_eq!(count, 1);

        // Verify a document was created
        let docs = db.list_documents(None).await.unwrap();
        assert_eq!(docs.len(), 1);
    }

    #[tokio::test]
    async fn apply_commits_new_doc_lands_in_transported_thread() {
        // Membership fix: the sealed snapshot now carries thread_id, so a
        // synced doc lands in its real lane instead of "default" (the root
        // cause of "N docs synced but only one lane renders").
        let (db, svc) = mock_sync_service();
        let peer = remote_peer();
        register_peer(&svc, &peer);

        let ec = commit_to_transport(
            &Commit {
                id: None,
                document_id: "document:in_thread".into(),
                parent_commit: None,
                message: "remote commit".into(),
                timestamp: chrono::Utc::now(),
                snapshot: sovereign_db::schema::DocumentSnapshot {
                    document_id: "document:in_thread".into(),
                    title: "Lane Doc".into(),
                    content: "body".into(),
                    thread_id: "thread:work".into(),
                    ..Default::default()
                },
                signature: None,
            },
            &[7u8; 32],
        )
        .unwrap();

        svc.apply_commits(vec![sign_commit_as(ec, 0xD9)], &peer).await.unwrap();
        let created = db.get_document("document:in_thread").await.unwrap();
        assert_eq!(
            created.thread_id, "thread:work",
            "synced doc must land in its transported thread, not 'default'"
        );
    }

    #[tokio::test]
    async fn apply_commits_updates_existing_doc() {
        let (db, svc) = mock_sync_service();
        let peer = remote_peer();
        register_peer(&svc, &peer);
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
                    thread_id: "default".into(),
                    ..Default::default()
                },
                signature: None,
            },
            &[7u8; 32],
        )
        .unwrap();

        let count = svc.apply_commits(vec![sign_commit_as(ec, 0xD9)], &peer).await.unwrap();
        assert_eq!(count, 1);

        // Verify the document was updated
        let updated = db.get_document(&doc_id).await.unwrap();
        assert_eq!(updated.title, "Updated Title");
        assert_eq!(updated.content, "updated content");
    }

    #[tokio::test]
    async fn apply_commits_peer_overwrite_is_nondestructive_and_flagged() {
        // p2p-no-per-doc-authz: a paired peer's overwrite must NOT silently
        // destroy the local version. The prior content is preserved as a
        // restorable commit, the new content is applied, and the doc is flagged
        // for review (surfaced on next open).
        let (db, svc) = mock_sync_service();
        let peer = remote_peer();
        register_peer(&svc, &peer);
        let t = db.create_thread(Thread::new("T".into(), "".into())).await.unwrap();
        let tid = t.id_string().unwrap();
        let doc = db
            .create_document(Document::new("Mine".into(), tid, true))
            .await
            .unwrap();
        let doc_id = doc.id_string().unwrap();
        db.update_document(&doc_id, Some("Mine"), Some("local body"))
            .await
            .unwrap();

        let ec = commit_to_transport(
            &Commit {
                id: None,
                document_id: doc_id.clone(),
                parent_commit: None,
                message: "peer change".into(),
                timestamp: chrono::Utc::now(),
                snapshot: sovereign_db::schema::DocumentSnapshot {
                    document_id: doc_id.clone(),
                    title: "Mine".into(),
                    content: "PEER BODY".into(),
                    thread_id: "default".into(),
                    ..Default::default()
                },
                signature: None,
            },
            &[7u8; 32],
        )
        .unwrap();
        svc.apply_commits(vec![sign_commit_as(ec, 0xD9)], &peer)
            .await
            .unwrap();

        let after = db.get_document(&doc_id).await.unwrap();
        assert_eq!(after.content, "PEER BODY", "peer content is applied");
        assert!(after.peer_review_pending, "doc must be flagged for review");
        assert_eq!(
            after.peer_review_peer.as_deref(),
            Some(peer.to_string().as_str()),
            "the originating peer must be recorded"
        );
        let prior = after
            .peer_review_prior_commit
            .clone()
            .expect("prior local version must be recorded as a commit");

        // The prior local version is fully restorable.
        let restored = db.restore_document(&doc_id, &prior).await.unwrap();
        assert_eq!(
            restored.content, "local body",
            "the pre-overwrite local version must be restorable"
        );
    }

    #[tokio::test]
    async fn apply_commits_rejects_unsigned_forged_author_and_wrong_signer() {
        // AUTOCOMMIT-001 / P2P-001: commit-envelope authentication.
        let (db, svc) = mock_sync_service();
        let peer = remote_peer(); // PeerId of test_keypair(0xD9)
        register_peer(&svc, &peer);

        fn make_ec() -> EncryptedCommit {
            seal_snapshot(
                "commit:h".into(),
                "document:victim".into(),
                "2026-02-01T00:00:00Z".into(),
                sovereign_db::schema::DocumentSnapshot {
                    document_id: "document:victim".into(),
                    title: "T".into(),
                    content: "x".into(),
                    thread_id: "default".into(),
                    ..Default::default()
                },
                &TEST_PAIR_KEY,
            )
            .unwrap()
        }

        // 1. Unsigned (empty signature) → rejected.
        let n = svc.apply_commits(vec![make_ec()], &peer).await.unwrap();
        assert_eq!(n, 0, "unsigned commit must be rejected");

        // 2. Signed by the sender but claiming a ghost author device → rejected.
        let mut forged_author = make_ec();
        forged_author.version_device = "ghost-device".into();
        sign_commit(&mut forged_author, &test_keypair(0xD9)).unwrap();
        let n = svc.apply_commits(vec![forged_author], &peer).await.unwrap();
        assert_eq!(n, 0, "forged version_device must be rejected");

        // 3. Signed by a DIFFERENT key than the claimed sender → rejected.
        let wrong_signer = sign_commit_as(make_ec(), 0xC7);
        let n = svc.apply_commits(vec![wrong_signer], &peer).await.unwrap();
        assert_eq!(n, 0, "commit signed by a non-sender key must be rejected");

        // 4. Correctly signed by the sender → applied.
        let n = svc.apply_commits(vec![sign_commit_as(make_ec(), 0xD9)], &peer).await.unwrap();
        assert_eq!(n, 1, "a correctly signed commit from the sender applies");
        assert!(db.get_document("document:victim").await.is_ok());
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
                thread_id: "default".into(),
                ..Default::default()
            },
            signature: None,
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

        // Forge a remote row whose modified_at is 48h in the future. Even
        // with a winning version stamp, the skew bound must reject it.
        let mut forged = db.get_thread(&tid).await.unwrap();
        forged.name = "Forged".into();
        forged.modified_at = chrono::Utc::now() + chrono::Duration::hours(48);
        let row = stamp(row_from_thread(&forged, &[7u8; 32]).unwrap(), 99, "device-2");

        let applied = svc.apply_thread_row(&row, &TEST_PAIR_KEY).await.unwrap();
        assert!(!applied, "far-future row must be rejected (skipped)");

        // Local row must be unchanged.
        let after = db.get_thread(&tid).await.unwrap();
        assert_eq!(after.name, "Original");
    }

    #[tokio::test]
    async fn apply_thread_row_accepts_newer_version_within_skew() {
        let (db, svc) = mock_sync_service();
        let local = db
            .create_thread(Thread::new("Original".into(), "orig desc".into()))
            .await
            .unwrap();
        let tid = local.id_string().unwrap();

        // A remote row with a higher Lamport stamp (and sane timestamp) wins.
        let mut newer = db.get_thread(&tid).await.unwrap();
        newer.name = "Updated".into();
        newer.modified_at = chrono::Utc::now() + chrono::Duration::seconds(5);
        let row = stamp(row_from_thread(&newer, &[7u8; 32]).unwrap(), 99, "device-2");

        let applied = svc.apply_thread_row(&row, &TEST_PAIR_KEY).await.unwrap();
        assert!(applied, "higher-version row should apply");
        let after = db.get_thread(&tid).await.unwrap();
        assert_eq!(after.name, "Updated");
    }

    #[tokio::test]
    async fn apply_entity_row_applies_newer_update() {
        use sovereign_db::schema::{Entity, EntityKind};

        let (db, svc) = mock_sync_service();
        let local = db
            .create_entity(Entity::new("Acme".into(), EntityKind::Org))
            .await
            .unwrap();
        let eid = local.id_string().unwrap();

        // Remote update: higher version stamp, new name + domain.
        let mut newer = db.get_entity(&eid).await.unwrap();
        newer.name = "Acme Corp".into();
        newer.domains = vec!["acme.com".into()];
        newer.modified_at = chrono::Utc::now() + chrono::Duration::seconds(5);
        let row = stamp(row_from_entity(&newer, &[7u8; 32]).unwrap(), 99, "device-2");

        let applied = svc.apply_entity_row(&row, &TEST_PAIR_KEY).await.unwrap();
        assert!(applied, "newer entity row should apply (P1.1: update_entity)");
        let after = db.get_entity(&eid).await.unwrap();
        assert_eq!(after.name, "Acme Corp");
        assert_eq!(after.domains, vec!["acme.com".to_string()]);
    }

    #[tokio::test]
    async fn apply_entity_row_rejects_far_future_timestamp() {
        use sovereign_db::schema::{Entity, EntityKind};

        let (db, svc) = mock_sync_service();
        let local = db
            .create_entity(Entity::new("Original".into(), EntityKind::Org))
            .await
            .unwrap();
        let eid = local.id_string().unwrap();

        let mut forged = db.get_entity(&eid).await.unwrap();
        forged.name = "Forged".into();
        forged.modified_at = chrono::Utc::now() + chrono::Duration::hours(48);
        let row = stamp(row_from_entity(&forged, &[7u8; 32]).unwrap(), 99, "device-2");

        let applied = svc.apply_entity_row(&row, &TEST_PAIR_KEY).await.unwrap();
        assert!(!applied, "far-future entity row must be rejected");
        let after = db.get_entity(&eid).await.unwrap();
        assert_eq!(after.name, "Original");
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
        let row = stamp(row_from_pii_record(&forged, &[7u8; 32]).unwrap(), 99, "device-2");

        let applied = svc.apply_pii_record_row(&row, &TEST_PAIR_KEY).await.unwrap();
        assert!(!applied, "far-future pii row must be rejected (skipped)");

        let after = db.get_pii_record(&pid).await.unwrap();
        assert_eq!(after.value_encrypted, "ENC_ORIGINAL");
    }

    // ── P1.3: signed Lamport version stamps (P2P-003 deep fix) ──────────

    #[tokio::test]
    async fn version_stamp_decides_not_timestamp() {
        let (db, svc) = mock_sync_service();
        let local = db
            .create_thread(Thread::new("Original".into(), String::new()))
            .await
            .unwrap();
        let tid = local.id_string().unwrap();

        // Remote row with an OLDER wall-clock timestamp but a HIGHER
        // version stamp must win (timestamps are forgeable; stamps are
        // what's signed).
        let mut remote = db.get_thread(&tid).await.unwrap();
        remote.name = "From remote".into();
        remote.modified_at = chrono::Utc::now() - chrono::Duration::hours(3);
        let row = stamp(row_from_thread(&remote, &[7u8; 32]).unwrap(), 50, "device-2");
        assert!(svc.apply_thread_row(&row, &TEST_PAIR_KEY).await.unwrap());
        assert_eq!(db.get_thread(&tid).await.unwrap().name, "From remote");

        // Remote row with a NEWER timestamp but a zero/absent version
        // stamp must lose against the local row's stamp.
        let mut forged = db.get_thread(&tid).await.unwrap();
        forged.name = "Timestamp forger".into();
        forged.modified_at = chrono::Utc::now() + chrono::Duration::seconds(30);
        let row = row_from_thread(&forged, &[7u8; 32]).unwrap(); // counter 0
        assert!(!svc.apply_thread_row(&row, &TEST_PAIR_KEY).await.unwrap());
        assert_eq!(db.get_thread(&tid).await.unwrap().name, "From remote");
    }

    #[tokio::test]
    async fn replayed_and_stale_rows_are_rejected() {
        let (db, svc) = mock_sync_service();
        let local = db
            .create_thread(Thread::new("Original".into(), String::new()))
            .await
            .unwrap();
        let tid = local.id_string().unwrap();

        let mut v5 = db.get_thread(&tid).await.unwrap();
        v5.name = "Fifth write".into();
        let row_v5 = stamp(row_from_thread(&v5, &[7u8; 32]).unwrap(), 5, "device-2");
        assert!(svc.apply_thread_row(&row_v5, &TEST_PAIR_KEY).await.unwrap());

        // Exact replay of the same row: same stamp → not strictly greater
        // → skipped.
        assert!(
            !svc.apply_thread_row(&row_v5, &TEST_PAIR_KEY).await.unwrap(),
            "replayed row must be skipped"
        );

        // A stale (rolled-back) earlier write from the same device.
        let mut v4 = db.get_thread(&tid).await.unwrap();
        v4.name = "Fourth write (rollback attempt)".into();
        let row_v4 = stamp(row_from_thread(&v4, &[7u8; 32]).unwrap(), 4, "device-2");
        assert!(
            !svc.apply_thread_row(&row_v4, &TEST_PAIR_KEY).await.unwrap(),
            "stale row must be rejected"
        );
        assert_eq!(db.get_thread(&tid).await.unwrap().name, "Fifth write");
    }

    #[tokio::test]
    async fn local_edit_outranks_already_seen_remote_version() {
        let (db, svc) = mock_sync_service();
        let local = db
            .create_thread(Thread::new("Original".into(), String::new()))
            .await
            .unwrap();
        let tid = local.id_string().unwrap();

        // Apply a remote write at counter 10.
        let mut remote = db.get_thread(&tid).await.unwrap();
        remote.name = "Remote v10".into();
        let row = stamp(row_from_thread(&remote, &[7u8; 32]).unwrap(), 10, "device-2");
        assert!(svc.apply_thread_row(&row, &TEST_PAIR_KEY).await.unwrap());

        // User edits locally — the lazy stamp must merge ABOVE counter 10,
        // so a replay of the remote v10 row no longer wins.
        db.update_thread(&tid, Some("Local edit after sync"), None)
            .await
            .unwrap();
        assert!(
            !svc.apply_thread_row(&row, &TEST_PAIR_KEY).await.unwrap(),
            "local edit must outrank the previously applied remote version"
        );
        assert_eq!(
            db.get_thread(&tid).await.unwrap().name,
            "Local edit after sync"
        );
    }

    #[tokio::test]
    async fn apply_rows_rejects_unsigned_and_wrong_signer() {
        let (db_a, svc_a) = mock_sync_service_with(0xA1, "device-a");
        let (db_b, svc_b) = mock_sync_service_with(0xB2, "device-b");
        let a_peer = test_keypair(0xA1).public().to_peer_id();

        // Pair keys both ways (the pair key is symmetric — A's entry for B
        // equals B's entry for A).
        let b_peer = test_keypair(0xB2).public().to_peer_id();
        register_peer(&svc_a, &b_peer);
        register_peer(&svc_b, &a_peer);

        // Seed A with a thread and let A package it for B (stamps + signs).
        let t = db_a
            .create_thread(Thread::new("Signed".into(), String::new()))
            .await
            .unwrap();
        let tid = t.id_string().unwrap();
        let signed_rows = svc_a
            .get_rows(SyncTable::Thread, &[tid.clone()], &b_peer)
            .await
            .unwrap();
        assert_eq!(signed_rows.len(), 1);
        assert!(!signed_rows[0].signature.is_empty());
        assert_eq!(signed_rows[0].version_counter, 1);
        // P2P-001: version_device is now the author's verifiable peer id.
        assert_eq!(signed_rows[0].version_device, a_peer.to_string());

        // 1. Valid: signed by A, presented as coming from A's peer id.
        let RowApplyReport { written, skipped, .. } = svc_b
            .apply_rows(SyncTable::Thread, signed_rows.clone(), &a_peer)
            .await
            .unwrap();
        assert_eq!((written, skipped), (1, 0));
        assert_eq!(db_b.list_threads().await.unwrap().len(), 1);

        // 2. Unsigned: same envelope with the signature stripped (the shape
        // a pre-P1.3 build would send).
        let mut unsigned = signed_rows[0].clone();
        unsigned.signature = String::new();
        let RowApplyReport { written, skipped, .. } = svc_b
            .apply_rows(SyncTable::Thread, vec![unsigned], &a_peer)
            .await
            .unwrap();
        assert_eq!((written, skipped), (0, 1), "unsigned row must be skipped");

        // 3. Wrong signer: signed with a key that is NOT the one embedded
        // in the claimed sender peer id.
        let t2 = Thread::new("Imposter".into(), String::new());
        let mut forged = row_from_thread(&t2, &[7u8; 32]).unwrap();
        forged.id = "thread:imposter".into();
        forged.version_counter = 9;
        forged.version_device = "device-x".into();
        sign_row(&mut forged, SyncTable::Thread, &test_keypair(0xC7)).unwrap();
        let RowApplyReport { written, skipped, .. } = svc_b
            .apply_rows(SyncTable::Thread, vec![forged], &a_peer)
            .await
            .unwrap();
        assert_eq!(
            (written, skipped),
            (0, 1),
            "row signed by a different key than the sender's peer id must be skipped"
        );
    }

    #[tokio::test]
    async fn apply_rows_rejects_forged_version_device() {
        // P2P-001: a row CORRECTLY signed by the sender but claiming authorship
        // by a device id that isn't the sender's verified peer id must be
        // rejected — otherwise a paired peer forges a ghost device + inflated
        // counter and overwrites any row cluster-wide via the LWW race.
        let (db_b, svc_b) = mock_sync_service_with(0xB2, "b");
        let a_peer = test_keypair(0xA1).public().to_peer_id();
        register_peer(&svc_b, &a_peer);

        // B already holds a thread.
        let t = db_b
            .create_thread(Thread::new("Victim".into(), String::new()))
            .await
            .unwrap();
        let tid = t.id_string().unwrap();

        // A honestly signs a row for B's thread id, but forges the version
        // stamp: a ghost device with the maximum counter.
        let mut forged =
            row_from_thread(&Thread::new("PWNED".into(), String::new()), &TEST_PAIR_KEY).unwrap();
        forged.id = tid.clone();
        forged.version_counter = u64::MAX;
        forged.version_device = "ghost-device-never-existed".into();
        sign_row(&mut forged, SyncTable::Thread, &test_keypair(0xA1)).unwrap();

        let RowApplyReport { written, skipped, .. } = svc_b
            .apply_rows(SyncTable::Thread, vec![forged], &a_peer)
            .await
            .unwrap();
        assert_eq!(
            (written, skipped),
            (0, 1),
            "a forged version_device (not the sender's peer id) must be rejected"
        );
        assert_eq!(
            db_b.get_thread(&tid).await.unwrap().name,
            "Victim",
            "the victim's row must be untouched"
        );
    }

    #[tokio::test]
    async fn apply_rows_caps_forged_high_counter_from_paired_peer() {
        // p2p-no-per-doc-authz: a row CORRECTLY signed by the sender AND claiming
        // authorship by the sender's REAL peer id (so it passes both the
        // signature and the version_device==sender gates) but carrying a
        // forged-high counter must be rejected by the counter-delta cap —
        // otherwise it wins the (counter, device) LWW race and silently
        // overwrites the victim's row. This is the bypass the two earlier
        // defensive tests do NOT cover (they use a ghost device / post-sign
        // tamper, both caught earlier in apply_rows).
        let (db_b, svc_b) = mock_sync_service_with(0xB2, "b");
        let a_keypair = test_keypair(0xA1);
        let a_peer = a_keypair.public().to_peer_id();
        register_peer(&svc_b, &a_peer);

        let t = db_b
            .create_thread(Thread::new("Victim".into(), String::new()))
            .await
            .unwrap();
        let tid = t.id_string().unwrap();

        // A signs a row for B's thread id with its REAL peer id as
        // version_device (passes the authorship gate) and a u64::MAX counter.
        let mut forged =
            row_from_thread(&Thread::new("PWNED".into(), String::new()), &TEST_PAIR_KEY).unwrap();
        forged.id = tid.clone();
        forged.version_counter = u64::MAX;
        forged.version_device = a_peer.to_string();
        sign_row(&mut forged, SyncTable::Thread, &a_keypair).unwrap();

        let RowApplyReport { written, skipped, .. } = svc_b
            .apply_rows(SyncTable::Thread, vec![forged], &a_peer)
            .await
            .unwrap();
        assert_eq!(
            (written, skipped),
            (0, 1),
            "a forged-high counter from a legitimately-paired peer must be capped"
        );
        assert_eq!(
            db_b.get_thread(&tid).await.unwrap().name,
            "Victim",
            "the victim's row must be untouched"
        );
    }

    #[tokio::test]
    async fn peer_overwrite_of_row_stashes_sealed_recovery() {
        // p2p-no-per-doc-authz row shadow-recovery: when a peer overwrites an
        // existing row, the prior value is preserved (sealed, pending review)
        // so it's never silently lost. A fresh create stashes nothing.
        use base64::Engine;
        let (db_a, svc_a) = mock_sync_service_with(0xA1, "device-a");
        let (db_b, svc_b) = mock_sync_service_with(0xB2, "device-b");
        let a_peer = test_keypair(0xA1).public().to_peer_id();
        let b_peer = test_keypair(0xB2).public().to_peer_id();
        register_peer(&svc_a, &b_peer);
        register_peer(&svc_b, &a_peer);

        // A creates a thread; B receives it (create — no recovery).
        let t = db_a
            .create_thread(Thread::new("Original".into(), "".into()))
            .await
            .unwrap();
        let tid = t.id_string().unwrap();
        let rows = svc_a
            .get_rows(SyncTable::Thread, &[tid.clone()], &b_peer)
            .await
            .unwrap();
        let RowApplyReport { written: w, .. } = svc_b.apply_rows(SyncTable::Thread, rows, &a_peer).await.unwrap();
        assert_eq!(w, 1);
        assert!(
            db_b.list_pending_row_recoveries().await.unwrap().is_empty(),
            "a fresh create must not stash a recovery"
        );

        // A edits the thread; B applies the overwrite → prior preserved.
        db_a.update_thread(&tid, Some("Updated"), None).await.unwrap();
        let rows2 = svc_a
            .get_rows(SyncTable::Thread, &[tid.clone()], &b_peer)
            .await
            .unwrap();
        let RowApplyReport { written: w2, .. } = svc_b.apply_rows(SyncTable::Thread, rows2, &a_peer).await.unwrap();
        assert_eq!(w2, 1, "overwrite must apply");
        assert_eq!(db_b.get_thread(&tid).await.unwrap().name, "Updated");

        let recs = db_b.list_pending_row_recoveries().await.unwrap();
        assert_eq!(recs.len(), 1, "overwrite must stash exactly one recovery");
        let rec = &recs[0];
        assert_eq!(rec.row_id, tid);
        assert_eq!(rec.table, "thread");
        assert_eq!(rec.peer, a_peer.to_string());
        assert!(rec.review_pending);

        // The sealed prior round-trips to the pre-overwrite value (and is NOT
        // plaintext on disk — it only decrypts under the transport key).
        let ct = base64::engine::general_purpose::STANDARD
            .decode(&rec.prior_ciphertext)
            .unwrap();
        let nonce_v = base64::engine::general_purpose::STANDARD
            .decode(&rec.prior_nonce)
            .unwrap();
        let mut nonce = [0u8; 24];
        nonce.copy_from_slice(&nonce_v);
        let plain = sovereign_crypto::aead::decrypt(&ct, &nonce, &TEST_PAIR_KEY).unwrap();
        let prior: Thread = serde_json::from_slice(&plain).unwrap();
        assert_eq!(prior.name, "Original", "prior value must be recoverable");
    }

    #[tokio::test]
    async fn restore_row_recovery_reverts_to_prior() {
        // The shadow-recovery is actually recoverable: restore re-applies the
        // sealed prior value and resolves the recovery.
        let (db_a, svc_a) = mock_sync_service_with(0xA1, "device-a");
        let (db_b, svc_b) = mock_sync_service_with(0xB2, "device-b");
        let a_peer = test_keypair(0xA1).public().to_peer_id();
        let b_peer = test_keypair(0xB2).public().to_peer_id();
        register_peer(&svc_a, &b_peer);
        register_peer(&svc_b, &a_peer);

        let t = db_a
            .create_thread(Thread::new("Original".into(), "orig desc".into()))
            .await
            .unwrap();
        let tid = t.id_string().unwrap();
        let rows = svc_a
            .get_rows(SyncTable::Thread, &[tid.clone()], &b_peer)
            .await
            .unwrap();
        svc_b.apply_rows(SyncTable::Thread, rows, &a_peer).await.unwrap();

        // Peer overwrites the thread → recovery stashed.
        db_a.update_thread(&tid, Some("Hijacked"), Some("bad desc")).await.unwrap();
        let rows2 = svc_a
            .get_rows(SyncTable::Thread, &[tid.clone()], &b_peer)
            .await
            .unwrap();
        svc_b.apply_rows(SyncTable::Thread, rows2, &a_peer).await.unwrap();
        assert_eq!(db_b.get_thread(&tid).await.unwrap().name, "Hijacked");

        let recs = db_b.list_pending_row_recoveries().await.unwrap();
        let rec_id = recs[0].id_string().unwrap();
        svc_b.restore_row_recovery(&rec_id).await.unwrap();

        assert_eq!(
            db_b.get_thread(&tid).await.unwrap().name,
            "Original",
            "restore must revert the row to its prior value"
        );
        assert!(
            db_b.list_pending_row_recoveries().await.unwrap().is_empty(),
            "the recovery must be resolved after restore"
        );
    }

    #[tokio::test]
    async fn tampered_signed_row_fails_verification() {
        let (db_a, svc_a) = mock_sync_service_with(0xA1, "device-a");
        let (db_b, svc_b) = mock_sync_service_with(0xB2, "device-b");
        let a_peer = test_keypair(0xA1).public().to_peer_id();
        let b_peer = test_keypair(0xB2).public().to_peer_id();
        register_peer(&svc_a, &b_peer);
        register_peer(&svc_b, &a_peer);

        let t = db_a
            .create_thread(Thread::new("Genuine".into(), String::new()))
            .await
            .unwrap();
        let tid = t.id_string().unwrap();
        let rows = svc_a.get_rows(SyncTable::Thread, &[tid], &b_peer).await.unwrap();

        // Inflate the signed counter — signature must no longer verify.
        let mut tampered = rows[0].clone();
        tampered.version_counter = 999;
        let RowApplyReport { written, skipped, .. } = svc_b
            .apply_rows(SyncTable::Thread, vec![tampered], &a_peer)
            .await
            .unwrap();
        assert_eq!((written, skipped), (0, 1), "tampered stamp must be rejected");
        assert!(db_b.list_threads().await.unwrap().is_empty());
    }

    #[test]
    fn row_sig_binds_table_and_delete_marker() {
        let key = [7u8; 32];
        let t = Thread::new("X".into(), String::new());
        let mut row = row_from_thread(&t, &key).unwrap();
        row.version_counter = 3;
        row.version_device = "device-a".into();
        let kp = test_keypair(0xA1);
        sign_row(&mut row, SyncTable::Thread, &kp).unwrap();
        let pubkey = kp.public();

        assert!(verify_row(&row, SyncTable::Thread, &pubkey));
        // Same envelope replayed against another table must fail.
        assert!(!verify_row(&row, SyncTable::Entity, &pubkey));
        // Flipping the soft-delete marker must fail.
        let mut deleted = row.clone();
        deleted.deleted_at = Some(String::new());
        assert!(!verify_row(&deleted, SyncTable::Thread, &pubkey));
    }

    #[test]
    fn peer_id_pubkey_roundtrip() {
        let kp = test_keypair(0x42);
        let peer = kp.public().to_peer_id();
        let extracted = public_key_from_peer_id(&peer).expect("ed25519 peer ids embed the key");
        assert_eq!(extracted, kp.public());
    }

    // ── P2: all-tables sync ──────────────────────────────────────────────

    use sovereign_db::schema::{
        ChannelType, EntityKind, MessageDirection, RelationType, SuggestionSource,
        SuggestionStatus,
    };

    struct SyncPair {
        db_a: Arc<MockGraphDB>,
        svc_a: SyncService,
        db_b: Arc<MockGraphDB>,
        svc_b: SyncService,
        a_peer: PeerId,
        b_peer: PeerId,
    }

    fn sync_pair() -> SyncPair {
        let (db_a, svc_a) = mock_sync_service_with(0xA1, "device-a");
        let (db_b, svc_b) = mock_sync_service_with(0xB2, "device-b");
        let a_peer = test_keypair(0xA1).public().to_peer_id();
        let b_peer = test_keypair(0xB2).public().to_peer_id();
        register_peer(&svc_a, &b_peer);
        register_peer(&svc_b, &a_peer);
        SyncPair { db_a, svc_a, db_b, svc_b, a_peer, b_peer }
    }

    /// Pull every row of `table` from A into B, returning (written, skipped).
    async fn pull_table(p: &SyncPair, table: SyncTable, ids: &[String]) -> (u32, u32) {
        let rows = p.svc_a.get_rows(table, ids, &p.b_peer).await.unwrap();
        assert_eq!(rows.len(), ids.len(), "every requested {table:?} row must be served");
        let r = p.svc_b.apply_rows(table, rows, &p.a_peer).await.unwrap();
        (r.written, r.skipped)
    }

    /// Per-table id lists from a manifest, in a fixed order.
    fn manifest_row_ids(m: &SyncManifest) -> Vec<(SyncTable, Vec<String>)> {
        vec![
            (SyncTable::Thread, m.threads.iter().map(|e| e.thread_id.clone()).collect()),
            (SyncTable::Entity, m.entities.iter().map(|e| e.entity_id.clone()).collect()),
            (SyncTable::PiiRecord, m.pii_records.iter().map(|e| e.record_id.clone()).collect()),
            (SyncTable::ShareRecord, m.share_records.iter().map(|e| e.record_id.clone()).collect()),
            (SyncTable::Contact, m.contacts.iter().map(|e| e.id.clone()).collect()),
            (SyncTable::Message, m.messages.iter().map(|e| e.id.clone()).collect()),
            (SyncTable::Conversation, m.conversations.iter().map(|e| e.id.clone()).collect()),
            (SyncTable::Milestone, m.milestones.iter().map(|e| e.id.clone()).collect()),
            (SyncTable::Relationship, m.relationships.iter().map(|e| e.id.clone()).collect()),
            (SyncTable::SuggestedLink, m.suggested_links.iter().map(|e| e.id.clone()).collect()),
        ]
    }

    #[tokio::test]
    async fn full_table_sync_converges_without_duplication() {
        let p = sync_pair();

        // Seed A with one row of every row-synced table.
        let thread = p.db_a.create_thread(Thread::new("T".into(), "d".into())).await.unwrap();
        let tid = thread.id_string().unwrap();
        p.db_a.create_entity(Entity::new("Acme".into(), EntityKind::Org)).await.unwrap();
        let contact = p.db_a.create_contact(Contact::new("Alice".into(), true)).await.unwrap();
        let cid = contact.id_string().unwrap();
        let conv = p
            .db_a
            .create_conversation(Conversation::new("Chat".into(), ChannelType::Email, vec![cid.clone()]))
            .await
            .unwrap();
        let vid = conv.id_string().unwrap();
        p.db_a
            .create_message(Message::new(
                vid.clone(),
                ChannelType::Email,
                MessageDirection::Inbound,
                cid.clone(),
                vec![],
                "hello".into(),
            ))
            .await
            .unwrap();
        p.db_a
            .create_milestone(Milestone::new("Shipped".into(), tid.clone(), "v1".into()))
            .await
            .unwrap();
        let d1 = p.db_a.create_document(Document::new("A".into(), tid.clone(), true)).await.unwrap();
        let d2 = p.db_a.create_document(Document::new("B".into(), tid.clone(), true)).await.unwrap();
        let d1_id = d1.id_string().unwrap();
        let d2_id = d2.id_string().unwrap();
        p.db_a
            .create_relationship(&d1_id, &d2_id, RelationType::References, 0.9)
            .await
            .unwrap();
        p.db_a
            .create_suggested_link(&d1_id, &d2_id, RelationType::Supports, 0.7, "related", SuggestionSource::Consolidation)
            .await
            .unwrap();

        // Round 1: pull every row table A → B.
        let manifest_a = p.svc_a.build_manifest().await.unwrap();
        assert_eq!(manifest_a.contacts.len(), 1);
        assert_eq!(manifest_a.messages.len(), 1);
        assert_eq!(manifest_a.conversations.len(), 1);
        assert_eq!(manifest_a.milestones.len(), 1);
        assert_eq!(manifest_a.relationships.len(), 1);
        assert_eq!(manifest_a.suggested_links.len(), 1);
        for (table, ids) in manifest_row_ids(&manifest_a) {
            let (written, skipped) = pull_table(&p, table, &ids).await;
            assert_eq!((written as usize, skipped), (ids.len(), 0), "round 1: {table:?}");
        }

        // Every row landed on B under its ORIGIN id.
        assert!(p.db_b.get_thread(&tid).await.is_ok());
        assert_eq!(p.db_b.get_contact(&cid).await.unwrap().name, "Alice");
        assert_eq!(p.db_b.get_conversation(&vid).await.unwrap().title, "Chat");
        assert_eq!(p.db_b.list_all_messages().await.unwrap().len(), 1);
        assert_eq!(p.db_b.list_all_milestones().await.unwrap().len(), 1);
        let rels_b = p.db_b.list_all_relationships().await.unwrap();
        assert_eq!(rels_b.len(), 1);
        assert_eq!(
            rels_b[0].in_.as_ref().map(sovereign_db::schema::thing_to_raw),
            Some(d1_id.clone()),
            "edge endpoints must survive the device boundary"
        );
        assert_eq!(p.db_b.list_all_suggested_links().await.unwrap().len(), 1);

        // The manifests now agree: no row table has any diff work. (The
        // pre-P2 minted-id bug made this loop re-fetch rows forever.)
        let manifest_b = p.svc_b.build_manifest().await.unwrap();
        let manifest_a2 = p.svc_a.build_manifest().await.unwrap();
        let diffs = crate::sync_engine::compute_all_row_diffs(&manifest_b, &manifest_a2);
        assert!(
            diffs.is_empty(),
            "all row tables must converge after one round, still dirty: {:?}",
            diffs.keys()
        );

        // Round 2: replaying the same rows is a no-op everywhere.
        for (table, ids) in manifest_row_ids(&manifest_a2) {
            let (written, _skipped) = pull_table(&p, table, &ids).await;
            assert_eq!(written, 0, "round 2 must write nothing for {table:?}");
        }
        assert_eq!(p.db_b.list_threads().await.unwrap().len(), 1);
        assert_eq!(p.db_b.list_contacts().await.unwrap().len(), 1);
        assert_eq!(p.db_b.list_all_messages().await.unwrap().len(), 1);
        assert_eq!(p.db_b.list_all_milestones().await.unwrap().len(), 1);
        assert_eq!(p.db_b.list_all_relationships().await.unwrap().len(), 1);
        assert_eq!(p.db_b.list_all_suggested_links().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn contact_update_and_soft_delete_propagate() {
        let p = sync_pair();
        let contact = p.db_a.create_contact(Contact::new("Alice".into(), true)).await.unwrap();
        let cid = contact.id_string().unwrap();

        // Initial sync.
        let (w, _) = pull_table(&p, SyncTable::Contact, &[cid.clone()]).await;
        assert_eq!(w, 1);

        // A renames + adds an address; the row re-syncs with a fresh stamp.
        p.db_a.update_contact(&cid, Some("Alice Smith"), None, None).await.unwrap();
        p.db_a
            .add_contact_address(
                &cid,
                sovereign_db::schema::ChannelAddress {
                    channel: ChannelType::Email,
                    address: "alice@example.com".into(),
                    display_name: None,
                    is_primary: true,
                },
            )
            .await
            .unwrap();
        let (w, s) = pull_table(&p, SyncTable::Contact, &[cid.clone()]).await;
        assert_eq!((w, s), (1, 0), "edited contact must re-apply");
        let on_b = p.db_b.get_contact(&cid).await.unwrap();
        assert_eq!(on_b.name, "Alice Smith");
        assert_eq!(on_b.addresses.len(), 1);
        assert_eq!(on_b.addresses[0].address, "alice@example.com");

        // A deleted-contact row soft-deletes B's copy.
        p.db_a.soft_delete_contact(&cid).await.unwrap();
        let row = row_from_contact(&p.db_a.get_contact(&cid).await.unwrap(), &TEST_PAIR_KEY).unwrap();
        // P2P-001: this row goes through apply_rows as sender A, so its
        // version_device must be A's verified peer id.
        let mut row = stamp(row, 99, &p.a_peer.to_string());
        sign_row(&mut row, SyncTable::Contact, &test_keypair(0xA1)).unwrap();
        let RowApplyReport { written: w, .. } = p
            .svc_b
            .apply_rows(SyncTable::Contact, vec![row], &p.a_peer)
            .await
            .unwrap();
        assert_eq!(w, 1);
        assert!(
            p.db_b.get_contact(&cid).await.unwrap().deleted_at.is_some(),
            "soft delete must propagate when the row is applied"
        );
    }

    // ── Deletion convergence (v0.0.9 H-p2p1: no resurrection of deleted data) ──

    #[tokio::test]
    async fn manifest_includes_tombstones() {
        let (db, svc) = mock_sync_service();
        let t = db.create_thread(Thread::new("T".into(), "".into())).await.unwrap();
        let tid = t.id_string().unwrap();
        let doc = db
            .create_document(Document::new("D".into(), tid.clone(), true))
            .await
            .unwrap();
        let did = doc.id_string().unwrap();

        db.soft_delete_document(&did).await.unwrap();
        db.soft_delete_thread(&tid).await.unwrap();

        let m = svc.build_manifest().await.unwrap();
        let de = m
            .documents
            .iter()
            .find(|e| e.doc_id == did)
            .expect("a deleted document must STAY in the manifest (H-p2p1)");
        assert!(de.deleted_at.is_some());
        assert!(
            de.content_hash.ends_with(":tombstone"),
            "deleted state must be part of the document's content identity"
        );
        let te = m
            .threads
            .iter()
            .find(|e| e.thread_id == tid)
            .expect("a deleted thread must stay in the manifest");
        assert!(te.deleted_at.is_some());
        // The LWW timestamp must advance to the deletion time, or the
        // tombstone loses every diff to the peer's live copy.
        let thread_row = db.get_thread(&tid).await.unwrap();
        let expected = std::cmp::max(
            thread_row.modified_at.to_rfc3339(),
            thread_row.deleted_at.clone().unwrap_or_default(),
        );
        assert_eq!(te.modified_at, expected);
    }

    #[tokio::test]
    async fn thread_deletion_converges_via_full_row_loop() {
        let p = sync_pair();
        let t = p.db_a.create_thread(Thread::new("Doomed".into(), "".into())).await.unwrap();
        let tid = t.id_string().unwrap();

        // Round 1: B receives the live thread.
        let (w, _) = pull_table(&p, SyncTable::Thread, &[tid.clone()]).await;
        assert_eq!(w, 1);
        assert!(p.db_b.get_thread(&tid).await.unwrap().deleted_at.is_none());

        // A deletes; round 2 re-pulls the same id (the manifest still lists
        // it — see manifest_includes_tombstones). The tombstone row gets a
        // fresh Lamport stamp (its content hash changed) and must win.
        p.db_a.soft_delete_thread(&tid).await.unwrap();
        let (w, s) = pull_table(&p, SyncTable::Thread, &[tid.clone()]).await;
        assert_eq!((w, s), (1, 0), "the tombstone row must apply");
        assert!(
            p.db_b.get_thread(&tid).await.unwrap().deleted_at.is_some(),
            "thread deletion must converge instead of B keeping the live copy"
        );
        assert!(
            p.db_b.list_threads().await.unwrap().iter().all(|x| x.id_string() != Some(tid.clone())),
            "the deleted thread must not list on B"
        );
    }

    #[tokio::test]
    async fn pii_redaction_converges_and_does_not_resurrect() {
        use sovereign_db::schema::{PiiKind, ReviewState};
        let p = sync_pair();
        let rec = p
            .db_a
            .create_pii_record(PiiRecord {
                id: None,
                kind: PiiKind::Email,
                value_encrypted: "ENC_SECRET".into(),
                value_nonce: "NONCE".into(),
                label: Some("work email".into()),
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
        let pid = rec.id_string().unwrap();

        let (w, _) = pull_table(&p, SyncTable::PiiRecord, &[pid.clone()]).await;
        assert_eq!(w, 1);

        // The user redacts (L5) on A — the review's headline resurrection
        // case: B's live copy must not survive, let alone flow back.
        p.db_a.soft_delete_pii_record(&pid).await.unwrap();
        let (w, s) = pull_table(&p, SyncTable::PiiRecord, &[pid.clone()]).await;
        assert_eq!((w, s), (1, 0), "the redaction tombstone must apply");
        assert!(
            p.db_b.get_pii_record(&pid).await.unwrap().deleted_at.is_some(),
            "redacted PII must be soft-deleted on the peer too"
        );
    }

    #[tokio::test]
    async fn document_deletion_propagates_via_commits_nondestructively() {
        let (db, svc) = mock_sync_service();
        let peer = remote_peer();
        register_peer(&svc, &peer);
        let doc = db
            .create_document(Document::new("Mine".into(), "default".into(), true))
            .await
            .unwrap();
        let doc_id = doc.id_string().unwrap();
        db.update_document(&doc_id, Some("Mine"), Some("precious body")).await.unwrap();

        // Peer's snapshot: same content, but deleted.
        let ec = commit_to_transport(
            &Commit {
                id: None,
                document_id: doc_id.clone(),
                parent_commit: None,
                message: "peer delete".into(),
                timestamp: chrono::Utc::now(),
                snapshot: sovereign_db::schema::DocumentSnapshot {
                    document_id: doc_id.clone(),
                    title: "Mine".into(),
                    content: "precious body".into(),
                    thread_id: "default".into(),
                    deleted_at: Some(chrono::Utc::now().to_rfc3339()),
                    ..Default::default()
                },
                signature: None,
            },
            &[7u8; 32],
        )
        .unwrap();
        svc.apply_commits(vec![sign_commit_as(ec, 0xD9)], &peer).await.unwrap();

        let after = db.get_document(&doc_id).await.unwrap();
        assert!(after.deleted_at.is_some(), "peer deletion must apply as a soft delete");
        assert!(after.peer_review_pending, "a peer deletion must be flagged for review");
        let prior = after
            .peer_review_prior_commit
            .clone()
            .expect("pre-delete state must be preserved as a commit");
        // Non-destructive: the content is fully restorable.
        let restored = db.restore_document(&doc_id, &prior).await.unwrap();
        assert_eq!(restored.content, "precious body");
    }

    #[tokio::test]
    async fn deleted_document_lands_as_tombstone_on_fresh_device() {
        let (db, svc) = mock_sync_service();
        let peer = remote_peer();
        register_peer(&svc, &peer);

        let ec = seal_snapshot(
            "commit:head".into(),
            "document:was_deleted".into(),
            "2026-03-01T00:00:00Z".into(),
            sovereign_db::schema::DocumentSnapshot {
                document_id: "document:was_deleted".into(),
                title: "Gone".into(),
                content: "body".into(),
                thread_id: "default".into(),
                deleted_at: Some("2026-03-02T00:00:00Z".into()),
                ..Default::default()
            },
            &TEST_PAIR_KEY,
        )
        .unwrap();
        svc.apply_commits(vec![sign_commit_as(ec, 0xD9)], &peer).await.unwrap();

        let got = db.get_document("document:was_deleted").await.unwrap();
        assert!(
            got.deleted_at.is_some(),
            "a deleted document syncing to a fresh device must land as a tombstone, not resurrect"
        );
        assert!(
            db.list_documents(None).await.unwrap().is_empty(),
            "the tombstone must not render as a live document"
        );
    }

    #[tokio::test]
    async fn message_read_status_propagates() {
        let p = sync_pair();
        let msg = p
            .db_a
            .create_message(Message::new(
                "conversation:c1".into(),
                ChannelType::Email,
                MessageDirection::Inbound,
                "contact:alice".into(),
                vec![],
                "unread mail".into(),
            ))
            .await
            .unwrap();
        let mid = msg.id_string().unwrap();

        let (w, _) = pull_table(&p, SyncTable::Message, &[mid.clone()]).await;
        assert_eq!(w, 1);
        assert_eq!(
            p.db_b.get_message(&mid).await.unwrap().read_status,
            sovereign_db::schema::ReadStatus::Unread
        );

        // A reads the message; the change re-syncs.
        p.db_a
            .update_message_read_status(&mid, sovereign_db::schema::ReadStatus::Read)
            .await
            .unwrap();
        let (w, s) = pull_table(&p, SyncTable::Message, &[mid.clone()]).await;
        assert_eq!((w, s), (1, 0));
        assert_eq!(
            p.db_b.get_message(&mid).await.unwrap().read_status,
            sovereign_db::schema::ReadStatus::Read
        );
        // Body rode along unchanged.
        assert_eq!(p.db_b.get_message(&mid).await.unwrap().body, "unread mail");
    }

    #[tokio::test]
    async fn suggested_link_status_propagates_without_promotion() {
        let p = sync_pair();
        let link = p
            .db_a
            .create_suggested_link(
                "document:a",
                "document:b",
                RelationType::Supports,
                0.7,
                "related topics",
                SuggestionSource::Consolidation,
            )
            .await
            .unwrap();
        let lid = link.id_string().unwrap();

        let (w, _) = pull_table(&p, SyncTable::SuggestedLink, &[lid.clone()]).await;
        assert_eq!(w, 1);
        assert_eq!(
            p.db_b.get_suggested_link(&lid).await.unwrap().status,
            SuggestionStatus::Pending
        );

        // User accepts on A (resolve_suggestion promotes A's edge locally).
        let bare_key = lid.split(':').nth(1).unwrap().to_string();
        p.db_a
            .resolve_suggestion(&bare_key, SuggestionStatus::Accepted)
            .await
            .unwrap();
        assert_eq!(p.db_a.list_all_relationships().await.unwrap().len(), 1);

        // Status change syncs to B WITHOUT re-promoting an edge there —
        // the promoted edge arrives separately through Relationship rows.
        let (w, s) = pull_table(&p, SyncTable::SuggestedLink, &[lid.clone()]).await;
        assert_eq!((w, s), (1, 0));
        let on_b = p.db_b.get_suggested_link(&lid).await.unwrap();
        assert_eq!(on_b.status, SuggestionStatus::Accepted);
        assert!(on_b.resolved_at.is_some());
        assert!(
            p.db_b.list_all_relationships().await.unwrap().is_empty(),
            "apply must not promote; the edge syncs as its own row"
        );
    }

    #[tokio::test]
    async fn milestones_and_relationships_are_append_only() {
        let p = sync_pair();
        let ms = p
            .db_a
            .create_milestone(Milestone::new("M1".into(), "thread:t".into(), String::new()))
            .await
            .unwrap();
        let mid = ms.id_string().unwrap();
        let rel = p
            .db_a
            .create_relationship("document:a", "document:b", RelationType::References, 0.5)
            .await
            .unwrap();
        let rid = rel.id_string().unwrap();

        let (w, _) = pull_table(&p, SyncTable::Milestone, &[mid.clone()]).await;
        assert_eq!(w, 1);
        let (w, _) = pull_table(&p, SyncTable::Relationship, &[rid.clone()]).await;
        assert_eq!(w, 1);

        // Replays are skipped, not duplicated.
        let (w, s) = pull_table(&p, SyncTable::Milestone, &[mid]).await;
        assert_eq!((w, s), (0, 1));
        let (w, s) = pull_table(&p, SyncTable::Relationship, &[rid]).await;
        assert_eq!((w, s), (0, 1));
        assert_eq!(p.db_b.list_all_milestones().await.unwrap().len(), 1);
        assert_eq!(p.db_b.list_all_relationships().await.unwrap().len(), 1);
    }
}
