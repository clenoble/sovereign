//! v0.0.12 lot 1 in the native shell — History restore (1a: H1, H3, H4, H5)
//! and Discard after a cancelled quit (5: D1–D3).
//!
//! Contract: `doc/plans/V0012-lot1.md`. Written before the code, by another
//! author: whoever implements lot 1 makes these pass **without editing this
//! file**; a test that looks wrong is reported, not worked around. H2 (the
//! service's reasons) is in `sovereign-docs/tests/lot1_h2_version_ids.rs`.
//!
//! A child module of `app` (declared in `app.rs`, like
//! `editor_default_tests`), so it drives the window manager's own paths on a
//! real `App`: open a document from its card, open its History as the
//! window's History button does (`open_history` with the window's id and the
//! card's title), choose a row as the keyboard and the click do, and "Use
//! this version" as Enter and the button do (`restore_commit` with the row's
//! own id). A click itself needs a winit window (`handle_click` reads the
//! window size), so the click arms are reproduced: the row index the click
//! sets, then `restore_commit(hp.doc_id, hp.commits[sel].commit_id)`.
//!
//! **No id is built by the test.** H1's bug was an id printed with `Thing`'s
//! `Display`; a test that builds ids that way agrees with it. Every id used
//! against the History comes from what the app itself shows (its cards, its
//! History rows); the service's own list (`DocumentService::versions`) is
//! read only to know what each row should be.
//!
//! Store: a real `EncryptedGraphDB` over the generated `FaultyDb` of the
//! document service's suite (`sovereign-docs/tests/common/faulty.rs`, reused
//! as is) over an in-memory SurrealDB — the fault switch is H3's failing
//! store, the raw store is what H4 tampers. Data directory: a fresh temporary
//! one unless `SOVEREIGN_DATA_DIR` is set; an in-memory clipboard.

use std::sync::{Arc, OnceLock};

use sovereign_ai::docs_host::{new_user_document, testing};
use sovereign_crypto::device_key::DeviceKey;
use sovereign_crypto::index_key::IndexKey;
use sovereign_crypto::kek::Kek;
use sovereign_crypto::key_db::KeyDatabase;
use sovereign_crypto::master_key::MasterKey;
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::schema::{Document, Thread};
use sovereign_db::surreal::{StorageMode, SurrealGraphDB};
use sovereign_db::traits::GraphDB;
use sovereign_docs::{DocumentService, Edit, EditContent, Origin};
use sovereign_editor::host::ClipboardPrivacy;
use sovereign_editor::input::{Command, Modifiers};
use sovereign_editor::{Position, Selection};
use winit::keyboard::{Key, NamedKey};

use super::editor_v2::{self, route_key, EditorPane, SystemClipboard};
use super::{App, UnsavedChoice, WindowContent};

#[path = "../../sovereign-docs/tests/common/faulty.rs"]
#[allow(dead_code, clippy::all)]
mod faulty;
use faulty::FaultyDb;

// ======================================================================= fixtures

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
        let d = std::env::temp_dir().join(format!("sov-lot1-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::env::set_var("SOVEREIGN_DATA_DIR", &d);
        d
    });
}

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

/// The encrypted store over `FaultyDb` over in-memory SurrealDB.
async fn faulty_store(tag: &str) -> (Arc<SurrealGraphDB>, Arc<FaultyDb>, Arc<EncryptedGraphDB>) {
    let raw = Arc::new(SurrealGraphDB::new(StorageMode::Memory).await.expect("memory store"));
    raw.connect().await.expect("connect");
    raw.init_schema().await.expect("schema");
    let faulty = Arc::new(FaultyDb::new(raw.clone() as Arc<dyn GraphDB>));
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir();
    let kdb = |s: &str| {
        Arc::new(tokio::sync::RwLock::new(KeyDatabase::new(
            dir.join(format!("lot1-shell-{}-{nanos}-{seq}-{tag}-{s}.db", std::process::id())),
        )))
    };
    let mk = MasterKey::generate();
    let edb = Arc::new(EncryptedGraphDB::new(
        faulty.clone() as Arc<dyn GraphDB>,
        kdb("doc"),
        kdb("msg"),
        kdb("thr"),
        kdb("conv"),
        kdb("con"),
        kdb("shr"),
        Arc::new(Kek::generate()),
        Arc::new(IndexKey::generate()),
        Arc::new(DeviceKey::derive(&mk, "test-device").expect("device key")),
    ));
    (raw, faulty, edb)
}

