//! C-DOC adoption step 5 in the native shell (`doc/plans/C-DOC-adoption.md`,
//! *Step 5 — delete, trash, purge in every host*). Written before the code, by
//! another author: whoever implements step 5 makes these pass without editing
//! this file.
//!
//! - **S5-4 Delete targets by id (AT8)** — through `card_menu` (the menu's
//!   state and its trash call; `App` itself needs a window and a GPU, so its
//!   wiring is checked by a scan of `app.rs` below and by hand).
//! - **S5-5 Trash view**, **S5-6 Restore from trash**, **S5-7 Empty trash** —
//!   through `trash::TrashView` (state and service calls, no drawing).
//! - **S5-8 One click and Undo** (Céline, Q3 → b, 2026-10-17) — the card
//!   menu's Delete deletes on the first click; `delete_notice::DeleteNotice`
//!   (the notice's text and its Undo, with an injected time).
//! - **S5-10 Deleting a thread**, as the trash view shows it.
//!
//! Revised 2026-10-17 after Céline's answers (Q2: a deleted thread's documents
//! go to the trash; restoring one whose thread is gone asks for a thread; Q3:
//! one click and Undo). Not here: a per-item "delete forever" (Q1: none), the
//! rendering (Settings → Trash, the modals: `TRASH_CHECKLIST.md`, by hand).
//! S5-3 (the purge tick) and S5-10's core are tested in `sovereign-docs`.
//!
//! Stores: a real encrypted store over an in-memory SurrealDB
//! (`docs_host::testing`); nothing touches a real workspace.

use std::sync::Arc;
use std::time::Instant;

use chrono::Duration;
use sovereign_ai::docs_host::{build_document_service, new_user_document, testing};
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::schema::{RelationType, Thread};
use sovereign_db::traits::GraphDB;
use sovereign_docs::DocumentService;

use crate::card_menu::{trash_target, CardMenu};
use crate::delete_notice::{DeleteNotice, UNDO_WINDOW};
use crate::trash::TrashView;

/// Text that must never appear in the trash view.
const CONTENT_MARKER: &str = "zebra-content-marker-42";

async fn fixture(tag: &str) -> (Arc<EncryptedGraphDB>, DocumentService) {
    let store = testing::encrypted_memory_store(tag).await;
    let docs = build_document_service(store.clone(), Some(testing::account_key()), "desktop".into());
    (store, docs)
}

async fn thread(store: &EncryptedGraphDB, name: &str) -> String {
    store.create_thread(Thread::new(name.into(), String::new())).await.unwrap().id_string().unwrap()
}

async fn create(docs: &DocumentService, title: &str, thread_id: &str, text: &str) -> String {
    docs.create(new_user_document(title, thread_id, text)).await.unwrap().id
}

/// Lets the system clock move, so two trash times differ.
async fn tick() {
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
}

async fn trashed(docs: &DocumentService) -> Vec<String> {
    docs.list_trash().await.unwrap().into_iter().map(|e| e.id).collect()
}

/// One click on Delete. Céline, Q3 → (b), 2026-10-17: "One click; the notice
/// says the document is in the trash and can be restored for 30 days, with
/// Undo." A click that only arms a confirmation fails here.
fn delete_from_menu(menu: &mut CardMenu) -> Option<String> {
    let first = menu.click_delete();
    assert!(first.is_some(), "Delete must delete on the first click (one click, with Undo — Q3 → b)");
    first
}

/// The thread is gone (its record soft-deleted, as sync or the thread delete
/// leaves it). Test setup only.
async fn thread_gone(store: &EncryptedGraphDB, thread_id: &str) {
    store.soft_delete_thread(thread_id).await.unwrap();
}

