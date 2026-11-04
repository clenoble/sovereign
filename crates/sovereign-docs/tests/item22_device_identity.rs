//! v0.0.12 item 22 — a device's identity in its stamps is unique (D1–D3, the
//! document service's half).
//!
//! Contract: `doc/plans/V0012-item22-device-identity.md` (finding: claude-coord
//! `from-windows/0142-device-identity-finding.md`). Written before the code,
//! by another author: whoever implements item 22 makes these pass **without
//! editing this file**; a test that looks wrong is reported, not worked around.
//!
//! The document service takes its device identity from its host
//! (`ServiceDeps::device`); the hosts' half (D1's "never the display name",
//! D4, D5, D6) is tested in `sovereign-shell` (`item22_identity_tests.rs`) and
//! `sovereign-app` (`item22_identity_tests.rs`). This file checks what the
//! service does with an identity that is a libp2p peer id:
//!
//! - **D1** — every place the service writes its device carries the identity
//!   it was given, unchanged: new stamps, local stampings' author
//!   (`author_device`), the state's `device`, trust decisions, thread stamps.
//! - **D2** — two devices with distinct identities, writing the same document
//!   at the same `wall_ms` with counter 0, write different stamps, and the
//!   second state is applied or kept as a concurrent version, never ignored.
//! - **D3** — a store written by v0.0.11's rule (stamps whose device is the
//!   display name "Sovereign Device") keeps its stamps after the upgrade; an
//!   edit after the upgrade fast-forwards on a peer; ordering is by `wall_ms`
//!   and counter first.
//!
//! Everything here is today's API. Expected green today for D1 and D2 (the
//! service already writes what it is given; the defect is in what the hosts
//! give it), and for D3 if old and new stamps coexist as the finding claims —
//! these tests are the check of that claim, not a restatement of it.

mod common;

use chrono::Duration;
use common::wire::*;
use common::*;
use sovereign_docs::{Stamp, Trust, WireApplied};

/// Two libp2p peer ids, as a node prints them (`12D3KooW…`, base58, no
/// colon: they survive the stored stamp form `wall_ms:counter:device`).
const PEER_LAPTOP: &str = "12D3KooWLaptopAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const PEER_DESK: &str = "12D3KooWDeskBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";

/// The display name every desktop had by default until items 14 + 19
/// (`sovereign-core/src/config.rs`, `P2pConfig::default`): what v0.0.11
/// wrote in every stamp.
const DISPLAY_NAME: &str = "Sovereign Device";

/// Whether `id`'s current state, or one of its versions, holds `text`.
async fn holds(s: &Stack, id: &str, text: &str) -> bool {
    let current = texts(&s.stored(id).await.1);
    if current.iter().any(|t| t == text) {
        return true;
    }
    s.committed_states(id).await.iter().any(|(_, c)| texts(c).iter().any(|t| t == text))
}

// ======================================================================== D1