struct H {
    app: App,
    raw: Arc<SurrealGraphDB>,
    faulty: Arc<FaultyDb>,
    store: Arc<EncryptedGraphDB>,
    thread: String,
}

/// The unlock sequence of `App::try_login`, on `store` (no auth store, no P2P).
fn unlock_into(app: &mut App, store: Arc<EncryptedGraphDB>) {
    app.account_key = Some(testing::account_key());
    app.db = Some(store.clone() as Arc<dyn GraphDB>);
    app.persona = Some(sovereign_core::auth::PersonaKind::Primary);
    app.install_document_service(store);
    wait_for_unlock_migration(app);
    app.locked = false;
    app.load_workspace_now();
}

/// The unlock starts the one-time format migration in the background
/// (`App::start_unlock_migration`). It converts — and so versions — any row
/// stored before it runs, and marks the store complete. A row a test writes
/// raw after the unlock must not race it: under load (a full suite) the
/// migration sometimes ran after the write and versioned it, so H3's "never
/// versioned" document had one version (measured 2026-10-24, 1 run in 3).
/// Waiting here gives the order a real session has: the migration over the
/// store as it was at the unlock, then the rows written later.
fn wait_for_unlock_migration(app: &mut App) {
    if let Some(task) = app.migration_task.take() {
        let _ = app.rt.block_on(task);
    }
}

fn harness(tag: &str) -> H {
    ensure_test_data_dir();
    editor_v2::use_test_system_clipboard(Box::new(MemClipboard::default()));
    let mut app = App::new();
    let (raw, faulty, store) = app.rt.block_on(faulty_store(tag));
    let thread = app
        .rt
        .block_on(store.create_thread(Thread::new("Lane".into(), String::new())))
        .unwrap()
        .id_string()
        .unwrap();
    unlock_into(&mut app, store.clone());
    H { app, raw, faulty, store, thread }
}

impl H {
    fn docs(&self) -> Arc<DocumentService> {
        self.app.docs.clone().expect("the workspace is unlocked")
    }

    /// A document created through the service, with one version per text
    /// (the first at creation, each next one an assistant write, which always
    /// makes a version). Returns the document's id **as its card carries it**.
    fn add_versions(&mut self, title: &str, texts: &[&str]) -> String {
        let docs = self.docs();
        let out = self.app.rt.block_on(docs.create(new_user_document(title, &self.thread, texts[0]))).unwrap();
        for t in &texts[1..] {
            let base = self.app.rt.block_on(docs.read(&out.id)).unwrap().revision;
            let edit = Edit {
                base,
                title: None,
                content: Some(EditContent::PlainText((*t).into())),
                origin: Origin::Assistant,
                explicit: false,
                pasted_tokens: vec![],
            };
            self.app.rt.block_on(docs.save(&out.id, edit)).unwrap();
        }
        self.app.load_workspace_now();
        let n = self.app.rt.block_on(docs.versions(&out.id)).unwrap().len();
        assert!(n >= texts.len(), "precondition: {n} versions for {} texts", texts.len());
        self.card_id(title)
    }

    /// A row written before the service existed: no version at all.
    fn add_raw(&mut self, title: &str, content: &str) -> String {
        let d = Document::new(title.into(), self.thread.clone(), true);
        let mut d = d;
        d.content = content.into();
        self.app.rt.block_on(self.store.create_document(d)).unwrap();
        self.app.load_workspace_now();
        self.card_id(title)
    }

