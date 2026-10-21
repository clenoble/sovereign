//! G20 — duplicate.
//!
//! Spec ch. 03, *Document service — contract (C-DOC)*, G20 (specified
//! 2026-10-15): "**Duplicate** makes a copy of a document: its content as
//! stored, the title and thread asked for (the source's by default), its own
//! first version, ingest as for any create (G10). Its personal-data tokens stay
//! what they were: the copy is linked to every record that is linked to the
//! source and whose token is in the source's text fields — and to no other.
//! Copying never raises or lowers trust: the copy has the source's trust (a
//! copy of her own document is owned). Its provenance records the copy:
//! *copy of <source>*, with the source's provenance. The source is unchanged.
//! A trashed, purged or read-only source is refused and nothing is created.
//! Only duplicate makes a copy: `create` refuses a *copy* provenance."
//!
//! Ch. 04 (decided 2026-10-15): "What the user writes inside Sovereign from
//! scratch is owned by default: … a copy of her own document".

mod common;

use chrono::{TimeZone, Utc};
use common::*;
use sovereign_docs::{
    ChangeKind, DocError, Duplicate, Edit, EditContent, EditSession, NewContent, Origin,
    Provenance, TextField, TokenStatus, Trust,
};

fn dup(source: &str) -> Duplicate {
    Duplicate {
        source: source.into(),
        title: None,
        thread_id: None,
        origin: Origin::Editor { session: EditSession(9) },
    }
}

