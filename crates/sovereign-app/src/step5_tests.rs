//! C-DOC adoption step 5 in the Tauri host and the CLI
//! (`doc/plans/C-DOC-adoption.md`, *Step 5 — delete, trash, purge in every
//! host*). Written before the code, by another author: whoever implements step
//! 5 makes these pass without editing this file.
//!
//! - **S5-2 The CLI trashes** — with the account, `delete-doc` moves the
//!   document to the trash; nothing is hard-deleted. (Without the account it
//!   refuses: `tests/cli_delete_trash.rs`.)
//! - **S5-9 Mobile**, limited to the Rust side of the Tauri commands
//!   `list_trash`, `restore_from_trash`, `empty_trash` (`doc_trash.rs`, the
//!   layer under the `#[tauri::command]` wrappers, as `doc_saves.rs` is for
//!   saves) and their registration. It carries S5-5 (the list), S5-6 (restore)
//!   and S5-7 (empty). The confirmation before emptying (K7) and S5-8 are the
//!   Svelte UI's (vitest: `frontend/src/lib/**/trash*.test.ts`,
//!   `deleteUndo*.test.ts`), not tested here.
//! - Revised 2026-10-17 after Céline's answers: restoring a document whose
//!   thread is gone needs a chosen thread (`restore_from_trash_to`, and the
//!   command's optional `thread_id`); **S5-10** the phone's thread delete
//!   sends the thread's documents to the trash (`delete_thread_to_trash`).
//!
//! Stores: a real encrypted store over an in-memory SurrealDB
//! (`docs_host::testing`); nothing touches a real workspace.

use std::sync::Arc;

use chrono::Duration;
use sovereign_ai::docs_host::{build_document_service, new_user_document, testing};
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::schema::{RelationType, Thread};
use sovereign_db::GraphDB;
use sovereign_docs::DocumentService;

use crate::commands::{create_doc_with, delete_doc_with};
use crate::doc_trash;

/// Text that must never appear in what the trash list returns.
const CONTENT_MARKER: &str = "zebra-content-marker-71";