// D1: every new stamp's `device`, local stampings' author (`author_device`),
// the state's `device` and trust decisions' device are the identity the
// service was given (the persona's peer id), on every kind of local write.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d1_every_local_write_carries_the_services_identity_unchanged() {
    let s = Stack::with_device("item22-d1", PEER_LAPTOP).await;

    // A creation.
    let id = s.create_text("T", "first").await;
    let st = s.svc.stored_state(&id).await.unwrap();
    assert_eq!(st.meta.stamp.as_ref().map(|x| x.device.as_str()), Some(PEER_LAPTOP), "D1: a new stamp's device");
    assert_eq!(st.meta.author_device.as_deref(), Some(PEER_LAPTOP), "D1: a local stamping's author");
    assert_eq!(st.document.device, PEER_LAPTOP, "D1: the state's device");
    assert_eq!(s.stored_doc(&id).await.author_device.as_deref(), Some(PEER_LAPTOP), "D1: the row's author_device");

    // An editor save, an assistant save, trash and restore: each a new stamp
    // from this device.
    s.clock.advance(Duration::seconds(1));
    s.editor_save(&id, 1, v2(vec![text_block(1, "edited")])).await.unwrap();
    assert_eq!(current_stamp(&s, &id).await.device, PEER_LAPTOP, "D1: an editor save's stamp");
    s.assistant_save(&id, "assistant").await.unwrap();
    assert_eq!(current_stamp(&s, &id).await.device, PEER_LAPTOP, "D1: an assistant save's stamp");
    s.svc.trash(&id).await.unwrap();
    assert_eq!(current_stamp(&s, &id).await.device, PEER_LAPTOP, "D1: a trash's stamp");
    s.svc.restore_from_trash(&id).await.unwrap();
    assert_eq!(current_stamp(&s, &id).await.device, PEER_LAPTOP, "D1: a restore's stamp");
    assert_eq!(
        s.svc.stored_state(&id).await.unwrap().meta.author_device.as_deref(),
        Some(PEER_LAPTOP),
        "D1: the author after local writes"
    );

    // A trust decision.
    s.svc.set_trust(&id, Trust::External).await.unwrap();
    let st = s.svc.stored_state(&id).await.unwrap();
    let decision = st.document.trust_decision.expect("a trust decision is stored");
    assert_eq!(decision.device, PEER_LAPTOP, "D1: a trust decision's device");
    assert_eq!(
        st.meta.decision_stamp.as_ref().map(|x| x.device.as_str()),
        Some(PEER_LAPTOP),
        "D1: a trust decision's stamp"
    );

    // A thread write (stamped by the same clock, THREAD-sync S1).
    let thread = s.svc.thread_create("Lane", "").await.unwrap();
    let t = s.edb.get_thread(&thread).await.expect("the thread reads");
    let tstamp = t.stamp.expect("a thread write is stamped");
    assert!(tstamp.ends_with(&format!(":{PEER_LAPTOP}")), "D1: a thread stamp's device: {tstamp}");
}

// D1: a state received from a peer keeps the peer's identity as its author;
// the receiving device's identity is not written over it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d1_a_received_state_keeps_its_authors_identity() {
    let a = Stack::with_device("item22-d1-a", PEER_LAPTOP).await;
    let b = Stack::with_device("item22-d1-b", PEER_DESK).await;
    let id = a.create_text("T", "from the laptop").await;
    let out = send(&a, &b, &id).await;
    assert!(matches!(out.applied, WireApplied::Applied(_)), "precondition: applied, {out:?}");
    let st = b.svc.stored_state(&id).await.unwrap();
    assert_eq!(st.meta.stamp.as_ref().map(|x| x.device.as_str()), Some(PEER_LAPTOP), "D1: the stamp is the author's");
    assert_eq!(st.meta.author_device.as_deref(), Some(PEER_LAPTOP), "D1: the author is the laptop's peer id");
    // B's next local write is B's.
    b.clock.advance(Duration::seconds(1));
    b.editor_save(&id, 1, v2(vec![text_block(1, "desk edit")])).await.unwrap();
    let st = b.svc.stored_state(&id).await.unwrap();
    assert_eq!(st.meta.stamp.as_ref().map(|x| x.device.as_str()), Some(PEER_DESK));
    assert_eq!(st.meta.author_device.as_deref(), Some(PEER_DESK));
}

// ======================================================================== D2

/// Two devices share a document, then each edits it at the same instant
/// (both clocks move by the same minute): same `wall_ms`, counter 0. Returns
/// the document's id and each edit's stamp.
async fn same_ms_edits(a: &Stack, b: &Stack) -> (String, Stamp, Stamp) {
    let id = a.create_text("T", "shared").await;
    let out = send(a, b, &id).await;
    assert!(matches!(out.applied, WireApplied::Applied(_)), "precondition: the shared state reaches b");
    a.clock.advance(Duration::minutes(1));
    b.clock.advance(Duration::minutes(1));
    assert_eq!(a.clock.get(), b.clock.get(), "precondition: both clocks read the same instant");
    a.editor_save(&id, 1, v2(vec![text_block(1, "laptop edit")])).await.unwrap();
    b.editor_save(&id, 1, v2(vec![text_block(1, "desk edit")])).await.unwrap();
    let (sa, sb) = (current_stamp(a, &id).await, current_stamp(b, &id).await);
    assert_eq!(sa.wall_ms, sb.wall_ms, "precondition: the same millisecond");
    assert_eq!((sa.counter, sb.counter), (0, 0), "precondition: counter 0 on both (the usual counter)");
    (id, sa, sb)
}

