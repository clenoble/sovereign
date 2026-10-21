//! The document editor (`sovereign-editor`, contract C-TEXT, spec ch. 12)
//! inside the shell's document window — the shell's only document editor, in
//! every build (contract E1–E9, `doc/plans/EDITOR-default.md`: the plain text
//! window was removed). The file keeps its historical name.
//!
//! A document window edits through the editor, which saves the **v2** block
//! format (ch. 03) through the document service (C-DOC adoption step 2,
//! [`save_request`]): personal data ingested at every save, versions by amount
//! of change (Ctrl+S and the end of the session always), a refused save from
//! another writer turned into the editor's remote-change flow. When the editor
//! cannot open a document, no window opens and a notice says why (E3).
//!
//! What lives here:
//! - the host adapters the editor asks for (`host.rs`): the document service,
//!   the clipboard (arboard + the privacy flags + the clear timer), the
//!   personal-data resolver rule (ch. 09, *Where a token resolves*), the
//!   accessibility sink, the clock and the window;
//! - input routing: winit keys / text / IME / pointer / focus → `InputEvent`,
//!   without sending a key's text twice;
//! - [`EditorPane`], what a document window holds, and the `impl App` glue the
//!   rest of `app.rs` calls.
//!
//! The development switches still read here (`SHELL_FAIL_SAVES_WHILE`,
//! `SHELL_SEED_PII_TEST`) sit in items compiled only into debug builds or
//! with the `dev-switches` feature (see `dev.rs`, E6).
//!
//! This is a child module of `app`, so it reaches the window manager's private
//! helpers (notices, the unsaved prompt) without widening their visibility.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use parley::Layout;
use sovereign_db::schema::{PiiRecord, SourceKind};
use sovereign_db::traits::GraphDB;
use sovereign_editor::host::{
    AccessibilitySink, Clipboard, ClipboardContent, ClipboardPrivacy, ClipboardWrite, Clock, DocumentService, Host,
    PiiDisplay, RemoteChange, SaveError, SaveId, SaveRequest, TokenDisplay, WindowHost,
};
use sovereign_editor::input::{Command, InputEvent, Modifiers, PointerButton, PointerEvent};
#[cfg(test)]
use sovereign_editor::input::{Key, KeyEvent, NamedKey};
use sovereign_editor::{
    CloseRequest, Editor, EditorConfig, EventResult, Fonts, NoticeKind, OpenDocument, OpenMode, ReadOnlyReason,
    SaveStatus, Trust,
};
use sovereign_docs::{
    DocError, DocumentService as DocService, Edit, EditContent, EditSession, Origin, PastedToken, Revision,
    TokenStatus as DocTokenStatus,
};
use std::collections::{HashMap, HashSet};
use vello::kurbo::{Affine, Point, Rect};
use vello::peniko::Fill;
use vello::Scene;
use winit::keyboard::{Key as WKey, ModifiersState, NamedKey as WNamed};

use super::{App, WindowContent};
use crate::clip::set_private;
use crate::panels::window_chrome;
use crate::text::{draw_text, Brush, TextShaper};
use crate::theme::{editor_palette, pal};
#[cfg(test)]
use crate::winit_input::EventSink;
use crate::winit_input::{click_count, map_ime};
// Re-exported where it lived before the move (the title tests import it here).
pub(crate) use crate::winit_input::route_key;

/// Height of the status strip (the status line: Saved / Saving / Save failed
/// / read-only / notes) above the editor.
const STATUS_STRIP_H: f64 = 22.0;

// ======================================================================== clock

/// One monotonic origin for the whole process (editors and the clipboard
/// timer read the same clock).
fn mono_now() -> Duration {
    static ORIGIN: OnceLock<Instant> = OnceLock::new();
    ORIGIN.get_or_init(Instant::now).elapsed()
}

struct ShellClock;
impl Clock for ShellClock {
    fn now(&self) -> Duration {
        mono_now()
    }
}

// ======================================================================== process-wide UI state

thread_local! {
    /// Keyboard modifiers, from `WindowEvent::ModifiersChanged`.
    static MODS: Cell<ModifiersState> = const { Cell::new(ModifiersState::empty()) };
    /// Whether the OS window has focus (`WindowEvent::Focused`).
    static WINDOW_FOCUSED: Cell<bool> = const { Cell::new(true) };
    /// The clipboard is one per process: all editors share this state.
    static CLIPBOARD: Rc<RefCell<ClipState>> = Rc::new(RefCell::new(ClipState::new(Box::new(OsClipboard::default()))));
}

pub(crate) fn set_modifiers(m: ModifiersState) {
    MODS.with(|c| c.set(m));
}

pub(crate) fn set_window_focused(f: bool) {
    WINDOW_FOCUSED.with(|c| c.set(f));
}

/// Tests: this thread's shared clipboard state talks to `sys` instead of the
/// operating system's clipboard (a test never touches the user's clipboard).
#[cfg(test)]
pub(crate) fn use_test_system_clipboard(sys: Box<dyn SystemClipboard>) {
    CLIPBOARD.with(|c| *c.borrow_mut() = ClipState::new(sys));
}

/// Tests: this thread's shared clipboard state (what every editor copies to).
#[cfg(test)]
pub(crate) fn clipboard_for_tests() -> Rc<RefCell<ClipState>> {
    CLIPBOARD.with(|c| c.clone())
}

/// Tests: editing sessions of closed editors still waiting to be ended.
#[cfg(test)]
pub(crate) fn pending_session_ends() -> usize {
    ENDED.with(|e| e.borrow().len())
}

fn editor_mods() -> Modifiers {
    let m = MODS.with(|c| c.get());
    Modifiers { shift: m.shift_key(), ctrl: m.control_key(), alt: m.alt_key(), meta: m.super_key() }
}

// ======================================================================== document service

/// Save requests wait here until the shell runs them through its save path
/// (next frame); the answer goes back with `Editor::save_finished`.
struct QueuedSaves(Rc<RefCell<Vec<SaveRequest>>>);
impl DocumentService for QueuedSaves {
    fn save(&mut self, request: SaveRequest) {
        self.0.borrow_mut().push(request);
    }
}

// ======================================================================== clipboard

/// The operating system's clipboard, as the adapter needs it. A trait so the
/// clear timer and the "is it still ours" rule are testable without the OS.
pub(crate) trait SystemClipboard {
    /// Replace the clipboard with `text`, applying the privacy flags the
    /// platform allows.
    fn write_text(&mut self, text: &str, privacy: ClipboardPrivacy) -> Result<(), String>;
    fn read_text(&mut self) -> Option<String>;
    /// A value that changes whenever anyone changes the clipboard (Windows:
    /// the clipboard sequence number). `None` where the platform has none.
    fn change_count(&self) -> Option<u64>;
    fn clear(&mut self) -> Result<(), String>;
}

/// What this process last put on the clipboard.
struct Ours {
    text: String,
    sovereign: Option<String>,
    change: Option<u64>,
    /// The document the copy was made in: a paste of its tokens elsewhere
    /// links them only if they are linked there (G14).
    from_document: Option<String>,
}

/// The shell's clipboard state, shared by every editor.
///
/// **The Sovereign payload stays in this process.** arboard 3.6 has no API for
/// a private clipboard format (text, HTML, images and file lists only), and
/// putting the payload in the HTML flavour would hand record ids to every
/// application that pastes HTML. So the payload is kept here, beside a marker
/// of the clipboard item it belongs to, and is handed back to the editor only
/// while the clipboard still holds that item: on Windows, the clipboard
/// sequence number is unchanged since our write; elsewhere, the clipboard
/// text is still ours. That is the acceptance rule of C-TEXT (*this process
/// wrote it and it is still the current item*), and record ids never leave the
/// process. (The editor applies the same rule again on its side.)
pub(crate) struct ClipState {
    sys: Box<dyn SystemClipboard>,
    ours: Option<Ours>,
    /// When to clear the clipboard (`clear_after` of the current item).
    clear_at: Option<Duration>,
    /// A write that failed, for the shell to say so.
    pub(crate) error: Option<String>,
}

impl ClipState {
    pub(crate) fn new(sys: Box<dyn SystemClipboard>) -> ClipState {
        ClipState { sys, ours: None, clear_at: None, error: None }
    }

    /// Whether the clipboard still holds the item this process wrote last.
    fn holds_ours(&mut self) -> bool {
        let Some(o) = &self.ours else { return false };
        match (o.change, self.sys.change_count()) {
            (Some(a), Some(b)) => a == b,
            _ => {
                let text = o.text.clone();
                self.sys.read_text().as_deref() == Some(text.as_str())
            }
        }
    }

    /// (Used by the tests; the editors write with [`write_from`](Self::write_from).)
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn write(&mut self, item: ClipboardWrite, now: Duration) {
        self.write_from(item, now, None);
    }

    /// [`write`](Self::write), remembering the document the copy was made in.
    pub(crate) fn write_from(&mut self, item: ClipboardWrite, now: Duration, from_document: Option<String>) {
        // A new copy replaces the previous item, and with it its pending clear.
        self.clear_at = None;
        self.ours = None;
        match self.sys.write_text(&item.text, item.privacy) {
            Ok(()) => {
                self.ours = Some(Ours {
                    text: item.text,
                    sovereign: item.sovereign,
                    change: self.sys.change_count(),
                    from_document,
                });
                self.clear_at = item.clear_after.map(|d| now + d);
            }
            Err(e) => self.error = Some(e),
        }
    }

    pub(crate) fn read(&mut self) -> Option<ClipboardContent> {
        let text = self.sys.read_text()?;
        let sovereign = if self.holds_ours() {
            self.ours.as_ref().filter(|o| o.text == text).and_then(|o| o.sovereign.clone())
        } else {
            None
        };
        Some(ClipboardContent { text, sovereign })
    }

    /// The document the clipboard's current item was copied from, while it is
    /// still this process's copy.
    pub(crate) fn source_document(&mut self) -> Option<String> {
        if self.holds_ours() {
            self.ours.as_ref().and_then(|o| o.from_document.clone())
        } else {
            None
        }
    }

    /// Forget this process's copy, when the store is replaced (E9): its
    /// private payload and the document it came from are dropped, and a
    /// pending clear is run now — a revealed value still on the clipboard is
    /// cleared at once, not left to its timer. Another application's copy, and
    /// an ordinary copy of ours with no clear pending, stay on the clipboard.
    pub(crate) fn forget(&mut self) {
        if self.clear_at.take().is_some() && self.holds_ours() {
            if let Err(e) = self.sys.clear() {
                self.error = Some(e);
            }
        }
        self.ours = None;
    }

    /// Run the clear timer: at its deadline, clear the clipboard **only if it
    /// still holds the item the timer belongs to** — another application's copy
    /// (or a later one of ours) is left alone.
    pub(crate) fn tick(&mut self, now: Duration) {
        let Some(at) = self.clear_at else { return };
        if now < at {
            return;
        }
        self.clear_at = None;
        if self.holds_ours() {
            if let Err(e) = self.sys.clear() {
                self.error = Some(e);
            }
        }
        self.ours = None;
    }
}

/// The real clipboard, through arboard. One `arboard::Clipboard` is kept for
/// the process: on Linux, dropping it right after a write can lose the data.
#[derive(Default)]
struct OsClipboard {
    cb: Option<arboard::Clipboard>,
}

impl OsClipboard {
    fn cb(&mut self) -> Result<&mut arboard::Clipboard, String> {
        if self.cb.is_none() {
            self.cb = Some(arboard::Clipboard::new().map_err(|e| format!("clipboard unavailable: {e}"))?);
        }
        Ok(self.cb.as_mut().expect("just set"))
    }
}

// `set_private` (the privacy flags of a copy): in `crate::clip`, shared with
// the new-thread prompt's field.

#[cfg(windows)]
fn clipboard_sequence_number() -> Option<u64> {
    #[link(name = "user32")]
    extern "system" {
        fn GetClipboardSequenceNumber() -> u32;
    }
    // Safe: no arguments, no pointers. 0 means "no access" — then there is no
    // usable marker and the text comparison is used instead.
    let n = unsafe { GetClipboardSequenceNumber() };
    (n != 0).then_some(n as u64)
}

#[cfg(not(windows))]
fn clipboard_sequence_number() -> Option<u64> {
    None
}

