//! G1 — one stored form written (C-ENC).
//!
//! Spec ch. 03, *Document service — contract (C-DOC)*, G1: "**One form is
//! written.** Every write that stores content — create, save, skill output,
//! restore, sync, migration — stores format v2. Content that is not valid v2
//! is refused, and a refused write stores nothing."

mod common;

use common::*;
use sovereign_docs::{DocError, Edit, EditContent, EditSession, NewContent, Origin};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g01_create_from_text_stores_v2_with_the_text_whole() {
    let s = Stack::new("g01-create").await;
    let text = "# Notes\n\nLine *two*\n\u{1F600} and \"quotes\" \\ backslash";
    let id = s.create_text("Notes", text).await;
    let (_, stored) = s.stored(&id).await;
    assert_eq!(texts(&stored), vec![text.to_string()], "one text block, the text whole");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g01_create_from_empty_text_stores_v2() {
    let s = Stack::new("g01-create-empty").await;
    let id = s.create_text("Empty", "").await;
    let (_, stored) = s.stored(&id).await;
    let blocks = assert_v2(&stored);
    // Empty text: no block, or one empty text block — both are v2 and empty.
    assert!(texts(&stored).iter().all(|t| t.is_empty()), "{blocks:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g01_create_from_v2_keeps_every_block_and_unknown_blocks_exactly() {
    let s = Stack::new("g01-create-v2").await;
    // An unknown block written with its own odd spacing and number form.
    let unknown = r#"{"type":"x-chart","id":"00000000000000000000000000001009","series":[1, 2.50,-3],"opts":{"z":null}}"#;
    let content = format!(
        r#"{{"format":"sovereign.doc","v":2,"blocks":[{},{}]}}"#,
        serde_json::to_string(&text_block(1, "hello")).unwrap(),
        unknown
    );
    let mut new = new_text_doc("T", "");
    new.content = NewContent::V2(content);
    let id = s.svc.create(new).await.expect("create").id;
    let (_, stored) = s.stored(&id).await;
    assert_eq!(types_of(&stored), vec!["text", "x-chart"]);
    assert!(stored.contains(unknown), "an unknown block is preserved byte for byte");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g01_create_refuses_content_that_is_not_v2_and_creates_nothing() {
    let s = Stack::new("g01-create-bad").await;
    for bad in [
        "plain text claimed to be v2",
        r#"{"body":"a v1 envelope","images":[]}"#,
        r#"{"format":"sovereign.doc","v":2,"blocks":"not a list"}"#,
        r#"{"format":"sovereign.doc","v":3,"blocks":[]}"#,
    ] {
        let mut new = new_text_doc("Bad", "");
        new.content = NewContent::V2(bad.into());
        let r = s.svc.create(new).await;
        assert!(matches!(r, Err(DocError::InvalidContent(_))), "{bad:?} must be refused");
    }
    assert!(s.svc.list(None).await.unwrap().is_empty(), "a refused create leaves no document");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g01_editor_save_stores_v2() {
    let s = Stack::new("g01-save").await;
    let id = s.create_text("T", "one").await;
    let content = v2(vec![text_block(1, "two"), unknown_block(2)]);
    s.editor_save(&id, 1, content).await.expect("save");
    let (_, stored) = s.stored(&id).await;
    assert_eq!(types_of(&stored), vec!["text", "x-chart"]);
    assert_eq!(texts(&stored), vec!["two"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g01_save_refuses_content_that_is_not_v2_and_stores_nothing() {
    let s = Stack::new("g01-save-bad").await;
    let id = s.create_text("T", "kept").await;
    let before = s.stored(&id).await;
    let commits_before = s.commits(&id).await.len();
    for bad in [
        "raw text",
        r#"{"body":"v1"}"#,
        r#"{"format":"sovereign.doc","v":2}"#,
        r#"{"format":"sovereign.doc","v":9,"blocks":[]}"#,
    ] {
        let base = s.revision(&id).await;
        let r = s
            .svc
            .save(&id, Edit {
                base,
                title: None,
                content: Some(EditContent::V2(bad.into())),
                origin: Origin::Editor { session: EditSession(1) },
                explicit: true,
                pasted_tokens: vec![],
            })
            .await;
        assert!(matches!(r, Err(DocError::InvalidContent(_))), "{bad:?} must be refused");
    }
    assert_eq!(s.stored(&id).await, before, "a refused save stores nothing");
    assert_eq!(s.commits(&id).await.len(), commits_before, "and makes no version");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g01_text_write_on_a_raw_legacy_document_stores_v2() {
    let s = Stack::new("g01-legacy-raw").await;
    let id = s.legacy("Raw", "# imported\n\nbody").await;
    s.assistant_save(&id, "rewritten").await.expect("save");
    let (_, stored) = s.stored(&id).await;
    assert_eq!(texts(&stored), vec!["rewritten"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g01_title_only_save_on_a_legacy_document_stores_v2() {
    // A write stores the content in v2 even when only the title changed: the
    // stored document is rewritten, so it is rewritten in the one form.
    let s = Stack::new("g01-legacy-title").await;
    let id = s.legacy("Old", r#"{"body":"kept body","images":[]}"#).await;
    let base = s.revision(&id).await;
    s.svc
        .save(&id, Edit {
            base,
            title: Some("New".into()),
            content: None,
            origin: Origin::Editor { session: EditSession(1) },
            explicit: false,
            pasted_tokens: vec![],
        })
        .await
        .expect("save");
    let (title, stored) = s.stored(&id).await;
    assert_eq!(title, "New");
    assert_eq!(texts(&stored), vec!["kept body"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g01_skill_output_stores_v2() {
    let s = Stack::new("g01-skill").await;
    let id = s.legacy("Env", r#"{"body":"before","images":[]}"#).await;
    let base = s.revision(&id).await;
    s.svc
        .apply_skill_output(&id, base, "markdown_editor", EditContent::PlainText("after".into()))
        .await
        .expect("skill");
    let (_, stored) = s.stored(&id).await;
    assert_eq!(texts(&stored), vec!["after"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g01_restore_of_a_legacy_version_stores_v2() {
    let s = Stack::new("g01-restore").await;
    let id = s.legacy("Env", r#"{"body":"version one","images":[]}"#).await;
    // A legacy commit of the legacy content, made before the service existed.
    let c = s.edb.commit_document(&id, "legacy commit").await.unwrap();
    let cid = c.id_string().unwrap();
    s.assistant_save(&id, "version two").await.expect("save");
    s.svc.restore_version(&id, &cid).await.expect("restore");
    let (_, stored) = s.stored(&id).await;
    assert_eq!(texts(&stored), vec!["version one"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g01_import_create_from_markdown_stores_v2() {
    let s = Stack::new("g01-import").await;
    let mut new = new_text_doc("Imported", "# From disk\n\ntext");
    new.origin = Origin::Import;
    // Imports land undetermined (decided 2026-10-15, G23); was owned before.
    new.trust = sovereign_docs::Trust::Undetermined;
    new.provenance = sovereign_docs::Provenance::Import { source_path: "C:/corpus/a.md".into() };
    let id = s.svc.create(new).await.expect("create").id;
    let (_, stored) = s.stored(&id).await;
    assert_eq!(texts(&stored), vec!["# From disk\n\ntext"]);
}
