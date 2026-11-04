//! v0.0.12 item 8 (`doc/plans/V0012-item8-clipboard-labels.md`): the clipboard
//! forgets this process's copies at a store change and at quit (K1, K2, K3),
//! and no raw personal-data token is drawn or announced where a title is shown
//! outside the editor (L2). L1 (the editor's own label) is in
//! `crates/sovereign-editor/tests/l1_title_label.rs`; K3 for the shell's own
//! fields (`crate::clip::FieldClipboard`) is in `clipboard_field_tests.rs`,
//! which needs a seam this file does not.
//!
//! Written before the code, by another author: whoever builds item 8 makes
//! these pass **without editing this file**; a test that looks wrong is
//! reported, not worked around.
//!
//! A child module of `app` (declared in `app.rs`), so it drives the window
//! manager's own paths (the unlock that replaces a store, opening a document,
//! its History) on a real `App`, as `editor_default_tests` does. Stores: a real
//! encrypted store over an in-memory SurrealDB with the shipped personal-data
//! hook (`docs_host::testing`), so a title written through the service holds a
//! real token. The system clipboard is a fake: nothing touches the user's
//! clipboard.
//!
//! **Uses only what exists today** (it compiles at d0f6ba1 / 7813afc).
//!
//! What a test may assert, from the contract:
//! - K1: at a store change, a copy this process made that is still on the OS
//!   clipboard is cleared — a body copy, a title copy, a shell field's copy —
//!   and its payload and source document are dropped; another application's
//!   copy, or anything this process did not write, is never cleared.
//! - K2: the same at quit, by any path; checked by reading the source (a quit
//!   needs a winit event loop), and only as "every exit of the event loop
//!   runs the forgetting".
//! - K3: a later copy replaces an earlier one, whoever made it.
//! - Unchanged: the 30-second clear of a revealed value; ordinary text is not
//!   cleared *during* a session.
//! - L2: titles can hold tokens (pinned first: the service runs ingest on the
//!   title); the card, the window bar, History's title, the trash rows, the
//!   orphans' lines and the shell's accessibility tree show a token as its
//!   label, never `[pii:`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use sovereign_ai::docs_host::{new_user_document, testing};
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::schema::Thread;
use sovereign_db::traits::GraphDB;
use sovereign_docs::{DocumentService, TextField, TokenStatus};
use sovereign_editor::host::{ClipboardContent, ClipboardPrivacy, ClipboardWrite};
use sovereign_editor::input::Command;
use sovereign_editor::{Position, Selection};

use super::editor_v2::{self, ClipState, EditorPane, SystemClipboard};
use super::{App, WindowContent};
use crate::text::TextShaper;

// ======================================================================= the fake system clipboard

/// What the fake system clipboard holds, and what was done to it.
#[derive(Default)]
struct Sys {
    text: Option<String>,
    /// The platform's change counter (Windows: the sequence number).
    count: u64,
    /// Whether the platform has a change counter (Windows) or not (the text
    /// comparison is used instead).
    has_count: bool,
    clears: usize,
}

#[derive(Clone)]
struct SysHandle(Rc<RefCell<Sys>>);

impl SysHandle {
    fn new(has_count: bool) -> SysHandle {
        SysHandle(Rc::new(RefCell::new(Sys { has_count, ..Default::default() })))
    }
    fn text(&self) -> Option<String> {
        self.0.borrow().text.clone()
    }
    fn clears(&self) -> usize {
        self.0.borrow().clears
    }
    /// Another application copies `text` (no Sovereign payload, our record
    /// untouched).
    fn other_app_copies(&self, text: &str) {
        let mut s = self.0.borrow_mut();
        s.text = Some(text.into());
        s.count += 1;
    }
}

