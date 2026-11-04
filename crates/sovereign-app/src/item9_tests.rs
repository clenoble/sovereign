//! v0.0.12 item 9 in the Tauri host (the phone) — trash and orphan actions act
//! on what was shown; Delete says what it did
//! (`doc/plans/V0012-item9-act-on-what-was-shown.md`, T1–T3, the layer under
//! the `#[tauri::command]` wrappers). The Svelte side is tested by vitest
//! (`TrashPanel`, `trash`, `deleteUndo`, `SyncAlerts.orphans`); the service by
//! `sovereign-docs/tests/item9_act_on_what_was_shown.rs`.
//!
//! Written before the code, by another author: whoever builds item 9 makes
//! these pass **without editing this file**. A test that looks wrong is
//! reported, not worked around.
//!
//! **Seams this file names** (they do not exist at `c7e571b`, so this file
//! does not compile until they are built):
//!
//! - `doc_trash::empty_trash_of(docs: &DocumentService, ids: &[String]) ->
//!   Result<usize, String>` — T1: purges those of the shown `ids` still in the
//!   trash; how many were purged. The command `empty_trash` takes the shown
//!   ids (`ids: Vec<String>`, the frontend sends `{ ids }`) and still answers
//!   the number purged (the frontend tells the kept ones from it).
//! - `doc_orphans::resolve_orphans_shown(docs, thread_id: &str, doc_ids:
//!   &[String], resolution: OrphanResolutionDto) -> Result<usize, String>` —
//!   T2: the group's action on the documents shown; how many were acted on.
//!   The command `resolve_orphans_of_thread` takes them (`doc_ids:
//!   Vec<String>`, the frontend sends `{ threadId, docIds, resolution }`).
//! - `doc_trash::delete_document(docs: &DocumentService, id: &str) ->
//!   Result<DeleteOutcome, String>` and `doc_trash::DeleteOutcome { Trashed,
//!   AlreadyInTrash }` (`Serialize`, snake_case: `"trashed"`,
//!   `"already_in_trash"`; `Debug`, `PartialEq`) — T3. The command
//!   `delete_document` answers it.
//!
//! Stores: a real encrypted store over an in-memory SurrealDB
//! (`docs_host::testing`). A peer's change: a second document service (the
//! laptop's), its state exported (`stored_state`) and applied here
//! (`apply_wire`).

use std::sync::Arc;

use chrono::Utc;
use sovereign_ai::docs_host::{build_document_service, new_user_document, testing};
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::schema::Thread;
use sovereign_db::GraphDB;
use sovereign_docs::{delete_thread_to_trash, DocumentService, RemoteDocument, Stamp, Trust, WireMeta, WireState};

use crate::doc_orphans::{self, OrphanResolutionDto};
use crate::doc_trash::{self, DeleteOutcome};

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

async fn tick() {
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
}

async fn send(from: &DocumentService, to: &DocumentService, id: &str) {
    let WireState { document, meta, .. } = from.stored_state(id).await.expect("export");
    to.apply_wire(document, meta).await.expect("apply");
}