// ------------------------------------------------------------------ S5-4 --

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_4_delete_targets_the_document_the_menu_opened_on_across_reloads() {
    let (store, docs) = fixture("s5-4-reload").await;
    let lane = thread(&store, "Lane").await;
    let a = create(&docs, "A", &lane, "alpha").await;
    let b = create(&docs, "B", &lane, "beta").await;

    // The menu opens on card 0, which is A.
    let mut shown = vec![a.clone(), b.clone()];
    let mut menu = CardMenu::open_on(&shown, 0).expect("menu on card 0");
    assert_eq!(menu.target_id(), a);

    // A reload before the click: a document created from chat now comes first
    // (with one click there is no "between arming and confirming" any more).
    let n = create(&docs, "From chat", &lane, "new").await;
    shown = vec![n.clone(), a.clone(), b.clone()];
    assert_ne!(shown[0], a, "card 0 is no longer A");
    let target = delete_from_menu(&mut menu);
    assert_eq!(target.as_deref(), Some(a.as_str()), "the document the menu opened on, whatever the card order");

    trash_target(&docs, &a).await.expect("trashed");
    assert_eq!(trashed(&docs).await, vec![a.clone()], "A, and only A, is in the trash");
    for (id, text) in [(&b, "beta"), (&n, "new")] {
        let v = docs.read(id).await.unwrap();
        assert!(v.trashed_at.is_none(), "{id} untouched");
        assert_eq!(v.plain_text(), text);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_4_confirming_on_a_document_trashed_meanwhile_does_nothing_and_says_so() {
    let (store, docs) = fixture("s5-4-trashed").await;
    let lane = thread(&store, "Lane").await;
    let a = create(&docs, "A", &lane, "alpha").await;
    let b = create(&docs, "B", &lane, "beta").await;
    let mut menu = CardMenu::open_on(&[a.clone(), b.clone()], 0).unwrap();

    // Meanwhile A goes to the trash (from the phone, through sync).
    docs.trash(&a).await.unwrap();
    let when = docs.list_trash().await.unwrap()[0].trashed_at;
    tick().await;

    let target = delete_from_menu(&mut menu).expect("the click deletes");
    assert_eq!(target, a);
    let r = trash_target(&docs, &target).await;
    let msg = r.expect_err("a document already gone must say so");
    assert!(!msg.trim().is_empty(), "the notice says something");
    let trash = docs.list_trash().await.unwrap();
    assert_eq!(trash.len(), 1, "nothing else trashed");
    assert_eq!((trash[0].id.clone(), trash[0].trashed_at), (a.clone(), when), "A left as it was");
    assert!(docs.read(&b).await.unwrap().trashed_at.is_none(), "B untouched");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_4_confirming_on_a_document_purged_meanwhile_does_nothing_and_says_so() {
    let (store, docs) = fixture("s5-4-purged").await;
    let lane = thread(&store, "Lane").await;
    let a = create(&docs, "A", &lane, "alpha").await;
    let b = create(&docs, "B", &lane, "beta").await;
    let mut menu = CardMenu::open_on(&[a.clone(), b.clone()], 0).unwrap();

    docs.trash(&a).await.unwrap();
    docs.empty_trash().await.unwrap();

    let target = delete_from_menu(&mut menu).expect("the click deletes");
    assert_eq!(target, a);
    let msg = trash_target(&docs, &target).await.expect_err("a purged document must say so");
    assert!(!msg.trim().is_empty());
    assert!(docs.list_trash().await.unwrap().is_empty(), "nothing trashed");
    assert!(docs.read(&b).await.unwrap().trashed_at.is_none(), "B untouched");
}

#[test]
fn s5_4_run_skill_targets_the_document_the_menu_opened_on() {
    let (a, b) = ("document:a".to_string(), "document:b".to_string());
    let mut menu = CardMenu::open_on(&[a.clone(), b.clone()], 1).expect("menu on card 1");
    assert_eq!(menu.target_id(), b);
    menu.open_skills();
    assert_eq!(menu.target_id(), b, "the skills submenu is for the same document");
    assert_eq!(menu.click_skill(3), (b, 3));
}

/// The menu and the drag in `App` no longer name a card by its index (AT8).
/// `App` cannot be built without a window, so this reads `app.rs`: the
/// variants and the field that carried a card index for Delete, RunSkill (and
/// the targets that lead to them) and the card drag must be gone. A variant or
/// field that carries the document id passes.
#[test]
fn s5_4_app_menu_and_drag_carry_no_card_index() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/app.rs")).unwrap();
    // Production part: before the trailing test module.
    let prod = match src.find("#[cfg(test)]\nmod taskbar_tests") {
        Some(at) => &src[..at],
        None => &src[..],
    };
    let index_carrying = [
        "Card(usize)",
        "Skills(usize)",
        "Delete(usize)",
        "OpenSkills(usize)",
        "RunSkill(usize",
        "card_drag:Option<usize>",
    ];
    let mut found = vec![];
    for (n, line) in prod.lines().enumerate() {
        let code = line.split("//").next().unwrap_or("");
        let squashed: String = code.chars().filter(|c| !c.is_whitespace()).collect();
        for pat in index_carrying {
            let mut from = 0;
            while let Some(rel) = squashed[from..].find(pat) {
                let at = from + rel;
                from = at + pat.len();
                let ident_before = squashed[..at].chars().last().is_some_and(|c| c.is_alphanumeric() || c == '_');
                if !ident_before {
                    found.push(format!("app.rs:{}: {pat}", n + 1));
                }
            }
        }
    }
    assert!(found.is_empty(), "the menu or the drag still names a card by index:\n{}", found.join("\n"));
}

// ------------------------------------------------------------------ S5-5 --

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_5_the_trash_view_lists_newest_first_with_thread_date_days_left_and_no_content() {
    let (store, docs) = fixture("s5-5-view").await;
    let research = thread(&store, "Research").await;
    let admin = thread(&store, "Admin").await;
    let a = create(&docs, "Alpha", &research, CONTENT_MARKER).await;
    let b = create(&docs, "Beta", &admin, CONTENT_MARKER).await;
    let live = create(&docs, "Live", &admin, "live").await;
    docs.trash(&a).await.unwrap();
    tick().await;
    docs.trash(&b).await.unwrap();
    let entries = docs.list_trash().await.unwrap();
    let (tb, ta) = (entries[0].trashed_at, entries[1].trashed_at);

    let view = TrashView::load(&docs, store.as_ref(), ta + Duration::days(10)).await.expect("load");
    let got: Vec<(&str, &str, &str)> =
        view.lines().iter().map(|l| (l.id.as_str(), l.title.as_str(), l.thread_name.as_str())).collect();
    assert_eq!(got, vec![(b.as_str(), "Beta", "Admin"), (a.as_str(), "Alpha", "Research")], "newest first");
    assert!(view.lines().iter().all(|l| l.id != live), "a live document is not in the trash");
    assert_eq!((view.lines()[0].trashed_at, view.lines()[1].trashed_at), (tb, ta), "the date trashed");
    assert_eq!(view.lines()[1].days_left, 20, "trashed 10 days before: 20 days left");
    let shown = format!("{:?}", view.lines());
    assert!(!shown.contains(CONTENT_MARKER), "content in the trash view: {shown}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_5_days_left_count_down_from_30() {
    let (store, docs) = fixture("s5-5-days").await;
    let lane = thread(&store, "Lane").await;
    let a = create(&docs, "A", &lane, "a").await;
    docs.trash(&a).await.unwrap();
    let t = docs.list_trash().await.unwrap()[0].trashed_at;
    for (elapsed, left) in [(0, 30), (1, 29), (29, 1)] {
        let view = TrashView::load(&docs, store.as_ref(), t + Duration::days(elapsed)).await.unwrap();
        assert_eq!(view.lines()[0].days_left, left, "{elapsed} days after trashing");
    }
}

/// Revised after Céline's answer to Q2 (2026-10-17; plan, *What the answers
/// change*: "a trashed document whose thread is gone shows that its thread was
/// deleted (not the fallback lane's name)").
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_5_a_document_whose_thread_is_gone_says_its_thread_was_deleted() {
    let (store, docs) = fixture("s5-5-gone").await;
    let lane = thread(&store, "Lane").await;
    let a = create(&docs, "A", &lane, "a").await;
    docs.trash(&a).await.unwrap();
    thread_gone(&store, &lane).await;
    let t = docs.list_trash().await.unwrap()[0].trashed_at;
    let view = TrashView::load(&docs, store.as_ref(), t).await.unwrap();
    assert_eq!(view.lines().len(), 1);
    // The label decided by Céline (2026-10-17): the thread's old name, then
    // " - deleted" ("ancien nom - supprimé").
    let shown = &view.lines()[0].thread_name;
    assert_eq!(shown, "Lane - deleted", "the line says its thread was deleted");
    assert!(!shown.contains(crate::canvas::FALLBACK_LANE_NAME), "never the fallback lane's name: {shown:?}");
}

/// S5-10 as the trash view shows it: after a thread delete, its documents are
/// in the trash and each line says its thread was deleted.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_10_a_deleted_threads_documents_are_in_the_trash_saying_so() {
    let (store, docs) = fixture("s5-10-view").await;
    let lane = thread(&store, "Old drafts").await;
    let keep = thread(&store, "Keep").await;
    let a = create(&docs, "A", &lane, "a").await;
    let b = create(&docs, "B", &lane, "b").await;
    let k = create(&docs, "K", &keep, "k").await;
    sovereign_docs::delete_thread_to_trash(&docs, store.as_ref(), &lane).await.expect("thread deleted");
    let view = TrashView::load(&docs, store.as_ref(), chrono::Utc::now()).await.unwrap();
    let mut ids: Vec<&str> = view.lines().iter().map(|l| l.id.as_str()).collect();
    ids.sort();
    let mut expected = vec![a.as_str(), b.as_str()];
    expected.sort();
    assert_eq!(ids, expected);
    for l in view.lines() {
        assert_eq!(l.thread_name, "Old drafts - deleted", "the thread's old name, then \" - deleted\"");
    }
    assert!(docs.read(&k).await.unwrap().trashed_at.is_none(), "the other thread's document stays");
}

// ------------------------------------------------------------------ S5-6 --

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_6_restore_brings_the_document_back_exactly_and_lists_it_at_once() {
    let (store, docs) = fixture("s5-6-restore").await;
    let lane = thread(&store, "Lane").await;
    let a = create(&docs, "A", &lane, "alpha").await;
    let b = create(&docs, "B", &lane, "beta").await;
    let c = create(&docs, "C", &lane, "gamma").await;
    store.create_relationship(&a, &b, RelationType::References, 0.5).await.unwrap();
    let before = store.get_document(&a).await.unwrap();
    let view_before = docs.read(&a).await.unwrap();
    let versions = store.list_document_commits(&a).await.unwrap().len();
    docs.trash(&a).await.unwrap();
    tick().await;
    docs.trash(&c).await.unwrap();

    let now = chrono::Utc::now();
    let mut view = TrashView::load(&docs, store.as_ref(), now).await.unwrap();
    view.restore(&docs, store.as_ref(), &a, now).await.expect("restore");

    let ids: Vec<&str> = view.lines().iter().map(|l| l.id.as_str()).collect();
    assert_eq!(ids, vec![c.as_str()], "gone from the view at once; the other stays");
    let after = store.get_document(&a).await.unwrap();
    assert_eq!((&after.title, &after.content, &after.thread_id), (&before.title, &before.content, &before.thread_id));
    let v = docs.read(&a).await.unwrap();
    assert_eq!((v.trust, v.provenance.clone(), v.trashed_at), (view_before.trust, view_before.provenance, None));
    assert_eq!(store.list_document_commits(&a).await.unwrap().len(), versions, "versions");
    assert_eq!(store.list_outgoing_relationships(&a).await.unwrap().len(), 1, "links");
    assert!(docs.list(Some(lane.as_str())).await.unwrap().iter().any(|d| d.id == a), "listed again, in its thread");
    assert_eq!(trashed(&docs).await, vec![c.clone()]);
}

/// Revised after Céline's answer to Q2 (spec ch. 03, *Soft delete and 30-day
/// purge*, 2026-10-17: "if its thread was deleted, the user chooses the thread
/// it goes to"). Without a choice nothing is restored, and it says why.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_6_restore_when_the_thread_is_gone_needs_a_thread_and_says_so() {
    let (store, docs) = fixture("s5-6-gone").await;
    let lane = thread(&store, "Lane").await;
    let a = create(&docs, "A", &lane, "alpha").await;
    docs.trash(&a).await.unwrap();
    thread_gone(&store, &lane).await;
    let when = docs.list_trash().await.unwrap()[0].trashed_at;
    let now = chrono::Utc::now();
    let mut view = TrashView::load(&docs, store.as_ref(), now).await.unwrap();

    let msg = view.restore(&docs, store.as_ref(), &a, now).await.expect_err("a thread must be chosen");
    assert!(msg.to_lowercase().contains("thread"), "it says why: {msg:?}");
    let trash = docs.list_trash().await.unwrap();
    assert_eq!((trash.len(), trash[0].id.clone(), trash[0].trashed_at), (1, a.clone(), when), "still in the trash, as it was");
    assert_eq!(view.lines().len(), 1, "still in the view");
    assert!(docs.list(None).await.unwrap().iter().all(|d| d.id != a), "not listed: never into the fallback lane");
}

/// With a thread chosen — one the user has just created — the document comes
/// back exactly, in that thread, listed at once.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_6_restore_into_a_chosen_thread_brings_it_back_exactly_there() {
    let (store, docs) = fixture("s5-6-into").await;
    let lane = thread(&store, "Lane").await;
    let a = create(&docs, "A", &lane, "alpha").await;
    let b = create(&docs, "B", &lane, "beta").await;
    store.create_relationship(&a, &b, RelationType::References, 0.5).await.unwrap();
    let before = store.get_document(&a).await.unwrap();
    let view_before = docs.read(&a).await.unwrap();
    let versions = store.list_document_commits(&a).await.unwrap().len();
    docs.trash(&a).await.unwrap();
    thread_gone(&store, &lane).await;
    let revived = thread(&store, "Lane again").await;

    let now = chrono::Utc::now();
    let mut view = TrashView::load(&docs, store.as_ref(), now).await.unwrap();
    view.restore_into(&docs, store.as_ref(), &a, &revived, now).await.expect("restore into the chosen thread");

    let after = store.get_document(&a).await.unwrap();
    assert_eq!(after.thread_id, revived, "in the chosen thread");
    assert_eq!((&after.title, &after.content), (&before.title, &before.content), "content");
    let v = docs.read(&a).await.unwrap();
    assert_eq!((v.trust, v.provenance.clone(), v.trashed_at), (view_before.trust, view_before.provenance, None));
    assert_eq!(store.list_outgoing_relationships(&a).await.unwrap().len(), 1, "links");
    assert!(store.list_document_commits(&a).await.unwrap().len() >= versions, "versions kept");
    assert!(docs.list(Some(revived.as_str())).await.unwrap().iter().any(|d| d.id == a), "listed at once, there");
    assert!(view.lines().is_empty(), "gone from the view at once");
}

