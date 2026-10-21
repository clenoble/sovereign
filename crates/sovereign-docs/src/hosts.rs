//! What every host runs over the document service (C-DOC adoption step 5,
//! `doc/plans/C-DOC-adoption.md`, S5-3 and S5-5).
//!
//! Tests: `tests/s5_every_host_purges.rs`; the trash listing through the
//! hosts' own tests (`sovereign-shell` and `sovereign-app`, `step5_tests.rs`).

use std::collections::HashMap;
use std::fmt;

use chrono::{DateTime, Utc};
use sovereign_db::schema::Thread;
use sovereign_db::traits::GraphDB;
use sovereign_db::DbError;

use crate::service::DocumentService;
use crate::types::{DocError, PurgeReport, TrashEntry};
use crate::TRASH_RETENTION_DAYS;

/// How often a host runs [`purge_tick`] while unlocked (S5-3: "at unlock and
/// hourly").
pub const PURGE_TICK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(3600);

/// The one host-side purge tick (S5-3), run by the native shell and the
/// mobile host at unlock and every [`PURGE_TICK_INTERVAL`]: purges what has
/// been in the trash for 30 days or more by the service's clock — the
/// document, its versions, its key, its edges, leaving a purge marker (G13) —
/// and keeps the rest. It replaces the mobile host's `purge_deleted` loop.
///
/// It also shrinks deleted threads to their id (THREAD-sync S6, S9): once a
/// deleted thread's deletion stamp is 30 days old and no document — live or
/// in the trash — points to it, its name and description leave the disk and
/// only the id and the deletion stamp remain (a marker, shown nowhere, S7).
pub async fn purge_tick(docs: &DocumentService) -> Result<PurgeReport, DocError> {
    let report = docs.purge_expired().await?;
    docs.shrink_deleted_threads().await?;
    Ok(report)
}

/// Whole days left before a document trashed at `trashed_at` is purged, as of
/// `now`: 30 minus the whole days elapsed, never below 0 nor above 30 (a clock
/// set back does not promise more than the retention).
pub fn days_left(trashed_at: DateTime<Utc>, now: DateTime<Utc>) -> i64 {
    let elapsed = now.signed_duration_since(trashed_at).num_days().max(0);
    (TRASH_RETENTION_DAYS - elapsed).clamp(0, TRASH_RETENTION_DAYS)
}

/// One line of a host's trash view (S5-5): the service's entry, the name of
/// the thread the document was in (`None` when that thread is gone), the old
/// name of that thread when it was deleted (read from its tombstone, for the
/// label "<old name> - deleted", Céline 2026-10-17), and the days left. No
/// content.
#[derive(Clone)]
pub struct TrashListing {
    pub entry: TrashEntry,
    pub thread_name: Option<String>,
    pub deleted_thread_name: Option<String>,
    pub days_left: i64,
}

impl fmt::Debug for TrashListing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TrashListing")
            .field("entry", &self.entry)
            .field("thread_name_bytes", &self.thread_name.as_ref().map(String::len))
            .field("deleted_thread_name_bytes", &self.deleted_thread_name.as_ref().map(String::len))
            .field("days_left", &self.days_left)
            .finish()
    }
}

/// The trash as of `now`, newest first, each document with its thread's name
/// read from `db` (active threads only) and its days left. What the shell's
/// and the mobile host's trash views show (S5-5, S5-9).
pub async fn list_trash_named(
    docs: &DocumentService,
    db: &dyn GraphDB,
    now: DateTime<Utc>,
) -> Result<Vec<TrashListing>, DocError> {
    let entries = docs.list_trash().await?;
    let threads: HashMap<String, String> = db
        .list_threads()
        .await
        .map_err(|e| DocError::Store(e.to_string()))?
        .into_iter()
        .filter(|t| t.deleted_at.is_none())
        .filter_map(|t| t.id_string().map(|id| (id, t.name)))
        .collect();
    // The old names of deleted threads, from their tombstones. A tombstone
    // that cannot be read only leaves the name out.
    let deleted: HashMap<String, String> = if entries.iter().any(|e| !threads.contains_key(&e.thread_id)) {
        db.list_threads_including_deleted()
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|t| t.deleted_at.is_some())
            .filter_map(|t| t.id_string().map(|id| (id, t.name)))
            .filter(|(_, name)| !name.trim().is_empty())
            .collect()
    } else {
        HashMap::new()
    };
    Ok(entries
        .into_iter()
        .map(|entry| {
            let thread_name = threads.get(&entry.thread_id).cloned();
            let deleted_thread_name =
                if thread_name.is_none() { deleted.get(&entry.thread_id).cloned() } else { None };
            TrashListing { thread_name, deleted_thread_name, days_left: days_left(entry.trashed_at, now), entry }
        })
        .collect())
}

