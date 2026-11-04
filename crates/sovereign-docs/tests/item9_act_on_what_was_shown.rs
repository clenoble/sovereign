//! v0.0.12 item 9 — trash and orphan actions act on what was shown; Delete
//! says what it did (`doc/plans/V0012-item9-act-on-what-was-shown.md`, T1–T3,
//! the service half). Review of v0.0.11, R15.
//!
//! Written before the code, by another author: whoever builds item 9 makes
//! these pass **without editing this file**. A test that looks wrong is
//! reported, not worked around.
//!
//! **Seams this file names** (they do not exist at `c7e571b`, so this file
//! does not compile until they are built):
//!
//! - `DocumentService::empty_trash_of(&self, shown: &[DocId]) ->
//!   Result<EmptiedTrash, DocError>` — T1: purges exactly those of `shown`
//!   still in the trash ("[choice] The service API takes the ids (for example
//!   `empty_trash_of(&[ids])`)").
//! - `sovereign_docs::EmptiedTrash { pub purged: PurgeReport, pub skipped:
//!   Vec<DocId> }` — "The report counts what was purged" (`purged`) and "names
//!   the skipped ones" (`skipped`: shown documents left alone because they
//!   were no longer in the trash, for any reason — restored, or purged
//!   elsewhere; contract cb5a859).
//! - `DocumentService::trash(&self, id) -> Result<TrashOutcome, DocError>` —
//!   T3: "`trash` tells 'moved to the trash' apart from 'was already in the
//!   trash'".
//! - `sovereign_docs::TrashOutcome { Trashed, AlreadyInTrash }` (`Debug`,
//!   `PartialEq`).
//! - `DocumentService::resolve_orphans_shown(&self, deleted_thread_id: &str,
//!   shown: &[DocId], resolution: OrphanResolution) -> Result<OrphanResolved,
//!   DocError>` — T2: the group's action on "the ids the group listed when it
//!   was drawn, those of them still waiting"; `OrphanResolved::documents` is
//!   what was acted on ("The result says how many were acted on").
//!
//! How a peer's change arrives: through the wire, as in the item-3 tests — a
//! document shared from this device (`laptop`) to the phone, the phone's
//! change exported and applied here (`common::wire::send`); a document written
//! on the phone for a thread deleted here, applied by `apply_wire` (as in
//! `s5_12_orphans.rs`).

mod common;

use chrono::Duration;
use common::wire::*;
use common::*;
use sovereign_db::schema::Thread;
use sovereign_docs::{
    delete_thread_to_trash, EmptiedTrash, OrphanResolution, OrphanResolved, RemoteDocument, TrashOutcome, WireApplied,
};

// ======================================================================= fixtures

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

async fn trash_ids(s: &Stack) -> Vec<String> {
    sorted(s.svc.list_trash().await.unwrap().into_iter().map(|e| e.id).collect())
}

/// What the trash view lists when it is drawn: the ids, as shown.
async fn shown_trash(s: &Stack) -> Vec<String> {
    s.svc.list_trash().await.unwrap().into_iter().map(|e| e.id).collect()
}

/// Every outcome, exhaustively: a variant added or missing fails to compile.
fn said(o: &TrashOutcome) -> &'static str {
    match o {
        TrashOutcome::Trashed => "moved to the trash",
        TrashOutcome::AlreadyInTrash => "already in the trash",
    }
}

/// A document written here and received on the phone (both hold the same state).
async fn shared(a: &Stack, b: &Stack, title: &str) -> String {
    let id = a.create_text(title, &format!("{title} text")).await;
    let out = send(a, b, &id).await;
    assert!(matches!(out.applied, WireApplied::Applied(_)), "setup: {:?}", out.applied);
    id
}

/// The phone deletes `id` and the deletion reaches this device (`a`).
async fn peer_deletes(a: &Stack, phone: &Stack, id: &str) {
    phone.clock.advance(Duration::minutes(1));
    a.clock.advance(Duration::minutes(1));
    phone.svc.trash(id).await.expect("the phone's delete");
    let _ = send(phone, a, id).await;
    assert!(a.svc.read(id).await.unwrap().trashed_at.is_some(), "setup: the peer's deletion was applied here");
}

