//! C-DOC adoption step 6 — the one-time migration at unlock
//! (`doc/plans/C-DOC-adoption.md`, *Step 6 — the one-time migration and the
//! end of the tolerant read: contract*, M1, M2, M3, M5). Written before the
//! code, by another author: whoever implements step 6 makes these pass
//! without editing this file.
//!
//! - **M1 — Once per persona, at the first unlock**, in the background: the
//!   seam returns at once having touched nothing; a per-store record says it
//!   completed; later unlocks do not run it again; another store (another
//!   persona) has its own record; a write racing it converts its document
//!   once.
//! - **M2 — The notice** (revised 2026-10-18, "Settled after the test
//!   author's report"): N ≥ 1 → a notice with N that points to History;
//!   M ≥ 1 alone → "M documents could not be read and were left as they
//!   are"; N = 0 and M = 0 → none. Wording **[choice]**: asserted loosely.
//! - **M2b — The notice is not lost**: the record keeps the counts and
//!   whether the notice was shown; one not shown (the process died in
//!   between) is shown at the next unlock, once.
//! - **M3 — An interruption resumes**: a crash anywhere (the `FaultyDb`
//!   budget), a transient store failure (`fail_once_after`), a lock midway (the
//!   task aborted) leave every document converted or untouched; the next
//!   unlock finishes and its notice counts the documents converted overall;
//!   an unreadable document is left as it is and counted as not converted.
//! - **M5 — The duress persona is indistinguishable**, as far as a store can
//!   show it: the decoy's rows seeded raw (`sovereign-app/src/duress.rs`
//!   writes plaintext rows into the raw store) convert exactly as a real
//!   persona's legacy rows do, with the same notice and the same store work
//!   in the same order. On-disk traces: `sovereign-app/tests/step6_duress_disk.rs`.
//!
//! Every stack is the real one (`EncryptedGraphDB` over `FaultyDb` over an
//! in-memory SurrealDB, `tests/common`); stored content is checked with
//! `serde_json`.

mod common;

use std::sync::{mpsc, Arc, Mutex};

use chrono::{TimeZone, Utc};
use common::*;
use serde_json::Value;
use sovereign_db::schema::Document;
use sovereign_docs::{
    run_unlock_migration, start_unlock_migration, unlock_migration_complete, unlock_migration_notice_shown,
    unlock_migration_pending_notice, DocError, MigrationNotice, MigrationRun, MigrationTask,
};

/// Legacy stored forms (G2): a v1 envelope, one with media, raw markdown, the
/// empty string.
const LEGACY: [&str; 4] = [
    r#"{"body":"envelope body","images":[]}"#,
    r#"{"body":"with media","images":[{"path":"/p/a.png","caption":"A"}]}"#,
    "# raw markdown\n\nbody",
    "",
];

/// Content that claims to be a Sovereign document but cannot be read (M3: "a
/// document it cannot convert").
const UNREADABLE: &str = r#"{"format":"sovereign.doc","v":2,"blocks":"not a list"}"#;
const UNREADABLE_2: &str = r#"{"format":"sovereign.doc","v":"two","blocks":[]}"#;

// ----------------------------------------------------------------- helpers --

fn is_v2(stored: &str) -> bool {
    serde_json::from_str::<Value>(stored)
        .map(|v| v["format"] == "sovereign.doc" && v["v"] == 2 && v["blocks"].is_array())
        .unwrap_or(false)
}

/// How many versions of `id` hold exactly `legacy` as their content.
async fn legacy_versions(s: &Stack, id: &str, legacy: &str) -> usize {
    s.committed_states(id).await.into_iter().filter(|(_, c)| c == legacy).count()
}

/// A row as the decoy seeding writes it (`sovereign-app/src/duress.rs`):
/// plaintext, straight into the persona's **raw** store, no keys, no nonces.
async fn decoy_raw(s: &Stack, title: &str, content: &str) -> String {
    let mut d = Document::new(title.to_string(), "thread:t1".to_string(), true);
    d.content = content.to_string();
    d.created_at = Utc.with_ymd_and_hms(2025, 3, 1, 9, 0, 0).unwrap();
    d.modified_at = Utc.with_ymd_and_hms(2025, 3, 2, 9, 0, 0).unwrap();
    s.raw.create_document(d).await.unwrap().id_string().unwrap()
}

