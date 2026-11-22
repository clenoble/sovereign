//! The Alerts window's lists that cannot be read, in the native shell (spec
//! ch. 03, *What cannot be read is never shown as empty*). Written before the
//! code, by another author: whoever implements it makes these pass without
//! editing this file; a test that looks wrong is reported, not worked around.
//!
//! What was decided (2026-11-17, the Fable maille by delegation of Céline; the
//! scope by PANDA2, the contract's author: the three lists of the Alerts, on
//! both hosts):
//! - a list that cannot be read is said in its section's place, where its
//!   rows would be, with the words of `sovereign-docs`
//!   (`COULD_NOT_CHECK_ALERTS`, `COULD_NOT_CHECK_ORPHANS`,
//!   `COULD_NOT_CHECK_UNREADABLE`), never shown as an empty list;
//! - a failure counts as something to show: the status line says the
//!   sentence, as it says "1 document could not be read";
//! - under the unreadable documents' heading, the meaning line does not show
//!   with the sentence;
//! - `refresh_orphans` no longer keeps the list it had: it says the sentence.
//!
//! What is read: the window's accessibility lines (what a screen reader is
//! given, and what the window shows), `AlertsPanel::could_not_check`, the
//! status line (`App::status_notice`) and `App::could_not_check`, the state
//! behind it. The spec's shared assertion ("a session's own unreadable item
//! is counted, and the count is shown") is
//! `sovereign-docs/tests/common/counted_and_shown.rs`, used by both hosts.
//!
//! A child of `app` (declared in `app.rs`), like `lot1_tests`, so it drives a
//! real `App`: the unlock, `refresh_alerts`, `refresh_orphans` and the Alerts
//! window.
//!
//! Stores: a real `EncryptedGraphDB` over the generated `FaultyDb` of the
//! document service's suite (`sovereign-docs/tests/common/faulty.rs`) over an
//! in-memory SurrealDB. A list of alerts or of orphans cannot be read when
//! the store's read under it fails (`FaultyDb::fail_named`: an undecryptable
//! row is skipped by the listing, so it does not make the list fail). The
//! list of documents that could not be read is made unreadable for real: its
//! record, sealed at rest, is replaced by bytes that do not decrypt.
//! Data directory: a fresh temporary one unless `SOVEREIGN_DATA_DIR` is set.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use sovereign_ai::docs_host::{new_user_document, testing};
use sovereign_crypto::device_key::DeviceKey;
use sovereign_crypto::index_key::IndexKey;
use sovereign_crypto::kek::Kek;
use sovereign_crypto::key_db::KeyDatabase;
use sovereign_crypto::master_key::MasterKey;
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::schema::Thread;
use sovereign_db::surreal::{StorageMode, SurrealGraphDB};
use sovereign_db::traits::GraphDB;
use sovereign_docs::{
    DocumentService, IncomingAlert, COULD_NOT_CHECK_ALERTS, COULD_NOT_CHECK_ORPHANS, COULD_NOT_CHECK_UNREADABLE,
    UNREADABLE_HEADING, UNREADABLE_MEANING,
};

use super::{App, WindowContent, WindowKind};
use crate::panels::{load_alerts_as, AlertsPanel};

#[path = "../../sovereign-docs/tests/common/faulty.rs"]
#[allow(dead_code, clippy::all)]
mod faulty;
use faulty::FaultyDb;

#[path = "../../sovereign-docs/tests/common/counted_and_shown.rs"]
mod counted_and_shown;
use counted_and_shown::assert_counted_and_shown;

/// base64("not-a-real-ciphertext") and 24 zero bytes: valid base64, not a
/// ciphertext of anything (DB-M1's test).
const JUNK_CT: &str = "bm90LWEtcmVhbC1jaXBoZXJ0ZXh0";
const JUNK_NONCE: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

/// The store read under the alerts (`DocumentService::alerts`).
const ALERTS_READ: &str = "list_documents_including_deleted";
/// The store read under the documents whose thread was deleted
/// (`DocumentService::orphans_of_deleted_threads`, its first read).
const ORPHANS_READ: &str = "list_threads_including_deleted";

