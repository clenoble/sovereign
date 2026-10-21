//! Title editing in the shell — contract T1–T7 (`doc/plans/TITLE-edit.md`,
//! decided by Céline 2026-10-17; spec ch. 12, document window, *Title*). The
//! shell's half: the save through the document service (`save_request`), the
//! card on the canvas (`Card::set_title`), a new document's window
//! (`EditorPane::start_on_title`), winit keys reaching the title once, and the
//! unsaved-edit checks the window, the delete and the restore make. The
//! editor's half (the field itself: T1, T3–T7) is in
//! `sovereign-editor/tests/t_title.rs`.
//!
//! Written before the code, by another author: whoever builds title editing
//! makes these pass without editing this file. List and red reasons:
//! `crates/sovereign-shell/TESTS-title.md`. Drawing and real keyboard focus in
//! a window are checked by hand (`EDITOR_V2_CHECKLIST.md`, section 14).
//!
//! `App` needs a window and a GPU: what only `App` does (calling
//! `start_on_title` when it creates a document, `set_title` after a save) is
//! checked by reading the source, as `step5_tests.rs` does.
//!
//! Stores: a real encrypted store over an in-memory SurrealDB
//! (`docs_host::testing`); nothing touches a real workspace.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use sovereign_ai::docs_host::{build_document_service, new_edit_session, new_user_document, testing};
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::schema::Thread;
use sovereign_db::traits::GraphDB;
use sovereign_docs::{DocError, DocumentService, Edit, Origin};
use sovereign_editor::host::{ClipboardPrivacy, PiiDisplay, SaveError, TokenDisplay};
use sovereign_editor::input::{Command, Modifiers};
use sovereign_editor::{CloseRequest, EditorPart, Fonts, Position, Selection, Trust};
use winit::keyboard::{Key as WKey, NamedKey as WNamed};

use crate::app::editor_v2::{route_key, save_request, ClipState, EditorPane, SaveState, SystemClipboard};
use crate::canvas::Card;
use crate::panels::DocWindow;
use crate::text::TextShaper;

const TEST_FONT: &[u8] = include_bytes!("../../sovereign-editor/tests/fixtures/Cantarell-Regular.ttf");
const BODY: &str = "Body text";

// ------------------------------------------------------------------ fixtures

#[derive(Default)]
struct MemClipboard(Option<String>);
impl SystemClipboard for MemClipboard {
    fn write_text(&mut self, text: &str, _privacy: ClipboardPrivacy) -> Result<(), String> {
        self.0 = Some(text.to_string());
        Ok(())
    }
    fn read_text(&mut self) -> Option<String> {
        self.0.clone()
    }
    fn change_count(&self) -> Option<u64> {
        None
    }
    fn clear(&mut self) -> Result<(), String> {
        self.0 = None;
        Ok(())
    }
}

struct NoPii;
impl PiiDisplay for NoPii {
    fn display(&self, _: &str) -> TokenDisplay {
        TokenDisplay::NotAToken
    }
    fn link_to_document(&mut self, _: &[String]) {}
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap()
}

/// A store, its document service, and one document titled "Old title" with
/// [`BODY`], created through the service.
async fn fixture(tag: &str) -> (Arc<EncryptedGraphDB>, DocumentService, String) {
    let store = testing::encrypted_memory_store(tag).await;
    let docs = build_document_service(store.clone(), Some(testing::account_key()), "desktop".into());
    let tid = store.create_thread(Thread::new("Lane".into(), String::new())).await.unwrap().id_string().unwrap();
    let id = docs.create(new_user_document("Old title", &tid, BODY)).await.unwrap().id;
    (store, docs, id)
}

/// The editor pane the shell opens on `id` (as `v2_open` does: content as
/// stored, saves based on the revision read), focused.
async fn pane_on(store: &EncryptedGraphDB, docs: &DocumentService, id: &str) -> EditorPane {
    let view = docs.read(id).await.unwrap();
    let content = store.get_document(id).await.unwrap().content;
    let mut p = pane(&view.title, content, id);
    p.save = SaveState::new(Some(view.revision));
    p
}

