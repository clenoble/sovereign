//! Sovereign-app glue for the PII detection pipeline.
//!
//! Step 4d of the PII management & dashboard plan. Ties the AI-layer
//! ingest hook to the SurrealDB-backed `AppState` and the document
//! create/import/save Tauri commands.
//!
//! Conditional on the `encryption` feature: PII ingest needs the
//! `DeviceKey` to encrypt findings and preserved raw bodies. When
//! encryption isn't enabled, every ingest call is a pass-through.
//!
//! Idempotent on re-saves: skips ingest when the document already has
//! a `pii_scanned_at` marker. The plan's idle sweep mechanism handles
//! re-scans for taxonomy changes.

use std::sync::Arc;

use sovereign_ai::pii::ingest::{ingest_text, GraphDbPiiSink};
use sovereign_ai::pii::pipeline::PipelineConfig;
use sovereign_db::schema::SourceKind;
use sovereign_db::traits::GraphDB;

use crate::tauri_state::AppState;

/// Run PII ingest over `body` if all preconditions hold; otherwise
/// return `body` unchanged. The returned string is the canonical body
/// (with `[pii:<record_id>]` tokens) ready to be stored as the
/// document's content.
///
/// Preconditions checked in order (each returns the body unchanged on
/// short-circuit):
///   1. `state.device_key` is `Some` (encryption initialized).
///   2. Document doesn't already have a `pii_scanned_at` timestamp
///      (idempotent — re-saves pass through; the idle sweep will
///      rescan if the taxonomy changes).
///
/// Side effects when ingest runs: PiiRecords + proposed Entities
/// written via `state.db`; document's `body_raw_encrypted` /
/// `body_raw_nonce` / `pii_scanned_at` fields updated.
pub async fn maybe_ingest_document_body(
    state: &AppState,
    doc_id: &str,
    body: &str,
) -> Result<String, String> {
    let device_key = match state.device_key.as_ref() {
        Some(dk) => dk.clone(),
        None => {
            tracing::debug!("PII ingest skipped: device_key unavailable");
            return Ok(body.to_string());
        }
    };

    let existing = state
        .db
        .get_document(doc_id)
        .await
        .map_err(|e| format!("get_document: {e}"))?;
    if existing.pii_scanned_at.is_some() {
        tracing::debug!(
            "PII ingest skipped: document {doc_id} already scanned at {:?}",
            existing.pii_scanned_at
        );
        return Ok(body.to_string());
    }

    let entities = state
        .db
        .list_entities()
        .await
        .map_err(|e| format!("list_entities: {e}"))?;
    let contacts = state
        .db
        .list_contacts()
        .await
        .map_err(|e| format!("list_contacts: {e}"))?;

    let db_dyn: Arc<dyn GraphDB> = state.db.clone();
    let sink = GraphDbPiiSink::new(db_dyn);

    // Regex-only for now. The LLM-NER stage gets wired in once the
    // orchestrator's backend is shareable from a Tauri command —
    // tracked separately. Regex-only is the plan's documented
    // fallback for low-priority paths.
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
        device_key.as_ref(),
    )
    .await
    .map_err(|e| format!("ingest_text: {e}"))?;

    state
        .db
        .update_document_pii_fields(
            doc_id,
            Some(&result.body_raw_encrypted),
            Some(&result.body_raw_nonce),
            Some(result.pii_scanned_at),
        )
        .await
        .map_err(|e| format!("update_document_pii_fields: {e}"))?;

    tracing::info!(
        "PII ingest: doc {doc_id} → {} records, {} proposed entities",
        result.record_ids.len(),
        result.created_entity_ids.len()
    );

    Ok(result.canonical_body)
}
