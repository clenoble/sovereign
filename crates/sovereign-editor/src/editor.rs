//! The editor itself.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;
use std::time::Duration;

use parley::Affinity;
use unicode_segmentation::UnicodeSegmentation;

use crate::a11y;
use crate::clip::{self, Part};
use crate::doc::{Block, BlockId, BlockKind, DocumentV2, SourceForm};
use crate::host::{
    AccessibilitySink, Clipboard, ClipboardContent, ClipboardPrivacy, ClipboardWrite, Clock, DocumentService, Host, PiiDisplay,
    RemoteChange, SaveError, SaveId, SaveRequest, TokenDisplay, WindowHost,
};
use crate::input::{Command, ImeEvent, InputEvent, Key, KeyEvent, Modifiers, NamedKey, PointerButton, PointerEvent};
use crate::text::{self, Snap};
use crate::view::{self, Env, LineRef, Palette, Place, View, PAD};
use crate::Rect;

/// Guarantee 9's cap, decided 2026-10-10: 1 MB = 1,000,000 bytes of UTF-8
/// text in `text` blocks ([`crate::doc::DocumentV2::text_bytes`]). A document
/// with at most this much text is editable; above it, it opens read-only.
pub const MAX_EDITABLE_TEXT_BYTES: usize = 1_000_000;

/// Fonts the editor lays text out with. The host passes its own choice; tests
/// pass a bundled font only, so geometry does not depend on the machine.
#[derive(Clone)]
pub enum Fonts {
    /// The platform's fonts (what the shell uses).
    System,
    /// Only these font files (TrueType / OpenType bytes); no system fonts.
    Only(Vec<Vec<u8>>),
}

#[derive(Clone)]
pub struct EditorConfig {
    pub fonts: Fonts,
    /// The AccessKit tree the editor's updates belong to. The shell grafts it
    /// under its own tree; a host that has no other tree passes
    /// `accesskit::TreeId::ROOT`.
    pub accessibility_tree: accesskit::TreeId,
}

/// The document's trust state (ch. 03, ch. 04), shown as the provenance accent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trust {
    Owned,
    External,
    Undetermined,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenMode {
    Edit,
    /// The user chose **[Read only]** (e.g. the document is being edited on
    /// another device).
    ReadOnly,
}

/// What the host opens the editor on.
#[derive(Clone)]
pub struct OpenDocument {
    pub title: String,
    /// The stored content string: v2, the v1 envelope, or raw text (ch. 03).
    pub content: String,
    pub trust: Trust,
    pub mode: OpenMode,
}

/// A place in the document: a block and a byte offset into its text (for a
/// `text` block, into the markdown source as stored, tokens included; 0 for
/// other blocks).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub block: BlockId,
    pub offset: usize,
}

/// `anchor` stays put while the selection is extended; `focus` is the caret.
/// `anchor == focus` is a plain caret.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub anchor: Position,
    pub focus: Position,
}