fn pane(title: &str, content: String, id: &str) -> EditorPane {
    let cs = ClipState::new(Box::new(MemClipboard::default()));
    let mut p = EditorPane::open(
        title,
        content,
        Trust::Owned,
        Fonts::Only(vec![TEST_FONT.to_vec()]),
        Box::new(NoPii),
        Rc::default(),
        Rc::new(RefCell::new(cs)),
        Rc::default(),
        vec![id.to_string()],
    )
    .expect("the editor opens");
    p.set_focus(true);
    p
}

/// Types `s` as winit delivers it: one character key with its text per char.
fn type_str(p: &mut EditorPane, s: &str) {
    for c in s.chars() {
        let t = c.to_string();
        route_key(&mut p.editor, &WKey::Character(t.as_str().into()), Some(&t), Modifiers::NONE);
    }
}

fn named(p: &mut EditorPane, k: WNamed, text: &str, m: Modifiers) -> sovereign_editor::EventResult {
    route_key(&mut p.editor, &WKey::Named(k), Some(text), m)
}

fn body_of(p: &EditorPane) -> String {
    p.editor.document().blocks().iter().filter_map(|b| b.text()).collect::<Vec<_>>().join("\n")
}

/// The caret at the end of the body's first block.
fn caret_in_body_end(p: &mut EditorPane) {
    let b = &p.editor.document().blocks()[0];
    let at = Position { block: b.id(), offset: b.text().unwrap_or("").len() };
    p.editor.set_selection(Selection::caret(at));
}

/// Runs the pane's queued save requests through the service, as the shell's
/// save path does, and answers the editor. Returns how many there were.
async fn run_saves(p: &mut EditorPane, docs: &DocumentService) -> usize {
    let key = p.doc_key();
    let reqs = p.take_saves();
    for r in &reqs {
        let out = save_request(docs, &key, &mut p.save, r, false, vec![]).await;
        p.save_done(r.id, out.map(|_| ()).map_err(|e| SaveError { message: format!("{e:?}") }));
    }
    reqs.len()
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

/// The body of `fn <name>` in `src` (from its signature to the next `fn` at
/// the same indentation).
fn fn_body<'a>(src: &'a str, name: &str) -> &'a str {
    let sig = format!("fn {name}(");
    let at = src.find(&sig).unwrap_or_else(|| panic!("fn {name} not found"));
    let rest = &src[at + sig.len()..];
    let end = rest.find("\n    fn ").into_iter().chain(rest.find("\n    pub(crate) fn ")).min().unwrap_or(rest.len());
    &rest[..end]
}

// ======================================================================= T2

// T2: "A title change is saved through the document service's `save` (the
// `Edit`'s title, with the revision it was read at)": what is typed in the
// title reaches the store through the service; the body is left as it was.
#[test]
fn t2_a_title_change_reaches_the_store_through_the_service() {
    rt().block_on(async {
        let (store, docs, id) = fixture("title-t2-save").await;
        let mut p = pane_on(&store, &docs, &id).await;
        p.editor.focus_title();
        type_str(&mut p, "New title");
        p.editor.command(Command::Save);
        assert_eq!(run_saves(&mut p, &docs).await, 1);
        let view = docs.read(&id).await.unwrap();
        assert_eq!(view.title, "New title");
        assert_eq!(view.plain_text(), BODY, "the body is as it was");
        assert!(!p.has_unsaved(), "saved");
    });
}

// T2: "a title-only change and a body change made together land in one save"
// — one request, one service write holding both.
#[test]
fn t2_title_and_body_together_are_one_service_save() {
    rt().block_on(async {
        let (store, docs, id) = fixture("title-t2-one").await;
        let mut p = pane_on(&store, &docs, &id).await;
        p.editor.focus_title();
        type_str(&mut p, "Both");
        caret_in_body_end(&mut p);
        type_str(&mut p, " more");
        p.editor.command(Command::Save);
        assert_eq!(run_saves(&mut p, &docs).await, 1, "one save");
        let view = docs.read(&id).await.unwrap();
        assert_eq!(view.title, "Both");
        assert_eq!(view.plain_text(), format!("{BODY} more"));
    });
}

