//! v0.0.12 lot 1, item 2 — the stamp bound inside the clock (C1–C5).
//!
//! Contract: `doc/plans/V0012-lot1.md`, section "2 — The stamp bound inside
//! the clock". Written before the code, by another author: whoever implements
//! item 2 makes these pass **without editing this file**; a test that looks
//! wrong is reported, not worked around.
//!
//! - C1 "This device's clock never moves more than `MAX_STAMP_AHEAD_SECS`
//!   ahead of its wall clock (`device_clock_ms`, not the clock itself)
//!   because of a stamp it received or read." — checked here through every
//!   caller that receives a stamp (decision stamp, purge marker, backup), and
//!   at the choke point itself in `src/lot1_clock_tests.rs` (`observe_stamp`
//!   reports whether it took the stamp).
//! - C2 what is refused (decision stamp, purge marker) and what is restored
//!   without moving the clock (backup).
//! - C3 restart: `ensure_clock` skips stored stamps beyond the bound.
//! - C4 no overflow of the counter.
//! - C5 the scenario of finding 1 (review v0.0.11), on three devices.
//!
//! "Within the bound" is read as in `apply_wire` today: a stamp whose
//! `wall_ms` is at most `device_clock_ms() + MAX_STAMP_AHEAD_SECS * 1000`.
//! The stacks' clocks are fake and never move by themselves, so the device
//! clock of a stack is `stack.clock.get()`.

mod common;

use chrono::Duration;
use common::wire::*;
use common::*;
use sovereign_db::schema::{DocumentPatch, PurgeMarker, Thread};
use sovereign_docs::{
    BackupDocument, PurgeApplied, PurgeMarkerWire, Stamp, Trust, TrustDecision, WireApplied,
    WireMeta, MAX_STAMP_AHEAD_SECS,
};

const DAY: i64 = 24 * 3600;

/// The latest stamp `s` may issue or take: its wall clock plus the bound.
fn bound_ms(s: &Stack) -> i64 {
    s.clock.get().timestamp_millis() + MAX_STAMP_AHEAD_SECS * 1000
}

fn assert_within(s: &Stack, st: &Stamp, what: &str) {
    assert!(
        st.wall_ms <= bound_ms(s),
        "{what}: stamp {st:?} is {} ms beyond the bound of this device's clock",
        st.wall_ms - bound_ms(s)
    );
}

/// A stamp as stored on a row (`wall_ms:counter:device`, schema doc comment).
fn stored_stamp(st: &Stamp) -> String {
    format!("{}:{}:{}", st.wall_ms, st.counter, st.device)
}

/// The stamp the next local write of `s` gets: a new document, created here.
async fn next_local_stamp(s: &Stack, tag: &str) -> Stamp {
    let id = s.create_text(&format!("probe {tag}"), "probe").await;
    current_stamp(s, &id).await
}

/// A stack whose wall clock is `secs` ahead of the others' (a wrong RTC).
async fn ahead_stack(tag: &str, device: &str, secs: i64) -> Stack {
    let s = Stack::with_device(tag, device).await;
    s.clock.advance(Duration::seconds(secs));
    s
}

/// Device `far` (clock a day ahead) purges `id`, which it received from `from`.
async fn far_marker(from: &Stack, far: &Stack, id: &str) -> PurgeMarkerWire {
    send(from, far, id).await;
    far.svc.trash(id).await.unwrap();
    far.svc.purge_from_trash(id).await.unwrap();
    far.svc.purge_markers().await.unwrap().into_iter().find(|m| m.id == id).expect("a marker for the purge")
}

// ============================================================ C2 — decision