/// What the window says when there are no alerts (`AlertsPanel`): never in
/// place of alerts that could not be read.
const NO_ALERTS: &str = "No alerts.";

/// What the status line says while one document waits for a decision
/// (`orphans::status_segment`).
const ONE_WAITS: &str = "1 document waits";

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
        let d = std::env::temp_dir().join(format!("sov-could-not-check-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::env::set_var("SOVEREIGN_DATA_DIR", &d);
        d
    });
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
            dir.join(format!("could-not-check-shell-{}-{nanos}-{seq}-{tag}-{s}.db", std::process::id())),
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
}

/// The unlock sequence of `App::try_login`, on `store` (no auth store, no
/// P2P), waiting for the one-time conversion it starts (as `lot1_tests`).
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
    unlock_into(&mut app, store.clone());
    H { app, raw, faulty, store }
}

impl H {
    fn docs(&self) -> Arc<DocumentService> {
        self.app.docs.clone().expect("the workspace is unlocked")
    }

    fn thread(&self, name: &str) -> String {
        self.app
            .rt
            .block_on(self.store.create_thread(Thread::new(name.into(), String::new())))
            .unwrap()
            .id_string()
            .unwrap()
    }

    fn create(&self, title: &str, thread: &str) -> String {
        let docs = self.docs();
        self.app.rt.block_on(docs.create(new_user_document(title, thread, "text"))).unwrap().id
    }

    /// One document of a thread deleted by another device: it waits for a
    /// decision (as `orphans_tests`, "Notes").
    fn one_waiting(&mut self) -> String {
        let t = self.thread("Notes");
        let id = self.create("Mine", &t);
        self.app.rt.block_on(self.store.soft_delete_thread(&t)).unwrap();
        id
    }

    /// One open alert on a document of this store.
    fn one_alert(&mut self) -> String {
        let t = self.thread("Lane");
        let id = self.create("Watched", &t);
        let docs = self.docs();
        self.app
            .rt
            .block_on(docs.raise_alert(IncomingAlert {
                id: id.clone(),
                device: "peer-phone-c8".into(),
                finding: "injected instructions in the new content".into(),
                previous_version: None,
            }))
            .expect("the alert is raised");
        id
    }

    /// The list of documents that could not be read, made unreadable at
    /// rest: its record no longer decrypts.
    fn unreadable_list_cannot_be_read(&self) {
        self.app
            .rt
            .block_on(self.raw.put_store_record_sealed(sovereign_docs::unlock_migration::RECORD_KEY, JUNK_CT, JUNK_NONCE))
            .expect("corrupt the record at rest");
        let docs = self.docs();
        assert!(
            self.app.rt.block_on(sovereign_docs::unlock_migration_undecryptable(&docs)).is_err(),
            "fixture: the list of documents that could not be read can no longer be read"
        );
    }

    /// The reads under the alerts fail from now on.
    fn alerts_cannot_be_read(&self) {
        self.faulty.fail_named(&[ALERTS_READ]);
        assert!(self.app.rt.block_on(self.docs().alerts()).is_err(), "fixture: the alerts can no longer be read");
    }

    /// The reads under the documents whose thread was deleted fail from now on.
    fn orphans_cannot_be_read(&self) {
        self.faulty.fail_named(&[ORPHANS_READ]);
        assert!(
            self.app.rt.block_on(self.docs().orphans_of_deleted_threads()).is_err(),
            "fixture: the documents whose thread was deleted can no longer be read"
        );
        assert!(self.app.rt.block_on(self.docs().alerts()).is_ok(), "fixture: the alerts still read");
    }

    /// The Alerts window as `load_alerts_as` builds it (what the window opens with).
    fn panel(&self) -> AlertsPanel {
        let docs = self.docs();
        self.app.rt.block_on(load_alerts_as(&docs, &HashMap::new(), ""))
    }

    /// The open Alerts window (opened if it is not).
    fn window(&mut self) -> &AlertsPanel {
        if !self.app.windows.iter().any(|w| matches!(w.content, WindowContent::Alerts(_))) {
            self.app.focus_or_toggle(WindowKind::Alerts);
        }
        self.app
            .windows
            .iter()
            .find_map(|w| match &w.content {
                WindowContent::Alerts(ap) => Some(ap),
                _ => None,
            })
            .expect("the Alerts window is open (unlocked)")
    }

