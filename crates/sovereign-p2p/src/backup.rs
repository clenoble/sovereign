//! P4.1 — backup snapshot, sealing, key split, and erasure coding.
//!
//! The backup is a **logical export**: every synced table read decrypted
//! through the db handle (plaintext-on-sync-boundary model — raw DB
//! files are useless across devices because the at-rest key hierarchy is
//! device-local). The export is sealed under a **fresh random backup
//! key**, the key is Shamir-split across the user's guardians (the data
//! hosts alone can never attempt decryption), and the ciphertext is
//! Reed-Solomon erasure-coded into `n` fragments of which any `k`
//! reconstruct — losing hosts is harmless and no host holds the whole
//! snapshot (for n > 1).
//!
//! What is public by design (travels with the backup, per the plan):
//! the manifest, the MasterKey salt, and the fragments themselves
//! (opaque ciphertext). What is protected: the backup key — split into
//! guardian shards released only after guardian approval + the 72h
//! delay (see `backup_host`).

use sha2::{Digest, Sha256};
use serde::{Deserialize, Serialize};
use sovereign_db::GraphDB;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::{P2pError, P2pResult};

/// Default erasure coding: 3 data + 2 parity fragments (any 3 of 5
/// reconstruct) — mirrors the guardian 3-of-5 defaults.
pub const DEFAULT_DATA_FRAGMENTS: usize = 3;
pub const DEFAULT_PARITY_FRAGMENTS: usize = 2;

const SNAPSHOT_VERSION: u8 = 1;

/// Logical plaintext export of every synced table. Commit history is
/// intentionally absent — it is per-device (P1.2 crypto model); the
/// restored device starts a fresh chain from the restored content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupSnapshot {
    pub schema_version: u8,
    pub created_at: String,
    pub device_id: String,
    pub documents: Vec<sovereign_db::schema::Document>,
    pub threads: Vec<sovereign_db::schema::Thread>,
    pub entities: Vec<sovereign_db::schema::Entity>,
    pub pii_records: Vec<sovereign_db::schema::PiiRecord>,
    pub share_records: Vec<sovereign_db::schema::ShareRecord>,
    pub contacts: Vec<sovereign_db::schema::Contact>,
    pub messages: Vec<sovereign_db::schema::Message>,
    pub conversations: Vec<sovereign_db::schema::Conversation>,
    pub milestones: Vec<sovereign_db::schema::Milestone>,
    pub relationships: Vec<sovereign_db::schema::RelatedTo>,
    pub suggested_links: Vec<sovereign_db::schema::SuggestedLink>,
}

/// Read the full logical state through the (decrypting) db handle.
pub async fn build_snapshot(db: &dyn GraphDB, device_id: &str) -> P2pResult<BackupSnapshot> {
    let err = |what: &str, e: sovereign_db::DbError| {
        P2pError::SyncError(format!("backup snapshot {what}: {e}"))
    };
    Ok(BackupSnapshot {
        schema_version: SNAPSHOT_VERSION,
        created_at: chrono::Utc::now().to_rfc3339(),
        device_id: device_id.to_string(),
        documents: db.list_documents(None).await.map_err(|e| err("documents", e))?,
        threads: db.list_threads().await.map_err(|e| err("threads", e))?,
        entities: db.list_entities().await.map_err(|e| err("entities", e))?,
        pii_records: db
            .list_pii_records(None, None, None)
            .await
            .map_err(|e| err("pii_records", e))?,
        share_records: db
            .list_all_share_records()
            .await
            .map_err(|e| err("share_records", e))?,
        contacts: db.list_contacts().await.map_err(|e| err("contacts", e))?,
        messages: db.list_all_messages().await.map_err(|e| err("messages", e))?,
        conversations: db
            .list_conversations(None)
            .await
            .map_err(|e| err("conversations", e))?,
        milestones: db.list_all_milestones().await.map_err(|e| err("milestones", e))?,
        relationships: db
            .list_all_relationships()
            .await
            .map_err(|e| err("relationships", e))?,
        suggested_links: db
            .list_all_suggested_links()
            .await
            .map_err(|e| err("suggested_links", e))?,
    })
}