fn notice_of(run: &MigrationRun) -> Option<MigrationNotice> {
    match run {
        MigrationRun::Completed { notice } => notice.clone(),
        _ => None,
    }
}

fn completed(run: &Result<MigrationRun, DocError>) -> bool {
    matches!(run, Ok(MigrationRun::Completed { .. }))
}

/// The seam with a spawner that only keeps the task: what a host's unlock
/// does, minus the runtime.
fn start_captured(svc: &Arc<sovereign_docs::DocumentService>) -> (Option<MigrationTask>, mpsc::Receiver<MigrationNotice>) {
    let slot: Arc<Mutex<Option<MigrationTask>>> = Arc::new(Mutex::new(None));
    let (tx, rx) = mpsc::channel();
    let keep = slot.clone();
    start_unlock_migration(
        svc.clone(),
        move |task| {
            *keep.lock().unwrap() = Some(task);
        },
        move |notice| {
            let _ = tx.send(notice);
        },
    );
    let task = slot.lock().unwrap().take();
    (task, rx)
}

/// Lowercased, for loose wording checks.
fn low(s: &str) -> String {
    s.to_lowercase()
}

// ---------------------------------------------------------------------- M1 --

/// The first run converts every legacy document, leaves a document created by
/// the service alone, and records completion in the store.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m1_the_first_run_converts_every_legacy_document_and_records_completion() {
    let s = Stack::new("m1-first").await;
    let mut ids = vec![];
    for (i, c) in LEGACY.iter().enumerate() {
        ids.push((s.legacy(&format!("L{i}"), c).await, *c));
    }
    let native = s.create_text("Native", "v2 from birth").await;
    let native_commits = s.commits(&native).await.len();
    assert!(!unlock_migration_complete(&s.svc).await.unwrap(), "nothing recorded before the first unlock");

    let run = run_unlock_migration(&s.svc).await.expect("run");
    let notice = notice_of(&run).expect("N >= 1: a notice");
    assert_eq!(notice.converted, LEGACY.len(), "every legacy document, and only those");
    assert_eq!(notice.not_converted, 0);
    for (id, legacy) in &ids {
        assert!(is_v2(&s.stored(id).await.1), "{legacy:?} converted");
        assert_eq!(legacy_versions(&s, id, legacy).await, 1, "its earlier state is in its History, once");
    }
    assert_eq!(s.commits(&native).await.len(), native_commits, "a v2 document is not touched");
    assert!(unlock_migration_complete(&s.svc).await.unwrap(), "the store records completion");
}

/// Later unlocks do not run it again: a new service over the same store reads
/// the record; a legacy row that appears afterwards (received in v1 or raw
/// form, G2) is left for its next write, and nothing is emitted.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m1_a_later_unlock_does_not_run_it_again() {
    let mut s = Stack::new("m1-later").await;
    s.legacy("Old", LEGACY[0]).await;
    // The first unlock, through the seam: it completes and shows its notice.
    let (task, rx) = start_captured(&s.svc);
    task.expect("task").await;
    assert!(rx.try_recv().is_ok(), "the first unlock's notice");
    assert!(unlock_migration_complete(&s.svc).await.unwrap());

    s.restart(); // a later unlock: a new service over the same store
    assert!(unlock_migration_complete(&s.svc).await.unwrap(), "the record is in the store, not in the service");
    let late = s.legacy("Arrived later", LEGACY[2]).await;
    s.events.take();
    let run = run_unlock_migration(&s.svc).await.expect("run");
    assert_eq!(run, MigrationRun::AlreadyComplete);
    assert_eq!(s.stored(&late).await.1, LEGACY[2], "not converted by a later unlock");
    assert!(s.commits(&late).await.is_empty(), "no version made");
    assert!(s.events.peek().is_empty(), "a run that does not run emits nothing");

    // Through the seam too: no notice.
    let (task, rx) = start_captured(&s.svc);
    task.expect("a task, even when there is nothing to do").await;
    assert!(rx.try_recv().is_err(), "no notice from a later unlock");
}