impl SystemClipboard for SysHandle {
    fn write_text(&mut self, text: &str, _privacy: ClipboardPrivacy) -> Result<(), String> {
        let mut s = self.0.borrow_mut();
        s.text = Some(text.to_string());
        s.count += 1;
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

const ALL_PRIVATE: ClipboardPrivacy =
    ClipboardPrivacy { exclude_from_history: true, exclude_from_cloud_sync: true, exclude_from_monitoring: true };

fn item(text: &str, sovereign: Option<&str>, clear_after_s: Option<u64>) -> ClipboardWrite {
    ClipboardWrite {
        text: text.into(),
        sovereign: sovereign.map(String::from),
        privacy: ALL_PRIVATE,
        clear_after: clear_after_s.map(Duration::from_secs),
    }
}

fn clip_state(has_count: bool) -> (SysHandle, ClipState) {
    let sys = SysHandle::new(has_count);
    (sys.clone(), ClipState::new(Box::new(sys)))
}

// ======================================================================= K1 / K3 on `ClipState`

// K1: "any copy this process made that is still on the OS clipboard is
// cleared … It is no longer only a revealed value." An ordinary copy (no
// clear pending) is cleared by the forgetting — with the platform's change
// counter and without it.
#[test]
fn k1_an_ordinary_copy_is_cleared_when_forgotten() {
    for has_count in [true, false] {
        let (sys, mut cs) = clip_state(has_count);
        cs.write_from(item("ordinary document text", None, None), Duration::ZERO, Some("document:a".into()));
        assert_eq!(sys.text().as_deref(), Some("ordinary document text"));
        cs.forget();
        assert_eq!(sys.text(), None, "has_count={has_count}: our ordinary copy is cleared");
        assert_eq!(sys.clears(), 1, "has_count={has_count}: cleared once");
    }
}

// K1: "The private payload and the source document are dropped, as today" —
// and the copy itself is no longer on the clipboard.
#[test]
fn k1_a_copy_with_a_payload_is_cleared_and_its_payload_and_source_dropped() {
    for has_count in [true, false] {
        let (sys, mut cs) = clip_state(has_count);
        cs.write_from(item("Mail \u{2039}email\u{203a}", Some("payload"), None), Duration::ZERO, Some("document:a".into()));
        cs.forget();
        assert_eq!(sys.text(), None, "has_count={has_count}: the copy is cleared");
        assert_eq!(cs.read().and_then(|c| c.sovereign), None, "the payload is dropped");
        assert_eq!(cs.source_document(), None, "the source document is dropped");
    }
}

// Unchanged: a revealed value still on the clipboard is cleared by the
// forgetting too (it was before item 8).
#[test]
fn k1_a_revealed_copy_is_still_cleared_when_forgotten() {
    for has_count in [true, false] {
        let (sys, mut cs) = clip_state(has_count);
        cs.write(item("4111 1111 1111 1111", None, Some(30)), Duration::ZERO);
        cs.forget();
        assert_eq!(sys.text(), None, "has_count={has_count}");
    }
}

// K1: "Another application's copy … is never cleared": copied after ours.
#[test]
fn k1_another_applications_later_copy_is_never_cleared() {
    for has_count in [true, false] {
        for clear in [None, Some(30)] {
            let (sys, mut cs) = clip_state(has_count);
            cs.write_from(item("ours", Some("payload"), clear), Duration::ZERO, Some("document:a".into()));
            sys.other_app_copies("someone else's text");
            cs.forget();
            assert_eq!(sys.clears(), 0, "has_count={has_count}, clear={clear:?}");
            assert_eq!(sys.text().as_deref(), Some("someone else's text"));
        }
    }
}

// K1: "… or anything the clipboard holds that this process did not write, is
// never cleared": nothing of ours was ever written.
#[test]
fn k1_what_this_process_never_wrote_is_never_cleared() {
    for has_count in [true, false] {
        let (sys, mut cs) = clip_state(has_count);
        sys.other_app_copies("was there before Sovereign");
        cs.forget();
        assert_eq!(sys.clears(), 0, "has_count={has_count}");
        assert_eq!(sys.text().as_deref(), Some("was there before Sovereign"));
    }
}

// K1 with the platform's change counter: another application copying the
// very same text is still not our copy, and is left.
#[test]
fn k1_the_same_text_copied_by_another_application_is_not_ours() {
    let (sys, mut cs) = clip_state(true);
    cs.write(item("same words", None, None), Duration::ZERO);
    sys.other_app_copies("same words");
    cs.forget();
    assert_eq!(sys.clears(), 0);
    assert_eq!(sys.text().as_deref(), Some("same words"));
}

// K1 + K2: the forgetting at a store change, then at quit — the second finds
// nothing of ours and leaves what another application copied in between.
#[test]
fn k1_k2_once_forgotten_a_later_forgetting_leaves_other_copies() {
    for has_count in [true, false] {
        let (sys, mut cs) = clip_state(has_count);
        cs.write(item("ours", None, None), Duration::ZERO);
        cs.forget();
        sys.other_app_copies("copied after the lock");
        cs.forget();
        assert_eq!(sys.text().as_deref(), Some("copied after the lock"), "has_count={has_count}");
        assert_eq!(sys.clears(), 1, "has_count={has_count}: only our copy was cleared");
    }
}

// K3: "A copy made after another one replaces it": a revealed copy, then an
// ordinary one — the ordinary one is what the forgetting clears (once), and
// the earlier one's payload never comes back.
#[test]
fn k3_a_later_copy_replaces_the_earlier_one() {
    for has_count in [true, false] {
        let (sys, mut cs) = clip_state(has_count);
        cs.write_from(item("first", Some("first-payload"), Some(30)), Duration::ZERO, Some("document:a".into()));
        cs.write_from(item("second", Some("second-payload"), None), Duration::from_secs(1), Some("document:b".into()));
        assert_eq!(
            cs.read(),
            Some(ClipboardContent { text: "second".into(), sovereign: Some("second-payload".into()) }),
            "has_count={has_count}: the later copy is the current one"
        );
        assert_eq!(cs.source_document().as_deref(), Some("document:b"));
        cs.forget();
        assert_eq!(sys.text(), None, "has_count={has_count}: the later copy is cleared");
        assert_eq!(sys.clears(), 1, "has_count={has_count}: once");
    }
}

// Unchanged (ch. 12 as amended: "ordinary text is not cleared during a
// session"): the 30-second clear still clears a revealed value at its
// deadline, and still never clears an ordinary copy.
#[test]
fn unchanged_during_a_session_only_a_revealed_value_is_cleared_by_the_timer() {
    let (sys, mut cs) = clip_state(true);
    cs.write(item("revealed", None, Some(30)), Duration::ZERO);
    cs.tick(Duration::from_secs(30));
    assert_eq!(sys.text(), None, "the revealed value is cleared at its deadline");
    cs.write(item("ordinary", None, None), Duration::from_secs(40));
    cs.tick(Duration::from_secs(10_000));
    assert_eq!(sys.text().as_deref(), Some("ordinary"), "an ordinary copy stays during the session");
}

// ======================================================================= K1 through the App

/// The data directory every `App` of this module reads: `SOVEREIGN_DATA_DIR`
/// when the run sets it, otherwise a fresh temporary directory, set once for
/// the process — never the user's `~/.sovereign`.
fn ensure_test_data_dir() {
    static DIR: OnceLock<std::path::PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        if let Some(d) = std::env::var_os("SOVEREIGN_DATA_DIR") {
            return d.into();
        }
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let d = std::env::temp_dir().join(format!("sov-item8-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::env::set_var("SOVEREIGN_DATA_DIR", &d);
        d
    });
}

/// The unlock starts the one-time format migration in the background; a test
/// waits for it, as `lot1_tests::wait_for_unlock_migration` does.
fn wait_for_unlock_migration(app: &mut App) {
    if let Some(task) = app.migration_task.take() {
        let _ = app.rt.block_on(task);
    }
}

/// The unlock sequence of `App::try_login`, on `store` (no auth store, no
/// P2P) — the path that replaces a store (`install_document_service` →
/// `v2_leave_store`).
fn unlock_into(app: &mut App, store: Arc<EncryptedGraphDB>) {
    app.account_key = Some(testing::account_key());
    app.db = Some(store.clone() as Arc<dyn GraphDB>);
    app.persona = Some(sovereign_core::auth::PersonaKind::Primary);
    app.install_document_service(store);
    wait_for_unlock_migration(app);
    app.locked = false;
    app.load_workspace_now();
}

struct H {
    app: App,
    store: Arc<EncryptedGraphDB>,
    thread: String,
    sys: SysHandle,
}

fn harness(tag: &str) -> H {
    ensure_test_data_dir();
    let sys = SysHandle::new(false);
    editor_v2::use_test_system_clipboard(Box::new(sys.clone()));
    let mut app = App::new();
    let store = app.rt.block_on(testing::encrypted_memory_store(tag));
    let thread = app
        .rt
        .block_on(store.create_thread(Thread::new("Lane".into(), String::new())))
        .unwrap()
        .id_string()
        .unwrap();
    unlock_into(&mut app, store.clone());
    H { app, store, thread, sys }
}

impl H {
    fn docs(&self) -> Arc<DocumentService> {
        self.app.docs.clone().expect("the workspace is unlocked")
    }

    /// A document created through the service (personal-data ingest runs on
    /// its title and text); the canvas reloaded. Returns its id.
    fn add_doc(&mut self, title: &str, text: &str) -> String {
        let docs = self.docs();
        let out = self.app.rt.block_on(docs.create(new_user_document(title, &self.thread, text))).unwrap();
        self.app.load_workspace_now();
        out.id
    }

    fn card(&self, id: &str) -> usize {
        self.app.cards.iter().position(|c| c.id == id).unwrap_or_else(|| panic!("no card for {id}"))
    }

    fn open(&mut self, id: &str) -> usize {
        let ci = self.card(id);
        self.app.open_doc_window(ci);
        self.doc_window(id).expect("a document window opened")
    }

    fn doc_window(&self, id: &str) -> Option<usize> {
        self.app.windows.iter().position(|w| matches!(&w.content, WindowContent::Doc(d) if d.doc_id == id))
    }

    fn pane(&mut self, id: &str) -> &mut EditorPane {
        let wi = self.doc_window(id).expect("the document's window is open");
        match &mut self.app.windows[wi].content {
            WindowContent::Doc(d) => d.editor_pane_mut().expect("the window holds the editor"),
            _ => unreachable!(),
        }
    }

    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            self.app.v2_frame();
        }
    }

