//! v0.0.4 → v0.0.5 migration: re-encrypt at-rest data under the user-scoped
//! AccountKey instead of the per-device DeviceKey.
//!
//! In v0.0.4, vault entries (`PiiRecord.value_encrypted`) and document /
//! message `body_raw_encrypted` were AEAD-encrypted directly under the
//! per-device `DeviceKey`. v0.0.5 replaces the direct DeviceKey usage
//! with `AccountKey` so paired devices can decrypt each other's data.
//! For existing v0.0.4 installs we walk every encrypted row once and
//! re-key it. The migration is gated by a marker file at
//! `~/.sovereign/crypto/account_key.migrated`; if the marker exists,
//! `migrate_to_account_key` is a no-op.

use std::path::Path;
use std::sync::Arc;

use sovereign_crypto::account_key::AccountKey;
use sovereign_crypto::device_key::DeviceKey;
use sovereign_crypto::vault::EncryptedBlob;
use sovereign_db::traits::GraphDB;

const MARKER_NAME: &str = "account_key.migrated";
const MARKER_VERSION: &str = "v1";

/// Per-cycle stats for telemetry.
#[derive(Debug, Clone, Default)]
pub struct MigrationReport {
    pub already_done: bool,
    pub pii_records: usize,
    pub documents: usize,
    pub messages: usize,
    pub session_log_renamed: bool,
    pub skipped_rows: usize,
}