impl SystemClipboard for OsClipboard {
    fn write_text(&mut self, text: &str, privacy: ClipboardPrivacy) -> Result<(), String> {
        let cb = self.cb()?;
        set_private(cb, text, privacy).map_err(|e| format!("the copy could not be written to the clipboard ({e})"))
    }
    fn read_text(&mut self) -> Option<String> {
        self.cb().ok()?.get_text().ok()
    }
    fn change_count(&self) -> Option<u64> {
        clipboard_sequence_number()
    }
    fn clear(&mut self) -> Result<(), String> {
        self.cb()?.clear().map_err(|e| format!("the clipboard could not be cleared ({e})"))
    }
}

/// The editor's clipboard capability: the shared state, and the document this
/// editor shows (recorded with each copy).
struct ShellClipboard {
    state: Rc<RefCell<ClipState>>,
    doc: String,
}
impl Clipboard for ShellClipboard {
    fn read(&mut self) -> Option<ClipboardContent> {
        self.state.borrow_mut().read()
    }
    fn write(&mut self, item: ClipboardWrite) {
        self.state.borrow_mut().write_from(item, mono_now(), Some(self.doc.clone()));
    }
}

// ======================================================================== personal data

/// Whether a record id is worth asking the database about: the editor hands
/// over whatever sits between `[pii:` and `]`, which is user text.
fn plausible_record_id(id: &str) -> bool {
    id.len() <= 128 && id.strip_prefix("pii_record:").is_some_and(|k| !k.is_empty() && !k.contains(char::is_whitespace))
}

/// The resolver rule (ch. 09, *Where a token resolves*, decided 2026-10-10): a
/// token form is a token in this document only if its record exists and the
/// record's sources include this document — or, until the next save, the
/// record was display-linked here by a paste from a genuine Sovereign copy;
/// anywhere else it is ordinary text. A token shows at the `preview` level:
/// its kind's label (ch. 09 table). Reveal is not offered in the editor yet
/// (no `Revealed` answer).
pub(crate) fn token_display_for(record: Option<&PiiRecord>, doc_ids: &[String], display_linked: bool) -> TokenDisplay {
    let Some(r) = record else { return TokenDisplay::NotAToken };
    let linked = display_linked
        || r.sources.iter().any(|s| s.source_kind == SourceKind::Document && doc_ids.contains(&s.source_id));
    if linked {
        TokenDisplay::Label(sovereign_ai::pii::resolve::preview_label_for_kind(&r.kind).to_string())
    } else {
        TokenDisplay::NotAToken
    }
}

/// What the save did with the display links.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct LinksAtSave {
    /// Records whose sources now include this document.
    pub(crate) linked: usize,
    /// Display links dropped because their token is no longer in the content.
    pub(crate) dropped: usize,
    /// Records not linked by the save (the copy's document does not link
    /// them, or the record is gone): kept as display links for the next save.
    pub(crate) failed: usize,
}

/// After a **successful** save of `saved` (spec ch. 09 / ch. 12, G14): the
/// service linked the pasted records whose token is in the saved content and
/// that the copied-from document links (`Edit::pasted_tokens`, C-DOC adoption
/// step 2 — the shell no longer writes `sources` itself). Here the display
/// links are settled: a token gone from the content is dropped, a record now
/// linked to this document is dropped (the store holds the link), any other is
/// kept for the next save.
pub(crate) async fn settle_display_links(
    docs: &DocService,
    doc_key: &str,
    saved: &str,
    pending: &mut HashMap<String, Option<String>>,
) -> LinksAtSave {
    let mut out = LinksAtSave::default();
    let ids: Vec<String> = pending.keys().cloned().collect();
    for id in ids {
        if !saved.contains(&format!("[pii:{id}]")) {
            pending.remove(&id);
            out.dropped += 1;
            continue;
        }
        match docs.token_status(doc_key, &id).await {
            Ok(DocTokenStatus::Token { .. }) | Ok(DocTokenStatus::Placeholder) => {
                pending.remove(&id);
                out.linked += 1;
            }
            _ => out.failed += 1,
        }
    }
    out
}

/// The pasted tokens a save carries: each display link whose copy came from
/// a known document.
pub(crate) fn pasted_tokens(links: &HashMap<String, Option<String>>) -> Vec<PastedToken> {
    links
        .iter()
        .filter_map(|(record_id, from)| {
            from.clone().map(|from_document| PastedToken { record_id: record_id.clone(), from_document })
        })
        .collect()
}

/// Display links made by pastes in one open editor, not yet saved: record id
/// → the document the copy came from (when the clipboard says).
pub(crate) type DisplayLinks = Rc<RefCell<HashMap<String, Option<String>>>>;

/// The editor's personal-data resolver for one open document.
pub(crate) struct ShellPii {
    db: Arc<dyn GraphDB>,
    rt: tokio::runtime::Handle,
    /// This document's ids: the canonical `table:key` first (what sources
    /// hold), then the canvas card id if it is spelled differently.
    doc_ids: Vec<String>,
    /// Pasted tokens shown as tokens here before any save: in memory only,
    /// scoped to this editor; carried by the next save (`Edit::pasted_tokens`).
    display_links: DisplayLinks,
    /// The shared clipboard, to learn which document a pasted copy came from.
    clipboard: Option<Rc<RefCell<ClipState>>>,
}

impl ShellPii {
    pub(crate) fn new(
        db: Arc<dyn GraphDB>,
        rt: tokio::runtime::Handle,
        doc_ids: Vec<String>,
        display_links: DisplayLinks,
    ) -> ShellPii {
        ShellPii { db, rt, doc_ids, display_links, clipboard: None }
    }

    /// Learn the source of pastes from `clipboard`.
    pub(crate) fn with_clipboard(mut self, clipboard: Rc<RefCell<ClipState>>) -> ShellPii {
        self.clipboard = Some(clipboard);
        self
    }
}

impl PiiDisplay for ShellPii {
    fn display(&self, record_id: &str) -> TokenDisplay {
        if !plausible_record_id(record_id) {
            return TokenDisplay::NotAToken;
        }
        let rec = self.rt.block_on(self.db.get_pii_record(record_id)).ok();
        let pending = self.display_links.borrow().contains_key(record_id);
        token_display_for(rec.as_ref(), &self.doc_ids, pending)
    }

    /// Display only: the pasted tokens show as tokens in this editor at once;
    /// nothing is written to the store here. A token pasted and deleted before
    /// the next save links nothing (see [`settle_display_links`]).
    fn link_to_document(&mut self, record_ids: &[String]) {
        let from = self.clipboard.as_ref().and_then(|c| c.borrow_mut().source_document());
        let mut links = self.display_links.borrow_mut();
        for id in record_ids {
            if plausible_record_id(id) {
                links.insert(id.clone(), from.clone());
            }
        }
    }
}

// ======================================================================== accessibility, window

struct BufferedA11y(Rc<RefCell<Vec<accesskit::TreeUpdate>>>);
impl AccessibilitySink for BufferedA11y {
    fn update(&mut self, update: accesskit::TreeUpdate) {
        self.0.borrow_mut().push(update);
    }
}

#[derive(Default)]
struct WinState {
    /// The caret area for the input method, in the editor's coordinates.
    ime_area: Option<sovereign_editor::Rect>,
}

struct ShellWindow(Rc<RefCell<WinState>>);
impl WindowHost for ShellWindow {
    fn set_ime_cursor_area(&mut self, area: sovereign_editor::Rect) {
        self.0.borrow_mut().ime_area = Some(area);
    }
    fn request_redraw(&mut self) {
        // The shell redraws every frame (ControlFlow::Poll + about_to_wait).
    }
}

