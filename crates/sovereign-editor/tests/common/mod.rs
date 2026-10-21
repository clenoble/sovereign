//! Shared fakes and helpers for the C-TEXT suite. The fakes record what the
//! editor does through the host interface; they never decide anything.
#![allow(dead_code)]

pub mod deps;
pub mod session;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

use sovereign_editor::doc::{BlockId, DocumentV2};
use sovereign_editor::host::{
    AccessibilitySink, Clipboard, ClipboardContent, ClipboardWrite, Clock, DocumentService, Host, PiiDisplay,
    SaveError, SaveId, SaveRequest, TokenDisplay, WindowHost,
};
use sovereign_editor::input::{Command, ImeEvent, InputEvent, Key, KeyEvent, Modifiers, NamedKey};
use sovereign_editor::{
    Editor, EditorConfig, Fonts, OpenDocument, OpenMode, Position, Rect, Selection, Trust,
};
use unicode_segmentation::UnicodeSegmentation;

/// Cantarell Regular, SIL Open Font License 1.1 — see tests/fixtures/README.md.
/// Latin only: other scripts lay out with the font's .notdef glyph, which has
/// a positive advance, so caret geometry is still ordered.
pub const TEST_FONT: &[u8] = include_bytes!("../fixtures/Cantarell-Regular.ttf");

// ---------------------------------------------------------------- fakes

#[derive(Default)]
pub struct ClipboardState {
    pub content: Option<ClipboardContent>,
    pub writes: Vec<ClipboardWrite>,
    pub reads: usize,
}

#[derive(Clone, Debug)]
pub struct PiiEntry {
    pub label: String,
    pub value: String,
    pub revealed: bool,
}

/// Handles on everything the editor did through the host.
#[derive(Clone)]
pub struct Fakes {
    pub saves: Rc<RefCell<Vec<SaveRequest>>>,
    pub clipboard: Rc<RefCell<ClipboardState>>,
    pub a11y_updates: Rc<RefCell<Vec<accesskit::TreeUpdate>>>,
    pub clock: Rc<Cell<Duration>>,
    /// Personal-data records, by record id. A record listed here is linked to
    /// the open document (it is among its sources) unless its id is in
    /// `pii_elsewhere`.
    pub pii: Rc<RefCell<HashMap<String, PiiEntry>>>,
    /// Records that exist but whose sources do not include the open document:
    /// their token form is ordinary text here (resolver rule, 2026-10-10).
    pub pii_elsewhere: Rc<RefCell<HashSet<String>>>,
    /// Every `link_to_document` call the editor made.
    pub pii_links: Rc<RefCell<Vec<Vec<String>>>>,
    /// Every record id the editor asked `PiiDisplay` about.
    pub pii_queries: Rc<RefCell<Vec<String>>>,
    pub ime_areas: Rc<RefCell<Vec<Rect>>>,
    pub redraws: Rc<Cell<usize>>,
}

struct FakeDocs(Rc<RefCell<Vec<SaveRequest>>>);
impl DocumentService for FakeDocs {
    fn save(&mut self, request: SaveRequest) {
        self.0.borrow_mut().push(request);
    }
}

struct FakeClipboard(Rc<RefCell<ClipboardState>>);
impl Clipboard for FakeClipboard {
    fn read(&mut self) -> Option<ClipboardContent> {
        let mut s = self.0.borrow_mut();
        s.reads += 1;
        s.content.clone()
    }
    fn write(&mut self, item: ClipboardWrite) {
        let mut s = self.0.borrow_mut();
        s.content = Some(ClipboardContent { text: item.text.clone(), sovereign: item.sovereign.clone() });
        s.writes.push(item);
    }
}

struct FakeA11y(Rc<RefCell<Vec<accesskit::TreeUpdate>>>);
impl AccessibilitySink for FakeA11y {
    fn update(&mut self, update: accesskit::TreeUpdate) {
        self.0.borrow_mut().push(update);
    }
}

struct FakeClock(Rc<Cell<Duration>>);
impl Clock for FakeClock {
    fn now(&self) -> Duration {
        self.0.get()
    }
}

/// The resolver for ONE open document: a token is a token only if its record
/// exists and this document is among its sources; anything else in token form
/// is ordinary text.
struct FakePii(Fakes);
impl PiiDisplay for FakePii {
    fn display(&self, record_id: &str) -> TokenDisplay {
        self.0.pii_queries.borrow_mut().push(record_id.to_string());
        if self.0.pii_elsewhere.borrow().contains(record_id) {
            return TokenDisplay::NotAToken;
        }
        match self.0.pii.borrow().get(record_id) {
            Some(e) if e.revealed => TokenDisplay::Revealed(e.value.clone()),
            Some(e) => TokenDisplay::Label(e.label.clone()),
            None => TokenDisplay::NotAToken,
        }
    }
    fn link_to_document(&mut self, record_ids: &[String]) {
        self.0.pii_links.borrow_mut().push(record_ids.to_vec());
        for id in record_ids {
            self.0.pii_elsewhere.borrow_mut().remove(id);
        }
    }
}

