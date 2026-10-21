//! The shell's trash view, as state and service calls (C-DOC adoption step 5,
//! S5-5/S5-6/S5-7/S5-11; spec ch. 03, *Soft delete and 30-day purge* and
//! *Thread deletion and the fallback lane*, decided by Céline 2026-10-10 and
//! 2026-10-17). It is drawn in the Settings panel (`App`, Settings → Trash).
//!
//! A document comes back into its own thread; if that thread was deleted, the
//! user chooses an existing thread, alone or for the whole group of the
//! deleted thread. Nothing restored from the trash lands in the fallback
//! lane. There is no per-item "delete forever" (Q1): only Empty trash, after
//! its confirmation, and the 30-day purge.

use chrono::{DateTime, Utc};
use sovereign_db::traits::GraphDB;
use sovereign_docs::{active_thread, list_trash_named, restore_to_thread, DocumentService};

/// One line of the trash view — and no content.
#[derive(Clone, Debug)]
pub(crate) struct TrashLine {
    pub(crate) id: String,
    pub(crate) title: String,
    /// The thread the document was in.
    pub(crate) thread_id: String,
    /// The thread's name; when the thread was deleted, a text saying so
    /// (never the fallback lane's name).
    pub(crate) thread_name: String,
    /// Its thread was deleted: restoring it needs a thread chosen by the user.
    pub(crate) thread_deleted: bool,
    pub(crate) trashed_at: DateTime<Utc>,
    /// Whole days left before the document is purged (30 days after it was
    /// trashed).
    pub(crate) days_left: i64,
}

/// The documents of one deleted thread in the trash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TrashGroup {
    /// The deleted thread.
    pub(crate) thread_id: String,
    /// What the group's heading says (that the thread was deleted).
    pub(crate) label: String,
    /// Its documents in the trash.
    pub(crate) ids: Vec<String>,
}

/// The trash view.
pub(crate) struct TrashView {
    lines: Vec<TrashLine>,
    /// "Empty trash" was clicked and its confirmation is showing (K7: a
    /// Destruct action asks first, in a modal).
    empty_requested: bool,
}

impl TrashView {
    /// The trash as of `now`, newest first; thread names read from `db`.
    pub(crate) async fn load(docs: &DocumentService, db: &dyn GraphDB, now: DateTime<Utc>) -> Result<TrashView, String> {
        Ok(TrashView { lines: read_lines(docs, db, now).await?, empty_requested: false })
    }

    /// The lines shown.
    pub(crate) fn lines(&self) -> &[TrashLine] {
        &self.lines
    }

    /// Brings `id` back from the trash into its own thread; the view drops it
    /// at once. If its thread was deleted, nothing is restored and the error
    /// says a thread must be chosen (`restore_into`).
    pub(crate) async fn restore(
        &mut self,
        docs: &DocumentService,
        db: &dyn GraphDB,
        id: &str,
        now: DateTime<Utc>,
    ) -> Result<(), String> {
        let restored = restore_to_thread(docs, db, id, None).await;
        self.refresh(docs, db, now, &[id]).await;
        restored
    }

    /// Re-reads the trash, so whatever happened elsewhere meanwhile (a sync
    /// round, the hourly purge) shows at once; if the trash cannot be read,
    /// drops the documents `restored` only.
    async fn refresh(&mut self, docs: &DocumentService, db: &dyn GraphDB, now: DateTime<Utc>, restored: &[&str]) {
        match read_lines(docs, db, now).await {
            Ok(lines) => self.lines = lines,
            Err(_) => {
                for id in restored {
                    if docs.read(id).await.is_ok_and(|v| v.trashed_at.is_none()) {
                        self.lines.retain(|l| l.id != *id);
                    }
                }
            }
        }
    }

    /// The number of documents in the trash (Settings shows it).
    pub(crate) fn count(&self) -> usize {
        self.lines.len()
    }

    /// Brings `id` back from the trash into `thread_id`, an existing thread the
    /// user chose — the way back for a document whose thread was deleted
    /// (Céline, 2026-10-17). A thread that does not exist (deleted, unknown)
    /// is refused and nothing is restored. The view drops the document at once.
    pub(crate) async fn restore_into(
        &mut self,
        docs: &DocumentService,
        db: &dyn GraphDB,
        id: &str,
        thread_id: &str,
        now: DateTime<Utc>,
    ) -> Result<(), String> {
        let restored = restore_to_thread(docs, db, id, Some(thread_id)).await;
        self.refresh(docs, db, now, &[id]).await;
        restored
    }