/// A fresh random subtree id (UUID v4 bits) for one editor's AccessKit tree.
fn random_tree_id() -> accesskit::TreeId {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut bytes = [0u8; 16];
    for (i, chunk) in bytes.chunks_mut(8).enumerate() {
        let mut h = RandomState::new().build_hasher();
        h.write_usize(i);
        h.write_u128(mono_now().as_nanos());
        chunk.copy_from_slice(&h.finish().to_le_bytes());
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    accesskit::TreeId(accesskit::Uuid::from_bytes(bytes))
}

// ======================================================================== input routing

// In `crate::winit_input` (shared with the new-thread prompt).

// ======================================================================== saving through the document service

thread_local! {
    /// Editing sessions of closed editors, ended (`end_session`) at the next
    /// frame: (document, session).
    static ENDED: RefCell<Vec<(String, EditSession)>> = const { RefCell::new(Vec::new()) };
}

/// What an editor's saves are based on (C-DOC adoption step 2).
pub(crate) struct SaveState {
    pub(crate) session: EditSession,
    /// The revision the next save is based on: what this editor last read or
    /// saved. `None`: no document service when it opened (saves are refused).
    pub(crate) revision: Option<Revision>,
    /// The revision this session's last save produced.
    pub(crate) last_saved: Option<Revision>,
    /// Saves the user asked for (Ctrl+S, Save in the unsaved prompt): always a
    /// version.
    explicit: HashSet<SaveId>,
}

impl SaveState {
    pub(crate) fn new(revision: Option<Revision>) -> SaveState {
        SaveState {
            session: sovereign_ai::docs_host::new_edit_session(),
            revision,
            last_saved: None,
            explicit: HashSet::new(),
        }
    }
}

/// One editor save through the document service: the whole document in v2,
/// as the editor's session, based on the revision it last read or saved, with
/// the pasted tokens to link. On success the state moves to the new revision.
/// (The shell saves an editor's requests with [`save_request`], which carries
/// the title too; this body-only form is the tests'.)
#[cfg(test)]
pub(crate) async fn save_v2(
    docs: &DocService,
    doc_key: &str,
    st: &mut SaveState,
    content: &str,
    explicit: bool,
    pasted: Vec<PastedToken>,
) -> Result<sovereign_docs::SaveOutcome, DocError> {
    save_edit(docs, doc_key, st, None, content, explicit, pasted).await
}

/// One `Edit` of the editor's session: `title` (`None`: unchanged) and the
/// whole content in v2, based on the revision last read or saved.
async fn save_edit(
    docs: &DocService,
    doc_key: &str,
    st: &mut SaveState,
    title: Option<String>,
    content: &str,
    explicit: bool,
    pasted: Vec<PastedToken>,
) -> Result<sovereign_docs::SaveOutcome, DocError> {
    let base = st.revision.clone().ok_or_else(|| DocError::Store("this editor has no revision to save on".into()))?;
    let edit = Edit {
        base,
        title,
        content: Some(EditContent::V2(content.to_string())),
        origin: Origin::Editor { session: st.session },
        explicit,
        pasted_tokens: pasted,
    };
    let out = sovereign_ai::docs_host::editor_save(docs, doc_key, st.session, st.last_saved.as_ref(), edit).await?;
    st.revision = Some(out.revision.clone());
    st.last_saved = Some(out.revision.clone());
    Ok(out)
}

/// One editor save request through the document service (title contract T2,
/// `doc/plans/TITLE-edit.md`): its content and, when it carries one, its
/// title, in **one** `Edit`, as the editor's session, based on the revision it
/// last read or saved, with the pasted tokens to link. On success the state
/// moves to the new revision; a refused save (another writer first) is
/// `DocError::Conflict`, as for the body. Tests: `src/title_tests.rs`.
pub(crate) async fn save_request(
    docs: &DocService,
    doc_key: &str,
    st: &mut SaveState,
    request: &SaveRequest,
    explicit: bool,
    pasted: Vec<PastedToken>,
) -> Result<sovereign_docs::SaveOutcome, DocError> {
    save_edit(docs, doc_key, st, request.title.clone(), &request.content, explicit, pasted).await
}

/// What a conflict is reported as, in the editor's remote-change notice
/// ("This document was changed on …").
const OTHER_WRITER: &str = "this device or a paired one";

/// Development switch for manual check 10.2: while the file named by
/// `SHELL_FAIL_SAVES_WHILE` exists, the editor's saves fail as a store error
/// would. Compiled only into debug builds or with `dev-switches` (E6).
#[cfg(any(debug_assertions, feature = "dev-switches"))]
fn simulated_save_failure() -> bool {
    crate::dev::var("SHELL_FAIL_SAVES_WHILE").is_some_and(|f| std::path::Path::new(&f).exists())
}

/// A release build has no development switch: saves never fail on purpose.
#[cfg(not(any(debug_assertions, feature = "dev-switches")))]
fn simulated_save_failure() -> bool {
    false
}

// ======================================================================== the pane

/// What a document window holds: the editor and what the shell keeps for it.
pub(crate) struct EditorPane {
    pub(crate) editor: Editor,
    saves: Rc<RefCell<Vec<SaveRequest>>>,
    a11y_out: Rc<RefCell<Vec<accesskit::TreeUpdate>>>,
    win: Rc<RefCell<WinState>>,
    notes: Rc<RefCell<Vec<String>>>,
    /// Pasted tokens display-linked in this editor, linked in the store at save.
    pub(crate) display_links: DisplayLinks,
    /// What this editor's saves are based on (document service).
    pub(crate) save: SaveState,
    /// This document's ids (as `ShellPii` holds them), for the links at save.
    pub(crate) doc_ids: Vec<String>,
    pub(crate) tree_id: accesskit::TreeId,
    /// The editor's whole tree was sent (inlined in the shell's tree) while it
    /// was the top window and the adapter active.
    pub(crate) a11y_sent: bool,
    /// The editor tree's root and its last focus, in the editor's own ids.
    pub(crate) a11y_root: Option<accesskit::NodeId>,
    pub(crate) a11y_focus: Option<accesskit::NodeId>,
    pub(crate) focused: bool,
    scene: Scene,
    viewport: (f64, f64, f64),
    /// Where the editor sits, in the shell's logical coordinates.
    pub(crate) editor_rect: Rect,
    /// What the store is known to hold (opened content, then each saved one).
    pub(crate) stored: String,
    /// Saves sent to the store and not yet answered, with their content.
    in_flight: Vec<(SaveId, String)>,
    /// Close the window once everything is saved.
    pub(crate) close_after_save: bool,
    /// The user chose Discard in the unsaved prompt: unsaved edits do not
    /// block quitting any more.
    pub(crate) discarded: bool,
    /// A failed save was already announced in a modal (until a save succeeds).
    failure_announced: bool,
    /// The latest notice, shown in the status line.
    note: Option<String>,
    status: Option<(String, f64, Layout<Brush>)>,
    pointer_down: bool,
    last_press: Option<(Instant, f64, f64, u8)>,
}

impl EditorPane {
    /// Open the editor on a stored document. `content` is passed as stored:
    /// the editor reads the v1 envelope, raw text and v2.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn open(
        title: &str,
        content: String,
        trust: Trust,
        fonts: Fonts,
        pii: Box<dyn PiiDisplay>,
        notes: Rc<RefCell<Vec<String>>>,
        clipboard: Rc<RefCell<ClipState>>,
        display_links: DisplayLinks,
        doc_ids: Vec<String>,
    ) -> Result<EditorPane, String> {
        let saves = Rc::new(RefCell::new(Vec::new()));
        let a11y_out = Rc::new(RefCell::new(Vec::new()));
        let win = Rc::new(RefCell::new(WinState::default()));
        let tree_id = random_tree_id();
        let host = Host {
            documents: Box::new(QueuedSaves(saves.clone())),
            clipboard: Box::new(ShellClipboard {
                state: clipboard,
                doc: doc_ids.first().cloned().unwrap_or_default(),
            }),
            accessibility: Box::new(BufferedA11y(a11y_out.clone())),
            clock: Box::new(ShellClock),
            pii,
            window: Box::new(ShellWindow(win.clone())),
        };
        let doc = OpenDocument { title: title.to_string(), content: content.clone(), trust, mode: OpenMode::Edit };
        let editor = Editor::open(doc, EditorConfig { fonts, accessibility_tree: tree_id }, host)
            .map_err(|e| e.message)?;
        Ok(EditorPane {
            editor,
            saves,
            a11y_out,
            win,
            notes,
            display_links,
            save: SaveState::new(None),
            doc_ids,
            tree_id,
            a11y_sent: false,
            a11y_root: None,
            a11y_focus: None,
            focused: false,
            scene: Scene::new(),
            viewport: (0.0, 0.0, 0.0),
            editor_rect: Rect::ZERO,
            stored: content,
            in_flight: Vec::new(),
            close_after_save: false,
            discarded: false,
            failure_announced: false,
            note: None,
            status: None,
            pointer_down: false,
            last_press: None,
        })
    }

    /// A new document's window starts on its title: the title field has the
    /// caret, its text selected (title contract T3). Called by the shell's
    /// document creation.
    pub(crate) fn start_on_title(&mut self) {
        self.editor.focus_title();
    }

    /// Save requests queued and not taken yet.
    pub(crate) fn queued_saves(&self) -> usize {
        self.saves.borrow().len()
    }

    /// The saves queued since `from` (a count from [`queued_saves`]) were
    /// asked for by the user (Ctrl+S, Save in the unsaved prompt).
    ///
    /// [`queued_saves`]: Self::queued_saves
    pub(crate) fn mark_explicit_since(&mut self, from: usize) {
        for r in self.saves.borrow().iter().skip(from) {
            self.save.explicit.insert(r.id);
        }
    }

    /// The document's id for the service (the canonical `table:key`).
    pub(crate) fn doc_key(&self) -> String {
        self.doc_ids.first().cloned().unwrap_or_default()
    }

    /// Save requests the editor made since the last call.
    pub(crate) fn take_saves(&mut self) -> Vec<SaveRequest> {
        let reqs = std::mem::take(&mut *self.saves.borrow_mut());
        for r in &reqs {
            self.in_flight.push((r.id, r.content.clone()));
        }
        reqs
    }

    /// The store answered save `id`.
    pub(crate) fn save_done(&mut self, id: SaveId, result: Result<(), SaveError>) {
        if let Some(i) = self.in_flight.iter().position(|(s, _)| *s == id) {
            let (_, content) = self.in_flight.remove(i);
            if result.is_ok() {
                self.stored = content;
                self.failure_announced = false;
            }
        }
        self.editor.save_finished(id, result);
    }

    /// Whether `content` is something this window wrote (stored or in flight).
    fn wrote(&self, content: &str) -> bool {
        self.stored == content || self.in_flight.iter().any(|(_, c)| c == content)
    }

    pub(crate) fn has_unsaved(&self) -> bool {
        !self.discarded && self.editor.has_unsaved_changes()
    }

    pub(crate) fn set_focus(&mut self, focused: bool) {
        if self.focused != focused {
            self.focused = focused;
            self.editor.handle_event(InputEvent::Focus(focused));
            if !focused {
                self.pointer_down = false;
            }
        }
    }

    /// Notices since the last call → the status line. Returns a save-failure
    /// message the first time a save fails (for a modal), and anything else
    /// the shell must say.
    fn collect_notices(&mut self) -> Option<String> {
        let mut failure = None;
        for n in self.editor.take_notices() {
            if n.kind == NoticeKind::SaveFailed && !self.failure_announced {
                self.failure_announced = true;
                failure = Some(n.message.clone());
            }
            self.note = Some(n.message);
        }
        for n in self.notes.borrow_mut().drain(..) {
            self.note = Some(n);
        }
        failure
    }

    /// The status line (Saved / Saving / Save failed / read-only / notes).
    pub(crate) fn status_text(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        parts.push(match self.editor.save_status() {
            SaveStatus::Saved => "Saved".into(),
            SaveStatus::Unsaved => "Unsaved changes".into(),
            SaveStatus::Saving => "Saving\u{2026}".into(),
            SaveStatus::Failed { message } => format!("Save failed ({message}) \u{2014} your text is kept"),
        });
        if let Some(r) = self.editor.read_only_reason() {
            parts.push(read_only_text(&r));
        }
        if self.close_after_save {
            parts.push("closes once saved".into());
        }
        if let Some(n) = &self.note {
            parts.push(n.clone());
        }
        parts.join("  \u{00b7}  ")
    }

    /// The status strip and the editor area inside a window body.
    fn split(body: Rect) -> (Rect, Rect) {
        let status = Rect::new(body.x0, body.y0, body.x1, (body.y0 + STATUS_STRIP_H).min(body.y1));
        let ed = Rect::new(body.x0, status.y1 + 2.0, body.x1, body.y1.max(status.y1 + 2.0));
        (status, ed)
    }

    /// Paint the status line and the editor into the window body.
    pub(crate) fn draw(&mut self, scene: &mut Scene, shaper: &mut TextShaper, bounds: Rect, scale: f64) {
        let body = window_chrome(bounds).2;
        let (strip, ed) = EditorPane::split(body);
        self.editor_rect = ed;
        let w = strip.width().max(40.0);
        let status = self.status_text();
        let fresh = !matches!(&self.status, Some((s, sw, _)) if *s == status && (*sw - w).abs() < 0.5);
        if fresh {
            let l = shaper.shape(&status, 4000.0, 12.0);
            self.status = Some((status, w, l));
        }
        // The status line, in the theme's body text on the window's panel
        // (the document body's colours, so it reads in either theme).
        scene.push_layer(Fill::NonZero, vello::peniko::Mix::Normal, 1.0, Affine::IDENTITY, &strip);
        if let Some((_, _, l)) = &self.status {
            draw_text(scene, l, Affine::translate((strip.x0 + 6.0, strip.y0 + 3.0)), pal().text_body);
        }
        scene.pop_layer();

        let vp = (ed.width(), ed.height(), scale);
        if vp != self.viewport {
            self.viewport = vp;
            self.editor.set_viewport(vp.0, vp.1, vp.2);
        }
        // The theme's colours, every frame: a theme switch shows at once
        // (`set_palette` does nothing when they did not change).
        self.editor.set_palette(editor_palette(&pal()));
        self.scene.reset();
        self.editor.paint(&mut self.scene);
        scene.push_layer(Fill::NonZero, vello::peniko::Mix::Normal, 1.0, Affine::IDENTITY, &ed);
        scene.append(&self.scene, Some(Affine::translate((ed.x0, ed.y0))));
        scene.pop_layer();
        // Keep the theme's border around the editing area.
        scene.stroke(&vello::kurbo::Stroke::new(1.0), Affine::IDENTITY, pal().input_border, None, &ed);
    }

    /// Updates for the editor's AccessKit subtree since the last call.
    fn take_a11y(&mut self) -> Vec<accesskit::TreeUpdate> {
        std::mem::take(&mut *self.a11y_out.borrow_mut())
    }

    /// The input-method caret area in physical pixels, from the editor's last
    /// report, placed where the editor is drawn now.
    fn ime_area_physical(&self, total: f64) -> Option<(i32, i32, u32, u32)> {
        let a = self.win.borrow().ime_area?;
        let x = ((self.editor_rect.x0 + a.x0) * total).round() as i32;
        let y = ((self.editor_rect.y0 + a.y0) * total).round() as i32;
        let w = (((a.x1 - a.x0) * total).round() as u32).max(1);
        let h = (((a.y1 - a.y0) * total).round() as u32).max(1);
        Some((x, y, w, h))
    }

    fn to_editor(&self, p: Point) -> (f64, f64) {
        (p.x - self.editor_rect.x0, p.y - self.editor_rect.y0)
    }
}

/// Lines of the wheel → logical pixels, as the shell's other windows scroll.
const WHEEL_PX_PER_LINE: f64 = 48.0;

fn read_only_text(r: &ReadOnlyReason) -> String {
    match r {
        ReadOnlyReason::OpenedReadOnly => "Read only".into(),
        ReadOnlyReason::NewerFormat { v } => format!("Read only: saved by a newer version of Sovereign (format v{v})"),
        ReadOnlyReason::TooLarge { text_bytes } => {
            format!("Read only: {text_bytes} bytes of text, more than the 1 MB the editor edits")
        }
        ReadOnlyReason::EditingElsewhere { device } => format!("Read only: this document is being edited on {device}"),
    }
}

/// A closed editor's session ends (its last saved state becomes a version):
/// queued here, run by the next frame ([`App::v2_end_sessions`]).
impl Drop for EditorPane {
    fn drop(&mut self) {
        if self.save.last_saved.is_some() {
            let key = self.doc_key();
            ENDED.with(|e| e.borrow_mut().push((key, self.save.session)));
        }
    }
}

// ======================================================================== App glue

impl App {
    /// The window index whose editor should have keyboard focus: the top
    /// window, if it is a document window, the workspace is unlocked, no
    /// modal or menu is open and the OS window has focus.
    fn v2_focus_target(&self) -> Option<usize> {
        let top = self.windows.len().checked_sub(1)?;
        let is_v2 = matches!(&self.windows[top].content, WindowContent::Doc(d) if d.v2.is_some());
        let modal = self.unsaved_prompt.is_some()
            || self.comms_form.is_some()
            || self.compose_form.is_some()
            || self.pairing_modal.is_some()
            || self.guardian_modal.is_some()
            || self.recovery_wizard.is_some()
            || self.injection_prompt.is_some()
            || self.bubble_picker
            || self.pending_action.is_some()
            || self.notice.is_some()
            || self.ctx_menu.is_some()
            || self.new_thread_prompt.is_some();
        (is_v2 && !modal && !self.locked && WINDOW_FOCUSED.with(|c| c.get())).then_some(top)
    }