    /// The id the app's card carries for the document titled `title`.
    fn card_id(&self, title: &str) -> String {
        self.app
            .cards
            .iter()
            .find(|c| c.title == title)
            .unwrap_or_else(|| panic!("no card titled {title}"))
            .id
            .clone()
    }

    fn card_index(&self, id: &str) -> usize {
        self.app.cards.iter().position(|c| c.id == id).unwrap_or_else(|| panic!("no card for {id}"))
    }

    fn doc_window(&self, id: &str) -> Option<usize> {
        self.app.windows.iter().position(|w| matches!(&w.content, WindowContent::Doc(d) if d.doc_id == id))
    }

    /// Opens the document from its card, as a click does.
    fn open(&mut self, id: &str) -> usize {
        let ci = self.card_index(id);
        self.app.open_doc_window(ci);
        self.doc_window(id).expect("a document window opened")
    }

    /// The window's History button: its document id, the card's title.
    fn history_button(&mut self, id: &str) {
        let wi = self.doc_window(id).expect("the document window is open");
        let doc_id = match &self.app.windows[wi].content {
            WindowContent::Doc(d) => d.doc_id.clone(),
            _ => unreachable!(),
        };
        let title = self
            .app
            .cards
            .iter()
            .find(|c| c.id == doc_id)
            .map(|c| crate::canvas::display_title(&c.title).to_string())
            .unwrap_or_default();
        self.app.open_history(&doc_id, &title);
    }

    fn history_window(&self, id: &str) -> Option<usize> {
        self.app.windows.iter().position(|w| matches!(&w.content, WindowContent::History(h) if h.doc_id == id))
    }

    fn history(&self, id: &str) -> &crate::panels::HistoryPanel {
        let wi = self.history_window(id).expect("the History window is open");
        match &self.app.windows[wi].content {
            WindowContent::History(h) => h,
            _ => unreachable!(),
        }
    }

    /// The row ids the History shows, in order.
    fn row_ids(&self, id: &str) -> Vec<String> {
        self.history(id).commits.iter().map(|c| c.commit_id.clone()).collect()
    }

    /// Closes any History window of `id` (so the next open reads afresh).
    fn close_history(&mut self, id: &str) {
        while let Some(wi) = self.history_window(id) {
            self.app.windows.remove(wi);
        }
    }

    /// The keyboard: Down `row + 1` times from no selection, then Enter.
    fn use_row_by_keyboard(&mut self, id: &str, row: usize) {
        let wi = self.history_window(id).expect("History open");
        let top = self.app.windows.len() - 1;
        assert_eq!(wi, top, "the History window is the top window");
        for _ in 0..=row {
            self.app.history_key(&Key::Named(NamedKey::ArrowDown));
        }
        assert_eq!(self.history(id).selected, Some(row), "the keyboard selected the row");
        self.app.history_key(&Key::Named(NamedKey::Enter));
    }

    /// The click: the row index the list click sets, then the button's call.
    fn use_row_by_click(&mut self, id: &str, row: usize) {
        let wi = self.history_window(id).expect("History open");
        let (doc_id, commit_id) = match &mut self.app.windows[wi].content {
            WindowContent::History(hp) => {
                hp.selected = Some(row);
                (hp.doc_id.clone(), hp.commits.get(row).map(|c| c.commit_id.clone()).expect("the row exists"))
            }
            _ => unreachable!(),
        };
        self.app.restore_commit(&doc_id, &commit_id);
    }

    fn stored_text(&self, id: &str) -> String {
        let docs = self.docs();
        self.app.rt.block_on(docs.read(id)).unwrap().plain_text()
    }

    fn pane(&mut self, id: &str) -> &mut EditorPane {
        let wi = self.doc_window(id).expect("the document's window is open");
        match &mut self.app.windows[wi].content {
            WindowContent::Doc(d) => d.editor_pane_mut().expect("the window holds the editor"),
            _ => unreachable!(),
        }
    }

