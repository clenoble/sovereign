//! S5-12 — a live document in a deleted thread asks (`doc/plans/C-DOC-adoption.md`,
//! *S5-12 — a live document in a deleted thread asks: contract*, W1–W6),
//! the service side: W1 (the condition), W2 at the service (a state applied by
//! `apply_wire`), W3 (never hidden), W4 (three ways out) and W6 (nothing else
//! changes). Sync over the real envelopes is in `sovereign-p2p/tests/wire_orphans.rs`;
//! the shell and the phone in their own suites (`TESTS-s5-12.md`).
//!
//! Spec ch. 03, *Thread deletion and the fallback lane* (Céline, 2026-10-17):
//! "A document that arrives by sync for a thread deleted here (written on
//! another device meanwhile) raises a warning asking the user what to do with
//! it — move it to another thread, create a new thread for it, or put it in
//! the trash; until then it stays visible."
//!
//! Plan, design choice (laptop): "the warning is a condition the service
//! computes: every live document (not in the trash, not purged) whose thread
//! is deleted here (a thread tombstone)… survives restarts, and disappears
//! exactly when it is resolved."
//!
//! How the tests make a thread tombstone: through the host path
//! (`delete_thread_to_trash`), or — for "the other device's thread deletion
//! arrives" — through the store call the row sync itself makes
//! (`soft_delete_thread`, `sovereign-p2p` `apply_thread_row`). Documents are
//! only ever written through the service.

mod common;

use chrono::Duration;
use common::wire::*;
use common::*;
use sovereign_db::schema::Thread;
use sovereign_docs::{
    delete_thread_to_trash, purge_tick, IncomingAlert, OrphanResolution, OrphanedDocument, RemoteDocument,
};

async fn thread(s: &Stack, name: &str) -> String {
    s.edb.create_thread(Thread::new(name.into(), String::new())).await.unwrap().id_string().unwrap()
}

async fn doc_in(s: &Stack, thread_id: &str, title: &str, text: &str) -> String {
    let mut d = new_text_doc(title, text);
    d.thread_id = thread_id.to_string();
    s.svc.create(d).await.unwrap().id
}

/// A state written on the phone, for thread `thread_id`.
fn from_phone(id: &str, thread_id: &str, title: &str, text: &str) -> RemoteDocument {
    let mut r = remote_text(id, title, text, "phone");
    r.thread_id = thread_id.to_string();
    r
}

/// Applies a first state of `id` from the phone (a new arrival).
async fn arrive(s: &Stack, r: RemoteDocument) {
    s.svc.apply_wire(r, meta(Some(stamp(s, 1, 0, "phone")), None)).await.expect("applied");
}

async fn orphans(s: &Stack) -> Vec<OrphanedDocument> {
    let mut v = s.svc.orphans_of_deleted_threads().await.expect("the condition is computed");
    v.sort_by(|a, b| a.id.cmp(&b.id));
    v
}

async fn orphan_ids(s: &Stack) -> Vec<String> {
    orphans(s).await.into_iter().map(|o| o.id).collect()
}