// C2: "A document whose decision stamp is beyond the bound is refused whole
// (`RefusedStampAhead`), as for its own stamp, before anything is written."
// Also C1: the clock did not move (the next local write is within the bound).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c2_a_decision_stamp_beyond_the_bound_refuses_the_document_whole() {
    let s = Stack::new("c2-decision").await;
    let id = s.create_text("T", "x").await;
    let local = current_stamp(&s, &id).await;
    let before = s.stored(&id).await;
    let versions = s.commits(&id).await.len();
    let trust_before = s.svc.read(&id).await.unwrap().trust;
    s.events.take();

    let mut remote = remote_text(&id, "T", "from a phone whose clock is a day ahead", "phone");
    remote.trust = Some(Trust::External);
    remote.trust_decision =
        Some(TrustDecision { trust: Trust::External, device: "phone".into(), decided_at: s.clock.get() });
    let mut m = meta(Some(stamp(&s, 60, 0, "phone")), Some(local));
    m.decision_stamp = Some(stamp(&s, DAY, 0, "phone"));
    let out = s.svc.apply_wire(remote, m).await.unwrap();

    assert_eq!(out.applied, WireApplied::RefusedStampAhead, "refused whole");
    assert_eq!(s.stored(&id).await, before, "nothing written");
    assert_eq!(s.commits(&id).await.len(), versions, "no version");
    assert_eq!(s.svc.read(&id).await.unwrap().trust, trust_before, "the decision is not applied");
    assert!(s.events.peek().is_empty(), "no event");
    // C1: the decision stamp was not taken by the clock.
    let next = next_local_stamp(&s, "after decision").await;
    assert_within(&s, &next, "the next local write after a refused decision stamp");
}

// C2, a document this device does not hold yet: refused too, nothing created.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c2_a_new_document_with_a_decision_stamp_beyond_the_bound_is_not_created() {
    let s = Stack::new("c2-decision-new").await;
    let id = "document:decisionfromthefuture";
    let mut remote = remote_text(id, "T", "x", "phone");
    remote.trust = Some(Trust::External);
    remote.trust_decision =
        Some(TrustDecision { trust: Trust::External, device: "phone".into(), decided_at: s.clock.get() });
    let mut m = meta(Some(stamp(&s, 10, 0, "phone")), None);
    m.decision_stamp = Some(stamp(&s, DAY, 0, "phone"));
    let out = s.svc.apply_wire(remote, m).await.unwrap();
    assert_eq!(out.applied, WireApplied::RefusedStampAhead);
    assert!(s.svc.read(id).await.is_err(), "nothing created");
    let next = next_local_stamp(&s, "after new").await;
    assert_within(&s, &next, "the next local write");
}

// C2 boundary: a decision stamp within the bound is still accepted (the
// bound refuses only what is beyond it).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c2_a_decision_stamp_within_the_bound_is_accepted() {
    let s = Stack::new("c2-decision-near").await;
    let id = s.create_text("T", "x").await;
    let local = current_stamp(&s, &id).await;
    let mut remote = remote_text(&id, "T", "decided on the phone", "phone");
    remote.trust = Some(Trust::External);
    remote.trust_decision =
        Some(TrustDecision { trust: Trust::External, device: "phone".into(), decided_at: s.clock.get() });
    let mut m = meta(Some(stamp(&s, 30, 0, "phone")), Some(local));
    m.decision_stamp = Some(stamp(&s, MAX_STAMP_AHEAD_SECS - 60, 0, "phone"));
    let out = s.svc.apply_wire(remote, m).await.unwrap();
    assert!(matches!(out.applied, WireApplied::Applied(_)), "{:?}", out.applied);
}

// ============================================================ C2 — purge marker

