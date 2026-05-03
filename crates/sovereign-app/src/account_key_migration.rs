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

// Tests for this module live alongside the e2e sync test in Phase 6
// (see plan §6.3). They need a tempfile dev-dep and a MockGraphDB
// instance preloaded with v0.0.4-style ciphertext, both of which are
// added when the test crate `crates/sovereign-app/tests/migration_test.rs`
// is wired up.