    fn editor_text(&mut self, id: &str) -> String {
        self.pane(id).editor.document().plain_text()
    }

    fn card_body(&self, id: &str) -> String {
        self.app.cards[self.card_index(id)].body.clone()
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
            route_key(&mut p.editor, &Key::Character(t.as_str().into()), Some(&t), Modifiers::NONE);
        }
    }

    fn save(&mut self, id: &str) {
        self.pane(id).editor.command(Command::Save);
        self.frames(3);
    }

    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            self.app.v2_frame();
        }
    }

    fn notice(&self) -> Option<String> {
        self.app.notice.as_ref().map(|n| format!("{}\n{}", n.said.0, n.said.1))
    }

    /// What `about_to_wait` does with a pending quit: true when the app exits.
    fn about_to_wait(&mut self) -> bool {
        if self.app.quit_requested && self.app.unsaved_prompt.is_none() {
            self.app.quit_requested = false;
            return self.app.request_quit();
        }
        false
    }

    fn prompt_doc(&self) -> Option<String> {
        self.app.unsaved_prompt.as_ref().map(|p| p.doc_id.clone())
    }
}

/// A notice in plain words: no store text, no id, no JSON.
fn assert_plain(said: &str, ids: &[&str]) {
    assert!(!said.trim().is_empty());
    for bad in ["injected", "crash", "Query", "storage:", "commit:", "document:", "`", "{\"", "Invalid ID"] {
        assert!(!said.contains(bad), "the notice quotes the store or an id ({bad}): {said}");
    }
    for id in ids {
        assert!(!said.contains(id), "the notice quotes an id ({id}): {said}");
    }
}

// =================================================================== H1 — restore

/// H1 on one row: what restoring it must do, checked against the service.
fn restore_row_and_check(h: &mut H, id: &str, row: usize, by_click: bool) {
    let docs = h.docs();
    h.close_history(id);
    h.history_button(id);
    // The rows are the service's versions (H1: "The History window's rows
    // come from the document service").
    let service: Vec<String> =
        h.app.rt.block_on(docs.versions(id)).unwrap().into_iter().map(|v| v.commit_id).collect();
    assert_eq!(h.row_ids(id), service, "the rows carry the service's version ids, in its order");
    let before_rows = service.len();
    let expected = h
        .app
        .rt
        .block_on(docs.read_version(id, &service[row]))
        .unwrap()
        .doc
        .expect("a readable version")
        .plain_text();
    let current = h.stored_text(id);
    h.app.notice = None;
    if by_click {
        h.use_row_by_click(id, row);
    } else {
        h.use_row_by_keyboard(id, row);
    }
    if let Some(n) = h.notice() {
        assert!(!n.contains("Not restored"), "row {row}: refused: {n}");
    }
    assert_eq!(h.stored_text(id), expected, "row {row}: the store's text is the version's");
    assert_eq!(h.editor_text(id), expected, "row {row}: the open editor shows it");
    assert!(h.card_body(id).contains(&expected), "row {row}: the card shows it");
    let after = h.history(id).commits.len();
    if expected != current {
        assert_eq!(after, before_rows + 1, "row {row}: the History list gains the new version");
    }
    let service_after = h.app.rt.block_on(docs.versions(id)).unwrap().len();
    assert_eq!(after, service_after, "row {row}: the open History is the service's list");
}

// H1: "Choosing 'Use this version' on any row of a document with two or more
// versions restores that version", through the keyboard (Down, Enter).
#[test]
fn h1_every_history_row_restores_through_the_shell_keyboard() {
    let mut h = harness("h1-keys");
    let id = h.add_versions("Versions", &["zebra-one", "zebra-two", "zebra-three"]);
    h.open(&id);
    // The oldest, then the middle one, then the newest of the original three
    // (rows shift as restores add versions on top).
    let n = h.app.rt.block_on(h.docs().versions(&id)).unwrap().len();
    restore_row_and_check(&mut h, &id, n - 1, false);
    restore_row_and_check(&mut h, &id, 2, false);
    restore_row_and_check(&mut h, &id, 3, false);
}

