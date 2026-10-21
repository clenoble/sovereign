//! S5-10 — the assistant's `delete_thread` (C-DOC adoption step 5,
//! `doc/plans/C-DOC-adoption.md`, *What the answers change*; spec ch. 03,
//! *Thread deletion and the fallback lane*, decided by Céline 2026-10-17).
//!
//! "Deleting a thread moves every document still in it to the trash — not to
//! the fallback lane — and deletes the thread." The assistant's path (the
//! desktop's only thread delete today) goes through the one host-agnostic
//! function, `sovereign_docs::delete_thread_to_trash`, which is tested in
//! depth in `sovereign-docs` (`tests/s5_10_thread_delete.rs`). Written before
//! the code by another author; whoever implements it makes these pass without
//! editing this file.
//!
//! Not tested here: the confirmation's text inside the assistant's proposal
//! (ch. 08 AT5) and re-proposing when the thread's contents changed (AT4) —
//! contract C-HOST, not built.

use super::*;
use crate::docs_host::testing::{account_key, encrypted_memory_store};
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::schema::Thread;

struct World {
    store: Arc<EncryptedGraphDB>,
    docs: Arc<sovereign_docs::DocumentService>,
    orch: Orchestrator,
    events: std::sync::mpsc::Receiver<OrchestratorEvent>,
    lane: String,
    keep: String,
    a: String,
    b: String,
    k: String,
}

async fn world(tag: &str, with_service: bool) -> World {
    let store = encrypted_memory_store(tag).await;
    let docs = Arc::new(crate::docs_host::build_document_service(store.clone(), Some(account_key()), "t".into()));
    let (tx, events) = std::sync::mpsc::channel();
    let dir = std::env::temp_dir().join(format!("sovereign_test_step5_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let orch = Orchestrator::for_test(store.clone() as Arc<dyn GraphDB>, tx, dir);
    if with_service {
        orch.set_document_service(docs.clone());
    }
    let lane = store.create_thread(Thread::new("Old drafts".into(), String::new())).await.unwrap().id_string().unwrap();
    let keep = store.create_thread(Thread::new("Keep".into(), String::new())).await.unwrap().id_string().unwrap();
    let create = |title: &str, thread: &str, text: &str| {
        let docs = docs.clone();
        let new = crate::docs_host::new_user_document(title, thread, text);
        async move { docs.create(new).await.unwrap().id }
    };
    let a = create("A", &lane, "alpha").await;
    let b = create("B", &lane, "beta").await;
    let k = create("K", &keep, "kept").await;
    World { store, docs, orch, events, lane, keep, a, b, k }
}

fn executed(events: &[OrchestratorEvent]) -> Vec<bool> {
    events
        .iter()
        .filter_map(|e| match e {
            OrchestratorEvent::ActionExecuted { success, .. } => Some(*success),
            _ => None,
        })
        .collect()
}

async fn active_threads(store: &EncryptedGraphDB) -> Vec<String> {
    store.list_threads().await.unwrap().into_iter().filter(|t| t.deleted_at.is_none()).filter_map(|t| t.id_string()).collect()
}

async fn trashed(docs: &sovereign_docs::DocumentService) -> Vec<String> {
    let mut v: Vec<String> = docs.list_trash().await.unwrap().into_iter().map(|e| e.id).collect();
    v.sort();
    v
}

#[tokio::test]
async fn s5_10_the_assistants_delete_thread_sends_its_documents_to_the_trash() {
    let w = world("orch-del-thread", true).await;
    let earlier = w.docs.create(crate::docs_host::new_user_document("Earlier", &w.lane, "earlier")).await.unwrap().id;
    w.docs.trash(&earlier).await.unwrap();
    let earlier_at = w.docs.read(&earlier).await.unwrap().trashed_at;

    w.orch.execute_action("delete_thread", Some("Old drafts"), "delete the old drafts thread").await.unwrap();
    let evs: Vec<OrchestratorEvent> = w.events.try_iter().collect();
    assert_eq!(executed(&evs), vec![true]);
    assert!(
        evs.iter().any(|e| matches!(e, OrchestratorEvent::ThreadDeleted { thread_id } if *thread_id == w.lane)),
        "ThreadDeleted for the thread"
    );

    let mut expected = vec![w.a.clone(), w.b.clone(), earlier.clone()];
    expected.sort();
    assert_eq!(trashed(&w.docs).await, expected, "every document of the thread is in the trash");
    assert_eq!(w.docs.read(&earlier).await.unwrap().trashed_at, earlier_at, "one already in the trash stays as it was");
    assert_eq!(active_threads(&w.store).await, vec![w.keep.clone()], "the thread is no longer listed");
    assert!(w.docs.read(&w.k).await.unwrap().trashed_at.is_none(), "another thread's document stays");
}

/// Locked (no document service): nothing is deleted, nothing is trashed, and
/// the action does not report success.
#[tokio::test]
async fn s5_10_without_the_document_service_no_thread_is_deleted() {
    let w = world("orch-del-thread-locked", false).await;
    w.orch.execute_action("delete_thread", Some("Old drafts"), "q").await.unwrap();
    let evs: Vec<OrchestratorEvent> = w.events.try_iter().collect();
    assert_eq!(executed(&evs), vec![false], "no success without the service");
    assert!(!evs.iter().any(|e| matches!(e, OrchestratorEvent::ThreadDeleted { .. })));
    let mut both = vec![w.lane.clone(), w.keep.clone()];
    both.sort();
    let mut active = active_threads(&w.store).await;
    active.sort();
    assert_eq!(active, both, "the thread is still there");
    assert!(w.docs.list_trash().await.unwrap().is_empty(), "nothing trashed");
    let _ = (&w.a, &w.b);
}
