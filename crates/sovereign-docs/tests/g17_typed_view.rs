//! G17 — readers get a typed view; no text in debug output.
//!
//! Spec ch. 03, *Document service — contract (C-DOC)*, G17: "**Readers get a
//! typed view** — the blocks, the plain text (text blocks joined by a blank
//! line, tokens in canonical form) for search, the assistant and skills, and
//! the labelled text for display — so no caller parses stored content. Lists
//! and the trash carry no content. The `Debug` form of every type that holds
//! document text prints sizes, never the text."

mod common;

use common::*;
use sovereign_docs::{
    Edit, EditContent, EditSession, NewContent, NewDocument, Origin, PastedToken, Provenance,
    RemoteDocument, Trust,
};

const SECRET_TEXT: &str = "the-unmistakable-secret-paragraph";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g17_plain_text_joins_text_blocks_and_skips_the_rest() {
    let s = Stack::new("g17-plain").await;
    let content = v2(vec![
        text_block(1, "first"),
        image_block(2, "/a.png", Some("aa")),
        unknown_block(3),
        text_block(4, "second\nline"),
    ]);
    let id = s.legacy("P", &content).await;
    let v = s.svc.read(&id).await.unwrap();
    assert_eq!(v.plain_text(), "first\n\nsecond\nline");
    assert_eq!(v.doc.as_ref().unwrap().blocks().len(), 4);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g17_the_view_carries_the_record_fields_a_reader_needs() {
    let s = Stack::new("g17-fields").await;
    let mut new = new_text_doc("Title", "x");
    new.thread_id = "thread:lane9".into();
    new.trust = Trust::Undetermined;
    new.provenance = Provenance::Import { source_path: "C:/a.md".into() };
    let id = s.svc.create(new).await.unwrap().id;
    let v = s.svc.read(&id).await.unwrap();
    assert_eq!(v.id, id);
    assert_eq!(v.title, "Title");
    assert_eq!(v.thread_id, "thread:lane9");
    assert_eq!(v.trust, Trust::Undetermined);
    assert_eq!(v.created_at, s.clock.get());
    assert!(v.trashed_at.is_none());
    assert!(!v.peer_review_pending);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g17_list_filters_by_thread_and_excludes_the_trash() {
    let s = Stack::new("g17-list").await;
    let a = s.create_text("A", "a").await;
    let mut new = new_text_doc("B", "b");
    new.thread_id = "thread:other".into();
    let b = s.svc.create(new).await.unwrap().id;
    let c = s.create_text("C", "c").await;
    s.svc.trash(&c).await.unwrap();
    let all: Vec<_> = s.svc.list(None).await.unwrap().into_iter().map(|d| d.id).collect();
    assert!(all.contains(&a) && all.contains(&b) && !all.contains(&c));
    let t1: Vec<_> = s.svc.list(Some("thread:t1")).await.unwrap().into_iter().map(|d| d.id).collect();
    assert_eq!(t1, vec![a]);
}

#[test]
fn g17_debug_of_types_holding_text_never_prints_the_text() {
    let edit = Edit {
        base: sovereign_docs::Revision("r".into()),
        title: Some(SECRET_TEXT.into()),
        content: Some(EditContent::PlainText(SECRET_TEXT.into())),
        origin: Origin::Editor { session: EditSession(1) },
        explicit: false,
        pasted_tokens: vec![PastedToken { record_id: "pii_record:x".into(), from_document: "document:a".into() }],
    };
    let new = NewDocument {
        title: SECRET_TEXT.into(),
        thread_id: "thread:t".into(),
        content: NewContent::Text(SECRET_TEXT.into()),
        trust: Trust::Owned,
        provenance: Provenance::Written,
        origin: Origin::Assistant,
        created_at: None,
        modified_at: None,
    };
    let remote = RemoteDocument {
        id: "document:r".into(),
        title: SECRET_TEXT.into(),
        content: SECRET_TEXT.into(),
        thread_id: "thread:t".into(),
        trust: None,
        provenance: None,
        modified_at: chrono::Utc::now(),
        deleted_at: None,
        device: "d".into(),
        trust_decision: None,
    };
    let req = sovereign_docs::IngestRequest {
        document_id: "document:a".into(),
        path: sovereign_docs::WritePath::Save,
        items: vec![
            sovereign_docs::IngestItem { field: sovereign_docs::TextField::Title, text: SECRET_TEXT.into() },
            sovereign_docs::IngestItem { field: sovereign_docs::TextField::Text(bid(1)), text: SECRET_TEXT.into() },
        ],
    };
    let out = sovereign_docs::IngestedItem { field: sovereign_docs::TextField::Alt(bid(2)), text: SECRET_TEXT.into() };
    for printed in [
        format!("{edit:?}"),
        format!("{:?}", EditContent::V2(SECRET_TEXT.into())),
        format!("{:?}", EditContent::TextBlocks(vec![(bid(1), SECRET_TEXT.into())])),
        format!("{new:?}"),
        format!("{:?}", NewContent::V2(SECRET_TEXT.into())),
        format!("{remote:?}"),
        format!("{req:?}"),
        format!("{out:?}"),
    ] {
        assert!(!printed.contains(SECRET_TEXT), "{printed}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g17_debug_of_a_view_never_prints_the_text() {
    let s = Stack::new("g17-view-debug").await;
    let id = s.create_text(SECRET_TEXT, SECRET_TEXT).await;
    let v = s.svc.read(&id).await.unwrap();
    assert!(!format!("{v:?}").contains(SECRET_TEXT));
    let c = s.commits(&id).await[0].id_string().unwrap();
    let p = s.svc.read_version(&id, &c).await.unwrap();
    assert!(!format!("{p:?}").contains(SECRET_TEXT));
}
