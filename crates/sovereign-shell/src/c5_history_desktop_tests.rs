//! The desktop's History that cannot be read (item C5 of v0.0.13; spec ch. 03,
//! *History, in v0.0.13*, HR1, HR2 and HR4). The phone's half is in
//! `sovereign-app/src/c5_history_tests.rs` and the frontend's
//! `MobileHistory.c5.svelte.test.ts`; the rows' names (HN) are another
//! author's.
//!
//! Written before the code, by another author: whoever implements C5 makes
//! these pass **without editing this file**; a test that looks wrong is
//! reported, not worked around.
//!
//! What was decided (2026-11-18, the Fable maille by delegation of Céline):
//! the sentences are worded once, in the document service; the desktop builds
//! its notice from it and adds its own tail; one version that cannot be read
//! stays in its place, said by its sentence, and « Use this version » is not
//! offered for it; the mark is made where the version is read, so every
//! caller of the service gets it.
//!
//! The seams (the test author's « types and signatures » commit):
//! `VersionInfo::could_not_read` and `HistoryPanel::offers_restore`, which the
//! drawn bar, its click and Enter all ask.
//!
//! A child module of `app`, as `lot1_tests` is: it drives the window
//! manager's own paths on a real `App` (the window's History button, the
//! keyboard). Its fixtures are copied from `lot1_tests`, whose own are
//! private. Store: a real `EncryptedGraphDB` over the document service
//! suite's `FaultyDb` over an in-memory SurrealDB. One version cannot be read
//! for real: it is written under the encrypted layer with bytes that do not
//! decrypt where its title and content belong.

use std::sync::{Arc, OnceLock};

use chrono::{Duration, Utc};
use sovereign_ai::docs_host::{new_user_document, testing};
use sovereign_crypto::device_key::DeviceKey;
use sovereign_crypto::index_key::IndexKey;
use sovereign_crypto::kek::Kek;
use sovereign_crypto::key_db::KeyDatabase;
use sovereign_crypto::master_key::MasterKey;
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::schema::{Commit, DocumentSnapshot, Thread};
use sovereign_db::surreal::{StorageMode, SurrealGraphDB};
use sovereign_db::traits::GraphDB;
use sovereign_docs::{DocumentService, COULD_NOT_READ_HISTORY, VERSION_COULD_NOT_BE_READ};
use winit::keyboard::{Key, NamedKey};

use super::{App, WindowContent};

#[path = "../../sovereign-docs/tests/common/faulty.rs"]
#[allow(dead_code, clippy::all)]
mod faulty;
use faulty::FaultyDb;

/// base64("not-a-real-ciphertext") and 24 zero bytes: valid base64, not a
/// ciphertext of anything.
const JUNK_CT: &str = "bm90LWEtcmVhbC1jaXBoZXJ0ZXh0";
const JUNK_NONCE: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
/// The store read under `DocumentService::versions`.
const VERSIONS_READ: &str = "list_document_commits";
const NO_VERSIONS: &str = "No saved versions yet";

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
        let d = std::env::temp_dir().join(format!("sov-c5-desktop-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::env::set_var("SOVEREIGN_DATA_DIR", &d);
        d
    });
}

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
            dir.join(format!("c5-desktop-{}-{nanos}-{seq}-{tag}-{s}.db", std::process::id())),
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

/// The unlock sequence of `App::try_login`, on `store`, waiting for the
/// one-time migration as `lot1_tests` does.
fn unlock_into(app: &mut App, store: Arc<EncryptedGraphDB>) {
    app.account_key = Some(testing::account_key());
    app.db = Some(store.clone() as Arc<dyn GraphDB>);
    app.persona = Some(sovereign_core::auth::PersonaKind::Primary);
    app.install_document_service(store);
    if let Some(task) = app.migration_task.take() {
        let _ = app.rt.block_on(task);
    }
    app.locked = false;
    app.load_workspace_now();
}