/// Once **per persona**: each persona's store has its own record. Completing
/// in one store says nothing about another on the same device.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m1_the_record_is_per_store() {
    let a = Stack::new("m1-store-a").await;
    let b = Stack::new("m1-store-b").await;
    a.legacy("A", LEGACY[0]).await;
    b.legacy("B1", LEGACY[0]).await;
    b.legacy("B2", LEGACY[2]).await;

    assert!(completed(&run_unlock_migration(&a.svc).await));
    assert!(unlock_migration_complete(&a.svc).await.unwrap());
    assert!(!unlock_migration_complete(&b.svc).await.unwrap(), "the other store has not run it");

    let run = run_unlock_migration(&b.svc).await.expect("run");
    assert_eq!(notice_of(&run).map(|n| n.converted), Some(2), "it runs at that store's own first unlock");
}

/// In the background: the seam returns at once having made no store call and
/// sent no notice; the work is the task it hands to the host's spawner.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m1_the_seam_returns_at_once_and_touches_nothing() {
    let s = Stack::new("m1-seam").await;
    let mut ids = vec![];
    for (i, c) in LEGACY.iter().enumerate() {
        ids.push(s.legacy(&format!("L{i}"), c).await);
    }
    let before = s.faulty.calls().len();

    let (task, rx) = start_captured(&s.svc);
    assert_eq!(s.faulty.calls().len(), before, "the unlock waits for nothing: no store call before the task runs");
    assert!(rx.try_recv().is_err(), "no notice before the task runs");
    let task = task.expect("the work is handed to the host's spawner");
    for (id, c) in ids.iter().zip(LEGACY) {
        assert_eq!(s.stored(id).await.1, c, "nothing converted before the task runs");
    }

    task.await;
    let notice = rx.try_recv().expect("the notice, once the task completes");
    assert_eq!(notice.converted, LEGACY.len());
    for id in &ids {
        assert!(is_v2(&s.stored(id).await.1));
    }
    assert!(unlock_migration_complete(&s.svc).await.unwrap());
}

/// "A document written before it reaches it is converted by that write (G3),
/// not twice": saves race the running migration; every document ends in v2
/// with the saved text, and its legacy state is in its History exactly once.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn m1_a_write_racing_the_migration_converts_the_document_once() {
    const DOCS: usize = 6;
    for round in 0..8 {
        let s = Arc::new(Stack::new(&format!("m1-race-{round}")).await);
        let mut ids = vec![];
        for i in 0..DOCS {
            let legacy = LEGACY[i % LEGACY.len()];
            ids.push((s.legacy(&format!("R{i}"), legacy).await, legacy));
        }
        let handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>> = Arc::new(Mutex::new(None));
        let keep = handle.clone();
        let (tx, rx) = mpsc::channel();
        start_unlock_migration(
            s.svc.clone(),
            move |task| {
                *keep.lock().unwrap() = Some(tokio::spawn(task));
            },
            move |n| {
                let _ = tx.send(n);
            },
        );
        // Writes, in the opposite order, while the migration runs.
        for (i, (id, _)) in ids.iter().enumerate().rev() {
            let text = format!("saved {i}");
            let mut ok = false;
            for _ in 0..5 {
                match s.assistant_save(id, &text).await {
                    Ok(_) => {
                        ok = true;
                        break;
                    }
                    Err(DocError::Conflict { .. }) => continue, // the migration moved it: read again
                    Err(e) => panic!("round {round}: save failed: {e}"),
                }
            }
            assert!(ok, "round {round}: the save went through");
        }
        let h = handle.lock().unwrap().take().expect("spawned");
        tokio::time::timeout(std::time::Duration::from_secs(60), h).await.expect("bounded").expect("task");

        for (i, (id, legacy)) in ids.iter().enumerate() {
            let stored = s.stored(id).await.1;
            assert!(is_v2(&stored), "round {round}: R{i} is v2");
            assert_eq!(texts(&stored), vec![format!("saved {i}")], "round {round}: the write is kept");
            assert_eq!(
                legacy_versions(&s, id, legacy).await,
                1,
                "round {round}: R{i}'s legacy state is in its History once, not twice"
            );
        }
        assert!(unlock_migration_complete(&s.svc).await.unwrap(), "round {round}");
        let notices: Vec<_> = rx.try_iter().collect();
        assert!(notices.len() <= 1, "round {round}: one notice at most");
        if let Some(n) = notices.first() {
            assert!(n.converted <= DOCS, "round {round}: a document converted by its write is not counted twice");
        }
    }
}