/// Re-key every per-device-encrypted blob under the user-scoped AccountKey.
///
/// `db` is the decrypted graph DB (same instance used by the app).
/// `old_device_key` is the v0.0.4 per-device key derived from the just-
/// authenticated MasterKey + this device's `device_id`. `new_account_key`
/// is the user-scoped key derived from the same MasterKey.
///
/// Idempotent: writes a marker file at `<profile_dir>/crypto/account_key.migrated`
/// after a successful pass; subsequent calls return early.
///
/// Best-effort: rows that fail to decrypt under `old_device_key` are
/// counted in `skipped_rows` and left in place — the user will see them
/// as encrypted gibberish until they delete and re-create. We never
/// fail the login flow on a single bad row.
pub async fn migrate_to_account_key(
    db: Arc<dyn GraphDB>,
    old_device_key: &DeviceKey,
    new_account_key: &AccountKey,
    profile_dir: &Path,
) -> anyhow::Result<MigrationReport> {
    let marker = profile_dir.join("crypto").join(MARKER_NAME);
    if marker.exists() {
        return Ok(MigrationReport {
            already_done: true,
            ..Default::default()
        });
    }

    let mut report = MigrationReport::default();
    let old_bytes = old_device_key.as_bytes();

    // 1. PiiRecords (vault entries + discovered findings).
    let pii_records = db
        .list_pii_records(None, None, None)
        .await
        .map_err(|e| anyhow::anyhow!("list_pii_records: {e}"))?;
    for record in pii_records {
        let id = match record.id_string() {
            Some(s) => s,
            None => {
                report.skipped_rows += 1;
                continue;
            }
        };
        let blob = EncryptedBlob::from_pair(
            record.value_encrypted.clone(),
            record.value_nonce.clone(),
        );
        let plaintext = match blob.decrypt_with_key_bytes(old_bytes) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(
                    "AccountKey migration: skipping PiiRecord {id} (decrypt failed: {e})"
                );
                report.skipped_rows += 1;
                continue;
            }
        };
        let new_blob = match EncryptedBlob::encrypt(&plaintext, new_account_key) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(
                    "AccountKey migration: PiiRecord {id} re-encrypt failed: {e}"
                );
                report.skipped_rows += 1;
                continue;
            }
        };
        if let Err(e) = db
            .update_pii_record_value(&id, &new_blob.ciphertext_b64, &new_blob.nonce_b64)
            .await
        {
            tracing::warn!(
                "AccountKey migration: PiiRecord {id} update failed: {e}"
            );
            report.skipped_rows += 1;
            continue;
        }
        report.pii_records += 1;
    }

    // 2. Document body_raw_encrypted.
    let documents = db
        .list_documents(None)
        .await
        .map_err(|e| anyhow::anyhow!("list_documents: {e}"))?;
    for doc in documents {
        let (Some(enc), Some(nonce)) = (doc.body_raw_encrypted.clone(), doc.body_raw_nonce.clone())
        else {
            continue;
        };
        let id = match doc.id_string() {
            Some(s) => s,
            None => {
                report.skipped_rows += 1;
                continue;
            }
        };
        let blob = EncryptedBlob::from_pair(enc, nonce);
        let plaintext = match blob.decrypt_with_key_bytes(old_bytes) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(
                    "AccountKey migration: skipping Document {id} body_raw (decrypt failed: {e})"
                );
                report.skipped_rows += 1;
                continue;
            }
        };
        let new_blob = match EncryptedBlob::encrypt(&plaintext, new_account_key) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(
                    "AccountKey migration: Document {id} body_raw re-encrypt failed: {e}"
                );
                report.skipped_rows += 1;
                continue;
            }
        };
        if let Err(e) = db
            .update_document_pii_fields(
                &id,
                Some(&new_blob.ciphertext_b64),
                Some(&new_blob.nonce_b64),
                doc.pii_scanned_at,
            )
            .await
        {
            tracing::warn!(
                "AccountKey migration: Document {id} update failed: {e}"
            );
            report.skipped_rows += 1;
            continue;
        }
        report.documents += 1;
    }

    // 3. Message body_raw_encrypted.
    let messages = db
        .list_all_messages()
        .await
        .map_err(|e| anyhow::anyhow!("list_all_messages: {e}"))?;
    for msg in messages {
        let (Some(enc), Some(nonce)) = (msg.body_raw_encrypted.clone(), msg.body_raw_nonce.clone())
        else {
            continue;
        };
        let id = match msg.id_string() {
            Some(s) => s,
            None => {
                report.skipped_rows += 1;
                continue;
            }
        };
        let blob = EncryptedBlob::from_pair(enc, nonce);
        let plaintext = match blob.decrypt_with_key_bytes(old_bytes) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(
                    "AccountKey migration: skipping Message {id} body_raw (decrypt failed: {e})"
                );
                report.skipped_rows += 1;
                continue;
            }
        };
        let new_blob = match EncryptedBlob::encrypt(&plaintext, new_account_key) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(
                    "AccountKey migration: Message {id} body_raw re-encrypt failed: {e}"
                );
                report.skipped_rows += 1;
                continue;
            }
        };
        if let Err(e) = db
            .update_message_pii_fields(
                &id,
                Some(&new_blob.ciphertext_b64),
                Some(&new_blob.nonce_b64),
                msg.pii_scanned_at,
            )
            .await
        {
            tracing::warn!(
                "AccountKey migration: Message {id} update failed: {e}"
            );
            report.skipped_rows += 1;
            continue;
        }
        report.messages += 1;
    }

    // 4. Session log: rename old file to start fresh under the new key.
    //    Cheaper than re-encrypting line-by-line; one device's session
    //    log history is acceptable to lose on the migration boundary.
    let session_log = profile_dir.join("orchestrator").join("session_log.jsonl");
    if session_log.exists() {
        let backup = profile_dir.join("orchestrator").join("session_log.v0.4.bak");
        match std::fs::rename(&session_log, &backup) {
            Ok(()) => {
                report.session_log_renamed = true;
                tracing::info!(
                    "AccountKey migration: session log archived to {}",
                    backup.display()
                );
            }
            Err(e) => {
                tracing::warn!(
                    "AccountKey migration: failed to rename session log: {e}"
                );
            }
        }
    }

    // 5. Write the marker so subsequent launches skip the migration.
    let marker_dir = profile_dir.join("crypto");
    if let Err(e) = std::fs::create_dir_all(&marker_dir) {
        tracing::warn!("AccountKey migration: marker dir create failed: {e}");
    }
    let marker_body = if report.skipped_rows > 0 {
        format!("{MARKER_VERSION} with {} skipped\n", report.skipped_rows)
    } else {
        format!("{MARKER_VERSION}\n")
    };
    if let Err(e) = std::fs::write(&marker, marker_body) {
        // Marker write failure is unfortunate but not fatal — the next
        // launch will re-run the migration. All updates above were
        // idempotent at the DB level (re-running just re-keys again
        // under the same AccountKey), so this is safe.
        tracing::warn!("AccountKey migration: marker write failed: {e}");
    } else {
        tracing::info!(
            "AccountKey migration complete: {} pii_records, {} documents, {} messages, {} skipped",
            report.pii_records,
            report.documents,
            report.messages,
            report.skipped_rows
        );
    }

    Ok(report)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
//
// `sovereign-app` is a binary-only crate, so we can't host the test in
// `tests/migration_test.rs` (no library target to link against). The
// in-module tests below cover the same surface as the plan's §6.3:
//   - encrypt-under-DeviceKey -> migrate -> decrypt-under-AccountKey
//     round trip for PiiRecord, Document body_raw, and Message body_raw.
//   - Marker idempotency (second migrate call returns early).
//   - Skipped-row counter when a row's ciphertext can't be decrypted
//     under the supplied DeviceKey.

