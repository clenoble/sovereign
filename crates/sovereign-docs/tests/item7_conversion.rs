//! v0.0.12 item 7 — the one-time conversion counts what it cannot read, and
//! does not freeze saves (`doc/plans/V0012-item7-conversion.md`, V1, V4, V6;
//! review of v0.0.11, findings 11 and 12). Written before the code, by another
//! author: whoever implements item 7 makes these pass without editing this
//! file. The guarantees that need an API the item adds (the store's listing of
//! undecryptable rows, the report's list, the build version) are in
//! `item7_conversion_api.rs`, so this file compiles today and shows its reds.
//!
//! - **V1 — Undecryptable rows are counted**: a document row that exists but
//!   cannot be decrypted is counted in the notice ("… could not be read (left
//!   as it was)"), and nothing about it is changed.
//! - **V4 — Saves are not frozen by the conversion**: with a conversion slowed
//!   per document (every store call waits, `FaultyDb::slow_calls`), a save, a
//!   trash or a create issued mid-run completes before the run ends.
//! - **V6 — Unchanged**: a row whose content does not parse stays read-only
//!   and is counted as today (beside the undecryptable ones, not instead of
//!   them). M1–M8 are `step6_unlock_migration.rs` and `step6_scans.rs`,
//!   unchanged.
//!
//! An undecryptable row is made as DB-M1's test makes one
//! (`sovereign-db/tests/encrypted_over_surreal.rs`,
//! `one_undecryptable_row_does_not_brick_lists`): its ciphertext and nonce are
//! overwritten, below the encrypted layer, with valid base64 that does not
//! decrypt — what a corrupted restore leaves behind.

mod common;

use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use common::*;
use serde_json::Value;
use sovereign_docs::{
    run_unlock_migration, start_unlock_migration, unlock_migration_complete, ChangeKind, MigrationNotice,
    MigrationRun, ReadOnlyReason, Writable,
};

/// Legacy stored forms the conversion converts (G2).
const LEGACY: [&str; 4] = [
    r#"{"body":"envelope body","images":[]}"#,
    r#"{"body":"with media","images":[{"path":"/p/a.png","caption":"A"}]}"#,
    "# raw markdown\n\nbody",
    "",
];

/// Content that claims to be a Sovereign document and does not parse (V6:
/// "a row whose content does not parse").
const UNPARSEABLE: &str = r#"{"format":"sovereign.doc","v":2,"blocks":"not a list"}"#;

/// base64("not-a-real-ciphertext") and 24 zero bytes: valid base64, not a
/// ciphertext of anything (as in DB-M1's test).
const JUNK_CT: &str = "bm90LWEtcmVhbC1jaXBoZXJ0ZXh0";
const JUNK_NONCE: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

// ----------------------------------------------------------------- helpers --

fn is_v2(stored: &str) -> bool {
    serde_json::from_str::<Value>(stored)
        .map(|v| v["format"] == "sovereign.doc" && v["v"] == 2 && v["blocks"].is_array())
        .unwrap_or(false)
}

fn notice_of(run: &MigrationRun) -> Option<MigrationNotice> {
    match run {
        MigrationRun::Completed { notice } => notice.clone(),
        _ => None,
    }
}

/// Makes `id`'s row undecryptable at rest: its ciphertext and nonce replaced
/// below the encrypted layer. Returns the row's sealed body as it was (so a
/// test can give it back), and asserts the encrypted layer now refuses it.
async fn make_undecryptable(s: &Stack, id: &str) -> (String, String) {
    let row = s.raw.get_document(id).await.expect("the row at rest");
    let nonce = row.encryption_nonce.clone().expect("a body the service's store wrote is sealed at rest");
    s.raw.set_document_content_encryption(id, JUNK_CT, JUNK_NONCE).await.expect("corrupt at rest");
    assert!(s.edb.get_document(id).await.is_err(), "fixture: the row no longer decrypts");
    (row.content, nonce)
}

/// The row as stored at rest, as JSON: "nothing about it is changed" is this
/// value unchanged.
async fn at_rest(s: &Stack, id: &str) -> Value {
    serde_json::to_value(s.raw.get_document(id).await.expect("the row at rest")).unwrap()
}

/// The seam with a spawner that only keeps the task.
fn start_captured(
    svc: &Arc<sovereign_docs::DocumentService>,
) -> (Option<sovereign_docs::MigrationTask>, mpsc::Receiver<MigrationNotice>) {
    let slot: Arc<Mutex<Option<sovereign_docs::MigrationTask>>> = Arc::new(Mutex::new(None));
    let (tx, rx) = mpsc::channel();
    let keep = slot.clone();
    start_unlock_migration(
        svc.clone(),
        move |task| *keep.lock().unwrap() = Some(task),
        move |n| {
            let _ = tx.send(n);
        },
    );
    let task = slot.lock().unwrap().take();
    (task, rx)
}

// ---------------------------------------------------------------------- V1 --

