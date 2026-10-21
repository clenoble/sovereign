//! S5-10 — deleting a thread (C-DOC adoption step 5,
//! `doc/plans/C-DOC-adoption.md`, *What the answers change*).
//!
//! Spec ch. 03, *Thread deletion and the fallback lane*, decided 2026-10-17
//! (Céline): "Deleting a thread moves every document still in it to the trash
//! — not to the fallback lane — and deletes the thread: its id never comes
//! back, and nothing restores it. … The confirmation says that all the
//! thread's documents will be placed in the trash (with the thread's name and
//! its number of documents, ch. 08 AT5). The thread record survives only as
//! what sync needs to carry the deletion to paired devices, never shown."
//!
//! Plan, S5-10: "one `trash` each, so each carries its stamp and sync
//! tombstone". And all or nothing (Céline, 2026-10-17, *Three more answers*;
//! spec: "Deleting a thread is all or nothing: if one of its documents cannot
//! go to the trash, the others come back and the thread stays"), with "the
//! notice says nothing was deleted".
//!
//! The function under test is the host-agnostic `delete_thread_to_trash`
//! (`src/hosts.rs`, a step-5 stub), called by the assistant and the phone.
//! Their own paths are tested in `sovereign-ai` (orchestrator) and
//! `sovereign-app` (`step5_tests.rs`).

mod common;

use chrono::Duration;
use common::wire::current_stamp;
use common::*;
use sovereign_db::schema::Thread;
use sovereign_docs::{delete_thread_to_trash, preview_thread_delete};

async fn thread(s: &Stack, name: &str) -> String {
    s.edb.create_thread(Thread::new(name.into(), String::new())).await.unwrap().id_string().unwrap()
}

async fn doc_in(s: &Stack, thread_id: &str, title: &str, text: &str) -> String {
    let mut d = new_text_doc(title, text);
    d.thread_id = thread_id.to_string();
    s.svc.create(d).await.unwrap().id
}

