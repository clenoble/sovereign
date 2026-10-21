//! G7 — skill output, and text-only writes never touch other blocks.
//!
//! Spec ch. 03, *Document service — contract (C-DOC)*, G7: "**A skill's
//! output** is applied after a version of the prior state and becomes a
//! version named after the skill. A text-only write (plain text, or text by
//! block id) never removes, reorders or alters a non-text block; plain text is
//! refused for a document with more than one text block, and text for an id
//! that is not a text block of the document is refused."

mod common;

use common::*;
use sovereign_docs::{DocError, EditContent};

async fn skill(s: &Stack, id: &str, out: EditContent) -> Result<sovereign_docs::SaveOutcome, DocError> {
    let base = s.revision(id).await;
    s.svc.apply_skill_output(id, base, "markdown_editor", out).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g07_skill_output_versions_before_and_after_and_names_the_skill() {
    let s = Stack::new("g07-versions").await;
    let id = s.create_text("T", "x").await;
    s.editor_save(&id, 1, v2(vec![text_block(1, "user text")])).await.unwrap();
    let before = s.stored(&id).await;
    let out = skill(&s, &id, EditContent::PlainText("skill text".into())).await.unwrap();
    let commits = s.commits(&id).await;
    assert_eq!(texts(&commits[0].snapshot.content), vec!["skill text"]);
    assert!(commits[0].message.contains("markdown_editor"), "named after the skill");
    assert_eq!(out.version.as_deref(), commits[0].id_string().as_deref());
    assert!(s.committed_states(&id).await.contains(&before), "prior state kept");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g07_plain_text_on_a_structured_document_keeps_every_other_block() {
    let s = Stack::new("g07-structured").await;
    let content = v2(vec![
        image_block(1, "/a.png", Some("aa")),
        text_block(2, "old"),
        unknown_block(3),
    ]);
    let id = s.legacy("S", &content).await;
    let before = assert_v2(&content);
    skill(&s, &id, EditContent::PlainText("new".into())).await.unwrap();
    let after = assert_v2(&s.stored(&id).await.1);
    assert_eq!(after.len(), 3);
    assert_eq!(after[0], before[0], "image unchanged, in place");
    assert_eq!(after[1]["id"], before[1]["id"], "the text block keeps its id");
    assert_eq!(after[1]["text"], "new");
    assert_eq!(after[2], before[2], "unknown block unchanged, in place");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g07_plain_text_on_a_document_without_text_adds_one_text_block_first() {
    let s = Stack::new("g07-no-text").await;
    let content = v2(vec![image_block(1, "/a.png", Some("aa"))]);
    let id = s.legacy("I", &content).await;
    skill(&s, &id, EditContent::PlainText("caption text".into())).await.unwrap();
    let after = assert_v2(&s.stored(&id).await.1);
    assert_eq!(after.len(), 2);
    assert_eq!(after[0]["type"], "text");
    assert_eq!(after[1], assert_v2(&content)[0]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g07_plain_text_on_several_text_blocks_is_refused_and_writes_nothing() {
    let s = Stack::new("g07-ambiguous").await;
    let content = v2(vec![text_block(1, "one"), image_block(2, "/a.png", None), text_block(3, "two")]);
    let id = s.legacy("A", &content).await;
    let commits = s.commits(&id).await.len();
    let r = skill(&s, &id, EditContent::PlainText("collapse".into())).await;
    assert!(matches!(r, Err(DocError::AmbiguousText { text_blocks: 2 })));
    let r = s.assistant_save(&id, "collapse").await;
    assert!(matches!(r, Err(DocError::AmbiguousText { .. })), "same rule for the assistant");
    assert_eq!(s.stored(&id).await.1, content);
    assert_eq!(s.commits(&id).await.len(), commits);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g07_text_by_block_id_changes_only_those_blocks() {
    let s = Stack::new("g07-by-id").await;
    let content = v2(vec![text_block(1, "one"), unknown_block(2), text_block(3, "two")]);
    let id = s.legacy("B", &content).await;
    skill(&s, &id, EditContent::TextBlocks(vec![(bid(3), "TWO".into())])).await.unwrap();
    let after = assert_v2(&s.stored(&id).await.1);
    let before = assert_v2(&content);
    assert_eq!(after[0], before[0]);
    assert_eq!(after[1], before[1]);
    assert_eq!(after[2]["text"], "TWO");
    assert_eq!(after[2]["id"], before[2]["id"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g07_text_for_a_block_that_is_not_text_is_refused() {
    let s = Stack::new("g07-bad-id").await;
    let content = v2(vec![text_block(1, "one"), image_block(2, "/a.png", None), unknown_block(3)]);
    let id = s.legacy("B", &content).await;
    for target in [bid(2), bid(3), bid(77)] {
        let r = skill(&s, &id, EditContent::TextBlocks(vec![(target, "x".into())])).await;
        assert!(matches!(r, Err(DocError::NoSuchTextBlock(b)) if b == target), "{target:?}");
    }
    assert_eq!(s.stored(&id).await.1, content);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g07_skill_output_on_a_legacy_raw_document_does_not_run_on_an_empty_body() {
    // Top-1 of the roadmap: a raw body parsed as empty, the skill ran on "" and
    // saved over the real body. Here the skill output replaces the text, and
    // the raw body is kept as a version.
    let s = Stack::new("g07-raw").await;
    let id = s.legacy("Raw", "the real body, written raw by the shell").await;
    let v = s.svc.read(&id).await.unwrap();
    assert_eq!(v.plain_text(), "the real body, written raw by the shell", "what the skill reads");
    skill(&s, &id, EditContent::PlainText("transformed".into())).await.unwrap();
    assert!(s
        .committed_states(&id)
        .await
        .iter()
        .any(|(_, c)| c == "the real body, written raw by the shell"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g07_skill_output_from_a_stale_base_is_refused() {
    let s = Stack::new("g07-stale").await;
    let id = s.create_text("T", "x").await;
    let stale = s.revision(&id).await;
    s.editor_save(&id, 1, v2(vec![text_block(1, "typed meanwhile")])).await.unwrap();
    let r = s
        .svc
        .apply_skill_output(&id, stale, "redactor", EditContent::PlainText("skill".into()))
        .await;
    assert!(matches!(r, Err(DocError::Conflict { .. })));
    assert_eq!(texts(&s.stored(&id).await.1), vec!["typed meanwhile"]);
}