#[cfg(test)]
mod tests {
    use super::*;
    use sovereign_crypto::aead;
    use sovereign_crypto::master_key::MasterKey;
    use sovereign_db::mock::MockGraphDB;
    use sovereign_db::schema::{
        ChannelType, Document, Message, MessageDirection, PiiKind, PiiRecord, ReviewState,
        Thread,
    };

    fn keys() -> (DeviceKey, AccountKey) {
        let mk = MasterKey::from_passphrase(b"migration-test-pass", b"shared-salt-32B!")
            .unwrap();
        let dk = DeviceKey::derive(&mk, "device-001").unwrap();
        let ak = AccountKey::derive(&mk).unwrap();
        (dk, ak)
    }

    /// Encrypt `plaintext` under raw 32-byte key bytes, returning the
    /// (ciphertext_b64, nonce_b64) pair the v0.0.4 schema stored.
    fn encrypt_under_bytes(plaintext: &[u8], key_bytes: &[u8; 32]) -> (String, String) {
        use base64::Engine;
        let (ct, nonce) = aead::encrypt(plaintext, key_bytes).unwrap();
        let b64 = base64::engine::general_purpose::STANDARD.encode(&ct);
        let nonce_b64 = base64::engine::general_purpose::STANDARD.encode(nonce);
        (b64, nonce_b64)
    }

    fn make_pii_record(value_encrypted: String, value_nonce: String) -> PiiRecord {
        PiiRecord {
            id: None,
            kind: PiiKind::Note,
            value_encrypted,
            value_nonce,
            label: Some("test note".into()),
            entity_id: None,
            stored_secret: true,
            confidence: 1.0,
            sources: vec![],
            discovered_at: chrono::Utc::now(),
            last_revealed_at: None,
            use_count: 0,
            review_state: ReviewState::Confirmed,
            deleted_at: None,
        }
    }

    #[tokio::test]
    async fn migrates_pii_record_from_device_key_to_account_key() {
        let (old_dk, new_ak) = keys();
        let db: Arc<dyn GraphDB> = Arc::new(MockGraphDB::new());

        // v0.0.4 row: ciphertext under DeviceKey.
        let plaintext = b"my-secret-vault-value";
        let (ct, nonce) = encrypt_under_bytes(plaintext, old_dk.as_bytes());
        db.create_pii_record(make_pii_record(ct, nonce)).await.unwrap();

        let tmp = tempfile::tempdir().unwrap();
        let report = migrate_to_account_key(db.clone(), &old_dk, &new_ak, tmp.path())
            .await
            .unwrap();
        assert!(!report.already_done);
        assert_eq!(report.pii_records, 1);
        assert_eq!(report.skipped_rows, 0);

        // Verify the stored ciphertext now decrypts cleanly under AccountKey.
        let listed = db.list_pii_records(None, None, None).await.unwrap();
        assert_eq!(listed.len(), 1);
        let row = &listed[0];
        let blob = EncryptedBlob::from_pair(
            row.value_encrypted.clone(),
            row.value_nonce.clone(),
        );
        let recovered = blob.decrypt(&new_ak).unwrap();
        assert_eq!(recovered, plaintext);

        // Marker file written.
        assert!(tmp.path().join("crypto").join(MARKER_NAME).exists());
    }

    #[tokio::test]
    async fn marker_makes_second_call_no_op() {
        let (old_dk, new_ak) = keys();
        let db: Arc<dyn GraphDB> = Arc::new(MockGraphDB::new());

        let plaintext = b"value";
        let (ct, nonce) = encrypt_under_bytes(plaintext, old_dk.as_bytes());
        db.create_pii_record(make_pii_record(ct, nonce)).await.unwrap();

        let tmp = tempfile::tempdir().unwrap();
        let first = migrate_to_account_key(db.clone(), &old_dk, &new_ak, tmp.path())
            .await
            .unwrap();
        assert!(!first.already_done);
        assert_eq!(first.pii_records, 1);

        // Add a second row that *would* be migrated if we ran again.
        let (ct2, nonce2) = encrypt_under_bytes(b"second", old_dk.as_bytes());
        db.create_pii_record(make_pii_record(ct2, nonce2)).await.unwrap();

        // Second call returns early — marker present.
        let second = migrate_to_account_key(db.clone(), &old_dk, &new_ak, tmp.path())
            .await
            .unwrap();
        assert!(second.already_done);
        assert_eq!(second.pii_records, 0);

        // The newly-added second row was NOT migrated (still encrypted
        // under DeviceKey). This is intentional: the marker says "v0.0.4
        // data has been re-keyed once"; new rows written by post-v0.0.5
        // code paths are already AccountKey-encrypted.
    }