    /// The documents of each deleted thread, grouped (Céline, 2026-10-17: "In
    /// the trash, a deleted thread's documents are grouped"); one group per
    /// deleted thread, its documents in the order of `lines`. Documents whose
    /// thread exists are in no group.
    pub(crate) fn deleted_thread_groups(&self) -> Vec<TrashGroup> {
        let mut groups: Vec<TrashGroup> = Vec::new();
        for l in self.lines.iter().filter(|l| l.thread_deleted) {
            match groups.iter_mut().find(|g| g.thread_id == l.thread_id) {
                Some(g) => g.ids.push(l.id.clone()),
                None => groups.push(TrashGroup {
                    thread_id: l.thread_id.clone(),
                    label: l.thread_name.clone(),
                    ids: vec![l.id.clone()],
                }),
            }
        }
        groups
    }

    /// "Restore all into a thread…": brings every document of the deleted
    /// thread `deleted_thread_id` back from the trash into `into_thread_id`
    /// (an existing thread), each exactly; the view drops them at once. A
    /// thread that does not exist is refused and nothing is restored. The
    /// number of documents restored.
    ///
    /// The target is checked before anything moves. If a document fails on
    /// the way (it left the trash meanwhile, a store error), the others are
    /// still restored and the error says how many came back.
    pub(crate) async fn restore_group_into(
        &mut self,
        docs: &DocumentService,
        db: &dyn GraphDB,
        deleted_thread_id: &str,
        into_thread_id: &str,
        now: DateTime<Utc>,
    ) -> Result<usize, String> {
        let exists = active_thread(db, into_thread_id)
            .await
            .map_err(|e| format!("The thread couldn't be read ({e}). Nothing was restored."))?
            .is_some();
        if !exists || into_thread_id == deleted_thread_id {
            return Err("That thread does not exist (it was deleted, or never existed): choose an existing thread. \
                        Nothing was restored."
                .into());
        }
        let ids: Vec<String> = self
            .deleted_thread_groups()
            .into_iter()
            .find(|g| g.thread_id == deleted_thread_id)
            .map(|g| g.ids)
            .unwrap_or_default();
        let mut restored = 0;
        let mut failed: Option<String> = None;
        for id in &ids {
            match restore_to_thread(docs, db, id, Some(into_thread_id)).await {
                Ok(()) => restored += 1,
                Err(e) => {
                    failed.get_or_insert(e);
                }
            }
        }
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        self.refresh(docs, db, now, &refs).await;
        match failed {
            None => Ok(restored),
            Some(e) => Err(format!("{restored} of {} documents were restored. {e}", ids.len())),
        }
    }

    /// "Empty trash" clicked: asks first (Destruct, a modal — K7). Nothing is
    /// purged yet.
    pub(crate) fn request_empty(&mut self) {
        self.empty_requested = true;
    }

    /// The empty-trash confirmation is showing.
    pub(crate) fn empty_requested(&self) -> bool {
        self.empty_requested
    }

    /// The confirmation dismissed: nothing purged.
    pub(crate) fn cancel_empty(&mut self) {
        self.empty_requested = false;
    }

    /// The confirmation accepted: empties the trash (`empty_trash`) and
    /// reports how many documents were purged. Without a showing confirmation
    /// it purges nothing.
    pub(crate) async fn confirm_empty(&mut self, docs: &DocumentService) -> Result<usize, String> {
        if !self.empty_requested {
            return Err("Emptying the trash needs its confirmation. Nothing was purged.".into());
        }
        self.empty_requested = false;
        let report = docs
            .empty_trash()
            .await
            .map_err(|e| format!("The trash couldn't be emptied ({e}). Some documents may remain in it."))?;
        self.lines.clear();
        Ok(report.documents.len())
    }
}

async fn read_lines(docs: &DocumentService, db: &dyn GraphDB, now: DateTime<Utc>) -> Result<Vec<TrashLine>, String> {
    let listing = list_trash_named(docs, db, now)
        .await
        .map_err(|e| format!("The trash couldn't be read ({e})."))?;
    Ok(listing
        .into_iter()
        .map(|l| {
            let thread_deleted = l.thread_name.is_none();
            let thread_name = l.thread_name.unwrap_or_else(|| deleted_thread_label(l.deleted_thread_name.as_deref()));
            TrashLine {
                id: l.entry.id,
                title: l.entry.title,
                thread_id: l.entry.thread_id,
                thread_name,
                thread_deleted,
                trashed_at: l.entry.trashed_at,
                days_left: l.days_left,
            }
        })
        .collect())
}

/// What a line or a group says when its thread was deleted: the thread's old
/// name, then " - deleted" (Céline, 2026-10-17).
fn deleted_thread_label(name: Option<&str>) -> String {
    match name {
        Some(n) => format!("{n} - deleted"),
        None => "Deleted thread".to_string(),
    }
}
