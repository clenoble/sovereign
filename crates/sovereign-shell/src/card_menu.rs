//! The card context menu by document identity (C-DOC adoption step 5, S5-4;
//! spec ch. 08, *Action targets*, AT8: "A menu, a list or a drag in any host
//! carries the object's id, never its index in a list that a reload can
//! reorder").
//!
//! The menu captures the document's id when it opens, and every row acts on
//! that id: a workspace reload in between (a sync round, a document created
//! from chat) cannot change which document a row names. `App`'s right-click
//! menu holds one (`CtxTarget::Card`).
//!
//! Delete is one click (Céline, 2026-10-17, Q3 → b): the document goes to the
//! trash at once, and the notice offers Undo (`delete_notice`).

use sovereign_docs::{DocError, DocumentService};

/// Which rows the menu shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rows {
    /// Open, Pin, Skills ›, Delete.
    Main,
    /// The skills submenu, for the same document.
    Skills,
}

/// A card menu, open on one document.
#[derive(Clone, Debug)]
pub(crate) struct CardMenu {
    id: String,
    rows: Rows,
}

impl CardMenu {
    /// Opens the menu on the card at `index` of `card_ids` (the cards as shown
    /// when the right-click landed). `None` if there is no such card.
    pub(crate) fn open_on(card_ids: &[String], index: usize) -> Option<CardMenu> {
        card_ids.get(index).map(|id| CardMenu { id: id.clone(), rows: Rows::Main })
    }

    /// The document every row of this menu acts on.
    pub(crate) fn target_id(&self) -> &str {
        &self.id
    }

    /// A click on the Delete row: the document to move to the trash, at the
    /// first click (one click, with Undo). `None` is never returned today; the
    /// signature leaves room for a confirmation step.
    pub(crate) fn click_delete(&mut self) -> Option<String> {
        Some(self.id.clone())
    }

    /// A click on "Skills ›": the menu switches to the skills submenu for the
    /// same document.
    pub(crate) fn open_skills(&mut self) {
        self.rows = Rows::Skills;
    }

    /// The skills submenu is showing.
    pub(crate) fn in_skills(&self) -> bool {
        self.rows == Rows::Skills
    }

    /// A click on skill `skill` (an index into the skill registry, which a
    /// workspace reload does not reorder) in the skills submenu: the document
    /// and the skill to run.
    pub(crate) fn click_skill(&mut self, skill: usize) -> (String, usize) {
        (self.id.clone(), skill)
    }
}

/// Moves the menu's document to the trash, by id, through the service. If the
/// document is gone (already in the trash, purged, never existed), nothing is
/// done and the error says so, for the notice.
///
/// The service's `trash` is idempotent (a document already in the trash
/// answers `Ok`), so the document is read first: a menu confirmed on a
/// document trashed meanwhile (from another window, or a paired device through
/// sync) must say so rather than report a delete it did not make.
pub(crate) async fn trash_target(docs: &DocumentService, id: &str) -> Result<(), String> {
    let view = docs.read(id).await.map_err(|e| gone_message(&e))?;
    if view.trashed_at.is_some() {
        return Err("This document is already in the trash. Nothing was changed.".into());
    }
    docs.trash(id).await.map_err(|e| gone_message(&e))
}

/// The notice for a delete that did not happen.
fn gone_message(e: &DocError) -> String {
    match e {
        DocError::Purged(_) | DocError::NotFound(_) => "This document no longer exists. Nothing was changed.".into(),
        DocError::InTrash(_) => "This document is already in the trash. Nothing was changed.".into(),
        other => format!("The document couldn't be moved to the trash ({other}). Nothing was changed."),
    }
}
