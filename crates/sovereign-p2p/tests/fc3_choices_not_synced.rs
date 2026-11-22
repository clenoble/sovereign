//! FC3 (spec ch. 03, *Two threads of the name sought: asked once,
//! remembered*; v0.0.13, item 9): a remembered answer « is this persona's and
//! this device's: it does not travel to the other devices ».
//!
//! Written before the code, by another author; the implementer does not edit
//! this file. Two devices from `wire_common` (each its own encrypted store,
//! document service and sync service), paired. Rows move the way `node.rs`
//! moves them for the initiator of a round, every table of the manifest
//! (`compute_all_row_diffs`, then `get_rows` / `apply_rows`), each device
//! initiating in turn, then the documents (`settle`). The threads travel (the
//! fixture checks it); the choice does not.

mod wire_common;

use sovereign_db::GraphDB;
use sovereign_p2p::sync_engine::compute_all_row_diffs;
use wire_common::*;

const NAME: &str = "Zanzibar Quokka";

/// Every table's rows, `a` initiating, as `node.rs` runs a round.
async fn rows(a: &Device, b: &Device) {
    let (ma, mb) = (manifest(a).await, manifest(b).await);
    for (table, diff) in compute_all_row_diffs(&ma, &mb) {
        if !diff.need_from_remote.is_empty() {
            let rows = b.sync.get_rows(table, &diff.need_from_remote, &a.peer).await.expect("rows from b");
            a.sync.apply_rows(table, rows, &b.peer).await.expect("rows on a");
        }
        if !diff.push_to_remote.is_empty() {
            let rows = a.sync.get_rows(table, &diff.push_to_remote, &b.peer).await.expect("rows from a");
            b.sync.apply_rows(table, rows, &a.peer).await.expect("rows on b");
        }
    }
}

async fn full_sync(a: &Device, b: &Device) {
    for _ in 0..3 {
        rows(a, b).await;
        rows(b, a).await;
        settle(a, b, 4).await;
    }
}

async fn live_named(d: &Device) -> Vec<String> {
    let mut v: Vec<String> = d
        .edb
        .list_threads()
        .await
        .unwrap()
        .into_iter()
        .filter(|t| t.deleted_at.is_none() && t.name == NAME)
        .filter_map(|t| t.id_string())
        .collect();
    v.sort();
    v
}

// FC3: the choice made on one device is not in its manifest, does not reach the other, and the other asks.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fc3_a_choice_stays_on_the_device_that_made_it() {
    let a = device("fc3-a", 61).await;
    let b = device("fc3-b", 62).await;
    pair(&a, &b);
    let x = a.docs.thread_create(NAME, "").await.unwrap();
    let _y = a.docs.thread_create(NAME, "").await.unwrap();
    sovereign_db::remember_thread_choice(a.edb.as_ref(), NAME, &x).await.expect("her answer, on a");

    let m = serde_json::to_string(&manifest(&a).await).unwrap().to_lowercase();
    assert!(!m.contains("thread_choice"), "the manifest names no choice: the table is not synced");

    full_sync(&a, &b).await;
    assert_eq!(live_named(&b).await, live_named(&a).await, "fixture: the threads themselves travel");

    let seen = b.edb.thread_choice(NAME).await;
    assert!(!matches!(seen, Ok(Some(_))), "the choice did not travel: {seen:?}");
    assert!(b.raw.list_thread_choices_raw().await.unwrap().is_empty(), "nothing of it at rest on b");
    match sovereign_db::find_or_create_choice(b.edb.as_ref(), NAME).await.unwrap() {
        sovereign_db::FindOrCreate::Ask(ts) => assert_eq!(ts.len(), 2, "b asks among the two"),
        other => panic!("on the other device, she is asked: {other:?}"),
    }
    assert!(
        matches!(sovereign_db::find_or_create_choice(a.edb.as_ref(), NAME).await.unwrap(), sovereign_db::FindOrCreate::Use(id) if id == x),
        "a still uses its own choice"
    );
}
