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
use tauri::State;

use crate::tauri_state::AppState;

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