    fn v2_pane(&mut self, wi: usize) -> Option<&mut EditorPane> {
        match &mut self.windows.get_mut(wi)?.content {
            WindowContent::Doc(d) => d.v2.as_deref_mut(),
            _ => None,
        }
    }

    /// Open the editor for a document window. `None` — with a notice saying
    /// why, in plain words — means no window opens (E3): there is no other
    /// view of a document. The notice never quotes what is stored (JSON or
    /// ciphertext) and never names a store, a persona or a store error's text.
    pub(crate) fn v2_open(&mut self, doc_id: &str, title: &str) -> Option<Box<EditorPane>> {
        const NOTICE_TITLE: &str = "Document";
        let Some(db) = self.db.clone() else {
            self.set_notice(NOTICE_TITLE, "No workspace database is open, so the document can't be shown.");
            return None;
        };
        // The revision saves are based on, read before the content: a write in
        // between makes the first save a conflict, never a silent overwrite.
        let revision = self.docs.clone().and_then(|d| self.rt.block_on(d.read(doc_id)).ok().map(|v| v.revision));
        let doc = match self.rt.block_on(db.get_document(doc_id)) {
            Ok(d) => d,
            Err(e) => {
                // The store's own error stays in the log: it may carry ids or
                // a cause the notice should not show.
                eprintln!("sovereign-shell: a document couldn't be read for the editor ({e})");
                self.set_notice(NOTICE_TITLE, "This document couldn't be read, so it can't be shown.");
                return None;
            }
        };
        if doc.encryption_nonce.is_some() {
            // No-auth mode: the content is ciphertext. Never show or edit that.
            self.set_notice(NOTICE_TITLE, "This document is encrypted and this session can't decrypt it. Log in to open it.");
            return None;
        }
        let mut ids = vec![doc.id_string().unwrap_or_else(|| doc_id.to_string())];
        if ids[0] != doc_id {
            ids.push(doc_id.to_string());
        }
        let notes = Rc::new(RefCell::new(Vec::new()));
        let links: DisplayLinks = Rc::default();
        let clipboard = CLIPBOARD.with(|c| c.clone());
        let pii = ShellPii::new(db, self.rt.handle().clone(), ids.clone(), links.clone()).with_clipboard(clipboard.clone());
        let trust = if doc.is_owned { Trust::Owned } else { Trust::External };
        match EditorPane::open(title, doc.content, trust, Fonts::System, Box::new(pii), notes, clipboard, links, ids) {
            Ok(mut p) => {
                p.save = SaveState::new(revision);
                Some(Box::new(p))
            }
            Err(e) => {
                // The editor's refusal: its reader's message, which never
                // contains document text (`sovereign_docformat::FormatError`).
                self.set_notice(NOTICE_TITLE, &format!("This document can't be shown: {e}."));
                None
            }
        }
    }

    /// Re-open the editor of every v2 window on `doc_id` from the store (after
    /// a skill or a restore rewrote it; those refuse while edits are unsaved).
    pub(crate) fn v2_reopen(&mut self, doc_id: &str) {
        for wi in 0..self.windows.len() {
            let title = match &self.windows[wi].content {
                WindowContent::Doc(d) if d.doc_id == doc_id && d.v2.is_some() => {
                    self.cards.iter().find(|c| c.id == doc_id).map(|c| c.title.clone()).unwrap_or_default()
                }
                _ => continue,
            };
            let fresh = self.v2_open(doc_id, &title);
            if let WindowContent::Doc(d) = &mut self.windows[wi].content {
                if fresh.is_some() {
                    d.v2 = fresh;
                }
            }
        }
    }

    /// Per frame, before drawing: run queued saves through the shell's save
    /// path, let editors autosave, move focus, surface notices, close windows
    /// waiting for their save, run the clipboard clear timer.
    pub(crate) fn v2_frame(&mut self) {
        self.v2_end_sessions(false);
        CLIPBOARD.with(|c| c.borrow_mut().tick(mono_now()));
        if let Some(e) = CLIPBOARD.with(|c| c.borrow_mut().error.take()) {
            self.set_notice("Clipboard", &e);
        }
        let target = self.v2_focus_target();
        let mut close = Vec::new();
        let mut ask = Vec::new();
        let mut modal = Vec::new();
        for wi in 0..self.windows.len() {
            let (doc_id, reqs) = match &mut self.windows[wi].content {
                WindowContent::Doc(d) => match d.v2.as_mut() {
                    Some(p) => (d.doc_id.clone(), p.take_saves()),
                    None => continue,
                },
                _ => continue,
            };
            let mut saved_any = false;
            for r in reqs {
                let result = if simulated_save_failure() {
                    Err(SaveError { message: "simulated failure (a development switch)".into() })
                } else {
                    self.v2_save_one(wi, &r)
                };
                let stored = match &result {
                    Ok((stored, title)) => {
                        saved_any = true;
                        if let Some(c) = self.cards.iter_mut().find(|c| c.id == doc_id) {
                            c.set_stored(stored.clone());
                            // The card shows the saved title (title contract T2).
                            if let Some(t) = title {
                                c.set_title(t, &mut self.shaper);
                            }
                        }
                        // And so does the window's title bar.
                        if let (Some(t), WindowContent::Doc(d)) = (title, &mut self.windows[wi].content) {
                            d.set_title(t, &mut self.shaper);
                        }
                        Some(stored.clone())
                    }
                    Err(_) => None,
                };
                if let Some(p) = self.v2_pane(wi) {
                    p.save_done(r.id, result.map(|_| ()));
                    // What the store holds now (personal data replaced by
                    // tokens), so a sync's comparison sees this save as ours.
                    if let Some(stored) = stored {
                        p.stored = stored;
                    }
                }
            }
            if saved_any {
                self.refresh_history_for(&doc_id);
            }
            let Some(p) = self.v2_pane(wi) else { continue };
            p.editor.poll();
            p.set_focus(target == Some(wi));
            if let Some(msg) = p.collect_notices() {
                modal.push(msg);
            }
            if p.close_after_save {
                match p.editor.save_status() {
                    SaveStatus::Saved if !p.editor.has_unsaved_changes() => close.push(wi),
                    SaveStatus::Failed { .. } => {
                        p.close_after_save = false;
                        ask.push(wi);
                    }
                    _ => {}
                }
            }
        }
        for msg in modal {
            self.set_notice("Not saved", &msg);
        }
        if let Some(&wi) = ask.first() {
            if self.unsaved_prompt.is_none() {
                self.ask_unsaved(wi, super::AfterUnsaved::CloseWindow);
            }
        }
        for wi in close.into_iter().rev() {
            self.windows.remove(wi);
        }
    }

    /// Run one of window `wi`'s save requests through the document service:
    /// its content and, when it changed, its title, in one `Edit` (title
    /// contract T2). Returns what the store holds after it: the content, and
    /// the title when the request carried one.
    ///
    /// A save refused because another writer saved first (`Conflict`, G11) goes
    /// into the editor's remote-change flow (C-DOC weakness 9): the editor keeps
    /// its text and says the document changed; then its text is saved on the
    /// current revision — the other state stays in History (the service versions
    /// a state another writer made before replacing it; a state a direct writer
    /// left under this session's name is versioned by `editor_save`).
    fn v2_save_one(&mut self, wi: usize, r: &SaveRequest) -> Result<(String, Option<String>), SaveError> {
        let Some(docs) = self.docs.clone() else {
            return Err(SaveError { message: "the workspace is locked (no document service): log in to save".into() });
        };
        let db = self.db.clone();
        let rt = self.rt.handle().clone();
        let Some(p) = self.v2_pane(wi) else { return Err(SaveError { message: "the window is gone".into() }) };
        let key = p.doc_key();
        let explicit = p.save.explicit.remove(&r.id);
        let pasted = pasted_tokens(&p.display_links.borrow());
        let mut out = rt.block_on(save_request(&docs, &key, &mut p.save, r, explicit, pasted.clone()));
        if let Err(DocError::Conflict { current, .. }) = &out {
            let current = current.clone();
            let theirs = db.as_ref().and_then(|db| rt.block_on(db.get_document(&key)).ok()).map(|d| d.content);
            if let Some(content) = theirs {
                p.editor.remote_change(RemoteChange { content, device: OTHER_WRITER.into() });
            }
            p.save.revision = Some(current);
            out = rt.block_on(save_request(&docs, &key, &mut p.save, r, explicit, pasted));
        }
        let out = out.map_err(|e| SaveError { message: sovereign_ai::docs_host::doc_error_message(&e) })?;
        if let sovereign_docs::PiiOutcome::Failed { message } = &out.pii {
            p.note = Some(format!("Saved, but personal data in it wasn't replaced by tokens ({message})"));
        }
        // Pasted tokens: the service linked those still in what was saved;
        // settle the display links, and let the editor re-ask how its tokens show.
        if !p.display_links.borrow().is_empty() {
            let mut map = p.display_links.borrow().clone();
            let links = rt.block_on(settle_display_links(&docs, &key, &r.content, &mut map));
            *p.display_links.borrow_mut() = map;
            if links.dropped > 0 || links.linked > 0 {
                p.editor.pii_display_changed();
            }
            if links.failed > 0 {
                p.note = Some(format!(
                    "{} pasted personal-data token(s) could not be linked to this document yet; retried at the next save.",
                    links.failed
                ));
            }
        }
        let stored = db.and_then(|db| rt.block_on(db.get_document(&key)).ok());
        // The service may have replaced personal data in the title by a
        // token: the card shows what was stored.
        let title = r.title.as_ref().map(|t| stored.as_ref().map(|d| d.title.clone()).unwrap_or_else(|| t.clone()));
        Ok((stored.map(|d| d.content).unwrap_or_else(|| r.content.clone()), title))
    }

    /// End editing sessions (`end_session`): those of editors closed since the
    /// last frame, and — when `all` (the app quits) — those of every open
    /// editor. What each saved last becomes a version (G5).
    pub(crate) fn v2_end_sessions(&mut self, all: bool) {
        let mut ended: Vec<(String, EditSession)> = ENDED.with(|e| std::mem::take(&mut *e.borrow_mut()));
        if all {
            for w in &self.windows {
                if let WindowContent::Doc(d) = &w.content {
                    if let Some(p) = d.v2.as_deref() {
                        if p.save.last_saved.is_some() {
                            ended.push((p.doc_key(), p.save.session));
                        }
                    }
                }
            }
        }
        let Some(docs) = self.docs.clone() else { return };
        for (doc, session) in ended {
            if let Err(e) = self.rt.block_on(docs.end_session(&doc, session)) {
                eprintln!("sovereign-shell: an editing session's last version couldn't be made ({e})");
            }
        }
    }

    /// The store is about to be replaced (contract E9; called at the top of
    /// `install_document_service`, before the incoming service is installed).
    /// Every window that shows a document of the outgoing store closes
    /// ([`shows_store_documents`]: document windows, History, and the lists
    /// of its documents — E9 extended); the editing sessions of the document
    /// windows end **now**, on the outgoing service (`self.docs` is still
    /// it), so none is left for a later frame to end on the incoming store;
    /// the unsaved prompt (it can only be about one of them) goes; the
    /// clipboard forgets this process's copy (`ClipState::forget`). No prompt
    /// is shown: unsaved edits are dropped as a lock drops them, and the
    /// screen afterwards is the same whatever the windows held (the same in
    /// every persona). The first unlock — no store yet, no window — changes
    /// nothing.
    pub(crate) fn v2_leave_store(&mut self) {
        let has_store_windows = self.windows.iter().any(|w| shows_store_documents(&w.content));
        if self.docs.is_none() && !has_store_windows {
            return;
        }
        // Dropping a pane queues its session (`impl Drop for EditorPane`).
        self.windows.retain(|w| !shows_store_documents(&w.content));
        self.unsaved_prompt = None;
        // What else names the outgoing store's documents: a menu or a fanned
        // deck on its cards, a card being dragged, the trash and thread
        // chooser dialogs, and the delete notice (its Undo would act on the
        // incoming store with the outgoing store's id).
        self.ctx_menu = None;
        self.expanded_deck = None;
        self.card_drag = None;
        self.trash_modal = None;
        self.orphan_modal = None;
        self.delete_notice = None;
        self.v2_end_sessions(false);
        CLIPBOARD.with(|c| c.borrow_mut().forget());
    }

    /// Draw the editor of window `i` (its chrome is drawn by `draw_doc_window`).
    pub(crate) fn v2_draw(&mut self, i: usize) {
        let total = self.scale * self.ui_scale;
        let bounds = self.windows[i].bounds;
        if let WindowContent::Doc(d) = &mut self.windows[i].content {
            if let Some(p) = d.v2.as_mut() {
                p.draw(&mut self.scene, &mut self.shaper, bounds, total);
            }
        }
    }