// ------------------------------------------------------- thread delete --
//
// S5-10 (decided by Céline 2026-10-17, spec ch. 03, *Thread deletion and the
// fallback lane*): deleting a thread moves every document still in it to the
// trash, then deletes the thread for good. One host-agnostic function, called
// by the assistant's `delete_thread` (sovereign-ai) and the phone's
// `delete_thread` command (sovereign-app).
//
// Tests: `tests/s5_10_thread_delete.rs`, and the orchestrator's and the
// hosts' step-5 tests.

/// What deleting a thread will do, for its confirmation (Destruct, a modal —
/// K7; the thread's name and its number of documents — ch. 08 AT5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadDeletePreview {
    pub thread_id: String,
    pub name: String,
    /// Documents still in the thread (not those already in the trash).
    pub documents: usize,
}

impl ThreadDeletePreview {
    /// The confirmation's text: names the thread and its number of documents,
    /// and says they will be placed in the trash.
    pub fn confirmation_text(&self) -> String {
        let (what, pronoun) = match self.documents {
            1 => ("Its 1 document".to_string(), "it"),
            n => (format!("All {n} of its documents"), "they"),
        };
        format!(
            "Delete the thread \u{201c}{}\u{201d}? {what} will be placed in the trash, where {pronoun} can be \
             restored for 30 days. The thread itself cannot be restored.",
            self.name,
        )
    }
}

/// The preview of deleting `thread_id` (an active thread), read from `db`.
pub async fn preview_thread_delete(
    docs: &DocumentService,
    db: &dyn GraphDB,
    thread_id: &str,
) -> Result<ThreadDeletePreview, DocError> {
    let thread = active_thread(db, thread_id)
        .await
        .map_err(|e| DocError::Store(e.to_string()))?
        .ok_or_else(|| DocError::Store(format!("no active thread {thread_id}")))?;
    let documents = docs.list(Some(thread_id)).await?.len();
    Ok(ThreadDeletePreview { thread_id: thread_id.to_string(), name: thread.name, documents })
}

/// What a completed thread delete did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadDeleteReport {
    /// The documents this delete moved to the trash.
    pub trashed: Vec<crate::types::DocId>,
}

/// A thread delete that did not complete: all or nothing (Céline,
/// 2026-10-17) — the thread is **not** deleted and none of its documents was
/// moved to the trash by it (those already moved came back).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadDeleteError {
    /// What the notice says: that nothing was deleted, and why.
    pub message: String,
}

/// Deletes thread `thread_id`: every document still in it goes to the trash
/// through the service (one `trash` each, so each carries its stamp and sync
/// tombstone), documents already in the trash stay as they are, then the
/// thread is deleted — never listed, never restored by a host; its record
/// remains only as the tombstone sync needs. All or nothing: if a document
/// cannot be trashed, those already moved come back and the thread stays.
pub async fn delete_thread_to_trash(
    docs: &DocumentService,
    db: &dyn GraphDB,
    thread_id: &str,
) -> Result<ThreadDeleteReport, ThreadDeleteError> {
    let refused = |why: &str| ThreadDeleteError { message: format!("{why} Nothing was deleted.") };
    match active_thread(db, thread_id).await {
        Ok(Some(_)) => {}
        Ok(None) => return Err(refused("This thread does not exist, or was already deleted.")),
        Err(_) => return Err(refused("The thread couldn't be read.")),
    }
    let live = docs.list(Some(thread_id)).await.map_err(|_| refused("The thread's documents couldn't be read."))?;

    // One `trash` each: each document carries its own stamp and tombstone.
    let mut moved: Vec<String> = Vec::with_capacity(live.len());
    for d in &live {
        if docs.trash(&d.id).await.is_err() {
            put_back(docs, &moved).await;
            return Err(refused("A document of the thread couldn't be moved to the trash, so the thread was kept."));
        }
        moved.push(d.id.clone());
    }
    // The thread record through the one thread-write path: its deletion is
    // stamped after every document it sent to the trash (THREAD-sync S1).
    if docs.thread_delete(thread_id).await.is_err() {
        put_back(docs, &moved).await;
        return Err(refused("The thread couldn't be deleted."));
    }
    Ok(ThreadDeleteReport { trashed: moved })
}