// ---------------------------------------------------------------------- M2 --

/// The notice carries the number and points to History (wording [choice]).
#[test]
fn m2_the_notice_says_how_many_and_points_to_history() {
    for n in [1usize, 7, 1234] {
        let text = MigrationNotice { converted: n, not_converted: 0 }.text();
        assert!(text.contains(&n.to_string()), "{n} in the notice");
        assert!(text.contains("History"), "the notice points to History");
    }
}

/// N = 0 (a fresh install, or nothing left to convert): no notice.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m2_nothing_converted_gives_no_notice() {
    // A fresh store.
    let fresh = Stack::new("m2-fresh").await;
    assert_eq!(run_unlock_migration(&fresh.svc).await.unwrap(), MigrationRun::Completed { notice: None });
    assert!(unlock_migration_complete(&fresh.svc).await.unwrap(), "completed, though there was nothing to do");

    // A store whose documents are all v2 already, through the seam.
    let v2only = Stack::new("m2-v2only").await;
    v2only.create_text("A", "a").await;
    v2only.create_text("B", "b").await;
    let (task, rx) = start_captured(&v2only.svc);
    task.expect("task").await;
    assert!(rx.try_recv().is_err(), "N = 0: notify is never called");
    assert!(unlock_migration_complete(&v2only.svc).await.unwrap());
}

/// The notice names no document: no title, no text.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m2_the_notice_names_no_document() {
    let s = Stack::new("m2-no-names").await;
    s.legacy("Quarterly zebra plan", r#"{"body":"marmalade-secret-17","images":[]}"#).await;
    s.legacy("Otter diary", "pumpernickel-secret-23").await;
    let run = run_unlock_migration(&s.svc).await.unwrap();
    let text = notice_of(&run).expect("notice").text();
    for word in ["zebra", "Otter", "marmalade", "pumpernickel"] {
        assert!(!text.contains(word), "the notice names no document ({word})");
    }
}

/// M2 revised: documents that could not be read are told even when nothing
/// was converted.
#[test]
fn m2_unreadable_documents_alone_still_give_a_notice_text() {
    let text = MigrationNotice { converted: 0, not_converted: 3 }.text();
    assert!(text.contains('3'), "M in the notice");
    assert!(low(&text).contains("read"), "that they could not be read");
    assert!(!text.contains("0 documents converted"), "no \"0 documents converted\"");
}

/// M2 revised, end to end: a store whose only legacy content is unreadable
/// completes with a notice of M, through the seam.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m2_a_store_with_only_unreadable_documents_gets_a_notice() {
    let s = Stack::new("m2-only-unreadable").await;
    s.create_text("Fine", "already v2").await;
    let bad = s.legacy("Bad", UNREADABLE).await;
    s.legacy("Bad 2", UNREADABLE_2).await;
    let (task, rx) = start_captured(&s.svc);
    task.expect("task").await;
    assert_eq!(
        rx.try_recv().ok(),
        Some(MigrationNotice { converted: 0, not_converted: 2 }),
        "N = 0, M = 2: a notice"
    );
    assert_eq!(s.stored(&bad).await.1, UNREADABLE, "left as it is");
    assert!(unlock_migration_complete(&s.svc).await.unwrap());
}

// --------------------------------------------------------------------- M2b --

