//! v0.0.12 item 7 — the guarantees that need the API the item adds
//! (`doc/plans/V0012-item7-conversion.md`, V1 and V3). Written before the
//! code, by another author: whoever implements item 7 makes these pass without
//! editing this file. **It does not compile until that API exists**; the
//! guarantees testable with today's API are in `item7_conversion.rs`.
//!
//! The API named here (the most natural names; the implementer provides them
//! or reports why not):
//!
//! - `EncryptedGraphDB::list_documents_including_deleted_and_undecryptable()
//!   -> DbResult<(Vec<Document>, Vec<UndecryptableDocument>)>`, with
//!   `sovereign_db::encrypted::UndecryptableDocument { id: String,
//!   modified_at: DateTime<Utc>, .. }` — "an API that returns them beside the
//!   readable ones; the ordinary listings stay as they are" (V1). The id is
//!   the row's id as `Document::id_string` gives it; the date is the row's
//!   `modified_at`, which is stored in clear (V2 shows it).
//! - `MigrationReport::undecryptable: Vec<DocId>` — the rows the conversion
//!   found and could not decrypt, apart from `unreadable` (rows that decrypt
//!   and do not parse, V6).
//! - `DocumentService::with_build_version(self, version: &str) -> Self` — the
//!   build version the service runs under, which the one-time conversion
//!   records and compares (V3). A host sets its own (`env!("CARGO_PKG_VERSION")`
//!   of the shell or the app: `sovereign-docs` has its own crate version,
//!   which is not the build's). `run_unlock_migration` and
//!   `start_unlock_migration` keep their signatures (the M1–M8 tests call them
//!   as they are).
//!
//! - **V1 — Undecryptable rows are counted**: the conversion learns which rows
//!   exist but cannot be decrypted, by id, from the store.
//! - **V3 — Retried after a version change only**: the completion record
//!   keeps the build version that ran and the undecryptable ids; at an unlock
//!   with the same version nothing is retried; with a different version those
//!   ids are tried again, and a row that now decrypts is converted like any
//!   other and leaves the list.
//!
//! "A row that now decrypts" is the same row given back its sealed body at
//! rest (as if what kept it from decrypting had been fixed by the update).

mod common;

use std::sync::{Arc, Mutex};

use chrono::{TimeZone, Utc};
use common::*;
use serde_json::Value;
use sovereign_docs::unlock_migration::RECORD_KEY;
use sovereign_docs::{
    run_unlock_migration, start_unlock_migration, unlock_migration_complete, DocumentService, MigrationRun,
    ServiceDeps,
};

const LEGACY_MD: &str = "# raw markdown\n\nzebra body";
const LEGACY_ENV: &str = r#"{"body":"envelope body","images":[]}"#;
const UNPARSEABLE: &str = r#"{"format":"sovereign.doc","v":2,"blocks":"not a list"}"#;
const JUNK_CT: &str = "bm90LWEtcmVhbC1jaXBoZXJ0ZXh0";
const JUNK_NONCE: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

/// Two build versions, neither of them any crate's real version.
const BUILD_A: &str = "0.0.12-item7-test-a";
const BUILD_B: &str = "0.0.13-item7-test-b";

// ----------------------------------------------------------------- helpers --

fn is_v2(stored: &str) -> bool {
    serde_json::from_str::<Value>(stored)
        .map(|v| v["format"] == "sovereign.doc" && v["v"] == 2 && v["blocks"].is_array())
        .unwrap_or(false)
}

async fn make_undecryptable(s: &Stack, id: &str) -> (String, String) {
    let row = s.raw.get_document(id).await.expect("the row at rest");
    let nonce = row.encryption_nonce.clone().expect("sealed at rest");
    s.raw.set_document_content_encryption(id, JUNK_CT, JUNK_NONCE).await.expect("corrupt at rest");
    assert!(s.edb.get_document(id).await.is_err(), "fixture: the row no longer decrypts");
    (row.content, nonce)
}

/// Gives the row its sealed body back: from now on it decrypts.
async fn give_back(s: &Stack, id: &str, sealed: &(String, String)) {
    s.raw.set_document_content_encryption(id, &sealed.0, &sealed.1).await.expect("restore at rest");
    assert!(s.edb.get_document(id).await.is_ok(), "fixture: the row decrypts again");
}

