//! Naming a new thread — contract N1–N7 (`doc/plans/THREAD-name.md`, spec
//! ch. 12, *New thread — named first, as in the Tauri desktop*). The shell
//! restores the Tauri desktop's prompt (`frontend/src/lib/components/Canvas.svelte`,
//! `handleCreateThread` and the `new-thread-popup`): a name field and a
//! *Create* button; Enter creates, Esc cancels, an empty name creates nothing.
//!
//! Written before the code, by another author: whoever builds it makes these
//! pass without editing this file. List, red reasons and the readings chosen:
//! `crates/sovereign-shell/TESTS-thread-name.md`. Drawing, real keyboard focus,
//! a real input method and a screen reader are checked by hand
//! (`THREAD_CHECKLIST.md`).
//!
//! Seams: the prompt (`new_thread::NewThreadPrompt`), the creation path
//! (`new_thread::create_named_thread`) over a real encrypted store on an
//! in-memory SurrealDB (`docs_host::testing`), the canvas's workspace load
//! (`canvas::load_workspace`). `App` needs a window and a GPU: what only `App`
//! does (the menu row opening the prompt, creating on `Create`, reloading,
//! announcing the prompt) is checked by reading its source, as
//! `step5_tests.rs` and `title_tests.rs` do.
//!
//! N4 ("the field follows the editor's single-line field rules … the title's
//! T6 and T9") is tested through the shared field: a few checks that the
//! prompt routes each kind of input to it, not the field's own suites again.
//! The long-name check (`n4_a_long_name_scrolls_inside_the_field`) depends on
//! title T9 (`shell/tests-title-t9`, not merged when this was written): it
//! stays red until T9 is built, by design.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use sovereign_ai::docs_host::testing;
use sovereign_db::traits::GraphDB;
use sovereign_editor::host::{Clipboard, ClipboardContent, ClipboardWrite};
use sovereign_editor::input::{Command, ImeEvent, InputEvent, Modifiers};
use sovereign_editor::{Fonts, Position, Selection};
use vello::kurbo::{Point, Rect};
use winit::keyboard::{Key as WKey, NamedKey as WNamed};

use crate::new_thread::{create_named_thread, NewThreadPrompt, PromptStep, CREATE_LABEL, DIALOG_LABEL, FIELD_LABEL, PLACEHOLDER};

const TEST_FONT: &[u8] = include_bytes!("../../sovereign-editor/tests/fixtures/Cantarell-Regular.ttf");
/// The window the prompt is laid out in (logical pixels).
const W: f64 = 1280.0;
const H: f64 = 800.0;

// ------------------------------------------------------------------ fixtures

/// A clipboard in memory: what it holds, and every write.
#[derive(Clone, Default)]
struct MemClip {
    held: Rc<RefCell<Option<ClipboardContent>>>,
    writes: Rc<RefCell<Vec<ClipboardWrite>>>,
}
impl Clipboard for MemClip {
    fn read(&mut self) -> Option<ClipboardContent> {
        self.held.borrow().clone()
    }
    fn write(&mut self, item: ClipboardWrite) {
        *self.held.borrow_mut() = Some(ClipboardContent { text: item.text.clone(), sovereign: item.sovereign.clone() });
        self.writes.borrow_mut().push(item);
    }
}
impl MemClip {
    fn set_text(&self, text: &str) {
        *self.held.borrow_mut() = Some(ClipboardContent { text: text.to_string(), sovereign: None });
    }
    fn text(&self) -> Option<String> {
        self.held.borrow().as_ref().map(|c| c.text.clone())
    }
}

/// The prompt as the gestures open it, not laid out yet.
fn open_prompt() -> (NewThreadPrompt, MemClip) {
    let clip = MemClip::default();
    let p = NewThreadPrompt::open(Fonts::Only(vec![TEST_FONT.to_vec()]), Box::new(clip.clone()));
    (p, clip)
}

/// The prompt, laid out in a `W` × `H` window (as the first frame does).
fn prompt() -> (NewThreadPrompt, MemClip) {
    let (mut p, clip) = open_prompt();
    p.layout(W, H);
    (p, clip)
}

/// Types `s` as winit delivers it: a character key with its text per char,
/// Space as the named key with `" "`. Returns the steps answered.
fn type_str(p: &mut NewThreadPrompt, s: &str) -> Vec<PromptStep> {
    s.chars()
        .map(|c| {
            let t = c.to_string();
            if c == ' ' {
                p.key(&WKey::Named(WNamed::Space), Some(" "), Modifiers::NONE)
            } else {
                p.key(&WKey::Character(t.as_str().into()), Some(&t), Modifiers::NONE)
            }
        })
        .collect()
}

