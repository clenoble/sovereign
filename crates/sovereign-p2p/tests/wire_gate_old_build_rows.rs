//! C-WIRE guarantees 11, 12, 15 — only the service writes documents; older
//! builds exchange no documents; rows.
//!
//! Spec ch. 05, *Sync wire — contract (C-WIRE)*.

mod wire_common;

use std::path::Path;

use sovereign_db::GraphDB;
use sovereign_p2p::protocol::sync::SyncTable;
use sovereign_p2p::sync_engine::compute_all_row_diffs;
use sovereign_p2p::wire::{plan_documents, restorable_tables, WIRE_VERSION};
use wire_common::*;

/// The non-test part of a source file: everything before its trailing test
/// module (`#[cfg(test)]` followed by `mod …`). A `#[cfg(test)]` on a single
/// item (an import, a helper) does not end the production code — the first
/// version of this helper cut `sync_service.rs` at a test-only import on its
/// line 10 and saw none of its writes.
fn production(path: &Path) -> String {
    let s = std::fs::read_to_string(path).expect("source");
    let mut from = 0;
    while let Some(rel) = s[from..].find("#[cfg(test)]") {
        let at = from + rel;
        let rest = s[at + "#[cfg(test)]".len()..].trim_start();
        if rest.starts_with("mod ") {
            return s[..at].to_string();
        }
        from = at + 1;
    }
    s
}

/// 11. "Only the service writes documents: no store write to documents from
/// `sovereign-p2p`."
#[test]
fn wire11_sync_makes_no_document_write_of_its_own() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let forbidden = [
        ".update_document(",
        "commit_document(",
        "create_document_with_id(",
        "soft_delete_document(",
        "restore_soft_deleted_document(",
        "set_document_peer_review(",
    ];
    let mut found = vec![];
    for file in ["sync_service.rs", "backup.rs", "node.rs", "recovery.rs"] {
        let text = production(&src.join(file));
        for pat in forbidden {
            for (n, line) in text.lines().enumerate() {
                if line.contains(pat) && !line.trim_start().starts_with("//") {
                    found.push(format!("{file}:{}: {pat}", n + 1));
                }
            }
        }
    }
    assert!(found.is_empty(), "document writes around the service:\n{}", found.join("\n"));
}

/// 12. "Older peers exchange no documents: a peer without a wire version is
/// sent no document state and none of its document states is applied; rows sync
/// with it as before."
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wire12_a_peer_on_an_older_build_gets_and_gives_no_documents() {
    let (a, b) = (device("w12-a", 31).await, device("w12-b", 32).await);
    pair(&a, &b);
    let on_a = create(&a, "Only on A", "a").await;
    let on_b = create(&b, "Only on B", "b").await;
    b.edb.create_thread(sovereign_db::schema::Thread::new("Lane on B".into(), String::new())).await.unwrap();
    let ma = manifest(&a).await;
    assert_eq!(ma.wire_version, Some(WIRE_VERSION), "this build says which wire it speaks");
    let old = as_old_build(manifest(&b).await);
    let plan = plan_documents(&ma, &old);
    assert!(plan.blocked_old_peer, "recognised as an older build");
    assert!(plan.fetch.is_empty() && plan.push.is_empty() && plan.concurrent.is_empty(), "{plan:?}");
    assert!(!compute_all_row_diffs(&ma, &old).is_empty(), "rows still sync with it");
    // Its document envelopes (no wire state) are not applied either.
    let mut env = b.sync.get_commits_since(&on_b, None, &a.peer).await.unwrap();
    for e in &mut env {
        e.sealed_wire.clear();
        e.wire_nonce.clear();
    }
    let _ = a.sync.apply_commits(env, &b.peer).await;
    assert!(a.edb.get_document(&on_b).await.is_err(), "nothing from the older build applied");
    let _ = on_a;
}

/// 15. "Rows: … every table sync can overwrite is restorable in both UIs."
#[test]
fn wire15_every_table_sync_can_overwrite_is_restorable() {
    let restorable = restorable_tables();
    for t in [
        SyncTable::Thread,
        SyncTable::Entity,
        SyncTable::PiiRecord,
        SyncTable::Contact,
        SyncTable::Message,
        SyncTable::Conversation,
        SyncTable::SuggestedLink,
    ] {
        assert!(restorable.contains(&t), "{t:?} can be overwritten by sync but is not restorable");
    }
}