/// The record keeps the counts and whether the notice was shown: a run that
/// completes leaves its notice pending in the store (a new service reads
/// it); marking it shown clears it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m2b_the_record_keeps_the_counts_until_the_notice_is_shown() {
    let mut s = Stack::new("m2b-record").await;
    for (i, c) in LEGACY.iter().take(3).enumerate() {
        s.legacy(&format!("L{i}"), c).await;
    }
    s.legacy("Bad", UNREADABLE).await;
    assert_eq!(unlock_migration_pending_notice(&s.svc).await.unwrap(), None, "nothing before completion");

    let run = run_unlock_migration(&s.svc).await.expect("run");
    let want = MigrationNotice { converted: 3, not_converted: 1 };
    assert_eq!(notice_of(&run), Some(want.clone()));
    s.restart();
    assert_eq!(
        unlock_migration_pending_notice(&s.svc).await.unwrap(),
        Some(want),
        "the counts are in the store's record, not yet shown"
    );
    unlock_migration_notice_shown(&s.svc).await.expect("mark shown");
    s.restart();
    assert_eq!(unlock_migration_pending_notice(&s.svc).await.unwrap(), None, "shown: nothing pending");
    assert!(unlock_migration_complete(&s.svc).await.unwrap(), "still complete");
}

/// A notice not shown (the process died between completing and showing it)
/// is shown at the next unlock, with the same counts, and only once.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m2b_a_notice_not_shown_is_shown_at_the_next_unlock_once() {
    let mut s = Stack::new("m2b-next").await;
    for (i, c) in LEGACY.iter().take(2).enumerate() {
        s.legacy(&format!("L{i}"), c).await;
    }
    // The first unlock completes, then the process dies before the notice.
    let run = run_unlock_migration(&s.svc).await.expect("run");
    assert!(completed(&Ok(run)));
    s.restart();

    let (task, rx) = start_captured(&s.svc);
    task.expect("task").await;
    assert_eq!(
        rx.try_recv().ok(),
        Some(MigrationNotice { converted: 2, not_converted: 0 }),
        "the pending notice, at the next unlock"
    );
    assert_eq!(unlock_migration_pending_notice(&s.svc).await.unwrap(), None, "recorded as shown");

    s.restart();
    let (task, rx) = start_captured(&s.svc);
    task.expect("task").await;
    assert!(rx.try_recv().is_err(), "shown once, not at every unlock");
}

/// A notice the seam shows is recorded as shown.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m2b_the_seam_records_its_notice_as_shown() {
    let s = Stack::new("m2b-seam").await;
    s.legacy("Old", LEGACY[2]).await;
    let (task, rx) = start_captured(&s.svc);
    task.expect("task").await;
    assert!(rx.try_recv().is_ok());
    assert_eq!(unlock_migration_pending_notice(&s.svc).await.unwrap(), None);
}

// ---------------------------------------------------------------------- M3 --

/// An unreadable document is left as it is and counted as not converted; it
/// does not hold the migration back, and later unlocks do not retry it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m3_an_unreadable_document_is_left_as_it_is_and_counted() {
    let s = Stack::new("m3-unreadable").await;
    for (i, c) in LEGACY.iter().take(3).enumerate() {
        s.legacy(&format!("L{i}"), c).await;
    }
    let bad = s.legacy("Bad", UNREADABLE).await;
    let bad2 = s.legacy("Bad 2", UNREADABLE_2).await;

    let run = run_unlock_migration(&s.svc).await.expect("run");
    let notice = notice_of(&run).expect("notice");
    assert_eq!(notice, MigrationNotice { converted: 3, not_converted: 2 });
    let text = notice.text();
    assert!(text.contains('3') && text.contains("History"), "N and History");
    assert!(text.contains('2') && low(&text).contains("read"), "M and that they could not be read");
    for (id, c) in [(&bad, UNREADABLE), (&bad2, UNREADABLE_2)] {
        assert_eq!(s.stored(id).await.1, c, "left exactly as it is");
        assert!(s.commits(id).await.is_empty(), "no version made of it");
    }
    assert!(unlock_migration_complete(&s.svc).await.unwrap(), "an unreadable document does not block completion");
    assert_eq!(run_unlock_migration(&s.svc).await.unwrap(), MigrationRun::AlreadyComplete);
}