fn enter(p: &mut NewThreadPrompt) -> PromptStep {
    p.key(&WKey::Named(WNamed::Enter), Some("\r"), Modifiers::NONE)
}

fn esc(p: &mut NewThreadPrompt) -> PromptStep {
    p.key(&WKey::Named(WNamed::Escape), Some("\u{1b}"), Modifiers::NONE)
}

fn named(p: &mut NewThreadPrompt, k: WNamed) -> PromptStep {
    p.key(&WKey::Named(k), None, Modifiers::NONE)
}

/// Ctrl + a letter, with the control character winit gives as its text.
fn ctrl(p: &mut NewThreadPrompt, c: char) -> PromptStep {
    let ctl = char::from_u32((c as u32) & 0x1f).unwrap().to_string();
    p.key(&WKey::Character(c.to_string().as_str().into()), Some(&ctl), Modifiers::CTRL)
}

/// The caret (selection focus) offset in the field's one block.
fn caret(p: &NewThreadPrompt) -> usize {
    p.field().selection().focus.offset
}

fn caret_at(p: &mut NewThreadPrompt, offset: usize) {
    let block = p.field().document().blocks()[0].id();
    p.field_mut().set_selection(Selection::caret(Position { block, offset }));
}

fn center(r: Rect) -> Point {
    Point::new((r.x0 + r.x1) / 2.0, (r.y0 + r.y1) / 2.0)
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap()
}

fn as_dyn(store: &Arc<sovereign_db::encrypted::EncryptedGraphDB>) -> Arc<dyn GraphDB> {
    store.clone()
}

/// The document service over `store`: since THREAD-sync S1 (decided by Céline
/// 2026-10-18) a thread is created only through its stamped thread-write path,
/// so the creation path is given the service (migrated by the THREAD-sync test
/// author; a bare store refuses, `n2_a_bare_store_creates_no_thread`).
fn service(store: &Arc<sovereign_db::encrypted::EncryptedGraphDB>) -> sovereign_docs::DocumentService {
    sovereign_ai::docs_host::build_document_service(store.clone(), Some(testing::account_key()), "shell-test".into())
}

/// The production part of a shell source file (before its trailing test
/// module, if any).
fn production(file: &str, test_marker: &str) -> String {
    let src = std::fs::read_to_string(format!("{}/src/{file}", env!("CARGO_MANIFEST_DIR"))).unwrap();
    match src.find(test_marker) {
        Some(at) => src[..at].to_string(),
        None => src,
    }
}

fn app_rs() -> String {
    production("app.rs", "#[cfg(test)]\nmod taskbar_tests")
}

/// Code without `//` comments, so a comment naming a call does not count.
fn code(src: &str) -> String {
    src.lines().map(|l| l.split("//").next().unwrap_or("")).collect::<Vec<_>>().join("\n")
}

/// The body of `fn <name>` in `src` (from its signature to the next `fn` at
/// the same indentation).
fn fn_body<'a>(src: &'a str, name: &str) -> &'a str {
    let sig = format!("fn {name}(");
    let at = src.find(&sig).unwrap_or_else(|| panic!("fn {name} not found"));
    let rest = &src[at + sig.len()..];
    let end = rest.find("\n    fn ").into_iter().chain(rest.find("\n    pub(crate) fn ")).min().unwrap_or(rest.len());
    &rest[..end]
}

/// The method of `src` (4-space indented `fn`) that contains byte `at`.
fn enclosing_fn(src: &str, at: usize) -> &str {
    let head = &src[..at];
    let start = head.rfind("\n    fn ").into_iter().chain(head.rfind("\n    pub(crate) fn ")).max().expect("inside a method");
    let rest = &src[start + 1..];
    let end = rest[1..].find("\n    fn ").into_iter().chain(rest[1..].find("\n    pub(crate) fn ")).min().map(|e| e + 1).unwrap_or(rest.len());
    &rest[..end]
}

/// The prompt's accessibility tree, read as a screen reader's library reads it.
fn a11y(p: &mut NewThreadPrompt) -> accesskit_consumer::Tree {
    accesskit_consumer::Tree::new(p.accessibility_tree(), true)
}

/// Every node under the root, in document (pre-)order: (role, label, disabled, id).
fn preorder(tree: &accesskit_consumer::Tree) -> Vec<(accesskit::Role, Option<String>, bool, accesskit_consumer::NodeId)> {
    fn walk(n: accesskit_consumer::Node<'_>, out: &mut Vec<(accesskit::Role, Option<String>, bool, accesskit_consumer::NodeId)>) {
        out.push((n.role(), n.label(), n.is_disabled(), n.id()));
        for c in n.children() {
            walk(c, out);
        }
    }
    let mut out = vec![];
    walk(tree.state().root(), &mut out);
    out
}