    /// Another store unlocked into the app: the store change (K1).
    fn replace_store(&mut self, tag: &str) {
        let other = self.app.rt.block_on(testing::encrypted_memory_store(tag));
        unlock_into(&mut self.app, other);
    }
}

// K1 through the real path: the document's body copied by the editor (as
// Ctrl+C does), then the store replaced — the copy is gone from the OS
// clipboard.
#[test]
fn k1_a_body_copy_from_the_editor_is_cleared_at_a_store_change() {
    let mut h = harness("item8-k1-body-a");
    let a = h.add_doc("Notes", "zebra body text");
    h.open(&a);
    let p = h.pane(&a);
    p.set_focus(true);
    let b = p.editor.document().blocks()[0].id();
    p.editor.set_selection(Selection::caret(Position { block: b, offset: 0 }));
    p.editor.command(Command::SelectAll);
    p.editor.command(Command::Copy);
    assert!(
        h.sys.text().is_some_and(|t| t.contains("zebra body text")),
        "the copy reached the system clipboard: {:?}",
        h.sys.text()
    );
    h.replace_store("item8-k1-body-b");
    h.frames(1);
    assert_eq!(h.sys.text(), None, "the body copy does not outlive the store");
}

// K1: "That covers a copy from a document body, from its title …" — the title
// copied in the editor (its field selects the whole title when focused, T3).
#[test]
fn k1_a_title_copy_from_the_editor_is_cleared_at_a_store_change() {
    let mut h = harness("item8-k1-title-a");
    let a = h.add_doc("zebra-title", "body");
    h.open(&a);
    let p = h.pane(&a);
    p.set_focus(true);
    p.editor.focus_title();
    p.editor.command(Command::Copy);
    assert_eq!(h.sys.text().as_deref(), Some("zebra-title"), "the title copy reached the system clipboard");
    h.replace_store("item8-k1-title-b");
    h.frames(1);
    assert_eq!(h.sys.text(), None, "the title copy does not outlive the store");
}