// H1, the click path: a row clicked, then the "Use this version" button.
#[test]
fn h1_every_history_row_restores_through_the_shell_click() {
    let mut h = harness("h1-click");
    let id = h.add_versions("Versions", &["zebra-one", "zebra-two"]);
    h.open(&id);
    let n = h.app.rt.block_on(h.docs().versions(&id)).unwrap().len();
    for row in (1..n).rev() {
        restore_row_and_check(&mut h, &id, row, true);
    }
}

// H1 without an open editor: History reached from the card's window, which
// is then closed — the restore still lands in the store and the card.
#[test]
fn h1_a_restore_without_the_editor_open_reaches_the_store_and_the_card() {
    let mut h = harness("h1-closed");
    let id = h.add_versions("Closed", &["zebra-old", "zebra-new"]);
    h.open(&id);
    h.history_button(&id);
    let wi = h.doc_window(&id).unwrap();
    h.app.windows.remove(wi);
    let n = h.history(&id).commits.len();
    h.use_row_by_click(&id, n - 1);
    assert_eq!(h.stored_text(&id), "zebra-old");
    assert!(h.card_body(&id).contains("zebra-old"));
}

// =================================================================== H3 — errors

// H3: "When the versions cannot be read, no History window opens and a notice
// says the history could not be read, in plain words (no store text, no id)."
#[test]
fn h3_a_store_error_opens_no_history_and_says_so_plainly() {
    let mut h = harness("h3-error");
    let id = h.add_versions("Broken", &["zebra-one", "zebra-two"]);
    h.open(&id);
    let windows = h.app.windows.len();
    h.app.notice = None;
    h.faulty.crash_after(0); // every store call fails from here
    h.history_button(&id);
    h.faulty.disarm();
    assert!(h.history_window(&id).is_none(), "no History window (an empty one would say 'no versions')");
    assert_eq!(h.app.windows.len(), windows, "no window at all");
    let said = h.notice().expect("a notice says why");
    let low = said.to_lowercase();
    assert!(low.contains("history"), "it names the history: {said}");
    assert!(
        low.contains("read") || low.contains("open") || low.contains("load"),
        "it says the history could not be read: {said}"
    );
    let raw_id = id.clone();
    assert_plain(&said, &[raw_id.as_str()]);
}

// H3: "An empty History still opens with its empty state."
#[test]
fn h3_an_empty_history_still_opens_with_its_empty_state() {
    let mut h = harness("h3-empty");
    let id = h.add_raw("Never versioned", "zebra-raw");
    h.open(&id);
    h.app.notice = None;
    h.history_button(&id);
    assert!(h.history_window(&id).is_some(), "the History opens");
    assert!(h.history(&id).commits.is_empty());
    assert_eq!(h.history(&id).a11y_lines(), vec!["No saved versions yet".to_string()]);
}

// =================================================================== H4 — integrity

/// "can't be verified", with either apostrophe, or "cannot be verified".
fn says_unverifiable(line: &str) -> bool {
    let l = line.to_lowercase();
    l.contains("can't be verified") || l.contains("can\u{2019}t be verified") || l.contains("cannot be verified")
}

