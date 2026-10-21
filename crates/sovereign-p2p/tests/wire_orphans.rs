//! S5-12 W2 — both directions over sync (`doc/plans/C-DOC-adoption.md`,
//! *S5-12 — a live document in a deleted thread asks: contract*), and W4's
//! resolutions travelling to the other device.
//!
//! W2: "After a sync apply (`apply_remote` / the C-WIRE path): a document
//! arriving for a thread deleted here is listed; a thread tombstone arriving
//! for a thread that still holds live documents here lists those documents —
//! they are **not** trashed by the sync."
//!
//! Two devices over the real sealed, signed envelopes, without the network
//! (`wire_common`): documents through the document plan (`round`), threads
//! through the row protocol (`thread_rows`, what `node.rs` runs for
//! `SyncTable::Thread`: the manifest diff, `get_rows`, `apply_rows`). The
//! thread is deleted the way a host deletes it (`delete_thread_to_trash`).

mod wire_common;

use sovereign_db::schema::Thread;
use sovereign_db::GraphDB;
use sovereign_docs::{delete_thread_to_trash, OrphanResolution};
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

async fn new_thread(d: &Device, name: &str) -> String {
    d.edb.create_thread(Thread::new(name.into(), String::new())).await.unwrap().id_string().unwrap()
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

async fn orphan_ids(d: &Device) -> Vec<String> {
    let mut v: Vec<String> =
        d.docs.orphans_of_deleted_threads().await.expect("the condition").into_iter().map(|o| o.id).collect();
    v.sort();
    v
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

async fn live(d: &Device, id: &str) -> bool {
    d.docs.read(id).await.expect("present").trashed_at.is_none()
}

async fn thread_deleted_on(d: &Device, id: &str) -> bool {
    d.edb.list_threads_including_deleted().await.unwrap().iter().any(|t| t.id_string().as_deref() == Some(id) && t.deleted_at.is_some())
}

/// `a` (laptop) and `b` (phone) share thread "Shared" holding `shared`, synced.
/// Then, apart: `b` writes `offline` in "Shared"; `a` deletes "Shared"
/// (`shared` goes to the trash there).
struct Apart {
    a: Device,
    b: Device,
    lane: String,
    shared: String,
    offline: String,
}

async fn apart(tag: &str) -> Apart {
    let (a, b) = (device(&format!("{tag}-a"), 1).await, device(&format!("{tag}-b"), 2).await);
    pair(&a, &b);
    let lane = new_thread(&a, "Shared").await;
    let shared = create_in(&a, &lane, "Shared doc", "both have it").await;
    full_sync(&a, &b).await;
    assert!(live(&b, &shared).await, "setup: synced to the phone");
    assert!(b.edb.get_thread(&lane).await.is_ok_and(|t| t.deleted_at.is_none()), "setup: the thread on the phone");

    let offline = create_in(&b, &lane, "Written offline", "phone text").await;
    delete_thread_to_trash(&a.docs, a.edb.as_ref(), &lane).await.expect("deleted on the laptop");
    assert!(orphan_ids(&a).await.is_empty() && orphan_ids(&b).await.is_empty(), "setup: nothing asks before sync");
    Apart { a, b, lane, shared, offline }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn w2_a_document_arriving_for_a_thread_deleted_here_is_listed() {
    let w = apart("w2-arrive").await;
    // Documents only: the phone's offline document reaches the laptop.
    settle(&w.a, &w.b, 4).await;
    assert!(live(&w.a, &w.offline).await, "applied live, not trashed by the sync");
    assert_eq!(w.a.docs.read(&w.offline).await.unwrap().thread_id, w.lane);
    assert_eq!(orphan_ids(&w.a).await, vec![w.offline.clone()], "the laptop asks about it");
    let o = w.a.docs.orphans_of_deleted_threads().await.unwrap().remove(0);
    assert_eq!(o.deleted_thread_name.as_deref(), Some("Shared"));
    assert_eq!(o.device, w.b.peer.to_string(), "it came from the phone");
    // And it stays in the laptop's lists (W3).
    assert!(w.a.docs.list(None).await.unwrap().iter().any(|d| d.id == w.offline));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn w2_a_thread_deletion_arriving_lists_the_live_documents_and_trashes_none() {
    let w = apart("w2-tomb").await;
    // Thread rows only: the laptop's tombstone reaches the phone first.
    thread_rows(&w.a, &w.b).await;
    assert!(thread_deleted_on(&w.b, &w.lane).await, "the deletion arrived");
    assert!(live(&w.b, &w.offline).await && live(&w.b, &w.shared).await, "the sync trashed nothing");
    assert_eq!(
        orphan_ids(&w.b).await,
        sorted(vec![w.offline.clone(), w.shared.clone()]),
        "the phone asks about every live document of the thread"
    );

    // Then the documents: `shared`'s trash (the laptop's own delete) arrives
    // and ends its question; the offline document stays live and asking.
    settle(&w.a, &w.b, 4).await;
    assert!(!live(&w.b, &w.shared).await, "the laptop's delete of `shared` travelled");
    assert!(live(&w.b, &w.offline).await, "never trashed by the sync");
    assert_eq!(orphan_ids(&w.b).await, vec![w.offline.clone()]);
    assert_eq!(orphan_ids(&w.a).await, vec![w.offline.clone()], "both devices ask about it");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn w2_after_a_full_sync_both_devices_ask_and_nothing_is_lost() {
    let w = apart("w2-full").await;
    full_sync(&w.a, &w.b).await;
    for d in [&w.a, &w.b] {
        assert_eq!(orphan_ids(d).await, vec![w.offline.clone()], "{}: asks", d.name);
        assert!(live(d, &w.offline).await, "{}: live", d.name);
        assert_eq!(texts_of(d, &w.offline).await, vec!["phone text".to_string()], "{}: intact", d.name);
        assert!(thread_deleted_on(d, &w.lane).await, "{}: the thread stays deleted (never restored)", d.name);
    }
}

async fn texts_of(d: &Device, id: &str) -> Vec<String> {
    stored_text(d, id).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn w4_a_move_into_a_new_thread_travels_and_ends_the_question_on_both_devices() {
    let w = apart("w4-new-sync").await;
    full_sync(&w.a, &w.b).await;
    let done = w
        .a
        .docs
        .resolve_orphan(&w.offline, OrphanResolution::NewThread("Brought back".into()))
        .await
        .expect("resolved on the laptop");
    let new_id = done.thread_id.expect("the new thread");
    full_sync(&w.a, &w.b).await;
    for d in [&w.a, &w.b] {
        assert!(orphan_ids(d).await.is_empty(), "{}: no longer asks", d.name);
        assert_eq!(d.docs.read(&w.offline).await.unwrap().thread_id, new_id, "{}: in the new thread", d.name);
        let t = d.edb.get_thread(&new_id).await.expect("the new thread travelled");
        assert_eq!((t.name.as_str(), t.deleted_at.is_none()), ("Brought back", true), "{}", d.name);
        assert!(live(d, &w.offline).await);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn w4_trash_from_the_phone_travels_to_the_laptop() {
    let w = apart("w4-trash-sync").await;
    full_sync(&w.a, &w.b).await;
    w.b.docs.resolve_orphan(&w.offline, OrphanResolution::Trash).await.expect("trashed on the phone");
    full_sync(&w.a, &w.b).await;
    for d in [&w.a, &w.b] {
        assert!(orphan_ids(d).await.is_empty(), "{}: no longer asks", d.name);
        assert!(!live(d, &w.offline).await, "{}: in the trash", d.name);
        assert!(d.docs.list_trash().await.unwrap().iter().any(|e| e.id == w.offline), "{}: listed in the trash", d.name);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn w4_a_move_to_an_existing_thread_travels() {
    let w = apart("w4-move-sync").await;
    let keep = new_thread(&w.a, "Keep").await;
    full_sync(&w.a, &w.b).await;
    w.b.docs.resolve_orphan(&w.offline, OrphanResolution::MoveTo(keep.clone())).await.expect("moved on the phone");
    full_sync(&w.a, &w.b).await;
    for d in [&w.a, &w.b] {
        assert!(orphan_ids(d).await.is_empty(), "{}", d.name);
        assert_eq!(d.docs.read(&w.offline).await.unwrap().thread_id, keep, "{}: in Keep", d.name);
    }
}