// T2: "a conflicting save behaves as the body's does (C-DOC G11)": another
// writer saved first — the title save is refused and overwrites nothing;
// based on the current revision it lands, and the other writer's state is in
// History.
#[test]
fn t2_a_conflicting_title_save_overwrites_nothing() {
    rt().block_on(async {
        let (store, docs, id) = fixture("title-t2-conflict").await;
        let mut p = pane_on(&store, &docs, &id).await;
        p.editor.focus_title();
        type_str(&mut p, "Mine");
        p.editor.command(Command::Save);
        run_saves(&mut p, &docs).await;
        assert_eq!(docs.read(&id).await.unwrap().title, "Mine");

        // Another writer: a second editing session saves through the service
        // on the same base revision as this pane (C-DOC G11).
        let base = p.save.revision.clone().expect("the pane saves on a revision");
        let theirs = Edit {
            base,
            title: Some("Theirs".into()),
            content: None,
            origin: Origin::Editor { session: new_edit_session() },
            explicit: false,
            pasted_tokens: vec![],
        };
        docs.save(&id, theirs).await.expect("the other writer's save lands");
        assert_eq!(docs.read(&id).await.unwrap().title, "Theirs");
        p.editor.focus_title();
        type_str(&mut p, "Mine 2");
        p.editor.command(Command::Save);
        let reqs = p.take_saves();
        assert_eq!(reqs.len(), 1);
        let key = p.doc_key();
        let err = save_request(&docs, &key, &mut p.save, &reqs[0], false, vec![]).await.unwrap_err();
        let DocError::Conflict { current, .. } = err else { panic!("a conflict, as for the body: {err:?}") };
        assert_eq!(docs.read(&id).await.unwrap().title, "Theirs", "nothing overwritten");

        p.save.revision = Some(current);
        save_request(&docs, &key, &mut p.save, &reqs[0], false, vec![]).await.unwrap();
        assert_eq!(docs.read(&id).await.unwrap().title, "Mine 2");
        let commits = store.list_document_commits(&id).await.unwrap();
        assert!(commits.iter().any(|c| c.snapshot.title == "Theirs"), "the other title is in History");
    });
}

// T2: "… in the same editing session": a title-only save is the editor
// session's — ending the session versions it, as it does a body save.
#[test]
fn t2_a_title_only_save_belongs_to_the_editing_session() {
    rt().block_on(async {
        let (store, docs, id) = fixture("title-t2-session").await;
        let mut p = pane_on(&store, &docs, &id).await;
        p.editor.focus_title();
        type_str(&mut p, "Session title");
        p.editor.command(Command::Save);
        run_saves(&mut p, &docs).await;
        let before = docs.versions(&id).await.unwrap().len();
        docs.end_session(&id, p.save.session).await.unwrap();
        assert_eq!(docs.versions(&id).await.unwrap().len(), before + 1, "the session's last save became a version");
    });
}

// T2: "The card on the canvas shows the new title after the save": the card
// takes the title and its drawn layout is re-shaped for it.
#[test]
fn t2_the_card_takes_the_saved_title() {
    let mut shaper = TextShaper::new();
    let old = "A";
    let layout = shaper.shape(old, (crate::canvas::CARD_W - 2.0 * crate::canvas::PAD) as f32, crate::canvas::TITLE_PX);
    let mut card = Card {
        id: "document:t".into(),
        x: 0.0,
        lane: 0,
        external: false,
        pinned: false,
        title: old.into(),
        body: String::new(),
        stored: String::new(),
        layout,
    };
    let old_w = card.layout.width();
    let new = "A much longer title than the one before";
    card.set_title(new, &mut shaper);
    assert_eq!(card.title, new);
    assert!(card.layout.width() > old_w, "the card's layout was shaped for the new title");
}

// T2: … and the shell does it after each successful save (`App` cannot be
// built in a test: its save path is read). The production part of
// `editor_v2.rs` (where the editor's saves are run) sets a card's title.
#[test]
fn t2_the_save_path_updates_the_cards_title() {
    let prod = production("app/editor_v2.rs", "#[cfg(test)]\nmod tests");
    let code: String = prod.lines().map(|l| l.split("//").next().unwrap_or("")).collect::<Vec<_>>().join("\n");
    assert!(code.contains(".set_title("), "the editor's save path never sets the card's title");
}