// K1 through the App: an ordinary copy recorded in the shared state (as every
// editor's copy is) is cleared at the store change.
#[test]
fn k1_an_ordinary_copy_is_cleared_when_the_store_is_replaced() {
    let mut h = harness("item8-k1-state-a");
    let a = h.add_doc("A", "Start");
    editor_v2::clipboard_for_tests().borrow_mut().write_from(item("zebra ordinary", None, None), Duration::ZERO, Some(a));
    h.replace_store("item8-k1-state-b");
    h.frames(1);
    assert_eq!(h.sys.text(), None, "cleared at the store change");
    let clip = editor_v2::clipboard_for_tests();
    assert_eq!(clip.borrow_mut().source_document(), None, "its source document is dropped");
}

// K1 through the App: another application's copy, made after ours, survives
// the store change untouched.
#[test]
fn k1_another_applications_copy_survives_a_store_change() {
    let mut h = harness("item8-k1-other-a");
    let a = h.add_doc("A", "Start");
    editor_v2::clipboard_for_tests().borrow_mut().write_from(item("zebra ours", Some("p"), None), Duration::ZERO, Some(a));
    h.sys.other_app_copies("from another application");
    h.replace_store("item8-k1-other-b");
    h.frames(1);
    assert_eq!(h.sys.text().as_deref(), Some("from another application"));
    assert_eq!(h.sys.clears(), 0);
}

