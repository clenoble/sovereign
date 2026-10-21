//! G9 — restoring a version.
//!
//! Spec ch. 03, *Document service — contract (C-DOC)*, G9: "**Restore** writes
//! the version's title and text back, after a version of the current state,
//! and the result is a new version — history is never rewritten. A legacy
//! snapshot is written as v2, a newer-format one verbatim. Restore changes
//! neither trust, provenance, thread nor trash state; it refuses a commit of
//! another document and a trashed or purged document."

mod common;

use common::*;
use sovereign_docs::{ChangeKind, DocError, Trust};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g09_restore_brings_back_title_and_text_as_a_new_version() {
    let s = Stack::new("g09-basic").await;
    s.policy.set(PolicyMode::Always);
    let id = s.create_text("Title one", "text one").await;
    let v1 = s.commits(&id).await[0].id_string().unwrap();
    let base = s.revision(&id).await;
    s.svc
        .save(&id, sovereign_docs::Edit {
            base,
            title: Some("Title two".into()),
            content: Some(sovereign_docs::EditContent::PlainText("text two".into())),
            origin: sovereign_docs::Origin::Assistant,
            explicit: false,
            pasted_tokens: vec![],
        })
        .await
        .unwrap();
    let n = s.commits(&id).await.len();
    let current = s.stored(&id).await;
    let out = s.svc.restore_version(&id, &v1).await.unwrap();
    let (title, content) = s.stored(&id).await;
    assert_eq!(title, "Title one");
    assert_eq!(texts(&content), vec!["text one"]);
    let commits = s.commits(&id).await;
    assert_eq!(commits.len(), n + 1, "one new version, history kept");
    assert_eq!(out.version.as_deref(), commits[0].id_string().as_deref());
    assert!(s.committed_states(&id).await.contains(&current), "the replaced state is kept");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g09_restore_versions_an_uncommitted_current_state_first() {
    let s = Stack::new("g09-uncommitted").await;
    s.policy.set(PolicyMode::Never);
    let id = s.create_text("T", "v1").await;
    let v1 = s.commits(&id).await[0].id_string().unwrap();
    s.editor_save(&id, 1, v2(vec![text_block(1, "typed, never versioned")])).await.unwrap();
    let typed = s.stored(&id).await;
    s.svc.restore_version(&id, &v1).await.unwrap();
    assert!(s.committed_states(&id).await.contains(&typed));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g09_restore_of_another_documents_commit_is_refused() {
    let s = Stack::new("g09-other").await;
    let a = s.create_text("A", "a").await;
    let b = s.create_text("B", "b").await;
    let cb = s.commits(&b).await[0].id_string().unwrap();
    let before = s.stored(&a).await;
    let r = s.svc.restore_version(&a, &cb).await;
    assert!(matches!(r, Err(DocError::VersionRefused { .. })));
    assert_eq!(s.stored(&a).await, before);
    let r = s.svc.read_version(&a, &cb).await;
    assert!(matches!(r, Err(DocError::VersionRefused { .. })), "preview refused too");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g09_restore_keeps_trust_provenance_and_thread() {
    let s = Stack::new("g09-meta").await;
    let mut new = new_text_doc("T", "x");
    new.trust = Trust::External;
    new.provenance = sovereign_docs::Provenance::Web { url: "https://example.org/a".into() };
    let id = s.svc.create(new).await.unwrap().id;
    let v1 = s.commits(&id).await[0].id_string().unwrap();
    s.assistant_save(&id, "y").await.unwrap();
    s.svc.set_trust(&id, Trust::Owned).await.unwrap();
    s.svc.restore_version(&id, &v1).await.unwrap();
    let v = s.svc.read(&id).await.unwrap();
    assert_eq!(v.trust, Trust::Owned, "the version does not carry trust back");
    assert_eq!(v.provenance, sovereign_docs::Provenance::Web { url: "https://example.org/a".into() });
    assert_eq!(v.thread_id, "thread:t1");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g09_restore_of_a_newer_format_version_is_written_verbatim() {
    let s = Stack::new("g09-newer").await;
    let v3 = r#"{"format":"sovereign.doc","v":3,"blocks":[]}"#;
    let id = s.legacy("F", v3).await;
    let c = s.edb.commit_document(&id, "legacy").await.unwrap().id_string().unwrap();
    // The document is replaced by sync with a v2 state, then the v3 version restored.
    s.svc
        .apply_remote(sovereign_docs::RemoteDocument {
            id: id.clone(),
            title: "F".into(),
            content: v2(vec![text_block(1, "now v2")]),
            thread_id: "thread:t1".into(),
            trust: Some(Trust::Owned),
            provenance: None,
            modified_at: s.clock.get(),
            deleted_at: None,
            device: "laptop".into(),
            trust_decision: None,
        })
        .await
        .unwrap();
    s.svc.restore_version(&id, &c).await.unwrap();
    assert_eq!(s.stored(&id).await.1, v3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g09_restore_is_refused_in_the_trash_and_after_purge() {
    let s = Stack::new("g09-trash").await;
    let id = s.create_text("T", "x").await;
    let v1 = s.commits(&id).await[0].id_string().unwrap();
    s.svc.trash(&id).await.unwrap();
    assert!(matches!(s.svc.restore_version(&id, &v1).await, Err(DocError::InTrash(_))));
    s.svc.empty_trash().await.unwrap();
    assert!(matches!(s.svc.restore_version(&id, &v1).await, Err(DocError::Purged(_))));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g09_read_version_previews_without_writing() {
    let s = Stack::new("g09-preview").await;
    let id = s.create_text("Old", "old text").await;
    let v1 = s.commits(&id).await[0].id_string().unwrap();
    s.assistant_save(&id, "new text").await.unwrap();
    let before = s.stored(&id).await;
    let n = s.commits(&id).await.len();
    s.events.take();
    let p = s.svc.read_version(&id, &v1).await.unwrap();
    assert_eq!(p.title, "Old");
    assert_eq!(p.doc.as_ref().unwrap().plain_text(), "old text");
    assert_eq!(s.stored(&id).await, before);
    assert_eq!(s.commits(&id).await.len(), n);
    assert!(s.events.peek().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g09_restore_emits_version_restored() {
    let s = Stack::new("g09-event").await;
    let id = s.create_text("T", "x").await;
    let v1 = s.commits(&id).await[0].id_string().unwrap();
    s.assistant_save(&id, "y").await.unwrap();
    s.events.take();
    s.svc.restore_version(&id, &v1).await.unwrap();
    let ev = s.events.take();
    assert_eq!(ev.len(), 1);
    assert_eq!(ev[0].kind, ChangeKind::VersionRestored);
    assert_eq!(ev[0].id, id);
}
