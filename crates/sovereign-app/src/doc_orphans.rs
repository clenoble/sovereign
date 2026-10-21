//! Documents whose thread was deleted, in the Tauri host (the mobile app):
//! what the commands `list_orphaned_documents`, `resolve_orphan` and
//! `resolve_orphans_of_thread` do, kept out of the `#[tauri::command]`
//! wrappers so they are tested against a real encrypted store without a
//! webview, like `doc_trash.rs` (S5-12, contract W1–W7 in
//! `doc/plans/C-DOC-adoption.md`; spec ch. 03, *Thread deletion and the
//! fallback lane*).
//!
//! The frontend's bindings (`frontend/src/lib/api/commands.ts`) fix the
//! command names and shapes: `list_orphaned_documents` → `OrphanDto[]`;
//! `resolve_orphan { docId, resolution }`; `resolve_orphans_of_thread
//! { threadId, resolution }` → the number resolved; `resolution` is
//! `{"kind":"move_to","thread_id":…} | {"kind":"new_thread","name":…} |
//! {"kind":"trash"}`.
//!
//! The wrappers are in [`crate::commands`] (gated, as `delete_document`).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sovereign_docs::{DocumentService, OrphanResolution};

/// One document whose thread was deleted, as the frontend receives it — and
/// no content.
#[derive(Clone, Debug, Serialize)]
pub struct OrphanDto {
    pub doc_id: String,
    pub title: String,
    /// The deleted thread it is still in.
    pub thread_id: String,
    /// That thread's old name; `None` when unknown.
    pub deleted_thread_name: Option<String>,
    /// The peer id of the device its current state came from.
    pub device: String,
    /// The device's name when it is a paired device; empty otherwise.
    pub device_name: String,
}

/// One of the three ways out, as the frontend sends it.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OrphanResolutionDto {
    MoveTo { thread_id: String },
    NewThread { name: String },
    Trash,
}

impl From<OrphanResolutionDto> for OrphanResolution {
    fn from(r: OrphanResolutionDto) -> Self {
        match r {
            OrphanResolutionDto::MoveTo { thread_id } => OrphanResolution::MoveTo(thread_id),
            OrphanResolutionDto::NewThread { name } => OrphanResolution::NewThread(name),
            OrphanResolutionDto::Trash => OrphanResolution::Trash,
        }
    }
}

/// The documents whose thread was deleted; device names from `device_names`
/// (peer id → name).
pub async fn list_orphaned_documents(
    docs: &DocumentService,
    device_names: &HashMap<String, String>,
) -> Result<Vec<OrphanDto>, String> {
    let list = docs
        .orphans_of_deleted_threads()
        .await
        .map_err(|e| format!("The documents whose thread was deleted couldn't be read ({e})."))?;
    Ok(list
        .into_iter()
        .map(|o| OrphanDto {
            device_name: device_names.get(&o.device).cloned().unwrap_or_default(),
            doc_id: o.id,
            title: o.title,
            thread_id: o.thread_id,
            deleted_thread_name: o.deleted_thread_name,
            device: o.device,
        })
        .collect())
}

/// One way out for one document (W4). Refused: nothing changes, and the
/// error says why, for the notice.
pub async fn resolve_orphan(docs: &DocumentService, doc_id: &str, resolution: OrphanResolutionDto) -> Result<(), String> {
    docs.resolve_orphan(doc_id, resolution.into()).await.map(|_| ()).map_err(|e| e.to_string())
}

/// The same way out for every document of the deleted thread `thread_id`;
/// the number of documents resolved.
pub async fn resolve_orphans_of_thread(
    docs: &DocumentService,
    thread_id: &str,
    resolution: OrphanResolutionDto,
) -> Result<usize, String> {
    docs.resolve_orphans_of_thread(thread_id, resolution.into())
        .await
        .map(|r| r.documents.len())
        .map_err(|e| e.to_string())
}

/// The `#[tauri::command]` wrappers, registered in `run_tauri`. Gated like
/// `delete_document` and the trash: an unlocked session, from the main
/// webview.
pub mod commands {
    use tauri::State;

    use super::{OrphanDto, OrphanResolutionDto};
    use crate::tauri_state::AppState;

    /// The documents whose thread was deleted (S5-12), with device names.
    #[tauri::command]
    pub async fn list_orphaned_documents(webview: tauri::Webview, state: State<'_, AppState>) -> Result<Vec<OrphanDto>, String> {
        state.require_unlocked(&webview).await?;
        let docs = state.docs().await?;
        let names = crate::tauri_commands::sync_alerts::device_names(&state).await;
        super::list_orphaned_documents(&docs, &names).await
    }

    /// One way out for one document: move it, put it in a new thread, or
    /// put it in the trash.
    #[tauri::command]
    pub async fn resolve_orphan(
        webview: tauri::Webview,
        state: State<'_, AppState>,
        doc_id: String,
        resolution: OrphanResolutionDto,
    ) -> Result<(), String> {
        state.require_unlocked(&webview).await?;
        let docs = state.docs().await?;
        super::resolve_orphan(&docs, &doc_id, resolution).await
    }

    /// The same way out for every document of a deleted thread; how many.
    #[tauri::command]
    pub async fn resolve_orphans_of_thread(
        webview: tauri::Webview,
        state: State<'_, AppState>,
        thread_id: String,
        resolution: OrphanResolutionDto,
    ) -> Result<usize, String> {
        state.require_unlocked(&webview).await?;
        let docs = state.docs().await?;
        super::resolve_orphans_of_thread(&docs, &thread_id, resolution).await
    }
}
