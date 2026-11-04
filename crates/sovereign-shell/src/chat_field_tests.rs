//! v0.0.12 item 21 — the chat composer and the search field are real text
//! fields (`doc/plans/V0012-item21-composer.md`, contract K1–K8, amended
//! 2026-10-23 on the laptop's 0130: K3b, K7, K8, the test seam).
//!
//! Written before the code, by another author than the implementer: whoever
//! builds item 21 makes these pass without editing this file. A real input
//! method, a screen reader and the Windows clipboard are checked by hand (the
//! v0.0.12 checklist).
//!
//! # The seam these tests assume (the implementer builds exactly this)
//!
//! A new module `crate::chat_field` (registered in `main.rs` as
//! `mod chat_field;`), on the shape of `crate::lane_rename`: one headless
//! component with a mode, whose key handling returns a step that `App` only
//! routes.
//!
//! ```ignore
//! /// Which window the field belongs to.
//! #[derive(Clone, Copy, Debug, PartialEq, Eq)]
//! pub(crate) enum FieldMode { Message, Search }
//!
//! /// What the field asks `App` to do after an input.
//! #[derive(Clone, Debug, PartialEq, Eq)]
//! pub(crate) enum FieldStep {
//!     /// Nothing for `App` to do.
//!     None,
//!     /// Message: send this text (trimmed). Search: open a chat with this
//!     /// query (trimmed) sent (K3).
//!     Send(String),
//!     /// Close the window (K3: Escape; a blank search query on Enter).
//!     Close,
//!     /// Search only: the query changed to this text; refresh the results (K4).
//!     QueryChanged(String),
//! }
//!
//! pub(crate) struct ChatField { /* the editor's single-line field, … */ }
//!
//! impl ChatField {
//!     /// Opens the field, empty and focused, laid out with `fonts`, copying and
//!     /// pasting through `clipboard` (K1, K2).
//!     pub(crate) fn open(mode: FieldMode, fonts: Fonts, clipboard: Box<dyn Clipboard>) -> ChatField;
//!     /// The field's committed text (no input-method pre-edit).
//!     pub(crate) fn text(&self) -> String;
//!     /// The field: the editor's single-line field (K1).
//!     pub(crate) fn field(&self) -> &Editor;
//!     /// One winit key press, with the text winit gives it (`"\r"` for
//!     /// Enter), through `crate::winit_input` (K2, K3, K3b, K4).
//!     pub(crate) fn key(&mut self, key: &WKey, text: Option<&str>, mods: Modifiers) -> FieldStep;
//!     /// Any other input for the field: text, input method, pointer (field
//!     /// coordinates), focus (K2, K4).
//!     pub(crate) fn event(&mut self, event: InputEvent) -> FieldStep;
//!     /// The field's AccessKit tree: a text input named "Message" or
//!     /// "Search", holding the focus, with its value and selection (K5).
//!     pub(crate) fn accessibility_tree(&mut self) -> TreeUpdate;
//! }
//! ```
//!
//! (`WKey` is `winit::keyboard::Key`; `Fonts`, `Editor`, `Modifiers`,
//! `InputEvent`, `Clipboard` are `sovereign-editor`'s; `TreeUpdate` is
//! AccessKit's.) Other methods (layout, drawing, the pointer from shell
//! coordinates, the input-method area) are the implementer's to name.
//!
//! # Readings chosen where the contract leaves room
//!
//! - `QueryChanged` carries the query; these tests accept the field's text or
//!   its trimmed form.
//! - An edit in the composer (Message mode) gives `FieldStep::None`: only the
//!   search's edits ask `App` for anything (K4 is the search's; K7, nothing
//!   leaves the field until it is sent).
//! - A pre-edit that is not committed, a copy, and a key that changes nothing
//!   are not asserted either way for K4.
//! - The field is focused on open (as the new-thread and rename fields are),
//!   so the K5 checks read the focused node.

use std::cell::RefCell;
use std::rc::Rc;

use sovereign_editor::host::{Clipboard, ClipboardContent, ClipboardWrite};
use sovereign_editor::input::{ImeEvent, InputEvent, Modifiers};
use sovereign_editor::Fonts;
use winit::keyboard::{Key as WKey, NamedKey as WNamed};

use crate::chat_field::{ChatField, FieldMode, FieldStep};

const TEST_FONT: &[u8] = include_bytes!("../../sovereign-editor/tests/fixtures/Cantarell-Regular.ttf");

// ------------------------------------------------------------------ fixtures