// ======================================================================= T3

// T3: "Creating a document opens its window with the title field focused and
// its text selected; Enter or Tab moves the caret to the start of the body;
// Shift+Tab from the body's start comes back" — the pane a new document opens
// in, driven by winit keys as the shell routes them.
#[test]
fn t3_a_new_documents_pane_starts_on_its_title() {
    for title in ["", "Untitled"] {
        let content = sovereign_editor::doc::DocumentV2::from_text("", "seed").to_json_string();
        let mut p = pane(title, content, "document:new");
        p.start_on_title();
        assert_eq!(p.editor.part(), EditorPart::Title, "{title:?}");
        let (a, f) = p.editor.title_selection();
        assert_eq!((a.min(f), a.max(f)), (0, title.len()), "{title:?}: its text is selected");
        type_str(&mut p, "Typed name");
        assert_eq!(p.editor.title(), "Typed name", "{title:?}: typed once, replacing it");
    }
}

// T3 / T4 through winit: Enter and Tab pressed in the title (with the text
// winit gives them, "\r" and "\t") move to the body and type nothing;
// Shift+Tab from the body comes back.
#[test]
fn t3_winit_enter_tab_and_shift_tab_move_between_title_and_body() {
    for (k, t) in [(WNamed::Enter, "\r"), (WNamed::Tab, "\t")] {
        let mut p = pane("Name", sovereign_editor::doc::DocumentV2::from_text(BODY, "seed").to_json_string(), "document:t");
        p.start_on_title();
        let r = named(&mut p, k, t, Modifiers::NONE);
        assert_eq!(r, sovereign_editor::EventResult::Handled, "{k:?}: the editor's (the shell does nothing else with it)");
        assert_eq!(p.editor.part(), EditorPart::Body, "{k:?}");
        assert_eq!(p.editor.selection().focus.offset, 0, "{k:?}: the start of the body");
        assert_eq!(p.editor.title(), "Name", "{k:?}");
        assert_eq!(body_of(&p), BODY, "{k:?}: nothing typed into the body");
        // Shift+Tab from anywhere in the body (99c662b): here, its end.
        caret_in_body_end(&mut p);
        let r = named(&mut p, WNamed::Tab, "\t", Modifiers::SHIFT);
        assert_eq!(r, sovereign_editor::EventResult::Handled);
        assert_eq!(p.editor.part(), EditorPart::Title, "{k:?}: Shift+Tab comes back");
        assert_eq!(body_of(&p), BODY);
    }
}

// T3: the shell's document creation puts the new window on its title
// (`App::new_document` is read: it calls `start_on_title`).
#[test]
fn t3_creating_a_document_starts_on_its_title() {
    let prod = production("app.rs", "#[cfg(test)]\nmod taskbar_tests");
    let body = fn_body(&prod, "new_document");
    let code: String = body.lines().map(|l| l.split("//").next().unwrap_or("")).collect::<Vec<_>>().join("\n");
    assert!(code.contains("start_on_title"), "new_document does not start the new window on its title");
}

// T1 with T3, decided 2026-10-17 (99c662b): "A new document is still created
// with the stored title "Untitled"", which T3 selects so typing replaces it;
// the placeholder is only for a title cleared to empty.
#[test]
fn t1_t3_a_new_document_is_created_with_the_stored_title_untitled() {
    let prod = production("app.rs", "#[cfg(test)]\nmod taskbar_tests");
    let body = fn_body(&prod, "new_document");
    let squashed: String = body.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(squashed.contains("new_user_document(\"Untitled\""), "a new document is stored with the title \"Untitled\"");
}

// ======================================================================= T5