async fn active_thread_ids(s: &Stack) -> Vec<String> {
    s.edb.list_threads().await.unwrap().into_iter().filter(|t| t.deleted_at.is_none()).filter_map(|t| t.id_string()).collect()
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_10_every_document_still_in_the_thread_goes_to_the_trash_then_the_thread_is_deleted() {
    let s = Stack::new("s5-10-delete").await;
    let drafts = thread(&s, "Old drafts").await;
    let keep = thread(&s, "Keep").await;
    let a = doc_in(&s, &drafts, "A", "alpha").await;
    let b = doc_in(&s, &drafts, "B", "beta").await;
    let earlier = doc_in(&s, &drafts, "Earlier", "trashed before").await;
    let other = doc_in(&s, &keep, "Other", "other").await;
    s.svc.trash(&earlier).await.unwrap();
    let earlier_trashed_at = s.svc.read(&earlier).await.unwrap().trashed_at;
    let earlier_stamp = current_stamp(&s, &earlier).await;
    let stamps_before = [current_stamp(&s, &a).await, current_stamp(&s, &b).await];
    s.clock.advance(Duration::minutes(5));

    let report = delete_thread_to_trash(&s.svc, s.edb.as_ref(), &drafts).await.expect("thread deleted");

    assert_eq!(sorted(report.trashed.clone()), sorted(vec![a.clone(), b.clone()]), "exactly the documents still in it");
    let trash: Vec<String> = s.svc.list_trash().await.unwrap().into_iter().map(|e| e.id).collect();
    assert_eq!(sorted(trash), sorted(vec![a.clone(), b.clone(), earlier.clone()]), "all of them in the trash");
    for (id, before) in [(&a, &stamps_before[0]), (&b, &stamps_before[1])] {
        let v = s.svc.read(id).await.unwrap();
        assert_eq!(v.trashed_at, Some(s.clock.get()), "{id} trashed by the service, now");
        assert!(current_stamp(&s, id).await > *before, "{id} carries a new stamp (its sync tombstone)");
        assert_eq!(s.stored_doc(id).await.thread_id, drafts, "{id} keeps its thread id");
    }
    // Already in the trash: left as it was.
    assert_eq!(s.svc.read(&earlier).await.unwrap().trashed_at, earlier_trashed_at);
    assert_eq!(current_stamp(&s, &earlier).await, earlier_stamp, "not written again");
    // The thread: not listed; its record kept only as a tombstone.
    assert_eq!(active_thread_ids(&s).await, vec![keep.clone()]);
    let all = s.edb.list_threads_including_deleted().await.unwrap();
    let row = all.iter().find(|t| t.id_string().as_deref() == Some(drafts.as_str()));
    assert!(row.is_some_and(|t| t.deleted_at.is_some()), "the thread record remains as the tombstone sync carries");
    // The other thread and its document are untouched.
    assert!(s.svc.read(&other).await.unwrap().trashed_at.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_10_an_empty_thread_is_deleted_and_nothing_is_trashed() {
    let s = Stack::new("s5-10-empty").await;
    let empty = thread(&s, "Empty").await;
    let keep = thread(&s, "Keep").await;
    let other = doc_in(&s, &keep, "Other", "other").await;
    let report = delete_thread_to_trash(&s.svc, s.edb.as_ref(), &empty).await.expect("deleted");
    assert!(report.trashed.is_empty());
    assert!(s.svc.list_trash().await.unwrap().is_empty());
    assert_eq!(active_thread_ids(&s).await, vec![keep]);
    assert!(s.svc.read(&other).await.unwrap().trashed_at.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_10_an_unknown_or_already_deleted_thread_is_refused_and_nothing_changes() {
    let s = Stack::new("s5-10-unknown").await;
    let lane = thread(&s, "Lane").await;
    let a = doc_in(&s, &lane, "A", "alpha").await;
    let err = delete_thread_to_trash(&s.svc, s.edb.as_ref(), "thread:nope").await.expect_err("unknown thread");
    assert!(!err.message.trim().is_empty(), "the notice says why");
    assert!(s.svc.list_trash().await.unwrap().is_empty());

    delete_thread_to_trash(&s.svc, s.edb.as_ref(), &lane).await.expect("first delete");
    let err = delete_thread_to_trash(&s.svc, s.edb.as_ref(), &lane).await.expect_err("already deleted");
    assert!(!err.message.trim().is_empty());
    assert_eq!(s.svc.list_trash().await.unwrap().len(), 1, "the trash is as the first delete left it");
    let _ = a;
}

/// A thread "Lane" with three documents, one more of it already in the trash
/// (`earlier`), and a document of another thread.
struct Sweep {
    s: Stack,
    lane: String,
    ids: [String; 3],
    earlier: String,
    other: String,
}

async fn sweep_world(tag: &str) -> Sweep {
    let s = Stack::new(tag).await;
    let lane = thread(&s, "Lane").await;
    let keep = thread(&s, "Keep").await;
    let ids = [
        doc_in(&s, &lane, "A", "alpha").await,
        doc_in(&s, &lane, "B", "beta").await,
        doc_in(&s, &lane, "C", "gamma").await,
    ];
    let earlier = doc_in(&s, &lane, "Earlier", "trashed before").await;
    s.svc.trash(&earlier).await.unwrap();
    let other = doc_in(&s, &keep, "Other", "other").await;
    s.clock.advance(Duration::minutes(5));
    Sweep { s, lane, ids, earlier, other }
}

async fn in_trash(w: &Sweep) -> Vec<String> {
    let trash = w.s.svc.list_trash().await.unwrap();
    w.ids.iter().filter(|id| trash.iter().any(|e| &e.id == *id)).cloned().collect()
}

/// All or nothing. One store call fails — at every point of the delete in
/// turn — and every call after it succeeds (a transient failure the delete
/// survives). Whenever the delete fails: the thread is still there, **no
/// document of it is in the trash** (those already moved came back), the one
/// that was in the trash before stays there as it was, the other thread is
/// untouched, and the notice says nothing was deleted. Whenever it succeeds,
/// the result is complete.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_10_a_thread_delete_is_all_or_nothing() {
    let mut completed = false;
    let mut failures = 0;
    for k in 0..400 {
        let w = sweep_world(&format!("s5-10-once-{k}")).await;
        let earlier_at = w.s.svc.read(&w.earlier).await.unwrap().trashed_at;
        w.s.faulty.fail_once_after(k);
        let result = delete_thread_to_trash(&w.s.svc, w.s.edb.as_ref(), &w.lane).await;
        let fired = w.s.faulty.failed_once();
        w.s.faulty.disarm();
        match result {
            Err(e) => {
                failures += 1;
                assert!(active_thread_ids(&w.s).await.contains(&w.lane), "k={k}: the thread stays");
                assert!(in_trash(&w).await.is_empty(), "k={k}: no document of it is in the trash");
                for id in &w.ids {
                    assert!(w.s.svc.list(None).await.unwrap().iter().any(|d| &d.id == id), "k={k}: {id} listed again");
                }
                assert!(e.message.to_lowercase().contains("nothing"), "k={k}: the notice says nothing was deleted: {:?}", e.message);
            }
            Ok(r) => {
                assert_eq!(r.trashed.len(), 3, "k={k}");
                assert_eq!(in_trash(&w).await.len(), 3, "k={k}");
                assert!(!active_thread_ids(&w.s).await.contains(&w.lane), "k={k}");
                if !fired {
                    completed = true;
                }
            }
        }
        assert_eq!(w.s.svc.read(&w.earlier).await.unwrap().trashed_at, earlier_at, "k={k}: already in the trash, as it was");
        assert!(w.s.svc.read(&w.other).await.unwrap().trashed_at.is_none(), "k={k}: the other thread untouched");
        if completed {
            break;
        }
    }
    assert!(completed, "the delete never ran without the fault firing");
    assert!(failures > 0, "the sweep never made the delete fail");
}

/// A crash — the store fails every call from some point on, so even putting
/// back what was already moved can be interrupted. *Reading of the test
/// author (Céline's answer covers a failure, not a crash during the undo):*
/// after any crash and a restart, the thread is never gone while a document
/// of it is out of the trash, and running the same delete again converges to
/// the complete result (all its documents in the trash, the thread deleted,
/// the one trashed before untouched).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_10_after_a_crash_a_rerun_converges() {
    let mut completed = false;
    for k in 0..400 {
        let mut w = sweep_world(&format!("s5-10-crash-{k}")).await;
        let earlier_at = w.s.svc.read(&w.earlier).await.unwrap().trashed_at;
        w.s.faulty.crash_after(k);
        let _ = delete_thread_to_trash(&w.s.svc, w.s.edb.as_ref(), &w.lane).await;
        let crashed = w.s.faulty.crashed();
        w.s.restart();
        if active_thread_ids(&w.s).await.contains(&w.lane) {
            delete_thread_to_trash(&w.s.svc, w.s.edb.as_ref(), &w.lane).await.expect("the same delete, run again");
        } else {
            assert_eq!(in_trash(&w).await.len(), 3, "k={k}: the thread is gone, so every document of it is in the trash");
        }
        assert_eq!(in_trash(&w).await.len(), 3, "k={k}: converged");
        assert!(!active_thread_ids(&w.s).await.contains(&w.lane), "k={k}: converged");
        assert_eq!(w.s.svc.read(&w.earlier).await.unwrap().trashed_at, earlier_at, "k={k}");
        assert!(w.s.svc.read(&w.other).await.unwrap().trashed_at.is_none(), "k={k}");
        if !crashed {
            completed = true;
            break;
        }
    }
    assert!(completed);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_10_the_confirmation_names_the_thread_its_documents_and_the_trash() {
    let s = Stack::new("s5-10-confirm").await;
    let lane = thread(&s, "Old drafts").await;
    for (i, t) in ["A", "B", "C"].iter().enumerate() {
        doc_in(&s, &lane, t, &format!("text {i}")).await;
    }
    let trashed = doc_in(&s, &lane, "Gone", "already trashed").await;
    s.svc.trash(&trashed).await.unwrap();

    let p = preview_thread_delete(&s.svc, s.edb.as_ref(), &lane).await.expect("preview");
    assert_eq!((p.thread_id.as_str(), p.name.as_str(), p.documents), (lane.as_str(), "Old drafts", 3));
    let text = p.confirmation_text();
    assert!(text.contains("Old drafts"), "names the thread: {text}");
    assert!(text.contains('3'), "says how many documents: {text}");
    assert!(text.to_lowercase().contains("trash"), "says they go to the trash: {text}");
    // A preview changes nothing.
    assert_eq!(s.svc.list_trash().await.unwrap().len(), 1);
    assert!(active_thread_ids(&s).await.contains(&lane));
}