/// A clipboard in memory: what it holds.
#[derive(Clone, Default)]
struct MemClip(Rc<RefCell<Option<ClipboardContent>>>);
impl Clipboard for MemClip {
    fn read(&mut self) -> Option<ClipboardContent> {
        self.0.borrow().clone()
    }
    fn write(&mut self, item: ClipboardWrite) {
        *self.0.borrow_mut() = Some(ClipboardContent { text: item.text, sovereign: item.sovereign });
    }
}
impl MemClip {
    fn set_text(&self, text: &str) {
        *self.0.borrow_mut() = Some(ClipboardContent { text: text.to_string(), sovereign: None });
    }
    fn text(&self) -> Option<String> {
        self.0.borrow().as_ref().map(|c| c.text.clone())
    }
}

fn open(mode: FieldMode) -> (ChatField, MemClip) {
    let clip = MemClip::default();
    let f = ChatField::open(mode, Fonts::Only(vec![TEST_FONT.to_vec()]), Box::new(clip.clone()));
    (f, clip)
}

/// One key per character, as a keyboard types them; the steps, in order.
fn type_str(f: &mut ChatField, s: &str) -> Vec<FieldStep> {
    s.chars()
        .map(|c| {
            let t = c.to_string();
            if c == ' ' {
                f.key(&WKey::Named(WNamed::Space), Some(" "), Modifiers::NONE)
            } else {
                f.key(&WKey::Character(t.as_str().into()), Some(&t), Modifiers::NONE)
            }
        })
        .collect()
}

fn enter(f: &mut ChatField) -> FieldStep {
    f.key(&WKey::Named(WNamed::Enter), Some("\r"), Modifiers::NONE)
}

fn esc(f: &mut ChatField) -> FieldStep {
    f.key(&WKey::Named(WNamed::Escape), Some("\u{1b}"), Modifiers::NONE)
}

fn named(f: &mut ChatField, k: WNamed) -> FieldStep {
    f.key(&WKey::Named(k), None, Modifiers::NONE)
}

fn shift(f: &mut ChatField, k: WNamed) -> FieldStep {
    f.key(&WKey::Named(k), None, Modifiers::SHIFT)
}

/// Ctrl + a letter, with the control character winit gives as its text.
fn ctrl(f: &mut ChatField, c: char) -> FieldStep {
    let ctl = char::from_u32((c as u32) & 0x1f).unwrap().to_string();
    f.key(&WKey::Character(c.to_string().as_str().into()), Some(&ctl), Modifiers::CTRL)
}

/// An input-method composition showing `にほ`.
fn compose(f: &mut ChatField) {
    f.event(InputEvent::Ime(ImeEvent::Enabled));
    f.event(InputEvent::Ime(ImeEvent::Preedit { text: "にほ".into(), cursor: Some((6, 6)) }));
}

/// The input method ends the composition and commits `日本`; the commit's step.
fn commit(f: &mut ChatField) -> FieldStep {
    f.event(InputEvent::Ime(ImeEvent::Preedit { text: String::new(), cursor: None }));
    f.event(InputEvent::Ime(ImeEvent::Commit("日本".into())))
}

/// `step` is `QueryChanged` with the field's current text (as is, or trimmed).
fn assert_query_changed(f: &ChatField, step: &FieldStep, what: &str) {
    let text = f.text();
    match step {
        FieldStep::QueryChanged(q) => {
            assert!(q == &text || q == text.trim(), "{what}: the query carried ({q:?}) is the field's text ({text:?})")
        }
        other => panic!("{what}: expected QueryChanged({text:?}), got {other:?}"),
    }
}

fn a11y(f: &mut ChatField) -> accesskit_consumer::Tree {
    accesskit_consumer::Tree::new(f.accessibility_tree(), true)
}

// ======================================================================= K1

// K1: "The composer and the search query are each the editor's single-line
// field": the text is the field's, on one line, a single-line text input.
#[test]
fn k1_each_is_the_editors_single_line_field() {
    for mode in [FieldMode::Message, FieldMode::Search] {
        let (mut f, _c) = open(mode);
        type_str(&mut f, "hello");
        assert_eq!(f.field().field_text(), f.text(), "{mode:?}: the text is the field's");
        assert_eq!(f.text(), "hello", "{mode:?}: typed text reaches the field");
        assert_eq!(f.field().document().blocks().len(), 1, "{mode:?}: one line");
    }
}

// ======================================================================= K3

// K3: "Enter in the composer sends the field's trimmed text when it is not
// blank, then empties the field."
#[test]
fn k3_enter_sends_the_trimmed_text_then_empties_the_field() {
    let (mut f, _c) = open(FieldMode::Message);
    type_str(&mut f, "  hello there  ");
    assert_eq!(enter(&mut f), FieldStep::Send("hello there".into()));
    assert_eq!(f.text(), "", "the field is empty after a send");
    type_str(&mut f, "next");
    assert_eq!(f.text(), "next", "the emptied field takes the next message");
    assert_eq!(enter(&mut f), FieldStep::Send("next".into()));
}