/// The laptop (`a`, the device under test) and the phone, sharing `titles`.
async fn two_devices(tag: &str, titles: &[&str]) -> (Stack, Stack, Vec<String>) {
    let a = Stack::with_device(&format!("{tag}-a"), "laptop").await;
    let b = Stack::with_device(&format!("{tag}-b"), "phone").await;
    let mut ids = Vec::new();
    for t in titles {
        ids.push(shared(&a, &b, t).await);
    }
    (a, b, ids)
}

// ======================================================================= T1

// T1 — "The confirm acts on the ids the trash view listed when it was drawn.
// The service purges exactly those of them that are still in the trash. A
// document that entered the trash after the view was drawn is not purged: it
// stays, and the next view lists it."
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn t1_a_document_a_peer_trashed_after_the_view_was_drawn_is_not_purged() {
    let (a, phone, ids) = two_devices("t1-peer", &["A", "B", "C"]).await;
    let (da, db, dc) = (ids[0].clone(), ids[1].clone(), ids[2].clone());
    a.svc.trash(&da).await.unwrap();
    a.svc.trash(&db).await.unwrap();

    // The view is drawn: A and B.
    let shown = shown_trash(&a).await;
    assert_eq!(sorted(shown.clone()), sorted(vec![da.clone(), db.clone()]), "setup: the view shows A and B");

    // Meanwhile the phone deletes C, and its deletion is applied here.
    peer_deletes(&a, &phone, &dc).await;
    assert_eq!(trash_ids(&a).await, sorted(vec![da.clone(), db.clone(), dc.clone()]), "setup: C entered the trash");

    // The confirm, with what was shown.
    let EmptiedTrash { purged, skipped } = a.svc.empty_trash_of(&shown).await.expect("emptied");
    assert_eq!(sorted(purged.documents.clone()), sorted(vec![da.clone(), db.clone()]), "exactly what was shown");
    assert!(skipped.is_empty(), "nothing shown was skipped: {skipped:?}");
    for id in [&da, &db] {
        assert!(a.svc.is_purged(id).await.unwrap(), "{id} purged");
    }
    assert!(!a.svc.is_purged(&dc).await.unwrap(), "C was never shown: not purged");
    assert!(a.svc.read(&dc).await.unwrap().trashed_at.is_some(), "C stays in the trash");
    assert_eq!(shown_trash(&a).await, vec![dc.clone()], "the next view lists C");
}