// ======================================================================= N1

// N1: "No thread exists until it is confirmed": the prompt opens empty, and
// *Create* is not active on an empty name.
#[test]
fn n1_the_prompt_opens_empty_with_create_inactive() {
    let (p, _c) = open_prompt();
    assert_eq!(p.name(), "", "the prompt opens on an empty name");
    assert!(!p.can_create(), "Create is not active on an empty name");
}

// N1: "a small name field, focused": typing reaches it at once, with no click.
#[test]
fn n1_the_field_takes_typing_at_once() {
    let (mut p, _c) = open_prompt();
    let steps = type_str(&mut p, "Ideas");
    assert!(steps.iter().all(|s| *s == PromptStep::Open), "typing neither creates nor cancels: {steps:?}");
    assert_eq!(p.name(), "Ideas");
    assert!(p.can_create());
}

// N1 + N5: the focus is the field, announced with its label and the
// placeholder *Thread name…*.
#[test]
fn n1_the_field_is_focused_and_shows_the_placeholder() {
    let (mut p, _c) = prompt();
    let tree = a11y(&mut p);
    let focus = tree.state().focus().expect("the prompt has the focus");
    assert_eq!(focus.role(), accesskit::Role::TextInput, "the focus is the name field");
    assert_eq!(focus.label().as_deref(), Some(FIELD_LABEL));
    assert_eq!(focus.placeholder(), Some(PLACEHOLDER));
    assert_eq!(PLACEHOLDER, "Thread name\u{2026}", "the placeholder of N1");
    assert_eq!(FIELD_LABEL, "Thread name", "the label of N5");
}

// N1: "a small name field … and a *Create* button" — both inside the prompt,
// side by side, the prompt inside the window.
#[test]
fn n1_the_prompt_has_a_field_and_a_create_button() {
    let (mut p, _c) = open_prompt();
    let l = p.layout(W, H);
    let window = Rect::new(0.0, 0.0, W, H);
    assert!(l.bounds.area() > 0.0 && window.contains_rect(l.bounds), "the prompt lies in the window: {:?}", l.bounds);
    for (what, r) in [("field", l.field), ("Create", l.create)] {
        assert!(r.area() > 0.0, "{what} has a size: {r:?}");
        assert!(l.bounds.contains_rect(r), "{what} lies in the prompt: {r:?} in {:?}", l.bounds);
    }
    assert!(l.field.intersect(l.create).area() <= 0.0, "field and button do not overlap: {:?} {:?}", l.field, l.create);
    assert!(l.field.width() < W / 2.0, "a small field, not a window: {:?}", l.field);
    assert_eq!(CREATE_LABEL, "Create");
}

// N1: "The new-thread gestures (the `+` button, the canvas menu's *New
// lane*) open a small name field … No thread exists until it is confirmed."
// The `+` opens the canvas menu (spec ch. 12, *Context menu*), whose *New
// lane* row is `CtxAction::NewThread`: its arm opens the prompt and creates
// nothing.
#[test]
fn n1_new_lane_opens_the_prompt_and_creates_nothing() {
    let prod = code(&app_rs());
    let at = prod.find("CtxAction::NewThread =>").expect("the New lane row's arm");
    let rest = &prod[at + "CtxAction::NewThread =>".len()..];
    let arm = &rest[..rest.find("CtxAction::").unwrap_or(rest.len())];
    assert!(arm.contains("open_new_thread_prompt"), "New lane opens the name prompt: {arm}");
    assert!(!arm.contains("create") && !arm.contains("new_thread()"), "New lane creates nothing by itself: {arm}");
    let open = fn_body(&prod, "open_new_thread_prompt");
    assert!(open.contains("NewThreadPrompt::open"), "open_new_thread_prompt opens a NewThreadPrompt");
    assert!(!open.contains("create_thread") && !open.contains("create_named_thread"), "opening the prompt creates no thread");
}

// N1: the shell no longer names a lane itself ("New lane N", app.rs:3009-3018
// before this contract).
#[test]
fn n1_the_shell_no_longer_invents_a_lane_name() {
    let prod = code(&app_rs());
    let squashed: String = prod.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(!squashed.contains("format!(\"Newlane"), "a thread is no longer created as \"New lane N\"");
}

// ======================================================================= N2

// N2: "Enter … creates the thread with the name trimmed".
#[test]
fn n2_enter_creates_with_the_trimmed_name() {
    let (mut p, _c) = prompt();
    type_str(&mut p, "  Ideas for spring  ");
    assert_eq!(enter(&mut p), PromptStep::Create("Ideas for spring".into()));
}