impl Selection {
    pub fn caret(at: Position) -> Selection {
        Selection { anchor: at, focus: at }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadOnlyReason {
    /// Opened with [`OpenMode::ReadOnly`].
    OpenedReadOnly,
    /// The stored `v` is newer than this version knows (ch. 03).
    NewerFormat { v: u64 },
    /// More text than the editable cap (guarantee 9): more than
    /// [`MAX_EDITABLE_TEXT_BYTES`].
    TooLarge { text_bytes: usize },
    /// Editing was switched to another device (C-TEXT, *One device edits a
    /// document at a time*).
    EditingElsewhere { device: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveStatus {
    /// The in-memory model is what the store holds.
    Saved,
    /// Changes not yet sent to the store.
    Unsaved,
    /// A save is in flight.
    Saving,
    /// The last save of the current model failed; the model is kept.
    Failed { message: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeKind {
    SaveFailed,
    /// Editing input arrived while the document is read-only (opened read-only,
    /// newer format, over the cap). Input never disappears without a notice
    /// (decided 2026-10-10).
    ReadOnly,
    /// Input that could not be applied (e.g. it would exceed the cap).
    InputRefused,
    RemoteChange,
    /// Editing input arrived after editing moved to another device; the
    /// message names the device ("this document is being edited on <device>").
    EditingElsewhere,
}

/// Something the editor tells the user. Messages never contain document text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub kind: NoticeKind,
    pub message: String,
}

/// Whether the editor used an input event. A key the editor does not use
/// (e.g. the one that moves focus out) is left to the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventResult {
    Handled,
    NotHandled,
}

/// The answer to [`Editor::request_close`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseRequest {
    /// Nothing unsaved: close now.
    Ready,
    /// A save of the unsaved changes was just requested; close after
    /// [`Editor::save_finished`] reports success.
    Saving,
    /// The last save failed: ask the user (the unsaved prompt).
    ConfirmUnsaved,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenError {
    /// Must not contain document text.
    pub message: String,
}

/// The single-line text fields built on the same component (the text-field
/// contract). A password field never exposes its content (guarantee 10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    SingleLine,
    Password,
}

/// The two parts of a document editor that take the caret (title contract,
/// `doc/plans/TITLE-edit.md`): the single-line title field above the body,
/// and the body. An editor opened on a document starts in the body; the host
/// puts a new document on its title ([`Editor::focus_title`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorPart {
    Title,
    Body,
}

/// What the title's text is drawn with (title contract T1, revised
/// 2026-10-17): its size and the body's, in logical pixels, and its weight on
/// the CSS scale (400 regular, 600 semi-bold, 700 bold). Read by the tests
/// (`tests/t_title.rs`), which also check the size against the laid-out
/// lines.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TitleFont {
    pub size: f32,
    pub weight: u16,
    pub body_size: f32,
}

/// A pause longer than this ends a typing burst (one undo step).
const BURST_GAP: Duration = Duration::from_millis(1500);
/// Autosave after this pause in typing.
const AUTOSAVE_DELAY: Duration = Duration::from_secs(2);
/// After a failed save, autosave tries again no sooner than this.
const RETRY_DELAY: Duration = Duration::from_secs(30);
/// A copy holding a revealed personal-data value is cleared after this (ch. 09).
const REVEALED_CLEAR_AFTER: Duration = Duration::from_secs(30);
const MAX_UNDO: usize = 500;
const MAX_NOTICES: usize = 64;
/// The position of an empty document (it has no block yet).
const NO_BLOCK: BlockId = BlockId([0; 16]);
const ALL_EXCLUDED: ClipboardPrivacy =
    ClipboardPrivacy { exclude_from_history: true, exclude_from_cloud_sync: true, exclude_from_monitoring: true };
/// The title field's text style (T1 revised: 1.1 × body, semi-bold).
const TITLE_HEADING: u8 = view::TITLE_LEVEL;

/// The host, shared by a document editor and its title field: each gets a
/// [`Host`] whose capabilities forward to the one the host gave. The title
/// field never saves (the document editor saves its title with the body)
/// and never sends accessibility updates (the document editor's tree holds
/// the title's node).
#[derive(Clone)]
struct Shared(Rc<RefCell<Host>>);

impl Shared {
    /// `title_dy`: for the title field, how far the page's scroll moves it
    /// (its input-method area is reported moved by that much).
    fn host(&self, title_dy: Option<Rc<Cell<f64>>>) -> Host {
        if let Some(dy) = title_dy {
            Host {
                documents: Box::new(Nothing),
                clipboard: Box::new(self.clone()),
                accessibility: Box::new(Nothing),
                clock: Box::new(self.clone()),
                pii: Box::new(self.clone()),
                window: Box::new(TitleWindow { shared: self.clone(), dy }),
            }
        } else {
            Host {
                documents: Box::new(self.clone()),
                clipboard: Box::new(self.clone()),
                accessibility: Box::new(self.clone()),
                clock: Box::new(self.clone()),
                pii: Box::new(self.clone()),
                window: Box::new(self.clone()),
            }
        }
    }
}

impl DocumentService for Shared {
    fn save(&mut self, request: SaveRequest) {
        self.0.borrow_mut().documents.save(request);
    }
}
impl Clipboard for Shared {
    fn read(&mut self) -> Option<ClipboardContent> {
        self.0.borrow_mut().clipboard.read()
    }
    fn write(&mut self, item: ClipboardWrite) {
        self.0.borrow_mut().clipboard.write(item);
    }
}
impl AccessibilitySink for Shared {
    fn update(&mut self, update: accesskit::TreeUpdate) {
        self.0.borrow_mut().accessibility.update(update);
    }
}
impl Clock for Shared {
    fn now(&self) -> Duration {
        self.0.borrow().clock.now()
    }
}
impl PiiDisplay for Shared {
    fn display(&self, record_id: &str) -> TokenDisplay {
        self.0.borrow().pii.display(record_id)
    }
    fn link_to_document(&mut self, record_ids: &[String]) {
        self.0.borrow_mut().pii.link_to_document(record_ids);
    }
}
impl WindowHost for Shared {
    fn set_ime_cursor_area(&mut self, area: Rect) {
        self.0.borrow_mut().window.set_ime_cursor_area(area);
    }
    fn request_redraw(&mut self) {
        self.0.borrow_mut().window.request_redraw();
    }
}

/// The title field's window: its input-method area moved to where the
/// scrolled page draws the title.
struct TitleWindow {
    shared: Shared,
    dy: Rc<Cell<f64>>,
}
impl WindowHost for TitleWindow {
    fn set_ime_cursor_area(&mut self, area: Rect) {
        let dy = self.dy.get();
        self.shared.set_ime_cursor_area(Rect { y0: area.y0 + dy, y1: area.y1 + dy, ..area });
    }
    fn request_redraw(&mut self) {
        self.shared.request_redraw();
    }
}

/// What the title field has instead of a document service and an
/// accessibility sink.
struct Nothing;
impl DocumentService for Nothing {
    fn save(&mut self, _: SaveRequest) {}
}
impl AccessibilitySink for Nothing {
    fn update(&mut self, _: accesskit::TreeUpdate) {}
}

/// A title as one line: a line break (CR LF, LF or CR) becomes a space (T4).
fn one_line(s: &str) -> String {
    s.replace("\r\n", " ").replace(['\n', '\r'], " ")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditKind {
    Typing,
    DeleteBack,
    DeleteForward,
    Other,
}

struct Snapshot {
    blocks: Vec<Block>,
    sel: Selection,
    aff: Affinity,
    state: u64,
}

/// The editor component.
pub struct Editor {
    host: Host,
    config: EditorConfig,
    trust: Trust,
    field: Option<FieldKind>,
    doc: DocumentV2,
    read_only: Option<ReadOnlyReason>,

    sel: Selection,
    aff: Affinity,
    goal_x: Option<f32>,
    focused: bool,
    preedit: Option<(String, Option<(usize, usize)>)>,
    dragging: bool,

    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    group: Option<(EditKind, Duration)>,
    mutations: u64,

    /// Identifies the model's content: a fresh value per change; undo and
    /// redo restore the value of the state they restore.
    state: u64,
    next_state: u64,
    /// The state the store is known to hold.
    saved_state: u64,
    /// The title the store is known to hold (as saved: trimmed, T4).
    saved_title: String,
    next_save: u64,
    /// Saves asked for: their id, the body's state and the (trimmed) title
    /// they carry.
    in_flight: Vec<(SaveId, u64, String)>,
    last_ok: Option<SaveId>,
    failure: Option<(SaveId, String)>,
    failed_at: Option<Duration>,
    last_edit: Option<Duration>,

    notices: Vec<Notice>,
    text_bytes: usize,

    view: View,
    viewport: (f64, f64, f64),
    scroll_y: f32,
    /// A single-line field's horizontal scroll (title contract T9): its text
    /// never wraps, and scrolls sideways so the caret stays visible, as an
    /// HTML single-line input does. Always 0 for a document's body.
    scroll_x: f32,
    /// Scroll keeps the caret in view (until the host scrolls the text
    /// itself; any caret move or edit sets it again).
    follow_caret: bool,
    palette: Palette,
    pii_cache: RefCell<HashMap<String, TokenDisplay>>,
    a11y_active: bool,
    a11y_dirty: bool,
    ime_area: Option<Rect>,

    /// A document editor's title field (title contract T1–T7): a
    /// single-line field on this same component, the top of the page (it
    /// scrolls with the body, T1 revised). `None` for a text field.
    title_field: Option<Box<Editor>>,
    /// Which part takes keyboard and input-method input.
    part: EditorPart,
    /// The height of the title at the top of the page (0 without a title
    /// field). The body follows it.
    title_h: f32,
    /// The title's accessible nodes changed since they were last sent.
    title_a11y_dirty: bool,
    /// A press on the title is being dragged.
    drag_title: bool,
    title_placeholder: Option<parley::Layout<view::Brush>>,
    /// The title is painted here, then placed where the scroll puts it.
    body_scene: vello::Scene,
    /// How far the scroll moves the title (minus the page's scroll), shared
    /// with the title field's window forwarder.
    title_dy: Rc<Cell<f64>>,
}

/// How text in token form shows, through the host's resolver (cached until
/// the host says the display changed). The host applies the resolver rule:
/// `NotAToken` where this document is not among the record's sources.
fn token_display(pii: &dyn crate::host::PiiDisplay, cache: &RefCell<HashMap<String, TokenDisplay>>, id: &str) -> TokenDisplay {
    if let Some(d) = cache.borrow().get(id) {
        return d.clone();
    }
    let d = pii.display(id);
    cache.borrow_mut().insert(id.to_string(), d.clone());
    d
}

/// What a token shows, or `None` for ordinary text.
fn shown(d: TokenDisplay) -> Option<String> {
    match d {
        TokenDisplay::Label(s) | TokenDisplay::Revealed(s) => Some(s),
        TokenDisplay::NotAToken => None,
    }
}

impl Editor {
    fn new(host: Host, config: EditorConfig, doc: DocumentV2) -> Editor {
        let view = View::new(&config.fonts);
        let text_bytes = doc.text_bytes();
        let mut ed = Editor {
            host,
            config,
            trust: Trust::Owned,
            field: None,
            doc,
            read_only: None,
            sel: Selection::caret(Position { block: NO_BLOCK, offset: 0 }),
            aff: Affinity::Downstream,
            goal_x: None,
            focused: false,
            preedit: None,
            dragging: false,
            undo: vec![],
            redo: vec![],
            group: None,
            mutations: 0,
            state: 0,
            next_state: 1,
            saved_state: 0,
            saved_title: String::new(),
            next_save: 1,
            in_flight: vec![],
            last_ok: None,
            failure: None,
            failed_at: None,
            last_edit: None,
            notices: vec![],
            text_bytes,
            view,
            viewport: (800.0, 600.0, 1.0),
            scroll_y: 0.0,
            scroll_x: 0.0,
            follow_caret: true,
            palette: Palette::DARK,
            pii_cache: RefCell::new(HashMap::new()),
            a11y_active: false,
            a11y_dirty: true,
            ime_area: None,
            title_field: None,
            part: EditorPart::Body,
            title_h: 0.0,
            title_a11y_dirty: false,
            drag_title: false,
            title_placeholder: None,
            body_scene: vello::Scene::new(),
            title_dy: Rc::new(Cell::new(0.0)),
        };
        ed.sel = Selection::caret(ed.doc_start());
        ed
    }

    /// Opens a document. The stored content is read in any of its forms
    /// (ch. 03); a newer `v`, more text than the cap, or `OpenMode::ReadOnly`
    /// opens read-only.
    pub fn open(document: OpenDocument, config: EditorConfig, host: Host) -> Result<Editor, OpenError> {
        let (doc, form) = DocumentV2::read(&document.content).map_err(|e| OpenError { message: e.message })?;
        let shared = Shared(Rc::new(RefCell::new(host)));
        let mut ed = Editor::new(shared.host(None), config.clone(), doc);
        ed.trust = document.trust;
        ed.read_only = ed.read_only_for(&form, document.mode == OpenMode::ReadOnly);

        // The title field: the stored title, on one line (T4).
        let title = one_line(&document.title);
        let mut field = Editor::open_field(FieldKind::SingleLine, config, shared.host(Some(ed.title_dy.clone())));
        field.view.plain_heading = TITLE_HEADING;
        if let Some(b) = field.doc.blocks_mut().first_mut() {
            b.text_mut().push_str(&title);
        }
        field.text_bytes = field.doc.text_bytes();
        field.sel = Selection::caret(field.doc_start());
        let line = field.view.plain_layout("Ag", TITLE_HEADING).height().max(view::FONT_SIZE);
        ed.title_h = (2.0 * PAD + line).ceil();
        ed.saved_title = title.trim().to_string();
        let (w, _, s) = ed.viewport;
        field.set_viewport(w, ed.title_h as f64, s);
        ed.title_field = Some(Box::new(field));
        Ok(ed)
    }

    fn read_only_for(&self, form: &SourceForm, opened_read_only: bool) -> Option<ReadOnlyReason> {
        if let SourceForm::NewerVersion { v } = form {
            return Some(ReadOnlyReason::NewerFormat { v: *v });
        }
        if self.text_bytes > MAX_EDITABLE_TEXT_BYTES {
            return Some(ReadOnlyReason::TooLarge { text_bytes: self.text_bytes });
        }
        opened_read_only.then_some(ReadOnlyReason::OpenedReadOnly)
    }

    /// A single-line text field on the same component. It never asks the
    /// document service to save.
    pub fn open_field(kind: FieldKind, config: EditorConfig, host: Host) -> Editor {
        let mut ed = Editor::new(host, config, DocumentV2::field());
        ed.field = Some(kind);
        ed.view.set_width(None);
        ed
    }

    // ================================================================ input

    /// Left / Right move to the next cluster in **visual** order; a selection
    /// is always a logical range. In a read-only state, editing input changes
    /// nothing and produces a [`Notice`].
    pub fn handle_event(&mut self, event: InputEvent) -> EventResult {
        match event {
            InputEvent::Key(k) => self.key(k),
            InputEvent::Text(t) if self.in_title() => self.forward_title(InputEvent::Text(t)),
            InputEvent::Ime(e) if self.in_title() => {
                self.forward_title(InputEvent::Ime(e));
                EventResult::Handled
            }
            InputEvent::Text(t) => {
                let t: String = t.chars().filter(|c| !c.is_control() || matches!(c, '\t' | '\n' | '\r')).collect();
                if !t.is_empty() {
                    self.insert_typed(&t, EditKind::Typing);
                }
                EventResult::Handled
            }
            InputEvent::Ime(e) => {
                self.ime(e);
                EventResult::Handled
            }
            InputEvent::Pointer(p) => self.pointer(p),
            InputEvent::Focus(f) => {
                self.focused = f;
                if !f {
                    self.preedit = None;
                    self.dragging = false;
                }
                self.group = None;
                self.sync_title_focus();
                self.changed_view();
                EventResult::Handled
            }
        }
    }

    pub fn command(&mut self, command: Command) {
        if self.in_title() {
            match command {
                Command::Save => self.save_now(),
                Command::RemoveBlock(id) => self.remove_block(id),
                c => {
                    self.title_do(|f| f.command(c));
                }
            }
            return;
        }
        match command {
            Command::Undo => self.undo(),
            Command::Redo => self.redo(),
            Command::Copy => {
                self.copy();
            }
            Command::Cut => self.cut(),
            Command::Paste => self.paste(),
            Command::SelectAll => self.select_all(),
            Command::Save => self.save_now(),
            Command::RemoveBlock(id) => self.remove_block(id),
        }
    }

    fn key(&mut self, k: KeyEvent) -> EventResult {
        let m = k.modifiers;
        let cmd = m.ctrl || m.meta;
        if self.in_title() {
            return self.title_key(k);
        }
        // Shift+Tab from anywhere in the body goes to the title (T3: the
        // field order title → body).
        if k.key == Key::Named(NamedKey::Tab) && m.shift && !cmd && !m.alt && self.title_editable() {
            self.focus_title();
            return EventResult::Handled;
        }
        match k.key {
            Key::Named(n) => self.named_key(n, m),
            Key::Character(c) if cmd && !m.alt => {
                match c.to_lowercase().as_str() {
                    "a" => self.select_all(),
                    "c" => {
                        self.copy();
                    }
                    "x" => self.cut(),
                    "v" => self.paste(),
                    "z" if m.shift => self.redo(),
                    "z" => self.undo(),
                    "y" => self.redo(),
                    "s" => self.save_now(),
                    _ => return EventResult::NotHandled,
                }
                EventResult::Handled
            }
            Key::Character(_) => EventResult::NotHandled,
        }
    }

    fn named_key(&mut self, n: NamedKey, m: Modifiers) -> EventResult {
        let word = m.ctrl || m.alt || m.meta;
        match n {
            NamedKey::ArrowLeft | NamedKey::ArrowRight => self.move_horizontal(n == NamedKey::ArrowRight, m.shift, word),
            NamedKey::ArrowUp | NamedKey::ArrowDown => self.move_vertical(n == NamedKey::ArrowDown, m.shift, 1),
            NamedKey::PageUp | NamedKey::PageDown => {
                let lines = ((self.body_h() / self.view.line_h) as usize).saturating_sub(1).max(1);
                self.move_vertical(n == NamedKey::PageDown, m.shift, lines)
            }
            NamedKey::Home | NamedKey::End => {
                if m.ctrl || m.meta {
                    let p = if n == NamedKey::Home { self.doc_start() } else { self.doc_end() };
                    self.move_to(p, if n == NamedKey::Home { Affinity::Downstream } else { Affinity::Upstream }, m.shift);
                } else {
                    self.line_edge(n == NamedKey::End, m.shift);
                }
            }
            NamedKey::Backspace => self.delete(false, word),
            NamedKey::Delete => self.delete(true, word),
            NamedKey::Enter => {
                if self.field.is_some() {
                    return EventResult::NotHandled;
                }
                // An Enter while a pre-edit is showing belongs to the input
                // method (it commits the composition): no line break, no list
                // continuation; the commit arrives as its own event (U35).
                if self.preedit.is_some() {
                    return EventResult::Handled;
                }
                self.enter(m.shift);
            }
            NamedKey::Escape => {
                if self.preedit.take().is_some() {
                    self.changed_view();
                } else {
                    return EventResult::NotHandled;
                }
            }
            NamedKey::Tab | NamedKey::F6 => return EventResult::NotHandled,
        }
        EventResult::Handled
    }

    fn ime(&mut self, e: ImeEvent) {
        match e {
            ImeEvent::Enabled => {}
            ImeEvent::Preedit { text, cursor } => {
                if self.read_only.is_some() {
                    return;
                }
                self.preedit = if text.is_empty() { None } else { Some((text, cursor)) };
                self.changed_view();
            }
            ImeEvent::Commit(t) => {
                self.preedit = None;
                if !t.is_empty() {
                    self.insert_typed(&t, EditKind::Typing);
                }
                self.changed_view();
            }
            ImeEvent::Disabled => {
                if self.preedit.take().is_some() {
                    self.changed_view();
                }
            }
        }
    }

    /// A pointer event in the editor's coordinates. The page is the title
    /// (its top) then the body, scrolled together (T1 revised): a press on
    /// the title, where it is now drawn, goes to the title field (in its own
    /// coordinates: the page's), anywhere else to the body. A drag stays with
    /// the part it started in.
    fn pointer(&mut self, p: PointerEvent) -> EventResult {
        if self.title_field.is_none() {
            return self.body_pointer(p);
        }
        let s = self.scroll_y as f64;
        // The same event in the title field's coordinates.
        let in_field = |p: PointerEvent| match p {
            PointerEvent::Down { x, y, button, click_count, modifiers } => {
                PointerEvent::Down { x, y: y + s, button, click_count, modifiers }
            }
            PointerEvent::Move { x, y } => PointerEvent::Move { x, y: y + s },
            PointerEvent::Up { x, y, button } => PointerEvent::Up { x, y: y + s, button },
        };
        match p {
            PointerEvent::Down { y, button: PointerButton::Primary, .. } => {
                self.drag_title = false;
                if y + s < self.title_h as f64 {
                    if !self.title_editable() {
                        // Read-only, the title is text (T7).
                        return EventResult::Handled;
                    }
                    self.preedit = None;
                    self.dragging = false;
                    self.enter_part(EditorPart::Title);
                    self.drag_title = true;
                    self.forward_title(InputEvent::Pointer(in_field(p)));
                    return EventResult::Handled;
                }
                self.enter_part(EditorPart::Body);
                self.body_pointer(p)
            }
            PointerEvent::Move { .. } if self.drag_title => {
                self.forward_title(InputEvent::Pointer(in_field(p)));
                EventResult::Handled
            }
            PointerEvent::Up { button: PointerButton::Primary, .. } if self.drag_title => {
                self.drag_title = false;
                self.forward_title(InputEvent::Pointer(in_field(p)));
                EventResult::Handled
            }
            _ => self.body_pointer(p),
        }
    }

    fn body_pointer(&mut self, p: PointerEvent) -> EventResult {
        match p {
            PointerEvent::Down { x, y, button: PointerButton::Primary, click_count, modifiers } => {
                if self.doc.blocks().is_empty() {
                    return EventResult::Handled;
                }
                self.preedit = None;
                let (pos, aff) = self.hit(x, y);
                self.group = None;
                self.goal_x = None;
                match click_count {
                    2 => self.select_word_at(pos),
                    n if n >= 3 => self.select_paragraph_at(pos),
                    _ => {
                        if modifiers.shift {
                            self.sel.focus = pos;
                        } else {
                            self.sel = Selection::caret(pos);
                        }
                        self.aff = aff;
                        self.dragging = true;
                    }
                }
                self.changed_view();
                EventResult::Handled
            }
            PointerEvent::Down { .. } => EventResult::NotHandled,
            PointerEvent::Move { x, y } => {
                if self.dragging && !self.doc.blocks().is_empty() {
                    let (pos, aff) = self.hit(x, y);
                    if pos != self.sel.focus {
                        self.sel.focus = pos;
                        self.aff = aff;
                        self.changed_view();
                    }
                }
                EventResult::Handled
            }
            PointerEvent::Up { button, .. } => {
                if button == PointerButton::Primary {
                    self.dragging = false;
                    EventResult::Handled
                } else {
                    EventResult::NotHandled
                }
            }
        }
    }

    // ================================================================ time and saving

    /// Lets the editor act on time (autosave). The host calls it at
    /// [`Editor::next_deadline`] at the latest.
    pub fn poll(&mut self) {
        if let Some(d) = self.next_deadline() {
            if self.now() >= d {
                self.request_save();
            }
        }
    }

    /// When the editor next needs [`Editor::poll`], on the host clock.
    pub fn next_deadline(&self) -> Option<Duration> {
        if self.field.is_some() || self.read_only.is_some() || !self.has_unsaved_changes() || self.state_in_flight() {
            return None;
        }
        let mut d = self.last_edit.unwrap_or_default() + AUTOSAVE_DELAY;
        if self.failure.is_some() {
            if let Some(t) = self.failed_at {
                d = d.max(t + RETRY_DELAY);
            }
        }
        Some(d)
    }

    fn now(&self) -> Duration {
        self.host.clock.now()
    }

    /// Whether a save of what is in memory now (body and title) is in flight.
    fn state_in_flight(&self) -> bool {
        let title = self.title_to_save();
        self.in_flight.iter().any(|(_, s, t)| *s == self.state && *t == title)
    }

    /// The title as it is saved: trimmed of leading and trailing spaces (T4).
    fn title_to_save(&self) -> String {
        match &self.title_field {
            Some(f) => f.doc.blocks().first().map(|b| b.text_str().trim().to_string()).unwrap_or_default(),
            None => String::new(),
        }
    }

    fn request_save(&mut self) {
        if self.field.is_some() || matches!(self.read_only, Some(ReadOnlyReason::NewerFormat { .. })) {
            return;
        }
        let id = SaveId(self.next_save);
        self.next_save += 1;
        let content = self.doc.to_json_string();
        let title = self.title_to_save();
        // The title travels with the body, in the same request, when it is
        // not what the store is known to hold (T2).
        let changed = (title != self.saved_title).then(|| title.clone());
        self.in_flight.push((id, self.state, title));
        self.host.documents.save(SaveRequest { id, content, title: changed });
    }

    /// Ctrl+S.
    fn save_now(&mut self) {
        let allowed = match &self.read_only {
            // "Ctrl+S saves at once" — also an unchanged model (a v1 or raw
            // document is then written in the v2 form).
            None => true,
            // "after saving what it has": text kept after a failed save can
            // still be saved; nothing else is written once editing moved.
            Some(ReadOnlyReason::EditingElsewhere { .. }) => self.has_unsaved_changes(),
            Some(_) => false,
        };
        if !allowed || self.field.is_some() || self.state_in_flight() {
            return;
        }
        self.request_save();
    }

    /// The document service's answer to a save request.
    pub fn save_finished(&mut self, id: SaveId, result: Result<(), SaveError>) {
        let Some(i) = self.in_flight.iter().position(|(s, _, _)| *s == id) else { return };
        let (_, st, title) = self.in_flight.remove(i);
        let newest_ok = self.last_ok.is_none_or(|l| id > l);
        match result {
            Ok(()) => {
                if newest_ok {
                    self.last_ok = Some(id);
                    self.saved_state = st;
                    self.saved_title = title;
                    if self.failure.as_ref().is_some_and(|(f, _)| *f < id) {
                        self.failure = None;
                        self.failed_at = None;
                    }
                }
            }
            Err(e) => {
                if newest_ok {
                    if self.failure.as_ref().is_none_or(|(f, _)| id > *f) {
                        self.failure = Some((id, e.message.clone()));
                    }
                    self.failed_at = Some(self.now());
                    self.notify(Notice {
                        kind: NoticeKind::SaveFailed,
                        message: format!("The document could not be saved ({}). Your changes are kept in the editor.", e.message),
                    });
                }
            }
        }
        self.host.window.request_redraw();
    }

    /// The open document changed in the store because of sync.
    pub fn remote_change(&mut self, change: RemoteChange) {
        let clean = self.field.is_none()
            && !self.has_unsaved_changes()
            && self.in_flight.is_empty()
            && self.failure.is_none()
            && !matches!(self.read_only, Some(ReadOnlyReason::EditingElsewhere { .. }));
        if clean {
            if let Ok((doc, form)) = DocumentV2::read(&change.content) {
                // Nothing local to keep: show the new version. Undo history
                // belongs to the older one and is dropped, so undo cannot
                // silently revert the other device's change.
                self.doc = doc;
                self.text_bytes = self.doc.text_bytes();
                let opened_ro = matches!(self.read_only, Some(ReadOnlyReason::OpenedReadOnly));
                self.read_only = self.read_only_for(&form, opened_ro);
                self.undo.clear();
                self.redo.clear();
                self.group = None;
                self.preedit = None;
                self.state = self.fresh_state();
                self.saved_state = self.state;
                let focus = self.sel.focus;
                let p = self.clamp_position(focus).unwrap_or_else(|| self.doc_start());
                self.sel = Selection::caret(p);
                self.view.retain(&self.doc);
                self.title_a11y_dirty = true;
                self.sync_title_focus();
                self.notify(Notice {
                    kind: NoticeKind::RemoteChange,
                    message: format!("This document was changed on {}; the editor now shows that version.", change.device),
                });
                self.changed_view();
                return;
            }
        }
        self.notify(Notice {
            kind: NoticeKind::RemoteChange,
            message: format!(
                "This document was changed on {}. Your version here is kept; both stay in the document's history once saved.",
                change.device
            ),
        });
    }

    /// Editing was switched to `device`: save what this editor has, then turn
    /// read-only.
    pub fn editing_moved_to(&mut self, device: &str) {
        if self.field.is_some() {
            return;
        }
        self.preedit = None;
        let editable = matches!(self.read_only, None | Some(ReadOnlyReason::EditingElsewhere { .. }));
        if !editable {
            return;
        }
        if self.has_unsaved_changes() && !self.state_in_flight() {
            self.request_save();
        }
        self.read_only = Some(ReadOnlyReason::EditingElsewhere { device: device.to_string() });
        // The title is now text, not a field (T7).
        self.title_a11y_dirty = true;
        self.sync_title_focus();
        self.redraw();
    }

    /// The host revealed or hid a personal-data value: re-ask
    /// [`crate::host::PiiDisplay`].
    pub fn pii_display_changed(&mut self) {
        self.pii_cache.borrow_mut().clear();
        self.view.invalidate_layouts();
        self.view.invalidate_a11y();
        if let Some(f) = self.title_field.as_mut() {
            f.pii_display_changed();
            self.title_a11y_dirty = true;
        }
        self.redraw();
    }

    /// The user wants to close the editor.
    pub fn request_close(&mut self) -> CloseRequest {
        if self.field.is_some() || !self.has_unsaved_changes() {
            return CloseRequest::Ready;
        }
        match &self.read_only {
            None | Some(ReadOnlyReason::EditingElsewhere { .. }) => {}
            Some(_) => return CloseRequest::Ready,
        }
        if let Some((f, _)) = &self.failure {
            if !self.in_flight.iter().any(|(i, _, _)| i > f) {
                return CloseRequest::ConfirmUnsaved;
            }
        }
        if !self.state_in_flight() {
            self.request_save();
        }
        CloseRequest::Saving
    }

    // ================================================================ view

    /// The editor's size in logical pixels, and the display scale factor.
    pub fn set_viewport(&mut self, width: f64, height: f64, scale: f64) {
        self.viewport = (width, height, scale);
        if self.field.is_none() {
            self.view.set_width(Some(((width as f32) - 2.0 * PAD).max(40.0)));
        }
        let th = self.title_h as f64;
        if let Some(f) = self.title_field.as_mut() {
            f.set_viewport(width, th, scale);
        }
        self.host.window.request_redraw();
    }

    /// The height of the editor's view (the page — title, then body —
    /// scrolls in it).
    fn body_h(&self) -> f32 {
        self.viewport.1 as f32
    }

    /// The body's scroll in its own coordinates: the page's scroll less the
    /// title above it (negative while the title is in view).
    fn body_scroll(&self) -> f32 {
        self.scroll_y - self.title_h
    }

    fn preedit_at(&self) -> Option<(BlockId, usize)> {
        self.preedit.as_ref()?;
        let (a, _) = self.ordered();
        Some((a.block, a.offset))
    }

    /// The colours to paint with (the host's theme). The editor starts with
    /// [`Palette::DARK`]; the host passes its own after opening and again
    /// whenever its theme changes. Only painting changes: nothing is laid
    /// out again.
    pub fn set_palette(&mut self, palette: Palette) {
        if let Some(f) = self.title_field.as_mut() {
            f.set_palette(palette);
        }
        if self.palette != palette {
            self.palette = palette;
            self.host.window.request_redraw();
        }
    }

    /// The colours the editor paints with.
    pub fn palette(&self) -> Palette {
        self.palette
    }

    /// Scrolls the text by `dy` logical pixels (positive: towards the end of
    /// the document), within the document: the top of the text cannot go
    /// below the top of the editor, nor its end above the bottom. The caret
    /// does not move, and the view stops following it until it moves or
    /// the text is edited (a mouse wheel over the editor).
    pub fn scroll_by(&mut self, dy: f64) {
        if dy == 0.0 || !dy.is_finite() {
            return;
        }
        let max = self.max_scroll();
        let y = (self.scroll_y + dy as f32).clamp(0.0, max);
        self.follow_caret = false;
        if y != self.scroll_y {
            self.scroll_y = y;
            self.host.window.request_redraw();
        }
    }

    /// How far the text is scrolled: the document y at the editor's top,
    /// in logical pixels.
    pub fn scroll_offset(&self) -> f64 {
        self.scroll_y as f64
    }

    /// The furthest the text scrolls: its height (estimated where not laid
    /// out yet) less the editor's.
    fn max_scroll(&mut self) -> f32 {
        let mut bottom = 0.0f32;
        self.view.walk(&self.doc, |_, _, y, h| {
            bottom = y + h;
            true
        });
        (bottom + PAD + self.title_h - self.body_h()).max(0.0)
    }

    /// Lays out what changed and paints the editor: a document editor's
    /// page is its title (T1 revised), then its body, scrolled together.
    pub fn paint(&mut self, scene: &mut vello::Scene) {
        if self.follow_caret {
            self.ensure_caret_visible();
        }
        self.paint_body(scene);
        if self.title_field.is_some() {
            self.paint_title(scene);
        }
        if self.a11y_active && self.a11y_dirty {
            let u = self.build_a11y(false);
            self.host.accessibility.update(u);
        }
    }

    /// The title, at the top of the page: the field (or, read-only, the
    /// title as text), the *Untitled* placeholder while it is empty, and a
    /// thin rule under it. Painted in the field's coordinates, then placed
    /// where the scroll puts it.
    fn paint_title(&mut self, scene: &mut vello::Scene) {
        self.sync_title_focus();
        let pal = self.palette;
        let (vw, th, s) = (self.viewport.0, self.title_h as f64, self.scroll_y as f64);
        if s >= th {
            return; // scrolled out of view
        }
        // The field reports its input-method area in its own coordinates;
        // the window forwarder moves it by the scroll.
        if self.title_dy.get() != -s {
            self.title_dy.set(-s);
            if let Some(f) = self.title_field.as_mut() {
                f.ime_area = None;
            }
        }
        let mut sub = std::mem::take(&mut self.body_scene);
        sub.reset();
        let Some(field) = self.title_field.as_mut() else { return };
        field.paint(&mut sub);
        if field.doc.blocks().first().is_none_or(|b| b.text_str().is_empty()) && field.preedit.is_none() {
            let l = self.title_placeholder.get_or_insert_with(|| field.view.plain_layout(a11y::TITLE_PLACEHOLDER, TITLE_HEADING));
            view::draw_layout(&mut sub, l, PAD as f64, PAD as f64, |_| pal.muted);
        }
        view::fill(&mut sub, pal.placeholder_border, PAD as f64, th - 1.0, (vw - PAD as f64).max(PAD as f64), th);
        scene.append(&sub, Some(vello::kurbo::Affine::translate((0.0, -s))));
        self.body_scene = sub;
        // The provenance accent runs along the page's edge, title included.
        view::fill(scene, self.accent(), 0.0, 0.0, 3.0, th - s);
    }

    /// Provenance accent (ch. 12): owned blue, external warm red,
    /// undetermined grey.
    fn accent(&self) -> vello::peniko::Color {
        match self.trust {
            Trust::Owned => self.palette.owned,
            Trust::External => self.palette.external,
            Trust::Undetermined => self.palette.undetermined,
        }
    }

    /// Paints the editor's background and the body, below the title.
    fn paint_body(&mut self, scene: &mut vello::Scene) {
        let (vw, _, _) = self.viewport;
        let (vw, vh) = (vw as f32, self.body_h());
        let pal = self.palette;
        view::fill(scene, pal.background, 0.0, 0.0, vw as f64, vh as f64);
        let accent = self.accent();
        if self.field.is_none() {
            view::fill(scene, accent, 0.0, 0.0, 3.0, vh as f64);
        }
        let scroll = self.body_scroll();
        // Which units are on screen depends on their heights, which are only
        // known once laid out: lay out what looks visible, then look again.
        let mut visible: Vec<(usize, usize, f32)> = vec![];
        for _ in 0..64 {
            visible.clear();
            self.view.walk(&self.doc, |b, u, y, h| {
                if y > scroll + vh {
                    return false;
                }
                if y + h >= scroll {
                    visible.push((b, u, y));
                }
                true
            });
            let need: Vec<(usize, usize)> =
                visible.iter().filter(|(b, u, _)| !self.view.has_layout(&self.doc, *b, *u)).map(|(b, u, _)| (*b, *u)).collect();
            if need.is_empty() {
                break;
            }
            self.with_env(|v, env, doc| {
                for (b, u) in need {
                    v.ensure_layout(env, doc, b, u);
                }
            });
        }
        let (sa, sb) = self.ordered();
        let (ia, ib) = (self.doc.index_of(sa.block), self.doc.index_of(sb.block));
        let pre = self.preedit_at();
        let pre_text = self.preedit.as_ref().map(|(s, _)| s.clone());
        let mask = self.field == Some(FieldKind::Password);
        // A single-line field's text, scrolled sideways, is cut at its
        // padding (T9), as an HTML input's.
        let clip = self.field.is_some();
        if clip {
            let r = vello::kurbo::Rect::new((PAD - 2.0) as f64, 0.0, (vw - PAD + 2.0).max(PAD) as f64, vh as f64);
            scene.push_layer(vello::peniko::Fill::NonZero, vello::peniko::Mix::Normal, 1.0, vello::kurbo::Affine::IDENTITY, &r);
        }
        for (b, u, y) in visible {
            let y = y - scroll;
            let block = self.doc.blocks()[b].share();
            let pii = &*self.host.pii;
            let cache = &self.pii_cache;
            let resolve = |id: &str| shown(token_display(pii, cache, id));
            let mut env = Env {
                resolve: &resolve,
                preedit: pre.zip(pre_text.as_deref()).map(|((bid, off), s)| (bid, off, s)),
                mask,
                markdown: self.field.is_none(),
            };
            let ul = self.view.ensure_layout(&mut env, &self.doc, b, u);
            let x0 = (PAD - self.scroll_x) as f64;
            let y0 = y as f64;
            if block.kind() != BlockKind::Text {
                view::placeholder_box(scene, &pal, x0 - 4.0, y0, x0 + ul.layout.width() as f64 + 8.0, y0 + ul.layout.height() as f64);
            }
            // selection: opaque highlight, then (after the text) the selected
            // glyphs again in the selection's text colour
            let mut sel_rects: Vec<(f64, f64, f64, f64)> = vec![];
            if let (Some(ia), Some(ib)) = (ia, ib) {
                if sa != sb && b >= ia && b <= ib {
                    let a = if b == ia { sa.offset } else { 0 };
                    let e = if b == ib { sb.offset } else { usize::MAX };
                    if block.kind() == BlockKind::Text {
                        sel_rects = View::selection_rects(ul, a, e)
                            .into_iter()
                            .map(|(rx0, ry0, rx1, ry1)| (rx0 as f64, ry0 as f64, rx1 as f64, ry1 as f64))
                            .collect();
                    } else if b > ia && b < ib {
                        sel_rects = vec![(0.0, 0.0, ul.layout.width() as f64, ul.layout.height() as f64)];
                    }
                    for &(rx0, ry0, rx1, ry1) in &sel_rects {
                        view::fill(scene, pal.selection, x0 + rx0, y0 + ry0, x0 + rx1, y0 + ry1);
                    }
                }
            }
            let base = if block.kind() == BlockKind::Text { pal.text } else { pal.muted };
            view::draw_layout(scene, &ul.layout, x0, y0, |ink| pal.ink(ink, base));
            view::draw_layout_clipped(scene, &ul.layout, x0, y0, &sel_rects, pal.selection_text);
            if let Some((p0, p1)) = ul.map.preedit_range() {
                // underline the composition
                let a = parley::Cursor::from_byte_index(&ul.layout, p0, Affinity::Downstream).geometry(&ul.layout, 1.0);
                let e = parley::Cursor::from_byte_index(&ul.layout, p1, Affinity::Upstream).geometry(&ul.layout, 1.0);
                let (lx, rx) = (a.x0.min(e.x0), a.x0.max(e.x0));
                view::fill(scene, pal.text, x0 + lx, y0 + a.y1 - 2.0, x0 + rx, y0 + a.y1 - 1.0);
            }
        }
        if clip {
            scene.pop_layer();
        }
        // caret (the title field draws its own while it holds it)
        if self.focused && !self.in_title() {
            if let Some(r) = self.body_caret_rect() {
                if self.read_only.is_none() || sa == sb {
                    view::fill(scene, pal.caret, r.x0, r.y0, r.x1, r.y1);
                }
                let area = r;
                if self.read_only.is_none() && self.ime_area != Some(area) {
                    self.ime_area = Some(area);
                    self.host.window.set_ime_cursor_area(area);
                }
            }
        }
    }

    /// The editor's whole accessibility tree (for the platform adapter's
    /// initial request). Incremental updates go to the
    /// [`crate::host::AccessibilitySink`].
    pub fn accessibility_tree(&mut self) -> accesskit::TreeUpdate {
        self.a11y_active = true;
        self.build_a11y(true)
    }

    /// The accessibility tree: whole (`full`), or an update holding the text
    /// input (selection, children) and the runs not sent yet. A unit's run is
    /// built once and kept until the unit changes, so typing in a large
    /// document sends one run, not the document.
    fn build_a11y(&mut self, full: bool) -> accesskit::TreeUpdate {
        self.a11y_dirty = false;
        let mask = self.field == Some(FieldKind::Password);
        let n = self.doc.blocks().len();
        let mut children = vec![];
        let mut send: std::collections::HashSet<u64> = std::collections::HashSet::new();
        for idx in 0..n {
            let block = self.doc.blocks()[idx].share();
            let nu = self.view.unit_count(&self.doc, idx);
            for u in 0..nu {
                let last_overall = idx + 1 == n && u + 1 == nu;
                let span = self.view.block_view(&self.doc, idx).units[u].span;
                let nl = !last_overall && (span.term > 0 || u + 1 == nu);
                let fresh = self.view.block_view(&self.doc, idx).units[u].a11y.as_ref().is_none_or(|a| a.nl != nl);
                if fresh {
                    let map = if block.kind() == BlockKind::Text {
                        let pii = &*self.host.pii;
                        let cache = &self.pii_cache;
                        let resolve = |id: &str| shown(token_display(pii, cache, id));
                        text::DisplayMap::build(
                            text::DisplayInput {
                                text: block.text_str(),
                                start: span.start,
                                end: span.end,
                                mask,
                                preedit: None,
                                marks: "",
                            },
                            &resolve,
                        )
                    } else {
                        let t = view::placeholder_text(&block);
                        let l = t.len();
                        text::DisplayMap {
                            text: t,
                            segs: vec![text::Seg { m0: 0, m1: 0, d0: 0, d1: l, kind: text::SegKind::Atomic }],
                            m_start: 0,
                            m_end: 0,
                            has_tokens: false,
                        }
                    };
                    let mut t = map.text.clone();
                    if nl {
                        t.push('\n');
                    }
                    let id = self.view.alloc_a11y_id();
                    self.view.block_view(&self.doc, idx).units[u].a11y =
                        Some(Box::new(view::A11yRun { id, map, run: a11y::Run::new(t), nl, sent: false }));
                }
                let a = self.view.block_view(&self.doc, idx).units[u].a11y.as_mut().expect("built");
                children.push(accesskit::NodeId(a.id));
                if full || !a.sent {
                    send.insert(a.id);
                    a.sent = true;
                }
            }
        }
        let locate = |p: Position, aff: Affinity, ed: &mut Editor| -> (accesskit::NodeId, usize) {
            let Some(idx) = ed.doc.index_of(p.block) else { return (a11y::EMPTY_RUN_ID, 0) };
            let u = ed.view.unit_index(&ed.doc, idx, p.offset, aff);
            match ed.view.block_view(&ed.doc, idx).units[u].a11y.as_ref() {
                Some(a) => (accesskit::NodeId(a.id), a.run.char_index(a.map.m2d(p.offset))),
                None => (a11y::EMPTY_RUN_ID, 0),
            }
        };
        let anchor = locate(self.sel.anchor, Affinity::Downstream, self);
        let focus = locate(self.sel.focus, self.aff, self);
        let role = match self.field {
            Some(FieldKind::Password) => accesskit::Role::PasswordInput,
            Some(FieldKind::SingleLine) => accesskit::Role::TextInput,
            None => accesskit::Role::MultilineTextInput,
        };
        let empty_run = a11y::Run::new(String::new());
        let mut runs: Vec<(accesskit::NodeId, &a11y::Run)> = vec![];
        if children.is_empty() {
            children.push(a11y::EMPTY_RUN_ID);
            runs.push((a11y::EMPTY_RUN_ID, &empty_run));
        }
        for b in self.doc.blocks() {
            if send.is_empty() {
                break;
            }
            if let Some(bv) = self.view.peek(b) {
                for uv in &bv.units {
                    if let Some(a) = &uv.a11y {
                        if send.remove(&a.id) {
                            runs.push((accesskit::NodeId(a.id), &a.run));
                        }
                    }
                }
            }
        }
        // The title field's nodes (T6), when they changed or the whole tree
        // is asked for: the title as shown, with its caret and selection.
        let title_run = match self.title_field.as_deref() {
            Some(f) if full || self.title_a11y_dirty => {
                let t = f.field_text();
                let pii = &*self.host.pii;
                let cache = &self.pii_cache;
                let resolve = |id: &str| shown(token_display(pii, cache, id));
                let map = text::DisplayMap::build(
                    text::DisplayInput { text: &t, start: 0, end: t.len(), mask: false, preedit: None, marks: "" },
                    &resolve,
                );
                let run = a11y::Run::new(map.text.clone());
                let sel = (run.char_index(map.m2d(f.sel.anchor.offset)), run.char_index(map.m2d(f.sel.focus.offset)));
                Some((run, sel))
            }
            _ => None,
        };
        self.title_a11y_dirty = false;
        let editable_title = self.title_editable();
        let title = title_run.as_ref().map(|(run, sel)| a11y::TitleNode { run, editable: editable_title, selection: *sel });
        let title_text = self.title_field.as_deref().map(|f| f.field_text()).unwrap_or_default();
        let label = (self.field.is_none() && !title_text.is_empty()).then_some(title_text.as_str());
        a11y::build(a11y::TreeInput {
            tree_id: self.config.accessibility_tree,
            role,
            label,
            read_only: self.read_only.is_some(),
            focused: self.focused,
            children,
            runs,
            selection: Some((anchor, focus)),
            full,
            has_title: self.title_field.is_some(),
            title,
            title_focused: self.in_title(),
        })
    }

    // ================================================================ reading state

    /// The in-memory model: committed text only (no input-method pre-edit).
    pub fn document(&self) -> &DocumentV2 {
        &self.doc
    }

    /// Whether an input-method composition (a pre-edit) is showing, in the
    /// text or in a document editor's title field. A host that acts on Enter
    /// or Esc itself (a field's Create, a dialog's Cancel) leaves them to the
    /// input method while this is true (U35; THREAD-name N7).
    pub fn is_composing(&self) -> bool {
        self.preedit.is_some() || self.title_field.as_deref().is_some_and(|f| f.preedit.is_some())
    }

    /// The text of a field opened with [`Editor::open_field`].
    pub fn field_text(&self) -> String {
        self.doc.blocks().first().map(|b| b.text_str().to_string()).unwrap_or_default()
    }

    // ---------------------------------------------------------------- title
    // Title contract T1–T7 (`doc/plans/TITLE-edit.md`, spec ch. 12, document
    // window, *Title*). The title field is a single-line field on this same
    // component (`open_field`), so caret, selection, clipboard, input methods
    // and bidi follow the same rules as the body's text (T6); this editor
    // routes input to it, saves its title with the body (T2) and exposes it
    // in its accessibility tree.

    /// What the title's text is drawn with (T1 revised: 1.1 × the body size,
    /// semi-bold).
    pub fn title_font(&self) -> TitleFont {
        TitleFont { size: view::TITLE_SIZE, weight: view::TITLE_WEIGHT, body_size: view::FONT_SIZE }
    }

    /// The document's title as it is in memory: exactly what was typed (it is
    /// trimmed only in what is saved, T4). Never contains a line break.
    pub fn title(&self) -> String {
        self.title_field.as_deref().map(|f| f.field_text()).unwrap_or_default()
    }

    /// Which part holds the caret: the title field or the body. Keyboard and
    /// input-method input go to that part; [`Editor::caret_rect`] is its caret.
    pub fn part(&self) -> EditorPart {
        if self.in_title() {
            EditorPart::Title
        } else {
            EditorPart::Body
        }
    }

    /// Puts the caret in the title field with all its text selected — how a
    /// new document starts (T3). In a read-only state the title is not a field
    /// (T7): this changes nothing that could be edited.
    pub fn focus_title(&mut self) {
        if !self.title_editable() {
            return;
        }
        self.preedit = None;
        self.dragging = false;
        if let Some(f) = self.title_field.as_mut() {
            f.select_all();
        }
        self.enter_part(EditorPart::Title);
        self.title_a11y_dirty = true;
        self.changed_view();
    }

    /// The title field's selection, as byte offsets into [`Editor::title`]:
    /// `(anchor, focus)`; equal for a plain caret.
    pub fn title_selection(&self) -> (usize, usize) {
        self.title_field.as_deref().map(|f| (f.sel.anchor.offset, f.sel.focus.offset)).unwrap_or((0, 0))
    }

    /// Moves the caret into the title field and sets its selection. Endpoints
    /// inside a grapheme cluster are moved to a boundary (as
    /// [`Editor::set_selection`] does in the body).
    pub fn set_title_selection(&mut self, anchor: usize, focus: usize) {
        if !self.title_editable() {
            return;
        }
        self.preedit = None;
        self.dragging = false;
        if let Some(f) = self.title_field.as_mut() {
            let block = f.doc_start().block;
            f.set_selection(Selection { anchor: Position { block, offset: anchor }, focus: Position { block, offset: focus } });
        }
        self.enter_part(EditorPart::Title);
        self.title_a11y_dirty = true;
        self.changed_view();
    }

    /// Whether the title is an editable field here: a document editor that
    /// is not read-only (T7).
    fn title_editable(&self) -> bool {
        self.title_field.is_some() && self.read_only.is_none()
    }

    /// Whether input goes to the title field.
    fn in_title(&self) -> bool {
        self.part == EditorPart::Title && self.title_editable()
    }

    /// Moves keyboard input to `part`.
    fn enter_part(&mut self, part: EditorPart) {
        if self.part != part {
            self.part = part;
            // Each part reports its own input-method area: report it again.
            self.ime_area = None;
            if let Some(f) = self.title_field.as_mut() {
                f.ime_area = None;
            }
            self.group = None;
            self.a11y_dirty = true;
        }
        self.sync_title_focus();
    }

    /// The title field has keyboard focus exactly when the editor has it
    /// and input goes to the title.
    fn sync_title_focus(&mut self) {
        let want = self.focused && self.in_title();
        if let Some(f) = self.title_field.as_mut() {
            if f.focused != want {
                f.handle_event(InputEvent::Focus(want));
            }
        }
    }

    /// Runs `act` on the title field, then takes what it changed: an edit of
    /// the title is an edit of the document (unsaved until saved, saved by
    /// the same delayed save, T2 and T5), its notices are this editor's, and
    /// its accessible node is sent again.
    fn title_do<R>(&mut self, act: impl FnOnce(&mut Editor) -> R) -> Option<R> {
        let f = self.title_field.as_mut()?;
        let (before, sel_before) = (f.state, f.sel);
        let r = act(f);
        let edited = f.state != before;
        let moved = f.sel != sel_before;
        let notices = f.take_notices();
        for n in notices {
            self.notify(n);
        }
        if edited {
            self.last_edit = Some(self.now());
            self.group = None;
        }
        if edited || moved {
            // The page scrolls back to the title, as to a caret in the body.
            self.follow_caret = true;
        }
        self.title_a11y_dirty = true;
        self.a11y_dirty = true;
        Some(r)
    }

    fn forward_title(&mut self, event: InputEvent) -> EventResult {
        self.title_do(|f| f.handle_event(event)).unwrap_or(EventResult::NotHandled)
    }

    /// A key while the title holds the caret. Enter and Tab go to the start
    /// of the body (T3; Enter never types a line break into the title, T4);
    /// Ctrl+S saves the document; Shift+Tab, Ctrl+Tab and F6 are the host's
    /// (no keyboard trap); everything else is the field's.
    fn title_key(&mut self, k: KeyEvent) -> EventResult {
        let m = k.modifiers;
        let cmd = m.ctrl || m.meta;
        let composing = self.title_field.as_deref().is_some_and(|f| f.preedit.is_some());
        match &k.key {
            Key::Named(NamedKey::Enter) => {
                // An Enter while a pre-edit shows belongs to the input method.
                if !composing {
                    self.title_to_body();
                }
                EventResult::Handled
            }
            Key::Named(NamedKey::Tab) if !m.shift && !cmd && !m.alt => {
                if !composing {
                    self.title_to_body();
                }
                EventResult::Handled
            }
            Key::Named(NamedKey::Tab) | Key::Named(NamedKey::F6) => EventResult::NotHandled,
            Key::Character(c) if cmd && !m.alt && c.to_lowercase() == "s" => {
                self.save_now();
                EventResult::Handled
            }
            _ => self.forward_title(InputEvent::Key(k)),
        }
    }

    /// From the title to the start of the body.
    fn title_to_body(&mut self) {
        self.sel = Selection::caret(self.doc_start());
        self.aff = Affinity::Downstream;
        self.goal_x = None;
        self.enter_part(EditorPart::Body);
        self.changed_view();
    }

    pub fn selection(&self) -> Selection {
        self.sel
    }

    /// Moves the selection. Endpoints inside a grapheme cluster or inside a
    /// personal-data token are moved to a boundary. The caret is then in the
    /// body ([`EditorPart::Body`], title contract).
    pub fn set_selection(&mut self, selection: Selection) {
        self.enter_part(EditorPart::Body);
        let Some(sel) = self.normalize(selection) else { return };
        self.sel = sel;
        self.aff = Affinity::Downstream;
        self.goal_x = None;
        self.group = None;
        self.changed_view();
    }

    /// Where the caret is drawn, in the editor's coordinate space (laid out
    /// with the current viewport): the title field's while it holds the
    /// caret, else the body's (the page scrolls both, T1 revised).
    pub fn caret_rect(&mut self) -> Option<Rect> {
        if self.follow_caret {
            self.ensure_caret_visible();
        }
        if self.in_title() {
            let s = self.scroll_y as f64;
            return self.title_field.as_mut()?.caret_rect().map(|r| Rect { y0: r.y0 - s, y1: r.y1 - s, ..r });
        }
        self.body_caret_rect()
    }

    /// The body's caret, in the editor's coordinates.
    fn body_caret_rect(&mut self) -> Option<Rect> {
        let (x0, y0, y1) = self.caret_doc_box()?;
        let s = self.body_scroll() as f64;
        let x0 = x0 - self.scroll_x as f64;
        Some(Rect { x0, y0: y0 - s, x1: x0 + view::CARET_W as f64, y1: y1 - s })
    }

    /// The caret's top and bottom on the page (title, then body).
    fn caret_page_span(&mut self) -> Option<(f32, f32)> {
        if self.in_title() {
            let r = self.title_field.as_mut()?.caret_rect()?;
            return Some((r.y0 as f32, r.y1 as f32));
        }
        let (_, y0, y1) = self.caret_doc_box()?;
        Some((y0 as f32 + self.title_h, y1 as f32 + self.title_h))
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only.is_some()
    }

    pub fn read_only_reason(&self) -> Option<ReadOnlyReason> {
        self.read_only.clone()
    }

    pub fn save_status(&self) -> SaveStatus {
        if let Some((fid, msg)) = &self.failure {
            if self.in_flight.iter().any(|(i, _, _)| i > fid) {
                return SaveStatus::Saving;
            }
            if self.has_unsaved_changes() {
                return SaveStatus::Failed { message: msg.clone() };
            }
        }
        if !self.has_unsaved_changes() {
            SaveStatus::Saved
        } else if self.state_in_flight() {
            SaveStatus::Saving
        } else {
            SaveStatus::Unsaved
        }
    }

    /// Whether the in-memory model differs from what the store is known to
    /// hold: the body, or the title as it would be saved (T5).
    pub fn has_unsaved_changes(&self) -> bool {
        self.state != self.saved_state || (self.title_field.is_some() && self.title_to_save() != self.saved_title)
    }

    /// Notices since the last call.
    pub fn take_notices(&mut self) -> Vec<Notice> {
        std::mem::take(&mut self.notices)
    }

    // ================================================================ positions

    fn doc_start(&self) -> Position {
        match self.doc.blocks().first() {
            Some(b) => Position { block: b.id(), offset: 0 },
            None => Position { block: NO_BLOCK, offset: 0 },
        }
    }

    fn doc_end(&self) -> Position {
        match self.doc.blocks().last() {
            Some(b) => Position { block: b.id(), offset: if b.kind() == BlockKind::Text { b.text_str().len() } else { 0 } },
            None => Position { block: NO_BLOCK, offset: 0 },
        }
    }

    fn idx(&self, p: Position) -> Option<usize> {
        self.doc.index_of(p.block)
    }

    fn key_of(&self, p: Position) -> (usize, usize) {
        (self.idx(p).unwrap_or(0), p.offset)
    }

    /// The selection's ends in document order.
    fn ordered(&self) -> (Position, Position) {
        let (a, f) = (self.sel.anchor, self.sel.focus);
        if self.key_of(a) <= self.key_of(f) {
            (a, f)
        } else {
            (f, a)
        }
    }

    fn clamp_position(&self, p: Position) -> Option<Position> {
        let i = self.idx(p)?;
        let b = &self.doc.blocks()[i];
        let offset = if b.kind() == BlockKind::Text { text::snap_caret(b.text_str(), p.offset, Snap::Nearest, &|id| self.is_token(id)) } else { 0 };
        Some(Position { block: p.block, offset })
    }

    fn snap(&self, p: Position, snap: Snap) -> Option<Position> {
        let i = self.idx(p)?;
        let b = &self.doc.blocks()[i];
        let offset = if b.kind() == BlockKind::Text { text::snap_caret(b.text_str(), p.offset, snap, &|id| self.is_token(id)) } else { 0 };
        Some(Position { block: p.block, offset })
    }

    fn normalize(&self, s: Selection) -> Option<Selection> {
        if self.doc.blocks().is_empty() {
            return None;
        }
        if s.anchor == s.focus {
            let p = self.snap(s.anchor, Snap::Nearest)?;
            return Some(Selection::caret(p));
        }
        let a_first = self.key_of(s.anchor) <= self.key_of(s.focus);
        let (sa, sf) = if a_first { (Snap::Floor, Snap::Ceil) } else { (Snap::Ceil, Snap::Floor) };
        Some(Selection { anchor: self.snap(s.anchor, sa)?, focus: self.snap(s.focus, sf)? })
    }

    fn select_all(&mut self) {
        if self.doc.blocks().is_empty() {
            return;
        }
        self.sel = Selection { anchor: self.doc_start(), focus: self.doc_end() };
        self.aff = Affinity::Upstream;
        self.group = None;
        self.changed_view();
    }

    fn move_to(&mut self, p: Position, aff: Affinity, extend: bool) {
        if extend {
            self.sel.focus = p;
        } else {
            self.sel = Selection::caret(p);
        }
        self.aff = aff;
        self.group = None;
        self.changed_view();
    }

    /// The next caret position in document order (crossing units and blocks).
    fn next_pos(&self, p: Position) -> Position {
        let Some(i) = self.idx(p) else { return p };
        let b = &self.doc.blocks()[i];
        if b.kind() == BlockKind::Text {
            if let Some(n) = text::next_caret(b.text_str(), p.offset, &|id| self.is_token(id)) {
                return Position { block: p.block, offset: n };
            }
        }
        match self.doc.blocks().get(i + 1) {
            Some(nb) => Position { block: nb.id(), offset: 0 },
            None => p,
        }
    }

    fn prev_pos(&self, p: Position) -> Position {
        let Some(i) = self.idx(p) else { return p };
        let b = &self.doc.blocks()[i];
        if b.kind() == BlockKind::Text {
            if let Some(n) = text::prev_caret(b.text_str(), p.offset, &|id| self.is_token(id)) {
                return Position { block: p.block, offset: n };
            }
        }
        if i == 0 {
            return p;
        }
        let pb = &self.doc.blocks()[i - 1];
        Position { block: pb.id(), offset: if pb.kind() == BlockKind::Text { pb.text_str().len() } else { 0 } }
    }

    // ================================================================ movement

    fn env_parts(&mut self) -> (Option<(BlockId, usize)>, Option<String>, bool) {
        let pre = self.preedit_at();
        (pre, self.preedit.as_ref().map(|(s, _)| s.clone()), self.field == Some(FieldKind::Password))
    }

    /// Runs `f` with the view and a layout environment.
    fn with_env<R>(&mut self, f: impl FnOnce(&mut View, &mut Env<'_>, &DocumentV2) -> R) -> R {
        let (pre, pre_text, mask) = self.env_parts();
        let pii = &*self.host.pii;
        let cache = &self.pii_cache;
        let resolve = |id: &str| shown(token_display(pii, cache, id));
        let markdown = self.field.is_none();
        let mut env = Env { resolve: &resolve, preedit: pre.zip(pre_text.as_deref()).map(|((b, o), s)| (b, o, s)), mask, markdown };
        f(&mut self.view, &mut env, &self.doc)
    }

    /// Whether the token form with this record id is a token here.
    fn is_token(&self, id: &str) -> bool {
        shown(token_display(&*self.host.pii, &self.pii_cache, id)).is_some()
    }

    fn caret_line(&mut self) -> Option<LineRef> {
        let p = self.sel.focus;
        let idx = self.idx(p)?;
        let aff = self.aff;
        Some(self.with_env(|view, env, doc| {
            let u = view.unit_for(env, doc, idx, p.offset, aff);
            let ul = view.ensure_layout(env, doc, idx, u);
            let line = View::line_of(ul, p.offset, aff);
            LineRef { block: idx, unit: u, line }
        }))
    }

    /// The caret box in document coordinates (x0, y0, y1).
    fn caret_doc_box(&mut self) -> Option<(f64, f64, f64)> {
        let p = self.sel.focus;
        let Some(idx) = self.idx(p) else {
            return Some((PAD as f64, PAD as f64, (PAD + self.view.line_h) as f64));
        };
        let aff = self.aff;
        let pre_cursor = self.preedit.as_ref().map(|(s, c)| c.map(|(_, e)| e).unwrap_or(s.len()));
        let composing_here = self.preedit_at().is_some_and(|(b, o)| b == p.block && o == p.offset);
        let (u, x, y0, y1) = self.with_env(|view, env, doc| {
            let u = view.unit_for(env, doc, idx, p.offset, aff);
            let ul = view.ensure_layout(env, doc, idx, u);
            let (x, y0, y1) = View::caret_box(ul, p.offset, aff, if composing_here { pre_cursor } else { None });
            (u, x, y0, y1)
        });
        let uy = self.view.unit_y(&self.doc, idx, u);
        Some(((PAD + x) as f64, (uy + y0) as f64, (uy + y1) as f64))
    }

    fn caret_x(&mut self) -> f32 {
        self.caret_doc_box().map(|(x, _, _)| x as f32 - PAD).unwrap_or(0.0)
    }

    /// Scrolls the page so the caret (title's or body's) is in view.
    fn ensure_caret_visible(&mut self) {
        let Some((y0, y1)) = self.caret_page_span() else { return };
        let vh = self.body_h();
        if y0 < self.scroll_y {
            let top = y0 - PAD;
            // On the body's first line, show the title above it too.
            self.scroll_y = if top <= self.title_h { 0.0 } else { top };
        } else if y1 > self.scroll_y + vh {
            self.scroll_y = (y1 - vh + PAD).max(0.0);
        }
        if self.field.is_some() {
            self.ensure_caret_visible_sideways();
        }
    }

    /// A single-line field scrolls sideways so its caret is inside it, and
    /// never further than its text needs (T9).
    fn ensure_caret_visible_sideways(&mut self) {
        let Some((x, _, _)) = self.caret_doc_box() else { return };
        let x = x as f32;
        let (left, right) = (PAD, (self.viewport.0 as f32 - PAD).max(PAD + view::CARET_W));
        if x - self.scroll_x < left {
            self.scroll_x = x - left;
        } else if x + view::CARET_W - self.scroll_x > right {
            self.scroll_x = x + view::CARET_W - right;
        }
        let text_w = if self.doc.blocks().is_empty() {
            0.0
        } else {
            self.with_env(|v, env, doc| v.ensure_layout(env, doc, 0, 0).layout.width())
        };
        let max = (PAD + text_w + view::CARET_W - right).max(0.0);
        self.scroll_x = self.scroll_x.clamp(0.0, max);
    }

    fn move_horizontal(&mut self, right: bool, extend: bool, word: bool) {
        self.goal_x = None;
        if self.doc.blocks().is_empty() {
            return;
        }
        if !extend && self.sel.anchor != self.sel.focus && !word {
            let (a, b) = self.ordered();
            let p = if right { b } else { a };
            self.move_to(p, Affinity::Downstream, false);
            return;
        }
        let focus = self.sel.focus;
        if word {
            let (p, aff) = self.word_target(right);
            self.move_to(p, aff, extend);
            return;
        }
        let Some(line) = self.caret_line() else { return };
        let (places, has_rtl, para_rtl) = self.with_env(|v, env, doc| v.line_places(env, doc, line));
        if !has_rtl {
            let p = if right { self.next_pos(focus) } else { self.prev_pos(focus) };
            self.move_to(p, Affinity::Downstream, extend);
            return;
        }
        let cur_x = self.caret_x();
        let cand = places
            .iter()
            .filter(|pl| if right { pl.x > cur_x + 0.01 } else { pl.x < cur_x - 0.01 })
            .min_by(|a, b| {
                let da = (a.x - cur_x).abs();
                let db = (b.x - cur_x).abs();
                da.partial_cmp(&db)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then((a.offset as isize - focus.offset as isize).abs().cmp(&(b.offset as isize - focus.offset as isize).abs()))
            })
            .copied();
        let target = match cand {
            Some(pl) => Some((line.block, pl)),
            None => {
                let forward = right != para_rtl;
                let next = self.with_env(|v, env, doc| if forward { v.next_line(env, doc, line) } else { v.prev_line(env, doc, line) });
                next.map(|nl| {
                    let (pl, _, _) = self.with_env(|v, env, doc| v.line_places(env, doc, nl));
                    let pick = if right {
                        pl.iter().copied().min_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
                    } else {
                        pl.iter().copied().max_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
                    };
                    (nl.block, pick.expect("a line has a place"))
                })
            }
        };
        if let Some((b, pl)) = target {
            let id = self.doc.blocks()[b].id();
            self.move_to(Position { block: id, offset: pl.offset }, pl.aff, extend);
        }
    }

    /// Ctrl+Left / Ctrl+Right: parley's word movement (`next_visual_word` /
    /// `previous_visual_word`, what its `PlainEditor` does), visual like the
    /// arrows, landing on a cluster boundary outside tokens. At the edge of a
    /// unit it continues into the next piece of a long paragraph, or steps
    /// over the line break to the next paragraph or block.
    fn word_target(&mut self, right: bool) -> (Position, Affinity) {
        let mut pos = self.sel.focus;
        let mut aff = self.aff;
        for _ in 0..8 {
            let Some(idx) = self.idx(pos) else { break };
            let b = self.doc.blocks()[idx].share();
            if b.kind() != BlockKind::Text {
                let p = if right { self.next_pos(pos) } else { self.prev_pos(pos) };
                return (p, Affinity::Downstream);
            }
            let (m, naff, moved, para_rtl, span, more_after, more_before) = self.with_env(|v, env, doc| {
                let u = v.unit_for(env, doc, idx, pos.offset, aff);
                let (span, n) = {
                    let bv = v.block_view(doc, idx);
                    (bv.units[u].span, bv.units.len())
                };
                let more_before = u > 0 && v.block_view(doc, idx).units[u - 1].span.term == 0;
                let ul = v.ensure_layout(env, doc, idx, u);
                let d = ul.map.m2d(pos.offset);
                let cur = parley::Cursor::from_byte_index(&ul.layout, d, aff);
                let next = if right { cur.next_visual_word(&ul.layout) } else { cur.previous_visual_word(&ul.layout) };
                let moved = next.index() != cur.index();
                (ul.map.d2m(next.index()), next.affinity(), moved, ul.layout.is_rtl(), span, u + 1 < n && span.term == 0, more_before)
            });
            if moved {
                let m = text::snap_caret(b.text_str(), m, Snap::Nearest, &|id| self.is_token(id));
                if m != pos.offset {
                    return (Position { block: pos.block, offset: m }, naff);
                }
            }
            let forward = right != para_rtl;
            if forward && more_after {
                pos = Position { block: pos.block, offset: span.end };
                aff = Affinity::Downstream;
                if span.end != self.sel.focus.offset {
                    return (pos, aff);
                }
                continue;
            }
            if !forward && more_before && pos.offset == span.start {
                aff = Affinity::Upstream;
                continue;
            }
            let p = if forward { self.next_pos(pos) } else { self.prev_pos(pos) };
            return (p, Affinity::Downstream);
        }
        (pos, aff)
    }

    fn move_vertical(&mut self, down: bool, extend: bool, times: usize) {
        if self.doc.blocks().is_empty() {
            return;
        }
        let goal = match self.goal_x {
            Some(g) => g,
            None => self.caret_x(),
        };
        for _ in 0..times {
            let Some(line) = self.caret_line() else { return };
            let next = self.with_env(|v, env, doc| if down { v.next_line(env, doc, line) } else { v.prev_line(env, doc, line) });
            match next {
                None => {
                    let p = if down { self.doc_end() } else { self.doc_start() };
                    self.move_to(p, if down { Affinity::Upstream } else { Affinity::Downstream }, extend);
                    break;
                }
                Some(nl) => {
                    let (places, _, _) = self.with_env(|v, env, doc| v.line_places(env, doc, nl));
                    let best: Place = places
                        .iter()
                        .copied()
                        .min_by(|a, b| {
                            (a.x - goal).abs().partial_cmp(&(b.x - goal).abs()).unwrap_or(std::cmp::Ordering::Equal).then(a.offset.cmp(&b.offset))
                        })
                        .expect("a line has a place");
                    let id = self.doc.blocks()[nl.block].id();
                    self.move_to(Position { block: id, offset: best.offset }, best.aff, extend);
                }
            }
        }
        self.goal_x = Some(goal);
    }

    fn line_edge(&mut self, end: bool, extend: bool) {
        self.goal_x = None;
        let Some(line) = self.caret_line() else { return };
        let (lo, hi) = self.with_env(|v, env, doc| v.line_range(env, doc, line));
        let b = self.doc.blocks()[line.block].share();
        let (off, aff) = if b.kind() != BlockKind::Text {
            (0, Affinity::Downstream)
        } else if end {
            (text::snap_caret(b.text_str(), hi, Snap::Floor, &|id| self.is_token(id)), Affinity::Upstream)
        } else {
            (text::snap_caret(b.text_str(), lo, Snap::Ceil, &|id| self.is_token(id)), Affinity::Downstream)
        };
        self.move_to(Position { block: b.id(), offset: off }, aff, extend);
    }

    /// Enter (spec ch. 12, *Lists continue on Enter*). Inside or at the end of
    /// a non-empty list item, the line break is followed by the next item's
    /// marker: same indentation (and block quotes), same bullet, the next
    /// number with the same delimiter, an unchecked box for a task item, the
    /// same spacing. On an empty item, the item's line is emptied instead
    /// (indentation included: U33). Shift+Enter, a line that is not an item,
    /// a caret before the item's content, a selection, and a line inside a
    /// fenced code block give a plain line break. Either way it is one edit:
    /// one undo step. List items are recognised by `md::list_item`, the rule
    /// drawing uses.
    fn enter(&mut self, shift: bool) {
        let p = self.sel.focus;
        let plain = |ed: &mut Editor| ed.insert_typed("\n", EditKind::Other);
        if shift || self.sel.anchor != p || !self.check_editable_quiet() {
            return plain(self);
        }
        let Some(i) = self.idx(p) else { return plain(self) };
        let block = self.doc.blocks()[i].share();
        if block.kind() != BlockKind::Text {
            return plain(self);
        }
        let t = block.text_str();
        let (ls, le) = Self::paragraph(t, p.offset);
        if Self::inside_fence(&t[..ls]) {
            return plain(self);
        }
        let line = &t.as_bytes()[ls..le];
        let Some(item) = crate::md::list_item(line) else { return plain(self) };
        let at = p.offset - ls;
        if at < item.content_start {
            return plain(self);
        }
        if line[item.content_start..].iter().all(|b| matches!(b, b' ' | b'\t')) {
            // an empty item: the list ends here, the line becomes empty
            let started = self.begin(EditKind::Other);
            self.replace_text(i, ls, le, "");
            self.sel = Selection::caret(Position { block: p.block, offset: ls });
            self.aff = Affinity::Downstream;
            self.finish(EditKind::Other, started);
            return;
        }
        let mut next = String::from("\n");
        next.push_str(&t[ls..ls + item.marker_start]);
        match item.kind {
            crate::md::ListKind::Bullet(c) => next.push(c as char),
            crate::md::ListKind::Ordered(n, d) => {
                next.push_str(&(n + 1).to_string());
                next.push(d as char);
            }
        }
        let gap = |g: &[u8]| if g.is_empty() { " ".to_string() } else { String::from_utf8_lossy(g).into_owned() };
        next.push_str(&gap(&item.gap));
        if let Some((_, _, after)) = &item.task {
            next.push_str("[ ]");
            next.push_str(&gap(after));
        }
        self.insert_typed(&next, EditKind::Other);
    }

    /// Whether a fenced code block is open at the end of `before` (the text of
    /// the block up to the caret's line): ` ``` ` or `~~~` with up to three
    /// spaces of indentation, closed by a fence of the same character, at
    /// least as long, with nothing after it (U37). An unclosed fence runs to
    /// the end of the block.
    fn inside_fence(before: &str) -> bool {
        let mut open: Option<(u8, usize)> = None;
        for line in before.split('\n') {
            let Some((c, run, info)) = crate::md::fence(line.as_bytes()) else { continue };
            match open {
                None => open = Some((c, run)),
                Some((oc, orun)) if c == oc && run >= orun && !info => open = None,
                Some(_) => {}
            }
        }
        open.is_some()
    }

    /// Like `check_editable`, without a notice: the edit that follows says it.
    fn check_editable_quiet(&self) -> bool {
        self.read_only.is_none()
    }

    /// The paragraph (stored range between line breaks) holding `off`.
    fn paragraph(text: &str, off: usize) -> (usize, usize) {
        let start = text[..off].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let mut end = text[off..].find('\n').map(|i| off + i).unwrap_or(text.len());
        if end > start && text.as_bytes()[end - 1] == b'\r' && end < text.len() {
            end -= 1;
        }
        (start, end.max(off.min(end)))
    }

    fn word_right(&self, p: Position) -> Position {
        let Some(i) = self.idx(p) else { return p };
        let b = &self.doc.blocks()[i];
        if b.kind() != BlockKind::Text {
            return self.next_pos(p);
        }
        let t = b.text_str();
        let (ps, pe) = Self::paragraph(t, p.offset);
        if p.offset >= pe {
            return self.next_pos(p);
        }
        let mut target = pe;
        for (k, w) in t[ps..pe].split_word_bound_indices() {
            let we = ps + k + w.len();
            if we > p.offset && w.chars().any(|c| c.is_alphanumeric()) {
                target = we;
                break;
            }
        }
        Position { block: p.block, offset: text::snap_caret(t, target, Snap::Ceil, &|id| self.is_token(id)).max(p.offset) }
    }

    fn word_left(&self, p: Position) -> Position {
        let Some(i) = self.idx(p) else { return p };
        let b = &self.doc.blocks()[i];
        if b.kind() != BlockKind::Text {
            return self.prev_pos(p);
        }
        let t = b.text_str();
        let (ps, _) = Self::paragraph(t, p.offset);
        if p.offset <= ps {
            return self.prev_pos(p);
        }
        let mut target = ps;
        for (k, w) in t[ps..p.offset].split_word_bound_indices() {
            if w.chars().any(|c| c.is_alphanumeric()) {
                target = ps + k;
            }
        }
        Position { block: p.block, offset: text::snap_caret(t, target, Snap::Floor, &|id| self.is_token(id)).min(p.offset) }
    }

    fn select_word_at(&mut self, p: Position) {
        let Some(i) = self.idx(p) else { return };
        let b = self.doc.blocks()[i].share();
        if b.kind() != BlockKind::Text {
            self.sel = Selection::caret(p);
            return;
        }
        let t = b.text_str();
        let (ps, pe) = Self::paragraph(t, p.offset);
        let mut range = (p.offset, p.offset);
        for (k, w) in t[ps..pe].split_word_bound_indices() {
            let (ws, we) = (ps + k, ps + k + w.len());
            if ws <= p.offset && p.offset < we {
                range = (ws, we);
                break;
            }
        }
        let a = text::snap_caret(t, range.0, Snap::Floor, &|id| self.is_token(id));
        let e = text::snap_caret(t, range.1, Snap::Ceil, &|id| self.is_token(id));
        self.sel = Selection { anchor: Position { block: p.block, offset: a }, focus: Position { block: p.block, offset: e } };
        self.aff = Affinity::Upstream;
    }

    fn select_paragraph_at(&mut self, p: Position) {
        let Some(i) = self.idx(p) else { return };
        let b = self.doc.blocks()[i].share();
        if b.kind() != BlockKind::Text {
            self.sel = Selection::caret(p);
            return;
        }
        let (ps, pe) = Self::paragraph(b.text_str(), p.offset);
        self.sel = Selection { anchor: Position { block: p.block, offset: ps }, focus: Position { block: p.block, offset: pe } };
        self.aff = Affinity::Upstream;
    }

    fn hit(&mut self, x: f64, y: f64) -> (Position, Affinity) {
        let doc_y = y as f32 + self.body_scroll();
        let scroll_x = self.scroll_x;
        let mut at = (0, 0, 0.0f32);
        // Heights are estimates until laid out (and a long paragraph's rest is
        // cut on demand): lay out the unit found, then look again.
        for _ in 0..64 {
            let mut found = None;
            let mut last = (0, 0, 0.0f32);
            self.view.walk(&self.doc, |b, u, uy, h| {
                last = (b, u, uy);
                if doc_y < uy + h {
                    found = Some((b, u, uy));
                    return false;
                }
                true
            });
            at = found.unwrap_or(last);
            let (b, u, _) = at;
            if self.view.has_layout(&self.doc, b, u) {
                break;
            }
            self.with_env(|v, env, doc| {
                v.ensure_layout(env, doc, b, u);
            });
        }
        let (b, u, uy) = at;
        let block = self.doc.blocks()[b].share();
        let (off, aff) = self.with_env(|v, env, doc| {
            let ul = v.ensure_layout(env, doc, b, u);
            View::hit(ul, &block, x as f32 + scroll_x - PAD, (doc_y - uy).max(0.0), env.resolve)
        });
        (Position { block: block.id(), offset: off }, aff)
    }

    // ================================================================ editing

    fn fresh_state(&mut self) -> u64 {
        let s = self.next_state;
        self.next_state += 1;
        s
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { blocks: self.doc.blocks().iter().map(Block::share).collect(), sel: self.sel, aff: self.aff, state: self.state }
    }

    fn restore(&mut self, s: Snapshot) {
        *self.doc.blocks_mut() = s.blocks;
        self.sel = s.sel;
        self.aff = s.aff;
        self.state = s.state;
        self.text_bytes = self.doc.text_bytes();
        self.view.retain(&self.doc);
        self.last_edit = Some(self.now());
        self.changed_view();
    }

    /// In a read-only state, says so and returns false.
    fn check_editable(&mut self) -> bool {
        let Some(r) = &self.read_only else { return true };
        let notice = match r {
            ReadOnlyReason::EditingElsewhere { device } => Notice {
                kind: NoticeKind::EditingElsewhere,
                message: format!("This document is now being edited on {device}. Nothing was changed here."),
            },
            ReadOnlyReason::OpenedReadOnly => Notice {
                kind: NoticeKind::ReadOnly,
                message: "This document is open read-only. Nothing was changed.".into(),
            },
            ReadOnlyReason::NewerFormat { .. } => Notice {
                kind: NoticeKind::ReadOnly,
                message: "This document uses a newer format and is read-only in this version. Nothing was changed.".into(),
            },
            ReadOnlyReason::TooLarge { .. } => Notice {
                kind: NoticeKind::ReadOnly,
                message: "This document holds more than 1 MB of text and is read-only. Nothing was changed.".into(),
            },
        };
        self.notify(notice);
        false
    }

    /// Queues a notice for the host. The same notice twice in a row is kept
    /// once, and the queue is bounded if the host never takes it.
    fn notify(&mut self, n: Notice) {
        if self.notices.last() == Some(&n) {
            return;
        }
        if self.notices.len() >= MAX_NOTICES {
            self.notices.remove(0);
        }
        self.notices.push(n);
    }

    fn refuse_input(&mut self) {
        let n = Notice {
            kind: NoticeKind::InputRefused,
            message: "This input would take the document over 1 MB of text; it was not inserted.".into(),
        };
        self.notify(n);
    }

    /// Starts an edit: an undo step unless it continues a typing burst.
    fn begin(&mut self, kind: EditKind) -> (bool, u64) {
        let now = self.now();
        let continues = kind != EditKind::Other
            && matches!(self.group, Some((k, t)) if k == kind && now.saturating_sub(t) < BURST_GAP);
        if !continues {
            self.undo.push(self.snapshot());
            if self.undo.len() > MAX_UNDO {
                self.undo.remove(0);
            }
        }
        (!continues, self.mutations)
    }

    fn finish(&mut self, kind: EditKind, started: (bool, u64)) {
        let (pushed, before) = started;
        if self.mutations != before {
            self.redo.clear();
            self.state = self.fresh_state();
            let now = self.now();
            self.last_edit = Some(now);
            self.group = Some((kind, now));
            self.goal_x = None;
            self.changed_view();
        } else if pushed {
            self.undo.pop();
        }
    }

    /// The caret, the selection or the text changed: redraw, and the view
    /// follows the caret again.
    fn changed_view(&mut self) {
        self.follow_caret = true;
        self.redraw();
    }

    /// Something shown changed, not the caret: redraw where the view is.
    fn redraw(&mut self) {
        self.a11y_dirty = true;
        self.host.window.request_redraw();
    }

    /// Replaces `a..b` of the text of block `idx` by `new`, byte for byte.
    /// Returns the end of the inserted text.
    fn replace_text(&mut self, idx: usize, a: usize, b: usize, new: &str) -> usize {
        if a == b && new.is_empty() {
            return a;
        }
        self.mutations += 1;
        let block = &mut self.doc.blocks_mut()[idx];
        let old_rev = block.rev();
        block.text_mut().replace_range(a..b, new);
        self.text_bytes = self.text_bytes + new.len() - (b - a);
        let block = &self.doc.blocks()[idx];
        self.view.edited(block, old_rev, a, b - a, new.len());
        a + new.len()
    }

    /// Where text typed at `p` goes: a text block and an offset. Typing on a
    /// block that is not text (or in an empty document) creates a text block
    /// after it.
    fn text_target(&mut self, p: Position) -> (usize, usize) {
        match self.idx(p) {
            Some(i) if self.doc.blocks()[i].kind() == BlockKind::Text => (i, p.offset),
            Some(i) => {
                self.insert_block(i + 1);
                (i + 1, 0)
            }
            None => {
                self.insert_block(0);
                (0, 0)
            }
        }
    }

    fn insert_block(&mut self, at: usize) {
        let mut id = BlockId::random();
        while self.doc.index_of(id).is_some() || id == NO_BLOCK {
            id = BlockId::random();
        }
        self.doc.blocks_mut().insert(at, Block::new_text(id, String::new()));
        self.mutations += 1;
    }

    /// Text bytes inside the selection.
    fn selected_text_bytes(&self) -> usize {
        let (a, b) = self.ordered();
        let (Some(ia), Some(ib)) = (self.idx(a), self.idx(b)) else { return 0 };
        let mut n = 0;
        for i in ia..=ib {
            let blk = &self.doc.blocks()[i];
            if blk.kind() != BlockKind::Text {
                continue;
            }
            let len = blk.text_str().len();
            let s = if i == ia { a.offset.min(len) } else { 0 };
            let e = if i == ib { b.offset.min(len) } else { len };
            n += e.saturating_sub(s);
        }
        n
    }

    /// Deletes the stored range between two positions (document order).
    /// Blocks that are not text are never removed by a range: only by the
    /// explicit command. Returns where the caret goes.
    fn delete_range(&mut self, a: Position, b: Position) -> Position {
        let (Some(ia), Some(ib)) = (self.idx(a), self.idx(b)) else { return a };
        if ia == ib {
            if self.doc.blocks()[ia].kind() == BlockKind::Text && a.offset < b.offset {
                self.replace_text(ia, a.offset, b.offset, "");
            }
            return a;
        }
        let a_text = self.doc.blocks()[ia].kind() == BlockKind::Text;
        let b_text = self.doc.blocks()[ib].kind() == BlockKind::Text;
        let all_text = (ia..=ib).all(|i| self.doc.blocks()[i].kind() == BlockKind::Text);
        if all_text {
            // Merge: a's head + b's tail into a; the blocks between and b go.
            let tail = self.doc.blocks()[ib].text_str()[b.offset..].to_string();
            let len = self.doc.blocks()[ia].text_str().len();
            self.replace_text(ia, a.offset, len, &tail);
            self.doc.blocks_mut().drain(ia + 1..=ib);
            self.mutations += 1;
            self.text_bytes = self.doc.text_bytes();
            return a;
        }
        if b_text && b.offset > 0 {
            self.replace_text(ib, 0, b.offset, "");
        }
        // Text blocks strictly between, wholly selected, go.
        let mut removed = 0;
        let mut i = ia + 1;
        while i < ib - removed {
            if self.doc.blocks()[i].kind() == BlockKind::Text {
                self.doc.blocks_mut().remove(i);
                self.mutations += 1;
                removed += 1;
            } else {
                i += 1;
            }
        }
        if a_text {
            let len = self.doc.blocks()[ia].text_str().len();
            if a.offset < len {
                self.replace_text(ia, a.offset, len, "");
            }
        }
        self.text_bytes = self.doc.text_bytes();
        if !a_text && b_text {
            return Position { block: b.block, offset: 0 };
        }
        a
    }

    /// Inserts typed (or committed, or pasted) text at the selection.
    fn insert_typed(&mut self, s: &str, kind: EditKind) {
        self.insert_parts(&[Part::Text(s.to_string())], kind);
    }

    fn insert_parts(&mut self, parts: &[Part], kind: EditKind) {
        if !self.check_editable() {
            return;
        }
        let single_line = self.field.is_some();
        let mut s = String::new();
        let mut linked: Vec<String> = vec![];
        for p in parts {
            match p {
                Part::Text(t) => {
                    if single_line {
                        s.push_str(&t.replace("\r\n", " ").replace(['\n', '\r'], " "));
                    } else {
                        s.push_str(t);
                    }
                }
                Part::Token(id) => {
                    if !linked.contains(id) {
                        linked.push(id.clone());
                    }
                    s.push_str(text::TOKEN_OPEN);
                    s.push_str(id);
                    s.push(']');
                }
            }
        }
        if s.is_empty() {
            return;
        }
        let removed = self.selected_text_bytes();
        if self.text_bytes - removed + s.len() > MAX_EDITABLE_TEXT_BYTES {
            self.refuse_input();
            return;
        }
        if !linked.is_empty() {
            // Tokens from a genuine Sovereign copy (W17-checked by the caller)
            // enter this document: the host adds it to their records' sources,
            // so they are tokens here at once.
            self.host.pii.link_to_document(&linked);
            self.pii_cache.borrow_mut().clear();
            self.view.invalidate_layouts();
        }
        let started = self.begin(kind);
        let (a, b) = self.ordered();
        let at = if a != b { self.delete_range(a, b) } else { a };
        let (idx, off) = self.text_target(at);
        let end = self.replace_text(idx, off, off, &s);
        let id = self.doc.blocks()[idx].id();
        self.sel = Selection::caret(Position { block: id, offset: end });
        self.aff = Affinity::Downstream;
        self.finish(kind, started);
    }

    fn delete(&mut self, forward: bool, word: bool) {
        if !self.check_editable() {
            return;
        }
        if self.doc.blocks().is_empty() {
            return;
        }
        let kind = if forward { EditKind::DeleteForward } else { EditKind::DeleteBack };
        let (a, b) = self.ordered();
        if a != b {
            let started = self.begin(EditKind::Other);
            let at = self.delete_range(a, b);
            self.sel = Selection::caret(at);
            self.finish(EditKind::Other, started);
            return;
        }
        let p = self.sel.focus;
        let Some(i) = self.idx(p) else { return };
        if self.doc.blocks()[i].kind() != BlockKind::Text {
            // Only the explicit command removes a block that is not text.
            return;
        }
        let len = self.doc.blocks()[i].text_str().len();
        let started = self.begin(kind);
        if forward {
            if p.offset >= len {
                // Join the next block if it is text.
                if self.doc.blocks().get(i + 1).is_some_and(|n| n.kind() == BlockKind::Text) {
                    let nb = self.doc.blocks()[i + 1].id();
                    self.delete_range(p, Position { block: nb, offset: 0 });
                }
            } else {
                let e = if word { self.word_right(p).offset } else { self.next_pos(p).offset };
                let e = text::snap_caret(self.doc.blocks()[i].text_str(), e.max(p.offset), Snap::Ceil, &|id| self.is_token(id));
                if e > p.offset {
                    self.replace_text(i, p.offset, e, "");
                }
            }
            self.sel = Selection::caret(p);
        } else if p.offset == 0 {
            if i > 0 && self.doc.blocks()[i - 1].kind() == BlockKind::Text {
                let pb = self.doc.blocks()[i - 1].id();
                let pl = self.doc.blocks()[i - 1].text_str().len();
                let at = Position { block: pb, offset: pl };
                self.delete_range(at, p);
                self.sel = Selection::caret(at);
            }
        } else {
            let s = if word { self.word_left(p).offset } else { self.prev_pos(p).offset };
            let s = text::snap_caret(self.doc.blocks()[i].text_str(), s.min(p.offset), Snap::Floor, &|id| self.is_token(id));
            if s < p.offset {
                self.replace_text(i, s, p.offset, "");
            }
            self.sel = Selection::caret(Position { block: p.block, offset: s });
        }
        self.aff = Affinity::Downstream;
        self.finish(kind, started);
    }

    fn remove_block(&mut self, id: BlockId) {
        if self.field.is_some() || !self.check_editable() {
            return;
        }
        let Some(i) = self.doc.index_of(id) else { return };
        let started = self.begin(EditKind::Other);
        self.doc.blocks_mut().remove(i);
        self.mutations += 1;
        self.text_bytes = self.doc.text_bytes();
        let fix = |p: Position, ed: &Editor| -> Position {
            if p.block == id {
                match ed.doc.blocks().get(i).or(ed.doc.blocks().last()) {
                    Some(b) => Position { block: b.id(), offset: 0 },
                    None => Position { block: NO_BLOCK, offset: 0 },
                }
            } else {
                p
            }
        };
        let a = fix(self.sel.anchor, self);
        let f = fix(self.sel.focus, self);
        self.sel = Selection { anchor: a, focus: f };
        self.view.retain(&self.doc);
        self.finish(EditKind::Other, started);
    }

    fn undo(&mut self) {
        if !self.check_editable() {
            return;
        }
        self.preedit = None;
        self.group = None;
        if let Some(s) = self.undo.pop() {
            let cur = self.snapshot();
            self.redo.push(cur);
            self.restore(s);
        }
    }

    fn redo(&mut self) {
        if !self.check_editable() {
            return;
        }
        self.preedit = None;
        self.group = None;
        if let Some(s) = self.redo.pop() {
            let cur = self.snapshot();
            self.undo.push(cur);
            self.restore(s);
        }
    }

    // ================================================================ clipboard

    /// Writes the selection to the clipboard. Returns whether it did.
    fn copy(&mut self) -> bool {
        if self.field == Some(FieldKind::Password) {
            return false;
        }
        let (a, b) = self.ordered();
        if a == b {
            return false;
        }
        let (Some(ia), Some(ib)) = (self.idx(a), self.idx(b)) else { return false };
        let mut plain = String::new();
        let mut parts: Vec<Part> = vec![];
        let mut has_token = false;
        let mut revealed = false;
        let mut first = true;
        for i in ia..=ib {
            let blk = self.doc.blocks()[i].share();
            if blk.kind() != BlockKind::Text {
                continue;
            }
            let t = blk.text_str();
            let s = if i == ia { a.offset.min(t.len()) } else { 0 };
            let e = if i == ib { b.offset.min(t.len()) } else { t.len() };
            if !first {
                plain.push('\n');
                parts.push(Part::Text("\n".into()));
            }
            first = false;
            let mut at = s;
            for (ta, tb) in text::tokens_in(t, s, e, e, &|id| self.is_token(id)) {
                if ta > at {
                    plain.push_str(&t[at..ta]);
                    parts.push(Part::Text(t[at..ta].to_string()));
                }
                let id = text::token_id(t, ta, tb).to_string();
                let d = token_display(&*self.host.pii, &self.pii_cache, &id);
                revealed |= matches!(d, TokenDisplay::Revealed(_));
                plain.push_str(&shown(d).unwrap_or_default());
                parts.push(Part::Token(id));
                has_token = true;
                at = tb;
            }
            if e > at {
                plain.push_str(&t[at..e]);
                parts.push(Part::Text(t[at..e].to_string()));
            }
        }
        let sovereign = if has_token {
            Some(clip::make_payload(&parts, &plain))
        } else {
            clip::forget();
            None
        };
        self.host.clipboard.write(ClipboardWrite {
            text: plain,
            sovereign,
            privacy: ALL_EXCLUDED,
            clear_after: revealed.then_some(REVEALED_CLEAR_AFTER),
        });
        true
    }

    fn cut(&mut self) {
        if !self.check_editable() {
            return;
        }
        let (a, b) = self.ordered();
        if a == b || !self.copy() {
            return;
        }
        let started = self.begin(EditKind::Other);
        let at = self.delete_range(a, b);
        self.sel = Selection::caret(at);
        self.finish(EditKind::Other, started);
    }

    fn paste(&mut self) {
        if !self.check_editable() {
            return;
        }
        let Some(content) = self.host.clipboard.read() else { return };
        let parts = clip::accept(&content).unwrap_or_else(|| vec![Part::Text(content.text.clone())]);
        self.insert_parts(&parts, EditKind::Other);
    }
}

impl fmt::Debug for Editor {
    /// Does not print document text (guarantee 3).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Editor")
            .field("blocks", &self.doc.blocks().len())
            .field("text_bytes", &self.text_bytes)
            .field("read_only", &self.read_only.is_some())
            .field("field", &self.field)
            .field("unsaved", &self.has_unsaved_changes())
            .field("focused", &self.focused)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use super::*;
    use crate::host::*;

    struct Docs(Rc<RefCell<Vec<SaveRequest>>>);
    impl DocumentService for Docs {
        fn save(&mut self, r: SaveRequest) {
            self.0.borrow_mut().push(r);
        }
    }
    struct Clip(Rc<RefCell<Option<ClipboardContent>>>);
    impl crate::host::Clipboard for Clip {
        fn read(&mut self) -> Option<ClipboardContent> {
            self.0.borrow().clone()
        }
        fn write(&mut self, w: ClipboardWrite) {
            *self.0.borrow_mut() = Some(ClipboardContent { text: w.text, sovereign: w.sovereign });
        }
    }
    struct Sink(Rc<RefCell<Vec<accesskit::TreeUpdate>>>);
    impl AccessibilitySink for Sink {
        fn update(&mut self, u: accesskit::TreeUpdate) {
            self.0.borrow_mut().push(u);
        }
    }
    struct Time(Rc<Cell<Duration>>);
    impl Clock for Time {
        fn now(&self) -> Duration {
            self.0.get()
        }
    }
    struct Pii(Rc<RefCell<Vec<Vec<String>>>>);
    impl PiiDisplay for Pii {
        fn display(&self, _: &str) -> TokenDisplay {
            TokenDisplay::Label("<label>".into())
        }
        fn link_to_document(&mut self, ids: &[String]) {
            self.0.borrow_mut().push(ids.to_vec());
        }
    }
    struct Win;
    impl WindowHost for Win {
        fn set_ime_cursor_area(&mut self, _: Rect) {}
        fn request_redraw(&mut self) {}
    }

    const FONT: &[u8] = include_bytes!("../tests/fixtures/Cantarell-Regular.ttf");

    struct T {
        ed: Editor,
        saves: Rc<RefCell<Vec<SaveRequest>>>,
        clip: Rc<RefCell<Option<ClipboardContent>>>,
        a11y: Rc<RefCell<Vec<accesskit::TreeUpdate>>>,
        links: Rc<RefCell<Vec<Vec<String>>>>,
    }

    fn open(content: &str) -> T {
        let saves = Rc::default();
        let clip = Rc::default();
        let a11y = Rc::default();
        let links = Rc::default();
        let host = Host {
            documents: Box::new(Docs(Rc::clone(&saves))),
            clipboard: Box::new(Clip(Rc::clone(&clip))),
            accessibility: Box::new(Sink(Rc::clone(&a11y))),
            clock: Box::new(Time(Rc::new(Cell::new(Duration::from_secs(5))))),
            pii: Box::new(Pii(Rc::clone(&links))),
            window: Box::new(Win),
        };
        let config = EditorConfig { fonts: Fonts::Only(vec![FONT.to_vec()]), accessibility_tree: accesskit::TreeId::ROOT };
        let doc = OpenDocument { title: "t".into(), content: content.into(), trust: Trust::Owned, mode: OpenMode::Edit };
        let mut ed = Editor::open(doc, config, host).unwrap();
        ed.set_viewport(800.0, 600.0, 1.0);
        ed.handle_event(InputEvent::Focus(true));
        T { ed, saves, clip, a11y, links }
    }

    fn paint(ed: &mut Editor) {
        ed.paint(&mut vello::Scene::new());
    }

    fn text(ed: &Editor) -> String {
        ed.document().blocks()[0].text().unwrap().to_string()
    }

    // Typing in a large document sends one run to the accessibility sink, and
    // the updates applied in order give the same text as a fresh snapshot.
    #[test]
    fn a11y_updates_send_only_what_changed() {
        let body: String = (0..200).map(|i| format!("line {i}\n")).collect();
        let mut t = open(&body);
        let mut tree = accesskit_consumer::Tree::new(t.ed.accessibility_tree(), true);
        let first = t.ed.document().blocks()[0].id();
        t.ed.set_selection(Selection::caret(Position { block: first, offset: 3 }));
        t.ed.handle_event(InputEvent::Text("X".into()));
        paint(&mut t.ed);
        let updates: Vec<_> = t.a11y.borrow_mut().drain(..).collect();
        assert_eq!(updates.len(), 1);
        assert!(updates[0].nodes.len() <= 3, "{} nodes sent", updates[0].nodes.len());
        struct H;
        impl accesskit_consumer::TreeChangeHandler for H {
            fn node_added(&mut self, _: &accesskit_consumer::Node) {}
            fn node_updated(&mut self, _: &accesskit_consumer::Node, _: &accesskit_consumer::Node) {}
            fn focus_moved(&mut self, _: Option<&accesskit_consumer::Node>, _: Option<&accesskit_consumer::Node>) {}
            fn node_removed(&mut self, _: &accesskit_consumer::Node) {}
        }
        for u in updates {
            tree.update_and_process_changes(u, &mut H);
        }
        let live = tree.state().focus().unwrap().document_range().text();
        let fresh = accesskit_consumer::Tree::new(t.ed.accessibility_tree(), true);
        assert_eq!(live, fresh.state().focus().unwrap().document_range().text());
        assert!(live.starts_with("linXe 0\nline 1\n"));
    }

    // A payload altered after this editor wrote it is not trusted: the plain
    // text is pasted (W17).
    #[test]
    fn an_altered_clipboard_payload_is_ignored() {
        let mut t = open(r#"{"format":"sovereign.doc","v":2,"blocks":[{"type":"text","id":"0123456789abcdef0123456789abcdef","markup":"markdown","text":"a[pii:pii_record:1]b"}]}"#);
        t.ed.command(Command::SelectAll);
        t.ed.command(Command::Copy);
        let mut c = t.clip.borrow().clone().unwrap();
        c.sovereign = c.sovereign.map(|p| p.replace("pii_record:1", "pii_record:2"));
        *t.clip.borrow_mut() = Some(c);
        let b = t.ed.document().blocks()[0].id();
        t.ed.set_selection(Selection::caret(Position { block: b, offset: 0 }));
        t.ed.command(Command::Paste);
        assert_eq!(text(&t.ed), "a<label>ba[pii:pii_record:1]b");
        assert!(t.links.borrow().is_empty(), "an altered payload links nothing");
        // the genuine payload links its record
        t.ed.command(Command::SelectAll);
        t.ed.command(Command::Copy);
        t.ed.command(Command::Paste);
        assert_eq!(t.links.borrow().as_slice(), &[vec!["pii_record:1".to_string()]]);
    }

    // A remote change to a document with nothing unsaved is shown; one with
    // unsaved local text keeps the local text.
    #[test]
    fn remote_change_is_shown_only_when_nothing_local_would_be_lost() {
        let doc = |s: &str| {
            format!(r#"{{"format":"sovereign.doc","v":2,"blocks":[{{"type":"text","id":"0123456789abcdef0123456789abcdef","markup":"markdown","text":"{s}"}}]}}"#)
        };
        let mut t = open(&doc("a"));
        t.ed.remote_change(RemoteChange { content: doc("remote"), device: "phone".into() });
        assert_eq!(text(&t.ed), "remote");
        assert!(!t.ed.has_unsaved_changes());
        t.ed.handle_event(InputEvent::Text("!".into()));
        t.ed.remote_change(RemoteChange { content: doc("again"), device: "phone".into() });
        assert!(text(&t.ed).contains('!'));
        assert!(t.saves.borrow().is_empty());
    }

    // Ctrl+Right follows parley's word movement: visual on a line with
    // right-to-left text, like the arrows — the caret never goes back, lands
    // on cluster boundaries, and reaches the right end.
    #[test]
    fn ctrl_right_moves_by_words_in_visual_order() {
        let line = "abc אבגד efg שלום עולם xyz";
        let mut t = open(&format!(
            r#"{{"format":"sovereign.doc","v":2,"blocks":[{{"type":"text","id":"0123456789abcdef0123456789abcdef","markup":"markdown","text":"{line}"}}]}}"#
        ));
        let b = t.ed.document().blocks()[0].id();
        t.ed.set_selection(Selection::caret(Position { block: b, offset: 0 }));
        let mut xs = vec![t.ed.caret_rect().unwrap().x0];
        for _ in 0..20 {
            t.ed.handle_event(InputEvent::Key(KeyEvent { key: Key::Named(NamedKey::ArrowRight), modifiers: Modifiers::CTRL }));
            assert!(text::is_grapheme_boundary(line, t.ed.selection().focus.offset));
            xs.push(t.ed.caret_rect().unwrap().x0);
        }
        assert!(xs.windows(2).all(|w| w[1] >= w[0] - 0.01), "{xs:?}");
        let right_end = (0..=line.len())
            .filter(|&o| line.is_char_boundary(o))
            .map(|o| {
                t.ed.set_selection(Selection::caret(Position { block: b, offset: o }));
                t.ed.caret_rect().unwrap().x0
            })
            .fold(f64::MIN, f64::max);
        assert!((xs.last().unwrap() - right_end).abs() < 0.5, "{xs:?} vs {right_end}");
        assert!(xs.windows(2).filter(|w| w[1] > w[0] + 0.01).count() >= 6, "moved by words: {xs:?}");
    }

    // Typing in one paragraph moves the stored offsets of the paragraphs
    // after it; what was laid out for them (and their accessible runs)
    // follows: Home lands on the paragraph's real start, and the exposed
    // caret is where the caret is.
    #[test]
    fn paragraphs_after_an_edit_keep_true_offsets() {
        let mut t = open(r#"{"format":"sovereign.doc","v":2,"blocks":[{"type":"text","id":"0123456789abcdef0123456789abcdef","markup":"markdown","text":"one
two
three"}]}"#);
        let _ = t.ed.accessibility_tree();
        paint(&mut t.ed);
        let b = t.ed.document().blocks()[0].id();
        t.ed.set_selection(Selection::caret(Position { block: b, offset: 1 }));
        t.ed.handle_event(InputEvent::Text("XYZ".into()));
        paint(&mut t.ed);
        let three = text(&t.ed).find("three").unwrap();
        t.ed.set_selection(Selection::caret(Position { block: b, offset: three + 3 }));
        t.ed.handle_event(InputEvent::Key(KeyEvent { key: Key::Named(NamedKey::Home), modifiers: Modifiers::NONE }));
        assert_eq!(t.ed.selection().focus.offset, three);
        t.ed.set_selection(Selection::caret(Position { block: b, offset: three + 2 }));
        let tree = accesskit_consumer::Tree::new(t.ed.accessibility_tree(), true);
        let caret = tree.state().focus().unwrap().text_selection_focus().unwrap().to_global_usv_index();
        assert_eq!(caret, text(&t.ed)[..three + 2].chars().count());
    }

    /// The packed form of `c` in a vello scene's draw data.
    fn packed(c: vello::peniko::Color) -> u32 {
        let mut s = vello::Scene::new();
        view::fill(&mut s, c, 0.0, 0.0, 1.0, 1.0);
        *s.encoding().draw_data.last().unwrap()
    }

    fn painted(ed: &mut Editor) -> Vec<u32> {
        let mut s = vello::Scene::new();
        ed.paint(&mut s);
        s.encoding().draw_data.clone()
    }

    // The host's palette is what is painted: background, text, selection and
    // selected text, caret, provenance accent — and changing it repaints with
    // the new one.
    #[test]
    fn the_host_palette_is_what_is_painted() {
        let mut t = open("hello *world*");
        assert_eq!(t.ed.palette(), Palette::DARK);
        let dark = painted(&mut t.ed);
        assert!(dark.contains(&packed(Palette::DARK.background)));
        assert!(dark.contains(&packed(Palette::DARK.text)));
        assert!(!dark.contains(&packed(Palette::LIGHT.background)));
        t.ed.set_palette(Palette::LIGHT);
        assert_eq!(t.ed.palette(), Palette::LIGHT);
        let light = painted(&mut t.ed);
        for c in [Palette::LIGHT.background, Palette::LIGHT.text, Palette::LIGHT.muted, Palette::LIGHT.caret, Palette::LIGHT.owned] {
            assert!(light.contains(&packed(c)), "{c:?} painted");
        }
        for c in [Palette::DARK.background, Palette::DARK.text, Palette::DARK.caret] {
            assert!(!light.contains(&packed(c)), "{c:?} not painted any more");
        }
        t.ed.command(Command::SelectAll);
        let selected = painted(&mut t.ed);
        assert!(selected.contains(&packed(Palette::LIGHT.selection)));
        // a host colour of its own
        let mut mine = Palette::LIGHT;
        mine.selection_text = vello::peniko::Color::from_rgb8(1, 2, 3);
        t.ed.set_palette(mine);
        assert!(painted(&mut t.ed).contains(&packed(mine.selection_text)));
    }

    fn lines(n: usize) -> String {
        (0..n).map(|i| format!("line {i}\n")).collect()
    }

    // The host scrolls the text (a mouse wheel), within the document; the
    // caret stays where it is and the view no longer jumps back to it, until
    // it moves.
    #[test]
    fn scrolling_is_clamped_and_leaves_the_caret_alone() {
        let mut t = open(&lines(300));
        paint(&mut t.ed);
        let caret = t.ed.selection();
        assert_eq!(t.ed.scroll_offset(), 0.0);
        t.ed.scroll_by(-50.0);
        assert_eq!(t.ed.scroll_offset(), 0.0, "not above the top");
        t.ed.scroll_by(120.0);
        assert_eq!(t.ed.scroll_offset(), 120.0);
        paint(&mut t.ed);
        assert_eq!(t.ed.scroll_offset(), 120.0, "painting does not scroll back to the caret");
        assert_eq!(t.ed.selection(), caret);
        // to the end: the last line is in view, nothing below it
        t.ed.scroll_by(1e9);
        paint(&mut t.ed);
        t.ed.scroll_by(1e9); // heights now laid out where painted
        let end = t.ed.scroll_offset();
        assert!(end > 1000.0, "{end}");
        paint(&mut t.ed);
        let last = t.ed.doc_end();
        t.ed.set_selection(Selection::caret(last));
        let r = t.ed.caret_rect().unwrap();
        assert_eq!(t.ed.scroll_offset(), end, "the end of the text is already in view");
        assert!(r.y0 >= 0.0 && r.y1 <= 600.0, "{r:?}");
        // a caret move brings the view back to the caret
        t.ed.handle_event(InputEvent::Key(KeyEvent { key: Key::Named(NamedKey::Home), modifiers: Modifiers::CTRL }));
        paint(&mut t.ed);
        assert_eq!(t.ed.scroll_offset(), 0.0);
        // a short document does not scroll
        let mut s = open("short");
        s.ed.scroll_by(100.0);
        assert_eq!(s.ed.scroll_offset(), 0.0);
    }

    // Markdown is only drawn: the stored bytes, the selection, the caret
    // places, what a screen reader reads and what is copied are the source's.
    #[test]
    fn markdown_drawing_never_alters_the_source() {
        let src = "# Title\n**bold** and *em* [link](http://x.org) `co*de*` snake_case\n- item\n> quote";
        let mut t = open(src);
        let json_before = t.ed.document().to_json_string();
        let mut tree = accesskit_consumer::Tree::new(t.ed.accessibility_tree(), true);
        paint(&mut t.ed);
        assert_eq!(text(&t.ed), src);
        assert_eq!(t.ed.document().to_json_string(), json_before);
        // the accessible text is the source, markup included
        let fresh = accesskit_consumer::Tree::new(t.ed.accessibility_tree(), true);
        let read = fresh.state().focus().unwrap().document_range().text();
        assert_eq!(read, src);
        let _ = &mut tree;
        // the caret stops at every character of the source, markup included
        let b = t.ed.document().blocks()[0].id();
        t.ed.set_selection(Selection::caret(Position { block: b, offset: 0 }));
        let mut seen = vec![0];
        loop {
            t.ed.handle_event(InputEvent::Key(KeyEvent { key: Key::Named(NamedKey::ArrowRight), modifiers: Modifiers::NONE }));
            let o = t.ed.selection().focus.offset;
            if o == *seen.last().unwrap() {
                break;
            }
            seen.push(o);
        }
        let all: Vec<usize> = (0..=src.len()).filter(|&o| src.is_char_boundary(o)).collect();
        assert_eq!(seen, all);
        // select the bold word with its stars, copy: the source
        let a = src.find("**bold**").unwrap();
        t.ed.set_selection(Selection { anchor: Position { block: b, offset: a }, focus: Position { block: b, offset: a + 8 } });
        paint(&mut t.ed);
        t.ed.command(Command::Copy);
        assert_eq!(t.clip.borrow().as_ref().unwrap().text, "**bold**");
        // typing a heading marker changes the layout, not the text around it
        t.ed.set_selection(Selection::caret(Position { block: b, offset: src.find("- item").unwrap() }));
        t.ed.handle_event(InputEvent::Text("## ".into()));
        paint(&mut t.ed);
        assert!(text(&t.ed).contains("\n## - item\n"));
    }

    // A heading is laid out larger than body text; the rest of the
    // document's line height (used for paging) stays the body's.
    #[test]
    fn headings_are_larger_and_do_not_change_the_body_line_height() {
        let mut t = open("body line\n# Heading\nbody again");
        paint(&mut t.ed);
        let b = t.ed.document().blocks()[0].id();
        let h_at = |t: &mut T, o: usize| {
            t.ed.set_selection(Selection::caret(Position { block: b, offset: o }));
            let r = t.ed.caret_rect().unwrap();
            r.y1 - r.y0
        };
        let body = h_at(&mut t, 2);
        let head = h_at(&mut t, 12);
        let again = h_at(&mut t, 22);
        assert!(head > body * 1.3, "heading caret {head} vs body {body}");
        assert!((again - body).abs() < 0.01);
        assert!((t.ed.view.line_h as f64 - body).abs() < 2.0, "line height {} vs body {body}", t.ed.view.line_h);
    }
}