// D2: two devices (each with its own peer id; both were "Sovereign Device" by
// name) write the same document at the same `wall_ms` with counter 0: the
// stamps differ, and each side applies the other's state or keeps it as a
// concurrent version — never `Ignored` as already known. Both edits survive
// on both devices, and both end on the same current state.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d2_two_devices_writing_the_same_document_in_the_same_ms_never_write_equal_stamps() {
    let a = Stack::with_device("item22-d2-a", PEER_LAPTOP).await;
    let b = Stack::with_device("item22-d2-b", PEER_DESK).await;
    let (id, sa, sb) = same_ms_edits(&a, &b).await;
    assert_ne!(sa, sb, "D2: the two stamps differ");

    // Each device sends its edit as it is now (the two sends cross).
    let wa = a.svc.stored_state(&id).await.unwrap();
    let wb = b.svc.stored_state(&id).await.unwrap();
    let to_b = b.svc.apply_wire(wa.document, wa.meta).await.unwrap();
    let to_a = a.svc.apply_wire(wb.document, wb.meta).await.unwrap();
    for (out, way) in [(&to_b, "the laptop's state on the desk"), (&to_a, "the desk's state on the laptop")] {
        assert_ne!(out.applied, WireApplied::Ignored, "D2: {way} is not ignored as known");
        assert!(
            matches!(out.applied, WireApplied::Applied(_) | WireApplied::StoredAsVersion),
            "D2: {way}: applied or kept as a version: {out:?}"
        );
        assert!(out.concurrent, "D2: {way}: a concurrent edit (for the notice)");
    }

    for s in [&a, &b] {
        for text in ["laptop edit", "desk edit"] {
            assert!(holds(s, &id, text).await, "D2: {}: the edit {text:?} is kept (current or in History)", s.device);
        }
    }
    assert_eq!(texts(&a.stored(&id).await.1), texts(&b.stored(&id).await.1), "D2: the same current state on both");
}

// D2, the tie-break: at equal `(wall_ms, counter)`, the device decides the
// order, so the two devices agree on which state is current.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d2_at_equal_wall_ms_and_counter_the_identity_breaks_the_tie_the_same_way_on_both() {
    let a = Stack::with_device("item22-d2t-a", PEER_LAPTOP).await;
    let b = Stack::with_device("item22-d2t-b", PEER_DESK).await;
    let (id, sa, sb) = same_ms_edits(&a, &b).await;
    let winner = if sa > sb { "laptop edit" } else { "desk edit" };
    send(&a, &b, &id).await;
    send(&b, &a, &id).await;
    assert_eq!(texts(&a.stored(&id).await.1), vec![winner.to_string()], "D2: the greater stamp is current on the laptop");
    assert_eq!(texts(&b.stored(&id).await.1), vec![winner.to_string()], "D2: and on the desk");
}

// ======================================================================== D3

/// Two devices on v0.0.11's rule: both stamp with the display name. A
/// document shared between them, and a second one left untouched. Returns
/// (shared id, untouched id, the shared document's old stamp, the untouched
/// one's).
async fn v0011_store(a: &Stack, b: &Stack) -> (String, String, Stamp, Stamp) {
    assert_eq!(a.device, DISPLAY_NAME, "precondition: v0.0.11's rule");
    assert_eq!(b.device, DISPLAY_NAME, "precondition: v0.0.11's rule");
    let shared = a.create_text("Shared", "before the upgrade").await;
    let untouched = a.create_text("Untouched", "never edited again").await;
    for id in [&shared, &untouched] {
        let out = send(a, b, id).await;
        assert!(matches!(out.applied, WireApplied::Applied(_)), "precondition: {id} reaches b");
    }
    let old = current_stamp(a, &shared).await;
    let old_untouched = current_stamp(a, &untouched).await;
    assert_eq!(old.device, DISPLAY_NAME, "precondition: the stored stamp names the display name");
    (shared, untouched, old, old_untouched)
}