/// V1: "The notice says so: '9 converted, 1 could not be read (left as it
/// was).' Nothing about such a row is changed." The run completes (an
/// undecryptable row is not a store failure: no interruption), the notice
/// counts the row among those that could not be read, and the row at rest —
/// ciphertext, nonce, title, times, head, writer — is exactly what it was,
/// with no version made of it and no event about it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v1_an_undecryptable_row_is_counted_in_the_notice_and_left_as_it_was() {
    let s = Stack::new("v1-counted").await;
    let mut legacy = vec![];
    for (i, c) in LEGACY.iter().take(3).enumerate() {
        legacy.push(s.legacy(&format!("L{i}"), c).await);
    }
    let bad = s.legacy("Zebra unreadable", LEGACY[2]).await;
    make_undecryptable(&s, &bad).await;
    let before = at_rest(&s, &bad).await;
    s.events.take();

    let run = run_unlock_migration(&s.svc).await.expect("run");
    assert!(matches!(run, MigrationRun::Completed { .. }), "an undecryptable row does not interrupt the run: {run:?}");
    let notice = notice_of(&run).expect("a notice");
    assert_eq!(
        notice,
        MigrationNotice { converted: 3, not_converted: 1 },
        "3 converted, 1 could not be read (V1)"
    );
    let text = notice.text();
    assert!(text.to_lowercase().contains("could not be read"), "the notice says it could not be read: {text}");
    assert!(!text.contains("Zebra"), "the notice names no document: {text}");

    for id in &legacy {
        assert!(is_v2(&s.stored(id).await.1), "the readable ones are converted");
    }
    assert_eq!(at_rest(&s, &bad).await, before, "nothing about the undecryptable row is changed (V1)");
    assert!(s.raw.list_document_commits(&bad).await.unwrap_or_default().is_empty(), "no version made of it");
    assert!(s.events.peek().iter().all(|e| e.id != bad), "no event about it: {:?}", s.events.peek());
    assert!(unlock_migration_complete(&s.svc).await.unwrap(), "the run completed");
}

/// V1 with M2 revised: a store whose only problem is one undecryptable row
/// still gets a notice, through the seam, saying one could not be read.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v1_an_undecryptable_row_alone_still_gets_a_notice() {
    let s = Stack::new("v1-alone").await;
    s.create_text("Fine", "already v2").await;
    let bad = s.legacy("Bad", LEGACY[0]).await;
    make_undecryptable(&s, &bad).await;

    let (task, rx) = start_captured(&s.svc);
    task.expect("task").await;
    assert_eq!(
        rx.try_recv().ok(),
        Some(MigrationNotice { converted: 0, not_converted: 1 }),
        "N = 0, M = 1: a notice (V1, M2 revised)"
    );
}

/// V1: a trashed row that cannot be decrypted is counted too (the conversion
/// covers trashed documents, G2), and stays as it was.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v1_a_trashed_undecryptable_row_is_counted_too() {
    let s = Stack::new("v1-trashed").await;
    s.legacy("Ok", LEGACY[2]).await;
    let bad = s.legacy("Bad, trashed", LEGACY[2]).await;
    s.svc.trash(&bad).await.expect("trash");
    make_undecryptable(&s, &bad).await;
    let before = at_rest(&s, &bad).await;

    let run = run_unlock_migration(&s.svc).await.expect("run");
    assert_eq!(notice_of(&run), Some(MigrationNotice { converted: 1, not_converted: 1 }));
    assert_eq!(at_rest(&s, &bad).await, before, "left as it was");
}

// ---------------------------------------------------------------------- V4 --

/// Legacy rows in the slowed run: enough that a run lasts several times a
/// single document's conversion.
const SLOW_DOCS: usize = 24;

/// Every store call waits this long while the conversion runs.
const PER_CALL: Duration = Duration::from_millis(8);