// ======================================================================= K2: the source

/// `src` with comments and the contents of string literals replaced by
/// spaces (same length, so offsets match).
fn blank(src: &str) -> String {
    let b = src.as_bytes();
    let mut out = b.to_vec();
    let mut i = 0;
    let fill = |out: &mut Vec<u8>, from: usize, to: usize| {
        for x in out.iter_mut().take(to).skip(from) {
            if *x != b'\n' {
                *x = b' ';
            }
        }
    };
    while i < b.len() {
        if b[i] == b'/' && b.get(i + 1) == Some(&b'/') {
            let end = src[i..].find('\n').map(|e| i + e).unwrap_or(b.len());
            fill(&mut out, i, end);
            i = end;
        } else if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
            let end = src[i + 2..].find("*/").map(|e| i + 2 + e + 2).unwrap_or(b.len());
            fill(&mut out, i, end);
            i = end;
        } else if b[i] == b'"' {
            let mut j = i + 1;
            while j < b.len() && b[j] != b'"' {
                if b[j] == b'\\' {
                    j += 1;
                }
                j += 1;
            }
            fill(&mut out, i + 1, j);
            i = j + 1;
        } else if b[i] == b'\'' && b.get(i + 2) == Some(&b'\'') {
            // A one-byte char literal ('{', '"', …).
            fill(&mut out, i + 1, i + 2);
            i += 3;
        } else if b[i] == b'\'' && b.get(i + 1) == Some(&b'\\') {
            // An escaped char literal ('\n', '\'', '\u{…}').
            let end = src[i + 2..].find('\'').map(|e| i + 2 + e + 1).unwrap_or(b.len());
            fill(&mut out, i + 1, end - 1);
            i = end;
        } else {
            i += 1;
        }
    }
    String::from_utf8(out).unwrap()
}

/// The production part of a shell source file: up to its inline test module.
fn production(file: &str) -> String {
    let path = format!("{}/src/{file}", env!("CARGO_MANIFEST_DIR"));
    let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let cut = ["#[cfg(test)]\nmod tests", "#[cfg(test)]\nmod taskbar_tests"]
        .iter()
        .filter_map(|m| src.find(m))
        .min()
        .unwrap_or(src.len());
    blank(&src[..cut])
}