// H4 [choice]: "A row whose integrity check failed says so on the row ('can't
// be verified'); 'Use this version' stays refused for it with the service's
// reason, as today. Unsigned rows (older versions) carry no mark."
//
// What the row says is read from its accessible text (`a11y_lines`, one line
// per row, what a screen reader reads for it), the only text of a row a test
// can read (the drawn row is a shaped layout).
#[test]
fn h4_a_row_that_fails_its_integrity_check_says_so_and_is_refused() {
    let mut h = harness("h4");
    let id = h.add_versions("Tampered", &["zebra-one", "zebra-two", "zebra-three"]);
    let docs = h.docs();
    // An unsigned version (made before commits were MAC'd): the raw store's
    // own commit, which carries no signature.
    h.app.rt.block_on(h.raw.commit_document(&id, "older, unsigned")).expect("an unsigned commit");
    let versions = h.app.rt.block_on(docs.versions(&id)).unwrap();
    use sovereign_docs::MacState;
    let unsigned = versions.iter().position(|v| v.mac == MacState::Unsigned).expect("precondition: an unsigned row");
    // A disk-write attacker swaps the MAC of the oldest signed version.
    let target = versions.iter().rposition(|v| v.mac == MacState::Verified).unwrap();
    h.app
        .rt
        .block_on(h.raw.set_commit_signature(&versions[target].commit_id, "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="))
        .unwrap();
    assert_eq!(h.app.rt.block_on(docs.versions(&id)).unwrap()[target].mac, MacState::Invalid, "precondition");

    h.open(&id);
    let before = h.stored_text(&id);
    h.history_button(&id);
    let lines = h.history(&id).a11y_lines();
    assert_eq!(lines.len(), versions.len());
    for (i, line) in lines.iter().enumerate() {
        if i == target {
            assert!(says_unverifiable(line), "the tampered row says so: {line}");
        } else {
            assert!(!says_unverifiable(line), "row {i} carries no mark: {line}");
            assert!(!line.to_lowercase().contains("verif"), "row {i} (unsigned: {}) carries no mark: {line}", i == unsigned);
        }
    }
    h.app.notice = None;
    h.use_row_by_click(&id, target);
    let said = h.notice().expect("refused, with a notice");
    assert!(said.contains("Not restored"), "{said}");
    assert!(said.to_lowercase().contains("integrity"), "the service's reason: {said}");
    assert_eq!(h.stored_text(&id), before, "nothing restored");
}

// =================================================================== H5 — locked

// H5: "Without a document service (not unlocked), History does not open and
// the notice says to log in, as restore does today."
#[test]
fn h5_without_the_document_service_history_does_not_open_and_says_to_log_in() {
    let mut h = harness("h5");
    let id = h.add_versions("Locked", &["zebra-one", "zebra-two"]);
    h.open(&id);
    let docs = h.app.docs.take();
    h.app.notice = None;
    h.history_button(&id);
    assert!(h.history_window(&id).is_none(), "no History window");
    let said = h.notice().expect("a notice");
    assert!(said.to_lowercase().contains("log in"), "it says to log in: {said}");
    h.app.docs = docs;
}

// =================================================================== D — Discard

/// D1's scenario: saves failing for A and B; on quit the user chooses Discard
/// for A, then Cancel for B. Returns (harness, A, B, the document service
/// taken away to make saves fail).
fn discard_then_cancel(tag: &str) -> (H, String, String, Option<Arc<DocumentService>>) {
    let mut h = harness(tag);
    let a = h.add_versions("Doc A", &["zebra-a"]);
    let b = h.add_versions("Doc B", &["zebra-b"]);
    h.open(&a);
    h.open(&b);
    let docs = h.app.docs.take(); // saves now fail
    h.type_in(&a, " typed-in-A");
    h.save(&a);
    h.type_in(&b, " typed-in-B");
    h.save(&b);
    assert!(!h.app.request_quit(), "unsaved edits: the quit asks");
    assert_eq!(h.prompt_doc().as_deref(), Some(a.as_str()), "it asks about A first");
    h.app.resolve_unsaved(UnsavedChoice::Discard);
    assert!(!h.about_to_wait(), "the quit goes on to B");
    assert_eq!(h.prompt_doc().as_deref(), Some(b.as_str()), "then about B");
    h.app.resolve_unsaved(UnsavedChoice::Cancel);
    (h, a, b, docs)
}