async fn fixture(tag: &str) -> (Arc<EncryptedGraphDB>, DocumentService) {
    let store = testing::encrypted_memory_store(tag).await;
    let docs = build_document_service(store.clone(), Some(testing::account_key()), "phone".into());
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

// ------------------------------------------------------------------ S5-2 --

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_2_delete_doc_moves_the_document_to_the_trash_and_deletes_nothing() {
    let (store, docs) = fixture("s5-2-cli").await;
    let id = create_doc_with(&docs, "Notes".into(), "thread:t".into(), true).await.unwrap();
    let other = create_doc_with(&docs, "Other".into(), "thread:t".into(), true).await.unwrap();
    let versions = docs.versions(&id).await.unwrap().len();

    delete_doc_with(&docs, &id).await.expect("delete-doc with the account");

    let trash: Vec<String> = docs.list_trash().await.unwrap().into_iter().map(|e| e.id).collect();
    assert_eq!(trash, vec![id.clone()], "in the trash");
    assert!(docs.list(None).await.unwrap().iter().all(|d| d.id != id), "no longer listed");
    // Nothing hard-deleted: the row and its versions are still stored.
    let rows = store.list_documents_including_deleted().await.unwrap();
    let row = rows.iter().find(|d| d.id_string().as_deref() == Some(id.as_str()));
    assert!(row.is_some_and(|d| d.deleted_at.is_some()), "the row is kept, marked trashed");
    assert_eq!(store.list_document_commits(&id).await.unwrap().len(), versions, "versions kept");
    assert!(!docs.is_purged(&id).await.unwrap(), "not purged");
    // Recoverable, and the other document untouched.
    docs.restore_from_trash(&id).await.expect("recoverable from the trash");
    assert!(docs.read(&other).await.unwrap().trashed_at.is_none());
}

// ------------------------------------------------------- S5-9 (S5-5 list) --

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_9_list_trash_newest_first_with_thread_date_days_left_and_no_content() {
    let (store, docs) = fixture("s5-9-list").await;
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
    let now = ta + Duration::days(10);
    let list = doc_trash::list_trash(&docs, store.as_ref(), now).await.expect("list_trash");

    let got: Vec<(&str, &str, &str, Option<&str>)> = list
        .iter()
        .map(|e| (e.id.as_str(), e.title.as_str(), e.thread_id.as_str(), e.thread_name.as_deref()))
        .collect();
    assert_eq!(
        got,
        vec![
            (b.as_str(), "Beta", admin.as_str(), Some("Admin")),
            (a.as_str(), "Alpha", research.as_str(), Some("Research")),
        ],
        "newest first, with title and thread; the live document is not listed"
    );
    assert!(list.iter().all(|e| e.id != live));
    assert_eq!((list[0].trashed_at, list[1].trashed_at), (tb, ta), "the date trashed");
    assert_eq!(list[1].days_left, 20, "trashed 10 days before `now`: 20 days left");

    // No content: not in what the frontend receives.
    let json = serde_json::to_string(&list).unwrap();
    assert!(!json.contains(CONTENT_MARKER), "content in the trash list: {json}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_9_list_trash_days_left_count_down_from_30() {
    let (store, docs) = fixture("s5-9-days").await;
    let lane = thread(&store, "Lane").await;
    let a = create(&docs, "A", &lane, "a").await;
    docs.trash(&a).await.unwrap();
    let t = docs.list_trash().await.unwrap()[0].trashed_at;
    for (elapsed, left) in [(0, 30), (1, 29), (29, 1)] {
        let list = doc_trash::list_trash(&docs, store.as_ref(), t + Duration::days(elapsed)).await.unwrap();
        assert_eq!(list[0].days_left, left, "{elapsed} days after trashing");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_9_list_trash_says_when_the_thread_is_gone() {
    let (store, docs) = fixture("s5-9-gone").await;
    let lane = thread(&store, "Lane").await;
    let a = create(&docs, "A", &lane, "a").await;
    docs.trash(&a).await.unwrap();
    store.soft_delete_thread(&lane).await.unwrap();
    let t = docs.list_trash().await.unwrap()[0].trashed_at;
    let list = doc_trash::list_trash(&docs, store.as_ref(), t).await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].thread_id, lane, "the thread id is kept");
    assert_eq!(list[0].thread_name, None, "the thread is gone: the UI says it was deleted");
    // The label decided by Céline (2026-10-17) is the thread's old name, then
    // " - deleted": the line carries that old name as `deleted_thread_name`
    // (read from the serialized DTO, as the frontend receives it).
    let json = serde_json::to_value(&list[0]).unwrap();
    assert_eq!(json["deleted_thread_name"], "Lane", "the deleted thread's old name reaches the phone: {json}");
}

// ---------------------------------------------------- S5-9 (S5-6 restore) --

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_9_restore_from_trash_brings_the_document_back_exactly_and_lists_it_at_once() {
    let (store, docs) = fixture("s5-9-restore").await;
    let lane = thread(&store, "Lane").await;
    let a = create(&docs, "A", &lane, "alpha").await;
    let b = create(&docs, "B", &lane, "beta").await;
    store.create_relationship(&a, &b, RelationType::References, 0.5).await.unwrap();
    let before = store.get_document(&a).await.unwrap();
    let view = docs.read(&a).await.unwrap();
    let versions = store.list_document_commits(&a).await.unwrap().len();

    docs.trash(&a).await.unwrap();
    doc_trash::restore_from_trash(&docs, &a).await.expect("restore_from_trash");

    let after = store.get_document(&a).await.unwrap();
    assert_eq!((&after.title, &after.content, &after.thread_id), (&before.title, &before.content, &before.thread_id));
    let v = docs.read(&a).await.unwrap();
    assert_eq!((v.trust, v.trashed_at), (view.trust, None));
    assert_eq!(store.list_document_commits(&a).await.unwrap().len(), versions, "versions");
    assert_eq!(store.list_outgoing_relationships(&a).await.unwrap().len(), 1, "links");
    assert!(docs.list(Some(lane.as_str())).await.unwrap().iter().any(|d| d.id == a), "listed again, in its thread");
    let t = docs.list(None).await.unwrap();
    assert!(t.iter().any(|d| d.id == a));
    let trash = doc_trash::list_trash(&docs, store.as_ref(), chrono::Utc::now()).await.unwrap();
    assert!(trash.iter().all(|e| e.id != a), "no longer in the trash");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_9_restore_from_trash_when_the_thread_is_gone_needs_a_thread_and_says_so() {
    // Revised after Céline's answer to Q2 (spec ch. 03, 2026-10-17: "if its
    // thread was deleted, the user chooses the thread it goes to").
    let (store, docs) = fixture("s5-9-restore-gone").await;
    let lane = thread(&store, "Lane").await;
    let a = create(&docs, "A", &lane, "alpha").await;
    docs.trash(&a).await.unwrap();
    store.soft_delete_thread(&lane).await.unwrap();

    let msg = doc_trash::restore_from_trash_to(&docs, store.as_ref(), &a, None).await.expect_err("no thread chosen");
    assert!(msg.to_lowercase().contains("thread"), "it says why: {msg:?}");
    // The restore without a thread parameter is no back door either.
    assert!(doc_trash::restore_from_trash(&docs, &a).await.is_err(), "restore_from_trash without a thread");
    let trash: Vec<String> = docs.list_trash().await.unwrap().into_iter().map(|e| e.id).collect();
    assert_eq!(trash, vec![a.clone()], "still in the trash");
    assert!(docs.list(None).await.unwrap().iter().all(|d| d.id != a), "never into the fallback lane");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_9_restore_into_a_chosen_thread_brings_it_back_exactly_there() {
    let (store, docs) = fixture("s5-9-restore-into").await;
    let lane = thread(&store, "Lane").await;
    let a = create(&docs, "A", &lane, "alpha").await;
    let b = create(&docs, "B", &lane, "beta").await;
    store.create_relationship(&a, &b, RelationType::References, 0.5).await.unwrap();
    let before = store.get_document(&a).await.unwrap();
    let view = docs.read(&a).await.unwrap();
    docs.trash(&a).await.unwrap();
    store.soft_delete_thread(&lane).await.unwrap();
    let revived = thread(&store, "Lane again").await;

    doc_trash::restore_from_trash_to(&docs, store.as_ref(), &a, Some(revived.as_str())).await.expect("restore into");
    let after = store.get_document(&a).await.unwrap();
    assert_eq!(after.thread_id, revived, "in the chosen thread");
    assert_eq!((&after.title, &after.content), (&before.title, &before.content));
    let v = docs.read(&a).await.unwrap();
    assert_eq!((v.trust, v.trashed_at), (view.trust, None));
    assert_eq!(store.list_outgoing_relationships(&a).await.unwrap().len(), 1, "links");
    assert!(docs.list(Some(revived.as_str())).await.unwrap().iter().any(|d| d.id == a), "listed at once, there");
    let list = doc_trash::list_trash(&docs, store.as_ref(), chrono::Utc::now()).await.unwrap();
    assert!(list.iter().all(|e| e.id != a));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_9_restore_into_a_thread_that_does_not_exist_is_refused() {
    let (store, docs) = fixture("s5-9-restore-bad").await;
    let lane = thread(&store, "Lane").await;
    let other = thread(&store, "Other").await;
    let a = create(&docs, "A", &lane, "alpha").await;
    docs.trash(&a).await.unwrap();
    store.soft_delete_thread(&lane).await.unwrap();
    store.soft_delete_thread(&other).await.unwrap();
    for bad in [other.as_str(), "thread:nope", lane.as_str()] {
        assert!(doc_trash::restore_from_trash_to(&docs, store.as_ref(), &a, Some(bad)).await.is_err(), "{bad}");
        assert_eq!(docs.list_trash().await.unwrap().len(), 1, "nothing restored ({bad})");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_9_restore_without_a_choice_when_the_thread_exists_goes_back_to_it() {
    let (store, docs) = fixture("s5-9-restore-own").await;
    let lane = thread(&store, "Lane").await;
    let a = create(&docs, "A", &lane, "alpha").await;
    docs.trash(&a).await.unwrap();
    doc_trash::restore_from_trash_to(&docs, store.as_ref(), &a, None).await.expect("restore");
    assert_eq!(store.get_document(&a).await.unwrap().thread_id, lane);
    assert!(docs.list(Some(lane.as_str())).await.unwrap().iter().any(|d| d.id == a));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_9_restoring_what_is_not_in_the_trash_is_refused() {
    let (store, docs) = fixture("s5-9-restore-live").await;
    let lane = thread(&store, "Lane").await;
    let a = create(&docs, "A", &lane, "alpha").await;
    assert!(doc_trash::restore_from_trash(&docs, &a).await.is_err(), "a live document");
    assert!(doc_trash::restore_from_trash(&docs, "document:nope").await.is_err(), "a missing one");
}

// ------------------------------------------------------ S5-9 (S5-7 empty) --

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_9_empty_trash_purges_the_trash_and_reports_how_many() {
    let (store, docs) = fixture("s5-9-empty").await;
    let lane = thread(&store, "Lane").await;
    let a = create(&docs, "A", &lane, "alpha").await;
    let b = create(&docs, "B", &lane, "beta").await;
    let live = create(&docs, "Live", &lane, "live").await;
    docs.trash(&a).await.unwrap();
    docs.trash(&b).await.unwrap();

    let n = doc_trash::empty_trash(&docs).await.expect("empty_trash");
    assert_eq!(n, 2, "how many were purged");
    assert!(docs.list_trash().await.unwrap().is_empty());
    for id in [&a, &b] {
        assert!(docs.is_purged(id).await.unwrap(), "{id} purged");
    }
    assert!(docs.read(&live).await.is_ok(), "nothing live is touched");
    assert_eq!(doc_trash::empty_trash(&docs).await.expect("empty again"), 0);
}

// ---------------------------------------------------- S5-9 (registration) --

/// The three commands are registered in the Tauri host (the mobile app's
/// commands are the `generate_handler!` list of `run_tauri`).
#[test]
fn s5_9_the_trash_commands_are_registered() {
    let lib = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs")).unwrap();
    let start = lib.find("tauri::generate_handler![").expect("generate_handler! in lib.rs");
    let end = start + lib[start..].find("])").expect("end of generate_handler!");
    let block = &lib[start..end];
    let registered: Vec<&str> = block
        .lines()
        .map(|l| l.trim().trim_end_matches(','))
        .filter(|l| !l.starts_with("//"))
        .filter_map(|l| l.rsplit("::").next())
        .collect();
    for cmd in ["list_trash", "restore_from_trash", "empty_trash"] {
        assert!(registered.contains(&cmd), "the Tauri command {cmd} is not registered");
    }
}

/// The command restores into a thread when one is chosen (Céline,
/// 2026-10-17): `restore_from_trash` takes an optional thread id.
#[test]
fn s5_9_the_restore_command_takes_an_optional_thread() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/doc_trash.rs")).unwrap();
    let start = src.find("pub async fn restore_from_trash(webview").expect("the restore_from_trash command");
    let signature: String = src[start..].split('{').next().unwrap().chars().filter(|c| !c.is_whitespace()).collect();
    assert!(signature.contains("thread_id:Option<String>"), "the command takes no thread choice: {signature}");
}

// ---------------------------------------------------------------- S5-10 --

/// The phone's thread delete: every document still in the thread goes to the
/// trash, the thread is deleted, its documents show it as gone.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_10_the_phones_thread_delete_sends_its_documents_to_the_trash() {
    let (store, docs) = fixture("s5-10-phone").await;
    let lane = thread(&store, "Old drafts").await;
    let keep = thread(&store, "Keep").await;
    let a = create(&docs, "A", &lane, "alpha").await;
    let b = create(&docs, "B", &lane, "beta").await;
    let k = create(&docs, "K", &keep, "kept").await;

    let moved = doc_trash::delete_thread_to_trash(&docs, store.as_ref(), &lane).await.expect("thread deleted");
    assert_eq!(moved, 2, "how many went to the trash");
    let list = doc_trash::list_trash(&docs, store.as_ref(), chrono::Utc::now()).await.unwrap();
    let mut ids: Vec<&str> = list.iter().map(|e| e.id.as_str()).collect();
    ids.sort();
    let mut expected = vec![a.as_str(), b.as_str()];
    expected.sort();
    assert_eq!(ids, expected);
    assert!(list.iter().all(|e| e.thread_id == lane && e.thread_name.is_none()), "each shows its thread as deleted");
    for e in &list {
        let json = serde_json::to_value(e).unwrap();
        assert_eq!(json["deleted_thread_name"], "Old drafts", "with the thread's old name: {json}");
    }
    let active: Vec<String> = store
        .list_threads()
        .await
        .unwrap()
        .into_iter()
        .filter(|t| t.deleted_at.is_none())
        .filter_map(|t| t.id_string())
        .collect();
    assert_eq!(active, vec![keep], "the thread is no longer listed");
    assert!(docs.read(&k).await.unwrap().trashed_at.is_none());
}

/// The Tauri `delete_thread` command goes through that path (and no longer
/// soft-deletes the thread record itself: `sovereign-docs`
/// `s5_10_no_host_deletes_or_restores_a_thread_around_the_one_path`).
#[test]
fn s5_10_the_delete_thread_command_uses_the_one_path() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/tauri_commands/threads.rs")).unwrap();
    let start = src.find("pub async fn delete_thread(").expect("the delete_thread command");
    let body_end = start + src[start..].find("\n}").expect("end of delete_thread");
    let body = &src[start..body_end];
    assert!(body.contains("delete_thread_to_trash("), "delete_thread does not send the documents to the trash:\n{body}");
}