/// The laptop deletes `id` (it holds a copy) and the deletion reaches the phone.
async fn peer_deletes(here: &DocumentService, laptop: &DocumentService, id: &str) {
    tick().await;
    laptop.trash(id).await.expect("the laptop's delete");
    send(laptop, here, id).await;
    assert!(here.read(id).await.unwrap().trashed_at.is_some(), "setup: the peer's deletion was applied here");
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

/// The source of the `#[tauri::command]` `pub async fn <name>(` in `file`
/// whose first parameter is `webview` (a layer function of the same name, as
/// `doc_trash::empty_trash`, is skipped), from its signature to the closing
/// brace at its own indentation, whitespace removed.
fn command_source(file: &str, name: &str) -> String {
    let src = std::fs::read_to_string(format!("{}/src/{file}", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let head = format!("pub async fn {name}(");
    let start = src
        .match_indices(&head)
        .map(|(i, _)| i)
        .find(|&i| src[i + head.len()..].trim_start().starts_with("webview"))
        .unwrap_or_else(|| panic!("the {name} command (first parameter `webview`) in {file}"));
    let line_start = src[..start].rfind('\n').map_or(0, |n| n + 1);
    let indent = &src[line_start..start];
    let rest = &src[start..];
    let end = rest.find(&format!("\n{indent}}}\n")).unwrap_or(rest.len());
    rest[..end].chars().filter(|c| !c.is_whitespace()).collect()
}

// ======================================================================= T1

// T1 — "The confirm acts on the ids the trash view listed when it was drawn …
// A document that entered the trash after the view was drawn is not purged …
// A shown document that is no longer in the trash … is left alone. The report
// counts what was purged. The same holds … on the phone."
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn t1_phone_empty_trash_purges_only_what_was_shown_and_still_in_the_trash() {
    let (store, docs) = fixture("i9-app-t1", "phone").await;
    let (_l, laptop) = fixture("i9-app-t1-laptop", "laptop").await;
    let lane = thread(&store, "Lane").await;
    let a = create(&docs, "A", &lane).await;
    let b = create(&docs, "B", &lane).await;
    let c = create(&docs, "C", &lane).await;
    send(&docs, &laptop, &c).await;
    docs.trash(&a).await.unwrap();
    docs.trash(&b).await.unwrap();
    let shown: Vec<String> =
        doc_trash::list_trash(&docs, store.as_ref(), Utc::now()).await.unwrap().into_iter().map(|e| e.id).collect();
    assert_eq!(sorted(shown.clone()), sorted(vec![a.clone(), b.clone()]), "setup: the trash shows A and B");

    // Meanwhile: the laptop deletes C (applied here), and B is restored.
    peer_deletes(&docs, &laptop, &c).await;
    docs.restore_from_trash(&b).await.unwrap();

    let n = doc_trash::empty_trash_of(&docs, &shown).await.expect("emptied");
    assert_eq!(n, 1, "counts what was purged: A only");
    assert!(docs.is_purged(&a).await.unwrap(), "A purged");
    assert!(docs.read(&b).await.unwrap().trashed_at.is_none(), "B, restored meanwhile, left alone");
    assert!(!docs.is_purged(&c).await.unwrap(), "C, never shown, not purged");
    let next: Vec<String> =
        doc_trash::list_trash(&docs, store.as_ref(), Utc::now()).await.unwrap().into_iter().map(|e| e.id).collect();
    assert_eq!(next, vec![c.clone()], "the next view lists C");
}

/// The command takes the ids shown, and no UI path empties by re-listing
/// ("[choice] … no UI calls it", the old `empty_trash()`).
#[test]
fn t1_phone_the_empty_trash_command_takes_the_shown_ids() {
    let body = command_source("doc_trash.rs", "empty_trash");
    assert!(body.contains("ids:Vec<String>"), "the command takes the shown ids: {body}");
    // The calls, after the signature (which itself reads `empty_trash(`).
    let calls = body.split_once('{').map_or("", |(_, b)| b);
    assert!(calls.contains("empty_trash_of("), "it empties what was shown: {body}");
    assert!(!calls.contains("empty_trash("), "it never empties by re-listing the trash: {body}");
}

// ======================================================================= T2

const LAPTOP: &str = "peerlaptopi9";

/// A document written on the laptop for `thread_id`, arriving by sync.
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
        modified_at: Utc::now(),
        deleted_at: None,
        device: LAPTOP.into(),
        trust_decision: None,
    };
    let meta = WireMeta {
        stamp: Some(Stamp { wall_ms: Utc::now().timestamp_millis(), counter: 0, device: LAPTOP.into() }),
        ..WireMeta::default()
    };
    docs.apply_wire(remote, meta).await.expect("applied");
}

async fn waiting(docs: &DocumentService) -> Vec<String> {
    sorted(docs.orphans_of_deleted_threads().await.unwrap().into_iter().map(|o| o.id).collect())
}

// T2 — "'Move all to a thread…', 'All in a new thread…' and 'Trash all' apply
// to the ids the group listed when it was drawn … A document that joined the
// group after is left waiting … The result says how many were acted on …
// Desktop and phone."
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn t2_phone_a_group_action_leaves_a_document_that_joined_later_waiting() {
    for (tag, resolution) in [
        ("i9-app-t2-trash", OrphanResolutionDto::Trash),
        ("i9-app-t2-new", OrphanResolutionDto::NewThread { name: "Revived".into() }),
        ("i9-app-t2-move", OrphanResolutionDto::MoveTo { thread_id: String::new() }),
    ] {
        let (store, docs) = fixture(tag, "phone").await;
        let gone = thread(&store, "Old drafts").await;
        let keep = thread(&store, "Keep").await;
        delete_thread_to_trash(&docs, store.as_ref(), &gone).await.expect("thread deleted");
        arrive(&docs, "document:i9ax", &gone, "X").await;
        arrive(&docs, "document:i9ay", &gone, "Y").await;
        let names = std::collections::HashMap::new();
        let shown: Vec<String> =
            doc_orphans::list_orphaned_documents(&docs, &names).await.unwrap().into_iter().map(|o| o.doc_id).collect();
        assert_eq!(sorted(shown.clone()), vec!["document:i9ax".to_string(), "document:i9ay".to_string()]);
        arrive(&docs, "document:i9az", &gone, "Z").await; // after the group was drawn

        let resolution = match resolution {
            OrphanResolutionDto::MoveTo { .. } => OrphanResolutionDto::MoveTo { thread_id: keep.clone() },
            r => r,
        };
        let n = doc_orphans::resolve_orphans_shown(&docs, &gone, &shown, resolution).await.expect("resolved");
        assert_eq!(n, 2, "{tag}: how many were acted on: what was shown");
        assert_eq!(waiting(&docs).await, vec!["document:i9az".to_string()], "{tag}: Z left waiting");
        let z = docs.read("document:i9az").await.unwrap();
        assert!(z.trashed_at.is_none() && z.thread_id == gone, "{tag}: Z untouched");
    }
}

#[test]
fn t2_phone_the_group_command_takes_the_shown_documents() {
    let body = command_source("doc_orphans.rs", "resolve_orphans_of_thread");
    assert!(body.contains("doc_ids:Vec<String>"), "the command takes the documents shown: {body}");
    assert!(body.contains("resolve_orphans_shown("), "it acts on them: {body}");
}

// ======================================================================= T3

// T3 — "`trash` tells 'moved to the trash' apart from 'was already in the
// trash'. In the second case … the phone say[s] 'Already in the trash', with
// no Undo." The host answers which it was, under the names the frontend reads.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn t3_phone_delete_says_whether_it_moved_the_document_or_found_it_in_the_trash() {
    let (store, docs) = fixture("i9-app-t3", "phone").await;
    let (_l, laptop) = fixture("i9-app-t3-laptop", "laptop").await;
    let lane = thread(&store, "Lane").await;
    let mine = create(&docs, "Mine", &lane).await;
    let theirs = create(&docs, "Theirs", &lane).await;
    send(&docs, &laptop, &theirs).await;

    assert_eq!(doc_trash::delete_document(&docs, &mine).await.unwrap(), DeleteOutcome::Trashed);
    assert!(docs.read(&mine).await.unwrap().trashed_at.is_some());
    assert_eq!(doc_trash::delete_document(&docs, &mine).await.unwrap(), DeleteOutcome::AlreadyInTrash, "a second Delete");

    peer_deletes(&docs, &laptop, &theirs).await;
    let when = docs.read(&theirs).await.unwrap().trashed_at;
    assert_eq!(
        doc_trash::delete_document(&docs, &theirs).await.unwrap(),
        DeleteOutcome::AlreadyInTrash,
        "the laptop's deletion is not this Delete's"
    );
    assert_eq!(docs.read(&theirs).await.unwrap().trashed_at, when, "left as it came");

    assert_eq!(serde_json::to_value(DeleteOutcome::Trashed).unwrap(), serde_json::json!("trashed"));
    assert_eq!(serde_json::to_value(DeleteOutcome::AlreadyInTrash).unwrap(), serde_json::json!("already_in_trash"));
}

#[test]
fn t3_phone_the_delete_command_answers_what_it_did() {
    let body = command_source("tauri_commands/documents.rs", "delete_document");
    assert!(body.contains("Result<DeleteOutcome,String>") || body.contains("Result<crate::doc_trash::DeleteOutcome,String>"),
        "the command answers the outcome: {body}");
}
