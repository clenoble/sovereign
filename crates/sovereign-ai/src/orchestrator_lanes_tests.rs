//! v0.0.12 lanes lot, item 15 — **M2** for the assistant
//! (`doc/plans/V0012-lot-lanes.md`): "the assistant's move (orchestrator and
//! tool) … move through M1" — a stamped write of the thread alone, so the
//! move travels (M3). The scan that no production code calls the store's
//! `move_document_to_thread` is in `sovereign-docs/tests/lanes_m2_one_move_path.rs`;
//! here, what the two paths do.
//!
//! Written before the code by another author; the implementer does not edit
//! this file. Uses only what exists (the document service's stored state),
//! so it compiles today.
//!
//! CONTRACT-QUESTION (M2): the contract does not say what the assistant's
//! move does with no document service (a locked workspace). Read as THREAD-sync
//! S1's rule for the assistant's thread writes (`tools.rs`
//! `execute_write_tool_thread_writes_without_a_service_write_nothing`): a path
//! without a service does not write, and does not report success.

use super::*;
use crate::docs_host::testing::{account_key, encrypted_memory_store};
use sovereign_db::encrypted::EncryptedGraphDB;

struct World {
    store: Arc<EncryptedGraphDB>,
    docs: Arc<sovereign_docs::DocumentService>,
    orch: Orchestrator,
    events: std::sync::mpsc::Receiver<OrchestratorEvent>,
    from: String,
    to: String,
    id: String,
}

async fn world(tag: &str, with_service: bool) -> World {
    let store = encrypted_memory_store(tag).await;
    let docs = Arc::new(crate::docs_host::build_document_service(store.clone(), Some(account_key()), "t".into()));
    let (tx, events) = std::sync::mpsc::channel();
    let dir = std::env::temp_dir().join(format!("sovereign_test_lanes_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let orch = Orchestrator::for_test(store.clone() as Arc<dyn GraphDB>, tx, dir);
    if with_service {
        orch.set_document_service(docs.clone());
    }
    let from = docs.thread_create("Garden", "").await.unwrap();
    let to = docs.thread_create("Kitchen", "").await.unwrap();
    let id = docs.create(crate::docs_host::new_user_document("Seeds", &from, "tomatoes")).await.unwrap().id;
    World { store, docs, orch, events, from, to, id }
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

/// The stamp and the previous stamp of `id`'s current state.
async fn stamps(docs: &sovereign_docs::DocumentService, id: &str) -> (Option<sovereign_docs::Stamp>, Option<sovereign_docs::Stamp>) {
    let st = docs.stored_state(id).await.unwrap();
    (st.meta.stamp, st.meta.prev_stamp)
}

/// M2: the orchestrator's `move_document` moves through the document service:
/// the thread changes under a new stamp that replaces the state it moved.
#[tokio::test]
async fn m2_the_orchestrators_move_document_is_a_stamped_move() {
    let w = world("orch-move", true).await;
    let (before, _) = stamps(&w.docs, &w.id).await;
    w.orch.execute_action("move_document", Some("Seeds to Kitchen"), "move seeds to kitchen").await.unwrap();
    let evs: Vec<OrchestratorEvent> = w.events.try_iter().collect();
    assert_eq!(executed(&evs), vec![true], "the move is reported done");
    assert!(
        evs.iter().any(|e| matches!(e, OrchestratorEvent::DocumentMoved { doc_id, new_thread_id } if *doc_id == w.id && *new_thread_id == w.to)),
        "DocumentMoved for the document and its new thread"
    );
    assert_eq!(w.docs.read(&w.id).await.unwrap().thread_id, w.to, "in Kitchen");
    let (after, prev) = stamps(&w.docs, &w.id).await;
    assert!(after > before, "a new stamp: the move travels ({before:?} -> {after:?})");
    assert_eq!(prev, before, "replacing the state it moved");
    let _ = (&w.store, &w.from);
}

/// M2 (CONTRACT-QUESTION above): locked, nothing moves.
#[tokio::test]
async fn m2_without_the_document_service_the_orchestrator_moves_nothing() {
    let w = world("orch-move-locked", false).await;
    let (before, _) = stamps(&w.docs, &w.id).await;
    w.orch.execute_action("move_document", Some("Seeds to Kitchen"), "q").await.unwrap();
    let evs: Vec<OrchestratorEvent> = w.events.try_iter().collect();
    assert_eq!(executed(&evs), vec![false], "no success without the service");
    assert!(!evs.iter().any(|e| matches!(e, OrchestratorEvent::DocumentMoved { .. })));
    assert_eq!(w.docs.read(&w.id).await.unwrap().thread_id, w.from, "still in Garden");
    assert_eq!(stamps(&w.docs, &w.id).await.0, before);
}

/// M2: the `move_document` tool moves through the document service too.
#[tokio::test]
async fn m2_the_move_document_tool_is_a_stamped_move() {
    let w = world("tool-move", true).await;
    let (before, _) = stamps(&w.docs, &w.id).await;
    let call = crate::tools::ToolCall {
        name: "move_document".into(),
        arguments: serde_json::json!({"document_title": "Seeds", "thread_name": "Kitchen"}),
    };
    let result = crate::tools::execute_write_tool(&call, w.store.as_ref(), Some(&w.docs)).await;
    assert!(result.success, "{}", result.output);
    assert_eq!(w.docs.read(&w.id).await.unwrap().thread_id, w.to);
    let (after, prev) = stamps(&w.docs, &w.id).await;
    assert!(after > before, "a new stamp: the move travels");
    assert_eq!(prev, before);
}

/// M2 (CONTRACT-QUESTION above): the tool without a service moves nothing.
#[tokio::test]
async fn m2_without_the_document_service_the_tool_moves_nothing() {
    let w = world("tool-move-locked", true).await;
    let call = crate::tools::ToolCall {
        name: "move_document".into(),
        arguments: serde_json::json!({"document_title": "Seeds", "thread_name": "Kitchen"}),
    };
    let result = crate::tools::execute_write_tool(&call, w.store.as_ref(), None).await;
    assert!(!result.success, "no success without the service: {}", result.output);
    assert!(result.event.is_none());
    assert_eq!(w.docs.read(&w.id).await.unwrap().thread_id, w.from, "still in Garden");
}
