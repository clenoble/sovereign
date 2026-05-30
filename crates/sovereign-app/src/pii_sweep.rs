//! Idle-time PII sweep — rescans documents, messages, and contacts
//! that lack a `pii_scanned_at` marker and runs the regex-only
//! pipeline over each.
//!
//! Step 4e4 of the PII management & dashboard plan. Mirrors the
//! `sovereign-ai/src/consolidation.rs` pattern: a single cycle does a
//! bounded amount of work, returns stats, and the caller spawns a
//! tokio task in `main.rs` that loops with cooldowns.
//!
//! Takes `db` and `account_key` directly rather than `&AppState`, so
//! the idle-watcher closure in `main.rs` can capture cheap Arc
//! clones — `AppState` itself is held inside Tauri's state manager
//! and isn't easily reachable from a spawned task.

use std::sync::Arc;

use sovereign_ai::pii::ingest::{ingest_text, GraphDbPiiSink};
use sovereign_ai::pii::pipeline::PipelineConfig;
use sovereign_comms::pii_hook::{ContactIngestHook, MessageIngestHook};
use sovereign_crypto::account_key::AccountKey;
use sovereign_db::schema::{thing_to_raw, SourceKind};
use sovereign_db::traits::GraphDB;

use crate::pii_contact_hook::PiiContactHook;
use crate::pii_message_hook::PiiMessageHook;

/// Maximum items per source kind to scan in one sweep cycle. Keeps each
/// cycle bounded so it can run alongside other idle-watcher tasks
/// without monopolizing the model lock or the DB.
const BATCH_SIZE: usize = 10;

/// Per-cycle stats reported back to the caller for telemetry / logging.
#[derive(Debug, Clone, Default)]
pub struct SweepStats {
    pub documents_scanned: usize,
    pub messages_scanned: usize,
    pub contacts_scanned: usize,
}

impl SweepStats {
    pub fn total(&self) -> usize {
        self.documents_scanned + self.messages_scanned + self.contacts_scanned
    }
}

/// Run one sweep cycle. Returns the per-kind counts of items rescanned.
///
/// Idempotent: items already carrying a `pii_scanned_at` marker are
/// skipped. Re-scans (taxonomy bumps, regex updates) are triggered by
/// resetting `pii_scanned_at` to `NULL` on the affected rows; the next
/// sweep picks them up.
pub async fn run_sweep_cycle(
    db: Arc<dyn GraphDB>,
    account_key: Arc<AccountKey>,
) -> SweepStats {
    let mut stats = SweepStats::default();

    // --- Documents ---
    match db.list_documents(None).await {
        Ok(docs) => {
            let unscanned: Vec<_> = docs
                .into_iter()
                .filter(|d| d.pii_scanned_at.is_none())
                .take(BATCH_SIZE)
                .collect();
            for doc in unscanned {
                let id = match doc.id.as_ref().map(thing_to_raw) {
                    Some(s) => s,
                    None => continue,
                };
                let body = extract_body(&doc.content);
                match ingest_document_body(&db, account_key.as_ref(), &id, &body).await {
                    Ok(Some(canonical)) => {
                        // Body changed — persist the canonical form.
                        let new_content = replace_body(&doc.content, &canonical);
                        if let Err(e) = db.update_document(&id, None, Some(&new_content)).await
                        {
                            tracing::warn!("PII sweep: update_document {id} failed: {e}");
                            continue;
                        }
                        stats.documents_scanned += 1;
                    }
                    Ok(None) => {
                        // No findings; pii_scanned_at already set inside
                        // ingest_document_body. Body left untouched.
                        stats.documents_scanned += 1;
                    }
                    Err(e) => tracing::warn!("PII sweep: document {id} ingest failed: {e}"),
                }
            }
        }
        Err(e) => tracing::warn!("PII sweep: list_documents failed: {e}"),
    }

    // --- Messages — reuse the channel-side hook ---
    let message_hook: Arc<dyn MessageIngestHook> =
        Arc::new(PiiMessageHook::new(db.clone(), account_key.clone()));
    match db.list_all_messages().await {
        Ok(messages) => {
            let unscanned: Vec<_> = messages
                .into_iter()
                .filter(|m| m.pii_scanned_at.is_none())
                .take(BATCH_SIZE)
                .collect();
            for msg in unscanned {
                message_hook.after_message_created(&msg).await;
                stats.messages_scanned += 1;
            }
        }
        Err(e) => tracing::warn!("PII sweep: list_all_messages failed: {e}"),
    }

    // --- Contacts — reuse the channel-side hook ---
    let contact_hook: Arc<dyn ContactIngestHook> =
        Arc::new(PiiContactHook::new(db.clone(), account_key.clone()));
    match db.list_contacts().await {
        Ok(contacts) => {
            let unscanned: Vec<_> = contacts
                .into_iter()
                .filter(|c| c.pii_scanned_at.is_none())
                .take(BATCH_SIZE)
                .collect();
            for contact in unscanned {
                contact_hook.after_contact_created(&contact).await;
                stats.contacts_scanned += 1;
            }
        }
        Err(e) => tracing::warn!("PII sweep: list_contacts failed: {e}"),
    }

    if stats.total() > 0 {
        tracing::info!(
            "PII sweep: scanned {} docs, {} msgs, {} contacts",
            stats.documents_scanned,
            stats.messages_scanned,
            stats.contacts_scanned
        );
    }
    stats
}