// N2: "… or *Create* creates the thread with the name trimmed", by the
// button's action and by a click on it.
#[test]
fn n2_the_create_button_creates_with_the_trimmed_name() {
    let (mut p, _c) = prompt();
    type_str(&mut p, " Ideas ");
    assert_eq!(p.press_create(), PromptStep::Create("Ideas".into()));

    let (mut p, _c) = prompt();
    type_str(&mut p, " Ideas ");
    let l = p.layout(W, H);
    assert_eq!(p.click(center(l.create), W, H), PromptStep::Create("Ideas".into()));
}

// N2: "(one line: a pasted newline becomes a space)".
#[test]
fn n2_a_pasted_newline_becomes_a_space() {
    let (mut p, clip) = prompt();
    clip.set_text("Line one\nLine two");
    p.command(Command::Paste);
    assert!(!p.name().contains(['\n', '\r']), "no line break in the name: {:?}", p.name());
    assert_eq!(enter(&mut p), PromptStep::Create("Line one Line two".into()));

    // Ctrl+V, and a Windows line break: one line, a space (or two) for it.
    let (mut p, clip) = prompt();
    clip.set_text("Line one\r\nLine two\r\n");
    ctrl(&mut p, 'v');
    match enter(&mut p) {
        PromptStep::Create(n) => assert!(n == "Line one Line two" || n == "Line one  Line two", "{n:?}"),
        other => panic!("Enter after a paste creates: {other:?}"),
    }
}

// N2: "An empty or spaces-only name creates nothing (*Create* inactive; Enter
// does nothing)."
#[test]
fn n2_an_empty_or_spaces_only_name_creates_nothing() {
    for typed in ["", "   "] {
        let (mut p, _c) = prompt();
        type_str(&mut p, typed);
        assert!(!p.can_create(), "{typed:?}: Create inactive");
        assert_eq!(enter(&mut p), PromptStep::Open, "{typed:?}: Enter does nothing");
        assert_eq!(p.name(), typed, "{typed:?}: Enter types nothing either");
        assert_eq!(p.press_create(), PromptStep::Open, "{typed:?}: the inactive Create does nothing");
        let l = p.layout(W, H);
        assert_eq!(p.click(center(l.create), W, H), PromptStep::Open, "{typed:?}: a click on the inactive Create does nothing");
        // Still open and usable.
        type_str(&mut p, "x");
        assert!(p.can_create(), "{typed:?}: the prompt stays open after the refused Enter");
    }
}

// N2 with N5: "*Create* inactive" is announced — the button is disabled for a
// screen reader while the name is empty, enabled once it is not.
#[test]
fn n2_create_is_announced_inactive_while_the_name_is_empty() {
    let (mut p, _c) = prompt();
    let create = |p: &mut NewThreadPrompt| {
        preorder(&a11y(p)).into_iter().find(|(r, ..)| *r == accesskit::Role::Button).expect("a Create button")
    };
    assert!(create(&mut p).2, "Create is announced inactive on an empty name");
    type_str(&mut p, "   ");
    assert!(create(&mut p).2, "Create is announced inactive on a spaces-only name");
    type_str(&mut p, "a");
    assert!(!create(&mut p).2, "Create is announced active once the name is not empty");
}

// N2: "creates the thread … through the same creation path as today" — since
// THREAD-sync S1 the one stamped thread-write path, empty description — with
// the name trimmed and on one line.
#[test]
fn n2_the_store_gets_the_named_thread() {
    rt().block_on(async {
        let store = testing::encrypted_memory_store("n2-store").await;
        let db = as_dyn(&store);
        let docs = service(&store);
        let id = create_named_thread(&docs, "  Ideas\nnext  ").await.expect("created").expect("a thread id");
        let threads = db.list_threads().await.unwrap();
        assert_eq!(threads.len(), 1, "one thread created");
        let t = &threads[0];
        assert_eq!(t.id_string().as_deref(), Some(id.as_str()), "the id returned is the thread's");
        assert_eq!(t.name, "Ideas next", "trimmed, the line break a space");
        assert_eq!(t.description, "", "an empty description, as today");
        assert!(docs.thread_stamp(&id).await.unwrap().is_some(), "written through the stamped path (S1)");
    });
}

// THREAD-sync S1: "a path that has no document service either obtains one or
// does not write" — given only the store, the creation path refuses and
// writes nothing.
#[test]
fn n2_a_bare_store_creates_no_thread() {
    rt().block_on(async {
        let store = testing::encrypted_memory_store("n2-bare").await;
        let db = as_dyn(&store);
        assert!(create_named_thread(&*db, "Ideas").await.is_err(), "refused without the service");
        assert!(db.list_threads().await.unwrap().is_empty(), "no thread in the store");
    });
}

