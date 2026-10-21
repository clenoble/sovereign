//! S5-12 in the Tauri host (the mobile app): the layer under the commands
//! `list_orphaned_documents`, `resolve_orphan`, `resolve_orphans_of_thread`
//! (`doc_orphans.rs`), their registration, and the JSON shapes the frontend
//! sends and reads (`frontend/src/lib/api/commands.ts`). Written before the
//! code, by another author: whoever implements S5-12 makes these pass without
//! editing this file. The Svelte side is tested by vitest
//! (`SyncAlerts.orphans.svelte.test.ts`); the service by
//! `sovereign-docs/tests/s5_12_orphans.rs`.
//!
//! Stores: a real encrypted store over an in-memory SurrealDB
//! (`docs_host::testing`); nothing touches a real workspace. The other
//! device's writes arrive through the service (`apply_wire`).

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use sovereign_ai::docs_host::{build_document_service, new_user_document, testing};
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::schema::Thread;
use sovereign_db::GraphDB;
use sovereign_docs::{delete_thread_to_trash, DocumentService, RemoteDocument, Stamp, Trust, WireMeta};

use crate::doc_orphans::{self, OrphanResolutionDto};

const MARKER: &str = "zebra-orphan-marker-app";
const LAPTOP: &str = "peerlaptop12D3";

async fn fixture(tag: &str) -> (Arc<EncryptedGraphDB>, DocumentService) {
    let store = testing::encrypted_memory_store(tag).await;
    let docs = build_document_service(store.clone(), Some(testing::account_key()), "phone".into());
    (store, docs)
}

async fn thread(store: &EncryptedGraphDB, name: &str) -> String {
    store.create_thread(Thread::new(name.into(), String::new())).await.unwrap().id_string().unwrap()
}

