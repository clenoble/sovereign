//! The notice after a one-click delete, and its Undo (C-DOC adoption step 5,
//! S5-8; spec ch. 03, *Soft delete and 30-day purge*, decided 2026-10-17:
//! "Deleting a document is one click; the notice says it is in the trash and
//! can be restored for 30 days, and offers Undo").
//!
//! `App` shows one at the bottom of the canvas after a delete
//! (`App::draw_delete_notice`). Time is passed in, so the Undo window is
//! testable.

use std::time::{Duration, Instant};

use sovereign_docs::DocumentService;

/// How long the notice, and its Undo, stay shown. **[choice]** (plan, S5-8).
pub(crate) const UNDO_WINDOW: Duration = Duration::from_secs(10);

/// The notice for one document just moved to the trash.
pub(crate) struct DeleteNotice {
    doc_id: String,
    shown_at: Instant,
    /// Undo already ran (successfully): there is nothing left to undo.
    undone: bool,
}

impl DeleteNotice {
    /// The notice for `doc_id`, shown at `shown_at`.
    pub(crate) fn new(doc_id: String, shown_at: Instant) -> DeleteNotice {
        DeleteNotice { doc_id, shown_at, undone: false }
    }

    /// The document it is about.
    #[cfg_attr(not(test), allow(dead_code))] // read by the tests; `App` needs only Undo
    pub(crate) fn doc_id(&self) -> &str {
        &self.doc_id
    }

    /// What it says ("Moved to the trash — it can be restored for 30 days").
    pub(crate) fn text(&self) -> String {
        "Moved to the trash \u{2014} it can be restored for 30 days".to_string()
    }

    /// Whether it (and its Undo) is still shown at `now`.
    pub(crate) fn shown(&self, now: Instant) -> bool {
        !self.undone && now.saturating_duration_since(self.shown_at) < UNDO_WINDOW
    }

    /// Undo, clicked at `now`: brings the document back from the trash while
    /// the notice is shown. If the notice is gone, the document has left the
    /// trash (restored or purged meanwhile), or its thread was deleted
    /// meanwhile, nothing is done and the error says so.
    pub(crate) async fn undo(&mut self, docs: &DocumentService, now: Instant) -> Result<(), String> {
        if self.undone {
            return Err("Already undone: the document is back.".into());
        }
        if !self.shown(now) {
            return Err("Too late to undo here: the document is in Settings \u{2192} Trash.".into());
        }
        // The one restore path: into its own thread, which must still exist
        // (a thread deleted inside the Undo window took it away; the document
        // stays in the trash, restorable there into a chosen thread).
        match sovereign_docs::restore_to_thread(docs, docs.store(), &self.doc_id, None).await {
            Ok(()) => {
                self.undone = true;
                Ok(())
            }
            Err(msg) => Err(msg),
        }
    }
}