// C2: "A purge marker whose stamp is beyond the bound is refused: nothing
// purged, no marker recorded; it is applied at a later sync once it is within
// the bound." Case: this device holds the same version (it would be purged).
//
// The refusal is read as a new `PurgeApplied::RefusedStampAhead`, the
// counterpart of `WireApplied::RefusedStampAhead` (API the implementer
// provides).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c2_a_purge_marker_beyond_the_bound_purges_nothing_and_is_applied_later() {
    let a = Stack::with_device("c2-marker-a", "laptop").await;
    let far = ahead_stack("c2-marker-far", "tablet", DAY).await;
    let id = a.create_text("Doc", "the same text").await;
    let marker = far_marker(&a, &far, &id).await;
    assert!(marker.stamp.wall_ms > bound_ms(&a), "precondition: the marker is beyond a's bound");
    let before = a.stored(&id).await;
    let versions = a.commits(&id).await.len();

    let out = a.svc.apply_purge_marker(marker.clone()).await.unwrap();
    assert_eq!(out, PurgeApplied::RefusedStampAhead);
    assert_eq!(a.stored(&id).await, before, "nothing purged");
    assert_eq!(a.commits(&id).await.len(), versions, "the versions are kept");
    assert!(!a.svc.is_purged(&id).await.unwrap(), "not purged");
    assert!(a.svc.read(&id).await.is_ok(), "still readable");
    assert!(
        !a.svc.purge_markers().await.unwrap().iter().any(|m| m.id == id),
        "no marker recorded (it would be re-advertised)"
    );
    // C1: the marker's stamp did not move the clock.
    let next = next_local_stamp(&a, "after marker").await;
    assert_within(&a, &next, "the next local write after a refused marker");

    // A day later, the same marker is within the bound: applied as usual.
    // Rewritten by item 3 (purges are local, L4): applied, a received
    // deletion moves a's older live copy to a's trash; it never purges it
    // (was: `PurgeApplied::Purged` and `is_purged`).
    a.clock.advance(Duration::seconds(DAY));
    let out = a.svc.apply_purge_marker(marker).await.unwrap();
    assert_ne!(out, PurgeApplied::RefusedStampAhead, "applied once within the bound");
    assert!(a.svc.read(&id).await.unwrap().trashed_at.is_some(), "a's copy is in a's trash");
    assert!(!a.svc.is_purged(&id).await.unwrap(), "never purged by a received deletion");
}

// C2, a device without the document: the marker is not recorded.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c2_a_purge_marker_beyond_the_bound_is_not_recorded_where_there_is_no_copy() {
    let a = Stack::with_device("c2-marker-none-a", "laptop").await;
    let far = ahead_stack("c2-marker-none-far", "tablet", DAY).await;
    let c = Stack::with_device("c2-marker-none-c", "phone").await;
    let id = a.create_text("Doc", "text").await;
    let marker = far_marker(&a, &far, &id).await;

    let out = c.svc.apply_purge_marker(marker.clone()).await.unwrap();
    assert_eq!(out, PurgeApplied::RefusedStampAhead);
    assert!(!c.svc.is_purged(&id).await.unwrap(), "no marker recorded");
    assert!(c.svc.purge_markers().await.unwrap().is_empty(), "nothing to re-advertise");
    let next = next_local_stamp(&c, "after marker").await;
    assert_within(&c, &next, "the next local write");
    // The document, arriving later from a's (correct) clock, is accepted:
    // the refused marker did not block it.
    let out = send(&a, &c, &id).await;
    assert!(matches!(out.applied, WireApplied::Applied(_)), "{:?}", out.applied);

    // Rewritten by item 3 (L4): once within the bound the deletion applies —
    // c's copy (older than it) goes to c's trash, never purged (was:
    // `PurgeApplied::Purged`).
    c.clock.advance(Duration::seconds(DAY));
    let out = c.svc.apply_purge_marker(marker).await.unwrap();
    assert_ne!(out, PurgeApplied::RefusedStampAhead, "applied once within the bound");
    assert!(c.svc.read(&id).await.unwrap().trashed_at.is_some(), "c's copy is in c's trash");
    assert!(!c.svc.is_purged(&id).await.unwrap(), "never purged by a received deletion");
}

// C2 + C1, a copy that differs (in v0.0.11 `KeptDiffers`; since item 3 a copy
// is judged by its stamp, never by content): the marker is refused
// as a whole, and above all its stamp does not move the clock (the review's
// finding 1: "even when the result is `KeptDiffers`").
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c2_a_purge_marker_beyond_the_bound_does_not_move_the_clock_when_the_copy_differs() {
    let a = Stack::with_device("c2-marker-diff-a", "laptop").await;
    let far = ahead_stack("c2-marker-diff-far", "tablet", DAY).await;
    let id = a.create_text("Doc", "first").await;
    let marker = far_marker(&a, &far, &id).await;
    a.editor_save(&id, 1, v2(vec![text_block(1, "edited here")])).await.unwrap();
    let before = a.stored(&id).await;

    let out = a.svc.apply_purge_marker(marker).await.unwrap();
    // CONTRACT-QUESTION (lot 1): C2 says a marker beyond the bound "is
    // refused"; it did not say whether a copy that differs reports the
    // refusal or `KeptDiffers` (both purge nothing). Read: the refusal comes
    // first, whatever the copy. (Item 3 removes `KeptDiffers`; the reading
    // stands: L4 says the bound "still applies first".)
    assert_eq!(out, PurgeApplied::RefusedStampAhead);
    assert_eq!(a.stored(&id).await, before, "kept");
    let next = next_local_stamp(&a, "after marker").await;
    assert_within(&a, &next, "the next local write");
}

