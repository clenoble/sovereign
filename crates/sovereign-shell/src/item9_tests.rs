//! v0.0.12 item 9 in the native shell — trash and orphan actions act on what
//! was shown, Delete says what it did (`doc/plans/V0012-item9-act-on-what-was-shown.md`,
//! T1–T3 through the shell's seams). T4 (the "Restore into…" chooser) needs
//! the API the item adds and is in `item9_api_tests.rs`.
//!
//! Written before the code, by another author: whoever builds item 9 makes
//! these pass **without editing this file**. A test that looks wrong is
//! reported, not worked around.
//!
//! This file uses only what exists at `c7e571b` (it compiles today): the trash
//! view (`trash::TrashView::confirm_empty`), the orphans section
//! (`orphans::OrphansSection::resolve_group`), and — as a child module of
//! `app`, declared in `app.rs` — a real headless `App` (the `lanes_tests`
//! harness) for what the user is told: Empty trash's confirmation clicked
//! (`trash_row_action`, `handle_trash_modal_click`), the card's Delete
//! (`trash_document`) and Undo (`undo_delete`), read through `App::notice`
//! (`said`) and `App::delete_notice`.
//!
//! A peer's change: a second document service, the phone's, over its own
//! in-memory store; the phone's state is exported (`stored_state`) and
//! applied here (`apply_wire`), as a sync round does.

use std::sync::Arc;

use sovereign_ai::docs_host::{build_document_service, new_user_document, testing};
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::schema::Thread;
use sovereign_db::traits::GraphDB;
use sovereign_docs::{delete_thread_to_trash, DocumentService, OrphanResolution, RemoteDocument, Stamp, Trust, WireMeta, WireState};
use vello::kurbo::Point;

use super::lanes_tests::{lanes, H, L, W};
use crate::orphans::OrphansSection;
use crate::trash::TrashView;
use crate::trash_ui::{modal_hit, ModalHit, TrashRowAction};

// ======================================================================= fixtures

async fn fixture(tag: &str, device: &str) -> (Arc<EncryptedGraphDB>, DocumentService) {
    let store = testing::encrypted_memory_store(tag).await;
    let docs = build_document_service(store.clone(), Some(testing::account_key()), device.into());
    (store, docs)
}

async fn thread(store: &EncryptedGraphDB, name: &str) -> String {
    store.create_thread(Thread::new(name.into(), String::new())).await.unwrap().id_string().unwrap()
}

async fn create(docs: &DocumentService, title: &str, thread_id: &str) -> String {
    docs.create(new_user_document(title, thread_id, format!("{title} text"))).await.unwrap().id
}

/// Lets the system clock move, so the next stamp is later.
async fn tick() {
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
}

/// One sync of `id`: `from`'s current state applied on `to`.
async fn send(from: &DocumentService, to: &DocumentService, id: &str) {
    let WireState { document, meta, .. } = from.stored_state(id).await.expect("export");
    to.apply_wire(document, meta).await.expect("apply");
}

/// The phone deletes `id` (it holds a copy) and the deletion reaches `here`.
async fn peer_deletes(here: &DocumentService, phone: &DocumentService, id: &str) {
    tick().await;
    phone.trash(id).await.expect("the phone's delete");
    send(phone, here, id).await;
    assert!(here.read(id).await.unwrap().trashed_at.is_some(), "setup: the peer's deletion was applied here");
}