// K3: "A blank text sends nothing and leaves the field as it is."
#[test]
fn k3_a_blank_text_sends_nothing_and_stays() {
    for typed in ["", "   "] {
        let (mut f, _c) = open(FieldMode::Message);
        type_str(&mut f, typed);
        assert_eq!(enter(&mut f), FieldStep::None, "{typed:?}: nothing is sent, the window stays");
        assert_eq!(f.text(), typed, "{typed:?}: the field is left as it is");
    }
}

// K3: "Enter in the search field opens a chat with the trimmed query sent."
#[test]
fn k3_search_enter_sends_the_trimmed_query() {
    let (mut f, _c) = open(FieldMode::Search);
    type_str(&mut f, "  spring plans  ");
    assert_eq!(enter(&mut f), FieldStep::Send("spring plans".into()));
}

// K3: "A blank query only closes the search window."
#[test]
fn k3_search_enter_with_a_blank_query_closes() {
    for typed in ["", "   "] {
        let (mut f, _c) = open(FieldMode::Search);
        type_str(&mut f, typed);
        assert_eq!(enter(&mut f), FieldStep::Close, "{typed:?}: the search window closes, nothing is sent");
    }
}

// K3: "Escape closes the window, as today", whatever was typed.
#[test]
fn k3_escape_closes() {
    for mode in [FieldMode::Message, FieldMode::Search] {
        let (mut f, _c) = open(mode);
        assert_eq!(esc(&mut f), FieldStep::Close, "{mode:?}, empty");
        let (mut f, _c) = open(mode);
        type_str(&mut f, "half a thought");
        assert_eq!(esc(&mut f), FieldStep::Close, "{mode:?}, with text");
    }
}

// K3 [choice] (amended 2026-10-24, efa4f1e): "during an input method's
// composition, the first Escape ends the composition and the next one closes,
// as the rename and new-thread fields already do (N7)" — N7: ending it commits
// nothing.
#[test]
fn k3_escape_during_a_composition_ends_it_then_closes() {
    for mode in [FieldMode::Message, FieldMode::Search] {
        let (mut f, _c) = open(mode);
        type_str(&mut f, "Hi ");
        compose(&mut f);
        assert_eq!(esc(&mut f), FieldStep::None, "{mode:?}: the first Esc does not close the window");
        assert!(!f.field().is_composing(), "{mode:?}: the first Esc ends the composition");
        assert_eq!(f.text(), "Hi ", "{mode:?}: nothing of the composition is committed");
        assert_eq!(esc(&mut f), FieldStep::Close, "{mode:?}: the next Esc closes");
    }
}

// ====================================================================== K3b

// K3 [choice] + K3b, through the composer: "Enter during an input method's
// composition commits the composition and does not send. A second Enter
// sends."
#[test]
fn k3b_composer_enter_during_a_composition_does_not_send() {
    let (mut f, _c) = open(FieldMode::Message);
    type_str(&mut f, "Hi ");
    compose(&mut f);
    assert_eq!(enter(&mut f), FieldStep::None, "no send of the half-composed message");
    assert_eq!(f.text(), "Hi ", "and the field keeps its text");
    commit(&mut f);
    assert_eq!(enter(&mut f), FieldStep::Send("Hi 日本".into()), "the next Enter sends the committed text");
}

// K3b through the search field: an Enter during a composition neither opens
// a chat nor closes the window; the next one sends the committed query.
#[test]
fn k3b_search_enter_during_a_composition_does_nothing() {
    let (mut f, _c) = open(FieldMode::Search);
    type_str(&mut f, "q ");
    compose(&mut f);
    assert_eq!(enter(&mut f), FieldStep::None, "no chat opened, no close, during the composition");
    commit(&mut f);
    assert_eq!(enter(&mut f), FieldStep::Send("q 日本".into()));
}

// K3b on a blank search: an Enter during the composition of a first word does
// not close the window either.
#[test]
fn k3b_search_enter_during_a_first_composition_does_not_close() {
    let (mut f, _c) = open(FieldMode::Search);
    compose(&mut f);
    assert_eq!(enter(&mut f), FieldStep::None, "the window stays while the first word is composed");
}

// ======================================================================= K4