// ============================================================ C2 — backup

/// A backup made by a device whose clock was a day ahead: one live document
/// with a trust decision, one in the trash, one purged (a marker).
async fn far_backup() -> (Vec<BackupDocument>, Vec<PurgeMarkerWire>, Vec<String>, String) {
    let src = ahead_stack("c2-backup-src", "tablet", DAY).await;
    let live = src.create_text("Live", "kept").await;
    src.svc.set_trust(&live, Trust::External).await.unwrap();
    let trashed = src.create_text("Trashed", "x").await;
    src.svc.trash(&trashed).await.unwrap();
    let gone = src.create_text("Purged", "erased").await;
    src.svc.trash(&gone).await.unwrap();
    src.svc.purge_from_trash(&gone).await.unwrap();
    let docs = src
        .svc
        .stored_states()
        .await
        .unwrap()
        .into_iter()
        .map(|w| BackupDocument { document: w.document, meta: w.meta })
        .collect();
    let markers = src.svc.purge_markers().await.unwrap();
    (docs, markers, vec![live, trashed], gone)
}

// C2: "Backup restore restores the documents and markers as stored, but a
// stamp beyond the bound does not move the clock. [choice] A backup is the
// user's own data; refusing it would lose documents."
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c2_backup_restore_restores_everything_but_its_stamps_beyond_the_bound_do_not_move_the_clock() {
    let (docs, markers, ids, gone) = far_backup().await;
    assert!(!markers.is_empty());
    let s = Stack::new("c2-backup").await;
    for d in &docs {
        assert!(
            d.meta.stamp.as_ref().is_some_and(|st| st.wall_ms > bound_ms(&s)),
            "precondition: the backup's stamps are beyond the bound"
        );
    }
    let report = s.svc.restore_backup(docs.clone(), markers.clone()).await.unwrap();
    let mut restored = report.restored.clone();
    restored.sort();
    let mut want = ids.clone();
    want.sort();
    assert_eq!(restored, want, "every document restored");
    for id in &ids {
        assert!(s.svc.read(id).await.is_ok(), "{id} restored");
    }
    assert!(s.svc.is_purged(&gone).await.unwrap(), "the marker is recorded");
    for m in &markers {
        assert!(s.svc.purge_markers().await.unwrap().contains(m), "marker kept as stored");
    }
    // "as stored": the restored document keeps the stamps it was backed up with.
    let live = s.svc.stored_state(&ids[0]).await.unwrap();
    assert_eq!(live.meta.stamp, docs.iter().find(|d| d.document.id == ids[0]).unwrap().meta.stamp);
    // C1: none of them moved the clock.
    let next = next_local_stamp(&s, "after backup").await;
    assert_within(&s, &next, "the first local write after the restore");
    // C3 too: still so after a restart.
    let mut s = s;
    s.restart();
    let next = next_local_stamp(&s, "after backup and restart").await;
    assert_within(&s, &next, "the first local write after a restart");
}

// ============================================================ C3 — restart

/// What a v0.0.11 device may have stored with a stamp a day ahead (each
/// alone, so a failure names its source).
#[derive(Clone, Copy, Debug)]
enum Stored {
    DocumentStamp,
    DecisionStamp,
    StampHistory,
    PurgeMarker,
    ThreadStamp,
}