// D1: "Cancel on a quit's unsaved prompt withdraws every Discard chosen
// during that quit: those documents' edits count as unsaved again, in their
// windows, as they were. Nothing is written, nothing is lost."
#[test]
fn d1_cancel_withdraws_the_discards_of_that_quit() {
    let (mut h, a, b, docs) = discard_then_cancel("d1");
    assert!(h.app.unsaved_prompt.is_none());
    assert!(!h.app.quit_requested, "Cancel ends the quit");
    assert!(!h.about_to_wait(), "the app does not exit");
    assert!(h.doc_window(&a).is_some(), "A's window is still there");
    assert!(h.pane(&a).has_unsaved(), "A's edits count as unsaved again");
    assert!(
        matches!(&h.app.windows[h.doc_window(&a).unwrap()].content, WindowContent::Doc(d) if d.has_unsaved_edits()),
        "and the window says so"
    );
    assert!(h.editor_text(&a).contains("typed-in-A"), "A's text is as it was");
    assert!(h.pane(&b).has_unsaved(), "B's too");
    assert!(h.editor_text(&b).contains("typed-in-B"));
    h.app.docs = docs;
    assert_eq!(h.stored_text(&a), "zebra-a", "nothing written");
    assert_eq!(h.stored_text(&b), "zebra-b");
}

// D2: "After that, a new quit asks again for them".
#[test]
fn d2_a_later_quit_asks_again_for_a_discarded_then_cancelled_document() {
    let (mut h, a, _b, docs) = discard_then_cancel("d2-quit");
    assert!(!h.app.request_quit(), "the later quit does not go through");
    assert_eq!(h.prompt_doc().as_deref(), Some(a.as_str()), "it asks about A again");
    h.app.resolve_unsaved(UnsavedChoice::Cancel);
    h.app.docs = docs;
}

// D2: "restoring a version … treat[s] them as unsaved (refuse or ask, as for
// any unsaved document)".
#[test]
fn d2_restoring_a_version_treats_the_document_as_unsaved() {
    let (mut h, a, _b, docs) = discard_then_cancel("d2-restore");
    h.app.docs = docs; // the service is back; the edits are still unsaved
    let first = h.app.rt.block_on(h.docs().versions(&a)).unwrap().last().unwrap().commit_id.clone();
    h.app.restore_commit(&a, &first);
    assert!(h.editor_text(&a).contains("typed-in-A"), "the edits stay");
    assert!(h.pane(&a).has_unsaved(), "still unsaved");
    assert!(
        h.notice().is_some_and(|n| n.contains("unsaved")) || h.app.unsaved_prompt.is_some(),
        "refused or asked: {:?}",
        h.notice()
    );
}

// D2: "applying a skill … treat[s] them as unsaved".
#[test]
fn d2_applying_a_skill_treats_the_document_as_unsaved() {
    let (mut h, a, _b, docs) = discard_then_cancel("d2-skill");
    h.app.docs = docs;
    let stored = h.stored_text(&a);
    let si = h
        .app
        .skills
        .all_skills()
        .iter()
        .position(|s| s.name() == "table-of-contents")
        .expect("the TOC skill");
    h.app.run_skill(&a, si);
    assert_eq!(h.stored_text(&a), stored, "nothing written");
    assert!(h.doc_window(&a).is_some(), "the window stays");
    assert!(h.editor_text(&a).contains("typed-in-A"), "the edits stay");
}

// D2: "'trash all' treat[s] them as unsaved". Read here as the card's
// "Move to trash" (`trash_document`), the line the review cites for it
// (app.rs:2927 in v0.0.11) — see the next test for the orphans' "Trash all".
#[test]
fn d2_moving_to_the_trash_treats_the_document_as_unsaved() {
    let (mut h, a, _b, docs) = discard_then_cancel("d2-trash");
    h.app.docs = docs;
    h.app.trash_document(&a);
    assert!(h.doc_window(&a).is_some(), "the window stays");
    assert!(h.editor_text(&a).contains("typed-in-A"), "the edits stay");
    let d = h.app.rt.block_on(h.docs().read(&a)).unwrap();
    assert!(d.trashed_at.is_none(), "not in the trash");
}