    /// What the status line says, or "" when it says nothing.
    fn status(&self) -> String {
        self.app.status_notice().map(|(s, _)| s).unwrap_or_default()
    }
}

fn index_of(lines: &[String], text: &str) -> Option<usize> {
    lines.iter().position(|l| l == text)
}

// ============================================================ the Alerts window

/// The alerts cannot be read: the window says so where the alerts would be
/// (its first line, before any other section), and never "No alerts.".
#[test]
fn the_alerts_that_cannot_be_read_are_said_in_their_place() {
    let mut h = harness("alerts-window");
    h.one_alert();
    h.alerts_cannot_be_read();
    let panel = h.panel();
    let lines = panel.a11y_lines();
    assert_eq!(
        lines.first().map(String::as_str),
        Some(COULD_NOT_CHECK_ALERTS),
        "the sentence in the alerts' place, first in the window: {lines:#?}"
    );
    assert_eq!(panel.could_not_check(), &[COULD_NOT_CHECK_ALERTS], "only the alerts could not be read");
    assert!(panel.rows.is_empty(), "no alert row is made up");
    assert_counted_and_shown("the alerts, in the Alerts window", panel.could_not_check().len(), &lines, COULD_NOT_CHECK_ALERTS, &[NO_ALERTS]);
}

/// The documents whose thread was deleted cannot be read: the window says so
/// in their section's place, after the alerts (which still read) and before
/// the documents that could not be read.
#[test]
fn the_documents_whose_thread_was_deleted_that_cannot_be_read_are_said_in_their_place() {
    let mut h = harness("orphans-window");
    h.one_alert();
    h.one_waiting();
    h.orphans_cannot_be_read();
    let panel = h.panel();
    let lines = panel.a11y_lines();
    let at = index_of(&lines, COULD_NOT_CHECK_ORPHANS)
        .unwrap_or_else(|| panic!("the sentence is a line of the window: {lines:#?}"));
    let last_alert = lines.iter().rposition(|l| l.starts_with("Alert ")).expect("fixture: the alert still shows");
    assert!(at > last_alert, "after the alerts, where the section's rows would be: {lines:#?}");
    if let Some(u) = lines.iter().position(|l| l.starts_with(UNREADABLE_HEADING)) {
        assert!(at < u, "before the documents that could not be read: {lines:#?}");
    }
    assert_eq!(panel.could_not_check(), &[COULD_NOT_CHECK_ORPHANS], "only the orphans could not be read");
    assert_eq!(panel.orphans().count(), 0, "no line is made up");
    assert_counted_and_shown(
        "the documents whose thread was deleted, in the Alerts window",
        panel.could_not_check().len(),
        &lines,
        COULD_NOT_CHECK_ORPHANS,
        &[],
    );
}

/// The list of documents that could not be read cannot itself be read: under
/// its heading, the sentence, and not the meaning line.
#[test]
fn the_unreadable_documents_that_cannot_be_listed_are_said_under_their_heading_without_the_meaning() {
    let h = harness("unreadable-window");
    h.unreadable_list_cannot_be_read();
    let panel = h.panel();
    let lines = panel.a11y_lines();
    let heading = lines
        .iter()
        .position(|l| l.starts_with(UNREADABLE_HEADING))
        .unwrap_or_else(|| panic!("the heading of the documents that could not be read: {lines:#?}"));
    assert_eq!(
        lines.get(heading + 1).map(String::as_str),
        Some(COULD_NOT_CHECK_UNREADABLE),
        "the sentence right under the heading: {lines:#?}"
    );
    assert!(!lines.iter().any(|l| l.contains(UNREADABLE_MEANING)), "the meaning line does not show with it: {lines:#?}");
    assert_eq!(panel.could_not_check(), &[COULD_NOT_CHECK_UNREADABLE]);
    assert!(panel.unreadable().is_empty(), "no line is made up");
    assert_counted_and_shown(
        "the documents that could not be read, in the Alerts window",
        panel.could_not_check().len(),
        &lines,
        COULD_NOT_CHECK_UNREADABLE,
        &[UNREADABLE_MEANING],
    );
}

