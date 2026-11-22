//! C-SWEEP **SW1** (spec ch. 09, v2.0.111), the document service's half: the
//! state follows the write's origin, « which ingest is given ». The service
//! gives the personal-data hook the origin of every write
//! (`IngestRequest::origin`), and the sweep and the review write through it
//! with their own origins, tagged `pii-sweep` and `pii-review` and shown as
//! « Personal-data sweep » and « Personal-data review » (the Fable maille, by
//! delegation of Céline, 2026-11-17). SW2: the sweep gives the hook the version
//! it analyses (its normalised hash).
//!
//! Over the fake hook of `common` (it records each request). What the regex
//! hook does with the origin is held in `sovereign-ai` (`pii/c_sweep_tests.rs`).
//!
//! Written before the code by the test author of lot LS(a); whoever implements
//! does not edit this file.

mod common;

use common::*;
use sovereign_docs::{Duplicate, Edit, EditContent, EditSession, IngestRequest, Origin, Provenance, RemoteDocument, Trust};

fn last_for(s: &Stack, id: &str) -> IngestRequest {
    s.pii.requests().into_iter().rev().find(|r| r.document_id == id).expect("the hook ran for the document")
}

fn is_editor(o: &Option<Origin>) -> bool {
    matches!(o, Some(Origin::Editor { .. }))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sw1_an_editor_creation_gives_the_editor_origin() {
    let s = Stack::new("sw1-o-create").await;
    let id = s.create_text("T", "text").await;
    assert!(is_editor(&last_for(&s, &id).origin), "{:?}", last_for(&s, &id).origin);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sw1_an_editor_save_gives_the_editor_origin() {
    let s = Stack::new("sw1-o-save").await;
    let id = s.create_text("T", "text").await;
    s.editor_save(&id, 7, v2(vec![text_block(1, "changed")])).await.unwrap();
    assert_eq!(last_for(&s, &id).origin, Some(Origin::Editor { session: EditSession(7) }));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sw1_an_assistant_save_gives_the_assistants_origin() {
    let s = Stack::new("sw1-o-assistant-save").await;
    let id = s.create_text("T", "text").await;
    s.assistant_save(&id, "changed").await.unwrap();
    assert_eq!(last_for(&s, &id).origin, Some(Origin::Assistant), "the editor's write path, not its origin");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sw1_an_assistant_creation_gives_the_assistants_origin() {
    let s = Stack::new("sw1-o-assistant-create").await;
    let mut new = new_text_doc("T", "text");
    new.origin = Origin::Assistant;
    let id = s.svc.create(new).await.unwrap().id;
    assert_eq!(last_for(&s, &id).origin, Some(Origin::Assistant));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sw1_a_skill_output_gives_the_skills_origin() {
    let s = Stack::new("sw1-o-skill").await;
    let id = s.create_text("T", "text").await;
    let base = s.revision(&id).await;
    s.svc.apply_skill_output(&id, base, "markdown_editor", EditContent::PlainText("changed".into())).await.unwrap();
    assert_eq!(last_for(&s, &id).origin, Some(Origin::Skill { name: "markdown_editor".into() }));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sw1_an_import_gives_the_imports_origin() {
    let s = Stack::new("sw1-o-import").await;
    let mut new = new_text_doc("Imported", "text");
    new.origin = Origin::Import;
    new.trust = Trust::Undetermined;
    new.provenance = Provenance::Import { source_path: "C:/x.md".into() };
    let id = s.svc.create(new).await.unwrap().id;
    assert_eq!(last_for(&s, &id).origin, Some(Origin::Import));
}

/// SW1, a copy (the Fable maille, by delegation of Céline, 2026-11-17): a
/// value the source had not analysed is replaced by the copy's ingest as
/// suspected, whoever asked for the copy: « the copy's ingest is never given
/// the editor's origin, though the request carries it ». (Replaces this
/// file's first reading, in ls-a-tests-v1, that a copy carries its request's
/// origin.)
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sw1_a_copys_ingest_is_never_given_the_editors_origin() {
    let s = Stack::new("sw1-o-duplicate").await;
    let src = s.create_text("T", "text").await;
    let copy = s
        .svc
        .duplicate(Duplicate { source: src, title: None, thread_id: None, origin: Origin::Editor { session: EditSession(3) } })
        .await
        .unwrap()
        .id;
    let origin = last_for(&s, &copy).origin;
    assert!(!is_editor(&origin), "the copy's ingest was given the editor's origin: {origin:?}");
}

/// The writes with no writer origin: content received by sync, and the
/// restore of a version. Their `None` is today's value too; this test was
/// seen red under the mutation « the service gives `Some(Origin::Import)` on
/// every path », reverted.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sw1_sync_and_restore_give_no_writer_origin() {
    let s = Stack::new("sw1-o-none").await;
    let synced = "document:fromphone".to_string();
    s.svc
        .apply_remote(RemoteDocument {
            id: synced.clone(),
            title: "From the phone".into(),
            content: v2(vec![text_block(1, "text")]),
            thread_id: "thread:t1".into(),
            trust: Some(Trust::Owned),
            provenance: Some(Provenance::Written),
            modified_at: s.clock.get(),
            deleted_at: None,
            device: "phone".into(),
            trust_decision: None,
        })
        .await
        .unwrap();
    assert_eq!(last_for(&s, &synced).origin, None);

    let legacy = s.legacy("Old", "old text").await;
    let c = s.edb.commit_document(&legacy, "legacy").await.unwrap().id_string().unwrap();
    s.assistant_save(&legacy, "clean").await.unwrap();
    s.svc.restore_version(&legacy, &c).await.unwrap();
    assert_eq!(last_for(&s, &legacy).origin, None);
}

/// SW1, SW2: the sweep runs the hook with its own origin and the version it
/// analyses (the normalised hash of the stored state), makes one version
/// shown as « Personal-data sweep », tagged `pii-sweep`, and marks the
/// document analysed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sw1_the_sweep_writes_with_its_own_origin_and_version() {
    let s = Stack::new("sw1-o-sweep").await;
    s.pii.set(PiiMode::Fail);
    let id = s.create_text("T", &format!("write to {SECRET}")).await;
    assert!(!s.svc.read(&id).await.unwrap().pii_scanned);
    assert_eq!(s.svc.unanalysed(10).await.unwrap(), vec![id.clone()]);
    let version = s.svc.stored_state(&id).await.unwrap().normalized_hash;
    let commits = s.commits(&id).await.len();

    s.pii.set(PiiMode::Tokenize);
    let out = s.svc.sweep(&id).await.unwrap();
    assert!(out.changed);
    let req = last_for(&s, &id);
    assert_eq!(req.origin, Some(Origin::PiiSweep));
    assert_eq!(req.version.as_deref(), Some(version.as_str()), "the finding's version is the state analysed");
    assert!(s.svc.read(&id).await.unwrap().pii_scanned);
    assert!(s.svc.unanalysed(10).await.unwrap().is_empty());
    assert_eq!(s.commits(&id).await.len(), commits + 1, "one new version");
    let messages: Vec<String> = s.svc.versions(&id).await.unwrap().into_iter().map(|v| v.message).collect();
    assert!(messages.iter().any(|m| m == "Personal-data sweep"), "{messages:?}");
    assert_eq!(s.stored_doc(&id).await.last_writer.as_deref(), Some("pii-sweep"));
}

/// SW1: the sweep never touches a document already analysed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sw1_the_sweep_leaves_an_analysed_document_alone() {
    let s = Stack::new("sw1-o-sweep-analysed").await;
    let id = s.create_text("T", "text").await;
    assert!(s.svc.read(&id).await.unwrap().pii_scanned);
    s.pii.clear();
    let commits = s.commits(&id).await.len();
    s.events.take();
    let out = s.svc.sweep(&id).await.unwrap();
    assert!(!out.changed);
    assert!(s.pii.requests().is_empty(), "the hook is not run");
    assert_eq!(s.commits(&id).await.len(), commits);
    assert!(s.events.take().is_empty(), "no event");
}

/// SW5: a review's write carries her review's origin, one version shown as
/// « Personal-data review », tagged `pii-review`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sw5_a_review_writes_with_its_own_origin() {
    let s = Stack::new("sw5-o-review").await;
    let id = s.create_text("T", "text").await;
    let commits = s.commits(&id).await.len();
    let base = s.revision(&id).await;
    s.svc
        .save(&id, Edit {
            base,
            title: None,
            content: Some(EditContent::PlainText("restored as written".into())),
            origin: Origin::PiiReview,
            explicit: false,
            pasted_tokens: vec![],
        })
        .await
        .unwrap();
    assert_eq!(last_for(&s, &id).origin, Some(Origin::PiiReview));
    assert!(s.commits(&id).await.len() > commits);
    let messages: Vec<String> = s.svc.versions(&id).await.unwrap().into_iter().map(|v| v.message).collect();
    assert!(messages.iter().any(|m| m == "Personal-data review"), "{messages:?}");
    assert_eq!(s.stored_doc(&id).await.last_writer.as_deref(), Some("pii-review"));
}