async fn active_thread_ids(s: &Stack) -> Vec<String> {
    s.edb.list_threads().await.unwrap().into_iter().filter(|t| t.deleted_at.is_none()).filter_map(|t| t.id_string()).collect()
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

/// "Old drafts" deleted here (its one document, `trashed`, went to the trash
/// with it), then a document for it arrives from the phone (`arrived`).
/// "Keep" stays live with `kept`.
struct World {
    s: Stack,
    gone: String,
    keep: String,
    trashed: String,
    arrived: String,
    kept: String,
}

async fn world(tag: &str) -> World {
    let s = Stack::new(tag).await;
    let gone = thread(&s, "Old drafts").await;
    let keep = thread(&s, "Keep").await;
    let trashed = doc_in(&s, &gone, "Draft", "draft text").await;
    let kept = doc_in(&s, &keep, "Kept", "kept text").await;
    delete_thread_to_trash(&s.svc, s.edb.as_ref(), &gone).await.expect("thread deleted");
    s.clock.advance(Duration::minutes(1));
    let arrived = "document:offline1".to_string();
    arrive(&s, from_phone(&arrived, &gone, "Written offline", "zebra-offline-text")).await;
    World { s, gone, keep, trashed, arrived, kept }
}

// -------------------------------------------------------------------- W1 --

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w1_a_live_document_in_a_deleted_thread_is_listed_with_its_title_old_thread_name_and_device() {
    let w = world("w1-listed").await;
    let list = orphans(&w.s).await;
    assert_eq!(list.len(), 1, "exactly the live document of the deleted thread: {list:?}");
    let o = &list[0];
    assert_eq!(o.id, w.arrived);
    assert_eq!(o.title, "Written offline");
    assert_eq!(o.thread_id, w.gone, "the deleted thread it is still in");
    assert_eq!(o.deleted_thread_name.as_deref(), Some("Old drafts"), "the old name, read from the tombstone");
    assert_eq!(o.device, "phone", "the device it came from");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w1_documents_in_the_trash_purged_or_in_a_live_thread_are_not_listed() {
    let w = world("w1-not-listed").await;
    // The trash: the deleted thread's own document is there, and not listed.
    assert!(w.s.svc.read(&w.trashed).await.unwrap().trashed_at.is_some());
    // A live document in a live thread: not listed.
    assert!(w.s.svc.read(&w.kept).await.unwrap().trashed_at.is_none());
    assert_eq!(orphan_ids(&w.s).await, vec![w.arrived.clone()]);

    // Purged: a second document of the deleted thread arrives, then is trashed
    // and purged — never listed.
    let second = "document:offline2".to_string();
    arrive(&w.s, from_phone(&second, &w.gone, "Second", "second")).await;
    assert_eq!(sorted(orphan_ids(&w.s).await), sorted(vec![w.arrived.clone(), second.clone()]));
    w.s.svc.trash(&second).await.unwrap();
    assert_eq!(orphan_ids(&w.s).await, vec![w.arrived.clone()], "in the trash: not listed");
    w.s.svc.empty_trash().await.unwrap();
    assert!(w.s.svc.is_purged(&second).await.unwrap());
    assert_eq!(orphan_ids(&w.s).await, vec![w.arrived.clone()], "purged: not listed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w1_a_document_whose_thread_never_existed_here_is_not_listed() {
    let s = Stack::new("w1-never").await;
    let keep = thread(&s, "Keep").await;
    // "thread:t1" was never created on this device (the fallback lane's other case).
    let never = s.create_text("Never had a thread here", "x").await;
    assert_eq!(s.stored_doc(&never).await.thread_id, "thread:t1");
    arrive(&s, from_phone("document:unknown1", "thread:unknownhere", "Unknown thread", "y")).await;
    let _ = doc_in(&s, &keep, "Kept", "kept").await;
    assert!(orphans(&s).await.is_empty(), "no thread tombstone, nothing to ask");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w1_the_condition_survives_a_restart() {
    let mut w = world("w1-restart").await;
    let before = orphans(&w.s).await;
    assert_eq!(before.len(), 1);
    w.s.restart();
    assert_eq!(orphans(&w.s).await, before, "computed again from the store, the same");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w1_a_document_written_here_lists_this_device() {
    // The other direction's raw material: this device's own document, its
    // thread's deletion arriving from the phone (the row sync's store call).
    let s = Stack::new("w1-local").await;
    let lane = thread(&s, "Shared").await;
    let mine = doc_in(&s, &lane, "Mine", "written here").await;
    s.edb.soft_delete_thread(&lane).await.unwrap();
    let list = orphans(&s).await;
    assert_eq!(list.iter().map(|o| o.id.clone()).collect::<Vec<_>>(), vec![mine]);
    assert_eq!(list[0].device, DEVICE, "written on this device");
    assert_eq!(list[0].deleted_thread_name.as_deref(), Some("Shared"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w1_the_listing_prints_no_text() {
    let w = world("w1-debug").await;
    let printed = format!("{:?}", orphans(&w.s).await);
    for text in ["Written offline", "Old drafts", "zebra-offline-text"] {
        assert!(!printed.contains(text), "{{:?}} printed {text:?}: {printed}");
    }
}

// -------------------------------------------------------------- W2 (service) --

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w2_a_thread_deletion_arriving_lists_the_live_documents_and_trashes_none() {
    let s = Stack::new("w2-tomb").await;
    let lane = thread(&s, "Shared").await;
    let a = doc_in(&s, &lane, "A", "alpha").await;
    let b = doc_in(&s, &lane, "B", "beta").await;
    let earlier = doc_in(&s, &lane, "Earlier", "trashed before").await;
    s.svc.trash(&earlier).await.unwrap();
    let stamps = [current_stamp(&s, &a).await, current_stamp(&s, &b).await];

    // The thread's tombstone arrives (what `apply_thread_row` does).
    s.edb.soft_delete_thread(&lane).await.unwrap();

    assert_eq!(sorted(orphan_ids(&s).await), sorted(vec![a.clone(), b.clone()]));
    for (id, st) in [(&a, &stamps[0]), (&b, &stamps[1])] {
        assert!(s.svc.read(id).await.unwrap().trashed_at.is_none(), "{id} not trashed by the sync");
        assert_eq!(current_stamp(&s, id).await, *st, "{id} not written by the condition");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w2_an_edit_arriving_for_a_document_trashed_with_its_thread_is_listed_when_it_wins() {
    // The phone edited a document offline that this device trashed with its
    // thread. Whatever the stamps decide (G25/G26), the condition holds: the
    // document is listed exactly when it is live here.
    let s = Stack::new("w2-edit").await;
    let lane = thread(&s, "Old drafts").await;
    let d = doc_in(&s, &lane, "Draft", "draft").await;
    let before_delete = current_stamp(&s, &d).await;
    delete_thread_to_trash(&s.svc, s.edb.as_ref(), &lane).await.unwrap();
    s.clock.advance(Duration::minutes(1));
    let mut edit = from_phone(&d, &lane, "Draft", "edited on the phone");
    edit.deleted_at = None;
    s.svc.apply_wire(edit, meta(Some(stamp(&s, 30, 0, "phone")), Some(before_delete))).await.unwrap();

    let live = s.svc.read(&d).await.unwrap().trashed_at.is_none();
    let listed = orphan_ids(&s).await.contains(&d);
    assert_eq!(listed, live, "listed exactly when live (live = {live})");
}

// -------------------------------------------------------------------- W3 --

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w3_a_listed_document_stays_in_every_list_readable_and_editable() {
    let w = world("w3-visible").await;
    let all: Vec<String> = w.s.svc.list(None).await.unwrap().into_iter().map(|d| d.id).collect();
    assert!(all.contains(&w.arrived), "in the list of all documents");
    let of_thread: Vec<String> = w.s.svc.list(Some(&w.gone)).await.unwrap().into_iter().map(|d| d.id).collect();
    assert_eq!(of_thread, vec![w.arrived.clone()], "in its thread's list");
    let v = w.s.svc.read(&w.arrived).await.unwrap();
    assert_eq!(v.plain_text(), "zebra-offline-text");
    assert!(v.trashed_at.is_none());

    w.s.editor_save(&w.arrived, 7, v2(vec![text_block(1, "edited here")])).await.expect("editable");
    assert_eq!(w.s.svc.read(&w.arrived).await.unwrap().plain_text(), "edited here");
    assert_eq!(orphan_ids(&w.s).await, vec![w.arrived.clone()], "still asking: an edit resolves nothing");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w3_the_purge_never_takes_a_listed_document() {
    let w = world("w3-purge").await;
    w.s.clock.advance(Duration::days(45));
    let report = purge_tick(&w.s.svc).await.unwrap();
    assert!(report.documents.contains(&w.trashed), "the trash is purged as usual");
    assert!(!report.documents.contains(&w.arrived), "never the unresolved document");
    assert!(w.s.svc.read(&w.arrived).await.unwrap().trashed_at.is_none());
    assert_eq!(orphan_ids(&w.s).await, vec![w.arrived.clone()], "still listed, still asking");
}

// -------------------------------------------------------------------- W4 --

/// What a resolution must leave unchanged in the document itself.
async fn same_document(s: &Stack, id: &str, title: &str, text: &str, versions_before: usize) {
    let v = s.svc.read(id).await.unwrap();
    assert_eq!(v.title, title, "title unchanged");
    assert_eq!(v.plain_text(), text, "content unchanged");
    assert!(s.svc.versions(id).await.unwrap().len() >= versions_before, "no version lost");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w4_move_to_an_existing_thread_stamps_the_move_and_ends_the_question() {
    let w = world("w4-move").await;
    let versions = w.s.svc.versions(&w.arrived).await.unwrap().len();
    let before = current_stamp(&w.s, &w.arrived).await;
    w.s.clock.advance(Duration::minutes(1));

    let done = w.s.svc.resolve_orphan(&w.arrived, OrphanResolution::MoveTo(w.keep.clone())).await.expect("moved");

    assert_eq!(done.documents, vec![w.arrived.clone()]);
    assert_eq!(done.thread_id.as_deref(), Some(w.keep.as_str()));
    assert_eq!(w.s.svc.read(&w.arrived).await.unwrap().thread_id, w.keep);
    let state = w.s.svc.stored_state(&w.arrived).await.unwrap();
    assert_eq!(state.document.thread_id, w.keep, "the state sync sends carries the new thread");
    assert!(state.meta.stamp.clone().unwrap() > before, "the move is a stamped write (it travels)");
    same_document(&w.s, &w.arrived, "Written offline", "zebra-offline-text", versions).await;
    assert!(orphans(&w.s).await.is_empty(), "no longer listed");
    assert!(w.s.svc.read(&w.arrived).await.unwrap().trashed_at.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w4_a_new_thread_is_created_with_the_name_given_and_the_document_moves_there() {
    let w = world("w4-new").await;
    let versions = w.s.svc.versions(&w.arrived).await.unwrap().len();
    let before = current_stamp(&w.s, &w.arrived).await;
    let threads_before = active_thread_ids(&w.s).await;

    let done = w
        .s
        .svc
        .resolve_orphan(&w.arrived, OrphanResolution::NewThread("  Brought back  ".into()))
        .await
        .expect("new thread");

    let new_id = done.thread_id.clone().expect("the new thread's id");
    assert!(!threads_before.contains(&new_id), "a new thread");
    assert_ne!(new_id, w.gone, "never the deleted thread's id (it never comes back)");
    let t = w.s.edb.get_thread(&new_id).await.expect("the new thread exists");
    assert!(t.deleted_at.is_none());
    assert_eq!(t.name, "Brought back", "named as given, trimmed");
    assert_eq!(active_thread_ids(&w.s).await.len(), threads_before.len() + 1, "exactly one thread created");
    assert_eq!(w.s.svc.read(&w.arrived).await.unwrap().thread_id, new_id);
    assert!(current_stamp(&w.s, &w.arrived).await > before, "stamped");
    same_document(&w.s, &w.arrived, "Written offline", "zebra-offline-text", versions).await;
    assert!(orphans(&w.s).await.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w4_trash_puts_it_in_the_trash_restorable_for_30_days() {
    let w = world("w4-trash").await;
    let before = current_stamp(&w.s, &w.arrived).await;
    let done = w.s.svc.resolve_orphan(&w.arrived, OrphanResolution::Trash).await.expect("trashed");
    assert_eq!(done.documents, vec![w.arrived.clone()]);
    assert_eq!(done.thread_id, None);
    assert_eq!(w.s.svc.read(&w.arrived).await.unwrap().trashed_at, Some(w.s.clock.get()), "trashed by the service, now");
    assert!(current_stamp(&w.s, &w.arrived).await > before, "stamped (its sync tombstone)");
    let trash: Vec<String> = w.s.svc.list_trash().await.unwrap().into_iter().map(|e| e.id).collect();
    assert!(trash.contains(&w.arrived));
    assert!(orphans(&w.s).await.is_empty());
    // Restorable, into a thread the user chooses (its own is deleted: S5-6).
    sovereign_docs::restore_to_thread(&w.s.svc, w.s.edb.as_ref(), &w.arrived, Some(&w.keep)).await.expect("restorable");
    assert_eq!(w.s.svc.read(&w.arrived).await.unwrap().plain_text(), "zebra-offline-text");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w4_a_deleted_or_unknown_target_or_an_empty_name_is_refused_and_nothing_changes() {
    let w = world("w4-refused").await;
    let other_gone = thread(&w.s, "Also gone").await;
    delete_thread_to_trash(&w.s.svc, w.s.edb.as_ref(), &other_gone).await.unwrap();
    let state = w.s.svc.stored_state(&w.arrived).await.unwrap();
    let threads = sorted(active_thread_ids(&w.s).await);

    for (what, r) in [
        ("its own deleted thread", OrphanResolution::MoveTo(w.gone.clone())),
        ("another deleted thread", OrphanResolution::MoveTo(other_gone.clone())),
        ("an unknown thread", OrphanResolution::MoveTo("thread:nope".into())),
        ("a malformed thread id", OrphanResolution::MoveTo("not a thread".into())),
        ("an empty new-thread name", OrphanResolution::NewThread(String::new())),
        ("a spaces-only new-thread name", OrphanResolution::NewThread("   ".into())),
    ] {
        assert!(w.s.svc.resolve_orphan(&w.arrived, r).await.is_err(), "{what}: refused");
        let now = w.s.svc.stored_state(&w.arrived).await.unwrap();
        assert_eq!(now.normalized_hash, state.normalized_hash, "{what}: the document unchanged");
        assert_eq!(now.meta.stamp, state.meta.stamp, "{what}: nothing written");
        assert_eq!(sorted(active_thread_ids(&w.s).await), threads, "{what}: no thread created");
        assert_eq!(orphan_ids(&w.s).await, vec![w.arrived.clone()], "{what}: still listed");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w4_a_document_that_is_not_listed_is_refused_and_nothing_changes() {
    // A stale view: the document was resolved meanwhile (on another device,
    // through sync), or never was in a deleted thread.
    let w = world("w4-stale").await;
    let elsewhere = thread(&w.s, "Elsewhere").await;
    for (what, id) in [("a document in a live thread", &w.kept), ("a document in the trash", &w.trashed)] {
        let state = w.s.svc.stored_state(id).await.unwrap();
        for r in [OrphanResolution::MoveTo(elsewhere.clone()), OrphanResolution::Trash, OrphanResolution::NewThread("N".into())] {
            assert!(w.s.svc.resolve_orphan(id, r.clone()).await.is_err(), "{what}, {r:?}: refused");
            let now = w.s.svc.stored_state(id).await.unwrap();
            assert_eq!((now.normalized_hash, now.meta.stamp), (state.normalized_hash.clone(), state.meta.stamp.clone()), "{what}, {r:?}");
        }
    }
    assert!(w.s.svc.resolve_orphan("document:nope", OrphanResolution::Trash).await.is_err(), "an unknown document");
    assert_eq!(w.s.edb.list_threads().await.unwrap().iter().filter(|t| t.name == "N").count(), 0, "no thread created");
}

/// "Old drafts" deleted here; three documents of it arrive from the phone; a
/// fourth arrives for another deleted thread, "Other".
struct Group {
    w: World,
    group: [String; 3],
    other_gone: String,
    other: String,
}

async fn group(tag: &str) -> Group {
    let w = world(tag).await;
    let b = "document:offline-b".to_string();
    let c = "document:offline-c".to_string();
    arrive(&w.s, from_phone(&b, &w.gone, "B", "bee")).await;
    arrive(&w.s, from_phone(&c, &w.gone, "C", "sea")).await;
    let other_gone = thread(&w.s, "Other").await;
    delete_thread_to_trash(&w.s.svc, w.s.edb.as_ref(), &other_gone).await.unwrap();
    let other = "document:offline-other".to_string();
    arrive(&w.s, from_phone(&other, &other_gone, "Other doc", "other")).await;
    let group = [w.arrived.clone(), b, c];
    Group { w, group, other_gone, other }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w4_all_of_a_deleted_thread_move_together_into_one_new_thread() {
    let g = group("w4-group-new").await;
    let s = &g.w.s;
    assert_eq!(orphan_ids(s).await.len(), 4);
    let threads_before = active_thread_ids(s).await.len();

    let done = s
        .svc
        .resolve_orphans_of_thread(&g.w.gone, OrphanResolution::NewThread("Old drafts, again".into()))
        .await
        .expect("the group");

    assert_eq!(sorted(done.documents.clone()), sorted(g.group.to_vec()), "every listed document of that thread");
    let new_id = done.thread_id.expect("one thread for the group");
    assert_eq!(active_thread_ids(s).await.len(), threads_before + 1, "one thread, not one per document");
    for id in &g.group {
        assert_eq!(s.svc.read(id).await.unwrap().thread_id, new_id, "{id} in the new thread");
    }
    assert_eq!(orphan_ids(s).await, vec![g.other.clone()], "the other deleted thread's document still asks");
    assert_eq!(s.svc.read(&g.other).await.unwrap().thread_id, g.other_gone);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w4_all_of_a_deleted_thread_go_to_the_trash_or_to_one_existing_thread() {
    let g = group("w4-group-trash").await;
    let s = &g.w.s;
    let done = s.svc.resolve_orphans_of_thread(&g.w.gone, OrphanResolution::Trash).await.expect("the group");
    assert_eq!(sorted(done.documents), sorted(g.group.to_vec()));
    for id in &g.group {
        assert!(s.svc.read(id).await.unwrap().trashed_at.is_some(), "{id} in the trash");
    }
    assert_eq!(orphan_ids(s).await, vec![g.other.clone()]);

    let done = s.svc.resolve_orphans_of_thread(&g.other_gone, OrphanResolution::MoveTo(g.w.keep.clone())).await.expect("move");
    assert_eq!(done.documents, vec![g.other.clone()]);
    assert_eq!(s.svc.read(&g.other).await.unwrap().thread_id, g.w.keep);
    assert!(orphans(s).await.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w4_a_group_with_a_refused_target_changes_nothing() {
    let g = group("w4-group-refused").await;
    let s = &g.w.s;
    let states: Vec<_> = {
        let mut v = vec![];
        for id in &g.group {
            v.push(s.svc.stored_state(id).await.unwrap().normalized_hash);
        }
        v
    };
    let threads = sorted(active_thread_ids(s).await);
    for r in [
        OrphanResolution::MoveTo(g.other_gone.clone()),
        OrphanResolution::MoveTo("thread:nope".into()),
        OrphanResolution::NewThread(" ".into()),
    ] {
        assert!(s.svc.resolve_orphans_of_thread(&g.w.gone, r.clone()).await.is_err(), "{r:?}: refused");
        for (id, h) in g.group.iter().zip(&states) {
            assert_eq!(&s.svc.stored_state(id).await.unwrap().normalized_hash, h, "{r:?}: {id} unchanged");
        }
        assert_eq!(sorted(active_thread_ids(s).await), threads, "{r:?}: no thread created");
    }
    // A thread that is not deleted is not a group to resolve.
    assert!(s.svc.resolve_orphans_of_thread(&g.w.keep, OrphanResolution::Trash).await.is_err());
    assert!(s.svc.read(&g.w.kept).await.unwrap().trashed_at.is_none(), "the live thread's document untouched");
    assert_eq!(orphan_ids(s).await.len(), 4, "everything still asks");
}

// -------------------------------------------------------------------- W6 --

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w6_the_condition_raises_and_replaces_no_alert() {
    let w = world("w6-none").await;
    assert_eq!(orphans(&w.s).await.len(), 1);
    assert!(w.s.svc.alerts().await.unwrap().is_empty(), "listing an orphan raises no alert");
    assert!(!w.s.svc.has_alert(&w.arrived).await.unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w6_a_peer_audit_alert_on_the_same_document_is_untouched_by_listing_and_resolving() {
    let w = world("w6-alert").await;
    let alert = IncomingAlert {
        id: w.arrived.clone(),
        device: "phone".into(),
        finding: "incoming content contains prompt-injection patterns".into(),
        previous_version: None,
    };
    w.s.svc.raise_alert(alert.clone()).await.unwrap();
    let before = w.s.svc.alerts().await.unwrap();
    assert_eq!(before, vec![alert.clone()]);

    assert_eq!(orphan_ids(&w.s).await, vec![w.arrived.clone()], "listed beside its alert");
    assert_eq!(w.s.svc.alerts().await.unwrap(), before, "listing changes no alert");

    w.s.svc.resolve_orphan(&w.arrived, OrphanResolution::MoveTo(w.keep.clone())).await.unwrap();
    assert_eq!(w.s.svc.alerts().await.unwrap(), before, "resolving changes no alert");

    // And dismissing the alert resolves nothing.
    let w2 = world("w6-dismiss").await;
    w2.s.svc.raise_alert(IncomingAlert { id: w2.arrived.clone(), ..alert }).await.unwrap();
    w2.s.svc.dismiss_alert(&w2.arrived).await.unwrap();
    assert_eq!(orphan_ids(&w2.s).await, vec![w2.arrived.clone()], "dismissing an alert resolves nothing");
}

// ----------------------------------------------------------- W7, now S6 --
//
// W7 ("thread tombstones are kept … for now, no thread tombstone is ever
// purged") is replaced by THREAD-sync S6 (`doc/plans/THREAD-sync.md`, Céline
// 2026-10-18, "ID oui"): "After 30 days in the deleted state, and once **no
// document — live or in the trash — points to it**, the tombstone becomes a
// **thread marker**: the id and the deletion stamp only (name and description
// removed from disk). The marker stays in the sync manifest; a received row
// for that id with an **older** stamp is refused (no resurrection) … Run by
// the existing purge tick." Revised by the THREAD-sync test author
// (2026-10-18); each change is listed in `crates/sovereign-p2p/TESTS-thread-sync.md`.
//
// What stays true from W7: while a document points to it, a deleted thread
// keeps its name (it keeps asking, the trash keeps "<old name> - deleted"),
// and nothing comes back. What changes: once nothing points to it, it shrinks.
//
// Still independent of the two clocks (the round-3 W7 design): the threads
// here are deleted through `delete_thread_to_trash` and the ticks run at +0,
// +31 days, +10 years and +100 years of the service's clock; a shrink is
// asserted only at the ten-year tick and later, where any reading of "30 days
// in the deleted state" has passed, and the name only where something points
// to the thread. The 30-day boundary itself is `ts6_thread_markers.rs`.
// Sync (refusing an older row) is `sovereign-p2p/tests/wire_thread_sync.rs`.

/// The thread record `id`, tombstone or live, as stored.
async fn thread_row(s: &Stack, id: &str) -> Option<sovereign_db::schema::Thread> {
    s.edb.list_threads_including_deleted().await.unwrap().into_iter().find(|t| t.id_string().as_deref() == Some(id))
}

/// Advances to now, +31 days, +10 years and +100 years, running a purge tick
/// at each (twice: a shrink may come in the tick after the one that purges the
/// last document), and calls `check` after each.
async fn ticks_far_into_the_future<F, Fut>(s: &Stack, mut check: F)
where
    F: FnMut(i64) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let mut at = 0;
    for days in [0, 31, 3650, 36500] {
        s.clock.advance(Duration::days(days - at));
        at = days;
        purge_tick(&s.svc).await.expect("tick");
        purge_tick(&s.svc).await.expect("tick");
        check(days).await;
    }
}

/// S6: thread `id` is a marker — deleted, its name and description gone from
/// the store, its stamp still there, in no list.
async fn assert_shrunk(s: &Stack, id: &str, old_name: &str, what: &str) {
    if let Some(row) = thread_row(s, id).await {
        assert!(row.deleted_at.is_some(), "{what}: still deleted (nothing comes back)");
        assert!(row.name.is_empty() && row.description.is_empty(), "{what}: shrunk to its id, no name");
    }
    if let Ok(raw) = s.raw.get_thread(id).await {
        assert!(!serde_json::to_string(&raw).unwrap().contains(old_name), "{what}: the name in clear on disk");
        assert!(raw.name_token_hashes.is_empty(), "{what}: the name's blind index is gone");
    }
    assert!(s.svc.thread_stamp(id).await.expect("read").is_some(), "{what}: the marker keeps the deletion stamp");
    assert!(!active_thread_ids(s).await.contains(&id.to_string()), "{what}: in no list");
}

/// `world`, plus "Emptied" (deleted with its one document, which goes to the
/// trash) and "Bare" (deleted with no document).
async fn tombstones_world(tag: &str) -> (World, String, String, String) {
    let w = world(tag).await;
    let emptied = thread(&w.s, "Emptied").await;
    let emptied_doc = doc_in(&w.s, &emptied, "Gone with it", "x").await;
    delete_thread_to_trash(&w.s.svc, w.s.edb.as_ref(), &emptied).await.unwrap();
    let bare = thread(&w.s, "Bare").await;
    delete_thread_to_trash(&w.s.svc, w.s.edb.as_ref(), &bare).await.unwrap();
    (w, emptied, emptied_doc, bare)
}

/// Was `w7_no_thread_tombstone_is_ever_purged_with_live_trashed_or_no_documents`.
/// S6: a tombstone a live document points to keeps its name at every tick; one
/// only a trashed document points to keeps it while that document is in the
/// trash and shrinks once it is purged; one nothing points to shrinks.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w7_s6_a_tombstone_shrinks_to_its_id_only_once_nothing_points_to_it() {
    let (w, emptied, emptied_doc, bare) = tombstones_world("w7-s6-shrinks").await;
    let s = &w.s;
    ticks_far_into_the_future(s, |days| {
        let (gone, emptied, emptied_doc, bare, keep) =
            (w.gone.clone(), emptied.clone(), emptied_doc.clone(), bare.clone(), w.keep.clone());
        async move {
            // A live document points to it: kept with its name, always.
            let row = thread_row(s, &gone).await.expect("the tombstone a live document points to");
            assert!(row.deleted_at.is_some(), "+{days} days: still deleted");
            assert_eq!(row.name, "Old drafts", "+{days} days, a live document points to it: its name is kept");
            if days == 0 {
                // Only a trashed document points to it / nothing does: not yet 30 days.
                for (id, name) in [(&emptied, "Emptied"), (&bare, "Bare")] {
                    let row = thread_row(s, id).await.expect("kept before 30 days");
                    assert_eq!((row.name.as_str(), row.deleted_at.is_some()), (name, true), "+0 days");
                }
                assert!(!s.svc.is_purged(&emptied_doc).await.unwrap(), "+0 days: its document still in the trash");
            }
            if days >= 3650 {
                assert!(s.svc.is_purged(&emptied_doc).await.unwrap(), "+{days} days: the trash is purged as usual");
                assert_shrunk(s, &emptied, "Emptied", &format!("+{days} days, its trashed document purged")).await;
                assert_shrunk(s, &bare, "Bare", &format!("+{days} days, nothing points to it")).await;
            }
            assert!(thread_row(s, &keep).await.is_some_and(|t| t.deleted_at.is_none() && t.name == "Keep"), "+{days} days: the live thread untouched");
        }
    })
    .await;
}

/// Was `w7_a_document_whose_thread_was_deleted_keeps_asking_through_every_purge`:
/// unchanged in substance — under S6 a live document points to the tombstone,
/// so it is never shrunk and the document keeps asking under the old name.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w7_s6_a_document_whose_thread_was_deleted_keeps_asking_through_every_purge() {
    let w = world("w7-s6-asking").await;
    let s = &w.s;
    let arrived = w.arrived.clone();
    ticks_far_into_the_future(s, |days| {
        let arrived = arrived.clone();
        async move {
            let list = orphans(s).await;
            assert_eq!(list.iter().map(|o| o.id.clone()).collect::<Vec<_>>(), vec![arrived.clone()], "+{days} days: still asking");
            assert_eq!(list[0].deleted_thread_name.as_deref(), Some("Old drafts"), "+{days} days: with its old name");
            assert!(s.svc.read(&arrived).await.unwrap().trashed_at.is_none(), "+{days} days: never trashed by a purge");
        }
    })
    .await;
}

/// Was `w7_the_trash_keeps_the_old_name_and_resolving_the_last_document_purges_no_tombstone`.
/// S6: the trash keeps the old name while a document of the thread is in it;
/// once the last live document is resolved and the last trashed one purged,
/// nothing points to the tombstone and it shrinks to its id.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w7_s6_the_trash_keeps_the_old_name_and_the_tombstone_shrinks_once_nothing_points_to_it() {
    let w = world("w7-s6-trash-name").await;
    let s = &w.s;
    ticks_far_into_the_future(s, |_| async {}).await;

    // A century later, a second document of the deleted thread arrives and is
    // put in the trash: the trash still names the deleted thread.
    let late = "document:offlinelate".to_string();
    arrive(s, from_phone(&late, &w.gone, "Late", "late")).await;
    s.svc.trash(&late).await.unwrap();
    purge_tick(&s.svc).await.unwrap();
    let listing = sovereign_docs::list_trash_named(&s.svc, s.edb.as_ref(), s.clock.get()).await.unwrap();
    let line = listing.iter().find(|l| l.entry.id == late).expect("in the trash");
    assert_eq!(line.thread_name, None, "its thread is not live");
    assert_eq!(line.deleted_thread_name.as_deref(), Some("Old drafts"), "the trash shows \"<old name> - deleted\"");

    // The last live document is resolved: only the trashed one points to the
    // tombstone now, and it keeps its name.
    s.svc.resolve_orphan(&w.arrived, OrphanResolution::MoveTo(w.keep.clone())).await.unwrap();
    purge_tick(&s.svc).await.unwrap();
    assert!(thread_row(s, &w.gone).await.is_some_and(|t| t.deleted_at.is_some() && t.name == "Old drafts"), "kept while a trashed document points to it");

    // Ten years on, the trashed document is purged: nothing points to it.
    s.clock.advance(Duration::days(3650));
    purge_tick(&s.svc).await.unwrap();
    purge_tick(&s.svc).await.unwrap();
    assert!(s.svc.is_purged(&late).await.unwrap());
    assert_shrunk(s, &w.gone, "Old drafts", "nothing points to it any more").await;
    assert_eq!(s.svc.read(&w.arrived).await.unwrap().thread_id, w.keep, "the moved document untouched");
}
