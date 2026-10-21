//! G25 — stamps, not clocks.
//!
//! Spec ch. 03, *Document service — contract (C-DOC)*, G25 (specified
//! 2026-10-15, for C-WIRE): "Every write gets a hybrid-logical-clock stamp
//! greater than every stamp this device has issued or received for any
//! document, whatever its clock says. A remote state is applied by stamp: one
//! that follows the local state (its previous stamp is the local stamp) is
//! applied; an older one is ignored; one more than 5 minutes ahead of this
//! device's clock is refused and writes nothing."
//!
//! C-WIRE guarantee 4 (spec ch. 05): "Order is by stamp, not by clock."

mod common;

use chrono::Duration;
use common::wire::*;
use common::*;
use sovereign_docs::{Trust, WireApplied, MAX_STAMP_AHEAD_SECS};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g25_every_local_write_gets_a_greater_stamp_even_when_the_clock_goes_back() {
    let s = Stack::new("g25-monotonic").await;
    let id = s.create_text("T", "x").await;
    let mut last = current_stamp(&s, &id).await;
    for step in 0..4 {
        s.clock.advance(Duration::hours(-1));
        match step {
            0 => {
                s.editor_save(&id, 1, v2(vec![text_block(1, "edited")])).await.unwrap();
            }
            1 => {
                s.assistant_save(&id, "assistant").await.unwrap();
            }
            2 => {
                s.svc.trash(&id).await.unwrap();
            }
            _ => {
                s.svc.restore_from_trash(&id).await.unwrap();
            }
        }
        let now = current_stamp(&s, &id).await;
        assert!(now > last, "step {step}: {now:?} not after {last:?}");
        last = now;
    }
    s.clock.advance(Duration::hours(-1));
    s.svc.set_trust(&id, Trust::External).await.unwrap();
    let decision = s.svc.stored_state(&id).await.unwrap().meta.decision_stamp.expect("a decision has a stamp");
    assert!(decision > last, "a trust decision is stamped after the last write");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g25_a_local_write_after_receiving_a_stamp_is_later_than_it() {
    let s = Stack::new("g25-receive").await;
    let id = s.create_text("T", "x").await;
    let local = current_stamp(&s, &id).await;
    let ahead = stamp(&s, 120, 7, "phone"); // two minutes ahead: accepted
    let out = s
        .svc
        .apply_wire(remote_text(&id, "T", "from the phone", "phone"), meta(Some(ahead.clone()), Some(local)))
        .await
        .unwrap();
    assert!(matches!(out.applied, WireApplied::Applied(_)));
    s.editor_save(&id, 2, v2(vec![text_block(1, "after")])).await.unwrap();
    assert!(current_stamp(&s, &id).await > ahead, "the next write is later than what was received");
    // A stamp received on another document counts too.
    let other = s.create_text("Other", "o").await;
    assert!(current_stamp(&s, &other).await > ahead);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g25_a_stamp_too_far_ahead_is_refused_and_writes_nothing() {
    let s = Stack::new("g25-ahead").await;
    let id = s.create_text("T", "x").await;
    let local = current_stamp(&s, &id).await;
    let before = s.stored(&id).await;
    let versions = s.commits(&id).await.len();
    s.events.take();
    let too_far = stamp(&s, MAX_STAMP_AHEAD_SECS + 60, 0, "phone");
    let out = s
        .svc
        .apply_wire(remote_text(&id, "T", "from the future", "phone"), meta(Some(too_far.clone()), Some(local.clone())))
        .await
        .unwrap();
    assert_eq!(out.applied, WireApplied::RefusedStampAhead);
    assert_eq!(s.stored(&id).await, before);
    assert_eq!(s.commits(&id).await.len(), versions);
    assert!(s.events.peek().is_empty());
    // A new document from the future is refused too.
    let out = s
        .svc
        .apply_wire(remote_text("document:future", "T", "x", "phone"), meta(Some(too_far), None))
        .await
        .unwrap();
    assert_eq!(out.applied, WireApplied::RefusedStampAhead);
    assert!(s.svc.read("document:future").await.is_err());
    // Within the bound, accepted.
    let near = stamp(&s, MAX_STAMP_AHEAD_SECS - 60, 0, "phone");
    let out = s.svc.apply_wire(remote_text(&id, "T", "soon", "phone"), meta(Some(near), Some(local))).await.unwrap();
    assert!(matches!(out.applied, WireApplied::Applied(_)));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g25_a_state_that_follows_the_local_one_applies_and_is_not_concurrent() {
    let s = Stack::new("g25-ff").await;
    let id = s.create_text("T", "x").await;
    let local = current_stamp(&s, &id).await;
    let out = s
        .svc
        .apply_wire(remote_text(&id, "T", "next", "phone"), meta(Some(stamp(&s, 1, 0, "phone")), Some(local)))
        .await
        .unwrap();
    assert!(matches!(out.applied, WireApplied::Applied(_)));
    assert!(!out.concurrent);
    assert_eq!(texts(&s.stored(&id).await.1), vec!["next"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g25_an_older_state_is_ignored() {
    let s = Stack::new("g25-older").await;
    let id = s.create_text("T", "first").await;
    let first = s.svc.stored_state(&id).await.unwrap();
    s.editor_save(&id, 1, v2(vec![text_block(1, "second")])).await.unwrap();
    let now = s.stored(&id).await;
    // The first state comes back from a peer that had not seen the second.
    let out = s.svc.apply_wire(first.document, first.meta).await.unwrap();
    assert_eq!(out.applied, WireApplied::Ignored);
    assert_eq!(s.stored(&id).await, now);
}