/// After any interruption, each document is either converted (its legacy
/// state in its History) or byte for byte untouched.
///
/// Not asserted here: that the legacy state is versioned only **once** across
/// a crash and the resume. A crash between the version and the content write
/// leaves a version the row does not point to, and the resumed run versions
/// the same state again (measured with a throwaway implementation over
/// `migrate_all`, 2026-10-18; G4/G6 territory, reported, not a step-6
/// guarantee). The race test above asserts "not twice" for a write racing the
/// migration, which M1 does state.
async fn assert_converted_or_untouched(s: &Stack, ids: &[(String, &str)], k: i64) -> usize {
    let mut converted = 0;
    for (id, legacy) in ids {
        let stored = s.stored(id).await.1;
        if stored != *legacy {
            assert!(is_v2(&stored), "k={k}: neither converted nor untouched: {} bytes", stored.len());
            assert!(
                legacy_versions(s, id, legacy).await >= 1,
                "k={k}: converted, so its legacy state is in its History"
            );
            converted += 1;
        }
    }
    converted
}

/// A crash anywhere (every store call fails from the k-th on, as if the
/// process stopped there) leaves every document converted or untouched; a
/// run that did not complete has not recorded completion; the next unlock
/// finishes, and its notice counts the documents converted overall.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m3_a_crash_anywhere_resumes_at_the_next_unlock_with_the_overall_count() {
    let mut reached_the_end = false;
    for k in 0..1500 {
        let mut s = Stack::new(&format!("m3-crash-{k}")).await;
        let mut ids = vec![];
        for (i, c) in LEGACY.iter().take(3).enumerate() {
            ids.push((s.legacy(&format!("L{i}"), c).await, *c));
        }
        let bad = s.legacy("Bad", UNREADABLE).await;
        s.create_text("Native", "v2").await;

        s.faulty.crash_after(k);
        let first = run_unlock_migration(&s.svc).await;
        let crashed = s.faulty.crashed();
        s.restart();

        assert_converted_or_untouched(&s, &ids, k).await;
        assert_eq!(s.stored(&bad).await.1, UNREADABLE, "k={k}: the unreadable document is never changed");
        let recorded = unlock_migration_complete(&s.svc).await.unwrap();
        assert_eq!(recorded, completed(&first), "k={k}: completion is recorded exactly when a run completes");

        let second = run_unlock_migration(&s.svc).await.expect("the next unlock");
        if completed(&first) {
            assert_eq!(second, MigrationRun::AlreadyComplete, "k={k}");
        } else {
            assert_eq!(
                notice_of(&second),
                Some(MigrationNotice { converted: 3, not_converted: 1 }),
                "k={k}: the run that finishes counts the documents converted overall"
            );
        }
        assert_eq!(assert_converted_or_untouched(&s, &ids, k).await, ids.len(), "k={k}: all converted in the end");
        assert!(unlock_migration_complete(&s.svc).await.unwrap(), "k={k}");

        if !crashed {
            assert!(completed(&first), "k={k}: no crash, the run completes");
            assert_eq!(notice_of(first.as_ref().unwrap()), Some(MigrationNotice { converted: 3, not_converted: 1 }));
            reached_the_end = true;
            break;
        }
    }
    assert!(reached_the_end, "the crash budget never exceeded the run's store calls: raise the bound");
}

/// A transient store failure (one call fails, the next ones succeed) is an
/// interruption, not an unreadable document: the run does not complete, and
/// the next unlock converts what is left and counts everything.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m3_a_transient_failure_resumes_at_the_next_unlock() {
    let mut reached_the_end = false;
    for k in 0..1500 {
        let mut s = Stack::new(&format!("m3-once-{k}")).await;
        let mut ids = vec![];
        for (i, c) in LEGACY.iter().take(3).enumerate() {
            ids.push((s.legacy(&format!("L{i}"), c).await, *c));
        }

        s.faulty.fail_once_after(k);
        let first = run_unlock_migration(&s.svc).await;
        let failed = s.faulty.failed_once();
        s.restart();

        assert_converted_or_untouched(&s, &ids, k).await;
        if failed {
            assert!(!completed(&first), "k={k}: a store failure is not a completed run");
            assert!(!unlock_migration_complete(&s.svc).await.unwrap(), "k={k}: not recorded");
            let second = run_unlock_migration(&s.svc).await.expect("the next unlock");
            assert_eq!(
                notice_of(&second),
                Some(MigrationNotice { converted: 3, not_converted: 0 }),
                "k={k}: the failed document was not counted as unreadable; the overall count"
            );
            assert_eq!(assert_converted_or_untouched(&s, &ids, k).await, 3, "k={k}");
        } else {
            assert!(completed(&first), "k={k}");
            reached_the_end = true;
            break;
        }
    }
    assert!(reached_the_end, "the one-shot fault always fired: raise the bound");
}

