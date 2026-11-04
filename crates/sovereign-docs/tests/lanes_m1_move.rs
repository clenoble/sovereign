//! v0.0.12 lanes lot, item 15 — **M1** (one public move) and the restore into
//! a thread as **one** service operation (M2's restore half). Contract:
//! `doc/plans/V0012-lot-lanes.md`, *Item 15 — a move goes through the stamped
//! path, and travels*.
//!
//! Written before the code, by another author: whoever builds item 15 makes
//! these pass **without editing this file**. A test that looks wrong is
//! reported, not worked around.
//!
//! API this file names (not built when it was written; the names are the test
//! author's, the bodies the implementer's):
//! - `DocumentService::move_to_thread(&self, id: &str, thread_id: &str)
//!   -> Result<_, DocError>` — M1, the public move;
//! - `DocumentService::restore_from_trash_into(&self, id: &str, thread_id: &str)
//!   -> Result<_, DocError>` — out of the trash and into a thread, one
//!   stamped state under one lock (M2).
//!
//! The Ok values are not looked at: any type will do.
//!
//! The source scan (no production move around M1) and the host's restore are
//! in `lanes_m2_one_move_path.rs`; the two-device half (M3, M4) in
//! `sovereign-p2p/tests/wire_lanes_moves.rs` and `wire_lanes_pin.rs`.

mod common;

use chrono::Duration;
use common::wire::*;
use common::*;
use sovereign_docs::{delete_thread_to_trash, Stamp};

async fn thread(s: &Stack, name: &str) -> String {
    s.svc.thread_create(name, "").await.expect("thread created")
}

async fn doc_in(s: &Stack, thread_id: &str, title: &str, text: &str) -> String {
    let mut d = new_text_doc(title, text);
    d.thread_id = thread_id.to_string();
    s.svc.create(d).await.expect("create").id
}

fn ms(s: &Stack) -> i64 {
    s.clock.get().timestamp_millis()
}

/// Everything a refused move must leave as it was.
#[derive(Debug, PartialEq)]
struct Snapshot {
    thread_id: String,
    trashed: bool,
    stored: (String, String),
    stamp: Stamp,
    versions: usize,
}

async fn snapshot(s: &Stack, id: &str) -> Snapshot {
    let st = s.svc.stored_state(id).await.expect("stored state");
    Snapshot {
        thread_id: st.document.thread_id.clone(),
        trashed: st.document.deleted_at.is_some(),
        stored: s.stored(id).await,
        stamp: st.meta.stamp.expect("a stamp"),
        versions: s.svc.versions(id).await.map(|v| v.len()).unwrap_or(0),
    }
}

// ------------------------------------------------------------------- M1 --

/// M1: "under the write lock and stamped as `move_locked` is: the thread
/// changes, title, content and versions do not; it emits the change."
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m1_a_move_changes_the_thread_alone_is_stamped_and_emits_the_change() {
    let s = Stack::new("m1-move").await;
    let from = thread(&s, "Garden").await;
    let to = thread(&s, "Kitchen").await;
    let id = doc_in(&s, &from, "Seeds", "tomatoes and beans").await;
    let before = s.svc.read(&id).await.unwrap();
    let stored_before = s.stored(&id).await;
    let versions = s.svc.versions(&id).await.unwrap().len();
    let st0 = current_stamp(&s, &id).await;

    s.clock.advance(Duration::minutes(1));
    s.events.take();
    s.svc.move_to_thread(&id, &to).await.expect("moved");

    let after = s.svc.read(&id).await.unwrap();
    assert_eq!(after.thread_id, to, "the thread changed");
    assert_eq!(after.title, before.title, "the title did not");
    assert_eq!(after.plain_text(), before.plain_text(), "the text did not");
    assert_eq!(s.stored(&id).await, stored_before, "title and content as stored did not");
    assert_eq!(s.svc.versions(&id).await.unwrap().len(), versions, "no version for a move");
    assert!(after.trashed_at.is_none());

    let st = s.svc.stored_state(&id).await.unwrap();
    let st1 = st.meta.stamp.clone().expect("a stamp");
    assert!(st1 > st0, "a new, later stamp: {st0:?} -> {st1:?}");
    assert_eq!((st1.wall_ms, st1.device.as_str()), (ms(&s), DEVICE), "from this device's clock");
    assert_eq!(st.meta.prev_stamp, Some(st0), "it replaces the state it moved");
    assert_eq!(st.document.thread_id, to, "the state the wire sends carries the new thread");
    assert!(st.stamped, "recorded by a write, not derived");

    let evs = s.events.take();
    assert!(evs.iter().any(|e| e.id == id), "the change is emitted: {evs:?}");
}

