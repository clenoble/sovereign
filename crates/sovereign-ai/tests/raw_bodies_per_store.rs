//! The pending raw bodies of `docs_host` belong to their store (BACKLOG §9,
//! found 2026-10-15). A document created through the service is ingested before
//! its row exists, so its raw body waits in a pending list and is written by
//! the next flush. That list was process-wide and flushed under one lock held
//! across awaits: a write in one store wrote another store's pending raw body
//! (SurrealDB transaction conflicts in that store), and awaited a store whose
//! runtime was gone (the lock never released, every later write hung).
//! Measured 2026-10-17 on the shell's step-5 tests run in parallel: 19
//! conflicts and 18 hangs in 200 runs; 0 in 80 runs serially.
//!
//! Invariants, whatever the implementation:
//! 1. A write in one store never writes another store's pending raw body.
//! 2. A pending raw body whose store's runtime is gone never blocks a write in
//!    another store.
//! 3. A store's own pending raw body is still written by its next flush.

use std::sync::Arc;
use std::time::Duration;

use sovereign_ai::docs_host::{self, build_document_service, new_user_document, testing};
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::schema::Thread;
use sovereign_db::traits::GraphDB;
use sovereign_docs::DocumentService;

const BOUND: Duration = Duration::from_secs(20);

async fn setup(tag: &str) -> (Arc<EncryptedGraphDB>, DocumentService, String) {
    let store = testing::encrypted_memory_store(tag).await;
    let docs = build_document_service(store.clone(), Some(testing::account_key()), "desktop".into());
    let lane = store
        .create_thread(Thread::new("Lane".into(), String::new()))
        .await
        .unwrap()
        .id_string()
        .unwrap();
    (store, docs, lane)
}

/// A document created through the service alone: its raw body is left pending.
async fn create_pending(docs: &DocumentService, store: &EncryptedGraphDB, lane: &str) -> String {
    let id = docs.create(new_user_document("Pending", lane, "a pending body")).await.unwrap().id;
    assert!(
        store.get_document(&id).await.unwrap().body_raw_encrypted.is_none(),
        "precondition: a create through the service alone leaves the raw body pending"
    );
    id
}

async fn raw_body_written(store: &EncryptedGraphDB, id: &str) -> bool {
    store.get_document(id).await.unwrap().body_raw_encrypted.is_some()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_write_in_one_store_never_writes_another_stores_pending_raw_body() {
    let (store_a, docs_a, lane_a) = setup("raw-per-store-a").await;
    let (store_b, docs_b, lane_b) = setup("raw-per-store-b").await;
    let a = create_pending(&docs_a, &store_a, &lane_a).await;

    let b = tokio::time::timeout(BOUND, docs_host::create(&docs_b, new_user_document("B", lane_b.as_str(), "b body")))
        .await
        .expect("a create in store B finishes")
        .unwrap()
        .id;

    assert!(raw_body_written(&store_b, &b).await, "B's own raw body is written");
    assert!(!raw_body_written(&store_a, &a).await, "B's write touched store A");

    // 3: A's next flush writes A's pending raw body.
    tokio::time::timeout(BOUND, docs_host::create(&docs_a, new_user_document("A2", lane_a.as_str(), "a2 body")))
        .await
        .expect("a create in store A finishes")
        .unwrap();
    assert!(raw_body_written(&store_a, &a).await, "A's pending raw body is written by A's next flush");
}

#[test]
fn a_pending_raw_body_of_a_store_whose_runtime_is_gone_blocks_no_other_store() {
    // Store A lives on its own runtime, leaves a pending raw body, and its
    // runtime is shut down — as when a test ends, or a session's runtime stops.
    let rt_a = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap();
    let kept = rt_a.block_on(async {
        let (store_a, docs_a, lane_a) = setup("raw-gone-a").await;
        create_pending(&docs_a, &store_a, &lane_a).await;
        (store_a, docs_a) // kept alive: only the runtime goes
    });
    rt_a.shutdown_timeout(Duration::from_secs(5));

    let rt_b = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap();
    rt_b.block_on(async {
        let (store_b, docs_b, lane_b) = setup("raw-gone-b").await;
        let b = tokio::time::timeout(BOUND, docs_host::create(&docs_b, new_user_document("B", lane_b.as_str(), "b body")))
            .await
            .expect("a create in store B finishes while store A's runtime is gone")
            .unwrap()
            .id;
        assert!(raw_body_written(&store_b, &b).await, "B's own raw body is written");
        // And a second write too: nothing is left holding the pending list.
        tokio::time::timeout(BOUND, docs_host::create(&docs_b, new_user_document("B2", lane_b.as_str(), "b2 body")))
            .await
            .expect("a second create in store B finishes")
            .unwrap();
    });
    drop(kept);
}