fn harness(tag: &str) -> H {
    ensure_test_data_dir();
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

    /// A document with one version that reads; its id as its card carries it.
    fn document(&mut self, title: &str) -> String {
        let docs = self.docs();
        let id = self.app.rt.block_on(docs.create(new_user_document(title, &self.thread, "some text"))).unwrap().id;
        self.app.rt.block_on(self.store.commit_document(&id, "Saved")).expect("a version that reads");
        self.app.load_workspace_now();
        self.app.cards.iter().find(|c| c.title == title).expect("its card").id.clone()
    }

    /// Writes, under the encrypted layer, a version of `doc_id` whose title
    /// and content do not decrypt, a minute after now (the newest); its id.
    fn unreadable_version(&mut self, doc_id: &str) -> String {
        let c = self
            .app
            .rt
            .block_on(self.raw.insert_commit(Commit {
                id: None,
                document_id: doc_id.to_string(),
                parent_commit: None,
                message: "Saved".into(),
                timestamp: Utc::now() + Duration::minutes(1),
                snapshot: DocumentSnapshot {
                    document_id: doc_id.to_string(),
                    title: JUNK_CT.into(),
                    content: JUNK_CT.into(),
                    content_nonce: Some(JUNK_NONCE.into()),
                    title_nonce: Some(JUNK_NONCE.into()),
                    ..Default::default()
                },
                signature: None,
            }))
            .expect("a version written at rest");
        c.id_string().expect("its id")
    }

    /// Opens the document from its card, then its History, as the window's
    /// History button does.
    fn open_history(&mut self, id: &str) {
        let ci = self.app.cards.iter().position(|c| c.id == id).expect("its card");
        self.app.open_doc_window(ci);
        let title = crate::canvas::display_title(&self.app.cards[ci].title).to_string();
        self.app.open_history(id, &title);
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

    /// The keyboard: Down `row + 1` times from no selection.
    fn select_by_keyboard(&mut self, id: &str, row: usize) {
        assert_eq!(self.history_window(id), Some(self.app.windows.len() - 1), "History is the top window");
        for _ in 0..=row {
            self.app.history_key(&Key::Named(NamedKey::ArrowDown));
        }
        assert_eq!(self.history(id).selected, Some(row), "the keyboard selected the row");
    }

    fn stored_text(&self, id: &str) -> String {
        self.app.rt.block_on(self.docs().read(id)).unwrap().plain_text()
    }

    fn version_count(&self, id: &str) -> usize {
        self.app.rt.block_on(self.docs().versions(id)).unwrap().len()
    }

    fn notice(&self) -> Option<String> {
        self.app.notice.as_ref().map(|n| format!("{}\n{}", n.said.0, n.said.1))
    }
}

// ============================================================ HR1 — the whole list

// HR1: « On the desktop, History opens no window and its notice says it »,
// with the document service's sentence; the host holds no copy.
#[test]
fn hr1_a_history_that_cannot_be_read_opens_no_window_and_says_the_services_sentence() {
    let mut h = harness("hr1");
    let id = h.document("Notes");
    h.app.notice = None;
    h.faulty.fail_named(&[VERSIONS_READ]);
    h.open_history(&id);
    h.faulty.stop_failing_named();
    assert!(h.history_window(&id).is_none(), "no History window (an empty one would say there are no versions)");
    let said = h.notice().expect("a notice says it");
    assert!(said.contains(COULD_NOT_READ_HISTORY), "the document service's sentence, as worded there: {said}");
    assert!(!said.contains(NO_VERSIONS), "{said}");
}

// HR1: « The sentence is worded in the document service (`sovereign-docs`);
// the host shows it and holds no copy. » Test files are left out.
#[test]
fn hr1_the_shell_holds_no_copy_of_the_sentence() {
    // The apostrophe as the service writes it, as the character ’, and as
    // the escape the shell's sources write it with.
    let curly = COULD_NOT_READ_HISTORY.replace('\'', "\u{2019}");
    let escaped = COULD_NOT_READ_HISTORY.replace('\'', "\\u{2019}");
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut seen = 0;
    let mut stack = vec![dir];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            if !name.ends_with(".rs") || name.ends_with("_tests.rs") {
                continue;
            }
            seen += 1;
            let text = std::fs::read_to_string(&p).unwrap();
            for copy in [COULD_NOT_READ_HISTORY, curly.as_str(), escaped.as_str()] {
                assert!(!text.contains(copy), "{} holds a copy of « {copy} »", p.display());
            }
        }
    }
    assert!(seen > 10, "fixture: the scan read the shell's sources ({seen})");
}

// HR2: « A list asked for while the session is locked is refused, as any call
// is then; it is not said to be unreadable. »
#[test]
fn hr2_a_locked_session_is_not_said_to_be_unreadable() {
    let mut h = harness("hr2");
    let id = h.document("Notes");
    h.app.docs = None;
    h.app.notice = None;
    h.app.open_history(&id, "Notes");
    assert!(h.history_window(&id).is_none());
    let said = h.notice().expect("a notice says why");
    assert!(!said.contains(COULD_NOT_READ_HISTORY), "a refusal, not « could not be read »: {said}");
}