/// M1: "It refuses, with its reason: a document in the trash or purged; a
/// thread that does not exist or is deleted; a move to the thread it is
/// already in, which changes nothing (no stamp)." Nothing changes in any case.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m1_a_refused_move_says_why_and_changes_nothing() {
    let s = Stack::new("m1-refused").await;
    let home = thread(&s, "Home").await;
    let other = thread(&s, "Other").await;
    let gone = thread(&s, "Gone").await;
    let id = doc_in(&s, &home, "Note", "text").await;
    let left = doc_in(&s, &gone, "Left", "in a thread deleted later").await;
    delete_thread_to_trash(&s.svc, s.edb.as_ref(), &gone).await.expect("thread deleted");
    let _ = left;
    s.clock.advance(Duration::minutes(1));

    let mut reasons: Vec<(&str, String)> = vec![];

    // A thread that does not exist.
    let snap = snapshot(&s, &id).await;
    let e = s.svc.move_to_thread(&id, "thread:nowhere").await.expect_err("an unknown thread is refused");
    assert_eq!(snapshot(&s, &id).await, snap, "unknown thread: nothing changed");
    reasons.push(("unknown thread", e.to_string()));

    // A deleted thread.
    let e = s.svc.move_to_thread(&id, &gone).await.expect_err("a deleted thread is refused");
    assert_eq!(snapshot(&s, &id).await, snap, "deleted thread: nothing changed");
    reasons.push(("deleted thread", e.to_string()));

    // The thread it is already in: no stamp.
    let e = s.svc.move_to_thread(&id, &home).await.expect_err("a move to its own thread is refused");
    assert_eq!(snapshot(&s, &id).await, snap, "same thread: nothing changed, no stamp");
    reasons.push(("same thread", e.to_string()));

    // A document in the trash.
    s.svc.trash(&id).await.unwrap();
    let snap = snapshot(&s, &id).await;
    let e = s.svc.move_to_thread(&id, &other).await.expect_err("a document in the trash is refused");
    assert_eq!(snapshot(&s, &id).await, snap, "in the trash: nothing changed");
    assert!(s.svc.read(&id).await.unwrap().trashed_at.is_some(), "still in the trash");
    reasons.push(("in the trash", e.to_string()));

    // A purged document.
    s.svc.purge_from_trash(&id).await.expect("purged");
    let e = s.svc.move_to_thread(&id, &other).await.expect_err("a purged document is refused");
    reasons.push(("purged", e.to_string()));
    assert!(s.svc.read(&id).await.is_err(), "still purged");

    for (what, r) in &reasons {
        assert!(!r.trim().is_empty(), "{what}: the refusal says why");
    }
    // "with its reason": the unknown thread, the same thread and the trash are
    // three different reasons, and say so differently.
    let pick = |w: &str| reasons.iter().find(|(x, _)| *x == w).unwrap().1.clone();
    let three = [pick("unknown thread"), pick("same thread"), pick("in the trash")];
    assert!(
        three[0] != three[1] && three[1] != three[2] && three[0] != three[2],
        "each refusal gives its own reason: {three:?}"
    );
}

