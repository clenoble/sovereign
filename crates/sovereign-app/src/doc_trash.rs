//! The trash of the Tauri host (the mobile app): what the commands
//! `list_trash`, `restore_from_trash` and `empty_trash` do, kept out of the
//! `#[tauri::command]` wrappers so they are tested against a real encrypted
//! store without a webview, like `doc_saves.rs` (C-DOC adoption step 5,
//! `doc/plans/C-DOC-adoption.md`, S5-5/S5-6/S5-7 through S5-9). The wrappers
//! are in [`commands`]; the host's hourly purge is [`run_purge_ticks`].
//!
//! The confirmation before emptying (K7) is the Svelte UI's (S5-9, vitest).
//! **No Svelte component calls these yet**: where the trash lives (Q4) and the
//! delete gesture and its notice (Q3) are Céline's decisions, pending.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sovereign_db::GraphDB;
use sovereign_docs::{list_trash_named, DocError, DocumentService};

/// One line of the trash as the frontend receives it — and no content.
#[derive(Clone, Debug, Serialize)]
pub struct TrashEntryDto {
    pub id: String,
    pub title: String,
    /// The thread the document was in.
    pub thread_id: String,
    /// That thread's name; `None` when the thread is gone.
    pub thread_name: Option<String>,
    /// The deleted thread's old name when the thread is gone (the UI shows
    /// "<old name> - deleted"); `None` otherwise.
    pub deleted_thread_name: Option<String>,
    pub trashed_at: DateTime<Utc>,
    /// Whole days left before the document is purged (30 days after it was
    /// trashed).
    pub days_left: i64,
}

/// The trash, newest first, as of `now`. Thread names are read from `db`.
pub async fn list_trash(
    docs: &DocumentService,
    db: &dyn GraphDB,
    now: DateTime<Utc>,
) -> Result<Vec<TrashEntryDto>, String> {
    let listing = list_trash_named(docs, db, now)
        .await
        .map_err(|e| format!("The trash couldn't be read ({e})."))?;
    Ok(listing
        .into_iter()
        .map(|l| TrashEntryDto {
            id: l.entry.id,
            title: l.entry.title,
            thread_id: l.entry.thread_id,
            thread_name: l.thread_name,
            deleted_thread_name: l.deleted_thread_name,
            trashed_at: l.entry.trashed_at,
            days_left: l.days_left,
        })
        .collect())
}

/// Brings a document back from the trash into its own thread. A document
/// whose thread was deleted is refused (it needs a chosen thread:
/// [`restore_from_trash_to`]); it never lands in the fallback lane.
pub async fn restore_from_trash(docs: &DocumentService, id: &str) -> Result<(), String> {
    sovereign_docs::restore_to_thread(docs, docs.store(), id, None).await
}

/// Brings a document back from the trash: into its own thread when that
/// thread exists, into `thread_id` (an existing thread the user chose) when it
/// is given. A document whose thread was deleted needs that choice (Céline,
/// 2026-10-17); without it, or with a thread that does not exist, nothing is
/// restored and the error says why.
pub async fn restore_from_trash_to(
    docs: &DocumentService,
    db: &dyn GraphDB,
    id: &str,
    thread_id: Option<&str>,
) -> Result<(), String> {
    sovereign_docs::restore_to_thread(docs, db, id, thread_id).await
}

/// The phone's thread delete (S5-10): every document still in the thread goes
/// to the trash, then the thread is deleted (`sovereign_docs::
/// delete_thread_to_trash`, all or nothing); the number of documents moved.
pub async fn delete_thread_to_trash(docs: &DocumentService, db: &dyn GraphDB, thread_id: &str) -> Result<usize, String> {
    sovereign_docs::delete_thread_to_trash(docs, db, thread_id)
        .await
        .map(|report| report.trashed.len())
        .map_err(|e| e.message)
}

/// Empties the trash; the number of documents purged.
pub async fn empty_trash(docs: &DocumentService) -> Result<usize, String> {
    docs.empty_trash()
        .await
        .map(|report| report.documents.len())
        .map_err(|e| format!("The trash couldn't be emptied ({e}). Some documents may remain in it."))
}