/// The chosen thread must be an existing one: a deleted or unknown thread is
/// refused and nothing is restored.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_6_restore_into_a_thread_that_does_not_exist_is_refused() {
    let (store, docs) = fixture("s5-6-into-bad").await;
    let lane = thread(&store, "Lane").await;
    let other = thread(&store, "Other").await;
    let a = create(&docs, "A", &lane, "alpha").await;
    docs.trash(&a).await.unwrap();
    thread_gone(&store, &lane).await;
    thread_gone(&store, &other).await;
    let now = chrono::Utc::now();
    let mut view = TrashView::load(&docs, store.as_ref(), now).await.unwrap();
    for bad in [other.as_str(), "thread:nope", lane.as_str()] {
        let r = view.restore_into(&docs, store.as_ref(), &a, bad, now).await;
        assert!(r.is_err(), "restoring into {bad} must be refused");
        assert_eq!(trashed(&docs).await, vec![a.clone()], "nothing restored ({bad})");
    }
}

// ------------------------------------------- S5-6, grouped restore ---------
//
// Céline, 2026-10-17 (*Three more answers*; spec ch. 03, *Thread deletion*):
// "In the trash, a deleted thread's documents are grouped, and can be restored
// all at once into a thread the user chooses, or one by one."

/// Two deleted threads (X with two documents, Y with one) and a trashed
/// document of a live thread.
struct Groups {
    store: Arc<EncryptedGraphDB>,
    docs: DocumentService,
    x: String,
    y: String,
    live: String,
    x1: String,
    x2: String,
    y1: String,
    l1: String,
}