// T5: "A title change not yet saved counts as an unsaved edit: closing the
// window, deleting the card or restoring a version behaves exactly as with an
// unsaved body". Deleting and restoring refuse on `DocWindow::has_unsaved_edits`
// (app.rs, "This document has unsaved edits"); closing asks the editor.
#[test]
fn t5_an_unsaved_title_is_an_unsaved_edit_of_the_window() {
    let content = || sovereign_editor::doc::DocumentV2::from_text(BODY, "seed").to_json_string();
    let window = |p: EditorPane| {
        let mut shaper = TextShaper::new();
        let layout = shaper.shape("t", 100.0, 12.0);
        let mut card = Card {
            id: "document:t".into(),
            x: 0.0,
            lane: 0,
            external: false,
            pinned: false,
            title: "Old".into(),
            body: String::new(),
            stored: String::new(),
            layout,
        };
        card.set_stored(content());
        let mut dw = DocWindow::new(&card, &mut shaper, 200.0);
        dw.v2 = Some(Box::new(p));
        dw
    };

    let mut by_title = pane("Old", content(), "document:t");
    by_title.editor.focus_title();
    type_str(&mut by_title, "New");
    let mut by_body = pane("Old", content(), "document:t");
    caret_in_body_end(&mut by_body);
    type_str(&mut by_body, "x");

    let close_title = by_title.editor.request_close();
    let close_body = by_body.editor.request_close();
    assert_eq!(close_title, close_body, "closing answers as with an unsaved body");
    assert_ne!(close_title, CloseRequest::Ready);

    let dw_title = window(by_title);
    let dw_body = window(by_body);
    assert!(dw_body.has_unsaved_edits(), "baseline: an unsaved body blocks delete and restore");
    assert!(dw_title.has_unsaved_edits(), "an unsaved title blocks delete and restore the same way");
}

// T5: the unsaved prompt's Save (`v2_resolve_unsaved` → `Command::Save`)
// saves the title too.
#[test]
fn t5_the_unsaved_prompts_save_carries_the_title() {
    let mut p = pane("Old", sovereign_editor::doc::DocumentV2::from_text(BODY, "seed").to_json_string(), "document:t");
    p.editor.focus_title();
    type_str(&mut p, "Kept");
    p.editor.command(Command::Save);
    let reqs = p.take_saves();
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].title.as_deref(), Some("Kept"));
}

// ======================================================================= T4

// T4: "Leading and trailing spaces are trimmed at save; a title of spaces
// only is saved empty" — what the store holds, through the shell's path.
#[test]
fn t4_the_store_gets_the_trimmed_title() {
    rt().block_on(async {
        let (store, docs, id) = fixture("title-t4-trim").await;
        let mut p = pane_on(&store, &docs, &id).await;
        p.editor.focus_title();
        type_str(&mut p, "  Spaced out  ");
        p.editor.command(Command::Save);
        run_saves(&mut p, &docs).await;
        assert_eq!(docs.read(&id).await.unwrap().title, "Spaced out");

        p.editor.focus_title();
        type_str(&mut p, "   ");
        p.editor.command(Command::Save);
        run_saves(&mut p, &docs).await;
        assert_eq!(docs.read(&id).await.unwrap().title, "", "spaces only: saved empty");
    });
}

// ======================================================================= T8
// "Revised after Céline's manual run" (2026-10-17, main 6cd4787): "The
// window's bar keeps the title, small — 14 px [choice] (it was 21,
// `panels.rs:113` and `editor_v2.rs:1206` …), on one line, cut with an
// ellipsis … It follows the title as saved (T2)." The ellipsis is drawing,
// checked by hand (EDITOR_V2_CHECKLIST.md §14).

/// The bar's title size the contract chose.
const BAR_TITLE_PX: f32 = 14.0;

fn doc_window_titled(title: &str, shaper: &mut TextShaper) -> DocWindow {
    let layout = shaper.shape(title, 100.0, 12.0);
    let mut card = Card {
        id: "document:t".into(),
        x: 0.0,
        lane: 0,
        external: false,
        pinned: false,
        title: title.into(),
        body: String::new(),
        stored: String::new(),
        layout,
    };
    card.set_stored(sovereign_editor::doc::DocumentV2::from_text(BODY, "seed").to_json_string());
    DocWindow::new(&card, shaper, 200.0)
}