/// The host's purge (S5-3): runs `sovereign_docs::purge_tick` at unlock, then
/// every `PURGE_TICK_INTERVAL` while the session stays unlocked.
///
/// The session's document service is installed by the login path
/// (`tauri_commands/auth.rs`, `install_session`); this loop watches for it
/// rather than being called from there: it looks every [`PURGE_POLL`], runs a
/// tick as soon as it sees a service it has not purged with yet (an unlock),
/// and again once the interval has passed. A failure is logged without
/// content and retried at the next interval.
pub(crate) async fn run_purge_ticks(handle: tauri::AppHandle) {
    use std::sync::{Arc, Weak};
    use std::time::Instant;
    use tauri::Manager;

    let mut last: Option<(Weak<DocumentService>, Instant)> = None;
    let mut poll = tokio::time::interval(PURGE_POLL);
    loop {
        poll.tick().await;
        let state = handle.state::<crate::tauri_state::AppState>();
        let Ok(docs) = state.docs().await else {
            last = None; // locked: the next unlock purges at once
            continue;
        };
        let due = match &last {
            Some((seen, at)) => !Weak::ptr_eq(seen, &Arc::downgrade(&docs)) || at.elapsed() >= sovereign_docs::PURGE_TICK_INTERVAL,
            None => true,
        };
        if !due {
            continue;
        }
        match sovereign_docs::purge_tick(&docs).await {
            Ok(report) if !report.documents.is_empty() => {
                tracing::info!("Purged {} document(s) 30 days in the trash", report.documents.len())
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("Trash purge failed ({}); retried in an hour", purge_error_kind(&e)),
        }
        last = Some((Arc::downgrade(&docs), Instant::now()));
    }
}

/// How often the purge loop looks for an unlocked session.
pub(crate) const PURGE_POLL: std::time::Duration = std::time::Duration::from_secs(15);

/// A purge failure for the log: its kind, no ids, no content.
pub(crate) fn purge_error_kind(e: &DocError) -> &'static str {
    match e {
        DocError::Store(_) => "storage error",
        DocError::NotFound(_) | DocError::Purged(_) => "document missing",
        _ => "document service error",
    }
}

/// The `#[tauri::command]` wrappers, registered in `run_tauri`. Gated like
/// `delete_document`: an unlocked session, from the main webview.
pub mod commands {
    use tauri::State;

    use super::TrashEntryDto;
    use crate::tauri_state::AppState;

    /// The trash, newest first, with thread names and days left.
    #[tauri::command]
    pub async fn list_trash(webview: tauri::Webview, state: State<'_, AppState>) -> Result<Vec<TrashEntryDto>, String> {
        state.require_unlocked(&webview).await?;
        let docs = state.docs().await?;
        super::list_trash(&docs, state.db.as_ref(), chrono::Utc::now()).await
    }

    /// Brings a document back from the trash: into its thread, or into
    /// `thread_id` when the user chose one (needed when its thread was
    /// deleted). (Kept on one line: `step5_tests` reads this signature.)
    #[tauri::command]
    pub async fn restore_from_trash(webview: tauri::Webview, state: State<'_, AppState>, id: String, thread_id: Option<String>) -> Result<(), String> {
        state.require_unlocked(&webview).await?;
        let docs = state.docs().await?;
        match thread_id.as_deref() {
            None => super::restore_from_trash(&docs, &id).await,
            Some(into) => super::restore_from_trash_to(&docs, state.db.as_ref(), &id, Some(into)).await,
        }
    }

    /// Empties the trash (the UI asks first, K7); how many were purged.
    #[tauri::command]
    pub async fn empty_trash(webview: tauri::Webview, state: State<'_, AppState>) -> Result<usize, String> {
        state.require_unlocked(&webview).await?;
        let docs = state.docs().await?;
        super::empty_trash(&docs).await
    }
}
