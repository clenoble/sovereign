//! G14 — a token is linked to a document only by a Sovereign paste.
//!
//! Spec ch. 03, *Document service — contract (C-DOC)*, G14: "**Token links come
//! from a Sovereign paste only.** A save adds the document to a record's
//! sources only when it comes from the editor, names the record among its
//! pasted tokens, the record is linked to the document the token was copied
//! from, and the token is in the saved text. Token text typed by hand, or
//! written by the assistant, a skill, import or sync, never links; a pasted
//! token naming no record creates nothing."
//!
//! Ch. 09, *Where a token resolves*: "Copying a token from one Sovereign
//! document to another keeps it a token: saving the target document adds it to
//! the record's sources. A paste from outside Sovereign never does."

mod common;

use common::*;
use sovereign_docs::{
    Edit, EditContent, EditSession, NewContent, Origin, PastedToken, Provenance, RemoteDocument,
    TokenStatus, Trust,
};

async fn setup(tag: &str) -> (Stack, String, String, String) {
    let s = Stack::new(tag).await;
    let a = s.create_text("A", "source").await;
    let b = s.create_text("B", "target").await;
    let r = new_record(&s.edb, &[&a]).await;
    (s, a, b, r)
}

async fn save_in(s: &Stack, doc: &str, text: &str, origin: Origin, pasted: Vec<PastedToken>) {
    let base = s.revision(doc).await;
    s.svc
        .save(doc, Edit {
            base,
            title: None,
            content: Some(EditContent::V2(v2(vec![text_block(1, text)]))),
            origin,
            explicit: false,
            pasted_tokens: pasted,
        })
        .await
        .expect("save");
}

fn editor() -> Origin {
    Origin::Editor { session: EditSession(1) }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g14_a_paste_from_a_linked_document_links_the_target_on_save() {
    let (s, a, b, r) = setup("g14-paste").await;
    let pasted = vec![PastedToken { record_id: r.clone(), from_document: a.clone() }];
    save_in(&s, &b, &format!("call {}", token(&r)), editor(), pasted).await;
    let src = sources_of(&s.edb, &r).await;
    assert!(src.contains(&b) && src.contains(&a));
    assert!(matches!(s.svc.token_status(&b, &r).await.unwrap(), TokenStatus::Token { .. }));
    // Saving again does not add it twice.
    save_in(&s, &b, &format!("call {} now", token(&r)), editor(), vec![
        PastedToken { record_id: r.clone(), from_document: a.clone() },
    ])
    .await;
    assert_eq!(sources_of(&s.edb, &r).await.iter().filter(|d| **d == b).count(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g14_the_same_token_typed_by_hand_links_nothing() {
    let (s, _a, b, r) = setup("g14-typed").await;
    save_in(&s, &b, &format!("typed {}", token(&r)), editor(), vec![]).await;
    assert!(!sources_of(&s.edb, &r).await.contains(&b));
    assert_eq!(s.svc.token_status(&b, &r).await.unwrap(), TokenStatus::NotAToken);
    // Stored exactly as typed.
    assert_eq!(texts(&s.stored(&b).await.1), vec![format!("typed {}", token(&r))]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g14_a_pasted_token_not_in_the_saved_text_links_nothing() {
    let (s, a, b, r) = setup("g14-absent").await;
    let pasted = vec![PastedToken { record_id: r.clone(), from_document: a }];
    save_in(&s, &b, "the paste was deleted before saving", editor(), pasted).await;
    assert!(!sources_of(&s.edb, &r).await.contains(&b));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g14_a_paste_claiming_a_document_the_record_is_not_linked_to_links_nothing() {
    let (s, _a, b, r) = setup("g14-wrong-source").await;
    let c = s.create_text("C", "never linked").await;
    let pasted = vec![PastedToken { record_id: r.clone(), from_document: c }];
    save_in(&s, &b, &format!("x {}", token(&r)), editor(), pasted).await;
    assert!(!sources_of(&s.edb, &r).await.contains(&b));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g14_only_the_editor_can_carry_a_paste() {
    let (s, a, b, r) = setup("g14-origins").await;
    let text = format!("x {}", token(&r));
    for origin in [Origin::Assistant, Origin::Skill { name: "find_replace".into() }, Origin::Import] {
        let pasted = vec![PastedToken { record_id: r.clone(), from_document: a.clone() }];
        save_in(&s, &b, &format!("{text} {origin:?}"), origin.clone(), pasted).await;
        assert!(!sources_of(&s.edb, &r).await.contains(&b), "{origin:?} linked a record");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g14_a_pasted_token_naming_no_record_creates_nothing() {
    let (s, a, b, _r) = setup("g14-ghost").await;
    let ghost = "pii_record:doesnotexist";
    let before = s.edb.list_pii_records_including_deleted().await.unwrap().len();
    let pasted = vec![PastedToken { record_id: ghost.into(), from_document: a }];
    save_in(&s, &b, &format!("x {}", token(ghost)), editor(), pasted).await;
    assert_eq!(s.edb.list_pii_records_including_deleted().await.unwrap().len(), before);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g14_create_import_and_sync_never_link_tokens_in_their_content() {
    let (s, _a, _b, r) = setup("g14-create").await;
    let text = format!("contains {}", token(&r));
    let mut new = new_text_doc("Created", &text);
    new.origin = Origin::Import;
    new.trust = Trust::Undetermined;
    new.provenance = Provenance::Import { source_path: "C:/a.md".into() };
    let imported = s.svc.create(new).await.unwrap().id;
    let mut new = new_text_doc("Assistant", "");
    new.content = NewContent::Text(text.clone());
    new.origin = Origin::Assistant;
    new.provenance = Provenance::Assistant;
    let by_assistant = s.svc.create(new).await.unwrap().id;
    s.svc
        .apply_remote(RemoteDocument {
            id: "document:synced".into(),
            title: "Synced".into(),
            content: v2(vec![text_block(1, &text)]),
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
    let src = sources_of(&s.edb, &r).await;
    for d in [imported, by_assistant, "document:synced".to_string()] {
        assert!(!src.contains(&d), "{d} linked by its content");
        assert_eq!(s.svc.token_status(&d, &r).await.unwrap(), TokenStatus::NotAToken);
    }
}
