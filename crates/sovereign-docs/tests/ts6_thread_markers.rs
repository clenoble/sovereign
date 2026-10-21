//! THREAD-sync S6 and S7 at the service (`doc/plans/THREAD-sync.md`, decided
//! by Céline 2026-10-18: "ID oui"; spec ch. 03, *Thread deletion and the
//! fallback lane*: "A deleted thread shrinks to its id after 30 days, once no
//! document (live or in the trash) points to it: its name is removed from
//! disk").
//!
//! S6: "After 30 days in the deleted state, and once **no document — live or
//! in the trash — points to it**, the tombstone becomes a **thread marker**:
//! the id and the deletion stamp only (name and description removed from
//! disk) … Run by the existing purge tick."
//! S7: "A marker appears in no list, no trash, no lane, no alert."
//!
//! The thread is deleted through the one path (`delete_thread_to_trash`) at a
//! time of the service's fake clock; the 30 days run from the deletion stamp's
//! physical time (S9, settled 2026-10-18), which comes from that clock (S1).
//! A deletion received late is in `wire_thread_sync.rs`
//! (`s9_a_deletion_received_late_shrinks_30_days_after_its_stamp`).
//! S11: a document arriving for a thread already shrunk asks, without a name;
//! the marker stays shown nowhere.
//! What sync does with a marker (refuse an older row, re-create from a newer
//! one, stay in the manifest) is in `sovereign-p2p/tests/wire_thread_sync.rs`.
//! The revised S5-12 W7 tests are in `s5_12_orphans.rs`.
//!
//! Index and red reasons: `crates/sovereign-p2p/TESTS-thread-sync.md`.

mod common;

use chrono::Duration;
use common::wire::*;
use common::*;
use sovereign_db::schema::Thread;
use sovereign_docs::{
    active_thread, delete_thread_to_trash, list_trash_named, preview_thread_delete, purge_tick, OrphanResolution,
    Stamp,
};

const NAME: &str = "Old project";
const DESCRIPTION: &str = "notes about the old project";

async fn thread_with(s: &Stack, name: &str, description: &str) -> String {
    s.svc.thread_create(name, description).await.expect("created")
}

async fn doc_in(s: &Stack, thread_id: &str, title: &str) -> String {
    let mut d = new_text_doc(title, title);
    d.thread_id = thread_id.to_string();
    s.svc.create(d).await.unwrap().id
}

/// A first state of `id` from the phone, for thread `thread_id` (a new arrival).
async fn arrive(s: &Stack, id: &str, thread_id: &str) {
    let mut r = remote_text(id, "From the phone", "phone text", "phone");
    r.thread_id = thread_id.to_string();
    s.svc.apply_wire(r, meta(Some(stamp(s, 1, 0, "phone")), None)).await.expect("applied");
}

async fn tick(s: &Stack) {
    purge_tick(&s.svc).await.expect("tick");
}

async fn deletion_stamp(s: &Stack, id: &str) -> Stamp {
    s.svc.thread_stamp(id).await.expect("read").expect("the deletion is stamped")
}

async fn record(s: &Stack, id: &str) -> Option<Thread> {
    s.edb.list_threads_including_deleted().await.unwrap().into_iter().find(|t| t.id_string().as_deref() == Some(id))
}

/// Still a tombstone with its name (not shrunk).
async fn assert_named(s: &Stack, id: &str, what: &str) {
    let t = record(s, id).await.unwrap_or_else(|| panic!("{what}: the tombstone is there"));
    assert!(t.deleted_at.is_some(), "{what}: still deleted");
    assert_eq!(t.name, NAME, "{what}: with its name");
}

