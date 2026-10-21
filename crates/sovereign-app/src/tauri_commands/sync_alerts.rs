//! Alerts on changes from paired devices, and previous values of synced
//! items (spec ch. 05, C-WIRE 13: alerts, not reviews; decided by Céline
//! 2026-10-15).
//!
//! A change from one of her paired devices applies without asking. Every
//! received document change is analysed (deterministic checks during sync,
//! the model in the background); an alert is listed only when something
//! looks wrong, until she dismisses it. Dismissing changes nothing in the
//! document; going back is the ordinary "Use this version" in History.
//!
//! Rows raise no alert: a synced row's previous value is kept and can be
//! restored from the list of previous values (no pending badge).

use super::*;

/// One open alert.
#[derive(Serialize)]
pub struct SyncAlertDto {
    pub doc_id: String,
    /// The document's title as shown (personal data as labels).
    pub title: String,
    /// The device the change came from (its peer id).
    pub device: String,
    /// The device's name, when it is a paired device; empty otherwise.
    pub device_name: String,
    /// What was found, in words (no document text).
    pub finding: String,
    /// The version holding the state before the change, when there is one.
    pub previous_version: Option<String>,
}

/// One previous value of a synced item (a row a paired device's sync
/// overwrote; its earlier value is kept, sealed).
#[derive(Serialize)]
pub struct PreviousValueDto {
    pub id: String,
    /// What the item is, in words ("Thread: Work", "Contact: Ana").
    pub label: String,
    pub device: String,
    pub device_name: String,
    /// When the sync replaced it (RFC3339).
    pub at: String,
    /// Whether this kind of item can be put back.
    pub can_restore: bool,
}

/// Restore is wired for these row tables (see `SyncService::restore_row_recovery`).
fn row_table_restorable(table: &str) -> bool {
    matches!(table, "thread" | "contact" | "pii_record")
}

/// `peer id -> device name` for the paired devices.
#[allow(unused_variables)]
pub(crate) async fn device_names(state: &AppState) -> std::collections::HashMap<String, String> {
    #[cfg(feature = "p2p")]
    {
        if let Some(ref manager) = *state.pairing_manager.read().await {
            return manager.list_devices().into_iter().map(|d| (d.peer_id.clone(), d.device_name.clone())).collect();
        }
    }
    std::collections::HashMap::new()
}

/// The open alerts, newest first.
#[tauri::command]
pub async fn list_sync_alerts(webview: tauri::Webview, state: State<'_, AppState>) -> Result<Vec<SyncAlertDto>, String> {
    state.require_unlocked(&webview).await?;
    let docs = state.docs().await?;
    let names = device_names(&state).await;
    let alerts = docs.alerts().await.map_err(|e| sovereign_ai::docs_host::doc_error_message(&e))?;
    let mut out = Vec::with_capacity(alerts.len());
    for a in alerts {
        let title = match docs.read(&a.id).await {
            Ok(v) => v.labelled_title(),
            Err(_) => String::new(),
        };
        out.push(SyncAlertDto {
            device_name: names.get(&a.device).cloned().unwrap_or_default(),
            doc_id: a.id,
            title,
            device: a.device,
            finding: a.finding,
            previous_version: a.previous_version,
        });
    }
    Ok(out)
}

/// Dismisses a document's alert; changes nothing in the document.
#[tauri::command]
pub async fn dismiss_sync_alert(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    doc_id: String,
) -> Result<(), String> {
    state.require_unlocked(&webview).await?;
    let docs = state.docs().await?;
    docs.dismiss_alert(&doc_id).await.map_err(|e| sovereign_ai::docs_host::doc_error_message(&e))
}

/// A row's label in words, from its current value.
async fn row_label(db: &dyn GraphDB, table: &str, row_id: &str) -> String {
    match table {
        "thread" => match db.get_thread(row_id).await {
            Ok(t) => format!("Thread: {}", t.name),
            Err(_) => "A thread".into(),
        },
        "contact" => match db.get_contact(row_id).await {
            Ok(c) => format!("Contact: {}", c.name),
            Err(_) => "A contact".into(),
        },
        "pii_record" => match db.get_pii_record(row_id).await {
            Ok(p) => match p.label {
                Some(l) if !l.is_empty() => format!("Personal data: {l}"),
                _ => "A personal-data record".into(),
            },
            Err(_) => "A personal-data record".into(),
        },
        "entity" => "A person or organisation".into(),
        "message" => "A message".into(),
        "conversation" => "A conversation".into(),
        "suggested_link" => "A suggested link".into(),
        other => format!("An item ({other})"),
    }
}

/// The previous values a sync replaced, newest first.
#[tauri::command]
pub async fn list_previous_values(
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<Vec<PreviousValueDto>, String> {
    state.require_unlocked(&webview).await?;
    let names = device_names(&state).await;
    let mut recs = state.db.list_pending_row_recoveries().await.str_err()?;
    recs.sort_by(|a, b| b.overwritten_at.cmp(&a.overwritten_at));
    let mut out = Vec::with_capacity(recs.len());
    for r in recs {
        out.push(PreviousValueDto {
            id: r.id_string().unwrap_or_default(),
            label: row_label(state.db.as_ref(), &r.table, &r.row_id).await,
            device_name: names.get(&r.peer).cloned().unwrap_or_default(),
            device: r.peer,
            at: r.overwritten_at.to_rfc3339(),
            can_restore: row_table_restorable(&r.table),
        });
    }
    Ok(out)
}

/// Puts a previous value back (through the P2P node, which holds the key
/// that unseals it), then removes it from the list.
#[tauri::command]
pub async fn restore_previous_value(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state.require_unlocked(&webview).await?;
    #[cfg(feature = "p2p")]
    {
        let tx = state.p2p_command_tx().await.ok_or_else(|| "Sync is not running.".to_string())?;
        tx.send(sovereign_p2p::node::P2pCommand::RestoreRowRecovery { recovery_id: id })
            .await
            .map_err(|e| format!("The restore could not be started: {e}"))?;
        Ok(())
    }
    #[cfg(not(feature = "p2p"))]
    {
        let _ = (id, &state);
        Err("Sync is not available in this build.".to_string())
    }
}

/// Removes a previous value from the list (the current value stays).
#[tauri::command]
pub async fn forget_previous_value(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state.require_unlocked(&webview).await?;
    state.db.resolve_row_recovery(&id).await.str_err()
}