/// A document written on the laptop for `thread_id`, arriving by sync.
async fn arrive(docs: &DocumentService, id: &str, thread_id: &str, title: &str) {
    let content = serde_json::json!({"format": "sovereign.doc", "v": 2,
        "blocks": [{"type": "text", "id": "00000000000000000000000000001001", "markup": "markdown", "text": MARKER}]})
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

/// "Old drafts" deleted on the phone (its document `old` in the trash); two
/// documents of it written on the laptop arrive (`a`, `b`). "Keep" with `kept`.
struct World {
    store: Arc<EncryptedGraphDB>,
    docs: DocumentService,
    gone: String,
    keep: String,
    old: String,
    a: String,
    b: String,
    kept: String,
}

async fn world(tag: &str) -> World {
    let (store, docs) = fixture(tag).await;
    let gone = thread(&store, "Old drafts").await;
    let keep = thread(&store, "Keep").await;
    let old = docs.create(new_user_document("Old", &gone, "old")).await.unwrap().id;
    let kept = docs.create(new_user_document("Kept", &keep, "kept")).await.unwrap().id;
    delete_thread_to_trash(&docs, store.as_ref(), &gone).await.expect("deleted");
    let (a, b) = ("document:laptopa1".to_string(), "document:laptopb1".to_string());
    arrive(&docs, &a, &gone, "Alpha").await;
    arrive(&docs, &b, &gone, "Beta").await;
    World { store, docs, gone, keep, old, a, b, kept }
}

fn names() -> HashMap<String, String> {
    HashMap::from([(LAPTOP.to_string(), "Laptop".to_string())])
}

async fn listed(w: &World) -> Vec<String> {
    let mut v: Vec<String> =
        doc_orphans::list_orphaned_documents(&w.docs, &names()).await.unwrap().into_iter().map(|o| o.doc_id).collect();
    v.sort();
    v
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

// ------------------------------------------------- list_orphaned_documents --

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_12_list_orphaned_documents_names_the_document_its_old_thread_and_the_device() {
    let w = world("s5-12-app-list").await;
    let list = doc_orphans::list_orphaned_documents(&w.docs, &names()).await.expect("listed");
    assert_eq!(sorted(list.iter().map(|o| o.doc_id.clone()).collect()), sorted(vec![w.a.clone(), w.b.clone()]));
    let a = list.iter().find(|o| o.doc_id == w.a).unwrap();
    assert_eq!(a.title, "Alpha");
    assert_eq!(a.thread_id, w.gone);
    assert_eq!(a.deleted_thread_name.as_deref(), Some("Old drafts"));
    assert_eq!(a.device, LAPTOP);
    assert_eq!(a.device_name, "Laptop");
    assert!(!list.iter().any(|o| o.doc_id == w.old || o.doc_id == w.kept), "not the trash, not a live thread");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_12_the_list_travels_with_the_frontends_field_names_and_no_text() {
    let w = world("s5-12-app-json").await;
    let list = doc_orphans::list_orphaned_documents(&w.docs, &names()).await.unwrap();
    let json = serde_json::to_value(&list).unwrap();
    let first = json.as_array().unwrap()[0].as_object().unwrap();
    let mut keys: Vec<&str> = first.keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, vec!["deleted_thread_name", "device", "device_name", "doc_id", "thread_id", "title"]);
    assert!(!json.to_string().contains(MARKER), "no document text");
}

// ----------------------------------------------------- the resolution shape --

#[test]
fn s5_12_the_resolution_reads_the_three_shapes_the_frontend_sends() {
    let r = |s: &str| serde_json::from_str::<OrphanResolutionDto>(s);
    assert_eq!(r(r#"{"kind":"move_to","thread_id":"thread:k"}"#).unwrap(), OrphanResolutionDto::MoveTo { thread_id: "thread:k".into() });
    assert_eq!(r(r#"{"kind":"new_thread","name":"Brought back"}"#).unwrap(), OrphanResolutionDto::NewThread { name: "Brought back".into() });
    assert_eq!(r(r#"{"kind":"trash"}"#).unwrap(), OrphanResolutionDto::Trash);
    assert!(r(r#"{"kind":"delete_forever"}"#).is_err(), "no other way out");
}

#[test]
fn s5_12_the_resolution_maps_to_the_service() {
    use sovereign_docs::OrphanResolution;
    assert_eq!(OrphanResolution::from(OrphanResolutionDto::MoveTo { thread_id: "thread:k".into() }), OrphanResolution::MoveTo("thread:k".into()));
    assert_eq!(OrphanResolution::from(OrphanResolutionDto::NewThread { name: "N".into() }), OrphanResolution::NewThread("N".into()));
    assert_eq!(OrphanResolution::from(OrphanResolutionDto::Trash), OrphanResolution::Trash);
}

// ------------------------------------------------------------ resolve_orphan --

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_12_resolve_orphan_moves_creates_or_trashes_and_ends_the_question() {
    let w = world("s5-12-app-resolve").await;
    doc_orphans::resolve_orphan(&w.docs, &w.a, OrphanResolutionDto::MoveTo { thread_id: w.keep.clone() }).await.expect("moved");
    assert_eq!(w.docs.read(&w.a).await.unwrap().thread_id, w.keep);
    assert_eq!(listed(&w).await, vec![w.b.clone()]);

    doc_orphans::resolve_orphan(&w.docs, &w.b, OrphanResolutionDto::NewThread { name: "Brought back".into() }).await.expect("new thread");
    let t = w.docs.read(&w.b).await.unwrap().thread_id;
    assert_eq!(w.store.get_thread(&t).await.unwrap().name, "Brought back");
    assert!(listed(&w).await.is_empty());

    // Trash, on a third arrival.
    let c = "document:laptopc1".to_string();
    arrive(&w.docs, &c, &w.gone, "Gamma").await;
    doc_orphans::resolve_orphan(&w.docs, &c, OrphanResolutionDto::Trash).await.expect("trashed");
    assert!(w.docs.read(&c).await.unwrap().trashed_at.is_some());
    assert!(listed(&w).await.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_12_resolve_orphan_refused_says_why_and_changes_nothing() {
    let w = world("s5-12-app-refused").await;
    for (what, r) in [
        ("the deleted thread", OrphanResolutionDto::MoveTo { thread_id: w.gone.clone() }),
        ("an unknown thread", OrphanResolutionDto::MoveTo { thread_id: "thread:nope".into() }),
        ("an empty name", OrphanResolutionDto::NewThread { name: "  ".into() }),
    ] {
        let msg = doc_orphans::resolve_orphan(&w.docs, &w.a, r).await.expect_err(what);
        assert!(!msg.trim().is_empty(), "{what}: the notice says why");
        assert_eq!(w.docs.read(&w.a).await.unwrap().thread_id, w.gone, "{what}: not moved");
        assert!(w.docs.read(&w.a).await.unwrap().trashed_at.is_none(), "{what}: not trashed");
    }
    let msg = doc_orphans::resolve_orphan(&w.docs, &w.kept, OrphanResolutionDto::Trash).await.expect_err("not waiting");
    assert!(!msg.trim().is_empty());
    assert!(w.docs.read(&w.kept).await.unwrap().trashed_at.is_none(), "a document not waiting is untouched");
    assert_eq!(listed(&w).await, sorted(vec![w.a.clone(), w.b.clone()]));
}

// ------------------------------------------------- resolve_orphans_of_thread --

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_12_resolve_orphans_of_thread_resolves_the_group_and_counts_it() {
    let w = world("s5-12-app-group").await;
    let n = doc_orphans::resolve_orphans_of_thread(&w.docs, &w.gone, OrphanResolutionDto::NewThread { name: "Drafts".into() })
        .await
        .expect("group");
    assert_eq!(n, 2);
    let (ta, tb) = (w.docs.read(&w.a).await.unwrap().thread_id, w.docs.read(&w.b).await.unwrap().thread_id);
    assert_eq!(ta, tb, "one thread for the group");
    assert_eq!(w.store.get_thread(&ta).await.unwrap().name, "Drafts");
    assert!(listed(&w).await.is_empty());

    let err = doc_orphans::resolve_orphans_of_thread(&w.docs, &w.keep, OrphanResolutionDto::Trash).await.expect_err("a live thread");
    assert!(!err.trim().is_empty());
    assert!(w.docs.read(&w.kept).await.unwrap().trashed_at.is_none());
}

// ------------------------------------------------------------ registration --

/// The three commands are registered in the Tauri host (`generate_handler!`
/// of `run_tauri`), each under the name the frontend invokes.
#[test]
fn s5_12_the_three_commands_are_registered() {
    let lib = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs")).unwrap();
    let start = lib.find("tauri::generate_handler![").expect("generate_handler! in lib.rs");
    let end = start + lib[start..].find("])").expect("end of generate_handler!");
    let registered: Vec<&str> = lib[start..end]
        .lines()
        .map(|l| l.trim().trim_end_matches(','))
        .filter(|l| !l.starts_with("//"))
        .filter_map(|l| l.rsplit("::").next())
        .collect();
    for cmd in ["list_orphaned_documents", "resolve_orphan", "resolve_orphans_of_thread"] {
        assert!(registered.contains(&cmd), "the Tauri command {cmd} is not registered");
    }
}