async fn store_far_stamp(s: &Stack, what: Stored, far: &Stamp) {
    let enc = stored_stamp(far);
    match what {
        Stored::DocumentStamp | Stored::DecisionStamp | Stored::StampHistory => {
            let id = s.create_text("Stored with v0.0.11", "x").await;
            let mut p = DocumentPatch::default();
            match what {
                Stored::DocumentStamp => p.stamp = Some(Some(enc)),
                Stored::DecisionStamp => p.decision_stamp = Some(Some(enc)),
                _ => p.stamp_history = Some(vec![enc]),
            }
            s.edb.update_document_fields(&id, p).await.unwrap();
        }
        Stored::PurgeMarker => {
            s.edb
                .set_purge_marker(PurgeMarker {
                    id: None,
                    document_id: "document:purgedwithv0011".into(),
                    purged_at: s.clock.get(),
                    complete: true,
                    stamp: Some(enc),
                    // What v0.0.11 stored: a fingerprint (64 hex digits). Item 3
                    // removed `purge_fingerprint`, so the value is written out.
                    fingerprint: Some("5e".repeat(32)),
                    revoked: false,
                })
                .await
                .unwrap();
        }
        Stored::ThreadStamp => {
            let mut t = Thread::new("Stored with v0.0.11".into(), String::new());
            t.stamp = Some(enc);
            s.edb.create_thread(t).await.unwrap();
        }
    }
}

// C3: "`ensure_clock` skips stored stamps beyond the bound when it starts the
// clock. A device that stored one with v0.0.11 stops issuing future stamps at
// its first v0.0.12 start." Every place `ensure_clock` reads a stamp from.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c3_a_stored_stamp_beyond_the_bound_does_not_start_the_clock_ahead() {
    for what in [
        Stored::DocumentStamp,
        Stored::DecisionStamp,
        Stored::StampHistory,
        Stored::PurgeMarker,
        Stored::ThreadStamp,
    ] {
        let mut s = Stack::new(&format!("c3-{what:?}")).await;
        let far = stamp(&s, DAY, 3, "laptop");
        store_far_stamp(&s, what, &far).await;
        s.restart(); // the first v0.0.12 start over the v0.0.11 store
        let next = next_local_stamp(&s, "after restart").await;
        assert_within(&s, &next, &format!("{what:?}: the first local write after the start"));
    }
}

// C3, its other side: a stored stamp within the bound still starts the clock
// (G25 holds for stamps within the bound).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c3_a_stored_stamp_within_the_bound_still_starts_the_clock() {
    for what in [Stored::DocumentStamp, Stored::PurgeMarker, Stored::ThreadStamp] {
        let mut s = Stack::new(&format!("c3-near-{what:?}")).await;
        let near = stamp(&s, MAX_STAMP_AHEAD_SECS - 60, 9, "phone");
        store_far_stamp(&s, what, &near).await;
        s.restart();
        let next = next_local_stamp(&s, "after restart").await;
        assert!(next > near, "{what:?}: G25, the next stamp {next:?} is after the stored {near:?}");
    }
}

// ============================================================ C4 — overflow

// C4: "When the clock's counter is at its maximum, the next stamp moves to the
// next millisecond with counter 0. Never a wrap, never a panic." Built from a
// received stamp with `counter = u32::MAX` (within the bound).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c4_a_received_stamp_at_the_maximum_counter_makes_the_next_stamp_the_next_millisecond() {
    let s = Stack::new("c4-received").await;
    let id = s.create_text("T", "x").await;
    let local = current_stamp(&s, &id).await;
    let max = stamp(&s, 60, u32::MAX, "phone");
    let out = s
        .svc
        .apply_wire(remote_text(&id, "T", "from the phone", "phone"), meta(Some(max.clone()), Some(local)))
        .await
        .unwrap();
    assert!(matches!(out.applied, WireApplied::Applied(_)), "{:?}", out.applied);

    s.editor_save(&id, 1, v2(vec![text_block(1, "after")])).await.expect("no panic, no error");
    let next = current_stamp(&s, &id).await;
    assert!(next > max, "never a wrap: {next:?} after {max:?}");
    assert_eq!((next.wall_ms, next.counter), (max.wall_ms + 1, 0), "the next millisecond, counter 0");
    // And the clock goes on from there.
    s.editor_save(&id, 1, v2(vec![text_block(1, "again")])).await.unwrap();
    let again = current_stamp(&s, &id).await;
    assert_eq!((again.wall_ms, again.counter), (max.wall_ms + 1, 1));
}