// N2: an empty or spaces-only name reaching the creation path creates nothing.
#[test]
fn n2_an_empty_name_creates_no_thread_in_the_store() {
    rt().block_on(async {
        let store = testing::encrypted_memory_store("n2-empty").await;
        let db = as_dyn(&store);
        for name in ["", "   ", " \n \r\n "] {
            assert_eq!(create_named_thread(&*db, name).await, Ok(None), "{name:?} creates nothing");
        }
        assert!(db.list_threads().await.unwrap().is_empty(), "no thread in the store");
    });
}

// N2: "… and its lane appears with that name" — the canvas's workspace load
// (what `load_workspace_now` runs) names a lane with it.
#[test]
fn n2_the_new_lane_shows_the_name() {
    rt().block_on(async {
        let store = testing::encrypted_memory_store("n2-lane").await;
        let db = as_dyn(&store);
        create_named_thread(&service(&store), " Ideas for spring ").await.expect("created").expect("an id");
        let mut shaper = crate::text::TextShaper::new();
        let (_, _, _, lane_names, _) = crate::canvas::load_workspace(&db, &mut shaper).await.expect("a workspace with a thread loads");
        assert!(lane_names.iter().any(|n| n == "Ideas for spring"), "a lane is named after the new thread: {lane_names:?}");
    });
}

// N2 in the shell: the prompt's `Create` is what creates — through
// `create_named_thread`, at one place — and the workspace is reloaded there so
// the lane appears (as `new_thread` did before).
#[test]
fn n2_the_shell_creates_on_create_then_reloads() {
    let prod = code(&app_rs());
    assert!(prod.contains("PromptStep::Create("), "the shell acts on the prompt's Create");
    let calls: Vec<usize> = prod.match_indices("create_named_thread(").map(|(i, _)| i).collect();
    assert_eq!(calls.len(), 1, "one place creates a named thread (the prompt's Create)");
    let f = enclosing_fn(&prod, calls[0]);
    assert!(f.contains("load_workspace_now"), "the shell reloads after creating, so the lane appears: {f}");
}

// ======================================================================= N3

// N3: "**Esc** … creates nothing."
#[test]
fn n3_esc_cancels() {
    let (mut p, _c) = prompt();
    type_str(&mut p, "Ideas");
    assert_eq!(esc(&mut p), PromptStep::Cancel);
    let (mut p, _c) = prompt();
    assert_eq!(esc(&mut p), PromptStep::Cancel, "also on an empty name");
}

// N3: "… a click outside … creates nothing." A click inside the prompt but on
// neither part keeps it open.
#[test]
fn n3_a_click_outside_cancels() {
    let (mut p, _c) = prompt();
    type_str(&mut p, "Ideas");
    let l = p.layout(W, H);
    for q in [Point::new(1.0, 1.0), Point::new(W - 1.0, 1.0), Point::new(1.0, H - 1.0), Point::new(W - 1.0, H - 1.0)] {
        assert!(!l.bounds.contains(q), "test point {q:?} is outside the prompt");
        let (mut p, _c) = prompt();
        type_str(&mut p, "Ideas");
        assert_eq!(p.click(q, W, H), PromptStep::Cancel, "a click at {q:?} outside the prompt cancels");
    }
    // Inside the prompt, off the field and the button (when there is such a
    // place): nothing.
    let probe = [
        Point::new(l.bounds.x0 + 1.0, l.bounds.y0 + 1.0),
        Point::new(l.bounds.x1 - 1.0, l.bounds.y1 - 1.0),
        Point::new(l.bounds.x0 + 1.0, l.bounds.y1 - 1.0),
    ];
    for q in probe {
        if l.field.contains(q) || l.create.contains(q) {
            continue;
        }
        assert_eq!(p.click(q, W, H), PromptStep::Open, "a click at {q:?} inside the prompt keeps it open");
    }
    assert_eq!(p.name(), "Ideas", "clicks inside the prompt keep the name");
}

// N3: "… or closing the field creates nothing", whatever the name.
#[test]
fn n3_closing_cancels() {
    let (mut p, _c) = prompt();
    type_str(&mut p, "Ideas");
    assert_eq!(p.close(), PromptStep::Cancel);
}

// N7: "Enter or Esc while an input-method composition is showing ends the
// composition and does nothing else; … the next Esc cancels".
#[test]
fn n7_esc_while_composing_ends_the_composition_first() {
    let (mut p, _c) = prompt();
    type_str(&mut p, "Draft");
    p.event(InputEvent::Ime(ImeEvent::Enabled));
    p.event(InputEvent::Ime(ImeEvent::Preedit { text: "にほ".into(), cursor: Some((6, 6)) }));
    assert_eq!(esc(&mut p), PromptStep::Open, "the first Esc ends the composition, not the prompt");
    assert_eq!(p.name(), "Draft", "nothing of the composition is committed");
    assert_eq!(esc(&mut p), PromptStep::Cancel, "the next Esc cancels");
}

