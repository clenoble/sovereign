//! What the host gives the editor, and nothing else (C-TEXT, *What the host
//! gives it*). The editor reaches storage, the clipboard, the platform's
//! accessibility layer, time and the window only through these.

use std::time::Duration;

use crate::Rect;

/// Identifies one save the editor asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SaveId(pub u64);

/// A save the editor asks the document service for (guarantee 1: "a save
/// writes exactly that model").
#[derive(Clone)]
pub struct SaveRequest {
    pub id: SaveId,
    /// The whole document in the stored v2 form — what
    /// [`crate::doc::DocumentV2::to_json_string`] returned for the in-memory
    /// model at the moment of the request.
    pub content: String,
    /// The document's title to save (title contract T2, T4: trimmed of leading
    /// and trailing spaces), or `None` when the title is unchanged since what
    /// the store is known to hold. A document editor's title and body changes
    /// made together travel in one request.
    pub title: Option<String>,
}

/// Why the document service could not save. Reported to the user (guarantee 1:
/// "a failed save keeps it and says so").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveError {
    pub message: String,
}

/// The document service (contract C-DOC): encrypts, commits and runs
/// personal-data ingest. Saving is asynchronous: the host answers each request
/// later with [`crate::Editor::save_finished`].
pub trait DocumentService {
    fn save(&mut self, request: SaveRequest);
}

/// How a copy is marked (C-TEXT, *Clipboard*): every copy is excluded from
/// clipboard history, from cloud clipboard sync and from clipboard monitoring,
/// where the platform allows it — the host applies the flags it can.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipboardPrivacy {
    pub exclude_from_history: bool,
    pub exclude_from_cloud_sync: bool,
    pub exclude_from_monitoring: bool,
}

/// What the clipboard holds: plain text for every application, and an opaque
/// Sovereign payload that only a Sovereign editor reads.
///
/// The payload is what keeps a personal-data token a token across copy and
/// paste inside Sovereign (decided 2026-10-10): the text carries the token's
/// label (or its value when revealed); the payload carries the token itself.
/// The payload never carries a revealed value. Its format is the editor's.
/// Pasting it calls [`PiiDisplay::link_to_document`] for the tokens it holds;
/// pasting plain text never does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardContent {
    pub text: String,
    pub sovereign: Option<String>,
}

/// One clipboard write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardWrite {
    pub text: String,
    /// The Sovereign payload (see [`ClipboardContent`]), if the copy needs one.
    pub sovereign: Option<String>,
    pub privacy: ClipboardPrivacy,
    /// When the host must clear the clipboard again. `Some(30 s)` when the
    /// copied text includes a revealed personal-data value (ch. 09, decided
    /// 2026-10-10); `None` otherwise — ordinary text is never cleared.
    pub clear_after: Option<Duration>,
}

/// The clipboard capability. The clipboard holds one item: a write replaces it.
/// The host applies the privacy flags it can, and clears the clipboard after
/// `clear_after` (if it still holds that item).
pub trait Clipboard {
    fn read(&mut self) -> Option<ClipboardContent>;
    fn write(&mut self, item: ClipboardWrite);
}

/// Where the editor sends its AccessKit tree updates (guarantee 10). Updates
/// use the tree id given in [`crate::EditorConfig::accessibility_tree`].
pub trait AccessibilitySink {
    fn update(&mut self, update: accesskit::TreeUpdate);
}

/// A monotonic clock (autosave timing). Given by the host so tests are
/// deterministic.
pub trait Clock {
    /// Time since an arbitrary fixed origin.
    fn now(&self) -> Duration;
}

/// How text in token form `[pii:<record_id>]` is shown (ch. 09): as a token —
/// its label, or its value when the host has revealed it (the reveal itself is
/// gated by the host) — or as ordinary text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokenDisplay {
    Label(String),
    Revealed(String),
    /// Not a token in this document: show and edit the text as typed. The
    /// resolver rule (decided 2026-10-10): a token is a token only in a
    /// document listed among its personal-data record's sources.
    NotAToken,
}

/// Resolves text in token form `[pii:<record_id>]` for the document this
/// editor has open (guarantee 7). The host applies the resolver rule: a token
/// is shown as a token only in a document listed among its record's sources;
/// anywhere else it is ordinary text. The editor escapes nothing: what the
/// user types is stored as typed.
pub trait PiiDisplay {
    /// `record_id` is the text between `[pii:` and `]` (record ids contain
    /// `:`, e.g. `pii_record:x7k2`).
    fn display(&self, record_id: &str) -> TokenDisplay;

    /// Tokens pasted from a genuine Sovereign copy (the clipboard's Sovereign
    /// payload, not its plain text) entered this document: the host adds this
    /// document to those records' sources. Called at the paste, so the pasted
    /// tokens are tokens at once.
    fn link_to_document(&mut self, record_ids: &[String]);
}

/// The window the editor sits in.
pub trait WindowHost {
    /// Where the input-method candidate window should go: the caret area, in
    /// the editor's coordinate space (guarantee 6).
    fn set_ime_cursor_area(&mut self, area: Rect);
    /// The editor needs to be painted again.
    fn request_redraw(&mut self);
}

/// Notice that the open document changed in the store because of sync.
#[derive(Clone)]
pub struct RemoteChange {
    /// The document's new stored content.
    pub content: String,
    /// The paired device the change came from.
    pub device: String,
}

/// Everything the host gives the editor.
pub struct Host {
    pub documents: Box<dyn DocumentService>,
    pub clipboard: Box<dyn Clipboard>,
    pub accessibility: Box<dyn AccessibilitySink>,
    pub clock: Box<dyn Clock>,
    pub pii: Box<dyn PiiDisplay>,
    pub window: Box<dyn WindowHost>,
}
