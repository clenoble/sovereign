//! The documents the one-time conversion could not read, in the Tauri host
//! (v0.0.12 item 7 V2b, B1–B2 in `doc/plans/V0012-item7-v2b-app-list.md`):
//! what the command `list_unreadable_documents` does, kept out of the
//! `#[tauri::command]` wrapper so it is tested against a real encrypted store
//! without a webview, like `doc_orphans.rs`.
//!
//! The frontend's binding (`frontend/src/lib/api/commands.ts`) fixes the
//! shape: `list_unreadable_documents` → `{ heading, meaning, lines }`. The
//! heading, the meaning and each line are worded by `sovereign-docs` (B1):
//! neither the app nor the frontend holds a copy.

use serde::Serialize;
use sovereign_docs::{DocumentService, UNREADABLE_HEADING, UNREADABLE_MEANING};

/// The list as the frontend receives it: no title, no content.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct UnreadableDocumentsDto {
    pub heading: String,
    pub meaning: String,
    /// One line per document, in the order the conversion's record gives.
    pub lines: Vec<String>,
}

/// This store's documents that could not be read, worded (B2). No document
/// service gives no lines; a list that cannot be read gives no lines and a
/// warning, as the shell's `load_alerts_as` (B2 [choice]).
pub async fn list_unreadable_documents(docs: Option<&DocumentService>) -> UnreadableDocumentsDto {
    let lines = match docs {
        None => Vec::new(),
        Some(docs) => match sovereign_docs::unlock_migration_undecryptable(docs).await {
            Ok(list) => list.iter().map(sovereign_docs::unreadable_line).collect(),
            Err(e) => {
                tracing::warn!("the documents that could not be read could not be listed: {e}");
                Vec::new()
            }
        },
    };
    UnreadableDocumentsDto { heading: UNREADABLE_HEADING.into(), meaning: UNREADABLE_MEANING.into(), lines }
}

/// The `#[tauri::command]` wrapper, registered in `run_tauri`. Gated like
/// every data command (IPC-004): a locked session is refused before any
/// list is read; the frontend reads a refusal as an empty list.
pub mod commands {
    use tauri::State;

    use super::UnreadableDocumentsDto;
    use crate::tauri_state::AppState;

    /// This persona's documents that could not be read (V2b, B2).
    #[tauri::command]
    pub async fn list_unreadable_documents(
        webview: tauri::Webview,
        state: State<'_, AppState>,
    ) -> Result<UnreadableDocumentsDto, String> {
        state.require_unlocked(&webview).await?;
        let docs = state.docs().await.ok();
        Ok(super::list_unreadable_documents(docs.as_deref()).await)
    }
}