/// S6 and S7: thread `id` is a marker — the id and the deletion stamp only,
/// name and description gone from the store, shown nowhere.
async fn assert_marker(s: &Stack, id: &str, deletion: &Stamp, what: &str) {
    assert_eq!(s.svc.thread_stamp(id).await.expect("read").as_ref(), Some(deletion), "{what}: the deletion stamp, unchanged");
    if let Some(t) = record(s, id).await {
        assert!(t.deleted_at.is_some(), "{what}: still deleted");
        assert!(t.name.is_empty() && t.description.is_empty(), "{what}: no name, no description");
    }
    if let Ok(raw) = s.raw.get_thread(id).await {
        let text = serde_json::to_string(&raw).unwrap();
        assert!(!text.contains(NAME) && !text.contains(DESCRIPTION), "{what}: plaintext on disk");
        assert!(raw.name_token_hashes.is_empty(), "{what}: the name's blind index is gone");
    }
    // S7: no list, no lane, no trash, no alert.
    assert!(!s.edb.list_threads().await.unwrap().iter().any(|t| t.id_string().as_deref() == Some(id)), "{what}: in no list");
    assert!(active_thread(s.edb.as_ref(), id).await.unwrap().is_none(), "{what}: no lane");
    assert!(s.edb.find_thread_by_name(NAME).await.unwrap().is_none(), "{what}: not found by its old name");
    assert!(preview_thread_delete(&s.svc, s.edb.as_ref(), id).await.is_err(), "{what}: nothing to delete");
    let trash = list_trash_named(&s.svc, s.edb.as_ref(), s.clock.get()).await.unwrap();
    assert!(
        trash.iter().all(|l| l.entry.thread_id != id && l.deleted_thread_name.as_deref() != Some(NAME)),
        "{what}: not in the trash"
    );
    assert!(s.svc.orphans_of_deleted_threads().await.unwrap().iter().all(|o| o.thread_id != id), "{what}: nothing asks");
    assert!(s.svc.alerts().await.unwrap().is_empty(), "{what}: no alert");
}

/// S6: nothing points to it — kept with its name before 30 days, a marker
/// after, by the device clock.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s6_a_deleted_thread_nothing_points_to_shrinks_after_30_days() {
    let s = Stack::new("ts6-bare").await;
    let id = thread_with(&s, NAME, DESCRIPTION).await;
    s.clock.advance(Duration::minutes(1));
    delete_thread_to_trash(&s.svc, s.edb.as_ref(), &id).await.unwrap();
    let deletion = deletion_stamp(&s, &id).await;

    s.clock.advance(Duration::days(29));
    tick(&s).await;
    assert_named(&s, &id, "29 days after the deletion").await;

    s.clock.advance(Duration::days(2));
    tick(&s).await;
    assert_marker(&s, &id, &deletion, "31 days after the deletion").await;
    // A further tick changes nothing.
    s.clock.advance(Duration::days(3650));
    tick(&s).await;
    assert_marker(&s, &id, &deletion, "ten years on").await;
}

/// S6: a live document points to it (a document of it arrived after the
/// deletion and is waiting for a decision, S5-12) — the name stays, however
/// long; once it is resolved and nothing points any more, a marker.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s6_a_live_document_keeps_the_name_until_it_is_resolved() {
    let s = Stack::new("ts6-live").await;
    let id = thread_with(&s, NAME, DESCRIPTION).await;
    let keep = thread_with(&s, "Keep", "").await;
    delete_thread_to_trash(&s.svc, s.edb.as_ref(), &id).await.unwrap();
    let deletion = deletion_stamp(&s, &id).await;
    s.clock.advance(Duration::minutes(1));
    arrive(&s, "document:live1", &id).await;

    for days in [31, 3650] {
        s.clock.advance(Duration::days(days));
        tick(&s).await;
        assert_named(&s, &id, &format!("+{days} days, a live document points")).await;
        let o = s.svc.orphans_of_deleted_threads().await.unwrap();
        assert_eq!(o.len(), 1, "still asking");
        assert_eq!(o[0].deleted_thread_name.as_deref(), Some(NAME), "with the old name");
    }

    s.svc.resolve_orphan("document:live1", OrphanResolution::MoveTo(keep.clone())).await.unwrap();
    s.clock.advance(Duration::days(1));
    tick(&s).await;
    tick(&s).await;
    assert_marker(&s, &id, &deletion, "once the last document was moved out").await;
    assert_eq!(s.svc.read("document:live1").await.unwrap().thread_id, keep, "the moved document untouched");
}

/// S6: "live **or in the trash**" — a trashed document of it keeps the name
/// (the trash shows "<old name> - deleted") until that document is purged.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s6_a_trashed_document_keeps_the_name_until_it_is_purged() {
    let s = Stack::new("ts6-trashed").await;
    let id = thread_with(&s, NAME, DESCRIPTION).await;
    delete_thread_to_trash(&s.svc, s.edb.as_ref(), &id).await.unwrap();
    let deletion = deletion_stamp(&s, &id).await;
    // Twenty days later a document of it arrives and goes to the trash.
    s.clock.advance(Duration::days(20));
    arrive(&s, "document:late1", &id).await;
    s.svc.trash("document:late1").await.unwrap();

    s.clock.advance(Duration::days(11));
    tick(&s).await;
    assert_named(&s, &id, "31 days after the deletion, its document 11 days in the trash").await;
    let trash = list_trash_named(&s.svc, s.edb.as_ref(), s.clock.get()).await.unwrap();
    let line = trash.iter().find(|l| l.entry.id == "document:late1").expect("in the trash");
    assert_eq!(line.deleted_thread_name.as_deref(), Some(NAME), "the trash still names the deleted thread");

    s.clock.advance(Duration::days(20));
    tick(&s).await;
    tick(&s).await;
    assert!(s.svc.is_purged("document:late1").await.unwrap(), "the trashed document went with its 30 days");
    assert_marker(&s, &id, &deletion, "once its last document was purged").await;
}

