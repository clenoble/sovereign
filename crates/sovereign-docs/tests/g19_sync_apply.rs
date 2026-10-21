//! G19 — sync applies without losing anything.
//!
//! Spec ch. 03, *Document service — contract (C-DOC)*, G19: "**Sync applies
//! without losing anything.** The service applies the state sync decided on;
//! it does not decide who wins. Before a peer's state replaces the local one,
//! the local state is a version (changed 2026-10-15, C-WIRE 13: and no review flag is raised). A
//! peer's tombstone moves the document to the trash, restorable; a live peer
//! state brings a trashed document back; a purged id is ignored; a state equal
//! to the local one changes nothing. Legacy content from an older peer is
//! stored as v2, newer-format content verbatim (and read-only)."
//!
//! Ch. 05, *Deletions*: "A deletion applied from a peer is a soft delete
//! (restorable) and is flagged for review like any other peer change; a live
//! winning state restores a locally deleted record."

mod common;

use chrono::Duration;
use common::*;
use sovereign_docs::{ReadOnlyReason, RemoteDocument, RemoteOutcome, Trust, Writable};

fn remote(id: &str, content: String, at: chrono::DateTime<chrono::Utc>) -> RemoteDocument {
    RemoteDocument {
        id: id.into(),
        title: "From peer".into(),
        content,
        thread_id: "thread:t1".into(),
        trust: Some(Trust::Owned),
        provenance: None,
        modified_at: at,
        deleted_at: None,
        device: "phone".into(),
        trust_decision: None,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g19_a_new_document_is_created_under_its_origin_id_with_a_version() {
    let s = Stack::new("g19-new").await;
    let out = s.svc.apply_remote(remote("document:p1", v2(vec![text_block(1, "hi")]), s.clock.get())).await.unwrap();
    assert_eq!(out, RemoteOutcome::Created);
    assert_eq!(texts(&s.stored("document:p1").await.1), vec!["hi"]);
    assert_eq!(s.commits("document:p1").await.len(), 1);
    // Applying it again changes nothing.
    let again = s.svc.apply_remote(remote("document:p1", v2(vec![text_block(1, "hi")]), s.clock.get())).await.unwrap();
    assert_eq!(again, RemoteOutcome::Unchanged);
    assert_eq!(s.commits("document:p1").await.len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g19_an_overwrite_versions_the_local_state_and_raises_no_review_flag() {
    let s = Stack::new("g19-overwrite").await;
    s.policy.set(PolicyMode::Never);
    let id = s.create_text("Local", "x").await;
    s.editor_save(&id, 1, v2(vec![text_block(1, "local typing, not a version")])).await.unwrap();
    let local = s.stored(&id).await;
    s.clock.advance(Duration::minutes(1));
    let out = s.svc.apply_remote(remote(&id, v2(vec![text_block(1, "peer text")]), s.clock.get())).await.unwrap();
    assert_eq!(out, RemoteOutcome::Applied);
    let (title, content) = s.stored(&id).await;
    assert_eq!(title, "From peer");
    assert_eq!(texts(&content), vec!["peer text"]);
    assert!(s.committed_states(&id).await.contains(&local));
    // C-WIRE 13 (Céline, 2026-10-15): no review flag on a change from her own devices.
    assert!(!s.svc.read(&id).await.unwrap().peer_review_pending && !s.stored_doc(&id).await.peer_review_pending);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g19_a_tombstone_moves_the_document_to_the_trash_restorably() {
    let s = Stack::new("g19-tombstone").await;
    let id = s.create_text("T", "keep me").await;
    let before = s.stored(&id).await;
    let mut r = remote(&id, s.stored(&id).await.1, s.clock.get());
    r.title = "T".into();
    r.deleted_at = Some(s.clock.get());
    let out = s.svc.apply_remote(r).await.unwrap();
    assert_eq!(out, RemoteOutcome::Trashed);
    assert!(s.svc.list(None).await.unwrap().iter().all(|d| d.id != id));
    assert!(s.svc.list_trash().await.unwrap().iter().any(|d| d.id == id));
    assert!(!s.svc.read(&id).await.unwrap().peer_review_pending, "no review flag (C-WIRE 13)");
    s.svc.restore_from_trash(&id).await.unwrap();
    assert_eq!(s.stored(&id).await, before);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g19_a_tombstone_never_removes_content_or_history() {
    let s = Stack::new("g19-tombstone-keeps").await;
    let id = s.create_text("T", "x").await;
    s.editor_save(&id, 1, v2(vec![text_block(1, "local")])).await.unwrap();
    let local = s.stored(&id).await;
    let mut r = remote(&id, v2(vec![text_block(1, "older peer text")]), s.clock.get());
    r.deleted_at = Some(s.clock.get());
    s.svc.apply_remote(r).await.unwrap();
    assert!(s.stored(&id).await == local || s.committed_states(&id).await.contains(&local));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g19_a_live_peer_state_brings_a_trashed_document_back() {
    let s = Stack::new("g19-revive").await;
    let id = s.create_text("T", "x").await;
    s.svc.trash(&id).await.unwrap();
    let trashed = s.stored(&id).await;
    s.clock.advance(Duration::minutes(1));
    let out = s.svc.apply_remote(remote(&id, v2(vec![text_block(1, "edited on phone")]), s.clock.get())).await.unwrap();
    assert_eq!(out, RemoteOutcome::RestoredFromTrash);
    assert!(s.svc.list(None).await.unwrap().iter().any(|d| d.id == id));
    assert_eq!(texts(&s.stored(&id).await.1), vec!["edited on phone"]);
    assert!(s.committed_states(&id).await.contains(&trashed));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g19_legacy_content_from_an_older_peer_is_stored_as_v2() {
    let s = Stack::new("g19-legacy").await;
    for (n, legacy) in [r#"{"body":"from an old phone","images":[]}"#, "raw from an old shell"].iter().enumerate() {
        let id = format!("document:old{n}");
        s.svc.apply_remote(remote(&id, legacy.to_string(), s.clock.get())).await.unwrap();
        let stored = s.stored(&id).await.1;
        assert_v2(&stored);
        assert_eq!(texts(&stored).join(""), if n == 0 { "from an old phone" } else { "raw from an old shell" });
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g19_newer_format_content_from_a_newer_peer_is_stored_verbatim_and_read_only() {
    let s = Stack::new("g19-newer").await;
    let v3 = r#"{"format":"sovereign.doc","v":3,"blocks":[{"type":"text","id":"00000000000000000000000000001001","text":"future"}]}"#;
    s.svc.apply_remote(remote("document:fut", v3.into(), s.clock.get())).await.unwrap();
    assert_eq!(s.stored("document:fut").await.1, v3);
    let v = s.svc.read("document:fut").await.unwrap();
    assert_eq!(v.writable, Writable::ReadOnly(ReadOnlyReason::NewerFormat { v: 3 }));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g19_an_identical_peer_state_changes_nothing() {
    let s = Stack::new("g19-same").await;
    let id = s.create_text("Same", "same text").await;
    let (title, content) = s.stored(&id).await;
    let n = s.commits(&id).await.len();
    s.events.take();
    let mut r = remote(&id, content.clone(), s.clock.get());
    r.title = title;
    assert_eq!(s.svc.apply_remote(r).await.unwrap(), RemoteOutcome::Unchanged);
    assert_eq!(s.commits(&id).await.len(), n);
    assert!(s.events.peek().is_empty());
    assert!(!s.svc.read(&id).await.unwrap().peer_review_pending);
}