/// Restore a snapshot into a (fresh) database through the encrypting
/// handle, preserving every row's origin id (the id-preserving creates
/// from P2) so future sync with surviving devices doesn't duplicate.
/// Existing rows with the same id are left untouched (`Ok(false)` from
/// the with_id creates), so restoring over a non-empty DB is additive.
/// Returns the number of rows written.
pub async fn restore_snapshot(db: &dyn GraphDB, snapshot: &BackupSnapshot) -> P2pResult<u64> {
    let mut written = 0u64;
    let e = |what: &str, e: sovereign_db::DbError| {
        P2pError::SyncError(format!("backup restore {what}: {e}"))
    };

    // Threads first (documents reference them), then everything else.
    for t in &snapshot.threads {
        if db.create_thread_with_id(t.clone()).await.map_err(|x| e("thread", x))? {
            written += 1;
        }
    }
    for d in &snapshot.documents {
        if db.create_document_with_id(d.clone()).await.map_err(|x| e("document", x))? {
            written += 1;
        }
    }
    for x in &snapshot.entities {
        if db.create_entity_with_id(x.clone()).await.map_err(|x| e("entity", x))? {
            written += 1;
        }
    }
    for x in &snapshot.pii_records {
        if db.create_pii_record_with_id(x.clone()).await.map_err(|x| e("pii_record", x))? {
            written += 1;
        }
    }
    for x in &snapshot.share_records {
        if db
            .create_share_record_with_id(x.clone())
            .await
            .map_err(|x| e("share_record", x))?
        {
            written += 1;
        }
    }
    for x in &snapshot.contacts {
        if db.create_contact_with_id(x.clone()).await.map_err(|x| e("contact", x))? {
            written += 1;
        }
    }
    for x in &snapshot.conversations {
        if db
            .create_conversation_with_id(x.clone())
            .await
            .map_err(|x| e("conversation", x))?
        {
            written += 1;
        }
    }
    for x in &snapshot.messages {
        if db.create_message_with_id(x.clone()).await.map_err(|x| e("message", x))? {
            written += 1;
        }
    }
    for x in &snapshot.milestones {
        if db.create_milestone_with_id(x.clone()).await.map_err(|x| e("milestone", x))? {
            written += 1;
        }
    }
    for x in &snapshot.relationships {
        if db
            .create_relationship_with_id(x.clone())
            .await
            .map_err(|x| e("relationship", x))?
        {
            written += 1;
        }
    }
    for x in &snapshot.suggested_links {
        if db
            .create_suggested_link_with_id(x.clone())
            .await
            .map_err(|x| e("suggested_link", x))?
        {
            written += 1;
        }
    }
    Ok(written)
}

/// A fresh random backup key. Zeroized on drop; never persisted by the
/// owner — it exists only long enough to seal the snapshot and be
/// Shamir-split across the guardians.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct BackupKey(pub [u8; 32]);

impl BackupKey {
    pub fn generate() -> Self {
        use rand::Rng;
        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        Self(bytes)
    }
}

impl std::fmt::Debug for BackupKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BackupKey([REDACTED])")
    }
}

/// Seal a snapshot under the backup key. Returns (ciphertext, nonce).
pub fn seal_backup(snapshot: &BackupSnapshot, key: &BackupKey) -> P2pResult<(Vec<u8>, [u8; 24])> {
    let json = serde_json::to_vec(snapshot)
        .map_err(|e| P2pError::SyncError(format!("backup serialize: {e}")))?;
    sovereign_crypto::aead::encrypt(&json, &key.0)
        .map_err(|e| P2pError::SyncError(format!("backup seal: {e}")))
}

/// Unseal a backup ciphertext with the (guardian-reconstructed) key.
pub fn unseal_backup(
    ciphertext: &[u8],
    nonce: &[u8; 24],
    key: &[u8; 32],
) -> P2pResult<BackupSnapshot> {
    let plaintext = sovereign_crypto::aead::decrypt(ciphertext, nonce, key)
        .map_err(|_| P2pError::SyncError("backup unseal failed (wrong key?)".into()))?;
    serde_json::from_slice(&plaintext)
        .map_err(|e| P2pError::SyncError(format!("backup decode: {e}")))
}

pub fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    format!("{:x}", h.finalize())
}

/// One erasure-coded fragment of the sealed snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupFragment {
    pub index: u8,
    /// Base64 fragment bytes (data or parity shard).
    pub data_b64: String,
    /// SHA-256 of the raw fragment bytes — corrupted fragments are
    /// excluded before Reed-Solomon reconstruction (RS corrects
    /// erasures, not errors).
    pub digest: String,
}