/// Locking (or quitting) midway: the host drops the task wherever it is. The
/// next unlock finishes the rest; the notice, with the overall count, is
/// shown (M2b: never lost).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn m3_locking_midway_then_unlocking_finishes_the_rest() {
    const DOCS: usize = 5;
    for yields in 0..10usize {
        let mut s = Stack::new(&format!("m3-lock-{yields}")).await;
        let mut ids = vec![];
        for i in 0..DOCS {
            let legacy = LEGACY[i % LEGACY.len()];
            ids.push((s.legacy(&format!("K{i}"), legacy).await, legacy));
        }
        let (tx, rx) = mpsc::channel();

        let handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>> = Arc::new(Mutex::new(None));
        let keep = handle.clone();
        let tx1 = tx.clone();
        start_unlock_migration(
            s.svc.clone(),
            move |task| *keep.lock().unwrap() = Some(tokio::spawn(task)),
            move |n| {
                let _ = tx1.send(n);
            },
        );
        for _ in 0..yields {
            tokio::task::yield_now().await;
        }
        let h = handle.lock().unwrap().take().expect("spawned");
        h.abort(); // the lock
        let _ = tokio::time::timeout(std::time::Duration::from_secs(60), h).await.expect("bounded");
        s.restart();
        assert_converted_or_untouched(&s, &ids, yields as i64).await;

        // The next unlock.
        let keep2 = handle.clone();
        start_unlock_migration(
            s.svc.clone(),
            move |task| *keep2.lock().unwrap() = Some(tokio::spawn(task)),
            move |n| {
                let _ = tx.send(n);
            },
        );
        let h2 = handle.lock().unwrap().take().expect("spawned");
        tokio::time::timeout(std::time::Duration::from_secs(60), h2).await.expect("bounded").expect("task");

        assert_eq!(assert_converted_or_untouched(&s, &ids, yields as i64).await, DOCS, "yields={yields}");
        assert!(unlock_migration_complete(&s.svc).await.unwrap(), "yields={yields}");
        // M2b: never lost. A lock between showing the notice and recording it
        // shown may show it once more at the next unlock — never a third time,
        // never with another count.
        let notices: Vec<_> = rx.try_iter().collect();
        assert!(
            (1..=2).contains(&notices.len()),
            "yields={yields}: the notice is shown (once, or again after an interruption): {notices:?}"
        );
        assert!(
            notices.iter().all(|n| *n == MigrationNotice { converted: DOCS, not_converted: 0 }),
            "yields={yields}: counting every document converted: {notices:?}"
        );
        assert_eq!(unlock_migration_pending_notice(&s.svc).await.unwrap(), None, "yields={yields}");
    }
}

// ---------------------------------------------------------------------- M5 --

/// The decoy's documents, as `seed_duress_db` writes them (a sample).
const DECOY: [(&str, &str); 3] = [
    ("Grocery List", "- Milk\n- Eggs\n- Bread"),
    ("Weekend Plans", "Saturday:\n- Morning run at 8am"),
    ("Pasta Carbonara", "## Ingredients\n- 400g spaghetti"),
];