    #[tokio::test]
    async fn migrates_document_body_raw() {
        let (old_dk, new_ak) = keys();
        let db: Arc<dyn GraphDB> = Arc::new(MockGraphDB::new());

        // Documents need a thread to live in.
        let thread = db
            .create_thread(Thread::new("T".into(), String::new()))
            .await
            .unwrap();
        let tid = thread.id_string().unwrap();

        let mut doc = Document::new("Doc".into(), tid, true);
        let plaintext = b"the document body contains a secret";
        let (ct, nonce) = encrypt_under_bytes(plaintext, old_dk.as_bytes());
        doc.body_raw_encrypted = Some(ct);
        doc.body_raw_nonce = Some(nonce);
        db.create_document(doc).await.unwrap();

        let tmp = tempfile::tempdir().unwrap();
        let report = migrate_to_account_key(db.clone(), &old_dk, &new_ak, tmp.path())
            .await
            .unwrap();
        assert_eq!(report.documents, 1);
        assert_eq!(report.skipped_rows, 0);

        let docs = db.list_documents(None).await.unwrap();
        let body = EncryptedBlob::from_pair(
            docs[0].body_raw_encrypted.clone().unwrap(),
            docs[0].body_raw_nonce.clone().unwrap(),
        );
        assert_eq!(body.decrypt(&new_ak).unwrap(), plaintext);
    }

    #[tokio::test]
    async fn migrates_message_body_raw() {
        let (old_dk, new_ak) = keys();
        let db: Arc<dyn GraphDB> = Arc::new(MockGraphDB::new());

        // A minimal Message with body_raw_encrypted populated.
        let mut msg = Message::new(
            "conv:123".into(),
            ChannelType::Email,
            MessageDirection::Inbound,
            "contact:alice".into(),
            vec!["contact:me".into()],
            "plaintext body".into(),
        );
        let plaintext = b"the message body contains a secret too";
        let (ct, nonce) = encrypt_under_bytes(plaintext, old_dk.as_bytes());
        msg.body_raw_encrypted = Some(ct);
        msg.body_raw_nonce = Some(nonce);
        db.create_message(msg).await.unwrap();

        let tmp = tempfile::tempdir().unwrap();
        let report = migrate_to_account_key(db.clone(), &old_dk, &new_ak, tmp.path())
            .await
            .unwrap();
        assert_eq!(report.messages, 1);

        let msgs = db.list_all_messages().await.unwrap();
        let body = EncryptedBlob::from_pair(
            msgs[0].body_raw_encrypted.clone().unwrap(),
            msgs[0].body_raw_nonce.clone().unwrap(),
        );
        assert_eq!(body.decrypt(&new_ak).unwrap(), plaintext);
    }

    #[tokio::test]
    async fn rows_with_unrelated_ciphertext_are_skipped_not_failed() {
        // A row encrypted under a *different* DeviceKey can't be
        // decrypted by the migration; it must be skipped (counted) and
        // the migration must still complete + write the marker.
        let (_old_dk, new_ak) = keys();
        let db: Arc<dyn GraphDB> = Arc::new(MockGraphDB::new());

        // Use a key the migration won't see.
        let other_mk =
            MasterKey::from_passphrase(b"unrelated-pass", b"unrelated-salt!!").unwrap();
        let other_dk = DeviceKey::derive(&other_mk, "device-XYZ").unwrap();
        let (ct, nonce) = encrypt_under_bytes(b"unreadable", other_dk.as_bytes());
        db.create_pii_record(make_pii_record(ct, nonce)).await.unwrap();

        // Run migration with the *expected* old DeviceKey, which
        // cannot decrypt the row above.
        let (expected_old_dk, _) = keys();
        let tmp = tempfile::tempdir().unwrap();
        let report =
            migrate_to_account_key(db.clone(), &expected_old_dk, &new_ak, tmp.path())
                .await
                .unwrap();
        assert_eq!(report.pii_records, 0);
        assert_eq!(report.skipped_rows, 1);
        // Marker still written so we don't loop forever on bad data.
        assert!(tmp.path().join("crypto").join(MARKER_NAME).exists());
    }
}