/// Erasure-code a sealed snapshot into `data + parity` fragments; any
/// `data` of them reconstruct. The ciphertext is length-prefixed and
/// zero-padded to a multiple of `data` internally.
pub fn fragment_backup(
    ciphertext: &[u8],
    data: usize,
    parity: usize,
) -> P2pResult<Vec<BackupFragment>> {
    use base64::Engine;
    use reed_solomon_erasure::galois_8::ReedSolomon;

    if data == 0 || parity == 0 || data + parity > 255 {
        return Err(P2pError::SyncError(format!(
            "bad erasure parameters ({data} data + {parity} parity)"
        )));
    }
    let rs = ReedSolomon::new(data, parity)
        .map_err(|e| P2pError::SyncError(format!("reed-solomon init: {e}")))?;

    let shard_len = ciphertext.len().div_ceil(data).max(1);
    let mut shards: Vec<Vec<u8>> = Vec::with_capacity(data + parity);
    for i in 0..data {
        let start = (i * shard_len).min(ciphertext.len());
        let end = ((i + 1) * shard_len).min(ciphertext.len());
        let mut shard = ciphertext[start..end].to_vec();
        shard.resize(shard_len, 0);
        shards.push(shard);
    }
    for _ in 0..parity {
        shards.push(vec![0u8; shard_len]);
    }
    rs.encode(&mut shards)
        .map_err(|e| P2pError::SyncError(format!("reed-solomon encode: {e}")))?;

    Ok(shards
        .into_iter()
        .enumerate()
        .map(|(i, bytes)| BackupFragment {
            index: i as u8,
            digest: sha256_hex(&bytes),
            data_b64: base64::engine::general_purpose::STANDARD.encode(&bytes),
        })
        .collect())
}

/// Reassemble the sealed snapshot ciphertext from any `data` valid
/// fragments. Fragments failing their digest are dropped (treated as
/// erasures) before reconstruction.
pub fn reassemble_backup(
    fragments: &[BackupFragment],
    data: usize,
    parity: usize,
    ciphertext_len: u64,
) -> P2pResult<Vec<u8>> {
    use base64::Engine;
    use reed_solomon_erasure::galois_8::ReedSolomon;

    let total = data + parity;
    let rs = ReedSolomon::new(data, parity)
        .map_err(|e| P2pError::SyncError(format!("reed-solomon init: {e}")))?;

    let mut shards: Vec<Option<Vec<u8>>> = vec![None; total];
    for f in fragments {
        let idx = f.index as usize;
        if idx >= total {
            continue;
        }
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(&f.data_b64) else {
            tracing::warn!("backup fragment {idx}: bad base64, treating as erasure");
            continue;
        };
        if sha256_hex(&bytes) != f.digest {
            tracing::warn!("backup fragment {idx}: digest mismatch, treating as erasure");
            continue;
        }
        shards[idx] = Some(bytes);
    }

    let available = shards.iter().filter(|s| s.is_some()).count();
    if available < data {
        return Err(P2pError::SyncError(format!(
            "not enough valid fragments to reconstruct: {available} of {data} needed"
        )));
    }
    rs.reconstruct(&mut shards)
        .map_err(|e| P2pError::SyncError(format!("reed-solomon reconstruct: {e}")))?;

    // SUPPLY-001: `ciphertext_len` comes straight from the untrusted remote
    // manifest and the digest check runs only AFTER reassembly, so never
    // pre-allocate with it — a hostile `ciphertext_len = u64::MAX` would panic
    // ("capacity overflow") or OOM-abort the owner's recovery before any
    // integrity check, exactly when their device is already gone. The plaintext
    // can never exceed the reconstructed data shards, so bound it to that.
    let shard_len = shards.iter().flatten().map(|s| s.len()).next().unwrap_or(0);
    let max_len = data.saturating_mul(shard_len);
    let claimed = ciphertext_len as usize;
    if claimed > max_len {
        return Err(P2pError::SyncError(format!(
            "manifest ciphertext_len {claimed} exceeds reconstructable size {max_len}"
        )));
    }
    let mut out = Vec::with_capacity(claimed);
    for shard in shards.into_iter().take(data) {
        out.extend_from_slice(&shard.expect("reconstructed"));
    }
    out.truncate(claimed);
    Ok(out)
}