// ======================================================================= N4

// N4: "The field follows the editor's single-line field rules": it is the
// editor's field (`Editor::open_field`, single line), holding what is typed.
#[test]
fn n4_the_field_is_the_editors_single_line_field() {
    let (mut p, _c) = prompt();
    type_str(&mut p, "Ideas");
    assert_eq!(p.field().field_text(), p.name(), "the name is the field's text");
    assert_eq!(p.field().document().blocks().len(), 1, "one line");
    let own = p.field_mut().accessibility_tree();
    assert!(
        own.nodes.iter().any(|(_, n)| n.role() == accesskit::Role::TextInput),
        "the field's own tree is a single-line text input (not multiline)"
    );
    assert!(!own.nodes.iter().any(|(_, n)| n.role() == accesskit::Role::MultilineTextInput));
}

// N4 (title T6, winit): a key's text reaches the field once — a letter is not
// typed twice, Space types a space, Ctrl shortcuts type nothing.
#[test]
fn n4_text_is_sent_once() {
    let (mut p, _c) = prompt();
    type_str(&mut p, "a b");
    assert_eq!(p.name(), "a b");
    ctrl(&mut p, 'z');
    ctrl(&mut p, 'z');
    ctrl(&mut p, 'z');
    assert!(!p.name().contains('\u{1a}'), "Ctrl+Z types no control character");
}

// N4 (caret): the caret moves by grapheme cluster.
#[test]
fn n4_caret_moves_by_grapheme_cluster() {
    let (mut p, _c) = prompt();
    p.event(InputEvent::Text("ae\u{301}".into()));
    assert_eq!(caret(&p), 4);
    named(&mut p, WNamed::ArrowLeft);
    assert_eq!(caret(&p), 1, "Left skips the whole cluster e + combining acute");
    named(&mut p, WNamed::Backspace);
    assert_eq!(p.name(), "e\u{301}", "Backspace removes the cluster before the caret, and only it");
    named(&mut p, WNamed::ArrowRight);
    assert_eq!(caret(&p), 3, "Right skips the whole cluster");
}

// N4 (selection): select all, then typing replaces it.
#[test]
fn n4_select_all_then_typing_replaces() {
    let (mut p, _c) = prompt();
    type_str(&mut p, "Old");
    ctrl(&mut p, 'a');
    type_str(&mut p, "New");
    assert_eq!(p.name(), "New");
    p.command(Command::SelectAll);
    type_str(&mut p, "Ideas");
    assert_eq!(p.name(), "Ideas");
}

// N4 (clipboard): copy, cut and paste go through the clipboard the shell
// gives, marked as every editor copy is.
#[test]
fn n4_copy_cut_paste_go_through_the_clipboard() {
    let (mut p, clip) = prompt();
    type_str(&mut p, "Ideas");
    ctrl(&mut p, 'a');
    ctrl(&mut p, 'c');
    assert_eq!(clip.text().as_deref(), Some("Ideas"), "Ctrl+C copies the selection");
    let w = clip.writes.borrow().last().cloned().expect("a clipboard write");
    assert!(w.privacy.exclude_from_history && w.privacy.exclude_from_cloud_sync, "marked as an editor copy: {:?}", w.privacy);
    ctrl(&mut p, 'x');
    assert_eq!(p.name(), "", "Ctrl+X cuts");
    ctrl(&mut p, 'v');
    assert_eq!(p.name(), "Ideas", "Ctrl+V pastes");
    p.command(Command::SelectAll);
    p.command(Command::Cut);
    assert_eq!(p.name(), "");
    p.command(Command::Paste);
    assert_eq!(p.name(), "Ideas");
}

// N4 (input method): the pre-edit is shown, not committed; a commit lands
// once; a cancelled composition leaves nothing.
#[test]
fn n4_input_methods_compose_in_the_field() {
    let (mut p, _c) = prompt();
    p.event(InputEvent::Ime(ImeEvent::Enabled));
    p.event(InputEvent::Ime(ImeEvent::Preedit { text: "にほん".into(), cursor: Some((9, 9)) }));
    assert_eq!(p.name(), "", "the pre-edit is not the name");
    assert!(!p.can_create(), "a pre-edit alone does not make Create active");
    p.event(InputEvent::Ime(ImeEvent::Preedit { text: String::new(), cursor: None }));
    p.event(InputEvent::Ime(ImeEvent::Commit("日本".into())));
    assert_eq!(p.name(), "日本", "committed once");
    p.event(InputEvent::Ime(ImeEvent::Preedit { text: "ご".into(), cursor: Some((3, 3)) }));
    p.event(InputEvent::Ime(ImeEvent::Preedit { text: String::new(), cursor: None }));
    p.event(InputEvent::Ime(ImeEvent::Disabled));
    assert_eq!(p.name(), "日本", "a cancelled composition leaves nothing");
    assert_eq!(enter(&mut p), PromptStep::Create("日本".into()));
}