// K4: "Any edit of the search field (typing, deleting anywhere, paste, cut,
// undo, an input method's commit) refreshes the results".
#[test]
fn k4_typing_and_deleting_change_the_query() {
    let (mut f, _c) = open(FieldMode::Search);
    for (i, step) in type_str(&mut f, "abcd").iter().enumerate() {
        assert!(matches!(step, FieldStep::QueryChanged(_)), "typed character {i}: {step:?}");
    }
    assert_eq!(f.text(), "abcd");
    let s = named(&mut f, WNamed::Backspace);
    assert_query_changed(&f, &s, "Backspace");
    assert_eq!(f.text(), "abc");
    named(&mut f, WNamed::Home);
    let s = named(&mut f, WNamed::Delete);
    assert_query_changed(&f, &s, "Delete at the start");
    assert_eq!(f.text(), "bc");
    named(&mut f, WNamed::ArrowRight);
    let s = named(&mut f, WNamed::Backspace);
    assert_query_changed(&f, &s, "Backspace mid-text");
    assert_eq!(f.text(), "c");
}

#[test]
fn k4_paste_cut_and_undo_change_the_query() {
    let (mut f, clip) = open(FieldMode::Search);
    clip.set_text("pasted");
    let s = ctrl(&mut f, 'v');
    assert_query_changed(&f, &s, "paste");
    assert_eq!(f.text(), "pasted");

    ctrl(&mut f, 'a');
    let s = ctrl(&mut f, 'x');
    assert_query_changed(&f, &s, "cut");
    assert_eq!(f.text(), "");
    assert_eq!(clip.text().as_deref(), Some("pasted"), "the cut text is on the clipboard");

    type_str(&mut f, "ab");
    let before = f.text();
    let s = ctrl(&mut f, 'z');
    assert_ne!(f.text(), before, "undo changed the text");
    assert_query_changed(&f, &s, "undo");
}

#[test]
fn k4_an_input_method_commit_changes_the_query() {
    let (mut f, _c) = open(FieldMode::Search);
    type_str(&mut f, "q ");
    compose(&mut f);
    let s = commit(&mut f);
    assert_eq!(f.text(), "q 日本");
    assert_query_changed(&f, &s, "an input method's commit");
}

// K4: "Moving the caret or selecting does not."
#[test]
fn k4_caret_moves_and_selection_do_not_change_the_query() {
    let (mut f, _c) = open(FieldMode::Search);
    type_str(&mut f, "spring plans");
    for k in [WNamed::ArrowLeft, WNamed::ArrowLeft, WNamed::Home, WNamed::ArrowRight, WNamed::End] {
        assert_eq!(named(&mut f, k), FieldStep::None, "moving the caret ({k:?})");
    }
    for k in [WNamed::ArrowLeft, WNamed::Home, WNamed::ArrowRight] {
        assert_eq!(shift(&mut f, k), FieldStep::None, "selecting (Shift+{k:?})");
    }
    assert_eq!(ctrl(&mut f, 'a'), FieldStep::None, "select all");
    assert_eq!(f.text(), "spring plans", "nothing was edited");
}

// Reading chosen (see the header): edits in the composer ask App for nothing
// (K4 is the search's; K7: the text stays in the field until it is sent).
#[test]
fn k4_composer_edits_ask_for_nothing() {
    let (mut f, clip) = open(FieldMode::Message);
    for step in type_str(&mut f, "abc") {
        assert_eq!(step, FieldStep::None, "typing in the composer");
    }
    assert_eq!(named(&mut f, WNamed::Backspace), FieldStep::None, "Backspace in the composer");
    clip.set_text("x");
    assert_eq!(ctrl(&mut f, 'v'), FieldStep::None, "paste in the composer");
    compose(&mut f);
    assert_eq!(commit(&mut f), FieldStep::None, "an input method's commit in the composer");
    assert_eq!(f.text(), "abx日本");
}

// ========================================== K1/K2: each input reaches the field

// Caret movement: a missing letter is typed where the caret was moved to, not
// appended.
#[test]
fn k2_caret_movement_reaches_the_field() {
    let (mut f, _c) = open(FieldMode::Message);
    type_str(&mut f, "helo");
    named(&mut f, WNamed::ArrowLeft);
    type_str(&mut f, "l");
    assert_eq!(f.text(), "hello");
}

// Delete in the middle of the text.
#[test]
fn k2_delete_mid_text_reaches_the_field() {
    let (mut f, _c) = open(FieldMode::Message);
    type_str(&mut f, "abcd");
    named(&mut f, WNamed::Home);
    named(&mut f, WNamed::ArrowRight);
    named(&mut f, WNamed::Delete);
    assert_eq!(f.text(), "acd");
}