async fn trash_ids(docs: &DocumentService) -> Vec<String> {
    let mut v: Vec<String> = docs.list_trash().await.unwrap().into_iter().map(|e| e.id).collect();
    v.sort();
    v
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

// ======================================================================= T1, the view

// T1 — "The confirm acts on the ids the trash view listed when it was drawn …
// A document that entered the trash after the view was drawn is not purged: it
// stays, and the next view lists it." "The same holds on the desktop."
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn t1_desktop_confirm_does_not_purge_what_a_peer_trashed_after_the_view_was_drawn() {
    let (store, docs) = fixture("i9-t1-view", "desktop").await;
    let (_pstore, phone) = fixture("i9-t1-view-phone", "phone").await;
    let lane = thread(&store, "Lane").await;
    let a = create(&docs, "A", &lane).await;
    let b = create(&docs, "B", &lane).await;
    let c = create(&docs, "C", &lane).await;
    send(&docs, &phone, &c).await;
    docs.trash(&a).await.unwrap();
    docs.trash(&b).await.unwrap();
    let now = chrono::Utc::now();
    let mut view = TrashView::load(&docs, store.as_ref(), now).await.unwrap();
    let shown: Vec<String> = view.lines().iter().map(|l| l.id.clone()).collect();
    assert_eq!(sorted(shown), sorted(vec![a.clone(), b.clone()]), "setup: the view shows A and B");

    peer_deletes(&docs, &phone, &c).await;

    view.request_empty();
    let n = view.confirm_empty(&docs).await.expect("emptied");
    assert_eq!(n, 2, "it counts what was purged: what was shown");
    assert!(docs.is_purged(&a).await.unwrap() && docs.is_purged(&b).await.unwrap(), "A and B purged");
    assert!(!docs.is_purged(&c).await.unwrap(), "C was never shown: not purged");
    assert_eq!(trash_ids(&docs).await, vec![c.clone()], "C stays in the trash");
    let next = TrashView::load(&docs, store.as_ref(), chrono::Utc::now()).await.unwrap();
    assert_eq!(next.lines().iter().map(|l| l.id.clone()).collect::<Vec<_>>(), vec![c.clone()], "the next view lists C");
}

// T1 — "A shown document that is no longer in the trash (restored … meanwhile)
// is left alone. The report counts what was purged."
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn t1_desktop_confirm_leaves_a_shown_document_restored_meanwhile_alone() {
    let (store, docs) = fixture("i9-t1-restored", "desktop").await;
    let lane = thread(&store, "Lane").await;
    let x = create(&docs, "X", &lane).await;
    let y = create(&docs, "Y", &lane).await;
    docs.trash(&x).await.unwrap();
    docs.trash(&y).await.unwrap();
    let mut view = TrashView::load(&docs, store.as_ref(), chrono::Utc::now()).await.unwrap();
    assert_eq!(view.count(), 2);

    docs.restore_from_trash(&x).await.unwrap(); // from another window, meanwhile

    view.request_empty();
    let n = view.confirm_empty(&docs).await.expect("emptied");
    assert_eq!(n, 1, "only Y was purged");
    assert!(docs.read(&x).await.unwrap().trashed_at.is_none(), "X stays live");
    assert!(!docs.is_purged(&x).await.unwrap());
    assert!(docs.is_purged(&y).await.unwrap());
}

// ======================================================================= T1, what the user is told

/// The point of Empty trash's confirmation button, found through the modal's
/// own hit-testing (no geometry is assumed).
fn confirm_point(l: &L) -> Point {
    let modal = l.app.trash_modal.as_ref().expect("the confirmation is showing");
    let mut y = 0.0;
    while y < H {
        let mut x = 0.0;
        while x < W {
            let p = Point::new(x, y);
            if modal_hit(modal, W, H, p) == ModalHit::Confirm {
                return p;
            }
            x += 4.0;
        }
        y += 4.0;
    }
    panic!("no Confirm button in the modal");
}

/// Settings → Trash read, "Empty trash" clicked: the confirmation shows.
fn ask_to_empty(l: &mut L) {
    l.app.refresh_trash();
    l.app.trash_row_action(TrashRowAction::EmptyTrash);
    assert!(l.app.trash_modal.is_some(), "setup: the confirmation shows");
}

fn said(l: &L) -> String {
    l.app.notice.as_ref().map(|n| format!("{} {}", n.said.0, n.said.1)).unwrap_or_default()
}

// T1 — "[choice] The confirm's message names the skipped ones when there are
// any" — settled (contract cb5a859): skipped = the shown documents no longer
// in the trash for any reason, and the message is neutral: "1 document was no
// longer in the trash and was left alone." (plural: N documents … left alone).
#[test]
fn t1_desktop_the_confirms_message_names_a_document_restored_meanwhile() {
    let mut l = lanes("i9-t1-msg", 1);
    let x = l.doc_in(0, "X");
    let y = l.doc_in(0, "Y");
    let docs = l.docs();
    l.app.rt.block_on(docs.trash(&x)).unwrap();
    l.app.rt.block_on(docs.trash(&y)).unwrap();
    ask_to_empty(&mut l);

    l.app.rt.block_on(docs.restore_from_trash(&x)).unwrap(); // meanwhile

    let p = confirm_point(&l);
    l.app.handle_trash_modal_click(p, W, H);
    assert!(l.app.trash_modal.is_none(), "the confirmation closes");
    assert!(l.app.rt.block_on(docs.read(&x)).unwrap().trashed_at.is_none(), "X kept, live");
    assert!(l.app.rt.block_on(docs.is_purged(&y)).unwrap(), "Y purged");
    let text = said(&l);
    assert!(text.contains("1 document was no longer in the trash and was left alone."), "the message names it: {text:?}");
}

// The same, for two skipped for two reasons: one restored, one purged elsewhere.
#[test]
fn t1_desktop_the_confirms_message_counts_every_shown_document_no_longer_in_the_trash() {
    let mut l = lanes("i9-t1-msg-two", 1);
    let x = l.doc_in(0, "X");
    let y = l.doc_in(0, "Y");
    let z = l.doc_in(0, "Z");
    let docs = l.docs();
    for id in [&x, &y, &z] {
        l.app.rt.block_on(docs.trash(id)).unwrap();
    }
    ask_to_empty(&mut l);

    l.app.rt.block_on(docs.restore_from_trash(&x)).unwrap(); // meanwhile
    l.app.rt.block_on(docs.purge_from_trash(&z)).unwrap(); // meanwhile, another window

    let p = confirm_point(&l);
    l.app.handle_trash_modal_click(p, W, H);
    assert!(l.app.rt.block_on(docs.read(&x)).unwrap().trashed_at.is_none(), "X left alone, live");
    assert!(l.app.rt.block_on(docs.is_purged(&y)).unwrap(), "Y purged");
    let text = said(&l);
    assert!(text.contains("2 documents") && text.contains("left alone"), "both named, neutrally: {text:?}");
    assert!(!text.contains("1 document was no longer"), "not the singular: {text:?}");
}

#[test]
fn t1_desktop_the_confirms_message_names_nothing_skipped_when_nothing_was() {
    let mut l = lanes("i9-t1-msg-none", 1);
    let x = l.doc_in(0, "X");
    let docs = l.docs();
    l.app.rt.block_on(docs.trash(&x)).unwrap();
    ask_to_empty(&mut l);
    let p = confirm_point(&l);
    l.app.handle_trash_modal_click(p, W, H);
    assert!(l.app.rt.block_on(docs.is_purged(&x)).unwrap());
    let text = said(&l);
    assert!(!text.contains("left alone") && !text.contains("no longer in the trash"), "nothing was skipped, nothing is named: {text:?}");
}

// T1 through the whole desktop path: Settings → Trash drawn, a peer's deletion
// applied, the confirmation clicked.
#[test]
fn t1_desktop_empty_trash_clicked_keeps_what_a_peer_trashed_after_the_view() {
    let mut l = lanes("i9-t1-app", 1);
    let a = l.doc_in(0, "A");
    let c = l.doc_in(0, "C");
    let docs = l.docs();
    let phone = l.app.rt.block_on(fixture("i9-t1-app-phone", "phone")).1;
    l.app.rt.block_on(send(&docs, &phone, &c));
    l.app.rt.block_on(docs.trash(&a)).unwrap();
    ask_to_empty(&mut l);

    l.app.rt.block_on(peer_deletes(&docs, &phone, &c));

    let p = confirm_point(&l);
    l.app.handle_trash_modal_click(p, W, H);
    assert!(l.app.rt.block_on(docs.is_purged(&a)).unwrap(), "A, shown, purged");
    assert!(!l.app.rt.block_on(docs.is_purged(&c)).unwrap(), "C, never shown, not purged");
    assert_eq!(l.app.rt.block_on(trash_ids(&docs)), vec![c.clone()], "C stays in the trash");
}

// ======================================================================= T2

const PHONE: &str = "peer-phone-i9";

/// A document written on the phone for `thread_id`, arriving by sync.
async fn arrive(docs: &DocumentService, id: &str, thread_id: &str, title: &str) {
    let content = serde_json::json!({"format": "sovereign.doc", "v": 2,
        "blocks": [{"type": "text", "id": "00000000000000000000000000001001", "markup": "markdown", "text": "orphan text"}]})
    .to_string();
    let remote = RemoteDocument {
        id: id.into(),
        title: title.into(),
        content,
        thread_id: thread_id.into(),
        trust: Some(Trust::Owned),
        provenance: None,
        modified_at: chrono::Utc::now(),
        deleted_at: None,
        device: PHONE.into(),
        trust_decision: None,
    };
    let meta = WireMeta {
        stamp: Some(Stamp { wall_ms: chrono::Utc::now().timestamp_millis(), counter: 0, device: PHONE.into() }),
        ..WireMeta::default()
    };
    docs.apply_wire(remote, meta).await.expect("applied");
}

async fn waiting(docs: &DocumentService) -> Vec<String> {
    sorted(docs.orphans_of_deleted_threads().await.unwrap().into_iter().map(|o| o.id).collect())
}

/// "Old drafts" deleted here; X and Y arrive for it; the section is drawn.
async fn orphan_group(tag: &str) -> (Arc<EncryptedGraphDB>, DocumentService, OrphansSection, String, String) {
    let (store, docs) = fixture(tag, "desktop").await;
    let gone = thread(&store, "Old drafts").await;
    let keep = thread(&store, "Keep").await;
    delete_thread_to_trash(&docs, store.as_ref(), &gone).await.expect("thread deleted");
    arrive(&docs, "document:i9x", &gone, "X").await;
    arrive(&docs, "document:i9y", &gone, "Y").await;
    let section = OrphansSection::load(&docs, &Default::default()).await.unwrap();
    assert_eq!(section.count(), 2, "setup: the group shows X and Y");
    (store, docs, section, gone, keep)
}

// T2 — "'Move all to a thread…', 'All in a new thread…' and 'Trash all' apply
// to the ids the group listed when it was drawn, those of them still waiting.
// A document that joined the group after is left waiting, and is listed at the
// next refresh. The result says how many were acted on." Desktop.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn t2_desktop_trash_all_leaves_a_document_that_joined_later_waiting() {
    let (_store, docs, mut section, gone, _keep) = orphan_group("i9-t2-trash").await;
    arrive(&docs, "document:i9z", &gone, "Z").await; // after the section was drawn

    let n = section.resolve_group(&docs, &gone, OrphanResolution::Trash).await.expect("resolved");
    assert_eq!(n, 2, "how many were acted on: what was shown");
    for id in ["document:i9x", "document:i9y"] {
        assert!(docs.read(id).await.unwrap().trashed_at.is_some(), "{id} in the trash");
    }
    assert!(docs.read("document:i9z").await.unwrap().trashed_at.is_none(), "Z was not shown: not trashed");
    assert_eq!(waiting(&docs).await, vec!["document:i9z".to_string()], "Z still waits");
    let next = OrphansSection::load(&docs, &Default::default()).await.unwrap();
    assert_eq!(next.lines().iter().map(|l| l.id.clone()).collect::<Vec<_>>(), vec!["document:i9z".to_string()], "listed at the next refresh");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn t2_desktop_move_all_to_a_thread_leaves_a_document_that_joined_later_waiting() {
    let (_store, docs, mut section, gone, keep) = orphan_group("i9-t2-move").await;
    arrive(&docs, "document:i9z", &gone, "Z").await;

    let n = section.resolve_group(&docs, &gone, OrphanResolution::MoveTo(keep.clone())).await.expect("resolved");
    assert_eq!(n, 2);
    for id in ["document:i9x", "document:i9y"] {
        assert_eq!(docs.read(id).await.unwrap().thread_id, keep, "{id} moved");
    }
    assert_eq!(docs.read("document:i9z").await.unwrap().thread_id, gone, "Z not moved");
    assert_eq!(waiting(&docs).await, vec!["document:i9z".to_string()]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn t2_desktop_all_in_a_new_thread_leaves_a_document_that_joined_later_waiting() {
    let (_store, docs, mut section, gone, _keep) = orphan_group("i9-t2-new").await;
    arrive(&docs, "document:i9z", &gone, "Z").await;

    let n = section.resolve_group(&docs, &gone, OrphanResolution::NewThread("Revived".into())).await.expect("resolved");
    assert_eq!(n, 2);
    let x_thread = docs.read("document:i9x").await.unwrap().thread_id;
    assert_ne!(x_thread, gone, "X moved to the new thread");
    assert_eq!(docs.read("document:i9y").await.unwrap().thread_id, x_thread, "Y with it");
    assert_eq!(docs.read("document:i9z").await.unwrap().thread_id, gone, "Z not moved");
    assert_eq!(waiting(&docs).await, vec!["document:i9z".to_string()]);
}

// ======================================================================= T3

// T3 — "In the second case [already in the trash], the desktop … say[s]
// 'Already in the trash', with no Undo. An Undo restores only a deletion that
// this Delete made." The card is stale: a peer's deletion arrived after the
// canvas was drawn.
#[test]
fn t3_desktop_delete_of_a_document_a_peer_already_trashed_says_so_with_no_undo() {
    let mut l = lanes("i9-t3-already", 1);
    let id = l.doc_in(0, "Doc");
    let docs = l.docs();
    let phone = l.app.rt.block_on(fixture("i9-t3-already-phone", "phone")).1;
    l.app.rt.block_on(send(&docs, &phone, &id));
    l.app.rt.block_on(peer_deletes(&docs, &phone, &id));
    let when = l.app.rt.block_on(docs.read(&id)).unwrap().trashed_at;

    l.app.trash_document(&id); // the card's Delete, one click
    assert!(l.app.delete_notice.is_none(), "no Undo: this Delete made no deletion");
    let text = said(&l).to_lowercase();
    assert!(text.contains("already in the trash"), "it says so: {text:?}");

    l.app.undo_delete(); // whatever Undo there is: nothing comes back
    let v = l.app.rt.block_on(docs.read(&id)).unwrap();
    assert_eq!(v.trashed_at, when, "still in the trash, as the peer put it");
}

// T3 — the first case, unchanged: a Delete that moved the document offers
// Undo, and that Undo restores it.
#[test]
fn t3_desktop_delete_that_moved_the_document_offers_undo_for_it() {
    let mut l = lanes("i9-t3-moved", 1);
    let id = l.doc_in(0, "Doc");
    let docs = l.docs();
    l.app.trash_document(&id);
    assert!(l.app.rt.block_on(docs.read(&id)).unwrap().trashed_at.is_some(), "moved to the trash");
    let notice = l.app.delete_notice.as_ref().expect("Undo offered");
    assert_eq!(notice.doc_id(), id);
    l.app.undo_delete();
    assert!(l.app.rt.block_on(docs.read(&id)).unwrap().trashed_at.is_none(), "Undo brought it back");
}