// N7: "Enter … while an input-method composition is showing ends the
// composition and does nothing else; the next Enter creates" (as the editor
// body does, U35: the commit arrives as its own event). It never creates the
// thread with the name minus what is being composed.
#[test]
fn n7_enter_while_composing_does_not_create() {
    let (mut p, _c) = prompt();
    type_str(&mut p, "Draft ");
    p.event(InputEvent::Ime(ImeEvent::Enabled));
    p.event(InputEvent::Ime(ImeEvent::Preedit { text: "にほ".into(), cursor: Some((6, 6)) }));
    assert_eq!(enter(&mut p), PromptStep::Open, "Enter during a composition does not create");
    p.event(InputEvent::Ime(ImeEvent::Preedit { text: String::new(), cursor: None }));
    p.event(InputEvent::Ime(ImeEvent::Commit("日本".into())));
    assert_eq!(enter(&mut p), PromptStep::Create("Draft 日本".into()), "the committed name is created");
}

// N4 (bidi): the arrows move visually inside a right-to-left run.
#[test]
fn n4_bidi_in_the_field() {
    let text = "abc אבגד efg";
    let after = |c: char| text.find(c).unwrap() + c.len_utf8();
    let (mut p, _c) = prompt();
    p.event(InputEvent::Text(text.into()));
    assert_eq!(p.name(), text);
    caret_at(&mut p, after('ב'));
    named(&mut p, WNamed::ArrowRight);
    assert_eq!(caret(&p), after('א'), "Right inside the RTL run goes to logically earlier text");
    named(&mut p, WNamed::ArrowLeft);
    named(&mut p, WNamed::ArrowLeft);
    assert_eq!(caret(&p), after('ג'), "Left inside the RTL run goes to logically later text");
}

// N4 (pointer): a click in the field places the caret there — at the field's
// left edge, before the first character.
#[test]
fn n4_a_click_in_the_field_places_the_caret() {
    let (mut p, _c) = prompt();
    type_str(&mut p, "Ideas");
    assert_eq!(caret(&p), 5);
    let l = p.layout(W, H);
    let at = Point::new(l.field.x0 + 1.0, (l.field.y0 + l.field.y1) / 2.0);
    assert_eq!(p.click(at, W, H), PromptStep::Open, "a click in the field keeps the prompt open");
    assert_eq!(caret(&p), 0, "the caret goes where the click is");
}

// N4 (horizontal scroll, title T9): the prompt gives the field its own width,
// so a name wider than the field scrolls inside it and the caret stays
// visible — at the end after typing, at the start after Home. Red until T9.
#[test]
fn n4_a_long_name_scrolls_inside_the_field() {
    let (mut p, _c) = prompt();
    let l = p.layout(W, H);
    let width = l.field.width();
    p.event(InputEvent::Text("A rather long thread name that goes on ".repeat(8)));
    let visible = |p: &mut NewThreadPrompt, what: &str| {
        let r = p.field_mut().caret_rect().expect("a caret");
        assert!(r.x0 >= -0.5 && r.x1 <= width + 0.5, "{what}: the caret {r:?} stays within the field's width {width}");
    };
    visible(&mut p, "after typing");
    named(&mut p, WNamed::Home);
    visible(&mut p, "after Home");
    named(&mut p, WNamed::End);
    visible(&mut p, "after End");
}

// ======================================================================= N5

// N5: "The field is an AccessKit text input labelled "Thread name"", its value
// what is typed.
#[test]
fn n5_the_field_is_a_text_input_labelled_thread_name() {
    let (mut p, _c) = prompt();
    type_str(&mut p, "Ideas");
    let tree = a11y(&mut p);
    let inputs: Vec<_> = preorder(&tree).into_iter().filter(|(r, ..)| *r == accesskit::Role::TextInput).collect();
    assert_eq!(inputs.len(), 1, "one text input: {inputs:?}");
    assert_eq!(inputs[0].1.as_deref(), Some(FIELD_LABEL));
    let focus = tree.state().focus().expect("focus");
    assert_eq!(focus.id(), inputs[0].3, "the text input holds the focus");
    let value = if focus.supports_text_ranges() { focus.document_range().text() } else { focus.value().unwrap_or_default() };
    assert_eq!(value.trim_end_matches('\n'), "Ideas", "the announced value is the name");
}