/// The upgrade: the same store, the service built anew with the peer id.
fn upgrade(s: &mut Stack, peer: &str) {
    s.device = peer.into();
    s.restart();
}

// D3: stamps already stored keep their old `device` string; the upgrade (a new
// service with the peer id over the same store) rewrites nothing, on either
// device, for a document edited after it or for one never touched again.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d3_stored_stamps_keep_their_display_name_after_the_upgrade() {
    let mut a = Stack::with_device("item22-d3k-a", DISPLAY_NAME).await;
    let mut b = Stack::with_device("item22-d3k-b", DISPLAY_NAME).await;
    let (shared, untouched, old, old_untouched) = v0011_store(&a, &b).await;
    upgrade(&mut a, PEER_LAPTOP);
    upgrade(&mut b, PEER_DESK);

    for s in [&a, &b] {
        assert_eq!(current_stamp(s, &shared).await, old, "D3: {}: the stored stamp is not rewritten", s.device);
        assert_eq!(current_stamp(s, &untouched).await, old_untouched, "D3: {}: nor the untouched one's", s.device);
        assert_eq!(
            s.stored_doc(&untouched).await.author_device.as_deref().unwrap_or(DISPLAY_NAME),
            DISPLAY_NAME,
            "D3: {}: the stored author keeps the name",
            s.device
        );
    }

    // An edit after the upgrade: the new stamp carries the peer id, its
    // previous stamp names the old one exactly, and the old one stays in the
    // history as it was.
    a.clock.advance(Duration::seconds(5));
    a.editor_save(&shared, 1, v2(vec![text_block(1, "after the upgrade")])).await.unwrap();
    let st = a.svc.stored_state(&shared).await.unwrap();
    assert_eq!(st.meta.stamp.as_ref().map(|x| x.device.as_str()), Some(PEER_LAPTOP), "D3: a new stamp names the peer id");
    assert_eq!(st.meta.prev_stamp.as_ref(), Some(&old), "D3: its previous stamp is the old one, exactly");
    assert!(st.meta.history.contains(&old), "D3: the old stamp stays in the history, unchanged");
    assert_eq!(current_stamp(&a, &untouched).await, old_untouched, "D3: the untouched document is still not rewritten");
}

// D3: a document last stamped with a name before the upgrade and edited after
// it fast-forwards on a peer (its `prev` names the old stamp exactly): applied,
// not concurrent, no version kept as a conflict.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d3_an_edit_after_the_upgrade_fast_forwards_on_a_peer() {
    let mut a = Stack::with_device("item22-d3f-a", DISPLAY_NAME).await;
    let mut b = Stack::with_device("item22-d3f-b", DISPLAY_NAME).await;
    let (shared, _untouched, old, _) = v0011_store(&a, &b).await;
    upgrade(&mut a, PEER_LAPTOP);
    upgrade(&mut b, PEER_DESK);

    a.clock.advance(Duration::seconds(5));
    b.clock.advance(Duration::seconds(5));
    a.editor_save(&shared, 1, v2(vec![text_block(1, "after the upgrade")])).await.unwrap();
    let out = send(&a, &b, &shared).await;
    assert!(matches!(out.applied, WireApplied::Applied(_)), "D3: applied on the peer: {out:?}");
    assert!(!out.concurrent, "D3: a fast-forward, not a concurrent edit");
    assert_eq!(texts(&b.stored(&shared).await.1), vec!["after the upgrade"]);
    let commits = b.commits(&shared).await;
    assert!(
        !commits.iter().any(|c| c.message.starts_with("Concurrent edit from")),
        "D3: no conflict version: {:?}",
        commits.iter().map(|c| c.message.clone()).collect::<Vec<_>>()
    );
    let st = b.svc.stored_state(&shared).await.unwrap();
    assert!(st.meta.history.contains(&old), "D3: the peer keeps the old stamp in the history, unchanged");

    // And back: the peer's edit after the upgrade fast-forwards on the first
    // device too.
    b.clock.advance(Duration::seconds(5));
    b.editor_save(&shared, 2, v2(vec![text_block(1, "desk, after the upgrade")])).await.unwrap();
    let out = send(&b, &a, &shared).await;
    assert!(matches!(out.applied, WireApplied::Applied(_)) && !out.concurrent, "D3: the reply fast-forwards: {out:?}");
}