/// The byte range of the `{ … }` that opens at or after `from`.
fn block_from(src: &str, from: usize) -> (usize, usize) {
    let open = from + src[from..].find('{').expect("a block");
    let mut depth = 0usize;
    for (k, c) in src[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return (open, open + k + 1);
                }
            }
            _ => {}
        }
    }
    panic!("an unclosed block")
}

/// The body of `fn <name>` in `src`, if there is one.
fn fn_body<'a>(src: &'a str, name: &str) -> Option<&'a str> {
    let at = src.find(&format!("fn {name}("))?;
    let (a, b) = block_from(src, at);
    Some(&src[a..b])
}

/// The forgetting: a call to anything named `…forget…` (`ClipState::forget`,
/// or a helper wrapping it), or the store change's own path
/// (`v2_leave_store`), which runs it.
fn calls_the_forgetting(code: &str) -> bool {
    let mut rest = code;
    while let Some(i) = rest.find("forget") {
        let after = &rest[i..];
        let ident_end = after.find(|c: char| !(c.is_alphanumeric() || c == '_')).unwrap_or(after.len());
        if after[ident_end..].trim_start().starts_with('(') {
            return true;
        }
        rest = &rest[i + 6..];
    }
    code.contains("v2_leave_store(")
}

/// The function enclosing byte `at` in `src` (the last `fn ` before it whose
/// block contains it).
fn enclosing_fn(src: &str, at: usize) -> &str {
    let mut best: Option<(usize, usize)> = None;
    let mut search = 0;
    while let Some(i) = src[search..at].find("fn ") {
        let start = search + i;
        if let Some(open) = src[start..].find('{').map(|o| start + o) {
            if open < at {
                let (a, b) = block_from(src, start);
                if a <= at && at < b {
                    best = Some((a, b));
                }
            }
        }
        search = start + 3;
    }
    let (a, b) = best.expect("an exit inside a function");
    &src[a..b]
}

// K2: "The same, when the app quits by any path: the window closed, a
// confirmed quit, or the last window." A quit needs a winit event loop, so
// the source is read: either the event loop's `exiting` handler runs the
// forgetting (winit calls it on every exit), or every `event_loop.exit()`
// is reached only after the forgetting ran — in the same function, or in
// `request_quit`, the gate the function asked.
#[test]
fn k2_every_quit_path_runs_the_forgetting() {
    let app = production("app.rs");
    let handler = app.find("impl ApplicationHandler for App").expect("the event loop's handler");
    let (ha, hb) = block_from(&app, handler);
    let handler_src = &app[ha..hb];
    if let Some(body) = fn_body(handler_src, "exiting") {
        assert!(calls_the_forgetting(body), "`exiting` does not run the clipboard's forgetting:\n{body}");
        return;
    }
    let quit_gate = fn_body(&app, "request_quit").map(calls_the_forgetting).unwrap_or(false);
    let mut sites = vec![];
    let mut missing = vec![];
    for file in ["app.rs", "app/editor_v2.rs"] {
        let src = production(file);
        let mut from = 0;
        while let Some(i) = src[from..].find(".exit()") {
            let at = from + i;
            from = at + 7;
            let line = src[..at].matches('\n').count() + 1;
            sites.push(format!("{file}:{line}"));
            let f = enclosing_fn(&src, at);
            let before = &f[..f.find(".exit()").unwrap_or(f.len())];
            let ok = calls_the_forgetting(before) || (before.contains("request_quit(") && quit_gate);
            if !ok {
                missing.push(format!("{file}:{line}"));
            }
        }
    }
    assert!(sites.len() >= 2, "the scan sees the quit paths (window closed, confirmed quit): {sites:?}");
    assert!(missing.is_empty(), "these exits of the event loop do not run the clipboard's forgetting first: {missing:?}");
}