// N5: "The prompt is an AccessKit **dialog** named "New thread", holding a
// text input labelled "Thread name" and a *Create* button": one dialog, with
// that name, and both inside it — the focus too.
#[test]
fn n5_the_prompt_is_a_dialog_named_new_thread() {
    assert_eq!(DIALOG_LABEL, "New thread", "the dialog's name of N5");
    let (mut p, _c) = prompt();
    let tree = a11y(&mut p);
    let dialogs: Vec<_> = preorder(&tree).into_iter().filter(|(r, ..)| *r == accesskit::Role::Dialog).collect();
    assert_eq!(dialogs.len(), 1, "one dialog: {:?}", preorder(&tree));
    assert_eq!(dialogs[0].1.as_deref(), Some(DIALOG_LABEL));
    let dialog = tree.state().node_by_id(dialogs[0].3).expect("the dialog node");
    let mut inside = vec![];
    fn walk(n: accesskit_consumer::Node<'_>, out: &mut Vec<(accesskit::Role, Option<String>, accesskit_consumer::NodeId)>) {
        for c in n.children() {
            out.push((c.role(), c.label(), c.id()));
            walk(c, out);
        }
    }
    walk(dialog, &mut inside);
    assert!(
        inside.iter().any(|(r, l, _)| *r == accesskit::Role::TextInput && l.as_deref() == Some(FIELD_LABEL)),
        "the text input \"Thread name\" is in the dialog: {inside:?}"
    );
    assert!(
        inside.iter().any(|(r, l, _)| *r == accesskit::Role::Button && l.as_deref() == Some(CREATE_LABEL)),
        "the Create button is in the dialog: {inside:?}"
    );
    let focus = tree.state().focus().expect("focus").id();
    assert!(inside.iter().any(|(.., id)| *id == focus), "the focus is inside the dialog");
}

// N5: "a *Create* button … Esc closes the dialog …; no Cancel button, as in
// Tauri": one button, *Create*, which can be activated. (Esc closing it:
// `n3_esc_cancels`; announced disabled while empty:
// `n2_create_is_announced_inactive_while_the_name_is_empty`.)
#[test]
fn n5_create_is_the_only_button() {
    let (mut p, _c) = prompt();
    type_str(&mut p, "Ideas");
    let update = p.accessibility_tree();
    let raw_button = update.nodes.iter().find(|(_, n)| n.role() == accesskit::Role::Button).map(|(_, n)| n.clone());
    let tree = accesskit_consumer::Tree::new(update, true);
    let order = preorder(&tree);
    let buttons: Vec<_> = order.iter().filter(|n| n.0 == accesskit::Role::Button).collect();
    assert_eq!(buttons.len(), 1, "one button, no Cancel: {order:?}");
    assert_eq!(buttons[0].1.as_deref(), Some(CREATE_LABEL));
    assert!(raw_button.expect("the button node").supports_action(accesskit::Action::Click), "Create can be activated");
}

// N5 in the shell: the prompt's tree is part of what the shell announces.
#[test]
fn n5_the_shell_announces_the_prompt() {
    let prod = code(&app_rs());
    let body = fn_body(&prod, "build_a11y_tree");
    assert!(body.contains("new_thread_prompt"), "build_a11y_tree includes the open new-thread prompt");
}

// ======================================================================= N6

// N6: "A name already used by another thread is accepted, as in Tauri."
#[test]
fn n6_a_name_already_used_is_accepted() {
    rt().block_on(async {
        let store = testing::encrypted_memory_store("n6-dup").await;
        let db = as_dyn(&store);
        let docs = service(&store);
        let a = create_named_thread(&docs, "Ideas").await.expect("first").expect("an id");
        let b = create_named_thread(&docs, "Ideas").await.expect("second, same name").expect("an id");
        assert_ne!(a, b, "two threads");
        let named: Vec<_> = db.list_threads().await.unwrap().into_iter().filter(|t| t.name == "Ideas").collect();
        assert_eq!(named.len(), 2, "both threads keep the name");
    });
}

// N6 in the prompt: it does not refuse or alter a name because it is in use
// (it does not know the threads; Create carries the name as typed, trimmed).
#[test]
fn n6_the_prompt_takes_a_used_name_as_typed() {
    let (mut p, _c) = prompt();
    type_str(&mut p, "Welcome");
    assert!(p.can_create());
    assert_eq!(enter(&mut p), PromptStep::Create("Welcome".into()));
    let src = code(&production("new_thread.rs", "#[cfg(test)]"));
    assert!(!src.contains("list_threads") && !src.contains("find_thread_by_name"), "the prompt checks no existing names");
}