// ============================================================== the status line

/// Runs `refresh_alerts` with the Alerts window open and checks that the
/// status line and the window say `sentence`, with nothing else waiting.
fn said_by_status_line_and_window(h: &mut H, sentence: &'static str, what: &str) {
    h.app.refresh_alerts();
    let status = h.status();
    assert!(status.contains(sentence), "{what}: the status line says {sentence:?}, with nothing else waiting; it says {status:?}");
    assert_eq!(h.app.could_not_check, vec![sentence], "{what}: the state behind the status line");
    assert_counted_and_shown(&format!("{what}, in the status line"), h.app.could_not_check.len(), &[status], sentence, &[]);
    let window = h.window().a11y_lines();
    assert!(window.iter().any(|l| l == sentence), "{what}: the open window, re-read, says it too: {window:#?}");
}

#[test]
fn the_status_line_says_the_alerts_could_not_be_read() {
    let mut h = harness("alerts-status");
    h.window();
    h.alerts_cannot_be_read();
    said_by_status_line_and_window(&mut h, COULD_NOT_CHECK_ALERTS, "the alerts");
    // Read again once the store reads: the sentence is gone (it is what was
    // last read, not a mark that stays).
    h.faulty.stop_failing_named();
    h.app.refresh_alerts();
    assert!(!h.status().contains(COULD_NOT_CHECK_ALERTS), "once the alerts read again, not said: {:?}", h.status());
    assert!(h.app.could_not_check.is_empty());
}

#[test]
fn the_status_line_says_the_documents_whose_thread_was_deleted_could_not_be_read() {
    let mut h = harness("orphans-status");
    h.window();
    h.orphans_cannot_be_read();
    said_by_status_line_and_window(&mut h, COULD_NOT_CHECK_ORPHANS, "the documents whose thread was deleted");
    h.faulty.stop_failing_named();
    h.app.refresh_alerts();
    assert!(!h.status().contains(COULD_NOT_CHECK_ORPHANS), "once they read again, not said: {:?}", h.status());
    assert!(h.app.could_not_check.is_empty());
}

#[test]
fn the_status_line_says_the_unreadable_documents_could_not_be_listed() {
    let mut h = harness("unreadable-status");
    h.window();
    h.unreadable_list_cannot_be_read();
    said_by_status_line_and_window(&mut h, COULD_NOT_CHECK_UNREADABLE, "the documents that could not be read");
}

// ============================================================ refresh_orphans

/// `refresh_orphans` (run with every workspace reload) used to keep the list
/// it had when the new one could not be read. It no longer shows it: the
/// status line and the open window say the sentence instead.
#[test]
fn refresh_orphans_says_the_sentence_instead_of_keeping_the_old_list() {
    let mut h = harness("orphans-refresh");
    let waiting = h.one_waiting();
    h.app.refresh_orphans();
    assert_eq!(h.app.orphan_count, 1, "fixture: one document waits");
    assert_eq!(
        h.window().orphans().lines().iter().map(|l| l.id.clone()).collect::<Vec<_>>(),
        vec![waiting],
        "fixture: the open window lists it"
    );
    assert!(h.status().contains(ONE_WAITS), "fixture: the status line counts it: {:?}", h.status());

    h.orphans_cannot_be_read();
    h.app.refresh_orphans();

    let status = h.status();
    assert!(!status.contains(ONE_WAITS), "the list read before is not kept: {status:?}");
    assert_eq!(h.app.orphan_count, 0, "the count read before is not kept");
    assert!(status.contains(COULD_NOT_CHECK_ORPHANS), "the status line says the sentence: {status:?}");
    assert_eq!(h.app.could_not_check, vec![COULD_NOT_CHECK_ORPHANS]);
    let window = h.window();
    assert_eq!(window.orphans().count(), 0, "the open window no longer lists the old list");
    assert_eq!(window.could_not_check(), &[COULD_NOT_CHECK_ORPHANS]);
    let lines = window.a11y_lines();
    assert_counted_and_shown(
        "refresh_orphans, the open window",
        window.could_not_check().len(),
        &lines,
        COULD_NOT_CHECK_ORPHANS,
        &[],
    );
}