/// A service over the stack's store, running under `version`: an unlock of
/// a build with that version.
fn unlock_as(s: &Stack, version: &str) -> Arc<DocumentService> {
    Arc::new(
        DocumentService::new(ServiceDeps {
            store: s.edb.clone(),
            commit_policy: s.policy.clone(),
            pii: s.pii.clone(),
            media: s.media.clone(),
            clock: s.clock.clone(),
            events: s.events.clone(),
            device: s.device.clone(),
        })
        .with_build_version(version),
    )
}

/// The completion record as stored (decrypted by the encrypted layer). Its
/// shape is the implementer's; the tests only look for what it must keep.
async fn record(s: &Stack) -> String {
    s.edb.get_store_record(RECORD_KEY).await.expect("read").expect("the completion record")
}

/// The part of a row id that names it (`document:abc` → `abc`).
fn key_of(id: &str) -> &str {
    id.rsplit(':').next().unwrap_or(id)
}

async fn legacy_versions(s: &Stack, id: &str, legacy: &str) -> usize {
    s.committed_states(id).await.into_iter().filter(|(_, c)| c == legacy).count()
}

// ---------------------------------------------------------------------- V1 --

/// V1, the store's side: the new listing returns the readable rows (the same
/// as the ordinary listing) and, beside them, each row that cannot be
/// decrypted by id with its last modified date; the ordinary listing stays as
/// it is (it still leaves the row out, DB-M1).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v1_the_store_lists_undecryptable_rows_beside_the_readable_ones() {
    let s = Stack::new("v1-store").await;
    let ok = s.legacy("Readable", LEGACY_MD).await;
    let native = s.create_text("Native", "v2").await;
    let bad = s.legacy("Bad", LEGACY_MD).await;
    let when = Utc.with_ymd_and_hms(2025, 3, 2, 9, 0, 0).unwrap(); // `Stack::legacy`'s modified_at
    make_undecryptable(&s, &bad).await;

    let ordinary: Vec<String> =
        s.edb.list_documents_including_deleted().await.unwrap().iter().filter_map(|d| d.id_string()).collect();
    assert!(!ordinary.contains(&bad), "the ordinary listing stays as it is (V1)");

    let (readable, undecryptable) =
        s.edb.list_documents_including_deleted_and_undecryptable().await.expect("the listing survives the row");
    let mut readable: Vec<String> = readable.iter().filter_map(|d| d.id_string()).collect();
    readable.sort();
    let mut want = vec![ok.clone(), native.clone()];
    want.sort();
    assert_eq!(readable, want, "the readable rows, as the ordinary listing gives them");
    assert_eq!(undecryptable.len(), 1, "the one row that cannot be decrypted");
    assert_eq!(undecryptable[0].id, bad, "by id");
    assert_eq!(undecryptable[0].modified_at, when, "with its last modified date");
}

/// V1, the report: `migrate_all` lists the undecryptable row apart from the
/// unparseable one (V6), and neither is a store failure.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v1_the_report_lists_undecryptable_rows_apart_from_unparseable_ones() {
    let s = Stack::new("v1-report").await;
    s.legacy("L", LEGACY_ENV).await;
    let unparseable = s.legacy("Unparseable", UNPARSEABLE).await;
    let bad = s.legacy("Bad", LEGACY_MD).await;
    make_undecryptable(&s, &bad).await;

    let report = s.svc.migrate_all().await.expect("migrate");
    assert_eq!(report.undecryptable, vec![bad.clone()], "the undecryptable row, by id");
    assert_eq!(report.unreadable, vec![unparseable], "the unparseable row, as today (V6)");
    assert!(report.failed.is_empty(), "an undecryptable row is not a store failure: {:?}", report.failed);
    assert_eq!(report.migrated.len(), 1);
}

// ---------------------------------------------------------------------- V3 --

/// V3: "The completion record keeps the build version that ran and the
/// undecryptable ids."
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v3_the_completion_record_keeps_the_build_version_and_the_ids() {
    let s = Stack::new("v3-record").await;
    s.legacy("L", LEGACY_ENV).await;
    let bad = s.legacy("Bad", LEGACY_MD).await;
    let bad2 = s.legacy("Bad 2", LEGACY_ENV).await;
    make_undecryptable(&s, &bad).await;
    make_undecryptable(&s, &bad2).await;

    let svc = unlock_as(&s, BUILD_A);
    assert!(matches!(run_unlock_migration(&svc).await.expect("run"), MigrationRun::Completed { .. }));
    let rec = record(&s).await;
    assert!(rec.contains(BUILD_A), "the record keeps the build version that ran: {rec}");
    assert!(rec.contains(key_of(&bad)) && rec.contains(key_of(&bad2)), "and the undecryptable ids: {rec}");
}

