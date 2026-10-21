//! G8 — only MAC'd commits.
//!
//! Spec ch. 03, *Document service — contract (C-DOC)*, G8: "**Only MAC'd
//! commits.** Every commit the service makes, on every path, carries a MAC
//! that verifies (ch. 07); the service holds the store only through the
//! encrypted layer, so it has no way to make another kind. A version whose MAC
//! does not verify is listed as invalid and never restored."

mod common;

use common::*;
use sovereign_db::traits::GraphDB;
use sovereign_docs::{DocError, EditContent, EditSession, MacState, RemoteDocument, Trust};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g08_every_commit_on_every_path_is_signed_at_rest_and_verifies() {
    let s = Stack::new("g08-paths").await;
    s.policy.set(PolicyMode::Always);
    let id = s.create_text("T", "created").await; // initial version
    s.editor_save(&id, 1, v2(vec![text_block(1, "editor")])).await.unwrap(); // policy version
    s.assistant_save(&id, "assistant").await.unwrap(); // non-editor version
    let base = s.revision(&id).await;
    s.svc
        .apply_skill_output(&id, base, "redactor", EditContent::PlainText("skill".into()))
        .await
        .unwrap();
    let first = s.commits(&id).await.last().unwrap().id_string().unwrap();
    s.svc.restore_version(&id, &first).await.unwrap();
    s.policy.set(PolicyMode::Never);
    s.editor_save(&id, 2, v2(vec![text_block(1, "pending")])).await.unwrap();
    s.svc.end_session(&id, EditSession(2)).await.unwrap();
    s.svc
        .apply_remote(RemoteDocument {
            id: id.clone(),
            title: "T".into(),
            content: v2(vec![text_block(1, "remote")]),
            thread_id: "thread:t1".into(),
            trust: Some(Trust::Owned),
            provenance: None,
            modified_at: s.clock.get() + chrono::Duration::minutes(1),
            deleted_at: None,
            device: "phone".into(),
            trust_decision: None,
        })
        .await
        .unwrap();
    let legacy = s.legacy("L", "raw legacy").await;
    s.svc.migrate_all().await.unwrap();

    for doc in [&id, &legacy] {
        let raw = s.raw_commits(doc).await;
        assert!(!raw.is_empty());
        for c in &raw {
            assert!(c.signature.is_some(), "commit {:?} ({}) unsigned at rest", c.id, c.message);
        }
        for v in s.svc.versions(doc).await.unwrap() {
            assert_eq!(v.mac, MacState::Verified, "{}", v.message);
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g08_a_version_whose_mac_does_not_verify_is_listed_invalid_and_never_restored() {
    let s = Stack::new("g08-tamper").await;
    s.policy.set(PolicyMode::Always);
    let id = s.create_text("T", "first").await;
    s.editor_save(&id, 1, v2(vec![text_block(1, "second")])).await.unwrap();
    let target = s.commits(&id).await.last().unwrap().id_string().unwrap();
    // A disk-write attacker swaps the MAC.
    s.raw.set_commit_signature(&target, "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=").await.unwrap();

    let listed = s.svc.versions(&id).await.unwrap();
    let t = listed.iter().find(|v| v.commit_id == target).expect("still listed");
    assert_eq!(t.mac, MacState::Invalid);

    let before = s.stored(&id).await;
    let n = s.commits(&id).await.len();
    s.events.take();
    let r = s.svc.restore_version(&id, &target).await;
    assert!(matches!(r, Err(DocError::VersionRefused { .. })));
    assert_eq!(s.stored(&id).await, before, "nothing written");
    assert_eq!(s.commits(&id).await.len(), n, "no version made");
    assert!(s.events.peek().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g08_legacy_unsigned_versions_are_listed_as_such() {
    let s = Stack::new("g08-legacy").await;
    let id = s.legacy("L", "raw").await;
    // A commit made below the encrypted layer: unsigned, like pre-MAC history.
    let c = s.raw.commit_document(&id, "pre-MAC commit").await.unwrap();
    let cid = c.id_string().unwrap();
    let listed = s.svc.versions(&id).await.unwrap();
    assert_eq!(listed.iter().find(|v| v.commit_id == cid).unwrap().mac, MacState::Unsigned);
}