    /// After drawing: what the input method should follow for the focused
    /// editor — allowed while it has focus (and is editable), its candidate
    /// window at the caret (physical pixels). The shell applies it
    /// (`App::apply_ime`, shared with the new-thread prompt).
    pub(crate) fn v2_ime_request(&self) -> (bool, Option<(i32, i32, u32, u32)>) {
        let total = self.scale * self.ui_scale;
        let target = self.v2_focus_target();
        match target.and_then(|wi| match &self.windows[wi].content {
            WindowContent::Doc(d) => d.v2.as_deref(),
            _ => None,
        }) {
            Some(p) if !p.editor.is_read_only() => (true, p.ime_area_physical(total)),
            _ => (false, None),
        }
    }

    /// A key press, when the focused top window is an editor. Returns whether
    /// it was consumed here (the editor used it, or Esc closed the window).
    pub(crate) fn v2_key(&mut self, key: &WKey, text: Option<&str>) -> bool {
        let Some(wi) = self.v2_focus_target() else { return false };
        let mods = editor_mods();
        let r = match self.v2_pane(wi) {
            Some(p) => {
                // A save queued by a key (Ctrl+S) is one the user asked for.
                let n = p.queued_saves();
                let r = route_key(&mut p.editor, key, text, mods);
                p.mark_explicit_since(n);
                r
            }
            None => return false,
        };
        if r == EventResult::Handled {
            return true;
        }
        if matches!(key, WKey::Named(WNamed::Escape)) {
            self.v2_request_close(wi);
        }
        // Tab / F6 / other keys the editor left: nothing else in the shell
        // takes them while a document has focus.
        true
    }

    /// Ctrl+S (or another shell shortcut) as an editor command, when an editor
    /// has focus.
    pub(crate) fn v2_command(&mut self, cmd: Command) -> bool {
        let Some(wi) = self.v2_focus_target() else { return false };
        match self.v2_pane(wi) {
            Some(p) => {
                let n = p.queued_saves();
                p.editor.command(cmd);
                p.mark_explicit_since(n);
                true
            }
            None => false,
        }
    }

    pub(crate) fn v2_ime(&mut self, ime: winit::event::Ime) {
        let Some(wi) = self.v2_focus_target() else { return };
        if let Some(p) = self.v2_pane(wi) {
            p.editor.handle_event(InputEvent::Ime(map_ime(ime)));
        }
    }

    /// A left press at `p` (logical). Called after the window was brought to
    /// the front; forwards it if it landed on the editor area.
    pub(crate) fn v2_pointer_down(&mut self, p: Point) {
        let Some(wi) = self.windows.len().checked_sub(1) else { return };
        let mods = editor_mods();
        // Focus moves to the editor this frame (the press brought it to front).
        let focusable = self.v2_focus_target() == Some(wi);
        let Some(pane) = self.v2_pane(wi) else { return };
        if !pane.editor_rect.contains(p) {
            return;
        }
        if focusable {
            pane.set_focus(true);
        }
        let now = Instant::now();
        let (x, y) = pane.to_editor(p);
        let n = click_count(pane.last_press, now, x, y);
        pane.last_press = Some((now, x, y, n));
        pane.pointer_down = true;
        pane.editor.handle_event(InputEvent::Pointer(PointerEvent::Down {
            x,
            y,
            button: PointerButton::Primary,
            click_count: n,
            modifiers: mods,
        }));
    }

    pub(crate) fn v2_pointer_move(&mut self, p: Point) {
        let Some(wi) = self.windows.len().checked_sub(1) else { return };
        let Some(pane) = self.v2_pane(wi) else { return };
        if pane.pointer_down {
            let (x, y) = pane.to_editor(p);
            pane.editor.handle_event(InputEvent::Pointer(PointerEvent::Move { x, y }));
        }
    }

    /// The mouse wheel over window `wi` (`lines`: winit's vertical lines,
    /// positive = wheel up). Over an editor's text area the plain wheel
    /// scrolls the text (spec ch. 12: the wheel scrolls; inside the editor it
    /// scrolls the text, not the window or the canvas). Returns whether the
    /// window holds an editor — its wheel is then the editor's, even over
    /// the status line, where it does nothing.
    pub(crate) fn v2_wheel(&mut self, wi: usize, p: Point, lines: f64) -> bool {
        let Some(pane) = self.v2_pane(wi) else { return false };
        if pane.editor_rect.contains(p) {
            pane.editor.scroll_by(-lines * WHEEL_PX_PER_LINE);
        }
        true
    }

    pub(crate) fn v2_pointer_up(&mut self, p: Point) {
        for w in &mut self.windows {
            if let WindowContent::Doc(d) = &mut w.content {
                if let Some(pane) = d.v2.as_mut() {
                    if pane.pointer_down {
                        pane.pointer_down = false;
                        let (x, y) = pane.to_editor(p);
                        pane.editor.handle_event(InputEvent::Pointer(PointerEvent::Up {
                            x,
                            y,
                            button: PointerButton::Primary,
                        }));
                    }
                }
            }
        }
    }

    /// × / Esc on an editor window. The editor decides: nothing unsaved →
    /// close; unsaved and no failed save → it saves, the window closes after;
    /// a failed save → today's unsaved prompt. Returns whether `wi` is v2.
    pub(crate) fn v2_request_close(&mut self, wi: usize) -> bool {
        let Some(p) = self.v2_pane(wi) else { return false };
        match p.editor.request_close() {
            CloseRequest::Ready => {
                self.windows.remove(wi);
            }
            CloseRequest::Saving => p.close_after_save = true,
            CloseRequest::ConfirmUnsaved => self.ask_unsaved(wi, super::AfterUnsaved::CloseWindow),
        }
        true
    }

    /// Quitting: editors save what they hold first. `Some(false)` when the
    /// quit must wait (a save in flight) or ask (a failed save).
    pub(crate) fn v2_quit_check(&mut self) -> Option<bool> {
        let mut waiting = false;
        for wi in 0..self.windows.len() {
            let Some(p) = self.v2_pane(wi) else { continue };
            if p.discarded {
                continue;
            }
            match p.editor.request_close() {
                CloseRequest::Ready => {}
                CloseRequest::Saving => waiting = true,
                CloseRequest::ConfirmUnsaved => {
                    self.ask_unsaved(wi, super::AfterUnsaved::Quit);
                    return Some(false);
                }
            }
        }
        if waiting {
            self.quit_requested = true;
            return Some(false);
        }
        None
    }

    /// The unsaved prompt's answer, for an editor window. Returns whether the
    /// prompt was about one (and was handled here).
    pub(crate) fn v2_resolve_unsaved(&mut self, doc_id: &str, then: super::AfterUnsaved, save: bool) -> bool {
        let Some(wi) = self.doc_window_index(doc_id) else { return false };
        let Some(p) = self.v2_pane(wi) else { return false };
        if save {
            // A new save of the kept text; the window closes (or the app
            // quits) once it succeeds, or the prompt comes back if it fails.
            let n = p.queued_saves();
            p.editor.command(Command::Save);
            p.mark_explicit_since(n);
            match then {
                super::AfterUnsaved::CloseWindow => p.close_after_save = true,
                super::AfterUnsaved::Quit => self.quit_requested = true,
            }
        } else {
            p.discarded = true;
            match then {
                super::AfterUnsaved::CloseWindow => {
                    self.windows.remove(wi);
                }
                super::AfterUnsaved::Quit => self.quit_requested = true,
            }
        }
        true
    }

    /// A sync with `device` completed: every open editor whose document
    /// changed in the store (and not by this window) is told.
    ///
    /// TODO(spec ch. 12, *One device edits a document at a time*; ch. 05): the
    /// "editing on <device>" signal between paired devices does not exist yet,
    /// so `Editor::editing_moved_to` and the three-choice warning are not wired.
    /// No protocol is invented here.
    pub(crate) fn v2_after_sync(&mut self, device: &str) {
        let Some(db) = self.db.clone() else { return };
        for wi in 0..self.windows.len() {
            let doc_id = match &self.windows[wi].content {
                WindowContent::Doc(d) if d.v2.is_some() => d.doc_id.clone(),
                _ => continue,
            };
            let Ok(doc) = self.rt.block_on(db.get_document(&doc_id)) else { continue };
            if doc.encryption_nonce.is_some() {
                continue;
            }
            let revision = self.docs.clone().and_then(|d| self.rt.block_on(d.read(&doc_id)).ok().map(|v| v.revision));
            if let Some(p) = self.v2_pane(wi) {
                if !p.wrote(&doc.content) {
                    p.stored = doc.content.clone();
                    p.editor.remote_change(RemoteChange { content: doc.content, device: device.to_string() });
                    // The editor took the new version (it had nothing of its
                    // own): its next save is based on it. Otherwise it keeps
                    // its base, and its next save is a conflict (see
                    // `v2_save_one`).
                    if !p.editor.has_unsaved_changes() {
                        if let Some(r) = revision {
                            p.save.revision = Some(r);
                        }
                    }
                }
            }
        }
    }

    /// Push the shell's accessibility tree, then the top editor's nodes.
    ///
    /// The editor's tree is **inlined** in the shell's tree (its ids moved to
    /// their own range, [`map_node`]), not grafted as an AccessKit subtree:
    /// with a graft, AccessKit (0.35 to 0.37) re-applies the effective focus,
    /// which sits inside the subtree, to the root tree when the OS window gains
    /// or loses focus, and panics ("Focused ID #2 is not in the node list").
    /// Found in the first manual run, 2026-10-11. Inlined, focus never leaves
    /// the root tree. The first time (or after the adapter became active) the
    /// whole editor tree is sent, its incremental updates after that, each as
    /// a second root-tree update after the shell's own.
    pub(crate) fn v2_push_a11y(&mut self, root: accesskit::TreeUpdate) {
        let root_focus = root.focus;
        let ran = Cell::new(false);
        if let Some(adapter) = self.adapter.as_mut() {
            adapter.update_if_active(|| {
                ran.set(true);
                root
            });
        }
        let out = self.v2_a11y_updates(ran.get(), root_focus);
        if let Some(adapter) = self.adapter.as_mut() {
            for u in out {
                adapter.update_if_active(|| u);
            }
        }
    }

    /// The editor's updates that follow the shell's tree in a frame (see
    /// [`v2_push_a11y`](Self::v2_push_a11y)): `active`, whether the shell's
    /// update reached an active adapter; `root_focus`, that update's focus.
    /// Not active: nothing is sent, and the next active frame sends the
    /// editor's whole tree again.
    pub(crate) fn v2_a11y_updates(&mut self, active: bool, root_focus: accesskit::NodeId) -> Vec<accesskit::TreeUpdate> {
        let top = self.v2_a11y_slot().map(|(_, _, wi)| wi);
        let mut out = Vec::new();
        for (i, w) in self.windows.iter_mut().enumerate() {
            let WindowContent::Doc(d) = &mut w.content else { continue };
            let Some(p) = d.v2.as_mut() else { continue };
            let pending = p.take_a11y();
            if !active || top != Some(i) {
                p.a11y_sent = false;
                continue;
            }
            let mut nodes = Vec::new();
            if p.a11y_sent {
                for u in pending {
                    p.a11y_focus = Some(u.focus);
                    nodes.extend(remap_nodes(u.nodes));
                }
            } else {
                let full = p.editor.accessibility_tree();
                let root_local = full.tree.as_ref().map(|t| t.root).unwrap_or(full.focus);
                p.a11y_root = Some(root_local);
                p.a11y_focus = Some(full.focus);
                nodes.push((EDITOR_CONTAINER, container_node(Some(map_node(root_local)))));
                nodes.extend(remap_nodes(full.nodes));
                p.a11y_sent = true;
            }
            if nodes.is_empty() {
                continue;
            }
            let focus = match (p.focused, p.a11y_focus) {
                (true, Some(f)) => map_node(f),
                _ => root_focus,
            };
            out.push(accesskit::TreeUpdate { nodes, tree: None, tree_id: accesskit::TreeId::ROOT, focus });
        }
        out
    }