/// S6: a thread deleted with its documents (they went to the trash with it)
/// shrinks once they are purged — the usual path, nothing arriving later.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s6_a_thread_deleted_with_its_documents_shrinks_once_they_are_purged() {
    let s = Stack::new("ts6-with-docs").await;
    let id = thread_with(&s, NAME, DESCRIPTION).await;
    let docs = [doc_in(&s, &id, "one").await, doc_in(&s, &id, "two").await];
    delete_thread_to_trash(&s.svc, s.edb.as_ref(), &id).await.unwrap();
    let deletion = deletion_stamp(&s, &id).await;
    s.clock.advance(Duration::days(15));
    tick(&s).await;
    assert_named(&s, &id, "15 days, its documents in the trash").await;
    s.clock.advance(Duration::days(3650));
    tick(&s).await;
    tick(&s).await;
    for d in &docs {
        assert!(s.svc.is_purged(d).await.unwrap(), "{d} purged");
    }
    assert_marker(&s, &id, &deletion, "its documents purged").await;
}

/// S11: "A document arriving for a thread already shrunk to its id still asks
/// (S5-12), worded without a name … The marker itself is still shown nowhere
/// (S7); the document is." At the service: listed with no old name, live and
/// in the lists; the marker keeps no name and gets no lane. (The wording
/// "<title> — its thread was deleted." is the hosts' — shell `orphans.rs`,
/// phone `syncAlerts.svelte.ts` — for a line with no old name.)
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s11_a_document_arriving_for_a_thread_shrunk_to_its_id_asks_without_a_name() {
    let s = Stack::new("ts6-s11").await;
    let id = thread_with(&s, NAME, DESCRIPTION).await;
    delete_thread_to_trash(&s.svc, s.edb.as_ref(), &id).await.unwrap();
    let deletion = deletion_stamp(&s, &id).await;
    s.clock.advance(Duration::days(31));
    tick(&s).await;
    tick(&s).await;
    assert_marker(&s, &id, &deletion, "before the arrival").await;

    arrive(&s, "document:after1", &id).await;
    let asks = s.svc.orphans_of_deleted_threads().await.unwrap();
    assert_eq!(asks.len(), 1, "the arriving document asks");
    assert_eq!((asks[0].id.as_str(), asks[0].thread_id.as_str()), ("document:after1", id.as_str()));
    assert_eq!(asks[0].deleted_thread_name, None, "no name: the thread shrank to its id");
    assert!(s.svc.list(None).await.unwrap().iter().any(|d| d.id == "document:after1"), "the document is shown");
    assert_eq!(s.svc.thread_stamp(&id).await.unwrap().as_ref(), Some(&deletion), "the marker is unchanged");
    if let Some(t) = record(&s, &id).await {
        assert!(t.deleted_at.is_some() && t.name.is_empty(), "no name comes back");
    }
    assert!(active_thread(s.edb.as_ref(), &id).await.unwrap().is_none(), "no lane for the marker");
    // While the document points to it, the next ticks leave the marker as it is.
    s.clock.advance(Duration::days(31));
    tick(&s).await;
    assert_eq!(s.svc.orphans_of_deleted_threads().await.unwrap().len(), 1, "still asking");
}

/// S6 concerns deleted threads only: a live thread, empty or not, is never
/// touched by the tick.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s6_a_live_thread_is_never_shrunk() {
    let s = Stack::new("ts6-live-thread").await;
    let id = thread_with(&s, NAME, DESCRIPTION).await;
    let before = s.svc.thread_stamp(&id).await.unwrap();
    s.clock.advance(Duration::days(3650));
    tick(&s).await;
    let t = record(&s, &id).await.expect("there");
    assert!(t.deleted_at.is_none());
    assert_eq!((t.name.as_str(), t.description.as_str()), (NAME, DESCRIPTION));
    assert_eq!(s.svc.thread_stamp(&id).await.unwrap(), before, "the tick writes nothing");
}