/// All or nothing: brings back the documents a failed thread delete moved to
/// the trash. Best effort: a store that fails here too (a crash) leaves them
/// in the trash with the thread still there; running the same delete again
/// completes it (it never touches documents already in the trash).
async fn put_back(docs: &DocumentService, moved: &[String]) {
    for id in moved {
        let _ = docs.restore_from_trash(id).await;
    }
}

/// Thread `id` when it exists and is not deleted; `None` for an unknown,
/// malformed or deleted thread.
pub async fn active_thread(db: &dyn GraphDB, id: &str) -> Result<Option<Thread>, DbError> {
    match db.get_thread(id).await {
        Ok(t) if t.deleted_at.is_none() => Ok(Some(t)),
        Ok(_) | Err(DbError::NotFound(_)) | Err(DbError::InvalidId(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

// ------------------------------------------------- restore from the trash --
//
// S5-6 revised (Céline, 2026-10-17; spec ch. 03, *Soft delete and 30-day
// purge*, the gestures): a document comes back from the trash into its
// thread; if its thread was deleted, the user chooses the thread it goes to,
// and it never lands in the fallback lane from the trash.

/// Brings document `id` back from the trash: into `into` (an existing thread
/// the user chose) when given, else into its own thread, which must still
/// exist. A deleted or unknown target is refused and nothing is restored; the
/// error says why, for the notice.
pub async fn restore_to_thread(
    docs: &DocumentService,
    db: &dyn GraphDB,
    id: &str,
    into: Option<&str>,
) -> Result<(), String> {
    let view = docs.read(id).await.map_err(|e| match e {
        DocError::Purged(_) | DocError::NotFound(_) => "This document no longer exists.".to_string(),
        other => format!("The document couldn't be read ({other}). Nothing was restored."),
    })?;
    if view.trashed_at.is_none() {
        return Err("This document is not in the trash.".into());
    }
    let target = into.unwrap_or(&view.thread_id);
    let exists = active_thread(db, target)
        .await
        .map_err(|e| format!("The thread couldn't be read ({e}). Nothing was restored."))?
        .is_some();
    if !exists {
        return Err(match into {
            Some(_) => "That thread does not exist (it was deleted, or never existed): choose an existing thread. \
                        Nothing was restored."
                .into(),
            None => "Its thread was deleted: choose a thread to restore it into. Nothing was restored.".into(),
        });
    }
    let moved = target != view.thread_id;
    if moved {
        db.move_document_to_thread(id, target)
            .await
            .map_err(|e| format!("The document couldn't be moved to that thread ({e}). Nothing was restored."))?;
    }
    if let Err(e) = docs.restore_from_trash(id).await {
        if moved {
            let _ = db.move_document_to_thread(id, &view.thread_id).await;
        }
        return Err(match e {
            DocError::NotInTrash(_) => "This document is not in the trash.".into(),
            other => format!("The document couldn't be restored ({other})."),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::days_left;
    use chrono::{Duration, Utc};

    #[test]
    fn days_left_counts_whole_days_and_stays_within_the_retention() {
        let t = Utc::now();
        assert_eq!(days_left(t, t), 30);
        assert_eq!(days_left(t, t + Duration::hours(23)), 30);
        assert_eq!(days_left(t, t + Duration::days(1)), 29);
        assert_eq!(days_left(t, t + Duration::days(29)), 1);
        assert_eq!(days_left(t, t + Duration::days(30)), 0);
        assert_eq!(days_left(t, t + Duration::days(45)), 0);
        assert_eq!(days_left(t, t - Duration::days(3)), 30, "a clock set back");
    }
}
