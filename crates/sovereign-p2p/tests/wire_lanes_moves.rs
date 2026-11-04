//! v0.0.12 lanes lot, item 15 — **M3: a move travels** (`doc/plans/V0012-lot-lanes.md`).
//!
//! "Céline's A6, as a two-device test: a document moved on A shows in its new
//! thread on B after sync; moved on both, the later move wins on both; a move
//! and an edit of the same document on two devices are concurrent states, as
//! for any write." (Her rc3 test A6, 2026-10-21: a move did not converge,
//! because the store's move did not change the stamp the wire orders by.)
//!
//! Two devices over the real sealed, signed envelopes, without the network
//! (`wire_common`): documents through the document plan (`round`/`settle`),
//! threads through the row protocol (what `node.rs` runs for
//! `SyncTable::Thread`), as in `wire_orphans.rs`.
//!
//! Written before the code, by another author; the implementer does not edit
//! this file. API named: `DocumentService::move_to_thread(&self, id, thread_id)`
//! (M1, `sovereign-docs/tests/lanes_m1_move.rs`).

mod wire_common;

use chrono::Duration;
use sovereign_db::GraphDB;
use sovereign_p2p::protocol::sync::SyncTable;
use sovereign_p2p::sync_engine::compute_thread_diff;
use wire_common::*;

/// One exchange of thread rows between `a` and `b`, both ways.
async fn thread_rows(a: &Device, b: &Device) {
    let (ma, mb) = (manifest(a).await, manifest(b).await);
    let diff = compute_thread_diff(&ma.threads, &mb.threads);
    if !diff.need_from_remote.is_empty() {
        let rows = b.sync.get_rows(SyncTable::Thread, &diff.need_from_remote, &a.peer).await.expect("rows from b");
        a.sync.apply_rows(SyncTable::Thread, rows, &b.peer).await.expect("rows on a");
    }
    if !diff.push_to_remote.is_empty() {
        let rows = a.sync.get_rows(SyncTable::Thread, &diff.push_to_remote, &b.peer).await.expect("rows from a");
        b.sync.apply_rows(SyncTable::Thread, rows, &a.peer).await.expect("rows on b");
    }
}

/// Thread rows, then documents until settled (bounded).
async fn full_sync(a: &Device, b: &Device) {
    for _ in 0..3 {
        thread_rows(a, b).await;
        settle(a, b, 4).await;
    }
}

async fn create_in(d: &Device, thread_id: &str, title: &str, text: &str) -> String {
    let new = sovereign_docs::NewDocument {
        title: title.into(),
        thread_id: thread_id.into(),
        content: sovereign_docs::NewContent::Text(text.into()),
        trust: sovereign_docs::Trust::Owned,
        provenance: sovereign_docs::Provenance::Written,
        origin: sovereign_docs::Origin::Editor { session: sovereign_docs::EditSession(1) },
        created_at: None,
        modified_at: None,
    };
    d.docs.create(new).await.unwrap().id
}

async fn thread_of(d: &Device, id: &str) -> String {
    d.docs.read(id).await.expect("present").thread_id
}

/// `a` (laptop) and `b` (phone) share threads "Inbox", "Garden" and "Kitchen",
/// and a document `doc` in "Inbox", synced.
struct Shared {
    a: Device,
    b: Device,
    inbox: String,
    garden: String,
    kitchen: String,
    doc: String,
}

async fn shared(tag: &str, seeds: (u8, u8)) -> Shared {
    let (a, b) = (device(&format!("{tag}-a"), seeds.0).await, device(&format!("{tag}-b"), seeds.1).await);
    pair(&a, &b);
    let inbox = a.docs.thread_create("Inbox", "").await.unwrap();
    let garden = a.docs.thread_create("Garden", "").await.unwrap();
    let kitchen = a.docs.thread_create("Kitchen", "").await.unwrap();
    let doc = create_in(&a, &inbox, "Seeds", "tomatoes").await;
    full_sync(&a, &b).await;
    for t in [&inbox, &garden, &kitchen] {
        assert!(b.edb.get_thread(t).await.is_ok_and(|t| t.deleted_at.is_none()), "setup: the threads on the phone");
    }
    assert_eq!(thread_of(&b, &doc).await, inbox, "setup: the document on the phone, in Inbox");
    Shared { a, b, inbox, garden, kitchen, doc }
}