// C4, the clock already at its maximum when it starts: a stamp with
// `counter = u32::MAX` read from the store at a restart (`ensure_clock`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c4_a_clock_that_starts_at_the_maximum_counter_moves_to_the_next_millisecond() {
    let mut s = Stack::new("c4-started").await;
    let max = stamp(&s, 60, u32::MAX, "laptop");
    store_far_stamp(&s, Stored::DocumentStamp, &max).await;
    s.restart();
    let next = next_local_stamp(&s, "first").await;
    assert!(next > max, "never a wrap: {next:?} after {max:?}");
    assert_eq!((next.wall_ms, next.counter), (max.wall_ms + 1, 0));
    let again = next_local_stamp(&s, "second").await;
    assert_eq!((again.wall_ms, again.counter), (max.wall_ms + 1, 1));
}

// C4: "A received stamp with `counter = u32::MAX` is ordered as any other."
// (Expected green today: a regression guard on the ordering of the maximum.)
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c4_a_stamp_at_the_maximum_counter_is_ordered_as_any_other() {
    let s = Stack::new("c4-order").await;
    let id = s.create_text("T", "x").await;
    let local = current_stamp(&s, &id).await;
    let max = stamp(&s, 60, u32::MAX, "phone");
    let after = Stamp { wall_ms: max.wall_ms + 1, counter: 0, device: "phone".into() };
    let first = s
        .svc
        .apply_wire(remote_text(&id, "T", "max", "phone"), meta(Some(max.clone()), Some(local)))
        .await
        .unwrap();
    assert!(matches!(first.applied, WireApplied::Applied(_)));
    let next = s
        .svc
        .apply_wire(remote_text(&id, "T", "after max", "phone"), meta(Some(after.clone()), Some(max.clone())))
        .await
        .unwrap();
    assert!(matches!(next.applied, WireApplied::Applied(_)), "the state after the maximum follows it");
    assert!(!next.concurrent);
    assert_eq!(texts(&s.stored(&id).await.1), vec!["after max"]);
    // The maximum arriving again later is older: ignored.
    let late = s
        .svc
        .apply_wire(remote_text(&id, "T", "max", "phone"), WireMeta { stamp: Some(max), ..WireMeta::default() })
        .await
        .unwrap();
    assert_eq!(late.applied, WireApplied::Ignored);
    assert_eq!(texts(&s.stored(&id).await.1), vec!["after max"]);
}

// ============================================================ C5 — finding 1

// C5: "Device A receives a purge marker (and, separately, a document with a
// decision stamp) one day ahead. A's next local writes carry stamps within
// the bound, and device B accepts them." The far device purges a document A
// holds; a peer sends a document whose own stamp is fine and whose decision
// stamp is a day ahead.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c5_after_a_far_marker_and_a_far_decision_a_s_writes_still_reach_b() {
    let a = Stack::with_device("c5-a", "laptop").await;
    let b = Stack::with_device("c5-b", "phone").await;
    let far = ahead_stack("c5-far", "tablet", DAY).await;

    let shared = a.create_text("Shared", "first").await;
    send(&a, &b, &shared).await;
    let purged = a.create_text("Purged elsewhere", "same text").await;

    // 1. The far device's purge marker reaches A.
    let marker = far_marker(&a, &far, &purged).await;
    let _ = a.svc.apply_purge_marker(marker).await.unwrap();
    // 2. A document whose decision stamp is a day ahead reaches A.
    let mut remote = remote_text("document:farsdecision", "Theirs", "x", "tablet");
    remote.trust = Some(Trust::External);
    remote.trust_decision =
        Some(TrustDecision { trust: Trust::External, device: "tablet".into(), decided_at: a.clock.get() });
    let mut m = meta(Some(stamp(&a, 5, 0, "tablet")), None);
    m.decision_stamp = Some(stamp(&a, DAY, 0, "tablet"));
    let _ = a.svc.apply_wire(remote, m).await.unwrap();

    // 3. A's next local writes: an edit, a new document, a trash.
    a.editor_save(&shared, 1, v2(vec![text_block(1, "edited on A")])).await.unwrap();
    let edited = current_stamp(&a, &shared).await;
    assert_within(&a, &edited, "A's edit");
    let created = a.create_text("New on A", "new").await;
    assert_within(&a, &current_stamp(&a, &created).await, "A's new document");
    let trashed = a.create_text("Trashed on A", "t").await;
    send(&a, &b, &trashed).await;
    a.svc.trash(&trashed).await.unwrap();
    assert_within(&a, &current_stamp(&a, &trashed).await, "A's trash");

    // 4. B accepts them.
    let out = send(&a, &b, &shared).await;
    assert!(matches!(out.applied, WireApplied::Applied(_)), "B accepts A's edit: {:?}", out.applied);
    assert_eq!(texts(&b.stored(&shared).await.1), vec!["edited on A"]);
    let out = send(&a, &b, &created).await;
    assert!(matches!(out.applied, WireApplied::Applied(_)), "B accepts A's new document: {:?}", out.applied);
    let out = send(&a, &b, &trashed).await;
    assert!(matches!(out.applied, WireApplied::Applied(_)), "B accepts A's trash: {:?}", out.applied);
    assert!(b.svc.read(&trashed).await.unwrap().trashed_at.is_some());

    // And the other way: A still accepts B's next write (A's ordering was
    // not pushed a day ahead by what it received).
    b.editor_save(&shared, 2, v2(vec![text_block(1, "edited on B")])).await.unwrap();
    let out = send(&b, &a, &shared).await;
    assert!(matches!(out.applied, WireApplied::Applied(_)), "A accepts B's next edit: {:?}", out.applied);
    assert_eq!(texts(&a.stored(&shared).await.1), vec!["edited on B"]);
}