/// `layout` is `text` shaped at `px`, on one line: same width and height as
/// a reference shaping of `text` at `px` (unwrapped).
fn assert_shaped_at(layout: &parley::Layout<crate::text::Brush>, text: &str, px: f32, shaper: &mut TextShaper, what: &str) {
    let reference = shaper.shape(text, 100_000.0, px);
    assert!(
        (layout.width() - reference.width()).abs() < 0.5 && (layout.height() - reference.height()).abs() < 0.5,
        "{what}: the bar title is not {text:?} at {px} px (width {} vs {}, height {} vs {})",
        layout.width(),
        reference.width(),
        layout.height(),
        reference.height()
    );
    assert_eq!(layout.lines().count(), 1, "{what}: one line");
}

// T8: the window's bar shows the title at 14 px on one line when it opens —
// also a title far wider than any window.
#[test]
fn t8_the_window_bar_title_is_14_px_on_one_line_when_it_opens() {
    let mut shaper = TextShaper::new();
    // About 5,000 px at 14 px: wider than the 4,000 px the bar title used to
    // be shaped in, so a wrapped shaping shows.
    let long = "A very long document title that never fits ".repeat(16);
    for title in ["Minutes", long.as_str()] {
        let dw = doc_window_titled(title, &mut shaper);
        assert_shaped_at(&dw.title, title, BAR_TITLE_PX, &mut shaper, "on open");
    }
}

// T8: "It follows the title as saved" — the bar's title takes a new title,
// still 14 px on one line.
#[test]
fn t8_the_window_bar_title_follows_a_saved_title() {
    let mut shaper = TextShaper::new();
    let mut dw = doc_window_titled("Old", &mut shaper);
    let new = "The saved title, much longer than the old one";
    dw.set_title(new, &mut shaper);
    assert_shaped_at(&dw.title, new, BAR_TITLE_PX, &mut shaper, "after a save");
}

// T8, wiring (`App` cannot be built in a test: its source is read). After a
// save the bar's title goes through `DocWindow::set_title`: neither the save
// path nor `App` shapes a document window's title by hand any more (the old
// `d.title = self.shaper.shape(t, 4000.0, 21.0)`), and the save path calls
// `set_title` on the window as well as on the card.
#[test]
fn t8_the_save_path_sets_the_bar_title_through_the_window() {
    let mut found = vec![];
    for (file, marker) in [("app/editor_v2.rs", "#[cfg(test)]\nmod tests"), ("app.rs", "#[cfg(test)]\nmod taskbar_tests")] {
        let prod = production(file, marker);
        for (n, line) in prod.lines().enumerate() {
            let code: String = line.split("//").next().unwrap_or("").chars().filter(|c| !c.is_whitespace()).collect();
            if code.contains("d.title=") && code.contains("shape(") {
                found.push(format!("{file}:{}: {}", n + 1, line.trim()));
            }
        }
    }
    assert!(found.is_empty(), "a document window's bar title is shaped by hand:\n{}", found.join("\n"));
    let prod = production("app/editor_v2.rs", "#[cfg(test)]\nmod tests");
    let code: String = prod.lines().map(|l| l.split("//").next().unwrap_or("")).collect::<Vec<_>>().join("\n");
    assert!(code.matches(".set_title(").count() >= 2, "the save path sets the title of the card and of the window");
}

// ====================================================================== T10
// "Second manual run" (2026-10-17, main 595ce3d): "Wherever a title is shown
// — the card on the canvas, the window's bar, the trash, search results, the
// taskbar, lists — an empty title is displayed as *Untitled* (the stored
// title stays empty, T4). One display helper, used by every surface; the
// field keeps its dimmed placeholder (T1)."
//
// The helper is `canvas::display_title`. Surfaces a test can build (the card,
// the window bar, the canvas loader) are checked by what they shape; the ones
// only `App` draws are checked by reading the source: a document title read
// from a card or a trash line for display goes through the helper.

use crate::canvas::{display_title, CARD_W, PAD, TITLE_PX};