/// M3: "a document moved on A shows in its new thread on B after sync" — and
/// the two devices then hold the same state, so a further round exchanges
/// nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn m3_a_document_moved_on_one_device_shows_in_its_new_thread_on_the_other() {
    let w = shared("m3-travels", (61, 62)).await;
    w.a.clock.advance(Duration::minutes(1));
    w.a.docs.move_to_thread(&w.doc, &w.garden).await.expect("moved on the laptop");
    full_sync(&w.a, &w.b).await;
    assert_eq!(thread_of(&w.b, &w.doc).await, w.garden, "the move reached the phone");
    assert_eq!(thread_of(&w.a, &w.doc).await, w.garden);
    let (sa, sb) = (w.a.docs.stored_state(&w.doc).await.unwrap(), w.b.docs.stored_state(&w.doc).await.unwrap());
    assert_eq!(sa.normalized_hash, sb.normalized_hash, "the same state on both");
    assert_eq!(sa.meta.stamp, sb.meta.stamp, "the move's stamp travelled");
    let plan = round(&w.a, &w.b).await;
    assert!(plan.fetch.is_empty() && plan.push.is_empty() && plan.concurrent.is_empty(), "converged: {plan:?}");
    // The other way: a move on the phone reaches the laptop.
    w.b.clock.advance(Duration::minutes(2));
    w.b.docs.move_to_thread(&w.doc, &w.kitchen).await.expect("moved on the phone");
    full_sync(&w.a, &w.b).await;
    assert_eq!(thread_of(&w.a, &w.doc).await, w.kitchen, "the phone's move reached the laptop");
    let _ = &w.inbox;
}

/// M3: "moved on both, the later move wins on both" (A6).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn m3_moved_on_both_devices_the_later_move_wins_on_both() {
    let w = shared("m3-later-wins", (63, 64)).await;
    // Apart: the laptop moves it to Garden, then — later — the phone to Kitchen.
    w.a.clock.advance(Duration::minutes(1));
    w.a.docs.move_to_thread(&w.doc, &w.garden).await.expect("moved on the laptop");
    w.b.clock.advance(Duration::minutes(2));
    w.b.docs.move_to_thread(&w.doc, &w.kitchen).await.expect("moved on the phone, later");
    full_sync(&w.a, &w.b).await;
    assert_eq!(thread_of(&w.a, &w.doc).await, w.kitchen, "laptop: the later move wins");
    assert_eq!(thread_of(&w.b, &w.doc).await, w.kitchen, "phone: the later move wins");

    // And the other order: the laptop's move is the later one. (Clocks stay
    // within C-WIRE 4's five minutes of each other: the phone is now at +3
    // minutes, the laptop moves at +4.)
    w.b.clock.advance(Duration::minutes(1));
    w.b.docs.move_to_thread(&w.doc, &w.inbox).await.expect("moved on the phone");
    w.a.clock.advance(Duration::minutes(3));
    w.a.docs.move_to_thread(&w.doc, &w.garden).await.expect("moved on the laptop, later");
    full_sync(&w.a, &w.b).await;
    assert_eq!(thread_of(&w.a, &w.doc).await, w.garden, "laptop: the later move wins");
    assert_eq!(thread_of(&w.b, &w.doc).await, w.garden, "phone: the later move wins");
}

/// M3: "a move and an edit of the same document on two devices are concurrent
/// states, as for any write" (C-WIRE 7: both devices end with the same current
/// state, the greater stamp, and the other state in History).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn m3_a_move_and_an_edit_on_two_devices_are_concurrent_states() {
    let w = shared("m3-move-edit", (65, 66)).await;
    w.a.clock.advance(Duration::minutes(1));
    w.a.docs.move_to_thread(&w.doc, &w.garden).await.expect("moved on the laptop");
    edit(&w.b, &w.doc, "phone edit").await;
    let plan = round(&w.a, &w.b).await;
    assert!(plan.concurrent.contains(&w.doc), "a move and an edit are concurrent: {plan:?}");
    full_sync(&w.a, &w.b).await;
    let (sa, sb) = (w.a.docs.stored_state(&w.doc).await.unwrap(), w.b.docs.stored_state(&w.doc).await.unwrap());
    assert_eq!(sa.normalized_hash, sb.normalized_hash, "the same current state on both");
    assert_eq!(thread_of(&w.a, &w.doc).await, thread_of(&w.b, &w.doc).await);
    for d in [&w.a, &w.b] {
        let mut all = history(d, &w.doc).await;
        all.push(stored_text(d, &w.doc).await);
        assert!(all.contains(&vec!["phone edit".to_string()]), "{}: the edit is kept (current or in History)", d.name);
    }
}