struct FakeWindow(Rc<RefCell<Vec<Rect>>>, Rc<Cell<usize>>);
impl WindowHost for FakeWindow {
    fn set_ime_cursor_area(&mut self, area: Rect) {
        self.0.borrow_mut().push(area);
    }
    fn request_redraw(&mut self) {
        self.1.set(self.1.get() + 1);
    }
}

pub fn host() -> (Host, Fakes) {
    let fakes = Fakes {
        saves: Rc::default(),
        clipboard: Rc::default(),
        a11y_updates: Rc::default(),
        clock: Rc::new(Cell::new(Duration::from_secs(1000))),
        pii: Rc::default(),
        pii_elsewhere: Rc::default(),
        pii_links: Rc::default(),
        pii_queries: Rc::default(),
        ime_areas: Rc::default(),
        redraws: Rc::default(),
    };
    let host = Host {
        documents: Box::new(FakeDocs(fakes.saves.clone())),
        clipboard: Box::new(FakeClipboard(fakes.clipboard.clone())),
        accessibility: Box::new(FakeA11y(fakes.a11y_updates.clone())),
        clock: Box::new(FakeClock(fakes.clock.clone())),
        pii: Box::new(FakePii(fakes.clone())),
        window: Box::new(FakeWindow(fakes.ime_areas.clone(), fakes.redraws.clone())),
    };
    (host, fakes)
}

/// Puts plain text on the fake clipboard, as another application would (no
/// Sovereign payload).
pub fn set_clip_text(f: &Fakes, text: impl Into<String>) {
    f.clipboard.borrow_mut().content = Some(ClipboardContent { text: text.into(), sovereign: None });
}

/// The clipboard's plain text.
pub fn clip_text(f: &Fakes) -> Option<String> {
    f.clipboard.borrow().content.as_ref().map(|c| c.text.clone())
}

/// The last clipboard write.
pub fn last_write(f: &Fakes) -> ClipboardWrite {
    f.clipboard.borrow().writes.last().cloned().expect("the clipboard was written")
}

/// Every kind of input that would change the text. Used to check that, in a
/// read-only state, none of them disappears without a notice. Paste expects
/// text on the clipboard.
pub fn editing_attempts() -> Vec<(&'static str, fn(&mut Editor))> {
    vec![
        ("text", |e| {
            e.handle_event(InputEvent::Text("x".into()));
        }),
        ("Backspace", |e| press(e, NamedKey::Backspace)),
        ("Delete", |e| press(e, NamedKey::Delete)),
        ("Enter", |e| press(e, NamedKey::Enter)),
        ("paste", |e| {
            ctrl(e, "v");
        }),
        ("cut", |e| {
            ctrl(e, "a");
            ctrl(e, "x");
        }),
        ("IME commit", |e| {
            ime(e, ImeEvent::Enabled);
            preedit(e, "に");
            ime(e, ImeEvent::Commit("日".into()));
            ime(e, ImeEvent::Disabled);
        }),
    ]
}

/// The record id used for personal-data tokens in the tests: the code's form,
/// `pii_record:<key>` (decided 2026-10-10: `[pii:<record_id>]`, ids contain `:`).
pub fn rid(key: &str) -> String {
    format!("pii_record:{key}")
}

/// `[pii:pii_record:<key>]`
pub fn token(key: &str) -> String {
    format!("[pii:{}]", rid(key))
}

/// Registers a personal-data record with the fake resolver.
pub fn add_pii(f: &Fakes, key: &str, label: &str, value: &str, revealed: bool) {
    f.pii
        .borrow_mut()
        .insert(rid(key), PiiEntry { label: label.into(), value: value.into(), revealed });
}

/// The record exists, but the open document is not among its sources: its
/// token form is ordinary text here.
pub fn not_linked_here(f: &Fakes, key: &str) {
    f.pii_elsewhere.borrow_mut().insert(rid(key));
}

pub fn reveal(f: &Fakes, key: &str, revealed: bool) {
    f.pii.borrow_mut().get_mut(&rid(key)).expect("known record").revealed = revealed;
}

pub fn config() -> EditorConfig {
    EditorConfig { fonts: Fonts::Only(vec![TEST_FONT.to_vec()]), accessibility_tree: accesskit::TreeId::ROOT }
}