// D2, literally "trash all": the orphans' "Trash all" for a deleted thread's
// documents (`orphan_start(Group, Trash)`).
//
// CONTRACT-QUESTION: the contract's "trash all" may mean the single-document
// trash (the review lists app.rs:2927, `trash_document`, under "trashing
// all"). The orphans' "Trash all" has no unsaved guard at all today (review
// finding 22), for any document, so this test also asks for finding 22's
// guard on that path. Written for the literal reading; Céline/laptop to say
// whether it belongs to this lot.
#[test]
fn d2_the_orphans_trash_all_treats_the_document_as_unsaved() {
    let (mut h, a, _b, docs) = discard_then_cancel("d2-trash-all");
    h.app.docs = docs;
    let thread = h.thread.clone();
    h.app.rt.block_on(h.store.soft_delete_thread(&thread)).unwrap();
    h.app.load_workspace_now();
    h.app.orphan_start(
        crate::orphans_ui::OrphanTarget::Group(thread),
        crate::orphans::OrphanAction::Trash,
        "the documents".into(),
    );
    let d = h.app.rt.block_on(h.docs().read(&a)).unwrap();
    assert!(d.trashed_at.is_none(), "A is not trashed over its unsaved edits");
    assert!(h.editor_text(&a).contains("typed-in-A"), "the edits stay");
}

// D3: "Unchanged: Discard when closing one window closes it".
#[test]
fn d3_discard_when_closing_one_window_still_closes_it() {
    let mut h = harness("d3-close");
    let a = h.add_versions("Closing", &["zebra-a"]);
    let wi = h.open(&a);
    let docs = h.app.docs.take();
    h.type_in(&a, " typed");
    h.save(&a);
    h.app.request_close_window(wi);
    h.frames(3);
    assert_eq!(h.prompt_doc().as_deref(), Some(a.as_str()), "it asks");
    h.app.resolve_unsaved(UnsavedChoice::Discard);
    assert!(h.doc_window(&a).is_none(), "Discard closes it");
    h.app.docs = docs;
    assert_eq!(h.stored_text(&a), "zebra-a", "nothing written");
}

// D3: "Discard during a quit that goes through lets the app exit".
#[test]
fn d3_discard_during_a_quit_that_goes_through_lets_the_app_exit() {
    let mut h = harness("d3-quit");
    let a = h.add_versions("Quitting", &["zebra-a"]);
    let b = h.add_versions("Also quitting", &["zebra-b"]);
    h.open(&a);
    h.open(&b);
    let docs = h.app.docs.take();
    h.type_in(&a, " typed");
    h.save(&a);
    h.type_in(&b, " typed");
    h.save(&b);
    assert!(!h.app.request_quit());
    h.app.resolve_unsaved(UnsavedChoice::Discard);
    assert!(!h.about_to_wait(), "B is asked next");
    assert_eq!(h.prompt_doc().as_deref(), Some(b.as_str()));
    h.app.resolve_unsaved(UnsavedChoice::Discard);
    assert!(h.about_to_wait(), "every unsaved document discarded: the app exits");
    h.app.docs = docs;
}

// D3: "Save during a quit saves then quits."
#[test]
fn d3_save_during_a_quit_saves_then_quits() {
    let mut h = harness("d3-save");
    let a = h.add_versions("Saving", &["zebra-a"]);
    h.open(&a);
    let docs = h.app.docs.take();
    h.type_in(&a, " typed-then-saved");
    h.save(&a);
    assert!(!h.app.request_quit());
    assert_eq!(h.prompt_doc().as_deref(), Some(a.as_str()));
    h.app.docs = docs; // saves work again
    h.app.resolve_unsaved(UnsavedChoice::Save);
    let mut quit = false;
    for _ in 0..10 {
        h.frames(1);
        if h.about_to_wait() {
            quit = true;
            break;
        }
    }
    assert!(quit, "the app quits once saved");
    assert!(h.stored_text(&a).contains("typed-then-saved"), "saved before quitting");
}
