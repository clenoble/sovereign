//! G2 — every stored form is read; newer and unreadable forms are read-only (C-ENC).
//!
//! Spec ch. 03, *Document service — contract (C-DOC)*, G2: "**Every stored form
//! is read.** The v1 envelope (with or without media), a raw-text body, the
//! empty string, and anything that is neither v2 nor a v1 envelope read as v2
//! blocks; a non-empty stored string never reads as an empty document. A v2
//! document of a newer `v`, or one claiming `sovereign.doc` that cannot be
//! read, is shown read-only and never rewritten by any operation."
//!
//! And ch. 03, *Document format v2 — blocks*: "The v1 envelope … → one `text`
//! block holding `body`, then one `image` / `video` block per entry, in that
//! order … the migrated image block carries an empty `alt` flagged *needs a
//! description*, and `ref.sha256` stays absent until the document service
//! computes it."

mod common;

use common::*;
use sovereign_docs::{
    DocError, DocumentV2, EditContent, ReadOnlyReason, SourceForm, StoredForm, Writable,
};

async fn view_texts(s: &Stack, id: &str) -> Vec<String> {
    let v = s.svc.read(id).await.expect("read");
    let doc = v.doc.as_ref().expect("readable");
    doc.blocks().iter().filter_map(|b| b.text().map(str::to_string)).collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g02_v1_envelope_with_media_reads_as_text_then_media_in_order() {
    let s = Stack::new("g02-env-media").await;
    let env = r#"{"body":"the body","images":[{"path":"/m/a.png","caption":"A"},{"path":"/m/b.png","caption":"B"}],"videos":[{"path":"/m/v.mp4","caption":"V","duration_secs":12.5}]}"#;
    let id = s.legacy("Env", env).await;
    let v = s.svc.read(&id).await.expect("read");
    assert_eq!(v.stored_form, StoredForm::Read(SourceForm::V1Envelope));
    assert_eq!(v.writable, Writable::Yes);
    let doc = v.doc.as_ref().unwrap();
    let kinds: Vec<&str> = doc.blocks().iter().map(|b| b.type_name()).collect();
    assert_eq!(kinds, vec!["text", "image", "image", "video"]);
    let b = doc.blocks();
    assert_eq!(b[0].text(), Some("the body"));
    assert_eq!(b[1].media_path(), Some("/m/a.png"));
    assert_eq!(b[2].media_path(), Some("/m/b.png"));
    assert_eq!(b[3].media_path(), Some("/m/v.mp4"));
    for img in &b[1..3] {
        assert_eq!(img.image_alt(), Some(""), "migrated image: empty alt");
        assert!(img.needs_description(), "migrated image: flagged needs a description");
        assert_eq!(img.media_sha256(), None, "no hash until the service computes it");
    }
    assert_eq!(v.plain_text(), "the body");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g02_v1_envelope_without_media_reads_as_one_text_block() {
    let s = Stack::new("g02-env").await;
    for env in [r#"{"body":"only body","images":[]}"#, r#"{"body":"only body"}"#] {
        let id = s.legacy("Env", env).await;
        assert_eq!(view_texts(&s, &id).await, vec!["only body"], "{env}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g02_raw_text_reads_whole_as_one_text_block() {
    let s = Stack::new("g02-raw").await;
    let raw = "# Title\n\n- a\n- b\n\n{\"body\": \"json-looking line\"}\n";
    let id = s.legacy("Raw", raw).await;
    let v = s.svc.read(&id).await.unwrap();
    assert_eq!(v.stored_form, StoredForm::Read(SourceForm::RawText));
    assert_eq!(view_texts(&s, &id).await, vec![raw]);
    assert_eq!(v.plain_text(), raw);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g02_empty_reads_as_an_empty_writable_document() {
    let s = Stack::new("g02-empty").await;
    let id = s.legacy("Empty", "").await;
    let v = s.svc.read(&id).await.unwrap();
    assert_eq!(v.stored_form, StoredForm::Read(SourceForm::Empty));
    assert_eq!(v.writable, Writable::Yes);
    assert_eq!(v.plain_text(), "");
}

/// Invariant: a non-empty stored string never reads as an empty document,
/// unless it is an envelope whose body is empty. Invalid JSON and JSON that
/// is not an envelope are raw text, kept whole.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g02_anything_not_v2_or_envelope_is_raw_text_kept_whole() {
    let s = Stack::new("g02-raw-whole").await;
    let samples = [
        "not json",
        "   ",
        "{",
        "{}",
        r#"{"name": "x"}"#,
        r#"{"body": 42}"#,
        r#"{"body": null, "images": []}"#,
        r#"{"body":"truncated"#,
        "[1, 2, 3]",
        r#""a json string""#,
        "123",
        "null",
        r#"{"format":"other.doc","v":2,"blocks":[]}"#,
    ];
    for raw in samples {
        let id = s.legacy("Sample", raw).await;
        let v = s.svc.read(&id).await.expect("read");
        assert_eq!(v.stored_form, StoredForm::Read(SourceForm::RawText), "{raw:?}");
        assert_eq!(v.plain_text(), raw, "{raw:?} must survive whole");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g02_v2_reads_its_blocks_with_their_own_ids() {
    let s = Stack::new("g02-v2").await;
    let content = v2(vec![text_block(1, "a"), unknown_block(2), text_block(3, "b")]);
    let id = s.legacy("V2", &content).await;
    let v = s.svc.read(&id).await.unwrap();
    assert_eq!(v.stored_form, StoredForm::Read(SourceForm::V2));
    let doc = v.doc.as_ref().unwrap();
    let ids: Vec<_> = doc.blocks().iter().map(|b| b.id()).collect();
    assert_eq!(ids, vec![bid(1), bid(2), bid(3)]);
    assert!(!doc.blocks()[1].is_known());
    assert_eq!(v.plain_text(), "a\n\nb");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g02_newer_version_is_read_only_and_never_rewritten() {
    let s = Stack::new("g02-newer").await;
    let v3 = r#"{"format":"sovereign.doc","v":3,"blocks":[{"type":"text","id":"00000000000000000000000000001001","markup":"markdown","text":"from the future"}],"extra":{"k":1}}"#;
    let id = s.legacy("Future", v3).await;
    let v = s.svc.read(&id).await.unwrap();
    assert_eq!(v.writable, Writable::ReadOnly(ReadOnlyReason::NewerFormat { v: 3 }));
    let base = v.revision.clone();
    let commits = s.commits(&id).await.len();

    let r = s.assistant_save(&id, "overwrite").await;
    assert!(matches!(r, Err(DocError::ReadOnly { .. })), "save refused");
    let r = s
        .svc
        .apply_skill_output(&id, base, "redactor", EditContent::PlainText("x".into()))
        .await;
    assert!(matches!(r, Err(DocError::ReadOnly { .. })), "skill refused");
    let report = s.svc.migrate_all().await.unwrap();
    assert!(report.read_only.contains(&id), "migration leaves it, and says so");

    assert_eq!(s.stored(&id).await.1, v3, "stored bytes unchanged");
    assert_eq!(s.commits(&id).await.len(), commits, "no version made by a refused write");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g02_unreadable_sovereign_doc_is_read_only_and_never_rewritten() {
    let s = Stack::new("g02-unreadable").await;
    for bad in [
        r#"{"format":"sovereign.doc","v":2,"blocks":"oops"}"#,
        r#"{"format":"sovereign.doc","blocks":[]}"#,
        r#"{"format":"sovereign.doc","v":1,"blocks":[]}"#,
        r#"{"format":"sovereign.doc","v":"2","blocks":[]}"#,
    ] {
        let id = s.legacy("Bad", bad).await;
        let v = s.svc.read(&id).await.expect("an unreadable document can still be opened");
        assert!(
            matches!(v.writable, Writable::ReadOnly(ReadOnlyReason::Unreadable { .. })),
            "{bad}"
        );
        assert_eq!(v.stored_form, StoredForm::Unreadable);
        assert!(v.doc.is_none());
        let r = s.assistant_save(&id, "x").await;
        assert!(matches!(r, Err(DocError::ReadOnly { .. })), "{bad}");
        s.svc.migrate_all().await.unwrap();
        assert_eq!(s.stored(&id).await.1, bad, "never rewritten: {bad}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g02_a_read_only_document_can_still_go_to_the_trash_and_back() {
    let s = Stack::new("g02-ro-trash").await;
    let v3 = r#"{"format":"sovereign.doc","v":3,"blocks":[]}"#;
    let id = s.legacy("Future", v3).await;
    s.svc.trash(&id).await.expect("trash");
    s.svc.restore_from_trash(&id).await.expect("restore");
    assert_eq!(s.stored(&id).await.1, v3);
}

/// Reading is not a write: no migration, no version, no event on read.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g02_reading_a_legacy_document_writes_nothing() {
    let s = Stack::new("g02-read-pure").await;
    let env = r#"{"body":"x","images":[{"path":"/i.png","caption":""}]}"#;
    let id = s.legacy("Env", env).await;
    s.media.put("/i.png", "ab");
    let before = s.stored_doc(&id).await;
    for _ in 0..3 {
        s.svc.read(&id).await.unwrap();
        s.svc.list(None).await.unwrap();
        s.svc.versions(&id).await.unwrap();
    }
    let after = s.stored_doc(&id).await;
    assert_eq!(after.content, env);
    assert_eq!(after.modified_at, before.modified_at);
    assert!(s.commits(&id).await.is_empty());
    assert!(s.events.peek().is_empty());
    assert!(s.pii.requests().is_empty());
}

// ------------------------------------------------- the format, directly --

/// The model the service writes migrations with is deterministic per seed.
#[test]
fn g02_format_seeded_migration_is_deterministic_and_seed_dependent() {
    let env = r#"{"body":"b","images":[{"path":"/a.png","caption":"c"}]}"#;
    for stored in [env, "raw text", ""] {
        let a = DocumentV2::read_seeded(stored, "document:x").unwrap().0.to_json_string();
        let b = DocumentV2::read_seeded(stored, "document:x").unwrap().0.to_json_string();
        assert_eq!(a, b, "same seed, same bytes: {stored:?}");
    }
    let a = DocumentV2::read_seeded(env, "document:x").unwrap().0.to_json_string();
    let c = DocumentV2::read_seeded(env, "document:y").unwrap().0.to_json_string();
    assert_ne!(a, c, "block ids depend on the document");
}

/// A v2 document keeps its own ids and bytes whatever the seed.
#[test]
fn g02_format_v2_is_read_the_same_with_or_without_a_seed() {
    let content = v2(vec![text_block(1, "a"), unknown_block(2)]);
    let a = DocumentV2::read(&content).unwrap().0.to_json_string();
    let b = DocumentV2::read_seeded(&content, "document:z").unwrap().0.to_json_string();
    assert_eq!(a, content);
    assert_eq!(b, content);
}
