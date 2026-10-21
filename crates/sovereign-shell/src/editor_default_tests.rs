//! The editor in every build, the plain text path removed — contract E1–E9
//! (`doc/plans/EDITOR-default.md`, decided by Céline 2026-10-19, with the
//! additions after PANDA2's comments, claude-coord 0115).
//!
//! Written before the code, by another author: whoever makes the editor the
//! shell's only document editor makes these pass **without editing this
//! file**; a test that looks wrong is reported, not worked around. List, red
//! reasons and the readings chosen: `crates/sovereign-shell/TESTS-editor-default.md`.
//!
//! A child module of `app` (declared in `app.rs`), so it calls the window
//! manager's own paths — open, close, quit, run a skill, restore, trash, the
//! store's replacement at an unlock — on a real `App`. `App::new()` needs no window and no GPU (the
//! headless `run_shot` builds one the same way); `App::build_scene` lays a
//! frame out without rendering it. What only a winit event can reach (a key
//! press: Esc) and what is about the build itself (what is compiled into a
//! release build) is checked by reading the source.
//!
//! Stores: a real encrypted store over an in-memory SurrealDB
//! (`docs_host::testing`); the data directory is a fresh temporary one when
//! `SOVEREIGN_DATA_DIR` is not set; the clipboard is an in-memory one. Nothing
//! touches a real workspace or the user's clipboard.
//!
//! **Release builds.** This module is compiled under `cargo test --release`
//! too: the behavioural tests then run with `debug_assertions` off — the
//! release cfg (E1 "in debug and release builds alike", E8). Until the editor
//! is compiled into release builds, that run does not compile (the module
//! names `editor_v2`): that is E8's red, not a fault of the tests.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use sovereign_ai::docs_host::{new_user_document, testing};
use sovereign_core::interfaces::{ChatEnd, ChatOutcome, OrchestratorEvent, WriteOutcome, WriteResult};
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::schema::{Document, Thread};
use sovereign_db::traits::GraphDB;
use sovereign_docs::{DocumentService, Edit, EditContent, Origin, Revision};
use sovereign_editor::host::{ClipboardPrivacy, ClipboardWrite, PiiDisplay, TokenDisplay};
use sovereign_editor::input::{Command, Modifiers};
use sovereign_editor::{EditorPart, Fonts, Position, Selection, Trust};
use winit::keyboard::Key as WKey;

use super::editor_v2::{self, route_key, save_request, ClipState, EditorPane, SaveState, SystemClipboard};
use super::{App, UnsavedChoice, WindowContent};
use crate::panels::DocBtn;

const TEST_FONT: &[u8] = include_bytes!("../../sovereign-editor/tests/fixtures/Cantarell-Regular.ttf");

// ======================================================================= fixtures

/// The data directory every `App` of this module reads: `SOVEREIGN_DATA_DIR`
/// when the run sets it (it should), otherwise a fresh temporary directory,
/// set once for the process — never the user's `~/.sovereign`.
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
        let d = std::env::temp_dir().join(format!("sov-editor-default-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::env::set_var("SOVEREIGN_DATA_DIR", &d);
        d
    });
}

/// An in-memory system clipboard.
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

/// A shell unlocked on an encrypted in-memory store, with one thread.
struct H {
    app: App,
    store: Arc<EncryptedGraphDB>,
    thread: String,
}

/// The unlock sequence of `App::try_login`, on `store` (no auth store, no P2P).
fn unlock_into(app: &mut App, store: Arc<EncryptedGraphDB>) {
    app.account_key = Some(testing::account_key());
    app.db = Some(store.clone() as Arc<dyn GraphDB>);
    app.persona = Some(sovereign_core::auth::PersonaKind::Primary);
    app.install_document_service(store);
    app.locked = false;
    app.load_workspace_now();
}

fn harness(tag: &str) -> H {
    ensure_test_data_dir();
    editor_v2::use_test_system_clipboard(Box::new(MemClipboard::default()));
    let mut app = App::new();
    let store = app.rt.block_on(testing::encrypted_memory_store(tag));
    let thread = app
        .rt
        .block_on(store.create_thread(Thread::new("Lane".into(), String::new())))
        .unwrap()
        .id_string()
        .unwrap();
    unlock_into(&mut app, store.clone());
    H { app, store, thread }
}

/// The id a canvas card carries for a document the service created.
fn card_id(raw: &str) -> String {
    sovereign_db::schema::raw_to_thing(raw).map(|t| t.to_string()).unwrap_or_else(|| raw.to_string())
}

impl H {
    fn docs(&self) -> Arc<DocumentService> {
        self.app.docs.clone().expect("the workspace is unlocked")
    }

    /// A document created through the service; the canvas reloaded. Returns
    /// its card id and its creation version.
    fn add_doc_v(&mut self, title: &str, text: &str) -> (String, String) {
        let docs = self.docs();
        let out = self.app.rt.block_on(docs.create(new_user_document(title, &self.thread, text))).unwrap();
        self.app.load_workspace_now();
        (card_id(&out.id), out.version)
    }

    fn add_doc(&mut self, title: &str, text: &str) -> String {
        self.add_doc_v(title, text).0
    }

    /// A row as writers before the service left it (any stored content).
    fn add_raw(&mut self, title: &str, content: &str) -> String {
        let mut d = Document::new(title.into(), self.thread.clone(), true);
        d.content = content.into();
        let id = self.app.rt.block_on(self.store.create_document(d)).unwrap().id_string().unwrap();
        self.app.load_workspace_now();
        card_id(&id)
    }

    fn card(&self, id: &str) -> usize {
        self.app.cards.iter().position(|c| c.id == id).unwrap_or_else(|| panic!("no card for {id}"))
    }

    fn doc_window(&self, id: &str) -> Option<usize> {
        self.app.windows.iter().position(|w| matches!(&w.content, WindowContent::Doc(d) if d.doc_id == id))
    }

    fn doc_windows(&self) -> usize {
        self.app.windows.iter().filter(|w| matches!(&w.content, WindowContent::Doc(_))).count()
    }

    /// Opens the document from its card, as a click does; returns the window.
    fn open(&mut self, id: &str) -> usize {
        let ci = self.card(id);
        self.app.open_doc_window(ci);
        self.doc_window(id).expect("a document window opened")
    }

    fn pane(&mut self, id: &str) -> &mut EditorPane {
        let wi = self.doc_window(id).expect("the document's window is open");
        match &mut self.app.windows[wi].content {
            WindowContent::Doc(d) => d.editor_pane_mut().expect("the window holds the editor (E1)"),
            _ => unreachable!(),
        }
    }

    fn has_editor(&self, wi: usize) -> bool {
        matches!(&self.app.windows[wi].content, WindowContent::Doc(d) if d.editor_pane().is_some())
    }

    /// Types `s` at the end of the body, as winit delivers it.
    fn type_in(&mut self, id: &str, s: &str) {
        let p = self.pane(id);
        p.set_focus(true);
        let b = &p.editor.document().blocks()[0];
        let at = Position { block: b.id(), offset: b.text().unwrap_or("").len() };
        p.editor.set_selection(Selection::caret(at));
        for c in s.chars() {
            let t = c.to_string();
            route_key(&mut p.editor, &WKey::Character(t.as_str().into()), Some(&t), Modifiers::NONE);
        }
    }

    fn editor_text(&mut self, id: &str) -> String {
        self.pane(id).editor.document().plain_text()
    }

    /// Asks the editor to save now (as Ctrl+S does), then runs frames.
    fn save(&mut self, id: &str) {
        self.pane(id).editor.command(Command::Save);
        self.frames(3);
    }