/// The decoy's rows seeded raw convert exactly as a real persona's legacy rows
/// of the same stored form do (a real persona also holds plaintext rows
/// written before login: seed data, v0.0.5 desktop state — `tauri_commands/
/// auth.rs`), and end sealed like every body the service writes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m5_raw_seeded_decoy_rows_convert_exactly_like_a_real_personas_legacy_rows() {
    let primary = Stack::new("m5-rows-primary").await;
    let decoy = Stack::new("m5-rows-decoy").await;
    let mut pairs = vec![];
    for (title, body) in DECOY {
        // The real persona: one plaintext row (pre-login state) and one sealed
        // legacy row of the same content.
        let p_raw = decoy_raw(&primary, title, body).await;
        let p_sealed = primary.legacy(title, body).await;
        let d_raw = decoy_raw(&decoy, title, body).await;
        pairs.push((p_raw, p_sealed, d_raw, body));
    }
    let before: Vec<_> = {
        let mut v = vec![];
        for (_, _, d, _) in &pairs {
            v.push(decoy.stored_doc(d).await);
        }
        v
    };

    let rp = run_unlock_migration(&primary.svc).await.expect("primary");
    let rd = run_unlock_migration(&decoy.svc).await.expect("decoy");
    assert_eq!(notice_of(&rd).map(|n| n.converted), Some(DECOY.len()), "the decoy's rows are converted, all of them");
    assert_eq!(notice_of(&rp).map(|n| n.converted), Some(2 * DECOY.len()));

    for ((p_raw, p_sealed, d_raw, body), was) in pairs.iter().zip(&before) {
        let d = decoy.stored(d_raw).await;
        let p = primary.stored(p_raw).await;
        assert!(is_v2(&d.1), "decoy row converted");
        assert_eq!(texts(&d.1), vec![body.to_string()], "its text kept");
        assert_eq!(texts(&d.1), texts(&p.1), "same result as the real persona's row");
        assert_eq!(d.0, p.0, "title kept");
        assert_eq!(legacy_versions(&decoy, d_raw, body).await, 1, "its earlier state is in its History");
        assert_eq!(
            decoy.commits(d_raw).await.len(),
            primary.commits(p_raw).await.len(),
            "as many versions as the real persona's row"
        );
        assert_eq!(decoy.commits(d_raw).await.len(), primary.commits(p_sealed).await.len());
        let dm: Vec<String> = decoy.commits(d_raw).await.into_iter().map(|c| c.message).collect();
        let pm: Vec<String> = primary.commits(p_raw).await.into_iter().map(|c| c.message).collect();
        assert_eq!(dm, pm, "the same version messages");

        // Nothing else changed: times, thread, ownership.
        let after = decoy.stored_doc(d_raw).await;
        assert_eq!(after.created_at, was.created_at);
        assert_eq!(after.modified_at, was.modified_at);
        assert_eq!(after.thread_id, was.thread_id);
        assert_eq!(after.is_owned, was.is_owned);

        // At rest: the body is sealed, as every body the service writes;
        // the title is in the same state as the real persona's raw row's.
        let d_rest = decoy.raw.get_document(d_raw).await.unwrap();
        let p_rest = primary.raw.get_document(p_raw).await.unwrap();
        assert!(d_rest.encryption_nonce.is_some(), "the decoy's body is sealed at rest after conversion");
        assert!(!d_rest.content.contains(body.lines().next().unwrap()), "no plaintext body at rest");
        assert_eq!(d_rest.title_nonce.is_some(), p_rest.title_nonce.is_some(), "titles at rest alike");
    }
}

/// Same behaviour, same notice, same timing class: the seam returns at once
/// for both personas, the task does the same store work in the same order,
/// and the notices are equal. (The primary is an upgraded workspace, its
/// legacy rows sealed; the decoy is seeded raw.)
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m5_primary_and_decoy_do_the_same_work_and_get_the_same_notice() {
    let primary = Stack::new("m5-work-primary").await;
    let decoy = Stack::new("m5-work-decoy").await;
    for (title, body) in DECOY {
        primary.legacy(title, body).await;
        decoy_raw(&decoy, title, body).await;
    }

    let mut calls = vec![];
    let mut notices = vec![];
    for s in [&primary, &decoy] {
        let before = s.faulty.calls().len();
        let (task, rx) = start_captured(&s.svc);
        assert_eq!(s.faulty.calls().len(), before, "the unlock waits for nothing, for either persona");
        task.expect("task").await;
        calls.push(s.faulty.calls()[before..].to_vec());
        notices.push(rx.try_recv().expect("notice"));
    }
    assert_eq!(notices[0], notices[1], "the same notice");
    assert_eq!(notices[0].text(), notices[1].text());
    assert_eq!(calls[0], calls[1], "the same store calls, in the same order");
}