// T1 — "A shown document that is no longer in the trash (restored or purged
// meanwhile, here or elsewhere) is left alone. The report counts what was
// purged. [choice] The confirm's message names the skipped ones."
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn t1_a_shown_document_restored_meanwhile_here_is_left_alone_and_reported_skipped() {
    let s = Stack::new("t1-restored-here").await;
    let x = s.create_text("X", "x text").await;
    let y = s.create_text("Y", "y text").await;
    s.svc.trash(&x).await.unwrap();
    s.svc.trash(&y).await.unwrap();
    let shown = shown_trash(&s).await;

    // Restored meanwhile (from another window), then the confirm.
    s.svc.restore_from_trash(&x).await.unwrap();
    let before = s.stored_doc(&x).await;
    let stamp_before = current_stamp(&s, &x).await;

    let out = s.svc.empty_trash_of(&shown).await.expect("emptied");
    assert_eq!(out.purged.documents, vec![y.clone()], "only Y, still in the trash, is purged");
    assert_eq!(out.skipped, vec![x.clone()], "X is reported as skipped");
    assert!(!s.svc.is_purged(&x).await.unwrap());
    let after = s.stored_doc(&x).await;
    assert!(after.deleted_at.is_none(), "X stays live");
    assert_eq!((after.title, after.content), (before.title, before.content), "X untouched");
    assert_eq!(current_stamp(&s, &x).await, stamp_before, "X not written");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn t1_a_shown_document_restored_meanwhile_on_the_phone_is_left_alone() {
    let (a, phone, ids) = two_devices("t1-restored-peer", &["X", "Y"]).await;
    let (x, y) = (ids[0].clone(), ids[1].clone());
    // Both deleted on the phone; the deletions reach this device; the view is drawn.
    peer_deletes(&a, &phone, &x).await;
    peer_deletes(&a, &phone, &y).await;
    let shown = shown_trash(&a).await;
    assert_eq!(sorted(shown.clone()), sorted(vec![x.clone(), y.clone()]));

    // The phone restores X; its restore reaches this device before the confirm.
    phone.clock.advance(Duration::minutes(1));
    a.clock.advance(Duration::minutes(1));
    phone.svc.restore_from_trash(&x).await.unwrap();
    let _ = send(&phone, &a, &x).await;
    assert!(a.svc.read(&x).await.unwrap().trashed_at.is_none(), "setup: the phone's restore applied here");

    let out = a.svc.empty_trash_of(&shown).await.expect("emptied");
    assert_eq!(out.purged.documents, vec![y.clone()]);
    assert_eq!(out.skipped, vec![x.clone()]);
    assert!(!a.svc.is_purged(&x).await.unwrap(), "X, restored elsewhere, is not purged");
    assert!(a.svc.read(&x).await.unwrap().trashed_at.is_none(), "X stays live");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn t1_a_shown_document_purged_meanwhile_is_left_alone_and_not_counted() {
    let s = Stack::new("t1-purged-meanwhile").await;
    let x = s.create_text("X", "x text").await;
    let y = s.create_text("Y", "y text").await;
    s.svc.trash(&x).await.unwrap();
    s.svc.trash(&y).await.unwrap();
    let shown = shown_trash(&s).await;

    // X purged meanwhile (another window's per-item purge).
    s.svc.purge_from_trash(&x).await.unwrap();

    let out = s.svc.empty_trash_of(&shown).await.expect("a purged shown document is no error");
    assert_eq!(out.purged.documents, vec![y.clone()], "the report counts what this confirm purged");
    assert!(s.svc.is_purged(&x).await.unwrap() && s.svc.is_purged(&y).await.unwrap());
    // Settled (contract cb5a859): skipped = the shown ids no longer in the
    // trash, for any reason — purged elsewhere included.
    assert_eq!(out.skipped, vec![x.clone()], "X, purged meanwhile, is reported as skipped");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn t1_live_documents_and_ids_never_in_the_trash_are_left_alone() {
    let s = Stack::new("t1-live").await;
    let live = s.create_text("Live", "live text").await;
    let x = s.create_text("X", "x text").await;
    s.svc.trash(&x).await.unwrap();
    let stamp = current_stamp(&s, &live).await;

    // A shown list that (wrongly, or by a race) names a live document.
    let out = s.svc.empty_trash_of(&[x.clone(), live.clone()]).await.expect("emptied");
    assert_eq!(out.purged.documents, vec![x.clone()]);
    assert!(s.svc.read(&live).await.unwrap().trashed_at.is_none(), "a live document is never purged by it");
    assert_eq!(current_stamp(&s, &live).await, stamp);

    // Nothing shown, nothing purged.
    let y = s.create_text("Y", "y text").await;
    s.svc.trash(&y).await.unwrap();
    let none = s.svc.empty_trash_of(&[]).await.expect("an empty view");
    assert!(none.purged.documents.is_empty(), "an empty view purges nothing");
    assert_eq!(shown_trash(&s).await, vec![y.clone()]);
}

// "Unchanged: what a purge removes." A guard: the shown purge removes what
// `empty_trash` removed (the versions go with the document).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn t1_what_a_shown_purge_removes_is_unchanged() {
    let s = Stack::new("t1-what").await;
    let x = s.create_text("X", "x text").await;
    assert!(!s.raw_commits(&x).await.is_empty(), "setup: X has a version");
    s.svc.trash(&x).await.unwrap();
    let out = s.svc.empty_trash_of(&shown_trash(&s).await).await.unwrap();
    assert_eq!(out.purged.documents, vec![x.clone()]);
    assert!(out.purged.commits >= 1, "its versions are purged with it");
    assert!(s.raw_commits(&x).await.is_empty(), "no version left");
    assert!(s.svc.read(&x).await.is_err(), "gone");
}

// ======================================================================= T2

async fn thread(s: &Stack, name: &str) -> String {
    s.edb.create_thread(Thread::new(name.into(), String::new())).await.unwrap().id_string().unwrap()
}

/// A state written on the phone, for thread `thread_id`.
fn from_phone(id: &str, thread_id: &str, title: &str) -> RemoteDocument {
    let mut r = remote_text(id, title, &format!("{title} text"), "phone");
    r.thread_id = thread_id.to_string();
    r
}

/// The waiting documents, as the section lists them when it is drawn.
async fn waiting(s: &Stack) -> Vec<String> {
    sorted(s.svc.orphans_of_deleted_threads().await.unwrap().into_iter().map(|o| o.id).collect())
}

/// "Old drafts" deleted here, then two documents for it arrive from the phone
/// (`x`, `y`): the group as shown. "Keep" is live.
struct Group {
    s: Stack,
    gone: String,
    keep: String,
    x: String,
    y: String,
}

async fn group(tag: &str) -> Group {
    let s = Stack::new(tag).await;
    let gone = thread(&s, "Old drafts").await;
    let keep = thread(&s, "Keep").await;
    delete_thread_to_trash(&s.svc, s.edb.as_ref(), &gone).await.expect("thread deleted");
    s.clock.advance(Duration::minutes(1));
    let (x, y) = ("document:shownx".to_string(), "document:showny".to_string());
    for (id, t) in [(&x, "X"), (&y, "Y")] {
        s.svc.apply_wire(from_phone(id, &gone, t), meta(Some(stamp(&s, 1, 0, "phone")), None)).await.unwrap();
    }
    assert_eq!(waiting(&s).await, sorted(vec![x.clone(), y.clone()]), "setup: the group shows X and Y");
    Group { s, gone, keep, x, y }
}

/// A third document for the deleted thread arrives after the group was drawn.
async fn late_arrival(g: &Group) -> String {
    let z = "document:latez".to_string();
    g.s.clock.advance(Duration::minutes(1));
    g.s.svc.apply_wire(from_phone(&z, &g.gone, "Z"), meta(Some(stamp(&g.s, 1, 0, "phone")), None)).await.unwrap();
    assert!(waiting(&g.s).await.contains(&z), "setup: Z joined the group");
    z
}

fn resolved_ids(r: &OrphanResolved) -> Vec<String> {
    sorted(r.documents.clone())
}

// T2 — "'Trash all' applies to the ids the group listed when it was drawn,
// those of them still waiting. A document that joined the group after is left
// waiting, and is listed at the next refresh. The result says how many were
// acted on."
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn t2_trash_all_leaves_a_document_that_joined_later_waiting() {
    let g = group("t2-trash").await;
    let shown = vec![g.x.clone(), g.y.clone()];
    let z = late_arrival(&g).await;
    let z_stamp = current_stamp(&g.s, &z).await;

    let r = g.s.svc.resolve_orphans_shown(&g.gone, &shown, OrphanResolution::Trash).await.expect("resolved");
    assert_eq!(resolved_ids(&r), sorted(shown.clone()), "acted on what was shown: {r:?}");
    assert_eq!(r.thread_id, None);
    for id in &shown {
        assert!(g.s.svc.read(id).await.unwrap().trashed_at.is_some(), "{id} in the trash");
    }
    assert!(g.s.svc.read(&z).await.unwrap().trashed_at.is_none(), "Z was not shown: not trashed");
    assert_eq!(current_stamp(&g.s, &z).await, z_stamp, "Z not written");
    assert_eq!(waiting(&g.s).await, vec![z.clone()], "Z is listed at the next refresh");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn t2_move_all_to_a_thread_leaves_a_document_that_joined_later_waiting() {
    let g = group("t2-move").await;
    let shown = vec![g.x.clone(), g.y.clone()];
    let z = late_arrival(&g).await;

    let r = g.s.svc.resolve_orphans_shown(&g.gone, &shown, OrphanResolution::MoveTo(g.keep.clone())).await.unwrap();
    assert_eq!(resolved_ids(&r), sorted(shown.clone()));
    assert_eq!(r.thread_id.as_deref(), Some(g.keep.as_str()));
    for id in &shown {
        assert_eq!(g.s.stored_doc(id).await.thread_id, g.keep, "{id} moved");
    }
    assert_eq!(g.s.stored_doc(&z).await.thread_id, g.gone, "Z stays in the deleted thread");
    assert_eq!(waiting(&g.s).await, vec![z.clone()]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn t2_all_in_a_new_thread_leaves_a_document_that_joined_later_waiting() {
    let g = group("t2-new").await;
    let shown = vec![g.x.clone(), g.y.clone()];
    let z = late_arrival(&g).await;

    let r = g.s.svc.resolve_orphans_shown(&g.gone, &shown, OrphanResolution::NewThread("Revived".into())).await.unwrap();
    assert_eq!(resolved_ids(&r), sorted(shown.clone()));
    let created = r.thread_id.clone().expect("the new thread");
    for id in &shown {
        assert_eq!(g.s.stored_doc(id).await.thread_id, created, "{id} in the new thread");
    }
    assert_eq!(g.s.stored_doc(&z).await.thread_id, g.gone, "Z not moved into it");
    assert_eq!(waiting(&g.s).await, vec![z.clone()]);
}

// T2 — "those of them still waiting": a shown document that a peer moved to a
// live thread, or trashed, meanwhile is no longer waiting and is left alone.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn t2_a_shown_document_no_longer_waiting_is_left_alone() {
    let g = group("t2-left").await;
    let shown = vec![g.x.clone(), g.y.clone()];
    let elsewhere = thread(&g.s, "Elsewhere").await;

    // The phone moves X to a live thread; its state arrives.
    let prev = current_stamp(&g.s, &g.x).await;
    g.s.clock.advance(Duration::minutes(1));
    let mut moved = from_phone(&g.x, &elsewhere, "X");
    moved.content = g.s.stored(&g.x).await.1;
    g.s.svc.apply_wire(moved, meta(Some(stamp(&g.s, 1, 0, "phone")), Some(prev))).await.unwrap();
    assert_eq!(g.s.stored_doc(&g.x).await.thread_id, elsewhere, "setup: the phone's move applied");
    let x_stamp = current_stamp(&g.s, &g.x).await;

    let r = g.s.svc.resolve_orphans_shown(&g.gone, &shown, OrphanResolution::Trash).await.expect("resolved");
    assert_eq!(r.documents, vec![g.y.clone()], "only Y was still waiting");
    assert!(g.s.svc.read(&g.x).await.unwrap().trashed_at.is_none(), "X, moved meanwhile, is not trashed");
    assert_eq!(g.s.stored_doc(&g.x).await.thread_id, elsewhere);
    assert_eq!(current_stamp(&g.s, &g.x).await, x_stamp, "X not written");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn t2_a_shown_document_trashed_meanwhile_is_not_moved_out_of_the_trash() {
    let g = group("t2-trashed").await;
    let shown = vec![g.x.clone(), g.y.clone()];

    // The phone trashes Y; its deletion arrives.
    let prev = current_stamp(&g.s, &g.y).await;
    g.s.clock.advance(Duration::minutes(1));
    let mut tomb = from_phone(&g.y, &g.gone, "Y");
    tomb.content = g.s.stored(&g.y).await.1;
    tomb.deleted_at = Some(g.s.clock.get());
    g.s.svc.apply_wire(tomb, meta(Some(stamp(&g.s, 1, 0, "phone")), Some(prev))).await.unwrap();
    assert!(g.s.svc.read(&g.y).await.unwrap().trashed_at.is_some(), "setup: Y in the trash");

    let r = g.s.svc.resolve_orphans_shown(&g.gone, &shown, OrphanResolution::MoveTo(g.keep.clone())).await.unwrap();
    assert_eq!(r.documents, vec![g.x.clone()], "only X was still waiting");
    assert!(g.s.svc.read(&g.y).await.unwrap().trashed_at.is_some(), "Y stays in the trash");
    assert_eq!(g.s.stored_doc(&g.y).await.thread_id, g.gone, "Y not moved");
}

// T2, settled (contract cb5a859): "with none of the shown ids still waiting:
// does nothing and says so (zero, not a refusal)".
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn t2_with_none_of_the_shown_still_waiting_it_does_nothing_and_says_zero() {
    let g = group("t2-none").await;
    let shown = vec![g.x.clone(), g.y.clone()];
    // Both resolved meanwhile (from another window): moved to Keep.
    g.s.svc.resolve_orphan(&g.x, OrphanResolution::MoveTo(g.keep.clone())).await.unwrap();
    g.s.svc.resolve_orphan(&g.y, OrphanResolution::MoveTo(g.keep.clone())).await.unwrap();
    let stamps = [current_stamp(&g.s, &g.x).await, current_stamp(&g.s, &g.y).await];
    let z = late_arrival(&g).await;
    let threads_before = g.s.edb.list_threads().await.unwrap().len();

    for resolution in [OrphanResolution::Trash, OrphanResolution::NewThread("Revived".into())] {
        let r = g.s.svc.resolve_orphans_shown(&g.gone, &shown, resolution.clone()).await;
        let r = r.unwrap_or_else(|e| panic!("{resolution:?}: zero, not a refusal: {e:?}"));
        assert!(r.documents.is_empty(), "{resolution:?}: nothing acted on: {r:?}");
    }
    assert_eq!([current_stamp(&g.s, &g.x).await, current_stamp(&g.s, &g.y).await], stamps, "nothing written");
    assert_eq!(g.s.edb.list_threads().await.unwrap().len(), threads_before, "no thread created for nobody");
    assert_eq!(waiting(&g.s).await, vec![z.clone()], "Z, never shown, still waits");
}

// "Unchanged: the orphan resolutions' own rules." A guard: a refused target
// still changes nothing, whatever was shown.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn t2_a_refused_target_still_changes_nothing() {
    let g = group("t2-refused").await;
    let shown = vec![g.x.clone(), g.y.clone()];
    let stamps = [current_stamp(&g.s, &g.x).await, current_stamp(&g.s, &g.y).await];
    for bad in [OrphanResolution::MoveTo(g.gone.clone()), OrphanResolution::NewThread("   ".into())] {
        assert!(g.s.svc.resolve_orphans_shown(&g.gone, &shown, bad.clone()).await.is_err(), "{bad:?} refused");
    }
    assert_eq!([current_stamp(&g.s, &g.x).await, current_stamp(&g.s, &g.y).await], stamps, "nothing written");
    assert_eq!(waiting(&g.s).await, sorted(shown));
}

// ======================================================================= T3

// T3 — "`trash` tells 'moved to the trash' apart from 'was already in the
// trash'."
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn t3_trash_says_whether_it_moved_the_document_or_found_it_there() {
    let s = Stack::new("t3-outcome").await;
    let id = s.create_text("T", "text").await;
    let first = s.svc.trash(&id).await.expect("trashed");
    assert_eq!(first, TrashOutcome::Trashed, "{}", said(&first));
    let date = s.svc.read(&id).await.unwrap().trashed_at;
    let stamp = current_stamp(&s, &id).await;
    let _ = s.events.take();

    s.clock.advance(Duration::minutes(5));
    let again = s.svc.trash(&id).await.expect("no error");
    assert_eq!(again, TrashOutcome::AlreadyInTrash, "{}", said(&again));
    assert_eq!(s.svc.read(&id).await.unwrap().trashed_at, date, "its trash date unchanged");
    assert_eq!(current_stamp(&s, &id).await, stamp, "nothing written");
    assert!(
        s.events.take().iter().all(|e| e.id != id),
        "no change announced for a delete that did nothing"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn t3_a_document_a_peer_already_trashed_is_reported_already_in_the_trash() {
    let (a, phone, ids) = two_devices("t3-peer", &["Doc"]).await;
    let id = ids[0].clone();
    peer_deletes(&a, &phone, &id).await;
    let date = a.svc.read(&id).await.unwrap().trashed_at;
    let stamp = current_stamp(&a, &id).await;

    let out = a.svc.trash(&id).await.expect("no error");
    assert_eq!(out, TrashOutcome::AlreadyInTrash, "this Delete made no deletion: {}", said(&out));
    assert_eq!(a.svc.read(&id).await.unwrap().trashed_at, date);
    assert_eq!(current_stamp(&a, &id).await, stamp, "the peer's deletion is left as it came");
}
