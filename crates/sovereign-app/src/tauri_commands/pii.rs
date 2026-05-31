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
use sovereign_db::schema::{thing_to_raw, ReviewState, ShareChannel, ShareRecord};
use sovereign_db::GraphDB;
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

/// Frontend view of a ShareRecord — sharing-ledger entry.
#[derive(Debug, Serialize)]
pub struct ShareRecordDto {
    pub id: String,
    pub pii_record_id: String,
    pub to_entity_id: String,
    pub via_message_id: Option<String>,
    pub via_url: Option<String>,
    pub shared_at: String,
    /// Lowercase channel string ("email" | "signal" | "whatsapp" | "sms"
    /// | "matrix" | "phone" | "web" | "other").
    pub channel: String,
}

impl From<ShareRecord> for ShareRecordDto {
    fn from(r: ShareRecord) -> Self {
        let channel = match r.channel {
            ShareChannel::Email => "email",
            ShareChannel::Sms => "sms",
            ShareChannel::Signal => "signal",
            ShareChannel::WhatsApp => "whatsapp",
            ShareChannel::Matrix => "matrix",
            ShareChannel::Phone => "phone",
            ShareChannel::Web => "web",
            ShareChannel::Other => "other",
        }
        .to_string();
        Self {
            id: r.id.as_ref().map(thing_to_raw).unwrap_or_default(),
            pii_record_id: r.pii_record_id,
            to_entity_id: r.to_entity_id,
            via_message_id: r.via_message_id,
            via_url: r.via_url,
            shared_at: r.shared_at.to_rfc3339(),
            channel,
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

/// List share-ledger entries where the recipient entity is `entity_id`.
/// Used by the dashboard's Shared tab. Order: most-recently-shared first.
#[tauri::command]
pub async fn list_share_records_for_entity(
    state: State<'_, AppState>,
    entity_id: String,
) -> Result<Vec<ShareRecordDto>, String> {
    let records = state
        .db
        .list_share_records_for_entity(&entity_id)
        .await
        .str_err()?;
    Ok(records.into_iter().map(ShareRecordDto::from).collect())
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

/// Reveal a single PiiRecord's plaintext value. L3 Modify — bumps
/// `last_revealed_at`. Used by the dashboard's per-row reveal button
/// (one click per record, vs. resolve_pii_tokens which expands every
/// `[pii:<id>]` token in a body).
#[cfg(feature = "encryption")]
#[tauri::command]
pub async fn reveal_pii_record(
    state: State<'_, AppState>,
    id: String,
) -> Result<String, String> {
    use chrono::Utc;
    use sovereign_crypto::vault::EncryptedBlob;

    let account_key = state
        .account_key()
        .await
        .ok_or_else(|| "PII reveal unavailable: account key not loaded".to_string())?;
    let account_key = &*account_key;
    let record = state.db.get_pii_record(&id).await.str_err()?;
    let blob = EncryptedBlob::from_pair(record.value_encrypted, record.value_nonce);
    let plaintext = blob
        .decrypt_to_string(account_key)
        .map_err(|e| format!("decrypt failed: {e}"))?;
    // Side effect: bump last_revealed_at so the dashboard "last viewed"
    // hint stays accurate.
    state
        .db
        .update_pii_record_revealed_at(&id, Utc::now())
        .await
        .str_err()?;
    Ok(plaintext)
}

#[cfg(not(feature = "encryption"))]
#[tauri::command]
pub async fn reveal_pii_record(
    _state: State<'_, AppState>,
    _id: String,
) -> Result<String, String> {
    Err("PII reveal requires the encryption feature to be enabled at build time".to_string())
}

/// Input for [`create_vault_entry`]. The frontend passes this from the
/// "New secret" dialog.
#[derive(Debug, serde::Deserialize)]
pub struct VaultEntryInput {
    /// Snake_case PiiKind: "password", "api_token", "bank_account",
    /// "document_id", "note", or any other PiiKind variant.
    pub kind: String,
    /// Optional human label (e.g. "main bank password").
    pub label: Option<String>,
    /// Entity this secret belongs to. None means unattributed.
    pub entity_id: Option<String>,
    /// The plaintext value to encrypt and store.
    pub value: String,
}

/// Create a vault entry — a user-entered secret encrypted under the
/// DeviceKey. The created PiiRecord is `stored_secret = true`,
/// `review_state = Confirmed`, `confidence = 1.0`, `sources = []`.
/// Action level: Modify (L3) per the plan; frontend gates the
/// confirmation before calling.
#[cfg(feature = "encryption")]
#[tauri::command]
pub async fn create_vault_entry(
    state: State<'_, AppState>,
    input: VaultEntryInput,
) -> Result<PiiRecordDto, String> {
    use chrono::Utc;
    use sovereign_crypto::vault::EncryptedBlob;
    use sovereign_db::schema::{PiiKind, PiiRecord, ReviewState};

    let account_key = state
        .account_key()
        .await
        .ok_or_else(|| "Vault add unavailable: account key not loaded".to_string())?;
    let account_key = &*account_key;

    let kind = parse_pii_kind(&input.kind)
        .ok_or_else(|| format!("unknown PII kind: {}", input.kind))?;

    let blob = EncryptedBlob::encrypt_str(&input.value, account_key)
        .map_err(|e| format!("vault encrypt failed: {e}"))?;

    let now = Utc::now();
    let record = PiiRecord {
        id: None,
        kind,
        value_encrypted: blob.ciphertext_b64,
        value_nonce: blob.nonce_b64,
        label: input.label,
        entity_id: input.entity_id,
        stored_secret: true,
        confidence: 1.0,
        sources: vec![],
        discovered_at: now,
        last_revealed_at: None,
        use_count: 0,
        review_state: ReviewState::Confirmed,
        deleted_at: None,
    };
    let created = state.db.create_pii_record(record).await.str_err()?;
    Ok(PiiRecordDto::from(created))
}

#[cfg(not(feature = "encryption"))]
#[tauri::command]
pub async fn create_vault_entry(
    _state: State<'_, AppState>,
    _input: VaultEntryInput,
) -> Result<PiiRecordDto, String> {
    Err("Vault entries require the encryption feature to be enabled at build time".to_string())
}

// ---------------------------------------------------------------------------
// Browser-PII (8b) — embedded-browser form extraction + autofill
// Desktop-only: the embedded browser webview doesn't exist on mobile, so
// these commands are gated alongside crate::browser / crate::browser_pii.
// ---------------------------------------------------------------------------

/// Trigger a JS scan of the active browser webview for input fields.
/// Results arrive asynchronously via the `__browser_form_extracted`
/// command, which re-emits them as the `browser-form-extracted` Tauri
/// event for the frontend's SignupCapturePrompt.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub async fn extract_form_fields(app: tauri::AppHandle) -> Result<(), String> {
    crate::browser_pii::trigger_form_extraction(&app)
}

/// JS-→-Rust callback. Re-emits the payload as a typed Tauri event
/// the frontend can subscribe to. The leading underscore in the name
/// matches the existing `__browser_content_extracted` convention used
/// by `browser.rs`'s extraction script.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
#[allow(non_snake_case)]
pub async fn __browser_form_extracted(
    app: tauri::AppHandle,
    payload: crate::browser_pii::FormExtractionDto,
) -> Result<(), String> {
    use tauri::Emitter;
    app.emit("browser-form-extracted", payload)
        .map_err(|e| e.to_string())
}

/// Decrypt a vault entry's plaintext value and inject it into the
/// active browser webview's input matching `selector`. L3 Modify per
/// the plan; the frontend gates the AutofillPrompt before calling.
/// Bumps `last_revealed_at` and (per the plan) increments `use_count`
/// — though use_count tracking lands separately when the
/// successful-submit detector is wired.
#[cfg(all(feature = "encryption", not(any(target_os = "android", target_os = "ios"))))]
#[tauri::command]
pub async fn autofill_pii_record(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    record_id: String,
    selector: String,
) -> Result<(), String> {
    use chrono::Utc;
    use sovereign_crypto::vault::EncryptedBlob;

    let account_key = state
        .account_key()
        .await
        .ok_or_else(|| "Autofill unavailable: account key not loaded".to_string())?;
    let account_key = &*account_key;
    let record = state.db.get_pii_record(&record_id).await.str_err()?;
    let blob = EncryptedBlob::from_pair(record.value_encrypted, record.value_nonce);
    let plaintext = blob
        .decrypt_to_string(account_key)
        .map_err(|e| format!("decrypt failed: {e}"))?;

    crate::browser_pii::autofill_field(&app, &selector, &plaintext)?;

    state
        .db
        .update_pii_record_revealed_at(&record_id, Utc::now())
        .await
        .str_err()?;
    Ok(())
}

// Stub for builds where the real autofill is unavailable: no encryption,
// or mobile (no embedded browser to inject into).
#[cfg(any(not(feature = "encryption"), target_os = "android", target_os = "ios"))]
#[tauri::command]
pub async fn autofill_pii_record(
    _state: State<'_, AppState>,
    _app: tauri::AppHandle,
    _record_id: String,
    _selector: String,
) -> Result<(), String> {
    Err("Autofill is only available on desktop builds with the encryption feature".to_string())
}

/// Generate a password according to the supplied policy. Stateless;
/// purely a wrapper around `sovereign-crypto::password_gen` so the
/// frontend doesn't need a parallel JS implementation.
#[cfg(feature = "encryption")]
#[tauri::command]
pub async fn generate_password(
    policy: sovereign_crypto::password_gen::PasswordPolicy,
) -> Result<String, String> {
    sovereign_crypto::password_gen::generate_password(&policy).map_err(|e| e.to_string())
}

#[cfg(not(feature = "encryption"))]
#[tauri::command]
pub async fn generate_password(_policy: serde_json::Value) -> Result<String, String> {
    Err("Password generation requires the encryption feature to be enabled at build time".to_string())
}

// ---------------------------------------------------------------------------
// Signup capture (8d) — high-level multi-write command
// ---------------------------------------------------------------------------

/// One field captured from a signup form, ready to be encrypted +
/// stored as a PiiRecord. The frontend builds this list after the
/// user reviews + edits the SignupCapturePrompt.
#[derive(Debug, serde::Deserialize)]
pub struct SignupFieldInput {
    /// snake_case PiiKind ("password", "email", "phone", "first_name",
    /// "last_name", "address", "text", etc.).
    pub kind: String,
    /// User-edited or auto-suggested label (e.g. "main bank password").
    pub label: Option<String>,
    /// The value to encrypt + store. May be a generated password or
    /// the user's typed value.
    pub value: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct SignupCaptureInput {
    /// URL of the page that triggered the capture, used to look up
    /// or create the entity (parsed for the host) and recorded as
    /// `via_url` on each ShareRecord.
    pub url: String,
    /// Existing entity ID, or None to auto-create from the URL host.
    pub entity_id: Option<String>,
    pub fields: Vec<SignupFieldInput>,
}

#[derive(Debug, Serialize)]
pub struct SignupCaptureResult {
    pub entity_id: String,
    pub record_ids: Vec<String>,
    pub share_record_count: usize,
    /// True when this call auto-created the entity (frontend can
    /// surface that fact in a toast).
    pub entity_created: bool,
}

/// Commit a signup capture: resolve/create entity from the URL, write
/// one PiiRecord per field, write one Web-channel ShareRecord per
/// record. Atomic in spirit (best-effort: a partial failure leaves
/// previously-written records in place; the frontend should treat
/// errors as "some records may have been written, refresh the
/// dashboard to see").
///
/// L4 Transmit per the plan (signup is itself a disclosure to the
/// entity); the frontend gates the SignupCapturePrompt confirmation
/// before calling.
#[cfg(feature = "encryption")]
#[tauri::command]
pub async fn commit_signup_capture(
    state: State<'_, AppState>,
    input: SignupCaptureInput,
) -> Result<SignupCaptureResult, String> {
    use chrono::Utc;
    use sovereign_crypto::vault::EncryptedBlob;
    use sovereign_db::schema::{
        Entity, EntityKind, PiiRecord, ReviewState, ShareChannel, ShareRecord,
    };

    let account_key = state
        .account_key()
        .await
        .ok_or_else(|| "Signup capture unavailable: account key not loaded".to_string())?;
    let account_key = &*account_key;

    // Step 1: resolve or create the entity.
    let (entity_id, entity_created) = match input.entity_id {
        Some(id) => (id, false),
        None => {
            let host = url::Url::parse(&input.url)
                .map_err(|e| format!("invalid url: {e}"))?
                .host_str()
                .ok_or_else(|| "url has no host".to_string())?
                .to_ascii_lowercase();
            // Look for an existing entity with this domain.
            let entities = state.db.list_entities().await.str_err()?;
            let existing = entities.iter().find(|e| {
                e.domains
                    .iter()
                    .any(|d| d.eq_ignore_ascii_case(&host))
            });
            if let Some(e) = existing {
                let id = e
                    .id
                    .as_ref()
                    .map(thing_to_raw)
                    .ok_or_else(|| "matched entity has no id".to_string())?;
                (id, false)
            } else {
                let mut new_entity = Entity::new(host.clone(), EntityKind::Service);
                new_entity.domains = vec![host];
                new_entity.is_owned = true; // user just signed up — they own this association
                let created = state.db.create_entity(new_entity).await.str_err()?;
                let id = created
                    .id
                    .as_ref()
                    .map(thing_to_raw)
                    .ok_or_else(|| "create_entity returned no id".to_string())?;
                (id, true)
            }
        }
    };

    // Step 2: write one PiiRecord per field.
    let now = Utc::now();
    let mut record_ids: Vec<String> = Vec::with_capacity(input.fields.len());
    for field in &input.fields {
        let kind = parse_pii_kind(&field.kind)
            .ok_or_else(|| format!("unknown PII kind: {}", field.kind))?;
        let blob = EncryptedBlob::encrypt_str(&field.value, account_key)
            .map_err(|e| format!("vault encrypt: {e}"))?;
        let record = PiiRecord {
            id: None,
            kind,
            value_encrypted: blob.ciphertext_b64,
            value_nonce: blob.nonce_b64,
            label: field.label.clone(),
            entity_id: Some(entity_id.clone()),
            stored_secret: true,
            confidence: 1.0,
            sources: vec![],
            discovered_at: now,
            last_revealed_at: None,
            use_count: 0,
            review_state: ReviewState::Confirmed,
            deleted_at: None,
        };
        let created = state.db.create_pii_record(record).await.str_err()?;
        let rid = created
            .id
            .as_ref()
            .map(thing_to_raw)
            .ok_or_else(|| "create_pii_record returned no id".to_string())?;
        record_ids.push(rid);
    }

    // Step 3: write one Web-channel ShareRecord per record. Signup
    // IS a disclosure — the user just handed these values to the
    // entity. Per-field share entries let the dashboard's Shared tab
    // show the full disclosure trail.
    let mut share_count = 0usize;
    for record_id in &record_ids {
        let share = ShareRecord {
            id: None,
            pii_record_id: record_id.clone(),
            to_entity_id: entity_id.clone(),
            via_message_id: None,
            via_url: Some(input.url.clone()),
            shared_at: now,
            channel: ShareChannel::Web,
            via_url_nonce: None,
        };
        match state.db.create_share_record(share).await {
            Ok(_) => share_count += 1,
            Err(e) => tracing::warn!("commit_signup: share record failed for {record_id}: {e}"),
        }
    }

    Ok(SignupCaptureResult {
        entity_id,
        record_ids,
        share_record_count: share_count,
        entity_created,
    })
}

#[cfg(not(feature = "encryption"))]
#[tauri::command]
pub async fn commit_signup_capture(
    _state: State<'_, AppState>,
    _input: SignupCaptureInput,
) -> Result<SignupCaptureResult, String> {
    Err("Signup capture requires the encryption feature to be enabled at build time".to_string())
}

// ---------------------------------------------------------------------------
// Cookie management (8c) — Cookies tab in the entity-detail panel
// ---------------------------------------------------------------------------

/// List cookies whose domain attributes to `entity_id`'s domains[].
/// Returns an empty list when the entity has no `domains` set, the
/// browser webview isn't open, or no cookie matches.
#[tauri::command]
pub async fn list_cookies_for_entity(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    entity_id: String,
) -> Result<Vec<crate::cookie_api::CookieDto>, String> {
    let entity = state.db.get_entity(&entity_id).await.str_err()?;
    if entity.domains.is_empty() {
        return Ok(vec![]);
    }
    crate::cookie_api::list_cookies_for_domains(&app, &entity.domains)
}

/// Delete one cookie by (name, domain, path). L5 Destruct per the
/// plan; the frontend gates the confirmation prompt before calling.
#[tauri::command]
pub async fn delete_cookie(
    app: tauri::AppHandle,
    name: String,
    domain: String,
    path: String,
) -> Result<(), String> {
    crate::cookie_api::delete_one(&app, &name, &domain, &path)
}

/// Bulk-delete every cookie whose domain matches `entity_id`'s
/// domains. Returns the number deleted. L5 Destruct.
#[tauri::command]
pub async fn clear_entity_cookies(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    entity_id: String,
) -> Result<usize, String> {
    let entity = state.db.get_entity(&entity_id).await.str_err()?;
    if entity.domains.is_empty() {
        return Ok(0);
    }
    crate::cookie_api::clear_for_domains(&app, &entity.domains)
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

#[cfg(feature = "encryption")]
fn parse_pii_kind(s: &str) -> Option<sovereign_db::schema::PiiKind> {
    use sovereign_db::schema::PiiKind;
    match s {
        "email" => Some(PiiKind::Email),
        "phone" => Some(PiiKind::Phone),
        "ssn" => Some(PiiKind::Ssn),
        "credit_card" => Some(PiiKind::CreditCard),
        "ipv4" => Some(PiiKind::Ipv4),
        "avs" => Some(PiiKind::Avs),
        "iban" => Some(PiiKind::Iban),
        "passport" => Some(PiiKind::Passport),
        "dob" => Some(PiiKind::Dob),
        "address" => Some(PiiKind::Address),
        "person_name" => Some(PiiKind::PersonName),
        "org_name" => Some(PiiKind::OrgName),
        "password" => Some(PiiKind::Password),
        "api_token" => Some(PiiKind::ApiToken),
        "bank_account" => Some(PiiKind::BankAccount),
        "document_id" => Some(PiiKind::DocumentId),
        "note" => Some(PiiKind::Note),
        "other" => Some(PiiKind::Other),
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

    let account_key = state
        .account_key()
        .await
        .ok_or_else(|| "PII resolution unavailable: account key not loaded".to_string())?;
    let account_key = &*account_key;

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
                    resolve_raw_original(account_key, enc, nonce).map_err(|e| e.to_string())?
                }
                _ => {
                    let body = extract_document_body(&doc.content);
                    resolve_body(
                        state.db.as_ref() as &dyn GraphDB,
                        account_key,
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
                    resolve_raw_original(account_key, enc, nonce).map_err(|e| e.to_string())?
                }
                _ => resolve_body(
                    state.db.as_ref() as &dyn GraphDB,
                    account_key,
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