    /// The top window's editor, for the shell's tree: the editor root to hang
    /// under the container and the editor node holding focus (both only once
    /// the editor's nodes were sent), and the window index.
    pub(crate) fn v2_a11y_slot(
        &self,
    ) -> Option<(Option<accesskit::NodeId>, Option<accesskit::NodeId>, usize)> {
        if self.locked {
            return None;
        }
        let top = self.windows.len().checked_sub(1)?;
        match &self.windows[top].content {
            WindowContent::Doc(d) => d.v2.as_ref().map(|p| {
                let child = if p.a11y_sent { p.a11y_root.map(map_node) } else { None };
                let focus =
                    if p.a11y_sent && p.focused { p.a11y_focus.map(map_node) } else { None };
                (child, focus, top)
            }),
            _ => None,
        }
    }
}

/// Whether a window shows documents of the store it was opened on (E9: it
/// closes when that store is replaced): a document (its editor), its History
/// (versions and previews), and the windows listing documents or their
/// titles — Search results, Alerts (with the section of documents whose
/// thread was deleted), Previous values, and Settings (its Trash).
fn shows_store_documents(content: &WindowContent) -> bool {
    matches!(
        content,
        WindowContent::Doc(_)
            | WindowContent::History(_)
            | WindowContent::Search(_)
            | WindowContent::Alerts(_)
            | WindowContent::PreviousValues(_)
            | WindowContent::Settings(_)
    )
}

/// The container node for the editor in the shell's tree: far from the
/// per-frame item ids (1, 2, ...), so it is stable across frames.
pub(crate) const EDITOR_CONTAINER: accesskit::NodeId = accesskit::NodeId(0x5EED_0000);

/// The id range of the editor's nodes inside the shell's tree.
const EDITOR_NS: u64 = 1 << 62;

/// An editor node id moved into the shell's tree.
pub(crate) fn map_node(id: accesskit::NodeId) -> accesskit::NodeId {
    accesskit::NodeId(EDITOR_NS | id.0)
}

/// The node that holds the editor's tree in the shell's tree.
pub(crate) fn container_node(child: Option<accesskit::NodeId>) -> accesskit::Node {
    let mut n = accesskit::Node::new(accesskit::Role::GenericContainer);
    n.set_label("Document editor");
    if let Some(c) = child {
        n.set_children(vec![c]);
    }
    n
}

/// The editor's nodes with every node reference moved into the shell's range
/// (ids, children, text selection: the references the editor's tree uses).
fn remap_nodes(
    nodes: Vec<(accesskit::NodeId, accesskit::Node)>,
) -> Vec<(accesskit::NodeId, accesskit::Node)> {
    nodes
        .into_iter()
        .map(|(id, mut n)| {
            let kids: Vec<_> = n.children().iter().copied().map(map_node).collect();
            if !kids.is_empty() {
                n.set_children(kids);
            }
            if let Some(sel) = n.text_selection().copied() {
                n.set_text_selection(accesskit::TextSelection {
                    anchor: accesskit::TextPosition { node: map_node(sel.anchor.node), ..sel.anchor },
                    focus: accesskit::TextPosition { node: map_node(sel.focus.node), ..sel.focus },
                });
            }
            (map_node(id), n)
        })
        .collect()
}

// ============================================================ dev: token seed

