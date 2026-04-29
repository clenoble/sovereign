//! PII resolution Tauri commands.
//!
//! Step 5c of the PII management & dashboard plan. Glue between the
//! Svelte frontend and the AI-layer resolve module.
//!
//! Always compiles. The full implementation is gated on
//! `feature = "encryption"`; without that feature the command is a
//! stub returning a clear error so the frontend can surface the
//! limitation without needing its own feature-detection.

use serde::Serialize;
use sovereign_ai::pii::resolve::AccessLevel;
use sovereign_db::schema::{thing_to_raw, ReviewState};
use tauri::State;

use crate::err::ToStringErr;
use crate::tauri_state::AppState;

// ---------------------------------------------------------------------------
// DTOs (frontend-facing types — NO encrypted blobs)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct EntityDto {
    pub id: String,
    pub name: String,
    /// Lowercase string: "self", "org", "person", "service".
    pub kind: String,
    pub domains: Vec<String>,
    pub contact_ids: Vec<String>,
    pub notes: String,
    pub is_owned: bool,
    pub created_at: String,
    pub modified_at: String,
}

impl From<sovereign_db::schema::Entity> for EntityDto {
    fn from(e: sovereign_db::schema::Entity) -> Self {
        let kind = match e.kind {
            sovereign_db::schema::EntityKind::SelfEntity => "self",
            sovereign_db::schema::EntityKind::Org => "org",
            sovereign_db::schema::EntityKind::Person => "person",
            sovereign_db::schema::EntityKind::Service => "service",
        }
        .to_string();
        Self {
            id: e.id.as_ref().map(thing_to_raw).unwrap_or_default(),
            name: e.name,
            kind,
            domains: e.domains,
            contact_ids: e.contact_ids,
            notes: e.notes,
            is_owned: e.is_owned,
            created_at: e.created_at.to_rfc3339(),
            modified_at: e.modified_at.to_rfc3339(),
        }
    }
}

/// Frontend view of a PiiRecord. Deliberately omits `value_encrypted`
/// and `value_nonce` — the resolution flow is the only path to those.
#[derive(Debug, Serialize)]
pub struct PiiRecordDto {
    pub id: String,
    /// Snake_case kind string (e.g. "email", "person_name").
    pub kind: String,
    pub label: Option<String>,
    pub entity_id: Option<String>,
    pub stored_secret: bool,
    pub confidence: f32,
    /// "unreviewed" | "confirmed" | "dismissed".
    pub review_state: String,
    pub discovered_at: String,
    pub last_revealed_at: Option<String>,
    pub use_count: u32,
    pub sources: Vec<SourceRefDto>,
}

#[derive(Debug, Serialize)]
pub struct SourceRefDto {
    pub source_kind: String,
    pub source_id: String,
    pub span_start: usize,
    pub span_end: usize,
}

impl From<sovereign_db::schema::PiiRecord> for PiiRecordDto {
    fn from(r: sovereign_db::schema::PiiRecord) -> Self {
        let kind = match serde_json::to_value(&r.kind) {
            Ok(serde_json::Value::String(s)) => s,
            _ => "other".to_string(),
        };
        let review_state = match r.review_state {
            ReviewState::Unreviewed => "unreviewed",
            ReviewState::Confirmed => "confirmed",
            ReviewState::Dismissed => "dismissed",
        }
        .to_string();
        Self {
            id: r.id.as_ref().map(thing_to_raw).unwrap_or_default(),
            kind,
            label: r.label,
            entity_id: r.entity_id,
            stored_secret: r.stored_secret,
            confidence: r.confidence,
            review_state,
            discovered_at: r.discovered_at.to_rfc3339(),
            last_revealed_at: r.last_revealed_at.map(|t| t.to_rfc3339()),
            use_count: r.use_count,
            sources: r
                .sources
                .into_iter()
                .map(|s| SourceRefDto {
                    source_kind: match s.source_kind {
                        sovereign_db::schema::SourceKind::Document => "document",
                        sovereign_db::schema::SourceKind::Message => "message",
                        sovereign_db::schema::SourceKind::Contact => "contact",
                        sovereign_db::schema::SourceKind::SessionLog => "session_log",
                        sovereign_db::schema::SourceKind::UserInput => "user_input",
                    }
                    .to_string(),
                    source_id: s.source_id,
                    span_start: s.span_start,
                    span_end: s.span_end,
                })
                .collect(),
        }
    }
}

// ---------------------------------------------------------------------------
// Dashboard read paths
// ---------------------------------------------------------------------------

/// List all entities (excludes soft-deleted), ordered by name.
#[tauri::command]
pub async fn list_pii_entities(state: State<'_, AppState>) -> Result<Vec<EntityDto>, String> {
    let entities = state.db.list_entities().await.str_err()?;
    Ok(entities.into_iter().map(EntityDto::from).collect())
}

/// Fetch one entity by ID.
#[tauri::command]
pub async fn get_pii_entity(
    state: State<'_, AppState>,
    id: String,
) -> Result<EntityDto, String> {
    let entity = state.db.get_entity(&id).await.str_err()?;
    Ok(EntityDto::from(entity))
}

/// List PiiRecords matching the supplied filters. All filter args are
/// optional; passing none returns every non-deleted record.
///
/// `review_state` accepts "unreviewed" / "confirmed" / "dismissed";
/// any other value is silently ignored (treated as no filter).
#[tauri::command]
pub async fn list_pii_records(
    state: State<'_, AppState>,
    entity_id: Option<String>,
    review_state: Option<String>,
    stored_secret: Option<bool>,
) -> Result<Vec<PiiRecordDto>, String> {
    let parsed_state = review_state.as_deref().and_then(parse_review_state);
    let records = state
        .db
        .list_pii_records(entity_id.as_deref(), parsed_state, stored_secret)
        .await
        .str_err()?;
    Ok(records.into_iter().map(PiiRecordDto::from).collect())
}