pub fn open_with(content: &str, mode: OpenMode) -> (Editor, Fakes) {
    let (host, fakes) = host();
    let doc = OpenDocument { title: "Test".into(), content: content.into(), trust: Trust::Owned, mode };
    let mut ed = Editor::open(doc, config(), host).expect("open");
    ed.set_viewport(2000.0, 1000.0, 1.0);
    (ed, fakes)
}

pub fn open(content: &str) -> (Editor, Fakes) {
    open_with(content, OpenMode::Edit)
}

/// Opens and gives focus, caret wherever the editor puts it.
pub fn open_focused(content: &str) -> (Editor, Fakes) {
    let (mut ed, f) = open(content);
    ed.handle_event(InputEvent::Focus(true));
    (ed, f)
}

// ---------------------------------------------------------------- v2 builders

pub const FORMAT_HEAD: &str = r#"{"format":"sovereign.doc","v":2,"blocks":["#;

/// 32 hex digits, distinct per `n`.
pub fn id(n: u32) -> String {
    format!("{:032x}", 0xA5A5_0000_0000_0000_0000_0000_0000_0000u128 + n as u128)
}

pub fn bid(n: u32) -> BlockId {
    BlockId::from_hex(&id(n)).expect("hex id")
}

pub fn json_str(s: &str) -> String {
    serde_json::to_string(s).unwrap()
}