// A selection is replaced by what is typed.
#[test]
fn k2_typing_replaces_the_selection() {
    let (mut f, _c) = open(FieldMode::Message);
    type_str(&mut f, "hello world");
    named(&mut f, WNamed::Home);
    for _ in 0..5 {
        shift(&mut f, WNamed::ArrowRight);
    }
    type_str(&mut f, "HOWDY");
    assert_eq!(f.text(), "HOWDY world");
}

// Paste, through the clipboard the shell gives (K2: `FieldClipboard`).
#[test]
fn k2_paste_reaches_the_field() {
    let (mut f, clip) = open(FieldMode::Message);
    type_str(&mut f, "a ");
    clip.set_text("pasted");
    ctrl(&mut f, 'v');
    assert_eq!(f.text(), "a pasted");
}

// An input method's commit is inserted once; its pre-edit is not part of the
// text.
#[test]
fn k2_an_input_method_commit_is_inserted_once() {
    let (mut f, _c) = open(FieldMode::Message);
    compose(&mut f);
    assert_eq!(f.text(), "", "the pre-edit is not committed text");
    commit(&mut f);
    assert_eq!(f.text(), "日本", "the commit, once");
}

// ======================================================================= K5

/// The focused node of `f`'s tree: a text input labelled `label`, its value
/// `value`.
fn assert_text_input(f: &mut ChatField, label: &str, value: &str) {
    let tree = a11y(f);
    let focus = tree.state().focus().expect("the field has the focus");
    assert_eq!(focus.role(), accesskit::Role::TextInput, "{label}: a text input");
    assert_eq!(focus.label().as_deref(), Some(label));
    let v = if focus.supports_text_ranges() { focus.document_range().text() } else { focus.value().unwrap_or_default() };
    assert_eq!(v.trim_end_matches('\n'), value, "{label}: the announced value is the field's text");
}

// K5: "Each field is a text input in the AccessKit tree, named "Message" (the
// composer) and "Search" (the search field), with its value".
#[test]
fn k5_the_fields_are_text_inputs_named_message_and_search() {
    let (mut f, _c) = open(FieldMode::Message);
    type_str(&mut f, "hello");
    assert_text_input(&mut f, "Message", "hello");
    let (mut f, _c) = open(FieldMode::Search);
    type_str(&mut f, "spring");
    assert_text_input(&mut f, "Search", "spring");
}

// K5: "… and selection exposed as the editor's field already exposes them".
#[test]
fn k5_the_selection_is_exposed() {
    for mode in [FieldMode::Message, FieldMode::Search] {
        let (mut f, _c) = open(mode);
        type_str(&mut f, "hello world");
        named(&mut f, WNamed::Home);
        for _ in 0..5 {
            shift(&mut f, WNamed::ArrowRight);
        }
        let tree = a11y(&mut f);
        let focus = tree.state().focus().expect("the field has the focus");
        let selected = focus.text_selection().map(|r| r.text());
        assert_eq!(selected.as_deref(), Some("hello"), "{mode:?}: the selection is announced");
    }
}

// ============================================================ App (source)

/// The production part of a shell source file (before its trailing test
/// module, if any).
fn production(file: &str, test_marker: &str) -> String {
    let src = std::fs::read_to_string(format!("{}/src/{file}", env!("CARGO_MANIFEST_DIR"))).unwrap();
    match src.find(test_marker) {
        Some(at) => src[..at].to_string(),
        None => src,
    }
}

/// Code without `//` comments, so a comment naming a call does not count.
fn code(src: &str) -> String {
    src.lines().map(|l| l.split("//").next().unwrap_or("")).collect::<Vec<_>>().join("\n")
}

fn app_rs() -> String {
    code(&production("app.rs", "#[cfg(test)]\nmod taskbar_tests"))
}

// The seam: App routes each of the field's steps (one line per step). What it
// does with them needs a window and a GPU, and is the manual checklist's.
#[test]
fn app_routes_each_step() {
    let app = app_rs();
    assert!(app.contains("FieldStep::Send"), "App routes Send (the message sent, or the search escalated to a chat)");
    assert!(app.contains("FieldStep::Close"), "App routes Close (the window closes)");
    assert!(app.contains("FieldStep::QueryChanged"), "App routes QueryChanged (the results refresh)");
}

// K1: "No second text-editing implementation is written": the append/pop
// buffers of the composer and of the search query are gone from App.
#[test]
fn app_has_no_append_pop_buffer_for_chat_or_search() {
    let app = app_rs();
    for old in ["c.input.push", "c.input.pop", "sp.query.push", "sp.query.pop"] {
        assert!(!app.contains(old), "App still edits a text buffer by hand: `{old}`");
    }
}