    /// The shell's per-frame editor work (saves, closes waiting for a save).
    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            self.app.v2_frame();
        }
    }

    /// One whole frame laid out (orchestrator events drained, editors run).
    fn scene(&mut self) {
        self.app.build_scene(1400.0, 900.0);
    }

    fn stored_text(&self, id: &str) -> String {
        let docs = self.docs();
        self.app.rt.block_on(docs.read(id)).unwrap().plain_text()
    }

    fn versions(&self, id: &str) -> usize {
        let docs = self.docs();
        self.app.rt.block_on(docs.versions(id)).unwrap().len()
    }

    /// The assistant rewrites `id` through the service (its write tools).
    fn assistant_writes(&self, id: &str, text: &str) {
        let docs = self.docs();
        let base = self.app.rt.block_on(docs.read(id)).unwrap().revision;
        let edit = Edit {
            base,
            title: None,
            content: Some(EditContent::PlainText(text.into())),
            origin: Origin::Assistant,
            explicit: false,
            pasted_tokens: vec![],
        };
        self.app.rt.block_on(docs.save(id, edit)).unwrap();
    }

    fn history_holds(&self, id: &str, needle: &str) -> bool {
        let commits = self.app.rt.block_on(self.store.list_document_commits(id)).unwrap();
        commits.iter().any(|c| c.snapshot.content.contains(needle))
    }
}

/// What the notice on screen says (title, body), if one is shown.
fn notice_text(app: &App) -> Option<String> {
    app.notice.as_ref().map(|n| format!("{}\n{}", n.said.0, n.said.1))
}

/// E3 and its precision: a notice in plain words — never "the usual way",
/// never the development name, never stored JSON or ciphertext, and nothing
/// that says or implies another persona or another store.
fn assert_plain_notice(said: &str, never: &[&str]) {
    let low = said.to_lowercase();
    assert!(!said.trim().is_empty(), "the notice says why");
    assert!(!low.contains("usual way"), "no other view to fall back to: {said}");
    assert!(!low.contains("editor v2"), "notice titles say \"Document\": {said}");
    assert!(!said.contains("{\""), "never stored JSON: {said}");
    for n in never {
        assert!(!said.contains(n), "the notice quotes what is stored ({n}): {said}");
    }
    for w in ["persona", "duress", "decoy", "another store", "other store", "another workspace", "other workspace"] {
        assert!(!low.contains(w), "a notice never implies another persona or store ({w}): {said}");
    }
}

// ------------------------------------------------------------------ source reading