// ============================================================ HR4 — one version

// HR4, at the service: « The mark is made where the version is read, so every
// caller of the document service gets it. »
#[test]
fn hr4_the_service_marks_the_version_it_cannot_read_and_only_it() {
    let mut h = harness("hr4-service");
    let id = h.document("Notes");
    let bad = h.unreadable_version(&id);
    let versions = h.app.rt.block_on(h.docs().versions(&id)).expect("the list itself reads");
    assert!(versions.len() >= 2, "both versions are listed: {versions:?}");
    for v in &versions {
        if v.commit_id == bad {
            assert_eq!(v.could_not_read, Some(VERSION_COULD_NOT_BE_READ), "{v:?}");
        } else {
            assert_eq!(v.could_not_read, None, "a version that reads carries no mark: {v:?}");
        }
    }
}

// HR4: « It is listed at its place and time among the others », said by the
// service's sentence, on the row and in what a screen reader reads.
#[test]
fn hr4_the_row_stays_in_its_place_and_says_the_sentence() {
    let mut h = harness("hr4-row");
    let id = h.document("Notes");
    let bad = h.unreadable_version(&id);
    let service: Vec<String> =
        h.app.rt.block_on(h.docs().versions(&id)).unwrap().into_iter().map(|v| v.commit_id).collect();
    h.open_history(&id);
    let rows: Vec<String> = h.history(&id).commits.iter().map(|c| c.commit_id.clone()).collect();
    assert_eq!(rows, service, "every version is a row, in the service's order");
    let at = rows.iter().position(|r| *r == bad).expect("the version that cannot be read is a row");
    let lines = h.history(&id).a11y_lines();
    assert_eq!(lines.len(), rows.len());
    for (i, line) in lines.iter().enumerate() {
        if i == at {
            assert!(line.contains(VERSION_COULD_NOT_BE_READ), "the row says it: {line}");
            assert!(!line.contains(JUNK_CT), "never its ciphertext: {line}");
            assert!(line.contains(&h.history(&id).commits[i].when), "at its time: {line}");
        } else {
            assert!(!line.contains(VERSION_COULD_NOT_BE_READ), "row {i} reads: {line}");
        }
    }
}

// HR4: « "Restore" (desktop) [is] not offered for it. » The bar, its click and
// Enter ask `offers_restore`; the version that reads keeps it.
#[test]
fn hr4_use_this_version_is_not_offered_for_it_and_kept_for_the_others() {
    let mut h = harness("hr4-offer");
    let id = h.document("Notes");
    let bad = h.unreadable_version(&id);
    h.open_history(&id);
    let at = h.history(&id).commits.iter().position(|c| c.commit_id == bad).expect("its row");
    let other = (0..h.history(&id).commits.len()).find(|i| *i != at).expect("a row that reads");
    h.select_by_keyboard(&id, at);
    assert!(!h.history(&id).offers_restore(), "not offered for the version that cannot be read");
    h.app.windows.retain(|w| !matches!(&w.content, WindowContent::History(_)));
    h.open_history(&id);
    h.select_by_keyboard(&id, other);
    assert!(h.history(&id).offers_restore(), "offered for a version that reads");
}

// HR4: Enter on it restores nothing and is not refused after the fact (until
// v0.0.13 « the desktop offered Restore and refused it after the click »).
#[test]
fn hr4_enter_on_it_changes_nothing_and_says_no_refusal() {
    let mut h = harness("hr4-enter");
    let id = h.document("Notes");
    let bad = h.unreadable_version(&id);
    h.open_history(&id);
    let at = h.history(&id).commits.iter().position(|c| c.commit_id == bad).expect("its row");
    let (before, versions) = (h.stored_text(&id), h.version_count(&id));
    h.select_by_keyboard(&id, at);
    h.app.notice = None;
    h.app.history_key(&Key::Named(NamedKey::Enter));
    assert_eq!(h.stored_text(&id), before, "nothing restored");
    assert_eq!(h.version_count(&id), versions, "no version added");
    if let Some(said) = h.notice() {
        assert!(!said.contains("Not restored"), "nothing offered, so nothing refused: {said}");
    }
}
