//! v0.0.13 lot L5, B1 — C-MERGE on the phone, the Rust side (spec ch. 03,
//! *Merging keeps the source thread — contract (C-MERGE)*, v2.0.109; MG2 and
//! MG3): what `canvas_load` gives the phone for each thread, so that it draws
//! an ended lane with its mark and offers an ended thread for nothing.
//!
//! **The API this file names** (« types and signatures » commit; the body is
//! the implementer's): `tauri_commands::canvas::thread_dtos(db) ->
//! Result<Vec<ThreadDto>, String>`, the threads `canvas_load` returns, and the
//! `ThreadDto` fields `merged_into`, `merged_into_name`, `ended_at`. The
//! Svelte side is `frontend/src/lib/components/mobile/b1Merge.svelte.test.ts`.
//!
//! Stores: a real encrypted store over an in-memory SurrealDB
//! (`docs_host::testing`); nothing touches a real workspace.
//!
//! Written before the code by the B1 test author; the implementer does not
//! edit this file.

use sovereign_ai::docs_host::{build_document_service, testing};
use sovereign_db::GraphDB;

use crate::tauri_commands::canvas::thread_dtos;

/// MG2 and MG3: the ended thread is listed (its lane is drawn), with the
/// thread it was merged into, that thread's name and its end date (RFC 3339,
/// the merge stamp's time); the thread merged into, and any live thread, have
/// none. `canvas_load` returns these threads.
///
/// Red today: `thread_dtos` is not built (`unimplemented!`), and the merge
/// deletes the source.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mg2_the_phone_is_given_each_threads_merge() {
    let store = testing::encrypted_memory_store("b1-app-dtos").await;
    let docs = build_document_service(store.clone(), Some(testing::account_key()), "phone".into());
    let garden = docs.thread_create("Garden", "").await.unwrap();
    let kitchen = docs.thread_create("Kitchen", "").await.unwrap();
    let shed = docs.thread_create("Shed", "").await.unwrap();
    docs.thread_merge(&kitchen, &garden).await.expect("merged");
    let ended = store.get_thread(&garden).await.unwrap().ended_at.expect("MG1: Garden has its end date");

    let dtos = thread_dtos(store.as_ref()).await.expect("the threads");
    let of = |id: &str| dtos.iter().find(|t| t.id == id).unwrap_or_else(|| panic!("{id} is listed"));
    let g = of(&garden);
    assert_eq!(g.name, "Garden");
    assert_eq!(g.merged_into.as_deref(), Some(kitchen.as_str()));
    assert_eq!(g.merged_into_name.as_deref(), Some("Kitchen"));
    let at = g.ended_at.as_deref().map(|s| chrono::DateTime::parse_from_rfc3339(s).expect("RFC 3339").timestamp_millis());
    assert_eq!(at, Some(ended.timestamp_millis()), "the end date, to the millisecond");
    for id in [&kitchen, &shed] {
        let t = of(id);
        assert_eq!((t.merged_into.as_deref(), t.merged_into_name.as_deref(), t.ended_at.as_deref()), (None, None, None), "{}", t.name);
    }
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/tauri_commands/canvas.rs")).unwrap();
    let load = &src[src.find("pub async fn canvas_load(").expect("canvas_load")..];
    let load = &load[..load.find("\n}\n").unwrap_or(load.len())];
    assert!(load.contains("thread_dtos("), "canvas_load returns these threads");
}