async fn groups(tag: &str) -> Groups {
    let (store, docs) = fixture(tag).await;
    let x = thread(&store, "X").await;
    let y = thread(&store, "Y").await;
    let live = thread(&store, "Live").await;
    let x1 = create(&docs, "X one", &x, "x one").await;
    let x2 = create(&docs, "X two", &x, "x two").await;
    let y1 = create(&docs, "Y one", &y, "y one").await;
    let l1 = create(&docs, "L one", &live, "l one").await;
    for id in [&x1, &x2, &y1, &l1] {
        docs.trash(id).await.unwrap();
        tick().await;
    }
    thread_gone(&store, &x).await;
    thread_gone(&store, &y).await;
    Groups { store, docs, x, y, live, x1, x2, y1, l1 }
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_6_the_trash_groups_documents_by_deleted_thread() {
    let g = groups("s5-6-groups").await;
    let view = TrashView::load(&g.docs, g.store.as_ref(), chrono::Utc::now()).await.unwrap();
    let mut got: Vec<(String, Vec<String>)> =
        view.deleted_thread_groups().into_iter().map(|gr| (gr.thread_id, sorted(gr.ids))).collect();
    got.sort();
    let mut expected = vec![(g.x.clone(), sorted(vec![g.x1.clone(), g.x2.clone()])), (g.y.clone(), vec![g.y1.clone()])];
    expected.sort();
    assert_eq!(got, expected, "one group per deleted thread, with exactly its documents");
    for gr in view.deleted_thread_groups() {
        let name = if gr.thread_id == g.x { "X" } else { "Y" };
        assert_eq!(gr.label, format!("{name} - deleted"), "the group's heading: the old name, then \" - deleted\"");
        assert!(!gr.ids.contains(&g.l1), "a live thread's document is in no group");
    }
    assert_eq!(view.lines().len(), 4, "every document is still a line of its own");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_6_restore_all_brings_a_deleted_threads_documents_into_the_chosen_thread_at_once() {
    let g = groups("s5-6-group-restore").await;
    let befores = [g.store.get_document(&g.x1).await.unwrap(), g.store.get_document(&g.x2).await.unwrap()];
    let target = thread(&g.store, "X again").await;
    let now = chrono::Utc::now();
    let mut view = TrashView::load(&g.docs, g.store.as_ref(), now).await.unwrap();

    let n = view.restore_group_into(&g.docs, g.store.as_ref(), &g.x, &target, now).await.expect("restore all");
    assert_eq!(n, 2);
    for (id, before) in [(&g.x1, &befores[0]), (&g.x2, &befores[1])] {
        let after = g.store.get_document(id).await.unwrap();
        assert_eq!(after.thread_id, target, "{id} in the chosen thread");
        assert_eq!((&after.title, &after.content), (&before.title, &before.content), "{id} exactly");
        assert!(g.docs.read(id).await.unwrap().trashed_at.is_none());
    }
    let listed: Vec<String> = g.docs.list(Some(target.as_str())).await.unwrap().into_iter().map(|d| d.id).collect();
    assert_eq!(sorted(listed), sorted(vec![g.x1.clone(), g.x2.clone()]), "listed at once, there");
    let shown: Vec<String> = view.lines().iter().map(|l| l.id.clone()).collect();
    assert_eq!(sorted(shown), sorted(vec![g.y1.clone(), g.l1.clone()]), "gone from the view at once; the others stay");
    assert_eq!(sorted(trashed(&g.docs).await), sorted(vec![g.y1.clone(), g.l1.clone()]));
    let _ = &g.live;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_6_one_document_of_a_group_can_be_restored_alone() {
    let g = groups("s5-6-group-one").await;
    let target = thread(&g.store, "Elsewhere").await;
    let now = chrono::Utc::now();
    let mut view = TrashView::load(&g.docs, g.store.as_ref(), now).await.unwrap();
    view.restore_into(&g.docs, g.store.as_ref(), &g.x1, &target, now).await.expect("restore one");
    assert_eq!(g.store.get_document(&g.x1).await.unwrap().thread_id, target);
    let x_group = view.deleted_thread_groups().into_iter().find(|gr| gr.thread_id == g.x).expect("group X remains");
    assert_eq!(x_group.ids, vec![g.x2.clone()], "the rest of the group stays in the trash");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_6_restore_all_into_a_thread_that_does_not_exist_is_refused() {
    let g = groups("s5-6-group-bad").await;
    let now = chrono::Utc::now();
    let mut view = TrashView::load(&g.docs, g.store.as_ref(), now).await.unwrap();
    for bad in [g.y.as_str(), "thread:nope", g.x.as_str()] {
        let r = view.restore_group_into(&g.docs, g.store.as_ref(), &g.x, bad, now).await;
        assert!(r.is_err(), "restoring all into {bad} must be refused");
        assert_eq!(
            sorted(trashed(&g.docs).await),
            sorted(vec![g.x1.clone(), g.x2.clone(), g.y1.clone(), g.l1.clone()]),
            "nothing restored ({bad})"
        );
    }
}

// ------------------------------------------------------------------ S5-7 --

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_7_empty_trash_asks_first_then_purges_and_reports_how_many() {
    let (store, docs) = fixture("s5-7-empty").await;
    let lane = thread(&store, "Lane").await;
    let a = create(&docs, "A", &lane, "alpha").await;
    let b = create(&docs, "B", &lane, "beta").await;
    let live = create(&docs, "Live", &lane, "live").await;
    docs.trash(&a).await.unwrap();
    docs.trash(&b).await.unwrap();
    let mut view = TrashView::load(&docs, store.as_ref(), chrono::Utc::now()).await.unwrap();

    // Asking purges nothing.
    assert!(!view.empty_requested());
    view.request_empty();
    assert!(view.empty_requested(), "the confirmation shows");
    assert_eq!(trashed(&docs).await.len(), 2, "asking purges nothing");

    let n = view.confirm_empty(&docs).await.expect("empty");
    assert_eq!(n, 2, "how many were purged");
    assert!(docs.list_trash().await.unwrap().is_empty());
    for id in [&a, &b] {
        assert!(docs.is_purged(id).await.unwrap(), "{id} purged");
    }
    assert!(view.lines().is_empty(), "the view is empty");
    assert!(!view.empty_requested(), "the confirmation closes");
    assert!(docs.read(&live).await.is_ok(), "nothing live is touched");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_7_empty_trash_never_runs_without_its_confirmation() {
    let (store, docs) = fixture("s5-7-guard").await;
    let lane = thread(&store, "Lane").await;
    let a = create(&docs, "A", &lane, "alpha").await;
    docs.trash(&a).await.unwrap();
    let mut view = TrashView::load(&docs, store.as_ref(), chrono::Utc::now()).await.unwrap();

    // Confirming with no confirmation showing purges nothing.
    let r = view.confirm_empty(&docs).await;
    assert!(!matches!(r, Ok(n) if n > 0), "purged without asking: {r:?}");
    assert_eq!(trashed(&docs).await, vec![a.clone()]);

    // Asked, then dismissed: still nothing.
    view.request_empty();
    view.cancel_empty();
    assert!(!view.empty_requested());
    let r = view.confirm_empty(&docs).await;
    assert!(!matches!(r, Ok(n) if n > 0), "purged after the confirmation was dismissed: {r:?}");
    assert_eq!(trashed(&docs).await, vec![a.clone()]);
    assert!(!docs.is_purged(&a).await.unwrap());
}

// ------------------------------------------------------------------ S5-8 --
//
// Céline, Q3 → (b), 2026-10-17; spec ch. 03, *Soft delete and 30-day purge*,
// "the gestures": "Deleting a document is one click; the notice says it is in
// the trash and can be restored for 30 days, and offers Undo." Plan: the
// notice reads "Moved to the trash — it can be restored for 30 days", Undo
// restores it (`restore_from_trash`) while the notice is shown ([choice] 10 s).

/// A document trashed from the card menu, and its notice shown at `at`.
async fn deleted_with_notice(docs: &DocumentService, id: &str, at: Instant) -> DeleteNotice {
    let mut menu = CardMenu::open_on(&[id.to_string()], 0).unwrap();
    let target = delete_from_menu(&mut menu).unwrap();
    trash_target(docs, &target).await.expect("trashed");
    DeleteNotice::new(target, at)
}

#[test]
fn s5_8_the_card_menus_delete_deletes_on_the_first_click() {
    let mut menu = CardMenu::open_on(&["document:a".to_string()], 0).unwrap();
    assert_eq!(menu.click_delete().as_deref(), Some("document:a"), "one click, no confirmation step");
}

#[test]
fn s5_8_the_notice_says_where_the_document_went_and_for_how_long() {
    let n = DeleteNotice::new("document:a".into(), Instant::now());
    let text = n.text();
    assert!(text.to_lowercase().contains("trash"), "{text}");
    assert!(text.contains("30 days"), "{text}");
    assert_eq!(n.doc_id(), "document:a");
}

#[test]
fn s5_8_the_undo_window_is_ten_seconds() {
    assert_eq!(UNDO_WINDOW, std::time::Duration::from_secs(10), "[choice] 10 s (plan, S5-8)");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_8_undo_while_the_notice_is_shown_restores_the_document() {
    let (store, docs) = fixture("s5-8-undo").await;
    let lane = thread(&store, "Lane").await;
    let a = create(&docs, "A", &lane, "alpha").await;
    let before = store.get_document(&a).await.unwrap();
    let t0 = Instant::now();
    let mut notice = deleted_with_notice(&docs, &a, t0).await;
    assert_eq!(trashed(&docs).await, vec![a.clone()], "one click trashed it");

    let at = t0 + std::time::Duration::from_secs(3);
    assert!(notice.shown(at));
    notice.undo(&docs, at).await.expect("undo");
    assert!(trashed(&docs).await.is_empty(), "out of the trash");
    let after = store.get_document(&a).await.unwrap();
    assert_eq!((&after.title, &after.content, &after.thread_id), (&before.title, &before.content, &before.thread_id));
    assert!(docs.list(Some(lane.as_str())).await.unwrap().iter().any(|d| d.id == a), "back in its thread");

    // A second Undo has nothing left to undo: it does nothing and says so.
    let msg = notice.undo(&docs, at).await.expect_err("nothing to undo");
    assert!(!msg.trim().is_empty());
    assert!(docs.read(&a).await.unwrap().trashed_at.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_8_after_the_window_undo_is_gone_and_the_document_stays_in_the_trash() {
    let (store, docs) = fixture("s5-8-late").await;
    let lane = thread(&store, "Lane").await;
    let a = create(&docs, "A", &lane, "alpha").await;
    let t0 = Instant::now();
    let mut notice = deleted_with_notice(&docs, &a, t0).await;
    assert!(notice.shown(t0 + std::time::Duration::from_secs(9)), "still shown at 9 s");
    let late = t0 + std::time::Duration::from_secs(11);
    assert!(!notice.shown(late), "gone at 11 s");
    assert!(notice.undo(&docs, late).await.is_err(), "no Undo once the notice is gone");
    assert_eq!(trashed(&docs).await, vec![a.clone()], "still in the trash, restorable from there");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_8_undo_after_the_document_left_the_trash_does_nothing_and_says_so() {
    let (store, docs) = fixture("s5-8-left").await;
    let lane = thread(&store, "Lane").await;
    let a = create(&docs, "A", &lane, "alpha").await;
    let b = create(&docs, "B", &lane, "beta").await;
    let t0 = Instant::now();
    let at = t0 + std::time::Duration::from_secs(2);

    // Restored meanwhile (from the trash view), then edited: Undo must not
    // touch it.
    let mut na = deleted_with_notice(&docs, &a, t0).await;
    docs.restore_from_trash(&a).await.unwrap();
    let state = store.get_document(&a).await.unwrap();
    let msg = na.undo(&docs, at).await.expect_err("restored meanwhile");
    assert!(!msg.trim().is_empty(), "it says so");
    let now = store.get_document(&a).await.unwrap();
    assert_eq!((now.content, now.modified_at, now.deleted_at), (state.content, state.modified_at, state.deleted_at));

    // Purged meanwhile (Empty trash): nothing comes back.
    let mut nb = deleted_with_notice(&docs, &b, t0).await;
    docs.empty_trash().await.unwrap();
    let msg = nb.undo(&docs, at).await.expect_err("purged meanwhile");
    assert!(!msg.trim().is_empty(), "it says so");
    assert!(docs.is_purged(&b).await.unwrap());
}

/// Undo after the document's thread was deleted meanwhile (within the Undo
/// window). Céline's rule (spec ch. 03, 2026-10-17): nothing comes back from
/// the trash into the fallback lane — a document whose thread was deleted goes
/// back only into a thread the user chooses. So Undo restores nothing and
/// says the thread is gone; the document stays in the trash, where it can be
/// restored with a thread choice.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_8_undo_after_its_thread_was_deleted_restores_nothing_and_says_so() {
    let (store, docs) = fixture("s5-8-thread-gone").await;
    let lane = thread(&store, "Lane").await;
    let a = create(&docs, "A", &lane, "alpha").await;
    let t0 = Instant::now();
    let mut notice = deleted_with_notice(&docs, &a, t0).await;
    let when = docs.list_trash().await.unwrap()[0].trashed_at;

    // Meanwhile the thread is deleted (the assistant, or the phone through sync).
    sovereign_docs::delete_thread_to_trash(&docs, store.as_ref(), &lane).await.expect("thread deleted");

    let at = t0 + std::time::Duration::from_secs(3);
    assert!(notice.shown(at), "still within the Undo window");
    let msg = notice.undo(&docs, at).await.expect_err("nothing comes back into the fallback lane");
    assert!(msg.to_lowercase().contains("thread"), "it says the thread is gone: {msg:?}");
    let trash = docs.list_trash().await.unwrap();
    assert_eq!((trash.len(), trash[0].id.clone(), trash[0].trashed_at), (1, a.clone(), when), "still in the trash, as it was");
    assert!(docs.list(None).await.unwrap().iter().all(|d| d.id != a), "not listed: not in the fallback lane");

    // From the trash, with a thread chosen, it comes back.
    let revived = thread(&store, "Lane again").await;
    let now = chrono::Utc::now();
    let mut view = TrashView::load(&docs, store.as_ref(), now).await.unwrap();
    view.restore_into(&docs, store.as_ref(), &a, &revived, now).await.expect("restore with a thread choice");
    assert_eq!(store.get_document(&a).await.unwrap().thread_id, revived);
}