// ---------------------------------------------------------------------------
// Dashboard write paths
// ---------------------------------------------------------------------------

/// Mark an Unreviewed PII finding as Confirmed (user agrees this is
/// real PII). Action level: Annotate (L2).
#[tauri::command]
pub async fn confirm_pii_record(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state
        .db
        .update_pii_record_review_state(&id, ReviewState::Confirmed)
        .await
        .str_err()
}

/// Mark an Unreviewed PII finding as Dismissed (user rejects it as a
/// false positive). Action level: Annotate (L2).
#[tauri::command]
pub async fn dismiss_pii_record(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state
        .db
        .update_pii_record_review_state(&id, ReviewState::Dismissed)
        .await
        .str_err()
}

/// Redact a PII record — soft-deletes it so the dashboard inventory
/// drops it. Action level: Destruct (L5) per the plan; the frontend
/// gates the confirmation prompt before calling.
#[tauri::command]
pub async fn redact_pii_record(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state.db.soft_delete_pii_record(&id).await.str_err()
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn parse_review_state(s: &str) -> Option<ReviewState> {
    match s {
        "unreviewed" => Some(ReviewState::Unreviewed),
        "confirmed" => Some(ReviewState::Confirmed),
        "dismissed" => Some(ReviewState::Dismissed),
        _ => None,
    }
}

/// Result returned by [`resolve_pii_tokens`]. The `access_level` echo
/// lets the frontend confirm what level was applied.
#[derive(Debug, Serialize)]
pub struct ResolvedBodyDto {
    pub body: String,
    pub access_level: AccessLevel,
}

/// Resolve every `[pii:<record_id>]` token in a Document or Message
/// body and return the rendered text.
///
/// `source_kind` is `"document"` or `"message"`. `access_level` is
/// `"preview"` / `"masked_sample"` / `"reveal"` / `"raw_original"`.
///
/// `Reveal` records `last_revealed_at` on every PiiRecord that was
/// successfully decrypted (handled inside `resolve_body`).
/// `RawOriginal` decrypts the source's `body_raw_encrypted` blob;
/// the frontend is responsible for the L3 confirmation prompt
/// before calling — the backend doesn't double-prompt.
#[cfg(feature = "encryption")]
#[tauri::command]
pub async fn resolve_pii_tokens(
    state: State<'_, AppState>,
    source_kind: String,
    source_id: String,
    access_level: AccessLevel,
) -> Result<ResolvedBodyDto, String> {
    use sovereign_ai::pii::resolve::{resolve_body, resolve_raw_original};
    use sovereign_db::traits::GraphDB;

    use crate::err::ToStringErr;

    let device_key = state
        .device_key
        .as_ref()
        .ok_or_else(|| "PII resolution unavailable: device key not loaded".to_string())?;

    let body = match source_kind.as_str() {
        "document" => {
            let doc = state.db.get_document(&source_id).await.str_err()?;
            match access_level {
                AccessLevel::RawOriginal => {
                    let enc = doc.body_raw_encrypted.as_deref().ok_or_else(|| {
                        "document has no body_raw_encrypted (likely never PII-scanned)"
                            .to_string()
                    })?;
                    let nonce = doc
                        .body_raw_nonce
                        .as_deref()
                        .ok_or_else(|| "document has no body_raw_nonce".to_string())?;
                    resolve_raw_original(device_key, enc, nonce).map_err(|e| e.to_string())?
                }
                _ => {
                    let body = extract_document_body(&doc.content);
                    resolve_body(
                        state.db.as_ref() as &dyn GraphDB,
                        device_key,
                        &body,
                        access_level,
                    )
                    .await
                }
            }
        }
        "message" => {
            let msg = state.db.get_message(&source_id).await.str_err()?;
            match access_level {
                AccessLevel::RawOriginal => {
                    let enc = msg.body_raw_encrypted.as_deref().ok_or_else(|| {
                        "message has no body_raw_encrypted (likely never PII-scanned)"
                            .to_string()
                    })?;
                    let nonce = msg
                        .body_raw_nonce
                        .as_deref()
                        .ok_or_else(|| "message has no body_raw_nonce".to_string())?;
                    resolve_raw_original(device_key, enc, nonce).map_err(|e| e.to_string())?
                }
                _ => resolve_body(
                    state.db.as_ref() as &dyn GraphDB,
                    device_key,
                    &msg.body,
                    access_level,
                )
                .await,
            }
        }
        other => return Err(format!("unknown source_kind: {other}")),
    };

    Ok(ResolvedBodyDto {
        body,
        access_level,
    })
}

/// Stub for builds without the `encryption` feature. The command is
/// still registered so the Tauri frontend doesn't need a parallel
/// feature flag — it just receives a clear error string.
#[cfg(not(feature = "encryption"))]
#[tauri::command]
pub async fn resolve_pii_tokens(
    _state: State<'_, AppState>,
    _source_kind: String,
    _source_id: String,
    _access_level: AccessLevel,
) -> Result<ResolvedBodyDto, String> {
    Err("PII resolution requires the encryption feature to be enabled at build time".to_string())
}

/// Extract the body text from a Document.content JSON blob — same
/// convention as `pii_sweep::extract_body`. Plain-text imports fall
/// back to the raw content.
#[cfg(feature = "encryption")]
fn extract_document_body(content: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(content) {
        if let Some(body) = v["body"].as_str() {
            return body.to_string();
        }
    }
    content.to_string()
}