/// A document with text, an image and an unknown block.
async fn rich_doc(s: &Stack, title: &str) -> String {
    let mut new = new_text_doc(title, "");
    new.content = NewContent::V2(v2(vec![
        text_block(1, "first paragraph"),
        image_block(2, "/m/a.png", Some("ab")),
        unknown_block(3),
        text_block(4, "second paragraph"),
    ]));
    s.svc.create(new).await.unwrap().id
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g20_a_copy_has_the_source_content_and_its_own_first_version() {
    let s = Stack::new("g20-copy").await;
    let src = rich_doc(&s, "Source").await;
    let out = s.svc.duplicate(dup(&src)).await.expect("duplicate");
    assert_ne!(out.id, src);
    let (title, content) = s.stored(&out.id).await;
    assert_eq!(title, "Source", "the source's title by default");
    assert_eq!(assert_v2(&content), assert_v2(&s.stored(&src).await.1), "every block, in order");
    let commits = s.commits(&out.id).await;
    assert_eq!(commits.len(), 1, "its own first version");
    assert_eq!(commits[0].id_string().unwrap(), out.version);
    assert_eq!(s.stored_doc(&out.id).await.thread_id, "thread:t1", "the source's thread by default");
    assert!(s.svc.list(None).await.unwrap().iter().any(|d| d.id == out.id));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g20_title_and_thread_can_be_given() {
    let s = Stack::new("g20-names").await;
    let src = s.create_text("Source", "x").await;
    let mut d = dup(&src);
    d.title = Some("Source (copy)".into());
    d.thread_id = Some("thread:elsewhere".into());
    let id = s.svc.duplicate(d).await.unwrap().id;
    let doc = s.stored_doc(&id).await;
    assert_eq!(doc.title, "Source (copy)");
    assert_eq!(doc.thread_id, "thread:elsewhere");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g20_the_source_is_unchanged() {
    let s = Stack::new("g20-source").await;
    let src = rich_doc(&s, "Source").await;
    let r = new_record(&s.edb, &[&src]).await;
    let base = s.revision(&src).await;
    s.svc
        .save(&src, Edit {
            base,
            title: None,
            content: Some(EditContent::TextBlocks(vec![(bid(1), format!("call {}", token(&r)))])),
            origin: Origin::Editor { session: EditSession(1) },
            explicit: true,
            pasted_tokens: vec![],
        })
        .await
        .unwrap();
    let before = s.stored_doc(&src).await;
    let view = s.svc.read(&src).await.unwrap();
    let versions = s.committed_states(&src).await;
    let sources = sources_of(&s.edb, &r).await;
    s.events.take();
    s.clock.advance(chrono::Duration::hours(1));
    let copy = s.svc.duplicate(dup(&src)).await.unwrap().id;
    let after = s.stored_doc(&src).await;
    assert_eq!((after.title, after.content, after.modified_at, after.head_commit),
               (before.title, before.content, before.modified_at, before.head_commit));
    assert_eq!(s.committed_states(&src).await, versions);
    let v = s.svc.read(&src).await.unwrap();
    assert_eq!((v.trust, v.provenance, v.trust_decision), (view.trust, view.provenance, view.trust_decision));
    let mut want_sources = sources;
    want_sources.push(copy.clone());
    let mut got = sources_of(&s.edb, &r).await;
    got.sort();
    want_sources.sort();
    assert_eq!(got, want_sources, "the record gains the copy, keeps the source");
    let ev: Vec<_> = s.events.take().into_iter().map(|e| (e.id, e.kind)).collect();
    assert_eq!(ev, vec![(copy, ChangeKind::Created)], "one event, for the copy");
}

/// Every token-form occurrence of the source is, in the copy, what it was in
/// the source: tokens stay tokens, placeholders stay placeholders, ordinary
/// text stays ordinary text — in text, title and caption alike.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g20_tokens_stay_what_they_were_in_the_source() {
    let s = Stack::new("g20-tokens").await;
    let src = s.create_text("Source", "x").await;
    let other = s.create_text("Other", "o").await;
    let here = new_record(&s.edb, &[&src]).await;
    let gone = new_record(&s.edb, &[&src]).await;
    s.edb.soft_delete_pii_record(&gone).await.unwrap();
    let elsewhere = new_record(&s.edb, &[&other]).await;
    let mut img = image_block(2, "/a.png", Some("ab"));
    img["caption"] = serde_json::json!(format!("photo {}", token(&here)));
    let text = format!("{} {} {} {}", token(&here), token(&gone), token(&elsewhere), token("pii_record:ghost"));
    let base = s.revision(&src).await;
    s.svc
        .save(&src, Edit {
            base,
            title: Some(format!("about {}", token(&here))),
            content: Some(EditContent::V2(v2(vec![text_block(1, &text), img]))),
            origin: Origin::Editor { session: EditSession(1) },
            explicit: false,
            pasted_tokens: vec![],
        })
        .await
        .unwrap();
    let copy = s.svc.duplicate(dup(&src)).await.unwrap().id;
    let in_src = s.svc.read(&src).await.unwrap();
    let in_copy = s.svc.read(&copy).await.unwrap();
    assert_eq!(in_src.tokens.len(), 6);
    for t in &in_src.tokens {
        let c = in_copy
            .tokens
            .iter()
            .find(|c| c.field == t.field && c.record_id == t.record_id)
            .unwrap_or_else(|| panic!("{:?} {} missing in the copy", t.field, t.record_id));
        assert_eq!(c.status, t.status, "{:?} {}", t.field, t.record_id);
    }
    assert!(matches!(
        in_copy.tokens.iter().find(|t| t.field == TextField::Title).map(|t| &t.status),
        Some(TokenStatus::Token { .. })
    ));
    assert!(sources_of(&s.edb, &here).await.contains(&copy));
    assert!(!sources_of(&s.edb, &elsewhere).await.contains(&copy), "not linked to the source: not linked");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g20_a_record_linked_to_the_source_but_not_in_its_text_is_not_linked_to_the_copy() {
    let s = Stack::new("g20-absent").await;
    let src = s.create_text("Source", "no token here").await;
    let r = new_record(&s.edb, &[&src]).await;
    let copy = s.svc.duplicate(dup(&src)).await.unwrap().id;
    assert!(!sources_of(&s.edb, &r).await.contains(&copy));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g20_copying_never_raises_or_lowers_trust() {
    let s = Stack::new("g20-trust").await;
    let mut cases = vec![];
    // The user's own document.
    cases.push((s.create_text("Mine", "x").await, Trust::Owned));
    // External web content.
    let mut new = new_text_doc("Web", "x");
    new.trust = Trust::External;
    new.provenance = Provenance::Web { url: "https://example.org/w".into() };
    cases.push((s.svc.create(new).await.unwrap().id, Trust::External));
    // Undetermined import.
    let mut new = new_text_doc("Imported", "x");
    new.trust = Trust::Undetermined;
    new.provenance = Provenance::Import { source_path: "D:/in/a.md".into() };
    new.origin = Origin::Import;
    cases.push((s.svc.create(new).await.unwrap().id, Trust::Undetermined));
    // A web page she adopted: her own document now.
    let mut new = new_text_doc("Adopted", "x");
    new.trust = Trust::External;
    new.provenance = Provenance::Web { url: "https://example.org/a".into() };
    let adopted = s.svc.create(new).await.unwrap().id;
    s.svc.set_trust(&adopted, Trust::Owned).await.unwrap();
    cases.push((adopted, Trust::Owned));
    for (src, trust) in cases {
        let copy = s.svc.duplicate(dup(&src)).await.unwrap().id;
        assert_eq!(s.svc.read(&copy).await.unwrap().trust, trust, "copy of {src}");
        assert_eq!(s.stored_doc(&copy).await.is_owned, trust == Trust::Owned);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g20_provenance_records_the_copy_and_where_the_content_came_from() {
    let s = Stack::new("g20-provenance").await;
    let mut new = new_text_doc("Web", "x");
    new.trust = Trust::External;
    let web = Provenance::Web { url: "https://example.org/w".into() };
    new.provenance = web.clone();
    let src = s.svc.create(new).await.unwrap().id;
    let copy = s.svc.duplicate(dup(&src)).await.unwrap().id;
    let want = Provenance::Copy { of: src.clone(), from: Box::new(web.clone()) };
    assert_eq!(s.svc.read(&copy).await.unwrap().provenance, want);
    // A copy of the copy names the copy, and carries the chain.
    let copy2 = s.svc.duplicate(dup(&copy)).await.unwrap().id;
    assert_eq!(
        s.svc.read(&copy2).await.unwrap().provenance,
        Provenance::Copy { of: copy.clone(), from: Box::new(want) }
    );
    // A row from before the service: its provenance is unrecorded, and says so.
    let legacy = s.legacy("Old", "legacy body").await;
    let copy3 = s.svc.duplicate(dup(&legacy)).await.unwrap().id;
    assert_eq!(
        s.svc.read(&copy3).await.unwrap().provenance,
        Provenance::Copy { of: legacy, from: Box::new(Provenance::Unrecorded) }
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g20_a_trashed_purged_read_only_or_missing_source_is_refused() {
    let s = Stack::new("g20-refused").await;
    let trashed = s.create_text("Trashed", "x").await;
    s.svc.trash(&trashed).await.unwrap();
    let purged = s.create_text("Purged", "x").await;
    s.svc.trash(&purged).await.unwrap();
    s.svc.purge_from_trash(&purged).await.unwrap();
    let newer = s.legacy("Future", r#"{"format":"sovereign.doc","v":3,"blocks":[]}"#).await;
    let before = s.edb.list_documents_including_deleted().await.unwrap().len();
    s.events.take();
    assert!(matches!(s.svc.duplicate(dup(&trashed)).await, Err(DocError::InTrash(_))));
    assert!(matches!(s.svc.duplicate(dup(&purged)).await, Err(DocError::Purged(_))));
    assert!(matches!(s.svc.duplicate(dup(&newer)).await, Err(DocError::ReadOnly { .. })));
    assert!(matches!(s.svc.duplicate(dup("document:nosuch")).await, Err(DocError::NotFound(_))));
    assert_eq!(s.edb.list_documents_including_deleted().await.unwrap().len(), before, "nothing created");
    assert!(s.events.peek().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g20_the_copy_goes_through_ingest() {
    let s = Stack::new("g20-ingest").await;
    // A row written before the service, never scanned.
    let src = s.legacy("Old", &format!("write to {SECRET}")).await;
    s.pii.set(PiiMode::Tokenize);
    let copy = s.svc.duplicate(dup(&src)).await.unwrap().id;
    assert!(s.pii.requests().iter().any(|r| r.document_id == copy));
    assert!(!s.stored(&copy).await.1.contains(SECRET));
    assert!(s.svc.read(&copy).await.unwrap().pii_scanned);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g20_create_refuses_a_copy_provenance() {
    let s = Stack::new("g20-create").await;
    let src = s.create_text("Source", "x").await;
    let mut new = new_text_doc("Fake copy", "x");
    new.provenance = Provenance::Copy { of: src, from: Box::new(Provenance::Written) };
    new.created_at = Some(Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap());
    assert!(matches!(s.svc.create(new).await, Err(DocError::InvalidContent(_))));
    assert_eq!(s.svc.list(None).await.unwrap().len(), 1);
}