// The scan itself, on samples: what counts as the forgetting, and what does
// not (a comment, a string, a name without a call).
#[test]
fn k2_the_scan_sees_what_it_should() {
    assert!(calls_the_forgetting(&blank("CLIPBOARD.with(|c| c.borrow_mut().forget());")));
    assert!(calls_the_forgetting(&blank("editor_v2::forget_clipboard ();")));
    assert!(calls_the_forgetting(&blank("self.v2_leave_store();")));
    assert!(!calls_the_forgetting(&blank("// then forget() the copy\nlet x = 1;")));
    assert!(!calls_the_forgetting(&blank("let s = \"forget()\";")));
    assert!(!calls_the_forgetting(&blank("let forgotten = true;")));
    let src = blank("fn a() { if x { y(); } }\nfn b() { if self.request_quit() { event_loop.exit(); } }");
    let at = src.find(".exit()").unwrap();
    assert!(enclosing_fn(&src, at).contains("request_quit"));
}

// ======================================================================= L2: titles outside the editor

/// The first token in the stored title of `id`, with its label in this
/// document; and the title as the service labels it.
fn title_token(h: &H, id: &str) -> (String, String) {
    let docs = h.docs();
    let v = h.app.rt.block_on(docs.read(id)).unwrap();
    let label = v
        .tokens
        .iter()
        .find_map(|t| match (&t.field, &t.status) {
            (TextField::Title, TokenStatus::Token { label }) => Some(label.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("the title holds a token: {:?} / {:?}", v.title, v.tokens));
    (label, v.labelled_title())
}

/// A document whose title holds a personal-data token, written as the user
/// writes it (through the service, which runs ingest on the title).
const PII_TITLE: &str = "Mail alice.zebra@example.com today";

fn shows(layout: &parley::Layout<crate::text::Brush>, text: &str, max_w: f32, px: f32, shaper: &mut TextShaper) -> bool {
    let r = shaper.shape(text, max_w, px);
    (layout.width() - r.width()).abs() < 0.5 && (layout.height() - r.height()).abs() < 0.5
}

// L2: "If titles cannot hold tokens at all … a test pins that instead." They
// can: the service runs personal-data ingest on the title (G10, decided
// 2026-10-11), so a title written through it is stored with a token, which
// is a token in its document. This passes today, and is why the L2 tests
// below apply.
#[test]
fn l2_titles_can_hold_tokens() {
    let mut h = harness("item8-l2-pin");
    let a = h.add_doc(PII_TITLE, "body");
    let docs = h.docs();
    let v = h.app.rt.block_on(docs.read(&a)).unwrap();
    assert!(v.title.contains("[pii:"), "the stored title holds a token: {:?}", v.title);
    assert!(!v.title.contains("alice.zebra@example.com"), "the value is not stored in the title");
    let (label, labelled) = title_token(&h, &a);
    assert!(!label.is_empty() && !label.contains("[pii:"));
    assert!(labelled.contains(&label) && !labelled.contains("[pii:"), "the service's labelled title: {labelled:?}");
}

// L2: the card on the canvas draws the title with the token as its label.
#[test]
fn l2_the_card_shows_a_tokens_label() {
    let mut h = harness("item8-l2-card");
    let a = h.add_doc(PII_TITLE, "body");
    let (_, labelled) = title_token(&h, &a);
    let ci = h.card(&a);
    let w = (crate::canvas::CARD_W - 2.0 * crate::canvas::PAD) as f32;
    let mut shaper = TextShaper::new();
    let card = &h.app.cards[ci];
    assert!(
        shows(&card.layout, crate::canvas::display_title(&labelled), w, crate::canvas::TITLE_PX, &mut shaper),
        "the card draws the labelled title {labelled:?} (it holds {:?})",
        card.title
    );
}

// L2: the document window's bar draws the title with the token as its label
// (14 px, one line: title contract T8).
#[test]
fn l2_the_window_bar_shows_a_tokens_label() {
    let mut h = harness("item8-l2-bar");
    let a = h.add_doc(PII_TITLE, "body");
    let (_, labelled) = title_token(&h, &a);
    let wi = h.open(&a);
    let mut shaper = TextShaper::new();
    let WindowContent::Doc(d) = &h.app.windows[wi].content else { unreachable!() };
    assert!(
        shows(&d.title, crate::canvas::display_title(&labelled), 100_000.0, 14.0, &mut shaper),
        "the window bar draws the labelled title {labelled:?}"
    );
}

// L2: History's title (drawn as "History — <title>", announced as "History
// of <title>") holds the token's label, never `[pii:`. Opened by the path that
// opens it from a document (`open_previous_version`, as the alerts do; the
// window's History button takes its title the same way, from the card).
#[test]
fn l2_historys_title_shows_a_tokens_label() {
    let mut h = harness("item8-l2-history");
    let a = h.add_doc(PII_TITLE, "body");
    let (label, _) = title_token(&h, &a);
    h.app.open_previous_version(&a, "commit:none");
    let hp = h
        .app
        .windows
        .iter()
        .find_map(|w| match &w.content {
            WindowContent::History(hp) if hp.doc_id == a => Some(hp),
            _ => None,
        })
        .expect("History opened");
    assert!(!hp.doc_title.contains("[pii:"), "History's title holds a raw token: {:?}", hp.doc_title);
    assert!(hp.doc_title.contains(&label), "History's title shows the label {label:?}: {:?}", hp.doc_title);
}

// L2: the trash's rows show the title with the token as its label.
#[test]
fn l2_the_trash_rows_show_a_tokens_label() {
    let mut h = harness("item8-l2-trash");
    let a = h.add_doc(PII_TITLE, "body");
    let (label, _) = title_token(&h, &a);
    let docs = h.docs();
    h.app.rt.block_on(docs.trash(&a)).unwrap();
    let view = h
        .app
        .rt
        .block_on(crate::trash::TrashView::load(&docs, h.store.as_ref(), chrono::Utc::now()))
        .expect("the trash loads");
    let (rows, _) = crate::trash_ui::settings_rows(Some(&view));
    let texts: Vec<String> = rows.iter().flat_map(|(_, k, v)| [k.clone(), v.clone()]).collect();
    assert!(texts.iter().all(|t| !t.contains("[pii:")), "a trash row holds a raw token: {texts:?}");
    assert!(texts.iter().any(|t| t.contains(&label)), "a trash row shows the label {label:?}: {texts:?}");
}

// L2: the orphans' lines (a document whose thread was deleted, in Alerts)
// show and announce the title with the token as its label.
#[test]
fn l2_the_orphans_lines_show_a_tokens_label() {
    let mut h = harness("item8-l2-orphans");
    let a = h.add_doc(PII_TITLE, "body");
    let (label, _) = title_token(&h, &a);
    let thread = h.thread.clone();
    h.app.rt.block_on(h.store.soft_delete_thread(&thread)).unwrap();
    let docs = h.docs();
    let section = h
        .app
        .rt
        .block_on(crate::orphans::OrphansSection::load(&docs, &HashMap::new()))
        .expect("the section loads");
    let line = section.lines().iter().find(|l| l.id == a).expect("the document is an orphan");
    let mut texts = vec![line.notice()];
    texts.extend(section.a11y_lines());
    assert!(texts.iter().all(|t| !t.contains("[pii:")), "an orphan line holds a raw token: {texts:?}");
    assert!(line.notice().contains(&label), "the line shows the label {label:?}: {:?}", line.notice());
}

// L2: nothing the shell announces holds a raw token — the canvas, the
// document's window (its editor inlined) and its History open.
#[test]
fn l2_the_shells_accessibility_tree_holds_no_raw_token() {
    let mut h = harness("item8-l2-a11y");
    let a = h.add_doc(PII_TITLE, "body");
    h.open(&a);
    h.app.open_previous_version(&a, "commit:none");
    h.frames(2);
    let tree = h.app.build_a11y_tree();
    let mut raw = vec![];
    for (_, n) in &tree.nodes {
        for s in [n.label(), n.value(), n.description()].into_iter().flatten() {
            if s.contains("[pii:") {
                raw.push(s.to_string());
            }
        }
    }
    assert!(raw.is_empty(), "the shell announces a raw token: {raw:?}");
}
