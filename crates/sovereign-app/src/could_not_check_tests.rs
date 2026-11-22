//! The Alerts' lists that cannot be read, in the Tauri host (spec ch. 03,
//! *What cannot be read is never shown as empty*): the layer under the
//! commands `list_sync_alerts`, `list_orphaned_documents` and
//! `list_unreadable_documents`. Written before the code, by another author:
//! whoever implements it makes these pass without editing this file; a test
//! that looks wrong is reported, not worked around. The Svelte side is tested
//! by vitest (`SyncAlerts.couldNotCheck.svelte.test.ts`), the desktop shell by
//! `sovereign-shell/src/could_not_check_tests.rs`.
//!
//! What was decided (2026-11-17, the Fable maille by delegation of Céline; the
//! scope by PANDA2, the contract's author: the three lists of the Alerts, on
//! both hosts):
//! - a list that cannot be read is said, never shown as empty: the command
//!   returns no list and the sentence of `sovereign-docs`
//!   (`COULD_NOT_CHECK_ALERTS`, `COULD_NOT_CHECK_ORPHANS`,
//!   `COULD_NOT_CHECK_UNREADABLE`) in `could_not_check`; `None` when it reads;
//! - a locked session stays a refusal (`Err`), never a "could not check": the
//!   frontend reads a refusal as nothing to show.
//!
//! **The seam** (the test author's, in the « types and signatures » commit):
//! - `tauri_commands::sync_alerts::sync_alerts_listing(docs, names) ->
//!   Result<SyncAlertsDto, String>`, `SyncAlertsDto { alerts, could_not_check }`;
//! - `doc_orphans::orphans_listing(docs, names) -> Result<OrphansDto, String>`,
//!   `OrphansDto { orphans, could_not_check }`;
//! - `docs` is what `AppState::docs` gives: the session's document service,
//!   or why there is none; the two commands pass it as it comes;
//! - `UnreadableDocumentsDto::could_not_check: Option<String>`;
//! - the inner functions keep their signatures (`list_sync_alerts_of`,
//!   `doc_orphans::list_orphaned_documents`: `Result<Vec<…>, String>`).
//!
//! Stores: a real `EncryptedGraphDB` over the generated `FaultyDb` of the
//! document service's suite (`sovereign-docs/tests/common/faulty.rs`) over an
//! in-memory SurrealDB. A list of alerts or of orphans cannot be read when
//! the store's read under it fails (`FaultyDb::fail_named`: an undecryptable
//! row is skipped by the listing, so it does not make the list fail). The
//! list of documents that could not be read is made unreadable for real: its
//! record, sealed at rest, is replaced by bytes that do not decrypt. The
//! spec's shared assertion is `sovereign-docs/tests/common/counted_and_shown.rs`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sovereign_ai::docs_host::{build_document_service, new_user_document, testing};
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
    run_unlock_migration, DocumentService, IncomingAlert, COULD_NOT_CHECK_ALERTS, COULD_NOT_CHECK_ORPHANS,
    COULD_NOT_CHECK_UNREADABLE,
};

use crate::doc_orphans::{orphans_listing, OrphansDto};
use crate::doc_unreadable::{list_unreadable_documents, UnreadableDocumentsDto};
use crate::tauri_commands::sync_alerts::{sync_alerts_listing, SyncAlertsDto};

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

/// What `AppState::docs` says while the workspace is locked.
const LOCKED: &str = "The workspace is locked: log in to change documents.";

// ======================================================================= fixtures

struct S {
    raw: Arc<SurrealGraphDB>,
    faulty: Arc<FaultyDb>,
    store: Arc<EncryptedGraphDB>,
    docs: Arc<DocumentService>,
}