/// A `text` block in canonical key order.
pub fn text_block(n: u32, text: &str) -> String {
    format!(r#"{{"type":"text","id":"{}","markup":"markdown","text":{}}}"#, id(n), json_str(text))
}

/// A block of a type this version does not know, with odd fields.
pub fn unknown_block(n: u32) -> String {
    format!(
        r#"{{"zeta":[1.50,-0.0,1e3,123456789012345678901234567890],"type":"x-chart-v9","id":"{}","alpha":{{"b":"\u00e9\/","a":null}},"text":"not editable"}}"#,
        id(n)
    )
}

pub fn v2(blocks: &[String]) -> String {
    format!("{}{}]}}", FORMAT_HEAD, blocks.join(","))
}

/// A one-text-block document.
pub fn doc1(text: &str) -> String {
    v2(&[text_block(1, text)])
}

pub fn pos(n: u32, offset: usize) -> Position {
    Position { block: bid(n), offset }
}

pub fn caret_at(ed: &mut Editor, n: u32, offset: usize) {
    ed.set_selection(Selection::caret(pos(n, offset)));
}

// ---------------------------------------------------------------- input helpers

pub fn key(ed: &mut Editor, k: NamedKey, modifiers: Modifiers) -> sovereign_editor::EventResult {
    ed.handle_event(InputEvent::Key(KeyEvent { key: Key::Named(k), modifiers }))
}

pub fn press(ed: &mut Editor, k: NamedKey) {
    key(ed, k, Modifiers::NONE);
}

pub fn ctrl(ed: &mut Editor, c: &str) -> sovereign_editor::EventResult {
    ed.handle_event(InputEvent::Key(KeyEvent { key: Key::Character(c.into()), modifiers: Modifiers::CTRL }))
}

pub fn ctrl_shift(ed: &mut Editor, c: &str) -> sovereign_editor::EventResult {
    ed.handle_event(InputEvent::Key(KeyEvent {
        key: Key::Character(c.into()),
        modifiers: Modifiers::CTRL_SHIFT,
    }))
}

/// Types `s` one grapheme cluster per text event, as a keyboard would.
pub fn type_text(ed: &mut Editor, s: &str) {
    for g in s.graphemes(true) {
        ed.handle_event(InputEvent::Text(g.to_string()));
    }
}

pub fn ime(ed: &mut Editor, e: ImeEvent) {
    ed.handle_event(InputEvent::Ime(e));
}

pub fn preedit(ed: &mut Editor, s: &str) {
    let len = s.len();
    ime(ed, ImeEvent::Preedit { text: s.into(), cursor: Some((len, len)) });
}

pub fn cmd(ed: &mut Editor, c: Command) {
    ed.command(c);
}

pub fn advance(f: &Fakes, d: Duration) {
    f.clock.set(f.clock.get() + d);
}

// ---------------------------------------------------------------- model reading

/// Text of the block `n` in the in-memory model.
pub fn text_of(ed: &Editor, n: u32) -> String {
    ed.document().block(bid(n)).expect("block present").text().expect("text block").to_string()
}

/// Concatenation of all text blocks of the model, in order, joined by "\n".
pub fn all_text(doc: &DocumentV2) -> String {
    doc.blocks().iter().filter_map(|b| b.text()).collect::<Vec<_>>().join("\n")
}

pub fn model_json(ed: &Editor) -> String {
    ed.document().to_json_string()
}

pub fn caret(ed: &Editor) -> Position {
    ed.selection().focus
}

pub fn parse(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap_or_else(|e| panic!("saved content is not JSON: {e}"))
}

pub fn last_save(f: &Fakes) -> SaveRequest {
    f.saves.borrow().last().cloned().expect("a save was requested")
}

pub fn save_count(f: &Fakes) -> usize {
    f.saves.borrow().len()
}

pub fn finish(ed: &mut Editor, id: SaveId, ok: bool) {
    let r = if ok { Ok(()) } else { Err(SaveError { message: "disk unavailable".into() }) };
    ed.save_finished(id, r);
}

/// The texts of all `text` blocks in a stored v2 string, in order.
pub fn texts_in(stored: &str) -> Vec<String> {
    let v = parse(stored);
    v["blocks"]
        .as_array()
        .expect("blocks array")
        .iter()
        .filter(|b| b["type"] == "text")
        .map(|b| b["text"].as_str().expect("text field").to_string())
        .collect()
}

pub fn block_types(stored: &str) -> Vec<String> {
    let v = parse(stored);
    v["blocks"].as_array().expect("blocks array").iter().map(|b| b["type"].as_str().unwrap().to_string()).collect()
}

// ---------------------------------------------------------------- segmentation reference

/// Extended grapheme cluster boundaries of `s` (UAX #29), including 0 and len.
pub fn grapheme_boundaries(s: &str) -> Vec<usize> {
    let mut v: Vec<usize> = s.grapheme_indices(true).map(|(i, _)| i).collect();
    v.push(s.len());
    v
}

pub fn is_grapheme_boundary(s: &str, offset: usize) -> bool {
    grapheme_boundaries(s).contains(&offset)
}

/// Byte ranges of every `[pii:<id>]` token in `s`.
pub fn token_ranges(s: &str) -> Vec<(usize, usize)> {
    let mut out = vec![];
    let mut from = 0;
    while let Some(i) = s[from..].find("[pii:") {
        let start = from + i;
        match s[start..].find(']') {
            Some(j) => {
                out.push((start, start + j + 1));
                from = start + j + 1;
            }
            None => break,
        }
    }
    out
}

pub fn strictly_inside_token(s: &str, offset: usize) -> bool {
    token_ranges(s).iter().any(|&(a, b)| offset > a && offset < b)
}

// ---------------------------------------------------------------- accessibility reading

/// Builds the platform view of the editor's accessibility tree from its full
/// snapshot, with the host window focused.
pub fn a11y_tree(ed: &mut Editor) -> accesskit_consumer::Tree {
    accesskit_consumer::Tree::new(ed.accessibility_tree(), true)
}

pub struct FocusedText {
    pub role: accesskit::Role,
    pub text: String,
    pub read_only: bool,
    /// Caret as a character (USV) index into `text`.
    pub caret: Option<usize>,
    /// Selected text, if any.
    pub selected: Option<String>,
}

pub fn focused_text(tree: &accesskit_consumer::Tree) -> FocusedText {
    let state = tree.state();
    let node = state.focus().expect("the focused node is in the tree");
    assert!(node.supports_text_ranges(), "the focused node exposes its text (role {:?})", node.role());
    let text = node.document_range().text();
    let caret = node.text_selection_focus().map(|p| p.to_global_usv_index());
    let selected = node.text_selection().and_then(|r| if r.is_degenerate() { None } else { Some(r.text()) });
    FocusedText { role: node.role(), text, read_only: node.is_read_only(), caret, selected }
}

/// Every string the tree exposes: values, labels, descriptions, text runs.
pub fn all_exposed_strings(update: &accesskit::TreeUpdate) -> Vec<String> {
    let mut out = vec![];
    for (_, n) in &update.nodes {
        for s in [n.value(), n.label(), n.description(), n.placeholder(), n.tooltip()].into_iter().flatten() {
            out.push(s.to_string());
        }
    }
    out
}

pub struct NoopHandler;
impl accesskit_consumer::TreeChangeHandler for NoopHandler {
    fn node_added(&mut self, _: &accesskit_consumer::Node) {}
    fn node_updated(&mut self, _: &accesskit_consumer::Node, _: &accesskit_consumer::Node) {}
    fn focus_moved(&mut self, _: Option<&accesskit_consumer::Node>, _: Option<&accesskit_consumer::Node>) {}
    fn node_removed(&mut self, _: &accesskit_consumer::Node) {}
}

pub fn paint(ed: &mut Editor) {
    let mut scene = vello::Scene::new();
    ed.paint(&mut scene);
}

pub fn x_of(ed: &mut Editor, n: u32, offset: usize) -> f64 {
    caret_at(ed, n, offset);
    let r = ed.caret_rect().expect("caret rect");
    (r.x0 + r.x1) / 2.0
}