/// `src` with comments, and the contents of string and char literals when
/// `literals`, replaced by spaces — the same byte length, so offsets match.
fn blank(src: &str, literals: bool) -> String {
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
            let mut depth = 0;
            let mut j = i;
            while j < b.len() {
                if b[j] == b'/' && b.get(j + 1) == Some(&b'*') {
                    depth += 1;
                    j += 2;
                } else if b[j] == b'*' && b.get(j + 1) == Some(&b'/') {
                    depth -= 1;
                    j += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    j += 1;
                }
            }
            fill(&mut out, i, j);
            i = j;
        } else if b[i] == b'r' && (b.get(i + 1) == Some(&b'"') || b.get(i + 1) == Some(&b'#')) && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_')) {
            let mut j = i + 1;
            let mut hashes = 0;
            while b.get(j) == Some(&b'#') {
                hashes += 1;
                j += 1;
            }
            if b.get(j) != Some(&b'"') {
                i += 1;
                continue;
            }
            let close = format!("\"{}", "#".repeat(hashes));
            let end = src[j + 1..].find(&close).map(|e| j + 1 + e).unwrap_or(b.len());
            if literals {
                fill(&mut out, j + 1, end);
            }
            i = end + close.len();
        } else if b[i] == b'"' {
            let mut j = i + 1;
            while j < b.len() && b[j] != b'"' {
                if b[j] == b'\\' {
                    j += 1;
                }
                j += 1;
            }
            if literals {
                fill(&mut out, i + 1, j);
            }
            i = j + 1;
        } else if b[i] == b'\'' {
            // A char literal, or a lifetime (left alone).
            let rest = &src[i + 1..];
            // `e`: the length of the literal's contents ('x' → 1, '\n' → 2).
            let lit_end = match rest.chars().next() {
                Some('\\') => rest.get(2..).and_then(|r| r.find('\'')).map(|e| e + 2),
                Some(c) => {
                    let l = c.len_utf8();
                    rest[l..].starts_with('\'').then_some(l)
                }
                None => None,
            };
            match lit_end {
                Some(e) => {
                    if literals {
                        fill(&mut out, i + 1, i + 1 + e);
                    }
                    i = i + 2 + e;
                }
                None => i += 1,
            }
        } else {
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The index just past the bracket closing the one at `open` (in `code`,
/// where literals and comments are blanked).
fn matching(code: &str, open: usize) -> usize {
    let b = code.as_bytes();
    let mut depth = 0i32;
    for (j, &c) in b.iter().enumerate().skip(open) {
        match c {
            b'{' | b'(' | b'[' => depth += 1,
            b'}' | b')' | b']' => {
                depth -= 1;
                if depth == 0 {
                    return j + 1;
                }
            }
            _ => {}
        }
    }
    b.len()
}

/// Skips spaces and attributes (`#[...]`) from `i` in `code`.
fn skip_attrs(code: &str, mut i: usize) -> usize {
    let b = code.as_bytes();
    loop {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if b.get(i) == Some(&b'#') && b.get(i + 1) == Some(&b'[') {
            i = matching(code, i + 1);
        } else {
            return i;
        }
    }
}

/// The source of a shell file (path under `src/`) with its test modules
/// (`#[cfg(test)] mod … { … }`) and comments blanked; string literals kept.
fn production(rel: &str) -> String {
    let src = std::fs::read_to_string(format!("{}/src/{rel}", env!("CARGO_MANIFEST_DIR"))).unwrap();
    production_of(&src)
}

fn production_of(src: &str) -> String {
    let text = blank(src, false);
    let code = blank(src, true);
    let mut out = text.into_bytes();
    let mut from = 0;
    while let Some(at) = code[from..].find("#[cfg(test)]").map(|a| a + from) {
        let next = skip_attrs(&code, at + "#[cfg(test)]".len());
        let rest = &code[next..];
        let is_mod = rest.starts_with("mod ") || rest.starts_with("pub mod ") || rest.starts_with("pub(crate) mod ");
        let brace = rest.find(['{', ';']).map(|e| next + e);
        if let (true, Some(b)) = (is_mod, brace) {
            if code.as_bytes()[b] == b'{' {
                let end = matching(&code, b);
                for x in out.iter_mut().take(end).skip(at) {
                    if *x != b'\n' {
                        *x = b' ';
                    }
                }
                from = end;
                continue;
            }
        }
        from = at + 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Every production `.rs` file under `src/` (every subdirectory), test
/// files (`*_tests.rs`) excepted: (path under `src/`, production text).
fn production_files() -> Vec<(String, String)> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = Vec::new();
    let mut dirs = vec![root.clone()];
    while let Some(dir) = dirs.pop() {
        for e in std::fs::read_dir(&dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                dirs.push(p);
                continue;
            }
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            if !name.ends_with(".rs") || name.ends_with("_tests.rs") {
                continue;
            }
            let rel = p.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
            out.push((rel.clone(), production(&rel)));
        }
    }
    assert!(out.len() >= 20, "scanned only {} files", out.len());
    out
}

/// The body of `fn <name>` in a production text (signature to closing brace).
fn fn_body(text: &str, name: &str) -> Option<String> {
    let code = blank(text, true);
    let sig = format!("fn {name}(");
    let at = code.find(&sig)?;
    let open = code[at..].find('{').map(|e| at + e)?;
    Some(text[at..matching(&code, open)].to_string())
}

/// Whether a `#[cfg(...)]` attribute's text compiles its item only in
/// development builds (or only in tests).
fn dev_only_cfg(attr: &str) -> bool {
    let inner = attr.trim_start_matches("#[cfg(").trim_start();
    (attr.contains("debug_assertions") && attr.contains("dev-switches") && !inner.starts_with("not("))
        || inner.starts_with("test)")
}

/// The byte ranges of `text` covered by an item under a development-only (or
/// test-only) `#[cfg(...)]`: from the attribute to the end of its item — the
/// first `;` or `,` at its level, the end of its first `{…}` block, or the
/// end of the enclosing block.
fn dev_gated_ranges(text: &str) -> Vec<(usize, usize)> {
    let code = blank(text, true);
    let b = code.as_bytes();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(at) = code[from..].find("#[cfg(").map(|a| a + from) {
        let attr_end = matching(&code, at + 1);
        from = attr_end;
        if !dev_only_cfg(&text[at..attr_end]) {
            continue;
        }
        let mut i = skip_attrs(&code, attr_end);
        let mut depth = 0i32;
        let end = loop {
            if i >= b.len() {
                break b.len();
            }
            match b[i] {
                b'(' | b'[' => depth += 1,
                b')' | b']' => depth -= 1,
                b'{' if depth == 0 => break matching(&code, i),
                b'{' => depth += 1,
                b'}' if depth == 0 => break i,
                b'}' => depth -= 1,
                b';' | b',' if depth == 0 => break i + 1,
                _ => {}
            }
            i += 1;
        };
        out.push((at, end));
    }
    out
}

/// Every place in `text` (a production text) that reads a development
/// switch: a `dev::var(` / `dev::flag(` call or a `"SHELL_…"` literal, with
/// its line.
fn dev_switch_reads(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for pat in ["dev::var(", "dev::flag(", "\"SHELL_"] {
        let mut from = 0;
        while let Some(at) = text[from..].find(pat).map(|a| a + from) {
            let line_no = text[..at].matches('\n').count() + 1;
            let line = text.lines().nth(line_no - 1).unwrap_or("").trim().to_string();
            out.push((at, format!("{line_no}: {line}")));
            from = at + pat.len();
        }
    }
    out
}

// The source helpers read what they should (self-test on a sample).
#[test]
fn the_source_helpers_see_what_they_should() {
    let sample = r#"
fn a() {
    // "SHELL_IN_A_COMMENT"
    let s = "a { brace in a string";
    #[cfg(any(debug_assertions, feature = "dev-switches"))]
    let gated = crate::dev::var("SHELL_ONE").is_some_and(|f| { f.len() > 1 });
    let open = crate::dev::flag("SHELL_TWO");
    let c = '{';
}
#[cfg(any(debug_assertions, feature = "dev-switches"))]
impl X {
    fn b() { let _ = crate::dev::flag("SHELL_THREE"); }
}
#[cfg(not(any(debug_assertions, feature = "dev-switches")))]
fn c() { let _ = crate::dev::flag("SHELL_FOUR"); }
#[cfg(test)]
mod tests {
    fn t() { let _ = "SHELL_IN_TESTS"; }
}
"#;
    let text = production_of(sample);
    assert!(!text.contains("SHELL_IN_A_COMMENT") && !text.contains("SHELL_IN_TESTS"));
    let gated = dev_gated_ranges(&text);
    let ungated: Vec<String> = dev_switch_reads(&text)
        .into_iter()
        .filter(|(at, _)| !gated.iter().any(|(s, e)| s <= at && at < e))
        .map(|(_, l)| l)
        .collect();
    assert_eq!(ungated.len(), 4, "SHELL_TWO and SHELL_FOUR, each as a call and a literal: {ungated:?}");
    assert!(ungated.iter().all(|l| l.contains("SHELL_TWO") || l.contains("SHELL_FOUR")), "{ungated:?}");
    assert!(fn_body(&text, "a").unwrap().contains("let c"), "a brace in a string or a char does not end the body");
}

// ======================================================================= E1

// E1: "Opening a document window — from a card […] — always gives the
// editor, in debug and release builds alike, with no switch." (No
// `SHELL_EDITOR_V2` is set by these tests.)
#[test]
fn e1_a_card_opens_the_editor_with_no_switch() {
    let mut h = harness("e1-card");
    let id = h.add_doc("Card doc", "Body from the card");
    let wi = h.open(&id);
    assert!(h.has_editor(wi), "the window holds the editor");
    assert_eq!(h.editor_text(&id), "Body from the card");
}

// E1: "… from Alerts, an orphan resolution, … `open_doc_by_id`" — the
// Alerts window and the section "Documents whose thread was deleted" both
// open through `open_doc_by_id`.
#[test]
fn e1_alerts_and_orphans_open_the_editor() {
    let mut h = harness("e1-by-id");
    let id = h.add_doc("Alert doc", "Body from an alert");
    h.app.open_doc_by_id(&id);
    let wi = h.doc_window(&id).expect("a window opened");
    assert!(h.has_editor(wi), "the window holds the editor");
}

// E1: "… a new document …" — the new document's window is the editor,
// starting on its title (T3).
#[test]
fn e1_a_new_document_opens_the_editor_on_its_title() {
    let mut h = harness("e1-new");
    let before = h.app.cards.len();
    h.app.new_document(0);
    assert_eq!(h.app.cards.len(), before + 1, "the document was created");
    let wi = h.app.windows.len().checked_sub(1).expect("a window opened");
    assert!(h.has_editor(wi), "the new document's window holds the editor");
    let id = match &h.app.windows[wi].content {
        WindowContent::Doc(d) => d.doc_id.clone(),
        _ => panic!("the top window is not a document window"),
    };
    assert_eq!(h.pane(&id).editor.part(), EditorPart::Title, "it starts on its title");
}

// E1: "`SHELL_EDITOR_V2` is no longer read anywhere." No production file of
// the shell, in any subdirectory, holds the switch's name as a string.
#[test]
fn e1_shell_editor_v2_is_read_nowhere() {
    let hits: Vec<String> = production_files()
        .into_iter()
        .filter(|(_, text)| text.contains("\"SHELL_EDITOR_V2\""))
        .map(|(f, _)| f)
        .collect();
    assert!(hits.is_empty(), "still read in: {hits:?}");
}

// E1 / E8: the editor's module is compiled into every build — its
// declaration in `app.rs` carries no development-only `cfg`.
#[test]
fn e1_the_editor_module_is_compiled_in_every_build() {
    let src = std::fs::read_to_string(format!("{}/src/app.rs", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let code = blank(&src, true);
    let at = code.find("mod editor_v2;").expect("the editor keeps its module (`app/editor_v2.rs`)");
    // The attributes right above the declaration (doc comments are blank).
    let above: Vec<&str> = src[..at]
        .lines()
        .rev()
        .map(str::trim)
        .skip_while(|l| l.is_empty() || *l == "pub(crate)")
        .take_while(|l| l.starts_with("#[") || l.starts_with("//") || l.is_empty())
        .filter(|l| l.starts_with("#["))
        .collect();
    assert!(
        !above.iter().any(|a| dev_only_cfg(a)),
        "the editor's module is compiled only in development builds: {above:?}"
    );
}

// ======================================================================= E2

/// The plain path's names (contract E2), as they appear in code.
const PLAIN_NAMES: &[&str] = &[
    "edit_buf",
    "edit_origin",
    "text_edit",
    "TextEdit",
    "save_text",
    "begin_doc_edit",
    "save_doc",
    "save_active_doc",
    "request_leave_edit",
    "LeaveEdit",
    "top_doc_editing",
    "begin_edit_text",
    "begin_edit",
    "DocBtn::Edit",
    "DocBtn::Save",
    "DocBtn::Cancel",
];

/// Whether `name` occurs in `code` as a whole identifier (or path).
fn has_ident(code: &str, name: &str) -> Vec<usize> {
    let b = code.as_bytes();
    let is_id = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(at) = code[from..].find(name).map(|a| a + from) {
        let before = at.checked_sub(1).map(|i| b[i]);
        let after = b.get(at + name.len()).copied();
        if !before.is_some_and(is_id) && !after.is_some_and(is_id) {
            out.push(at);
        }
        from = at + name.len();
    }
    out
}

// E2: "No production code keeps the plain edit: […] `doc_writes::TextEdit`
// and `save_text`, `begin_doc_edit`, `save_doc`, `save_active_doc`,
// `request_leave_edit`, `AfterUnsaved::LeaveEdit`, the plain drawing,
// keyboard […]. A source scan fails on their reintroduction."
#[test]
fn e2_no_production_code_keeps_the_plain_edit() {
    let mut hits = Vec::new();
    for (file, text) in production_files() {
        let code = blank(&text, true);
        for name in PLAIN_NAMES {
            for at in has_ident(&code, name) {
                let line = text[..at].matches('\n').count() + 1;
                hits.push(format!("{file}:{line}: {name}"));
            }
        }
    }
    assert!(hits.is_empty(), "the plain edit is still there ({} uses):\n{}", hits.len(), hits.join("\n"));
}

// E2: "the `DocWindow` fields `editing`, `edit_buf`, `edit_origin`,
// `text_edit`, `dirty`, `needs_reshape`, the Edit / Save / Cancel buttons"
// — the window's struct, its methods and its buttons carry none of them.
#[test]
fn e2_the_document_window_has_no_plain_edit_state() {
    let text = production("panels.rs");
    let code = blank(&text, true);
    let at = code.find("struct DocWindow").expect("DocWindow");
    let open = code[at..].find('{').unwrap() + at;
    let fields = &code[at..matching(&code, open)];
    let mut found = Vec::new();
    for f in ["editing", "edit_buf", "edit_origin", "text_edit", "dirty", "needs_reshape", "lbl_edit", "lbl_save", "lbl_cancel"] {
        if !has_ident(fields, &format!("{f}:")).is_empty() || fields.contains(&format!(" {f}:")) {
            found.push(format!("field {f}"));
        }
    }
    // Its methods: the plain buffer's operations.
    let mut from = 0;
    while let Some(i) = code[from..].find("impl DocWindow").map(|a| a + from) {
        let open = code[i..].find('{').unwrap() + i;
        let end = matching(&code, open);
        let body = &code[i..end];
        for m in ["fn begin_edit", "fn cancel_edit", "fn insert_char", "fn backspace", "fn shown_text"] {
            if body.contains(m) {
                found.push(m.to_string());
            }
        }
        from = end;
    }
    // Its buttons: no Edit / Save / Cancel variant.
    let at = code.find("enum DocBtn").expect("DocBtn");
    let open = code[at..].find('{').unwrap() + at;
    let variants = &code[at..matching(&code, open)];
    for v in ["Edit", "Save", "Cancel"] {
        if !has_ident(variants, v).is_empty() {
            found.push(format!("DocBtn::{v}"));
        }
    }
    assert!(found.is_empty(), "the document window keeps the plain edit: {found:?}");
}

// E2: "The window's header keeps History (and the menu)." An open
// document's header shows History, and no Edit / Save / Cancel.
#[test]
fn e2_the_window_header_keeps_history_only() {
    let mut h = harness("e2-header");
    let id = h.add_doc("Header doc", "Body");
    let wi = h.open(&id);
    let bounds = h.app.windows[wi].bounds;
    let buttons: Vec<DocBtn> = match &h.app.windows[wi].content {
        WindowContent::Doc(d) => d.buttons(bounds).into_iter().map(|(_, b)| b).collect(),
        _ => unreachable!(),
    };
    assert!(buttons.contains(&DocBtn::History), "History stays");
    assert_eq!(buttons.len(), 1, "History is the header's only button (no Edit / Save / Cancel)");
}

// (E2, "the plain AccessKit body label": in the E8 accessibility test.)

// ======================================================================= E3

// E3: "When the editor cannot open a document, no window opens and a notice
// says why, in plain words (no database, …)".
#[test]
fn e3_no_database_opens_no_window_and_says_why() {
    let mut h = harness("e3-nodb");
    let id = h.add_doc("No db", "zebra-body-nodb");
    let ci = h.card(&id);
    h.app.db = None;
    h.app.docs = None;
    h.app.notice = None;
    h.app.open_doc_window(ci);
    assert!(h.doc_window(&id).is_none(), "no window opens");
    let said = notice_text(&h.app).expect("a notice says why");
    assert_plain_notice(&said, &["zebra-body-nodb"]);
    assert!(said.to_lowercase().contains("database"), "it says there is no database: {said}");
}

// E3: "… the document cannot be read, …"
#[test]
fn e3_an_unreadable_document_opens_no_window_and_says_why() {
    let mut h = harness("e3-unreadable");
    let id = h.add_doc("Unreadable", "zebra-body-unreadable");
    let ci = h.card(&id);
    let missing = "document:zebramissing".to_string();
    h.app.cards[ci].id = missing.clone();
    h.app.notice = None;
    h.app.open_doc_window(ci);
    assert!(h.doc_window(&missing).is_none(), "no window opens");
    let said = notice_text(&h.app).expect("a notice says why");
    assert_plain_notice(&said, &["zebra-body-unreadable"]);
    assert!(said.to_lowercase().contains("read"), "it says the document cannot be read: {said}");
}

// E3: "… it is encrypted and the session is not unlocked, … never stored
// JSON or ciphertext." The development no-auth mode: a raw store, no key.
#[test]
fn e3_an_encrypted_document_without_a_session_opens_no_window() {
    ensure_test_data_dir();
    editor_v2::use_test_system_clipboard(Box::new(MemClipboard::default()));
    let mut app = App::new();
    // The raw store the no-auth mode opens (an in-memory SurrealDB here).
    let db = app.rt.block_on(async {
        use sovereign_db::surreal::{StorageMode, SurrealGraphDB};
        let db = SurrealGraphDB::new(StorageMode::Memory).await.unwrap();
        db.connect().await.unwrap();
        db.init_schema().await.unwrap();
        Arc::new(db)
    });
    let tid = app.rt.block_on(db.create_thread(Thread::new("Lane".into(), String::new()))).unwrap().id_string().unwrap();
    let mut d = Document::new("Sealed".into(), tid, true);
    d.content = "zebra-ciphertext-QUJDRA==".into();
    d.encryption_nonce = Some("bm9uY2U=".into());
    app.rt.block_on(db.create_document(d)).unwrap();
    app.db = Some(db as Arc<dyn GraphDB>);
    app.docs = None;
    app.locked = false;
    app.load_workspace_now();
    assert_eq!(app.cards.len(), 1, "the sealed document's card");
    let ci = 0;
    app.notice = None;
    app.open_doc_window(ci);
    assert!(
        !app.windows.iter().any(|w| matches!(&w.content, WindowContent::Doc(_))),
        "no window opens on ciphertext"
    );
    let said = notice_text(&app).expect("a notice says why");
    assert_plain_notice(&said, &["zebra-ciphertext", "QUJDRA"]);
    assert!(said.to_lowercase().contains("encrypt"), "it says the document is encrypted: {said}");
}

// E3: "… or the editor's own refusal), … never stored JSON". A stored form
// the editor refuses to read.
#[test]
fn e3_the_editors_refusal_opens_no_window_and_never_quotes_the_json() {
    let mut h = harness("e3-refusal");
    let id = h.add_raw("Refused", r#"{"format":"sovereign.doc","v":2,"blocks":"zebra-json-marker"}"#);
    h.app.notice = None;
    h.open_no_window_expected(&id);
    let said = notice_text(&h.app).expect("a notice says why");
    assert_plain_notice(&said, &["zebra-json-marker", "sovereign.doc"]);
}

impl H {
    fn open_no_window_expected(&mut self, id: &str) {
        let ci = self.card(id);
        self.app.open_doc_window(ci);
        assert!(self.doc_window(id).is_none(), "no window opens");
    }
}

// E3: "The cases the editor already reports itself (newer format, too large,
// editing elsewhere) keep its own status line." A newer format opens, read
// only, saying so.
#[test]
fn e3_a_newer_format_opens_read_only_with_the_editors_status_line() {
    let mut h = harness("e3-newer");
    let id = h.add_raw("Newer", r#"{"format":"sovereign.doc","v":99,"blocks":[]}"#);
    let wi = h.open(&id);
    assert!(h.has_editor(wi), "the editor shows it");
    let p = h.pane(&id);
    assert!(p.editor.is_read_only());
    assert!(p.status_text().contains("newer version"), "its own status line: {}", p.status_text());
}

// ======================================================================= E4

// E4: "Every path that discards a document window […] — the × button […] —
// asks first when `has_unsaved_edits` is true." Read as: nothing unsaved is
// lost — the window stays (asking, or saving), or closes only once its text
// is stored.
#[test]
fn e4_the_close_button_never_loses_unsaved_edits() {
    let mut h = harness("e4-close");
    let id = h.add_doc("Close", "Start");
    let wi = h.open(&id);
    h.type_in(&id, " zebra-typed");
    assert!(matches!(&h.app.windows[wi].content, WindowContent::Doc(d) if d.has_unsaved_edits()));
    h.app.request_close_window(wi);
    h.frames(5);
    match h.doc_window(&id) {
        None => assert!(h.stored_text(&id).contains("zebra-typed"), "closed only once saved"),
        Some(_) => assert!(
            h.app.unsaved_prompt.is_some() || h.pane(&id).has_unsaved(),
            "still open, holding the edits"
        ),
    }
}

// E4: the × button and "the unsaved prompt": with a save that fails, closing
// asks; Save in the prompt fails again and keeps the window and the text;
// Discard closes it.
#[test]
fn e4_a_failing_save_makes_the_close_button_ask_and_the_prompt_keep_the_text() {
    let mut h = harness("e4-prompt");
    let id = h.add_doc("Prompt", "Start");
    let wi = h.open(&id);
    let docs = h.app.docs.take(); // saves now fail (no document service)
    h.type_in(&id, " zebra-typed");
    h.app.request_close_window(wi);
    h.frames(5);
    let wi = h.doc_window(&id).expect("the window stays");
    assert!(h.app.unsaved_prompt.is_some(), "it asks");
    h.app.resolve_unsaved(UnsavedChoice::Save);
    h.frames(5);
    assert!(h.doc_window(&id).is_some(), "a failed Save keeps the window");
    assert!(h.editor_text(&id).contains("zebra-typed"), "and the text");
    if h.app.unsaved_prompt.is_none() {
        h.app.request_close_window(wi);
        h.frames(5);
    }
    assert!(h.app.unsaved_prompt.is_some(), "the prompt comes back after a failed save");
    h.app.resolve_unsaved(UnsavedChoice::Discard);
    h.frames(1);
    assert!(h.doc_window(&id).is_none(), "Discard closes it");
    h.app.docs = docs;
    assert_eq!(h.stored_text(&id), "Start", "nothing was written");
}

// E4: "In particular Esc on a document window whose editor is not focused no
// longer closes it without asking." A key press needs a winit event loop, so
// this reads `window_event`: the Esc that closes the top window (the one that
// also collapses a fanned deck) goes through the unsaved check when it closes
// a document window.
#[test]
fn e4_escape_never_closes_a_document_window_without_the_unsaved_check() {
    let text = production("app.rs");
    let body = fn_body(&text, "window_event").expect("fn window_event");
    let code = blank(&body, true);
    let mut generic = Vec::new();
    let mut from = 0;
    while let Some(at) = code[from..].find("Key::Named(NamedKey::Escape)").map(|a| a + from) {
        from = at + 1;
        let after = &code[at..];
        let Some(arrow) = after.find("=>") else { continue };
        let start = at + arrow + 2;
        let rest = code[start..].trim_start();
        let s = start + (code[start..].len() - rest.len());
        let end = if rest.starts_with('{') {
            matching(&code, s)
        } else {
            s + rest.find(['\n', ',']).unwrap_or(rest.len())
        };
        let arm = &body[s..end];
        if arm.contains("expanded_deck") {
            generic.push(arm.to_string());
        }
    }
    assert!(
        !generic.is_empty(),
        "the Esc that closes the top window (and collapses a fanned deck) was not found in window_event"
    );
    const GUARDS: &[&str] =
        &["request_close_window", "v2_request_close", "has_unsaved_edits", "WindowContent::Doc", "WindowKind::Doc"];
    for arm in generic {
        let closes = arm.contains("windows.pop()") || arm.contains("windows.remove(") || arm.contains("windows.truncate(");
        if closes {
            assert!(
                GUARDS.iter().any(|g| arm.contains(g)),
                "Esc closes the top window without the unsaved check:\n{arm}"
            );
        }
    }
}

// E4: "quitting […] asks first when `has_unsaved_edits` is true" — read as:
// quitting with unsaved edits never loses them (it waits for their save,
// or asks); the app quits once they are stored.
#[test]
fn e4_quitting_never_loses_unsaved_edits() {
    let mut h = harness("e4-quit");
    let id = h.add_doc("Quit", "Start");
    h.open(&id);
    h.type_in(&id, " zebra-typed");
    let mut quit = h.app.request_quit();
    assert!(!quit, "the app does not quit over unsaved edits at once");
    // What `about_to_wait` does, frame after frame.
    for _ in 0..10 {
        h.frames(1);
        if h.app.quit_requested && h.app.unsaved_prompt.is_none() {
            h.app.quit_requested = false;
            quit = h.app.request_quit();
            if quit {
                break;
            }
        }
    }
    assert!(quit || h.app.unsaved_prompt.is_some(), "it quits once saved, or asks");
    if quit {
        assert!(h.stored_text(&id).contains("zebra-typed"), "it quit only once the edits were stored");
    }
}

// E4: "quitting still ends every editor session (`v2_end_sessions`) whether
// or not anything was unsaved" — nothing unsaved: the session's last save
// becomes a version at quit.
#[test]
fn e4_quitting_ends_every_editor_session() {
    let mut h = harness("e4-quit-sessions");
    let id = h.add_doc("Sessions", "Start");
    h.open(&id);
    h.type_in(&id, " more");
    h.save(&id);
    assert!(!h.pane(&id).has_unsaved(), "saved");
    let before = h.versions(&id);
    assert!(h.app.request_quit(), "nothing unsaved: the app may quit");
    h.frames(1);
    assert!(h.versions(&id) > before, "the session ended: its last save is a version");
}

// E4: "… whether or not anything was unsaved" — unsaved edits discarded in
// the prompt: the session still ends at quit.
#[test]
fn e4_quitting_after_discarding_still_ends_the_session() {
    let mut h = harness("e4-quit-discard");
    let id = h.add_doc("Discarded", "Start");
    h.open(&id);
    h.type_in(&id, " saved part");
    h.save(&id);
    let before = h.versions(&id);
    let docs = h.app.docs.take(); // the next save fails
    h.type_in(&id, " zebra-unsaved");
    h.save(&id);
    assert!(!h.app.request_quit(), "unsaved edits: it asks");
    assert!(h.app.unsaved_prompt.is_some(), "the prompt");
    h.app.resolve_unsaved(UnsavedChoice::Discard);
    h.app.docs = docs;
    h.app.quit_requested = false;
    assert!(h.app.request_quit(), "discarded: the app may quit");
    h.frames(1);
    assert!(h.versions(&id) > before, "the session ended though edits were unsaved");
    assert!(!h.stored_text(&id).contains("zebra-unsaved"), "the discarded text was not written");
}

// E4: "… run a skill …" — a skill does not rewrite a document over its
// window's unsaved edits.
#[test]
fn e4_a_skill_never_runs_over_unsaved_edits() {
    let mut h = harness("e4-skill");
    let id = h.add_doc("Skill", "# Heading\n\ntext");
    h.open(&id);
    h.type_in(&id, " zebra-typed");
    let si = h.app.skills.all_skills().iter().position(|s| s.name() == "table-of-contents").expect("the TOC skill");
    let stored = h.stored_text(&id);
    h.app.run_skill(&id, si);
    assert_eq!(h.stored_text(&id), stored, "nothing written");
    assert!(h.doc_window(&id).is_some(), "the window stays");
    assert!(h.editor_text(&id).contains("zebra-typed"), "the edits stay");
}

// E4: "… restore a version …"
#[test]
fn e4_a_restore_never_runs_over_unsaved_edits() {
    let mut h = harness("e4-restore");
    let (id, created) = h.add_doc_v("Restore", "First");
    h.open(&id);
    h.type_in(&id, " second");
    h.save(&id);
    h.type_in(&id, " zebra-typed");
    let stored = h.stored_text(&id);
    h.app.restore_commit(&id, &created);
    assert_eq!(h.stored_text(&id), stored, "nothing restored");
    assert!(h.editor_text(&id).contains("zebra-typed"), "the edits stay");
}

// E4: "… move to trash …"
#[test]
fn e4_moving_to_the_trash_never_runs_over_unsaved_edits() {
    let mut h = harness("e4-trash");
    let id = h.add_doc("Trash", "Start");
    h.open(&id);
    h.type_in(&id, " zebra-typed");
    h.app.trash_document(&id);
    assert!(h.doc_window(&id).is_some(), "the window stays");
    assert!(h.editor_text(&id).contains("zebra-typed"), "the edits stay");
    let docs = h.docs();
    assert!(h.app.rt.block_on(docs.read(&id)).is_ok(), "the document is not in the trash");
}

// ======================================================================= E5

// E5: "A change to an open document, whatever its source — […] the
// assistant […] — updates the editor the way a sync does today: reloaded if
// it holds nothing unsaved". The assistant's write tool, then its event.
#[test]
fn e5_an_assistant_write_reaches_an_open_editor() {
    let mut h = harness("e5-assistant");
    let id = h.add_doc("Assistant", "Before");
    h.open(&id);
    h.assistant_writes(&id, "zebra-from-the-assistant");
    h.app.orch_tx.send(OrchestratorEvent::ActionExecuted { action: "edit_document".into(), success: true }).unwrap();
    h.scene();
    assert_eq!(h.editor_text(&id), "zebra-from-the-assistant", "the editor reloaded");
}

// E5: the same through a chat turn's outcome (spec ch. 08: the turn's
// writes), the event that replaced `ActionExecuted` for chat.
#[test]
fn e5_a_chat_turns_write_reaches_an_open_editor() {
    let mut h = harness("e5-chat");
    let id = h.add_doc("Chat", "Before");
    h.open(&id);
    h.assistant_writes(&id, "zebra-from-a-chat-turn");
    let outcome = ChatOutcome {
        writes: vec![WriteResult { tool: "edit_document".into(), outcome: WriteOutcome::Succeeded }],
        end: ChatEnd::Completed,
    };
    h.app.orch_tx.send(OrchestratorEvent::ChatTurnEnded { outcome }).unwrap();
    h.scene();
    assert_eq!(h.editor_text(&id), "zebra-from-a-chat-turn", "the editor reloaded");
}

// E5: "… otherwise kept, with the next save resolving against the newer
// revision (C-DOC: nothing overwritten)."
#[test]
fn e5_with_unsaved_edits_the_editor_keeps_them_and_overwrites_nothing() {
    let mut h = harness("e5-unsaved");
    let id = h.add_doc("Kept", "Before");
    h.open(&id);
    h.type_in(&id, " zebra-mine");
    h.assistant_writes(&id, "zebra-assistant");
    h.app.orch_tx.send(OrchestratorEvent::ActionExecuted { action: "edit_document".into(), success: true }).unwrap();
    h.scene();
    assert!(h.editor_text(&id).contains("zebra-mine"), "the editor keeps its text");
    h.save(&id);
    assert!(h.stored_text(&id).contains("zebra-mine"), "the save lands");
    assert!(h.history_holds(&id, "zebra-assistant"), "the assistant's state is in History");
}

// E5: "… a restore" — nothing unsaved: the editor shows the restored text.
#[test]
fn e5_a_restore_reaches_an_open_editor() {
    let mut h = harness("e5-restore");
    let (id, created) = h.add_doc_v("Restore", "zebra-first");
    h.open(&id);
    h.type_in(&id, " second");
    h.save(&id);
    h.app.restore_commit(&id, &created);
    h.frames(1);
    assert_eq!(h.editor_text(&id), "zebra-first", "the editor shows the restored version");
}

// E5: "… a skill" — nothing unsaved: the editor shows what the skill wrote.
#[test]
fn e5_a_skill_reaches_an_open_editor() {
    let mut h = harness("e5-skill");
    let id = h.add_doc("Skill", "# Heading\n\ntext");
    h.open(&id);
    let si = h.app.skills.all_skills().iter().position(|s| s.name() == "table-of-contents").expect("the TOC skill");
    h.app.run_skill(&id, si);
    h.frames(1);
    let stored = h.stored_text(&id);
    assert_ne!(stored, "# Heading\n\ntext", "the skill wrote");
    assert_eq!(h.editor_text(&id), stored, "the editor shows it");
}

// E5: "… a peer sync" (today's path, kept).
#[test]
fn e5_a_peer_sync_reaches_an_open_editor() {
    let mut h = harness("e5-sync");
    let id = h.add_doc("Synced", "Before");
    h.open(&id);
    let raw = h.app.rt.block_on(h.store.get_document(&id)).unwrap().content;
    let theirs = raw.replace("Before", "zebra-from-a-peer");
    h.app.rt.block_on(h.store.update_document(&id, None, Some(&theirs))).unwrap();
    h.app.v2_after_sync("a paired device");
    assert_eq!(h.editor_text(&id), "zebra-from-a-peer");
}

// ======================================================================= E6

// E6: "The banner 'Editor v2 (development, unfinished)…' is removed" and
// "user-visible wording that said 'Editor v2' (notice titles) becomes
// 'Document'".
#[test]
fn e6_the_development_banner_and_name_are_gone() {
    let mut hits = Vec::new();
    for (file, text) in production_files() {
        for pat in ["Editor v2 (development", "\"Editor v2\""] {
            if text.contains(pat) {
                hits.push(format!("{file}: {pat}"));
            }
        }
        if !has_ident(&blank(&text, true), "BANNER").is_empty() && file.contains("editor_v2") {
            hits.push(format!("{file}: BANNER"));
        }
    }
    assert!(hits.is_empty(), "{hits:?}");
}

// E6: "the status line (Saved / Saving / Save failed / read-only / notes)
// stays in every build." (Run under `cargo test --release` for the release
// half.)
#[test]
fn e6_the_status_line_stays() {
    let mut h = harness("e6-status");
    let id = h.add_doc("Status", "Start");
    h.open(&id);
    h.type_in(&id, " more");
    h.save(&id);
    let s = h.pane(&id).status_text();
    assert!(s.starts_with("Saved"), "{s}");
    let docs = h.app.docs.take();
    h.type_in(&id, " again");
    h.save(&id);
    let s = h.pane(&id).status_text();
    assert!(s.contains("Save failed"), "{s}");
    h.app.docs = docs;
}

// E6: "`SHELL_FAIL_SAVES_WHILE`, `SHELL_SEED_PII_TEST` and every other
// `SHELL_*` code inside the editor are not compiled into a release build."
// Every read of a development switch in the editor's source sits under a
// development-only `cfg` (the module itself is compiled in every build, E1).
#[test]
fn e6_the_editors_development_switches_are_not_compiled_into_release() {
    let mut files = vec!["app/editor_v2.rs".to_string()];
    let sub = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/app/editor_v2");
    if sub.is_dir() {
        for (f, _) in production_files() {
            if f.starts_with("app/editor_v2/") {
                files.push(f);
            }
        }
    }
    let mut ungated = Vec::new();
    for f in files {
        let text = production(&f);
        let gated = dev_gated_ranges(&text);
        for (at, line) in dev_switch_reads(&text) {
            if !gated.iter().any(|(s, e)| *s <= at && at < *e) {
                ungated.push(format!("{f}:{line}"));
            }
        }
    }
    assert!(ungated.is_empty(), "compiled into a release build:\n{}", ungated.join("\n"));
}

// E6: "The development paths that opened a plain window (`SHELL_OPEN_DOC`,
// `SHELL_DOC_EDIT`) open the editor or are removed." (`SHELL_DOC_EDIT`'s
// plain fields are in E2's scan.)
#[test]
fn e6_shell_open_doc_opens_the_editor_or_is_gone() {
    let text = production("app.rs");
    if !text.contains("\"SHELL_OPEN_DOC\"") {
        return; // removed
    }
    let code = blank(&text, true);
    let at = code.find("if self.debug_open").expect("SHELL_OPEN_DOC is read: its opening code (`if self.debug_open`)");
    let open = code[at..].find('{').unwrap() + at;
    let block = &text[at..matching(&code, open)];
    assert!(
        block.contains("open_doc_window(") || block.contains("open_doc_by_id("),
        "SHELL_OPEN_DOC opens the window the way a click does (the editor):\n{block}"
    );
    assert!(!block.contains("DocWindow::new("), "not a window built by hand:\n{block}");
}

// ======================================================================= E7

// E7: "a document never shows stored JSON (v1 envelope, raw, v2)". The
// editor's text and its accessible values, for each stored form.
#[test]
fn e7_the_editor_never_shows_stored_json() {
    let v2 = sovereign_editor::doc::DocumentV2::from_text("In version two", "seed").to_json_string();
    for (stored, shown) in [
        (r#"{"body":"Enveloped","images":[],"videos":[]}"#.to_string(), "Enveloped"),
        ("Raw text".to_string(), "Raw text"),
        (v2, "In version two"),
    ] {
        let cs = ClipState::new(Box::new(MemClipboard::default()));
        let mut p = EditorPane::open(
            "T",
            stored.clone(),
            Trust::Owned,
            Fonts::Only(vec![TEST_FONT.to_vec()]),
            Box::new(NoPii),
            Default::default(),
            std::rc::Rc::new(std::cell::RefCell::new(cs)),
            Default::default(),
            vec!["document:t".into()],
        )
        .expect("the editor opens");
        assert_eq!(p.editor.document().plain_text(), shown, "{stored}");
        for (_, n) in p.editor.accessibility_tree().nodes {
            for s in [n.label(), n.value()].into_iter().flatten() {
                assert!(!s.contains("{\"") && !s.contains("sovereign.doc"), "stored JSON shown: {s}");
            }
        }
    }
}

// E7: "a write error is never reported as success (also at the service
// level for a missing document)". (Covered for a present document by
// `editor_v2::tests::a_failed_save_keeps_the_text_and_is_announced_once`.)
#[test]
fn e7_a_save_on_a_missing_document_is_an_error() {
    ensure_test_data_dir();
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(async {
        let store = testing::encrypted_memory_store("e7-missing").await;
        let docs = sovereign_ai::docs_host::build_document_service(store, Some(testing::account_key()), "test".into());
        let cs = ClipState::new(Box::new(MemClipboard::default()));
        let mut p = EditorPane::open(
            "T",
            "text".into(),
            Trust::Owned,
            Fonts::Only(vec![TEST_FONT.to_vec()]),
            Box::new(NoPii),
            Default::default(),
            std::rc::Rc::new(std::cell::RefCell::new(cs)),
            Default::default(),
            vec!["document:gone".into()],
        )
        .unwrap();
        p.editor.command(Command::Save);
        let reqs = p.take_saves();
        assert_eq!(reqs.len(), 1);
        let mut st = SaveState::new(Some(Revision("0-0".into())));
        assert!(save_request(&docs, "document:gone", &mut st, &reqs[0], true, vec![]).await.is_err());
        assert!(st.last_saved.is_none(), "no revision recorded for a failed save");
    });
}

// E7: the same through the window: a save the store refuses (the document
// went to the trash meanwhile) is shown as failed, never as saved, and the
// text stays unsaved.
#[test]
fn e7_a_refused_save_is_never_shown_as_saved_in_the_window() {
    let mut h = harness("e7-window");
    let id = h.add_doc("Gone", "Start");
    h.open(&id);
    let docs = h.docs();
    h.app.rt.block_on(docs.trash(&id)).unwrap();
    h.type_in(&id, " zebra-typed");
    h.save(&id);
    let p = h.pane(&id);
    assert!(p.has_unsaved(), "still unsaved");
    assert!(p.status_text().contains("Save failed"), "{}", p.status_text());
}

// ======================================================================= E8

/// The accessibility nodes reachable from the root after applying
/// `updates` in order.
fn reachable(updates: Vec<accesskit::TreeUpdate>) -> Vec<accesskit::Node> {
    let mut root = None;
    let mut all: HashMap<accesskit::NodeId, accesskit::Node> = HashMap::new();
    for u in updates {
        if let Some(t) = &u.tree {
            root = Some(t.root);
        }
        for (id, n) in u.nodes {
            all.insert(id, n);
        }
    }
    let mut out = Vec::new();
    let mut stack = vec![root.expect("a tree root")];
    let mut seen = std::collections::HashSet::new();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        if let Some(n) = all.get(&id) {
            stack.extend(n.children().iter().copied());
            out.push(n.clone());
        }
    }
    out
}

// E8 (precision): "In a release-cfg build, `build_a11y_tree` exposes the open
// document's text and title through the editor's own nodes." Read as: what
// the shell sends a screen reader in a frame — `build_a11y_tree`, then
// `App::a11y_frame_updates` — reaches the editor's title field and its text
// runs from the root, and no plain label holds the body (E2). The release
// cfg: this test run under `cargo test --release` (see the index).
#[test]
fn e8_the_accessibility_tree_exposes_the_document_through_the_editor() {
    let mut h = harness("e8-a11y");
    let id = h.add_doc("Zebra title", "Ringing zebra body");
    h.open(&id);
    h.frames(1);
    h.scene();
    let mut updates = vec![h.app.build_a11y_tree()];
    updates.extend(h.app.a11y_frame_updates());
    let nodes = reachable(updates);
    assert!(
        nodes.iter().any(|n| n.role() == accesskit::Role::TextInput
            && n.label() == Some("Title")
            && n.value() == Some("Zebra title")),
        "the editor's title field is reachable"
    );
    assert!(
        nodes.iter().any(|n| n.role() == accesskit::Role::TextRun && n.value().is_some_and(|v| v.contains("Ringing zebra body"))),
        "the editor's text is reachable"
    );
    assert!(
        !nodes.iter().any(|n| n.role() == accesskit::Role::Label && n.label().is_some_and(|l| l.contains("Ringing zebra body"))),
        "no plain label holds the body (E2)"
    );
}

// E8: in a release-cfg build the accessibility path has no development-only
// code: `build_a11y_tree` and `render` carry no development-only `cfg`.
#[test]
fn e8_the_accessibility_path_is_the_same_in_every_build() {
    let text = production("app.rs");
    for f in ["build_a11y_tree", "render"] {
        let body = fn_body(&text, f).unwrap_or_else(|| panic!("fn {f}"));
        let code = blank(&body, true);
        let mut from = 0;
        while let Some(at) = code[from..].find("#[cfg(").map(|a| a + from) {
            let end = matching(&code, at + 1);
            assert!(!dev_only_cfg(&body[at..end]), "fn {f} has development-only code: {}", &body[at..end]);
            from = end;
        }
    }
}

// ======================================================================= E9

// E9 (revised, "Settled after the test author's report"): the shell has no
// lock, logout or persona switch; a store is replaced by
// `install_document_service`, called by every unlock (`try_login`,
// `try_onboard`, `finish_onboarding`, `recovery_finalize`). `unlock_into`
// drives it as they do: the incoming store's `db` and key are set first, then
// `install_document_service`. "When the shell gets a lock, E9 applies to it
// unchanged."

const ALL_PRIVATE: ClipboardPrivacy =
    ClipboardPrivacy { exclude_from_history: true, exclude_from_cloud_sync: true, exclude_from_monitoring: true };

/// What a replacement leaves on screen, for comparing two of them.
fn after_replacement(app: &App) -> (bool, Option<String>, bool, usize, bool, bool) {
    (
        app.locked,
        notice_text(app),
        app.unsaved_prompt.is_some(),
        app.windows.iter().filter(|w| matches!(&w.content, WindowContent::Doc(_))).count(),
        app.pending_action.is_some(),
        app.quit_requested,
    )
}

/// The other store, unlocked into `h.app` while its windows are open.
fn replace_store(h: &mut H, tag: &str) {
    let other = h.app.rt.block_on(testing::encrypted_memory_store(tag));
    unlock_into(&mut h.app, other);
}

// E9: "whenever `install_document_service` replaces a store while document
// windows are open, it first closes them, ends their editor sessions on the
// store they belong to (the session queue is drained against the outgoing
// service, never the incoming one)".
//
// Proof of the store: the session's last save becomes a version **in the
// outgoing store**, and the queue is empty as soon as the replacement
// returns — before any frame, so nothing is left for the next frame to end
// on the incoming store.
#[test]
fn e9_replacing_the_store_closes_the_editors_and_ends_their_sessions_on_the_outgoing_store() {
    let mut h = harness("e9-replace-a");
    let a = h.add_doc("A", "Start A");
    let b = h.add_doc("B", "Start B");
    h.open(&a);
    h.open(&b);
    h.type_in(&a, " saved");
    h.save(&a);
    let outgoing = h.docs(); // the outgoing store's service, kept by the test
    let before = h.app.rt.block_on(outgoing.versions(&a)).unwrap().len();
    replace_store(&mut h, "e9-replace-b");
    assert_eq!(h.doc_windows(), 0, "every editor window closed");
    assert_eq!(editor_v2::pending_session_ends(), 0, "the queue was drained at the replacement, not left to a frame");
    let after = h.app.rt.block_on(outgoing.versions(&a)).unwrap().len();
    assert!(after > before, "the session ended on the outgoing store: its last save is a version there");
    h.frames(1);
    assert_eq!(
        h.app.rt.block_on(outgoing.versions(&a)).unwrap().len(),
        after,
        "nothing more is ended after the replacement"
    );
}

// E9: "no prompt is shown (the same in every persona)" — a replacement with
// unsaved edits leaves the screen as a replacement with no document open.
#[test]
fn e9_a_replacement_looks_the_same_with_or_without_unsaved_edits() {
    let mut with = harness("e9-same-a");
    let id = with.add_doc("Unsaved", "Start");
    with.open(&id);
    with.type_in(&id, " zebra-unsaved");
    with.app.notice = None;
    replace_store(&mut with, "e9-same-a2");
    with.frames(1);

    let mut without = harness("e9-same-b");
    without.app.notice = None;
    replace_store(&mut without, "e9-same-b2");
    without.frames(1);

    assert!(with.app.unsaved_prompt.is_none(), "no unsaved prompt at a replacement");
    assert_eq!(after_replacement(&with.app), after_replacement(&without.app), "the two look the same");
}

// E9: "clears the editor's clipboard state (`ClipState`); nothing of them is
// reachable afterwards" — no window, no queued session, no private payload
// or source document on the clipboard, nothing of the outgoing store on the
// canvas.
#[test]
fn e9_nothing_of_the_editor_is_reachable_after_the_store_is_replaced() {
    let mut h = harness("e9-next-a");
    let a = h.add_doc("zebra-title-a", "zebra-body-a");
    h.open(&a);
    h.type_in(&a, " zebra-typed-a");
    h.save(&a);
    // A private copy from document A (a token's payload, its source).
    editor_v2::clipboard_for_tests().borrow_mut().write_from(
        ClipboardWrite {
            text: "zebra [Email]".into(),
            sovereign: Some("zebra-payload".into()),
            privacy: ALL_PRIVATE,
            clear_after: None,
        },
        Duration::ZERO,
        Some(a.clone()),
    );
    replace_store(&mut h, "e9-next-b");
    h.frames(1);
    h.scene();

    assert_eq!(h.doc_windows(), 0, "no editor window carried over");
    assert!(h.app.unsaved_prompt.is_none(), "no prompt");
    assert!(h.app.notice.as_ref().map_or(true, |n| !n.said.1.contains("zebra")), "no notice about A's text");
    assert_eq!(editor_v2::pending_session_ends(), 0, "no session of A left to end");
    let clip = editor_v2::clipboard_for_tests();
    let mut clip = clip.borrow_mut();
    assert_eq!(clip.read().and_then(|c| c.sovereign), None, "the copy's private payload is gone");
    assert_eq!(clip.source_document(), None, "and its source document");
    assert!(
        !h.app.cards.iter().any(|c| c.title.contains("zebra") || c.body.contains("zebra")),
        "nothing of A on the canvas"
    );
}

// E9 (extended, "Settled after the implementer's report"): "A store
// replacement also closes every window that shows a document of the outgoing
// store — History windows (their version lists and previews) as well as
// document windows. Nothing of the outgoing store stays on screen." A
// document and its History (two versions) are open, then the store is
// replaced: no History window remains, no window names the outgoing
// document, and the screen reader's tree says nothing of it.
#[test]
fn e9_a_replacement_closes_the_outgoing_stores_history_windows() {
    let mut h = harness("e9-history-a");
    let a = h.add_doc("zebra-history-title", "zebra-version-one");
    h.open(&a);
    h.type_in(&a, " zebra-version-two");
    h.save(&a);
    h.app.open_history(&a, "zebra-history-title");
    assert!(
        h.app.windows.iter().any(|w| matches!(&w.content, WindowContent::History(hp) if hp.doc_id == a)),
        "the History window is open"
    );
    replace_store(&mut h, "e9-history-b");
    h.frames(1);
    h.scene();
    assert!(
        !h.app.windows.iter().any(|w| matches!(&w.content, WindowContent::History(_))),
        "no History window of the outgoing store remains"
    );
    assert!(
        !h.app.windows.iter().any(|w| match &w.content {
            WindowContent::History(hp) => hp.doc_id == a,
            WindowContent::Doc(d) => d.doc_id == a,
            _ => false,
        }),
        "no window shows the outgoing document"
    );
    let tree = h.app.build_a11y_tree();
    for (_, n) in tree.nodes {
        for s in [n.label(), n.value()].into_iter().flatten() {
            assert!(!s.contains("zebra"), "the screen still shows the outgoing store: {s}");
        }
    }
}

// E9, the clipboard state: a revealed value copied from an editor waits on
// the clipboard for its clear timer (ch. 12, *Clipboard*); a replacement does
// not leave it there for the next session — nor cancel its clear.
#[test]
fn e9_a_revealed_copy_does_not_outlive_the_replacement() {
    let mut h = harness("e9-revealed");
    let a = h.add_doc("A", "Start");
    h.open(&a);
    editor_v2::clipboard_for_tests().borrow_mut().write_from(
        ClipboardWrite {
            text: "zebra-revealed-4111".into(),
            sovereign: None,
            privacy: ALL_PRIVATE,
            clear_after: Some(Duration::from_secs(30)),
        },
        Duration::ZERO,
        Some(a.clone()),
    );
    replace_store(&mut h, "e9-revealed-b");
    h.frames(1);
    let clip = editor_v2::clipboard_for_tests();
    let text = clip.borrow_mut().read().map(|c| c.text);
    assert_ne!(text.as_deref(), Some("zebra-revealed-4111"), "the revealed value is cleared at the replacement");
}