/// V3: "At an unlock with the same version, nothing is retried" — even when
/// the row would now decrypt. It stays as it was, unconverted, and stays in
/// the record's list.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v3_the_same_version_retries_nothing() {
    let s = Stack::new("v3-same").await;
    let bad = s.legacy("Bad", LEGACY_MD).await;
    let sealed = make_undecryptable(&s, &bad).await;
    run_unlock_migration(&unlock_as(&s, BUILD_A)).await.expect("first unlock");

    give_back(&s, &bad, &sealed).await;
    s.events.take();
    let again = run_unlock_migration(&unlock_as(&s, BUILD_A)).await.expect("a later unlock, same build");
    assert_eq!(again, MigrationRun::AlreadyComplete, "same version: nothing runs (V3, M1)");
    assert_eq!(s.stored(&bad).await.1, LEGACY_MD, "not retried: still as it was");
    assert!(s.commits(&bad).await.is_empty(), "no version made");
    assert!(s.events.peek().is_empty(), "nothing emitted");
    assert!(record(&s).await.contains(key_of(&bad)), "still in the list");
}

/// V3: "At an unlock with a different build version, those ids are tried
/// again: a row that now decrypts is converted like any other and leaves the
/// list." Converted like any other: v2, its text kept, its earlier state in
/// its History once, the conversion's event; and the record now names the new
/// version.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v3_a_different_version_retries_and_a_row_that_now_decrypts_is_converted() {
    let s = Stack::new("v3-diff").await;
    s.legacy("L", LEGACY_ENV).await;
    let bad = s.legacy("Bad", LEGACY_MD).await;
    let sealed = make_undecryptable(&s, &bad).await;
    run_unlock_migration(&unlock_as(&s, BUILD_A)).await.expect("first unlock");
    assert!(record(&s).await.contains(key_of(&bad)));

    give_back(&s, &bad, &sealed).await;
    s.events.take();
    let svc = unlock_as(&s, BUILD_B);
    let run = run_unlock_migration(&svc).await.expect("an unlock with another build");
    assert!(!matches!(run, MigrationRun::Interrupted { .. }), "{run:?}");

    let stored = s.stored(&bad).await.1;
    assert!(is_v2(&stored), "retried and converted (V3)");
    // Raw markdown is kept whole, heading included, as for any other row (g03_migration.rs, g03_migration_keeps_the_text).
    assert_eq!(texts(&stored), vec!["# raw markdown\n\nzebra body".to_string()], "converted like any other: its text kept");
    assert_eq!(legacy_versions(&s, &bad, LEGACY_MD).await, 1, "its earlier state is in its History, once");
    assert!(
        s.events.peek().iter().any(|e| e.id == bad && e.kind == sovereign_docs::ChangeKind::Migrated),
        "the conversion's event"
    );
    let rec = record(&s).await;
    assert!(!rec.contains(key_of(&bad)), "it leaves the list: {rec}");
    assert!(rec.contains(BUILD_B), "the record names the build that ran: {rec}");
    assert!(unlock_migration_complete(&svc).await.unwrap());

    // And the retry happened once: a later unlock of the same new build has
    // nothing to do.
    assert_eq!(run_unlock_migration(&unlock_as(&s, BUILD_B)).await.unwrap(), MigrationRun::AlreadyComplete);
}

/// V3: a row that still cannot be decrypted at the different version stays
/// in the list, now under the new version — so a later unlock of that same
/// build does not retry it, even if it would decrypt by then.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v3_a_row_still_undecryptable_stays_listed_under_the_new_version() {
    let s = Stack::new("v3-still").await;
    let bad = s.legacy("Bad", LEGACY_MD).await;
    let sealed = make_undecryptable(&s, &bad).await;
    run_unlock_migration(&unlock_as(&s, BUILD_A)).await.expect("first unlock");

    run_unlock_migration(&unlock_as(&s, BUILD_B)).await.expect("another build, the row still undecryptable");
    let rec = record(&s).await;
    assert!(rec.contains(key_of(&bad)), "still listed: {rec}");
    assert!(rec.contains(BUILD_B), "under the build that tried: {rec}");

    give_back(&s, &bad, &sealed).await;
    assert_eq!(run_unlock_migration(&unlock_as(&s, BUILD_B)).await.unwrap(), MigrationRun::AlreadyComplete);
    assert_eq!(s.stored(&bad).await.1, LEGACY_MD, "same build: not retried");
}