/// The non-secret record of one backup generation. Persisted by the
/// owner, replicated to every fragment host, and held by guardians
/// alongside their key shard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    pub snapshot_id: String,
    /// Monotonic backup generation; hosts keep only the newest epoch
    /// per owner.
    pub epoch: u32,
    pub created_at: String,
    /// Public owner identifier ([`AccountKey::derive_backup_tag`]).
    pub owner_tag: String,
    pub ciphertext_digest: String,
    pub ciphertext_len: u64,
    /// AEAD nonce for the sealed snapshot (not secret).
    pub nonce_b64: String,
    pub data_fragments: u8,
    pub parity_fragments: u8,
    /// Per-fragment digests, indexed by fragment index.
    pub fragment_digests: Vec<String>,
    /// Shamir threshold for the backup key.
    pub key_threshold: u8,
    /// guardian_id → shard_id of the backup-key shards.
    pub guardian_shards: Vec<(String, String)>,
}

impl BackupManifest {
    pub fn to_json(&self) -> P2pResult<String> {
        serde_json::to_string(self)
            .map_err(|e| P2pError::SyncError(format!("manifest encode: {e}")))
    }

    pub fn from_json(json: &str) -> P2pResult<Self> {
        serde_json::from_str(json)
            .map_err(|e| P2pError::SyncError(format!("manifest decode: {e}")))
    }
}

/// What each guardian holds for the backup (base64-of-JSON inside the
/// existing `DeliverShard` verb): their Shamir share of the backup key,
/// plus the salt + manifest a total-loss recovery needs to bootstrap.
/// NOT independently encrypted — the share alone is useless below the
/// threshold, and salt/manifest are public by design.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupGuardianPayload {
    pub schema_version: u8,
    pub owner_tag: String,
    pub epoch: u32,
    pub key_share_b64: String,
    pub salt_b64: String,
    pub manifest_json: String,
}

impl BackupGuardianPayload {
    pub fn new(owner_tag: String, epoch: u32, key_share: &[u8], salt: &[u8], manifest_json: String) -> Self {
        use base64::Engine;
        Self {
            schema_version: 1,
            owner_tag,
            epoch,
            key_share_b64: base64::engine::general_purpose::STANDARD.encode(key_share),
            salt_b64: base64::engine::general_purpose::STANDARD.encode(salt),
            manifest_json,
        }
    }

    pub fn encode(&self) -> P2pResult<String> {
        use base64::Engine;
        let json = serde_json::to_vec(self)
            .map_err(|e| P2pError::SyncError(format!("guardian payload encode: {e}")))?;
        Ok(base64::engine::general_purpose::STANDARD.encode(&json))
    }

    pub fn decode(b64: &str) -> P2pResult<Self> {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| P2pError::SyncError(format!("guardian payload base64: {e}")))?;
        serde_json::from_slice(&bytes)
            .map_err(|e| P2pError::SyncError(format!("guardian payload decode: {e}")))
    }
}

/// Everything `prepare_backup` produces for the app to distribute:
/// fragments to hosts, guardian payloads to guardians, manifest to disk.
pub struct PreparedBackup {
    pub manifest: BackupManifest,
    pub fragments: Vec<BackupFragment>,
    /// guardian_id → encoded [`BackupGuardianPayload`].
    pub guardian_payloads: Vec<(String, String)>,
}

