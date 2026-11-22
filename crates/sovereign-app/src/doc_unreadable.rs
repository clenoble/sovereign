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
    /// When the list itself could not be read: the sentence said under the
    /// heading, in place of the meaning and the lines
    /// (`sovereign_docs::COULD_NOT_CHECK_UNREADABLE`; spec ch. 03, *What cannot
    /// be read is never shown as empty*). `None` when it was read, and then
    /// not sent: the list keeps the three fields of v0.0.12 item 7 V2b
    /// (`item7_v2b_tests`, B2, fixes them).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub could_not_check: Option<String>,
}

/// This store's documents that could not be read, worded (B2). No document
/// service gives no lines; a list that cannot be read gives no lines and the
/// sentence that says so (spec ch. 03, *What cannot be read is never shown as
/// empty*), as the shell's `load_alerts_as`.
pub async fn list_unreadable_documents(docs: Option<&DocumentService>) -> UnreadableDocumentsDto {
    let (lines, could_not_check) = match docs {
        None => (Vec::new(), None),
        Some(docs) => match sovereign_docs::unlock_migration_undecryptable(docs).await {
            Ok(list) => (list.iter().map(sovereign_docs::unreadable_line).collect(), None),
            Err(e) => {
                tracing::warn!("the documents that could not be read could not be listed: {e}");
                (Vec::new(), Some(sovereign_docs::COULD_NOT_CHECK_UNREADABLE.to_string()))
            }
        },
    };
    UnreadableDocumentsDto { heading: UNREADABLE_HEADING.into(), meaning: UNREADABLE_MEANING.into(), lines, could_not_check }
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