/// V3 with M1: what a version change retries is "those ids", not the whole
/// conversion. A legacy row that arrived after the conversion completed (in
/// v1 or raw form, G2) is left for its next write, as M1 says, even at an
/// unlock with another build.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v3_a_version_change_retries_those_ids_only() {
    let s = Stack::new("v3-only").await;
    let bad = s.legacy("Bad", LEGACY_MD).await;
    let sealed = make_undecryptable(&s, &bad).await;
    run_unlock_migration(&unlock_as(&s, BUILD_A)).await.expect("first unlock");
    let late = s.legacy("Arrived later", LEGACY_ENV).await;

    give_back(&s, &bad, &sealed).await;
    run_unlock_migration(&unlock_as(&s, BUILD_B)).await.expect("another build");
    assert!(is_v2(&s.stored(&bad).await.1), "the listed id is retried");
    assert_eq!(s.stored(&late).await.1, LEGACY_ENV, "the late legacy row is not converted (M1)");
    assert!(s.commits(&late).await.is_empty());
}

/// V3, the host's path: the seam records the version of the service it is
/// given (the host's build), and a later unlock through the seam with the
/// same version retries nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v3_the_seam_records_the_hosts_build_version() {
    let s = Stack::new("v3-seam").await;
    let bad = s.legacy("Bad", LEGACY_MD).await;
    let sealed = make_undecryptable(&s, &bad).await;
    for round in 0..2 {
        let slot: Arc<Mutex<Option<sovereign_docs::MigrationTask>>> = Arc::new(Mutex::new(None));
        let keep = slot.clone();
        start_unlock_migration(unlock_as(&s, BUILD_A), move |t| *keep.lock().unwrap() = Some(t), |_| {});
        let task = slot.lock().unwrap().take().expect("task");
        task.await;
        assert!(record(&s).await.contains(BUILD_A), "round {round}: the host's build version is recorded");
        if round == 0 {
            give_back(&s, &bad, &sealed).await;
        }
    }
    assert_eq!(s.stored(&bad).await.1, LEGACY_MD, "same build through the seam: not retried");
}

/// CONTRACT-QUESTION (finding 12, third bullet: "The completion record is
/// written, so the conversion never runs again for it"): a store whose
/// conversion completed under v0.0.11 has a record with no version and no
/// list — and its undecryptable rows were never counted. The contract does
/// not say what the first unlock of the new build does with it. Best reading,
/// asserted here: a record without a version is a record of another version,
/// and its list is unknown rather than empty, so that unlock looks for
/// undecryptable rows and lists them (they are then counted and shown in
/// Alerts), converting none of the readable legacy rows that arrived since
/// (M1). Whether it also tells the person (a notice) is left open.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v3_contract_question_a_record_from_v0_0_11_gets_its_undecryptable_rows_listed() {
    let s = Stack::new("v3-old-record").await;
    let bad = s.legacy("Bad", LEGACY_MD).await;
    make_undecryptable(&s, &bad).await;
    // The record exactly as v0.0.11 wrote it (`unlock_migration.rs`,
    // `Record::to_json`): the counts and the flag, sealed by the store.
    s.edb
        .put_store_record(RECORD_KEY, r#"{"converted":3,"not_converted":0,"notice_shown":true}"#)
        .await
        .expect("the v0.0.11 record");
    let late = s.legacy("Arrived later", LEGACY_ENV).await;

    run_unlock_migration(&unlock_as(&s, BUILD_A)).await.expect("the first unlock of the new build");
    let rec = record(&s).await;
    assert!(rec.contains(key_of(&bad)), "the row v0.0.11 skipped in silence is now listed: {rec}");
    assert!(rec.contains(BUILD_A), "{rec}");
    assert_eq!(s.stored(&late).await.1, LEGACY_ENV, "M1: no readable row converted by it");
}