// C5's precision (contract amended, b35e0ff, PANDA2 0134): "each device bounds
// against its own wall clock. With two wall clocks set d apart, a device
// pushed to its bound issues stamps the other refuses for up to d, retried at
// the next sync: a delay of at most d, not a stop."
//
// B's wall clock is d = 2 minutes behind A's. A receives a far marker (a day
// ahead: refused, C2) and a document stamped exactly at A's own bound (taken).
// A's next edit is then refused by B — for at most d: once B's clock has moved
// d, the same state is accepted.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c5_with_wall_clocks_d_apart_a_pushed_device_is_delayed_at_most_d_never_stopped() {
    const D: i64 = 120;
    let a = Stack::with_device("c5d-a", "laptop").await;
    let b = Stack::with_device("c5d-b", "phone").await;
    b.clock.advance(Duration::seconds(-D));
    let far = ahead_stack("c5d-far", "tablet", DAY).await;

    let shared = a.create_text("Shared", "first").await;
    let out = send(&a, &b, &shared).await;
    assert!(matches!(out.applied, WireApplied::Applied(_)), "precondition: {:?}", out.applied);
    let purged = a.create_text("Purged elsewhere", "same text").await;
    let marker = far_marker(&a, &far, &purged).await;
    let _ = a.svc.apply_purge_marker(marker).await.unwrap();
    // A document stamped exactly at A's bound: within it, so it is taken.
    let at_bound = Stamp { wall_ms: bound_ms(&a), counter: 0, device: "tablet".into() };
    let out = a
        .svc
        .apply_wire(remote_text("document:atabound", "Theirs", "x", "tablet"), meta(Some(at_bound), None))
        .await
        .unwrap();
    assert!(matches!(out.applied, WireApplied::Applied(_)), "a stamp at the bound is taken: {:?}", out.applied);

    a.editor_save(&shared, 1, v2(vec![text_block(1, "edited on A")])).await.unwrap();
    let edited = current_stamp(&a, &shared).await;
    assert_within(&a, &edited, "A's edit");

    // B, d behind, refuses it now (observed, the delay the contract names)…
    let state = a.svc.stored_state(&shared).await.unwrap();
    let now = b.svc.apply_wire(state.document.clone(), state.meta.clone()).await.unwrap();
    if now.applied == WireApplied::RefusedStampAhead {
        // …and accepts it once its clock has moved d: a later sync.
        b.clock.advance(Duration::seconds(D));
        let later = b.svc.apply_wire(state.document, state.meta).await.unwrap();
        assert!(
            matches!(later.applied, WireApplied::Applied(_)),
            "accepted after at most d: {:?} (stamp {} ms beyond B's bound)",
            later.applied,
            edited.wall_ms - bound_ms(&b)
        );
    } else {
        assert!(matches!(now.applied, WireApplied::Applied(_)), "{:?}", now.applied);
    }
    assert_eq!(texts(&b.stored(&shared).await.1), vec!["edited on A"]);
}
