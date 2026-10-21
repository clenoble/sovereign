//! G15 — the resolver rule.
//!
//! Spec ch. 03, *Document service — contract (C-DOC)*, G15: "**The resolver
//! rule.** Text in token form `[pii:<record_id>]` is a token only in a document
//! listed among its record's sources; there, a record that is deleted or cannot
//! be read shows as a placeholder. Anywhere else — another document, an
//! unknown record — it is ordinary text."
//!
//! Ch. 09, *Where a token resolves* (decided 2026-10-10): "A token resolves
//! only in a document or message listed among its record's sources. Anywhere
//! else, text in token form is ordinary text."

mod common;

use common::*;
use sovereign_db::traits::GraphDB;
use sovereign_docs::{TextField, TokenStatus};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g15_a_token_is_a_token_only_where_its_record_is_linked() {
    let s = Stack::new("g15-where").await;
    let a = s.create_text("A", "x").await;
    let b = s.create_text("B", "y").await;
    let r = new_record(&s.edb, &[&a]).await;
    assert!(matches!(s.svc.token_status(&a, &r).await.unwrap(), TokenStatus::Token { .. }));
    assert_eq!(s.svc.token_status(&b, &r).await.unwrap(), TokenStatus::NotAToken);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g15_an_unknown_record_is_ordinary_text() {
    let s = Stack::new("g15-unknown").await;
    let a = s.create_text("A", "x").await;
    for rid in ["pii_record:nosuch", "document:notarecord", "", "pii_record:"] {
        assert_eq!(s.svc.token_status(&a, rid).await.unwrap(), TokenStatus::NotAToken, "{rid:?}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g15_a_deleted_record_linked_here_is_a_placeholder_and_elsewhere_nothing() {
    let s = Stack::new("g15-deleted").await;
    let a = s.create_text("A", "x").await;
    let b = s.create_text("B", "y").await;
    let r = new_record(&s.edb, &[&a]).await;
    s.edb.soft_delete_pii_record(&r).await.unwrap();
    assert_eq!(s.svc.token_status(&a, &r).await.unwrap(), TokenStatus::Placeholder);
    assert_eq!(s.svc.token_status(&b, &r).await.unwrap(), TokenStatus::NotAToken);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g15_the_view_reports_each_occurrence_with_the_rule_applied() {
    let s = Stack::new("g15-view").await;
    let a = s.create_text("A", "x").await;
    let other = s.create_text("Other", "o").await;
    let here = new_record(&s.edb, &[&a]).await;
    let elsewhere = new_record(&s.edb, &[&other]).await;
    let gone = new_record(&s.edb, &[&a]).await;
    s.edb.soft_delete_pii_record(&gone).await.unwrap();
    let text = format!("{} {} {} {}", token(&here), token(&elsewhere), token(&gone), token("pii_record:ghost"));
    let id = s.legacy("V", &v2(vec![text_block(1, &text)])).await;
    // Link `here` and `gone` to this legacy document too.
    let mut r = s.edb.get_pii_record(&here).await.unwrap();
    r.sources[0].source_id = id.clone();
    s.edb.update_pii_record_sources(&here, r.sources).await.unwrap();
    let mut g = s.edb.get_pii_record(&gone).await.unwrap();
    g.sources[0].source_id = id.clone();
    s.edb.update_pii_record_sources(&gone, g.sources).await.unwrap();

    let v = s.svc.read(&id).await.unwrap();
    let status = |rid: &str| v.tokens.iter().find(|t| t.record_id == rid).map(|t| t.status.clone());
    assert!(matches!(status(&here), Some(TokenStatus::Token { .. })));
    assert_eq!(status(&elsewhere), Some(TokenStatus::NotAToken));
    assert_eq!(status(&gone), Some(TokenStatus::Placeholder));
    assert_eq!(status("pii_record:ghost"), Some(TokenStatus::NotAToken));
    assert!(v.tokens.iter().all(|t| t.field == TextField::Text(bid(1))));

    let labelled = v.labelled_text();
    assert!(!labelled.contains(&token(&here)), "a real token shows as its label");
    assert!(!labelled.contains(&token(&gone)), "a placeholder replaces it");
    assert!(labelled.contains(&token(&elsewhere)), "not a token here: shown as typed");
    assert!(labelled.contains(&token("pii_record:ghost")));
    assert_eq!(v.plain_text(), text, "plain text keeps the canonical form");
}

/// Titles and captions go through ingest (decided 2026-10-11), so tokens can
/// be in them; the rule is the same there.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g15_tokens_in_titles_and_captions_follow_the_same_rule() {
    let s = Stack::new("g15-title").await;
    let id = s.create_text("placeholder", "x").await;
    let other = s.create_text("Other", "o").await;
    let here = new_record(&s.edb, &[&id]).await;
    let elsewhere = new_record(&s.edb, &[&other]).await;
    let mut img = image_block(2, "/a.png", Some("ab"));
    img["caption"] = serde_json::json!(format!("photo of {}", token(&here)));
    let base = s.revision(&id).await;
    s.svc
        .save(&id, sovereign_docs::Edit {
            base,
            title: Some(format!("{} and {}", token(&here), token(&elsewhere))),
            content: Some(sovereign_docs::EditContent::V2(v2(vec![text_block(1, "x"), img]))),
            origin: sovereign_docs::Origin::Editor { session: sovereign_docs::EditSession(1) },
            explicit: false,
            pasted_tokens: vec![],
        })
        .await
        .unwrap();
    let v = s.svc.read(&id).await.unwrap();
    let at = |f: TextField, rid: &str| {
        v.tokens.iter().find(|t| t.field == f && t.record_id == rid).map(|t| t.status.clone())
    };
    assert!(matches!(at(TextField::Title, &here), Some(TokenStatus::Token { .. })));
    assert_eq!(at(TextField::Title, &elsewhere), Some(TokenStatus::NotAToken));
    assert!(matches!(at(TextField::Caption(bid(2)), &here), Some(TokenStatus::Token { .. })));
    let title = v.labelled_title();
    assert!(!title.contains(&token(&here)));
    assert!(title.contains(&token(&elsewhere)));
    assert_eq!(v.title, format!("{} and {}", token(&here), token(&elsewhere)), "title holds the canonical form");
}