// Compiled only into debug builds or with `dev-switches` (E6): a release
// build has neither the switch nor the seed.
#[cfg(any(debug_assertions, feature = "dev-switches"))]
impl App {
    /// Development switch for manual check 8 (`SHELL_SEED_PII_TEST=1`, debug /
    /// dev-switches builds only): after unlock, creates — once per workspace —
    /// two documents through the document service: "Token test A", written
    /// with an e-mail address that the service's personal-data ingest turns
    /// into a record of A and a token in A's text, and "Token test B", to paste
    /// into (C-DOC adoption step 3: the token comes from the service, nothing
    /// is written around it).
    pub(crate) fn v2_seed_pii_test(&mut self) {
        if !crate::dev::flag("SHELL_SEED_PII_TEST") {
            return;
        }
        // The ingest needs the AccountKey; without it the address would stay raw.
        let (Some(db), Some(docs), Some(_)) = (self.db.clone(), self.docs.clone(), self.account_key.clone()) else {
            return;
        };
        let result: Result<(), String> = self.rt.block_on(async {
            use sovereign_ai::docs_host::{create, doc_error_message, new_user_document};
            let listed = docs.list(None).await.map_err(|e| doc_error_message(&e))?;
            if listed.iter().any(|d| d.title == "Token test A") {
                return Ok(()); // already seeded
            }
            let threads = db.list_threads().await.map_err(|e| e.to_string())?;
            let tid = threads
                .first()
                .and_then(|t| t.id.as_ref().map(|x| x.to_string()))
                .ok_or("no thread to put the test documents in")?;
            let a = new_user_document(
                "Token test A",
                tid.clone(),
                "Contact: jeanne.test@example.org\nCopy the token above into Token test B.",
            );
            let a = create(&docs, a).await.map_err(|e| doc_error_message(&e))?;
            if a.pii != sovereign_docs::PiiOutcome::Scanned {
                return Err(format!("the address was not tokenized ({:?})", a.pii));
            }
            let b = new_user_document("Token test B", tid, "Paste the token here: ");
            create(&docs, b).await.map_err(|e| doc_error_message(&e))?;
            Ok(())
        });
        match result {
            Ok(()) => {
                println!("sovereign-shell: SHELL_SEED_PII_TEST seeded (or already present)");
                self.load_workspace_now();
            }
            Err(e) => println!("sovereign-shell: SHELL_SEED_PII_TEST failed: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sovereign_db::mock::MockGraphDB;
    use sovereign_db::schema::{PiiKind, ReviewState, SourceRef};

    const TEST_FONT: &[u8] = include_bytes!("../../../sovereign-editor/tests/fixtures/Cantarell-Regular.ttf");

    // The editor's colours under each shell theme meet WCAG 2.1 AA: text,
    // markup, links and placeholder labels 4.5:1 on their background,
    // selected text 4.5:1 on the selection; caret, selection and provenance
    // accent 3:1 against the background (non-text).
    #[test]
    fn the_editor_colours_meet_wcag_aa_in_both_themes() {
        use sovereign_editor::contrast_ratio as cr;
        for (name, theme) in [("dark", crate::theme::Palette::dark()), ("light", crate::theme::Palette::light())] {
            let p = editor_palette(&theme);
            for (what, fg, bg, min) in [
                ("text", p.text, p.background, 4.5),
                ("markup / muted", p.muted, p.background, 4.5),
                ("muted on placeholder", p.muted, p.placeholder_background, 4.5),
                ("link", p.link, p.background, 4.5),
                ("selected text", p.selection_text, p.selection, 4.5),
                ("caret", p.caret, p.background, 3.0),
                ("selection", p.selection, p.background, 3.0),
                ("owned accent", p.owned, p.background, 3.0),
                ("external accent", p.external, p.background, 3.0),
                ("undetermined accent", p.undetermined, p.background, 3.0),
            ] {
                let r = cr(fg, bg);
                assert!(r >= min, "{name}: {what} {r:.2} < {min}");
            }
        }
    }

    // A theme switch reaches an open editor: its palette follows `pal()`.
    #[test]
    fn the_editor_follows_the_theme() {
        let light = editor_palette(&crate::theme::Palette::light());
        let dark = editor_palette(&crate::theme::Palette::dark());
        assert_ne!(light, dark);
        assert_eq!(light.background, crate::theme::Palette::light().panel);
        assert_eq!(dark.text, crate::theme::Palette::dark().text);
    }

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap()
    }

    fn record(kind: PiiKind, sources: Vec<SourceRef>) -> PiiRecord {
        PiiRecord {
            id: None,
            kind,
            value_encrypted: "AAAA".into(),
            value_nonce: "BBBB".into(),
            label: None,
            entity_id: None,
            stored_secret: false,
            confidence: 1.0,
            sources,
            discovered_at: chrono::Utc::now(),
            last_revealed_at: None,
            use_count: 0,
            review_state: ReviewState::Confirmed,
            deleted_at: None,
        }
    }

    fn src(doc: &str) -> SourceRef {
        SourceRef { source_kind: SourceKind::Document, source_id: doc.into(), span_start: 0, span_end: 0 }
    }

    // ---------------------------------------------------------------- personal data

    #[test]
    fn a_token_is_a_token_only_where_its_record_is_linked() {
        let rt = rt();
        let db = Arc::new(MockGraphDB::new());
        let linked = rt.block_on(db.create_pii_record(record(PiiKind::Email, vec![src("document:here")]))).unwrap();
        let elsewhere = rt.block_on(db.create_pii_record(record(PiiKind::Phone, vec![src("document:other")]))).unwrap();
        let as_message = rt.block_on(db.create_pii_record(record(
            PiiKind::Iban,
            vec![SourceRef { source_kind: SourceKind::Message, source_id: "document:here".into(), span_start: 0, span_end: 0 }],
        )))
        .unwrap();
        let pii = ShellPii::new(db.clone(), rt.handle().clone(), vec!["document:here".into()], Rc::default());

        assert_eq!(pii.display(&linked.id_string().unwrap()), TokenDisplay::Label("[Email]".into()));
        assert_eq!(pii.display(&elsewhere.id_string().unwrap()), TokenDisplay::NotAToken, "linked to another document");
        assert_eq!(pii.display(&as_message.id_string().unwrap()), TokenDisplay::NotAToken, "a message source is not this document");
        assert_eq!(pii.display("pii_record:missing"), TokenDisplay::NotAToken, "no such record");
        assert_eq!(pii.display("not a record id"), TokenDisplay::NotAToken);
        assert_eq!(pii.display(""), TokenDisplay::NotAToken);
    }

    #[test]
    fn a_label_never_carries_a_value() {
        let r = record(PiiKind::Email, vec![src("document:d")]);
        match token_display_for(Some(&r), &["document:d".into()], false) {
            TokenDisplay::Label(l) => assert_eq!(l, "[Email]"),
            other => panic!("{other:?}"),
        }
        // A display link never revives a record that does not exist.
        assert_eq!(token_display_for(None, &["document:d".into()], true), TokenDisplay::NotAToken);
    }

    /// The paste links for display only: the token is a token in this editor
    /// at once, and the store is not touched.
    #[test]
    fn a_paste_links_for_display_only() {
        let rt = rt();
        let db = Arc::new(MockGraphDB::new());
        let r = rt.block_on(db.create_pii_record(record(PiiKind::Email, vec![src("document:a")]))).unwrap();
        let id = r.id_string().unwrap();
        let links: DisplayLinks = Rc::default();
        let mut pii = ShellPii::new(db.clone(), rt.handle().clone(), vec!["document:b".into()], links.clone());
        assert_eq!(pii.display(&id), TokenDisplay::NotAToken);

        pii.link_to_document(std::slice::from_ref(&id));
        assert_eq!(pii.display(&id), TokenDisplay::Label("[Email]".into()));
        let sources = rt.block_on(db.get_pii_record(&id)).unwrap().sources;
        assert_eq!(sources, vec![src("document:a")], "nothing written at the paste");
        // Another editor (another document) does not see this editor's link.
        let other = ShellPii::new(db.clone(), rt.handle().clone(), vec!["document:c".into()], Rc::default());
        assert_eq!(other.display(&id), TokenDisplay::NotAToken);
        // Junk ids are not kept.
        pii.link_to_document(&["not an id".to_string()]);
        assert_eq!(links.borrow().len(), 1);
    }

    // ------------------------------------------------- saving through the service

    use sovereign_ai::docs_host::testing::{account_key, encrypted_memory_store};
    use sovereign_db::encrypted::EncryptedGraphDB;

    async fn service(tag: &str) -> (Arc<EncryptedGraphDB>, DocService) {
        let store = encrypted_memory_store(tag).await;
        let docs = sovereign_ai::docs_host::build_document_service(store.clone(), Some(account_key()), "test".into());
        (store, docs)
    }

    /// A row as writers before the service left it.
    async fn legacy(store: &EncryptedGraphDB, content: &str) -> String {
        let mut d = sovereign_db::schema::Document::new("D".into(), "thread:t".into(), true);
        d.content = content.into();
        store.create_document(d).await.unwrap().id_string().unwrap()
    }

    fn v2(text: &str) -> String {
        sovereign_editor::doc::DocumentV2::from_text(text, "seed").to_json_string()
    }

    async fn state_for(docs: &DocService, id: &str) -> SaveState {
        SaveState::new(Some(docs.read(id).await.unwrap().revision))
    }

    /// Adoption step 2: what the editor saves (a v1 envelope, raw text or v2
    /// document before) ends as v2, the prior state versioned; small autosaves
    /// make no version, Ctrl+S does.
    #[test]
    fn editor_saves_go_through_the_service() {
        rt().block_on(async {
            let (store, docs) = service("v2save").await;
            for stored in [r#"{"body":"enveloped","images":[]}"#.to_string(), "raw".to_string(), v2("already v2")] {
                let id = legacy(&store, &stored).await;
                let mut st = state_for(&docs, &id).await;
                let out = save_v2(&docs, &id, &mut st, &v2("typed"), false, vec![]).await.unwrap();
                assert!(out.version.is_none(), "a small autosave is not a version");
                let row = store.get_document(&id).await.unwrap();
                let v: serde_json::Value = serde_json::from_str(&row.content).unwrap();
                assert_eq!((v["format"].as_str(), v["v"].as_u64()), (Some("sovereign.doc"), Some(2)));
                let commits = store.list_document_commits(&id).await.unwrap();
                assert!(commits.iter().any(|c| c.snapshot.content == stored), "prior state versioned");
                let out = save_v2(&docs, &id, &mut st, &v2("typed more"), true, vec![]).await.unwrap();
                assert!(out.version.is_some(), "Ctrl+S makes a version");
            }
        });
    }

    /// Personal data typed in the editor is tokenized at the save.
    #[test]
    fn an_editor_save_runs_ingest() {
        rt().block_on(async {
            let (store, docs) = service("v2ingest").await;
            let id = legacy(&store, "").await;
            let mut st = state_for(&docs, &id).await;
            save_v2(&docs, &id, &mut st, &v2("mail alice@example.com"), false, vec![]).await.unwrap();
            let row = store.get_document(&id).await.unwrap();
            assert!(!row.content.contains("alice@example.com"));
            assert!(row.pii_scanned_at.is_some());
        });
    }

    /// Another writer saved first: the editor's save is refused (nothing
    /// overwritten); based on the current revision it lands, and the other
    /// writer's state is in History — also when that writer wrote around the
    /// service under this session's name (sync, until adoption step 4).
    #[test]
    fn a_conflict_overwrites_nothing_and_a_rebased_save_keeps_both() {
        rt().block_on(async {
            let (store, docs) = service("v2conflict").await;
            let id = legacy(&store, "start").await;
            let mut st = state_for(&docs, &id).await;
            save_v2(&docs, &id, &mut st, &v2("mine 1"), false, vec![]).await.unwrap();
            store.update_document(&id, None, Some("from the other device")).await.unwrap();
            let err = save_v2(&docs, &id, &mut st, &v2("mine 2"), false, vec![]).await.unwrap_err();
            let DocError::Conflict { current, .. } = err else { panic!("{err:?}") };
            assert_eq!(docs.read(&id).await.unwrap().plain_text(), "from the other device");
            st.revision = Some(current);
            save_v2(&docs, &id, &mut st, &v2("mine 2"), false, vec![]).await.unwrap();
            assert_eq!(docs.read(&id).await.unwrap().plain_text(), "mine 2");
            let commits = store.list_document_commits(&id).await.unwrap();
            assert!(commits.iter().any(|c| c.snapshot.content == "from the other device"), "the other state is in History");
        });
    }

    /// A closed editor's session ends: its last autosave becomes a version.
    #[test]
    fn ending_the_session_versions_the_last_save() {
        rt().block_on(async {
            let (store, docs) = service("v2end").await;
            let id = legacy(&store, "").await;
            let mut st = state_for(&docs, &id).await;
            save_v2(&docs, &id, &mut st, &v2("a few words"), false, vec![]).await.unwrap();
            let before = docs.versions(&id).await.unwrap().len();
            docs.end_session(&id, st.session).await.unwrap();
            assert_eq!(docs.versions(&id).await.unwrap().len(), before + 1);
        });
    }

    async fn pasted_record(store: &EncryptedGraphDB, linked_to: &str) -> String {
        store.create_pii_record(record(PiiKind::Email, vec![src(linked_to)])).await.unwrap().id_string().unwrap()
    }

    /// Paste, then delete the token, then save: nothing is linked in the
    /// store, and the display link is dropped.
    #[test]
    fn paste_delete_save_links_nothing() {
        rt().block_on(async {
            let (store, docs) = service("paste-del").await;
            let a = legacy(&store, "").await;
            let b = legacy(&store, "").await;
            let rid = pasted_record(&store, &a).await;
            let mut links: HashMap<String, Option<String>> = [(rid.clone(), Some(a.clone()))].into();
            let mut st = state_for(&docs, &b).await;
            let saved = v2("no token any more");
            save_v2(&docs, &b, &mut st, &saved, false, pasted_tokens(&links)).await.unwrap();
            let out = settle_display_links(&docs, &b, &saved, &mut links).await;
            assert_eq!(out, LinksAtSave { linked: 0, dropped: 1, failed: 0 });
            assert!(links.is_empty());
            assert_eq!(store.get_pii_record(&rid).await.unwrap().sources, vec![src(&a)]);
        });
    }

    /// Paste, then save with the token in the content: the service links the
    /// document to the record (once), and the display link is settled.
    #[test]
    fn paste_save_links_the_record() {
        rt().block_on(async {
            let (store, docs) = service("paste-save").await;
            let a = legacy(&store, "").await;
            let b = legacy(&store, "").await;
            let rid = pasted_record(&store, &a).await;
            let saved = v2(&format!("mail [pii:{rid}] here"));
            let mut st = state_for(&docs, &b).await;
            for _ in 0..2 {
                let mut links: HashMap<String, Option<String>> = [(rid.clone(), Some(a.clone()))].into();
                save_v2(&docs, &b, &mut st, &saved, false, pasted_tokens(&links)).await.unwrap();
                settle_display_links(&docs, &b, &saved, &mut links).await;
                assert!(links.is_empty(), "the display link is replaced by the stored one");
            }
            let sources = store.get_pii_record(&rid).await.unwrap().sources;
            assert_eq!(sources.iter().filter(|s| s.source_id == b).count(), 1, "linked once");
            assert!(sources.iter().any(|s| s.source_id == a), "the original source is kept");
            assert!(matches!(docs.token_status(&b, &rid).await.unwrap(), DocTokenStatus::Token { .. }));
        });
    }

    /// A token pasted from a document that does not link its record (or whose
    /// source is unknown): the save does not link it; the display link is kept
    /// for the next save.
    #[test]
    fn a_link_the_save_could_not_make_is_kept_for_the_next() {
        rt().block_on(async {
            let (store, docs) = service("paste-fail").await;
            let a = legacy(&store, "").await;
            let b = legacy(&store, "").await;
            let c = legacy(&store, "").await;
            let rid = pasted_record(&store, &a).await;
            let saved = v2(&format!("x [pii:{rid}] y"));
            let mut links: HashMap<String, Option<String>> =
                [(rid.clone(), Some(c.clone())), ("pii_record:gone".to_string(), None)].into();
            let mut st = state_for(&docs, &b).await;
            save_v2(&docs, &b, &mut st, &saved, false, pasted_tokens(&links)).await.unwrap();
            let out = settle_display_links(&docs, &b, &saved, &mut links).await;
            assert_eq!(out, LinksAtSave { linked: 0, dropped: 1, failed: 1 });
            assert!(links.contains_key(&rid));
            assert!(!store.get_pii_record(&rid).await.unwrap().sources.iter().any(|s| s.source_id == b));
        });
    }

    /// The clipboard remembers which document a copy was made in, while it is
    /// still our copy; a paste's display link records it.
    #[test]
    fn a_paste_records_the_document_it_was_copied_from() {
        let (sys, st) = state(true);
        let st = Rc::new(RefCell::new(st));
        st.borrow_mut().write_from(write("t", Some("payload"), None), Duration::ZERO, Some("document:a".into()));
        let rt = rt();
        let db = Arc::new(MockGraphDB::new());
        let links: DisplayLinks = Rc::default();
        let mut pii =
            ShellPii::new(db, rt.handle().clone(), vec!["document:b".into()], links.clone()).with_clipboard(st.clone());
        pii.link_to_document(&["pii_record:r1".to_string()]);
        assert_eq!(links.borrow().get("pii_record:r1"), Some(&Some("document:a".to_string())));
        // Another application's copy: the source is no longer known.
        other_app_copies(&sys, "elsewhere");
        pii.link_to_document(&["pii_record:r2".to_string()]);
        assert_eq!(links.borrow().get("pii_record:r2"), Some(&None));
    }

    // ---------------------------------------------------------------- clipboard

    #[derive(Default)]
    struct FakeSys {
        text: Option<String>,
        count: u64,
        has_count: bool,
        clears: usize,
        privacy: Vec<ClipboardPrivacy>,
    }
    struct FakeHandle(Rc<RefCell<FakeSys>>);
    impl SystemClipboard for FakeHandle {
        fn write_text(&mut self, text: &str, p: ClipboardPrivacy) -> Result<(), String> {
            let mut s = self.0.borrow_mut();
            s.text = Some(text.to_string());
            s.count += 1;
            s.privacy.push(p);
            Ok(())
        }
        fn read_text(&mut self) -> Option<String> {
            self.0.borrow().text.clone()
        }
        fn change_count(&self) -> Option<u64> {
            let s = self.0.borrow();
            s.has_count.then_some(s.count)
        }
        fn clear(&mut self) -> Result<(), String> {
            let mut s = self.0.borrow_mut();
            s.text = None;
            s.count += 1;
            s.clears += 1;
            Ok(())
        }
    }
    /// Another application copying.
    fn other_app_copies(sys: &Rc<RefCell<FakeSys>>, text: &str) {
        let mut s = sys.borrow_mut();
        s.text = Some(text.into());
        s.count += 1;
    }

    const ALL: ClipboardPrivacy =
        ClipboardPrivacy { exclude_from_history: true, exclude_from_cloud_sync: true, exclude_from_monitoring: true };

    fn write(text: &str, sovereign: Option<&str>, clear: Option<u64>) -> ClipboardWrite {
        ClipboardWrite {
            text: text.into(),
            sovereign: sovereign.map(String::from),
            privacy: ALL,
            clear_after: clear.map(Duration::from_secs),
        }
    }

    fn state(has_count: bool) -> (Rc<RefCell<FakeSys>>, ClipState) {
        let sys = Rc::new(RefCell::new(FakeSys { has_count, ..Default::default() }));
        (sys.clone(), ClipState::new(Box::new(FakeHandle(sys))))
    }

    #[test]
    fn a_revealed_copy_is_cleared_at_its_deadline_and_not_before() {
        for has_count in [true, false] {
            let (sys, mut cs) = state(has_count);
            cs.write(write("4111 1111 1111 1111", None, Some(30)), Duration::from_secs(100));
            assert_eq!(sys.borrow().privacy, vec![ALL], "the privacy flags reach the platform");
            cs.tick(Duration::from_secs(129));
            assert_eq!(sys.borrow().clears, 0);
            cs.tick(Duration::from_secs(130));
            assert_eq!(sys.borrow().clears, 1);
            assert_eq!(sys.borrow().text, None);
            cs.tick(Duration::from_secs(200));
            assert_eq!(sys.borrow().clears, 1, "cleared once");
        }
    }

    #[test]
    fn the_clear_timer_never_clears_another_applications_copy() {
        for has_count in [true, false] {
            let (sys, mut cs) = state(has_count);
            cs.write(write("secret value", None, Some(30)), Duration::ZERO);
            other_app_copies(&sys, "someone else's text");
            cs.tick(Duration::from_secs(31));
            assert_eq!(sys.borrow().clears, 0, "has_count={has_count}");
            assert_eq!(sys.borrow().text.as_deref(), Some("someone else's text"));
        }
    }

    #[test]
    fn a_later_ordinary_copy_cancels_the_pending_clear() {
        let (sys, mut cs) = state(true);
        cs.write(write("secret value", None, Some(30)), Duration::ZERO);
        cs.write(write("ordinary", None, None), Duration::from_secs(5));
        cs.tick(Duration::from_secs(60));
        assert_eq!(sys.borrow().clears, 0);
        assert_eq!(sys.borrow().text.as_deref(), Some("ordinary"), "ordinary text is never cleared");
    }

    #[test]
    fn the_payload_comes_back_only_while_the_clipboard_holds_our_copy() {
        for has_count in [true, false] {
            let (sys, mut cs) = state(has_count);
            cs.write(write("a [Email]", Some("payload"), None), Duration::ZERO);
            assert_eq!(
                cs.read(),
                Some(ClipboardContent { text: "a [Email]".into(), sovereign: Some("payload".into()) })
            );
            other_app_copies(&sys, "elsewhere");
            assert_eq!(cs.read(), Some(ClipboardContent { text: "elsewhere".into(), sovereign: None }));
        }
        // With a change counter, the same text copied again by another
        // application is still not ours.
        let (sys, mut cs) = state(true);
        cs.write(write("a [Email]", Some("payload"), None), Duration::ZERO);
        other_app_copies(&sys, "a [Email]");
        assert_eq!(cs.read().unwrap().sovereign, None);
    }

    // ---------------------------------------------------------------- input routing

    #[derive(Default)]
    struct Recorder {
        events: Vec<InputEvent>,
        handle_chars: bool,
    }
    impl EventSink for Recorder {
        fn send(&mut self, e: InputEvent) -> EventResult {
            let r = match &e {
                InputEvent::Key(KeyEvent { key: Key::Named(NamedKey::Tab | NamedKey::Escape | NamedKey::F6), .. }) => {
                    EventResult::NotHandled
                }
                InputEvent::Key(KeyEvent { key: Key::Character(_), modifiers }) => {
                    if self.handle_chars && modifiers.ctrl && !modifiers.alt {
                        EventResult::Handled
                    } else {
                        EventResult::NotHandled
                    }
                }
                _ => EventResult::Handled,
            };
            self.events.push(e);
            r
        }
    }

    fn texts(r: &Recorder) -> Vec<String> {
        r.events.iter().filter_map(|e| if let InputEvent::Text(t) = e { Some(t.clone()) } else { None }).collect()
    }

    fn ch(s: &str) -> WKey {
        WKey::Character(s.into())
    }

    #[test]
    fn named_keys_never_send_their_text() {
        for (k, t) in [
            (WNamed::Enter, "\r"),
            (WNamed::Tab, "\t"),
            (WNamed::Backspace, "\u{8}"),
            (WNamed::Delete, "\u{7f}"),
            (WNamed::Escape, "\u{1b}"),
        ] {
            let mut r = Recorder::default();
            route_key(&mut r, &WKey::Named(k), Some(t), Modifiers::NONE);
            assert!(texts(&r).is_empty(), "{k:?} sent its text");
            assert_eq!(r.events.len(), 1);
        }
        // Tab is declined by the editor and the shell gets it back.
        let mut r = Recorder::default();
        assert_eq!(route_key(&mut r, &WKey::Named(WNamed::Tab), Some("\t"), Modifiers::NONE), EventResult::NotHandled);
    }

    #[test]
    fn a_character_key_types_its_text_once() {
        let mut r = Recorder { handle_chars: true, ..Default::default() };
        route_key(&mut r, &ch("a"), Some("a"), Modifiers::NONE);
        route_key(&mut r, &ch("A"), Some("A"), Modifiers::SHIFT);
        route_key(&mut r, &WKey::Named(WNamed::Space), Some(" "), Modifiers::NONE);
        assert_eq!(texts(&r), vec!["a", "A", " "]);
    }

    #[test]
    fn a_shortcut_types_nothing() {
        let mut r = Recorder { handle_chars: true, ..Default::default() };
        assert_eq!(route_key(&mut r, &ch("z"), Some("\u{1a}"), Modifiers::CTRL), EventResult::Handled);
        // Not a shortcut the editor knows: still no text while Ctrl is held.
        let mut r2 = Recorder::default();
        route_key(&mut r2, &ch("1"), Some("1"), Modifiers::CTRL);
        route_key(&mut r2, &WKey::Named(WNamed::Space), Some(" "), Modifiers::CTRL);
        assert!(texts(&r).is_empty() && texts(&r2).is_empty());
    }

    #[test]
    fn altgr_and_dead_keys_type_what_the_layout_gives() {
        let altgr = Modifiers { shift: false, ctrl: true, alt: true, meta: false };
        let mut r = Recorder { handle_chars: true, ..Default::default() };
        route_key(&mut r, &ch("@"), Some("@"), altgr);
        route_key(&mut r, &WKey::Dead(Some('\u{301}')), None, Modifiers::NONE);
        route_key(&mut r, &ch("e"), Some("\u{e9}"), Modifiers::NONE);
        assert_eq!(texts(&r), vec!["@", "\u{e9}"]);
    }

    #[test]
    fn multi_clicks_count_up_to_three_when_close_in_time_and_space() {
        let t = Instant::now();
        assert_eq!(click_count(None, t, 10.0, 10.0), 1);
        let n2 = click_count(Some((t, 10.0, 10.0, 1)), t + Duration::from_millis(200), 11.0, 10.0);
        assert_eq!(n2, 2);
        assert_eq!(click_count(Some((t, 10.0, 10.0, 2)), t + Duration::from_millis(300), 10.0, 10.0), 3);
        assert_eq!(click_count(Some((t, 10.0, 10.0, 3)), t + Duration::from_millis(300), 10.0, 10.0), 3);
        assert_eq!(click_count(Some((t, 10.0, 10.0, 1)), t + Duration::from_millis(900), 10.0, 10.0), 1);
        assert_eq!(click_count(Some((t, 10.0, 10.0, 1)), t + Duration::from_millis(100), 30.0, 10.0), 1);
    }

    // ---------------------------------------------------------------- the real editor

    struct NoPii;
    impl PiiDisplay for NoPii {
        fn display(&self, _: &str) -> TokenDisplay {
            TokenDisplay::NotAToken
        }
        fn link_to_document(&mut self, _: &[String]) {}
    }

    fn pane(content: &str) -> EditorPane {
        let (_sys, cs) = state(true);
        EditorPane::open(
            "T",
            content.into(),
            Trust::Owned,
            Fonts::Only(vec![TEST_FONT.to_vec()]),
            Box::new(NoPii),
            Rc::default(),
            Rc::new(RefCell::new(cs)),
            Rc::default(),
            vec!["document:t".into()],
        )
        .unwrap()
    }

    fn type_keys(p: &mut EditorPane, keys: &[(WKey, Option<&str>, Modifiers)]) {
        for (k, t, m) in keys {
            route_key(&mut p.editor, k, *t, *m);
        }
    }

    /// Through the real editor: Enter makes one line break, Tab inserts
    /// nothing, typed text goes in once, and Ctrl+S asks for one save of the
    /// v2 form.
    #[test]
    fn the_editor_receives_each_keystroke_once() {
        let mut p = pane("");
        p.set_focus(true);
        type_keys(
            &mut p,
            &[
                (ch("h"), Some("h"), Modifiers::NONE),
                (ch("i"), Some("i"), Modifiers::NONE),
                (WKey::Named(WNamed::Enter), Some("\r"), Modifiers::NONE),
                (WKey::Named(WNamed::Tab), Some("\t"), Modifiers::NONE),
                (ch("x"), Some("x"), Modifiers::NONE),
            ],
        );
        let text: String = p.editor.document().blocks().iter().filter_map(|b| b.text()).collect::<Vec<_>>().join("\n");
        assert_eq!(text, "hi\nx", "one line break, no tab, nothing doubled");
        type_keys(&mut p, &[(ch("s"), Some("\u{13}"), Modifiers::CTRL)]);
        let saves = p.take_saves();
        assert_eq!(saves.len(), 1);
        let (_, form) = sovereign_editor::doc::DocumentV2::read(&saves[0].content).unwrap();
        assert_eq!(form, sovereign_editor::doc::SourceForm::V2, "v2 is written");
        assert!(!saves[0].content.contains('\t'));
        // The store answers; the pane remembers what it holds.
        p.save_done(saves[0].id, Ok(()));
        assert_eq!(p.stored, saves[0].content);
        assert!(!p.has_unsaved());
    }

    #[test]
    fn a_failed_save_keeps_the_text_and_is_announced_once() {
        let mut p = pane("");
        p.set_focus(true);
        type_keys(&mut p, &[(ch("a"), Some("a"), Modifiers::NONE)]);
        p.editor.command(Command::Save);
        let s = p.take_saves();
        p.save_done(s[0].id, Err(SaveError { message: "disk full".into() }));
        assert!(p.collect_notices().is_some(), "first failure is announced");
        assert!(p.has_unsaved());
        assert!(p.status_text().contains("Save failed"));
        p.editor.command(Command::Save);
        let s = p.take_saves();
        p.save_done(s[0].id, Err(SaveError { message: "disk full".into() }));
        assert!(p.collect_notices().is_none(), "not announced again until a save succeeds");
        // Closing now asks (the unsaved prompt), it does not lose the text.
        assert_eq!(p.editor.request_close(), CloseRequest::ConfirmUnsaved);
    }

    #[test]
    fn opening_reads_raw_text_and_the_v1_envelope() {
        let p = pane("plain text body");
        assert_eq!(p.editor.document().blocks()[0].text(), Some("plain text body"));
        let env = sovereign_core::content::ContentFields { body: "enveloped".into(), ..Default::default() }.serialize();
        let p = pane(&env);
        assert_eq!(p.editor.document().blocks()[0].text(), Some("enveloped"));
    }

    // ---- accessibility: the editor's tree inlined in the shell's tree ----

    struct NoChanges;
    impl accesskit_consumer::TreeChangeHandler for NoChanges {
        fn node_added(&mut self, _: &accesskit_consumer::Node) {}
        fn node_updated(&mut self, _: &accesskit_consumer::Node, _: &accesskit_consumer::Node) {}
        fn focus_moved(&mut self, _: Option<&accesskit_consumer::Node>, _: Option<&accesskit_consumer::Node>) {}
        fn node_removed(&mut self, _: &accesskit_consumer::Node) {}
    }

    /// The shell's root tree as `build_a11y_tree` shapes it: a window, one
    /// label, and the editor container (with or without the editor's root).
    fn shell_root(child: Option<accesskit::NodeId>, focus: accesskit::NodeId) -> accesskit::TreeUpdate {
        use accesskit::{Node, NodeId, Role, Tree, TreeId, TreeUpdate};
        let mut root = Node::new(Role::Window);
        root.set_children(vec![NodeId(1), EDITOR_CONTAINER]);
        let mut label = Node::new(Role::Label);
        label.set_label("Document");
        TreeUpdate {
            nodes: vec![(NodeId(0), root), (NodeId(1), label), (EDITOR_CONTAINER, container_node(child))],
            tree: Some(Tree::new(NodeId(0))),
            tree_id: TreeId::ROOT,
            focus,
        }
    }

    /// Regression, 2026-10-11 (Céline's first manual run): with the editor
    /// grafted as a subtree, a change of OS window focus made AccessKit 0.35
    /// re-apply the subtree's focus to the root tree and panic ("Focused ID
    /// #2 is not in the node list"). Inlined, the same sequence (open, focus
    /// in the editor, window loses and regains focus, typing) must hold.
    #[test]
    fn inlined_editor_tree_survives_window_focus_changes() {
        let mut p = pane("hello");
        p.editor.handle_event(sovereign_editor::input::InputEvent::Focus(true));
        let full = p.editor.accessibility_tree();
        let root_local = full.tree.as_ref().map(|t| t.root).unwrap();
        let editor_focus = full.focus;

        // Frame 1: the shell's tree, container empty (editor not sent yet).
        let mut tree = accesskit_consumer::Tree::new(shell_root(None, accesskit::NodeId(0)), true);
        // Then the editor's whole tree, inlined, focus inside it.
        let mut nodes = vec![(EDITOR_CONTAINER, container_node(Some(map_node(root_local))))];
        nodes.extend(remap_nodes(full.nodes));
        tree.update_and_process_changes(
            accesskit::TreeUpdate { nodes, tree: None, tree_id: accesskit::TreeId::ROOT, focus: map_node(editor_focus) },
            &mut NoChanges,
        );
        // Frame 2: the shell's tree again, now hanging the editor and focusing it.
        tree.update_and_process_changes(shell_root(Some(map_node(root_local)), map_node(editor_focus)), &mut NoChanges);
        // The OS window loses, then regains focus (the path that crashed).
        tree.update_host_focus_state_and_process_changes(false, &mut NoChanges);
        tree.update_host_focus_state_and_process_changes(true, &mut NoChanges);
        assert_eq!(tree.state().focus().map(|n| n.role()), Some(accesskit::Role::MultilineTextInput));

        // Typing: the editor's incremental update, inlined, then a focus change again.
        p.editor.handle_event(sovereign_editor::input::InputEvent::Text("!".into()));
        p.editor.paint(&mut vello::Scene::new());
        for u in p.take_a11y() {
            tree.update_and_process_changes(
                accesskit::TreeUpdate { nodes: remap_nodes(u.nodes), tree: None, tree_id: accesskit::TreeId::ROOT, focus: map_node(u.focus) },
                &mut NoChanges,
            );
        }
        tree.update_host_focus_state_and_process_changes(false, &mut NoChanges);
        tree.update_host_focus_state_and_process_changes(true, &mut NoChanges);
    }
}