/// Run the document ingest pipeline for one body and persist the
/// `body_raw_*` + `pii_scanned_at` fields. Returns the canonical body
/// when it differs from the input, `None` when nothing was rewritten
/// (e.g. no findings — but pii_scanned_at is still set so subsequent
/// sweeps skip the document).
async fn ingest_document_body(
    db: &Arc<dyn GraphDB>,
    account_key: &AccountKey,
    doc_id: &str,
    body: &str,
) -> anyhow::Result<Option<String>> {
    let entities = db.list_entities().await?;
    let contacts = db.list_contacts().await?;
    let sink = GraphDbPiiSink::new(db.clone());
    let config = PipelineConfig::default();

    let result = ingest_text(
        body,
        doc_id,
        SourceKind::Document,
        &config,
        None,
        None,
        &entities,
        &contacts,
        &sink,
        account_key,
    )
    .await?;

    db.update_document_pii_fields(
        doc_id,
        Some(&result.body_raw_encrypted),
        Some(&result.body_raw_nonce),
        Some(result.pii_scanned_at),
    )
    .await?;

    if result.canonical_body != body {
        Ok(Some(result.canonical_body))
    } else {
        Ok(None)
    }
}

/// Extract the body text from a Document.content JSON blob. Mirrors
/// the parsing logic in consolidation.rs::extract_body.
fn extract_body(content: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(content) {
        if let Some(body) = v["body"].as_str() {
            return body.to_string();
        }
    }
    content.to_string()
}

/// Rewrite the `body` field of a Document.content JSON blob, preserving
/// `images` / `videos` and any other fields. Falls back to setting the
/// content to the canonical body when the input isn't valid JSON
/// (some imports use plain text).
fn replace_body(content: &str, new_body: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(content) {
        Ok(mut v) => {
            if let Some(map) = v.as_object_mut() {
                map.insert(
                    "body".to_string(),
                    serde_json::Value::String(new_body.to_string()),
                );
                return serde_json::to_string(&v).unwrap_or_else(|_| new_body.to_string());
            }
            new_body.to_string()
        }
        Err(_) => new_body.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_body_from_json() {
        let content = r#"{"body":"hello","images":[]}"#;
        assert_eq!(extract_body(content), "hello");
    }

    #[test]
    fn extract_body_falls_back_to_raw() {
        // Plain text (e.g. an imported markdown file) — just returned as-is.
        assert_eq!(extract_body("plain markdown"), "plain markdown");
    }

    #[test]
    fn replace_body_preserves_other_fields() {
        let content = r#"{"body":"old","images":[{"path":"a.png"}]}"#;
        let out = replace_body(content, "new");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["body"], "new");
        assert_eq!(v["images"][0]["path"], "a.png");
    }

    #[test]
    fn replace_body_on_plain_text_returns_canonical() {
        // When input isn't JSON, just persist the canonical body.
        assert_eq!(replace_body("plain", "canonical"), "canonical");
    }
}