/// `layout` shows exactly `text` at `px` (same size as a reference shaping).
fn shows(layout: &parley::Layout<crate::text::Brush>, text: &str, max_w: f32, px: f32, shaper: &mut TextShaper) -> bool {
    let r = shaper.shape(text, max_w, px);
    (layout.width() - r.width()).abs() < 0.5 && (layout.height() - r.height()).abs() < 0.5
}

// T10: the helper — empty reads "Untitled", anything else as stored.
#[test]
fn t10_the_display_helper_reads_an_empty_title_as_untitled() {
    assert_eq!(display_title(""), "Untitled");
    for t in ["Plan", "Untitled", "Notes 2026", "\u{05e9}\u{05dc}\u{05d5}\u{05dd}"] {
        assert_eq!(display_title(t), t, "a non-empty title is shown as stored");
    }
}

// T10: the card on the canvas shows *Untitled* for an empty title; its stored
// title stays empty (the editor opens on it and shows its own placeholder).
#[test]
fn t10_the_card_shows_untitled_and_keeps_the_empty_title() {
    let mut shaper = TextShaper::new();
    let layout = shaper.shape("Old", 100.0, 12.0);
    let mut card = Card {
        id: "document:t".into(),
        x: 0.0,
        lane: 0,
        external: false,
        pinned: false,
        title: "Old".into(),
        body: String::new(),
        stored: String::new(),
        layout,
    };
    card.set_title("", &mut shaper);
    assert_eq!(card.title, "", "the card keeps the stored (empty) title");
    let w = (CARD_W - 2.0 * PAD) as f32;
    assert!(shows(&card.layout, "Untitled", w, TITLE_PX, &mut shaper), "the card shows Untitled");
}

// T10: loading the workspace — a document with an empty title
// gets a card showing *Untitled* (today: a lock and an id, the placeholder of
// an encrypted title), its title still empty.
#[test]
fn t10_a_loaded_card_with_an_empty_title_shows_untitled() {
    rt().block_on(async {
        // Written through the document service (the writers gate), read back
        // the way the shell reads its workspace.
        let store = testing::encrypted_memory_store("title-t10-load").await;
        let docs = build_document_service(store.clone(), Some(testing::account_key()), "desktop".into());
        let tid = store.create_thread(Thread::new("Lane".into(), String::new())).await.unwrap().id_string().unwrap();
        docs.create(new_user_document("", &tid, "body")).await.unwrap();
        docs.create(new_user_document("Named", &tid, "body")).await.unwrap();
        let db: Arc<dyn GraphDB> = store;
        let mut shaper = TextShaper::new();
        let (cards, ..) = crate::canvas::load_workspace(&db, &mut shaper).await.expect("a workspace");
        let w = (CARD_W - 2.0 * PAD) as f32;
        assert_eq!(cards.len(), 2);
        let named = cards.iter().find(|c| c.title == "Named").expect("titles are read in plaintext (fixture)");
        assert!(shows(&named.layout, "Named", w, TITLE_PX, &mut shaper), "a named card shows its title");
        let empty = cards.iter().find(|c| c.title != "Named").expect("the untitled document's card");
        assert_eq!(empty.title, "", "the card's title stays the stored, empty one");
        assert!(shows(&empty.layout, "Untitled", w, TITLE_PX, &mut shaper), "its card shows Untitled");
    });
}

// T10: the window's bar shows *Untitled* for an empty title, on open and
// after a save that empties it.
#[test]
fn t10_the_window_bar_shows_untitled() {
    let mut shaper = TextShaper::new();
    let dw = doc_window_titled("", &mut shaper);
    assert!(shows(&dw.title, "Untitled", 100_000.0, BAR_TITLE_PX, &mut shaper), "on open");
    let mut dw = doc_window_titled("Named", &mut shaper);
    dw.set_title("", &mut shaper);
    assert!(shows(&dw.title, "Untitled", 100_000.0, BAR_TITLE_PX, &mut shaper), "after a save");
}

/// Production lines of `file` that read a document's title for display
/// without the helper. A read is `card.title`, `c.title`, `….cards[…].title`,
/// `line.title` or `l.title` (a trash line); not a read for display: a search
/// match (`.to_lowercase()`), and the title handed to the editor (its field
/// shows its own placeholder) in the functions `allowed`.
fn raw_title_reads(file: &str, marker: &str, allowed: &[&str]) -> Vec<String> {
    raw_reads_in(&production(file, marker), file, allowed)
}

