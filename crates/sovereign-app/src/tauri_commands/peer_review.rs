//! Peer-write review surface (p2p-no-per-doc-authz).
//!
//! A paired peer's sync overwrites are non-destructive: the prior value is
//! preserved (a commit for documents, a sealed `RowRecovery` for rows) and the
//! change is flagged for review. These commands back that review surface —
//! listing pending changes with their LLM/heuristic audit verdict, and letting
//! the user accept (keep the peer's version) or restore (revert to the prior).
//! The visual panel is a separate frontend task; this is the invoke() API.

use super::*;

/// One pending peer-originated change awaiting review (document or row).
#[derive(Serialize)]
pub struct PeerReviewDto {
    /// `"document"` or `"row"`.
    pub kind: String,
    /// The document id (kind=document) or the recovery id (kind=row) — the
    /// handle the accept/restore commands take.
    pub id: String,
    /// Human label: the document title, or `"<table>: <row_id>"` for a row.
    pub title: String,
    /// PeerId of the device that made the change.
    pub peer: String,
    /// When it was applied (RFC3339), if known.
    pub at: Option<String>,
    /// JSON `PeerChangeVerdict` from the audit, or null if not yet audited /
    /// no model available.
    pub assessment: Option<String>,
    /// Whether one-click restore is wired for this item.
    pub can_restore: bool,
}

/// Restore is wired for these row tables (see `SyncService::restore_row_recovery`).
fn row_table_restorable(table: &str) -> bool {
    matches!(table, "thread" | "contact" | "pii_record")
}

/// List every pending peer-review — overwritten documents and rows — newest
/// first within each kind.
#[tauri::command]
pub async fn list_peer_reviews(
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<Vec<PeerReviewDto>, String> {
    state.require_unlocked(&webview).await?;

    let mut out = Vec::new();

    let docs = state.db.list_documents_pending_peer_review().await.str_err()?;
    for d in docs {
        out.push(PeerReviewDto {
            kind: "document".to_string(),
            id: d.id_string().unwrap_or_default(),
            title: d.title,
            peer: d.peer_review_peer.unwrap_or_default(),
            at: d.peer_review_at.map(|t| t.to_rfc3339()),
            assessment: d.peer_review_assessment,
            can_restore: d.peer_review_prior_commit.is_some(),
        });
    }

    let recs = state.db.list_pending_row_recoveries().await.str_err()?;
    for r in recs {
        out.push(PeerReviewDto {
            kind: "row".to_string(),
            id: r.id_string().unwrap_or_default(),
            title: format!("{}: {}", r.table, r.row_id),
            peer: r.peer,
            at: Some(r.overwritten_at.to_rfc3339()),
            assessment: r.assessment,
            can_restore: row_table_restorable(&r.table),
        });
    }

    Ok(out)
}

/// Accept a peer change — keep the synced version, clear the review flag /
/// resolve the recovery. `kind` is `"document"` or `"row"`.
#[tauri::command]
pub async fn accept_peer_review(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    kind: String,
    id: String,
) -> Result<(), String> {
    state.require_unlocked(&webview).await?;
    match kind.as_str() {
        "document" => state.db.clear_document_peer_review(&id).await.str_err()?,
        "row" => state.db.resolve_row_recovery(&id).await.str_err()?,
        other => return Err(format!("unknown peer-review kind '{other}'")),
    }
    Ok(())
}

/// Restore the prior value — revert a peer overwrite. Documents restore inline
/// (the prior commit is re-applied); rows are routed to the P2P node, which
/// holds the key to unseal the recovery.
#[tauri::command]
pub async fn restore_peer_review(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    kind: String,
    id: String,
) -> Result<(), String> {
    state.require_unlocked(&webview).await?;
    match kind.as_str() {
        "document" => {
            let doc = state.db.get_document(&id).await.str_err()?;
            let prior = doc
                .peer_review_prior_commit
                .ok_or_else(|| "no prior version recorded for this document".to_string())?;
            state.db.restore_document(&id, &prior).await.str_err()?;
            state.db.clear_document_peer_review(&id).await.str_err()?;
            Ok(())
        }
        "row" => {
            #[cfg(feature = "p2p")]
            {
                let tx = state
                    .p2p_command_tx()
                    .await
                    .ok_or_else(|| "P2P node not running".to_string())?;
                tx.send(sovereign_p2p::node::P2pCommand::RestoreRowRecovery { recovery_id: id })
                    .await
                    .map_err(|e| format!("failed to send restore command: {e}"))?;
                Ok(())
            }
            #[cfg(not(feature = "p2p"))]
            {
                let _ = id;
                Err("P2P is not available in this build".to_string())
            }
        }
        other => Err(format!("unknown peer-review kind '{other}'")),
    }
}

/// Run the peer-change audit over every un-audited pending review, filling in
/// assessments. Returns the number audited. Normally triggered after a sync;
/// exposed as a command for the UI to refresh on demand.
#[tauri::command]
pub async fn audit_peer_reviews(
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<u32, String> {
    state.require_unlocked(&webview).await?;
    let orch = state
        .orchestrator
        .as_ref()
        .ok_or_else(|| "Orchestrator not available".to_string())?;
    let n = orch.audit_peer_reviews().await.str_err()?;
    Ok(n as u32)
}