/// The phone's service over an encrypted store over `FaultyDb`, after the
/// one-time conversion (what an unlock runs).
async fn session(tag: &str) -> S {
    let raw = Arc::new(SurrealGraphDB::new(StorageMode::Memory).await.expect("memory store"));
    raw.connect().await.expect("connect");
    raw.init_schema().await.expect("schema");
    let faulty = Arc::new(FaultyDb::new(raw.clone() as Arc<dyn GraphDB>));
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let dir = std::env::temp_dir();
    let kdb = |s: &str| {
        Arc::new(tokio::sync::RwLock::new(KeyDatabase::new(
            dir.join(format!("could-not-check-app-{}-{nanos}-{seq}-{tag}-{s}.db", std::process::id())),
        )))
    };
    let mk = MasterKey::generate();
    let store = Arc::new(EncryptedGraphDB::new(
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
    let docs = Arc::new(build_document_service(store.clone(), Some(testing::account_key()), "phone".into()));
    run_unlock_migration(&docs).await.expect("the conversion runs");
    S { raw, faulty, store, docs }
}

impl S {
    async fn thread(&self, name: &str) -> String {
        self.store.create_thread(Thread::new(name.into(), String::new())).await.unwrap().id_string().unwrap()
    }

    /// One open alert; the document's id.
    async fn one_alert(&self) -> String {
        let t = self.thread("Lane").await;
        let id = self.docs.create(new_user_document("Watched", &t, "text")).await.unwrap().id;
        self.docs
            .raise_alert(IncomingAlert {
                id: id.clone(),
                device: "peerlaptopc8".into(),
                finding: "injected instructions in the new content".into(),
                previous_version: None,
            })
            .await
            .expect("the alert is raised");
        id
    }

    /// One document of a thread deleted by another device; its id.
    async fn one_waiting(&self) -> String {
        let t = self.thread("Notes").await;
        let id = self.docs.create(new_user_document("Mine", &t, "text")).await.unwrap().id;
        self.store.soft_delete_thread(&t).await.unwrap();
        id
    }

    async fn alerts_cannot_be_read(&self) {
        self.faulty.fail_named(&[ALERTS_READ]);
        assert!(self.docs.alerts().await.is_err(), "fixture: the alerts can no longer be read");
    }

    async fn orphans_cannot_be_read(&self) {
        self.faulty.fail_named(&[ORPHANS_READ]);
        assert!(
            self.docs.orphans_of_deleted_threads().await.is_err(),
            "fixture: the documents whose thread was deleted can no longer be read"
        );
    }

    async fn unreadable_list_cannot_be_read(&self) {
        self.raw
            .put_store_record_sealed(sovereign_docs::unlock_migration::RECORD_KEY, JUNK_CT, JUNK_NONCE)
            .await
            .expect("corrupt the record at rest");
        assert!(
            sovereign_docs::unlock_migration_undecryptable(&self.docs).await.is_err(),
            "fixture: the list of documents that could not be read can no longer be read"
        );
    }
}

fn names() -> HashMap<String, String> {
    HashMap::new()
}

/// Every text a list hands the frontend to show: its sentence, then its items.
fn shown(could_not_check: &Option<String>, items: impl IntoIterator<Item = String>) -> Vec<String> {
    could_not_check.iter().cloned().chain(items).collect()
}

// ====================================================== list_sync_alerts

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_alerts_that_cannot_be_read_come_as_the_sentence_and_no_list() {
    let s = session("alerts-fail").await;
    s.one_alert().await;
    s.alerts_cannot_be_read().await;
    let dto: SyncAlertsDto = sync_alerts_listing(Ok(s.docs.clone()), &names())
        .await
        .expect("a read failure is not a refusal: the command answers, and says it");
    assert!(dto.alerts.is_empty(), "no alert is made up: {dto:?}");
    assert_eq!(dto.could_not_check.as_deref(), Some(COULD_NOT_CHECK_ALERTS));
    let shown = shown(&dto.could_not_check, dto.alerts.iter().map(|a| a.title.clone()));
    assert_counted_and_shown("list_sync_alerts", dto.could_not_check.iter().count(), &shown, COULD_NOT_CHECK_ALERTS, &[]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_alerts_that_read_come_as_the_list_and_no_sentence() {
    let s = session("alerts-read").await;
    let id = s.one_alert().await;
    let dto = sync_alerts_listing(Ok(s.docs.clone()), &names()).await.expect("the alerts read");
    assert_eq!(dto.could_not_check, None, "nothing to say when they read");
    assert_eq!(dto.alerts.iter().map(|a| a.doc_id.clone()).collect::<Vec<_>>(), vec![id]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_locked_session_is_still_refused_for_the_alerts() {
    let r = sync_alerts_listing(Err(LOCKED.to_string()), &names()).await;
    assert_eq!(r.map(|d| d.could_not_check).unwrap_err(), LOCKED, "a refusal stays a refusal, never a « could not check »");
}

// ================================================ list_orphaned_documents

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_documents_whose_thread_was_deleted_that_cannot_be_read_come_as_the_sentence_and_no_list() {
    let s = session("orphans-fail").await;
    s.one_waiting().await;
    s.orphans_cannot_be_read().await;
    let dto: OrphansDto = orphans_listing(Ok(s.docs.clone()), &names())
        .await
        .expect("a read failure is not a refusal: the command answers, and says it");
    assert!(dto.orphans.is_empty(), "no document is made up: {dto:?}");
    assert_eq!(dto.could_not_check.as_deref(), Some(COULD_NOT_CHECK_ORPHANS));
    let shown = shown(&dto.could_not_check, dto.orphans.iter().map(|o| o.title.clone()));
    assert_counted_and_shown(
        "list_orphaned_documents",
        dto.could_not_check.iter().count(),
        &shown,
        COULD_NOT_CHECK_ORPHANS,
        &[],
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_documents_whose_thread_was_deleted_that_read_come_as_the_list_and_no_sentence() {
    let s = session("orphans-read").await;
    let id = s.one_waiting().await;
    let dto = orphans_listing(Ok(s.docs.clone()), &names()).await.expect("they read");
    assert_eq!(dto.could_not_check, None, "nothing to say when they read");
    assert_eq!(dto.orphans.iter().map(|o| o.doc_id.clone()).collect::<Vec<_>>(), vec![id]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_locked_session_is_still_refused_for_the_documents_whose_thread_was_deleted() {
    let r = orphans_listing(Err(LOCKED.to_string()), &names()).await;
    assert_eq!(r.map(|d| d.could_not_check).unwrap_err(), LOCKED, "a refusal stays a refusal, never a « could not check »");
}

// ============================================== list_unreadable_documents

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_unreadable_documents_that_cannot_be_listed_come_as_the_sentence_and_no_lines() {
    let s = session("unreadable-fail").await;
    s.unreadable_list_cannot_be_read().await;
    let dto: UnreadableDocumentsDto = list_unreadable_documents(Some(&s.docs)).await;
    assert!(dto.lines.is_empty(), "no line is made up: {dto:?}");
    assert_eq!(dto.could_not_check.as_deref(), Some(COULD_NOT_CHECK_UNREADABLE), "{dto:?}");
    let shown = shown(&dto.could_not_check, dto.lines.clone());
    assert_counted_and_shown(
        "list_unreadable_documents",
        dto.could_not_check.iter().count(),
        &shown,
        COULD_NOT_CHECK_UNREADABLE,
        &[],
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_unreadable_documents_that_list_come_with_no_sentence() {
    let s = session("unreadable-read").await;
    let dto = list_unreadable_documents(Some(&s.docs)).await;
    assert_eq!(dto.could_not_check, None, "nothing to say when the list reads: {dto:?}");
}

// ============================================================ the shapes

/// The objects travel with the field names `frontend/src/lib/api/commands.ts`
/// reads (`SyncAlertsDto`, `OrphansDto`, `UnreadableDocumentsDto`).
#[test]
fn the_lists_travel_with_the_frontends_field_names() {
    let alerts = serde_json::to_value(SyncAlertsDto { alerts: vec![], could_not_check: None }).unwrap();
    assert_eq!(alerts, serde_json::json!({ "alerts": [], "could_not_check": null }));
    let orphans = serde_json::to_value(OrphansDto { orphans: vec![], could_not_check: Some("x".into()) }).unwrap();
    assert_eq!(orphans, serde_json::json!({ "orphans": [], "could_not_check": "x" }));
    // The list of documents that could not be read keeps its three fields of
    // v0.0.12 item 7 V2b when it reads; the sentence travels only when set.
    let unreadable = |could_not_check: Option<String>| {
        serde_json::to_value(UnreadableDocumentsDto { heading: "h".into(), meaning: "m".into(), lines: vec![], could_not_check })
            .unwrap()
    };
    assert_eq!(unreadable(None), serde_json::json!({ "heading": "h", "meaning": "m", "lines": [] }));
    assert_eq!(
        unreadable(Some("x".into())),
        serde_json::json!({ "heading": "h", "meaning": "m", "lines": [], "could_not_check": "x" })
    );
}

// ======================================================== the commands

fn src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// The body of `pub async fn <name>(` in `text`, to its first closing brace
/// at the start of a line, top level or in a module (the commands are
/// formatted that way, with no inner block closing at those indents).
fn body_of<'a>(text: &'a str, name: &str) -> &'a str {
    let start = text.find(&format!("pub async fn {name}(")).unwrap_or_else(|| panic!("fn {name}"));
    let rest = &text[start..];
    let end = [rest.find("\n}"), rest.find("\n    }")].into_iter().flatten().min().expect("the function's end");
    &rest[..end]
}

/// The two commands run the functions tested above, on what `AppState::docs`
/// gives, after the session check: what is tested is what the app runs.
#[test]
fn the_commands_run_the_tested_listings_after_the_session_check() {
    for (file, command, listing, dto) in [
        ("tauri_commands/sync_alerts.rs", "list_sync_alerts(webview", "sync_alerts_listing(state.docs().await", "SyncAlertsDto"),
        ("doc_orphans.rs", "list_orphaned_documents(webview", "orphans_listing(state.docs().await", "OrphansDto"),
    ] {
        let text = std::fs::read_to_string(src().join(file)).unwrap_or_else(|e| panic!("read {file}: {e}"));
        let name = command.split('(').next().unwrap();
        let start = text.find(&format!("pub async fn {command}")).unwrap_or_else(|| panic!("{file}: fn {command}"));
        let body = body_of(&text[start..], name);
        assert!(body.contains(&format!("Result<{dto}, String>")), "{file}: {name} returns {dto}: {body}");
        let gate = body.find("state.require_unlocked(&webview).await?").unwrap_or_else(|| panic!("{file}: {name} is gated: {body}"));
        let call = body.find(listing).unwrap_or_else(|| panic!("{file}: {name} runs `{listing}…`: {body}"));
        assert!(gate < call, "{file}: the session check comes first: {body}");
    }
}