fn raw_reads_in(prod: &str, file: &str, allowed: &[&str]) -> Vec<String> {
    let reads = ["card.title", "c.title", "].title", "line.title", "l.title"];
    let mut current_fn = String::new();
    let mut out = vec![];
    for (n, line) in prod.lines().enumerate() {
        let code = line.split("//").next().unwrap_or("");
        if let Some(at) = code.find("fn ") {
            let name: String = code[at + 3..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            let before = code[..at].chars().last();
            if !name.is_empty() && before.is_none_or(|c| !c.is_alphanumeric() && c != '_') {
                current_fn = name;
            }
        }
        let hit = reads.iter().any(|r| {
            code.match_indices(r).any(|(i, _)| {
                let prev = code[..i].chars().last();
                r.starts_with(']') || prev.is_none_or(|c| !c.is_alphanumeric() && c != '_')
            })
        });
        if !hit || code.contains("display_title(") || code.contains(".to_lowercase()") || allowed.contains(&current_fn.as_str()) {
            continue;
        }
        out.push(format!("{file}:{} (fn {current_fn}): {}", n + 1, line.trim()));
    }
    out
}

// T10: every surface `App` draws — the taskbar, search results (and their
// accessible labels), the card being dragged, the unsaved prompt, History's
// title, the trash's restore chooser — and the trash rows read a document's
// title through `display_title`. Only the title handed to the editor is read
// raw (`open_doc_window`, `v2_reopen`).
#[test]
fn t10_every_surface_reads_titles_through_the_helper() {
    let mut found = vec![];
    found.extend(raw_title_reads("app.rs", "#[cfg(test)]\nmod taskbar_tests", &["open_doc_window"]));
    found.extend(raw_title_reads("app/editor_v2.rs", "#[cfg(test)]\nmod tests", &["v2_reopen"]));
    found.extend(raw_title_reads("trash_ui.rs", "#[cfg(test)]", &[]));
    assert!(found.is_empty(), "a document title is shown without display_title:\n{}", found.join("\n"));
}

// T10, the scan is not vacuous: it finds a raw read and passes a wrapped one.
#[test]
fn t10_the_surface_scan_sees_what_it_should() {
    let sample = "fn draw_x(&self) {
    let t = self.cards[i].title.clone();
    let u = display_title(&card.title);
    if card.title.to_lowercase().contains(q) {}
    let v = doc.title.clone();
}
fn open_doc_window(&mut self) {
    let t = self.cards[i].title.clone();
}
";
    let hits = raw_reads_in(sample, "sample", &["open_doc_window"]);
    assert_eq!(hits.len(), 1, "only the raw read outside the editor's path is flagged: {hits:?}");
    assert!(hits[0].starts_with("sample:2 "), "{hits:?}");
    // And the real files hold reads to check (the scan reads them).
    assert!(production("app.rs", "#[cfg(test)]\nmod taskbar_tests").contains("].title"));
    assert!(production("trash_ui.rs", "#[cfg(test)]").contains("l.title"));
}

// T10 on the phone (`frontend/src/lib/components/mobile/`): an empty title
// reads "Untitled" there too — today the canvas card shows "(untitled)".
#[test]
fn t10_the_phone_shows_untitled_too() {
    let dir = format!("{}/../../frontend/src/lib/components/mobile", env!("CARGO_MANIFEST_DIR"));
    let mut found = vec![];
    for entry in std::fs::read_dir(&dir).expect("the phone's components") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("svelte") {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        for (n, line) in src.lines().enumerate() {
            let l = line.to_lowercase();
            if l.contains("untitled") && !line.contains("'Untitled'") && !line.contains("\"Untitled\"") {
                found.push(format!("{}:{}: {}", path.file_name().unwrap().to_string_lossy(), n + 1, line.trim()));
            }
        }
    }
    assert!(found.is_empty(), "the phone shows an empty title otherwise than \"Untitled\":\n{}", found.join("\n"));
}