// D3: a document never edited after the upgrade, sent again by an upgraded
// device to an upgraded peer, is the same state (its old stamp, kept on both,
// is equal on both): ignored as known, nothing written — the identities that
// changed do not make an old state look new.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d3_an_untouched_old_state_sent_again_is_known_on_the_peer() {
    let mut a = Stack::with_device("item22-d3u-a", DISPLAY_NAME).await;
    let mut b = Stack::with_device("item22-d3u-b", DISPLAY_NAME).await;
    let (_shared, untouched, _old, _) = v0011_store(&a, &b).await;
    upgrade(&mut a, PEER_LAPTOP);
    upgrade(&mut b, PEER_DESK);
    let before = b.stored(&untouched).await;
    let out = send(&a, &b, &untouched).await;
    assert_eq!(out.applied, WireApplied::Ignored, "D3: the same old state is known");
    assert_eq!(b.stored(&untouched).await, before);
}

// D3: ordering stays by `wall_ms` and counter first; the device (a display
// name before the upgrade, a peer id after) only breaks ties. "Sovereign
// Device" sorts after any "12D3KooW…" id, so an order by device first would
// put every old stamp after every new one.
#[test]
fn d3_ordering_is_by_wall_ms_and_counter_before_the_device() {
    let old = |wall_ms, counter| Stamp { wall_ms, counter, device: DISPLAY_NAME.into() };
    let new = |wall_ms, counter| Stamp { wall_ms, counter, device: PEER_LAPTOP.into() };
    assert!(DISPLAY_NAME > PEER_LAPTOP, "precondition: the name sorts after the peer id");
    assert!(new(2_000, 0) > old(1_000, 0), "D3: a later wall_ms wins over the device");
    assert!(new(1_000, 1) > old(1_000, 0), "D3: at equal wall_ms, a greater counter wins over the device");
    assert!(old(2_000, 0) > new(1_000, 9), "D3: wall_ms first, whatever the counter");
    assert!(old(1_000, 0) > new(1_000, 0), "D3: only at equal (wall_ms, counter) does the device decide");
}

// D3: after the upgrade, the clock still covers the old stamps: a new write is
// later than any stamp stored before the upgrade, even when the wall clock is
// behind it (the restart reads the stored stamps, names included).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d3_a_write_after_the_upgrade_is_later_than_every_old_stamp() {
    let mut a = Stack::with_device("item22-d3c-a", DISPLAY_NAME).await;
    let b = Stack::with_device("item22-d3c-b", DISPLAY_NAME).await;
    let (shared, untouched, old, old_untouched) = v0011_store(&a, &b).await;
    upgrade(&mut a, PEER_LAPTOP);
    a.clock.advance(Duration::hours(-1));
    let fresh = a.create_text("New", "after the upgrade, clock behind").await;
    let s = current_stamp(&a, &fresh).await;
    assert_eq!(s.device, PEER_LAPTOP);
    assert!(s > old && s > old_untouched, "D3: {s:?} later than the old stamps ({old:?}, {old_untouched:?})");
    let _ = (shared, untouched);
}