/// M1: "under the write lock": two moves of one document at once are one
/// after the other — the second replaces the state the first wrote (its
/// previous stamp is the first's stamp), never both the same old state.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn m1_two_moves_at_once_are_serialised_by_the_write_lock() {
    let s = Stack::new("m1-lock").await;
    let home = thread(&s, "Home").await;
    let b = thread(&s, "B").await;
    let c = thread(&s, "C").await;
    let id = doc_in(&s, &home, "Note", "text").await;
    let st0 = current_stamp(&s, &id).await;
    let (r1, r2) = tokio::join!(s.svc.move_to_thread(&id, &b), s.svc.move_to_thread(&id, &c));
    assert!(r1.is_ok() && r2.is_ok(), "both moves are made: {:?} / {:?}", r1.map(|_| ()), r2.map(|_| ()));
    let st = s.svc.stored_state(&id).await.unwrap();
    let last: Stamp = st.meta.stamp.clone().expect("a stamp");
    let prev: Stamp = st.meta.prev_stamp.clone().expect("a previous stamp");
    assert!(
        prev > st0 && prev < last,
        "the last move replaced the first move's state ({prev:?} between {st0:?} and {last:?}), \
         not the state both started from"
    );
    assert!(st.document.thread_id == b || st.document.thread_id == c);
}

// ------------------------------------------------- M2, the restore half --

/// The restore fixture: `id` in "Old", sent to the trash with its thread;
/// "Keep" live.
struct Restore {
    s: Stack,
    old: String,
    keep: String,
    id: String,
}

async fn restore_fixture(tag: &str) -> Restore {
    let s = Stack::new(tag).await;
    let old = thread(&s, "Old").await;
    let keep = thread(&s, "Keep").await;
    let id = doc_in(&s, &old, "Draft", "draft text").await;
    delete_thread_to_trash(&s.svc, s.edb.as_ref(), &old).await.expect("thread deleted");
    assert!(s.svc.read(&id).await.unwrap().trashed_at.is_some(), "setup: in the trash");
    s.clock.advance(Duration::minutes(1));
    Restore { s, old, keep, id }
}

/// M2: "The restore into a thread becomes one service operation under one
/// lock: out of the trash and into the thread, one stamped state."
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m2_the_restore_into_a_thread_is_one_stamped_state() {
    let r = restore_fixture("m2-restore-one").await;
    let s = &r.s;
    let trashed = current_stamp(s, &r.id).await;
    let stored_before = s.stored(&r.id).await;
    s.svc.restore_from_trash_into(&r.id, &r.keep).await.expect("restored into Keep");

    let v = s.svc.read(&r.id).await.unwrap();
    assert!(v.trashed_at.is_none(), "out of the trash");
    assert_eq!(v.thread_id, r.keep, "into the chosen thread");
    assert_eq!(s.stored(&r.id).await, stored_before, "title and content as they were");
    let st = s.svc.stored_state(&r.id).await.unwrap();
    let stamp = st.meta.stamp.clone().unwrap();
    assert!(stamp > trashed);
    assert_eq!((stamp.wall_ms, stamp.device.as_str()), (ms(s), DEVICE));
    assert_eq!(
        st.meta.prev_stamp,
        Some(trashed),
        "one state replaces the trashed one: no intermediate state (moved but still trashed, or restored into the old thread)"
    );
    assert_eq!(st.document.thread_id, r.keep);
    assert!(st.document.deleted_at.is_none());
}

/// M2: one operation — refused as a whole. Into a deleted or unknown thread
/// the document stays in the trash, in its old thread, unstamped; a document
/// not in the trash is refused too.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m2_a_refused_restore_into_a_thread_changes_nothing() {
    let r = restore_fixture("m2-restore-refused").await;
    let s = &r.s;
    let gone = thread(s, "Also gone").await;
    delete_thread_to_trash(&s.svc, s.edb.as_ref(), &gone).await.unwrap();
    let snap = snapshot(s, &r.id).await;
    for (what, target) in [("its own deleted thread", r.old.as_str()), ("another deleted thread", gone.as_str()), ("an unknown thread", "thread:nowhere")] {
        let e = s.svc.restore_from_trash_into(&r.id, target).await.expect_err(what);
        assert!(!e.to_string().trim().is_empty(), "{what}: says why");
        assert_eq!(snapshot(s, &r.id).await, snap, "{what}: still in the trash, unchanged, no stamp");
    }
    let live = doc_in(s, &r.keep, "Live", "not trashed").await;
    let snap = snapshot(s, &live).await;
    let e = s.svc.restore_from_trash_into(&live, &r.keep).await.expect_err("not in the trash");
    assert!(!e.to_string().trim().is_empty());
    assert_eq!(snapshot(s, &live).await, snap);
}