/// A store with [`SLOW_DOCS`] legacy rows and two documents of format v2
/// (`targets`: what a save or a trash acts on, never converted), and the
/// conversion started through the host seam on a spawned task, every store
/// call slowed; returns once the run has converted its first document.
async fn slowed_run(tag: &str) -> (Arc<Stack>, tokio::task::JoinHandle<()>, [String; 2]) {
    let s = Arc::new(Stack::new(tag).await);
    let targets = [s.create_text("Target A", "a").await, s.create_text("Target B", "b").await];
    for i in 0..SLOW_DOCS {
        s.legacy(&format!("S{i}"), LEGACY[i % LEGACY.len()]).await;
    }
    s.events.take();
    s.faulty.slow_calls(PER_CALL);
    let handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>> = Arc::new(Mutex::new(None));
    let keep = handle.clone();
    start_unlock_migration(s.svc.clone(), move |task| *keep.lock().unwrap() = Some(tokio::spawn(task)), |_| {});
    let h = handle.lock().unwrap().take().expect("spawned");
    // Mid-run: once the first document is converted.
    let started = std::time::Instant::now();
    while !s.events.peek().iter().any(|e| e.kind == ChangeKind::Migrated) {
        assert!(started.elapsed() < Duration::from_secs(120), "the conversion never converted a document");
        assert!(!h.is_finished(), "the run ended before converting anything");
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    (s, h, targets)
}

/// What a V4 test checks once its write returned: the conversion had not
/// ended when the write completed — its event comes before the run's last
/// conversion — and the run still completes, converting every document.
async fn assert_completed_mid_run(s: &Stack, h: tokio::task::JoinHandle<()>, id: &str, kind: ChangeKind, what: &str) {
    let converted_when_done = s.events.peek().iter().filter(|e| e.kind == ChangeKind::Migrated).count();
    s.faulty.fast_calls();
    tokio::time::timeout(Duration::from_secs(120), h).await.expect("bounded").expect("the conversion task");
    let events = s.events.peek();
    let write_at = events
        .iter()
        .position(|e| e.id == id && e.kind == kind)
        .unwrap_or_else(|| panic!("the {what} emitted {kind:?}: {events:?}"));
    let last_conversion = events.iter().rposition(|e| e.kind == ChangeKind::Migrated).expect("conversions");
    assert!(
        converted_when_done < SLOW_DOCS && write_at < last_conversion,
        "V4: a {what} issued mid-run completes before the run ends; it completed after {converted_when_done} of \
         {SLOW_DOCS} conversions (the write lock is held for the whole run: review finding 11)"
    );
    assert_eq!(
        events.iter().filter(|e| e.kind == ChangeKind::Migrated).count(),
        SLOW_DOCS,
        "the run still converts every document"
    );
    assert!(unlock_migration_complete(&s.svc).await.unwrap(), "and completes");
}

/// V4: "A save … issued during the conversion waits at most for one
/// document's conversion. … a save issued mid-run completes before the run
/// ends."
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn v4_a_save_issued_mid_run_completes_before_the_run_ends() {
    let (s, h, [target, _]) = slowed_run("v4-save").await;
    tokio::time::timeout(Duration::from_secs(120), s.assistant_save(&target, "saved mid-run"))
        .await
        .expect("bounded")
        .expect("the save");
    assert_completed_mid_run(&s, h, &target, ChangeKind::Saved, "save").await;
    assert_eq!(texts(&s.stored(&target).await.1), vec!["saved mid-run".to_string()], "the save is kept");
}

/// V4: "… a trash …".
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn v4_a_trash_issued_mid_run_completes_before_the_run_ends() {
    let (s, h, [_, target]) = slowed_run("v4-trash").await;
    tokio::time::timeout(Duration::from_secs(120), s.svc.trash(&target))
        .await
        .expect("bounded")
        .expect("the trash");
    assert_completed_mid_run(&s, h, &target, ChangeKind::Trashed, "trash").await;
    assert!(s.svc.read(&target).await.unwrap().trashed_at.is_some(), "in the trash");
}

/// V4: "… or a create …".
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn v4_a_create_issued_mid_run_completes_before_the_run_ends() {
    let (s, h, _) = slowed_run("v4-create").await;
    let created = tokio::time::timeout(Duration::from_secs(120), s.svc.create(new_text_doc("New", "created mid-run")))
        .await
        .expect("bounded")
        .expect("the create")
        .id;
    assert_completed_mid_run(&s, h, &created, ChangeKind::Created, "create").await;
    assert_eq!(texts(&s.stored(&created).await.1), vec!["created mid-run".to_string()]);
}

// ---------------------------------------------------------------------- V6 --

/// V6: "a row whose content does not parse stays read-only and is counted as
/// today" — beside an undecryptable row (V1), each counted once: the notice's
/// "could not be read" is both. The unparseable row is left exactly as it is
/// and still reads as read-only (unreadable); the undecryptable one is left
/// as it was.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v6_an_unparseable_row_stays_read_only_and_counted_beside_an_undecryptable_one() {
    let s = Stack::new("v6-both").await;
    s.legacy("L0", LEGACY[0]).await;
    s.legacy("L1", LEGACY[2]).await;
    let unparseable = s.legacy("Unparseable", UNPARSEABLE).await;
    let undecryptable = s.legacy("Undecryptable", LEGACY[2]).await;
    make_undecryptable(&s, &undecryptable).await;
    let before = at_rest(&s, &undecryptable).await;

    let run = run_unlock_migration(&s.svc).await.expect("run");
    assert_eq!(
        notice_of(&run),
        Some(MigrationNotice { converted: 2, not_converted: 2 }),
        "the unparseable row counted as today, the undecryptable one beside it"
    );
    assert_eq!(s.stored(&unparseable).await.1, UNPARSEABLE, "left exactly as it is");
    assert!(s.commits(&unparseable).await.is_empty(), "no version made of it");
    let view = s.svc.read(&unparseable).await.expect("it still reads");
    assert!(
        matches!(view.writable, Writable::ReadOnly(ReadOnlyReason::Unreadable { .. })),
        "still read-only: {:?}",
        view.writable
    );
    assert_eq!(at_rest(&s, &undecryptable).await, before, "the undecryptable row as it was");
}