/// P4.1 end-to-end: snapshot → seal under a fresh key → erasure-code →
/// Shamir-split the key across `guardian_ids` (threshold `key_threshold`).
/// The backup key is dropped (zeroized) before returning — after this,
/// only the guardian set can reconstruct it.
#[allow(clippy::too_many_arguments)]
pub async fn prepare_backup(
    db: &dyn GraphDB,
    device_id: &str,
    owner_tag: &str,
    salt: &[u8],
    epoch: u32,
    guardian_ids: &[String],
    key_threshold: u8,
    data_fragments: usize,
    parity_fragments: usize,
) -> P2pResult<PreparedBackup> {
    use base64::Engine;

    if guardian_ids.len() < key_threshold as usize {
        return Err(P2pError::SyncError(format!(
            "{} guardian(s) but key threshold is {key_threshold}",
            guardian_ids.len()
        )));
    }

    let snapshot = build_snapshot(db, device_id).await?;
    let key = BackupKey::generate();
    let (ciphertext, nonce) = seal_backup(&snapshot, &key)?;
    let fragments = fragment_backup(&ciphertext, data_fragments, parity_fragments)?;

    let shares = sovereign_crypto::guardian::shamir::split_secret(
        &key.0,
        key_threshold,
        guardian_ids.len(),
    )
    .map_err(|e| P2pError::SyncError(format!("backup key split: {e}")))?;

    let snapshot_id = {
        use rand::Rng;
        let mut id = [0u8; 16];
        rand::rng().fill_bytes(&mut id);
        id.iter().map(|b| format!("{b:02x}")).collect::<String>()
    };

    let mut manifest = BackupManifest {
        snapshot_id: snapshot_id.clone(),
        epoch,
        created_at: chrono::Utc::now().to_rfc3339(),
        owner_tag: owner_tag.to_string(),
        ciphertext_digest: sha256_hex(&ciphertext),
        ciphertext_len: ciphertext.len() as u64,
        nonce_b64: base64::engine::general_purpose::STANDARD.encode(nonce),
        data_fragments: data_fragments as u8,
        parity_fragments: parity_fragments as u8,
        fragment_digests: fragments.iter().map(|f| f.digest.clone()).collect(),
        key_threshold,
        guardian_shards: Vec::new(),
    };

    let manifest_json_placeholder = manifest.to_json()?;
    let mut guardian_payloads = Vec::with_capacity(guardian_ids.len());
    for (i, (gid, share)) in guardian_ids.iter().zip(shares.iter()).enumerate() {
        let shard_id = format!("{snapshot_id}-k{i}");
        manifest.guardian_shards.push((gid.clone(), shard_id));
        let payload = BackupGuardianPayload::new(
            owner_tag.to_string(),
            epoch,
            &sovereign_crypto::guardian::shamir::share_to_bytes(share),
            salt,
            manifest_json_placeholder.clone(),
        );
        guardian_payloads.push((gid.clone(), payload.encode()?));
    }

    Ok(PreparedBackup {
        manifest,
        fragments,
        guardian_payloads,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use sovereign_db::mock::MockGraphDB;
    use sovereign_db::schema::{Contact, Document, Entity, EntityKind, Milestone, Thread};

    async fn seeded_db() -> Arc<MockGraphDB> {
        let db = Arc::new(MockGraphDB::new());
        let t = db.create_thread(Thread::new("T".into(), "d".into())).await.unwrap();
        let tid = t.id_string().unwrap();
        let mut doc = Document::new("Doc".into(), tid.clone(), true);
        doc.content = r#"{"body":"backup me","images":[]}"#.into();
        db.create_document(doc).await.unwrap();
        db.create_entity(Entity::new("Acme".into(), EntityKind::Org)).await.unwrap();
        db.create_contact(Contact::new("Alice".into(), true)).await.unwrap();
        db.create_milestone(Milestone::new("M".into(), tid, String::new())).await.unwrap();
        db
    }

    #[tokio::test]
    async fn snapshot_seal_unseal_roundtrip() {
        let db = seeded_db().await;
        let snapshot = build_snapshot(db.as_ref(), "device-1").await.unwrap();
        assert_eq!(snapshot.documents.len(), 1);
        assert_eq!(snapshot.threads.len(), 1);

        let key = BackupKey::generate();
        let (ct, nonce) = seal_backup(&snapshot, &key).unwrap();
        assert!(
            !String::from_utf8_lossy(&ct).contains("backup me"),
            "snapshot content must not appear in the ciphertext"
        );
        let back = unseal_backup(&ct, &nonce, &key.0).unwrap();
        assert_eq!(back.documents[0].content, r#"{"body":"backup me","images":[]}"#);

        // Wrong key fails.
        assert!(unseal_backup(&ct, &nonce, &[0u8; 32]).is_err());
    }

    #[test]
    fn fragment_reassemble_with_losses_and_corruption() {
        let ciphertext: Vec<u8> = (0..10_000u32).map(|i| (i % 251) as u8).collect();
        let frags = fragment_backup(&ciphertext, 3, 2).unwrap();
        assert_eq!(frags.len(), 5);

        // All fragments present.
        let out = reassemble_backup(&frags, 3, 2, ciphertext.len() as u64).unwrap();
        assert_eq!(out, ciphertext);

        // Any 3 of 5 suffice (drop two).
        let subset = vec![frags[0].clone(), frags[3].clone(), frags[4].clone()];
        let out = reassemble_backup(&subset, 3, 2, ciphertext.len() as u64).unwrap();
        assert_eq!(out, ciphertext);

        // A corrupted fragment is detected via digest and treated as an
        // erasure — reconstruction still works with 3 valid ones left.
        let mut frags2 = frags.clone();
        frags2[1].data_b64 = frags2[0].data_b64.clone(); // wrong content for its digest
        let out = reassemble_backup(&frags2, 3, 2, ciphertext.len() as u64).unwrap();
        assert_eq!(out, ciphertext);

        // Only 2 valid fragments → fail.
        let too_few = vec![frags[1].clone(), frags[2].clone()];
        assert!(reassemble_backup(&too_few, 3, 2, ciphertext.len() as u64).is_err());
    }

    #[test]
    fn fragment_small_ciphertext() {
        // Smaller than the shard count — padding must handle it.
        let ciphertext = vec![7u8, 8, 9];
        let frags = fragment_backup(&ciphertext, 3, 2).unwrap();
        let out = reassemble_backup(&frags[1..].to_vec(), 3, 2, 3).unwrap();
        assert_eq!(out, ciphertext);
    }

    #[tokio::test]
    async fn prepare_backup_key_reconstructs_via_guardian_payloads() {
        let db = seeded_db().await;
        let guardians: Vec<String> = (1..=5).map(|i| format!("guardian-{i}")).collect();
        let prepared = prepare_backup(
            db.as_ref(),
            "device-1",
            "ownertag123",
            b"master-salt",
            1,
            &guardians,
            3,
            3,
            2,
        )
        .await
        .unwrap();

        assert_eq!(prepared.fragments.len(), 5);
        assert_eq!(prepared.guardian_payloads.len(), 5);
        assert_eq!(prepared.manifest.key_threshold, 3);
        assert_eq!(prepared.manifest.owner_tag, "ownertag123");

        // Reconstruct the backup key from any 3 guardian payloads and
        // decrypt the reassembled ciphertext end-to-end.
        use base64::Engine;
        let shares: Vec<_> = prepared.guardian_payloads[1..4]
            .iter()
            .map(|(_gid, b64)| {
                let p = BackupGuardianPayload::decode(b64).unwrap();
                assert_eq!(p.owner_tag, "ownertag123");
                assert_eq!(
                    base64::engine::general_purpose::STANDARD
                        .decode(&p.salt_b64)
                        .unwrap(),
                    b"master-salt"
                );
                sovereign_crypto::guardian::shamir::share_from_bytes(
                    &base64::engine::general_purpose::STANDARD
                        .decode(&p.key_share_b64)
                        .unwrap(),
                )
                .unwrap()
            })
            .collect();
        let key =
            sovereign_crypto::guardian::shamir::reconstruct_secret(&shares, 3).unwrap();

        let manifest = &prepared.manifest;
        let ct = reassemble_backup(
            &prepared.fragments[0..3].to_vec(),
            manifest.data_fragments as usize,
            manifest.parity_fragments as usize,
            manifest.ciphertext_len,
        )
        .unwrap();
        assert_eq!(sha256_hex(&ct), manifest.ciphertext_digest);

        let nonce_bytes = base64::engine::general_purpose::STANDARD
            .decode(&manifest.nonce_b64)
            .unwrap();
        let mut nonce = [0u8; 24];
        nonce.copy_from_slice(&nonce_bytes);
        let snapshot = unseal_backup(&ct, &nonce, &key).unwrap();
        assert_eq!(snapshot.documents.len(), 1);

        // Restore into a FRESH db preserves ids and content.
        let fresh = MockGraphDB::new();
        let written = restore_snapshot(&fresh, &snapshot).await.unwrap();
        assert!(written >= 4, "thread+doc+entity+contact+milestone expected");
        let docs = fresh.list_documents(None).await.unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].content, r#"{"body":"backup me","images":[]}"#);
        assert_eq!(
            docs[0].id_string(),
            snapshot.documents[0].id_string(),
            "restore must preserve origin ids for future sync"
        );

        // Restoring again over the same db is a no-op (idempotent).
        let again = restore_snapshot(&fresh, &snapshot).await.unwrap();
        assert_eq!(again, 0);
    }
}
