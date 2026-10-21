//! G13 — delete is trash; purge removes everything and sticks.
//!
//! Spec ch. 03, *Document service — contract (C-DOC)*, G13: "**Delete is
//! trash.** Trash keeps everything (content, versions, links, personal-data
//! sources) and hides the document from lists and pending suggestions;
//! restoring brings it back exactly. Emptying the trash, purging one item from
//! it **[choice]**, or 30 days in it, purges: the document, its versions, its
//! key, its edges and suggested links in both directions, its personal-data
//! source references — and a discovered record left with no source; a vault
//! entry is never removed by a purge. What remains is a purge marker carrying
//! no content. A purged document is never listed, read, restored, or brought
//! back by sync. A purge interrupted at any point completes when run again.
//! Nothing else is touched."
//!
//! Ch. 03, *Deletion* (decided 2026-10-10): "delete is a soft delete
//! everywhere, with a trash … recoverable for 30 days and is then purged. The
//! user can empty the trash at any time, which purges at once."

mod common;

use chrono::Duration;
use common::*;
use sovereign_db::schema::{RelationType, SuggestionSource};
use sovereign_db::traits::GraphDB;
use sovereign_docs::{DocError, RemoteDocument, RemoteOutcome, TokenStatus, Trust};

struct World {
    s: Stack,
    a: String,
    b: String,
    shared: String,
    only_a: String,
    vault_a: String,
}

/// A and B; edges and suggested links both ways; a record in both; a
/// discovered record in A only; a vault entry sourced in A only.
async fn world(tag: &str) -> World {
    let s = Stack::new(tag).await;
    let a = s.create_text("A", "alpha").await;
    let b = s.create_text("B", "beta").await;
    s.edb.create_relationship(&a, &b, RelationType::References, 0.5).await.unwrap();
    s.edb.create_relationship(&b, &a, RelationType::Supports, 0.5).await.unwrap();
    s.edb
        .create_suggested_link(&a, &b, RelationType::Continues, 0.8, "r", SuggestionSource::Consolidation)
        .await
        .unwrap();
    let c = s.create_text("C", "gamma").await;
    s.edb
        .create_suggested_link(&c, &a, RelationType::Supports, 0.8, "r", SuggestionSource::Consolidation)
        .await
        .unwrap();
    let shared = new_record(&s.edb, &[&a, &b]).await;
    let only_a = new_record(&s.edb, &[&a]).await;
    let vault_a = new_vault_record(&s.edb, &[&a]).await;
    World { s, a, b, shared, only_a, vault_a }
}

fn involves(id: &str, from: &Option<sovereign_db::schema::Thing>, to: &Option<sovereign_db::schema::Thing>) -> bool {
    let f = from.as_ref().map(sovereign_db::schema::thing_to_raw);
    let t = to.as_ref().map(sovereign_db::schema::thing_to_raw);
    f.as_deref() == Some(id) || t.as_deref() == Some(id)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g13_trash_hides_and_keeps_everything() {
    let w = world("g13-trash").await;
    let s = &w.s;
    let before = s.stored(&w.a).await;
    let commits = s.commits(&w.a).await.len();
    s.svc.trash(&w.a).await.unwrap();

    assert!(s.svc.list(None).await.unwrap().iter().all(|d| d.id != w.a), "not listed");
    let trash = s.svc.list_trash().await.unwrap();
    assert_eq!(trash.len(), 1);
    assert_eq!(trash[0].id, w.a);
    assert_eq!(trash[0].trashed_at, s.clock.get());
    let v = s.svc.read(&w.a).await.unwrap();
    assert_eq!(v.trashed_at, Some(s.clock.get()));

    assert_eq!(s.stored(&w.a).await, before, "content kept");
    assert_eq!(s.commits(&w.a).await.len(), commits, "versions kept");
    assert_eq!(sources_of(&s.edb, &w.shared).await.len(), 2, "sources kept");
    assert!(s.edb.list_outgoing_relationships(&w.a).await.unwrap().len() == 1, "edges kept");
    let pending = s.edb.list_pending_suggestions().await.unwrap();
    let _ = pending; // the store keeps them; the service hides them:
    // B is untouched and its token still resolves.
    assert!(matches!(s.svc.token_status(&w.b, &w.shared).await.unwrap(), TokenStatus::Token { .. }));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g13_trashed_documents_leave_the_pending_suggestions_until_restored() {
    let w = world("g13-suggest").await;
    let s = &w.s;
    let count = |list: Vec<sovereign_db::schema::SuggestedLink>, id: &str| {
        list.iter().filter(|l| involves(id, &l.in_, &l.out)).count()
    };
    assert_eq!(count(s.edb.list_pending_suggestions().await.unwrap(), &w.a), 2);
    s.svc.trash(&w.a).await.unwrap();
    assert_eq!(count(s.edb.list_pending_suggestions().await.unwrap(), &w.a), 0, "hidden while trashed");
    s.svc.restore_from_trash(&w.a).await.unwrap();
    assert_eq!(count(s.edb.list_pending_suggestions().await.unwrap(), &w.a), 2, "back on restore");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g13_restore_from_trash_brings_back_exactly() {
    let w = world("g13-restore").await;
    let s = &w.s;
    let mut new = new_text_doc("Ext", "external");
    new.trust = Trust::External;
    new.provenance = sovereign_docs::Provenance::Web { url: "https://e.org".into() };
    let ext = s.svc.create(new).await.unwrap().id;
    for id in [&w.a, &ext] {
        let before = s.stored_doc(id).await;
        let view = s.svc.read(id).await.unwrap();
        let commits = s.committed_states(id).await;
        s.svc.trash(id).await.unwrap();
        s.clock.advance(Duration::days(3));
        s.svc.restore_from_trash(id).await.unwrap();
        let after = s.stored_doc(id).await;
        assert_eq!((after.title, after.content, after.thread_id), (before.title, before.content, before.thread_id));
        assert_eq!(after.modified_at, before.modified_at, "timeline position kept");
        let v = s.svc.read(id).await.unwrap();
        assert_eq!((v.trust, v.provenance.clone(), v.trashed_at), (view.trust, view.provenance, None));
        assert_eq!(s.committed_states(id).await, commits);
        assert!(s.svc.list(None).await.unwrap().iter().any(|d| &d.id == id));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g13_restore_from_trash_of_a_live_document_is_refused() {
    let s = Stack::new("g13-not-trashed").await;
    let id = s.create_text("T", "x").await;
    assert!(matches!(s.svc.restore_from_trash(&id).await, Err(DocError::NotInTrash(_))));
    assert!(matches!(s.svc.purge_from_trash(&id).await, Err(DocError::NotInTrash(_))));
    assert_eq!(texts(&s.stored(&id).await.1), vec!["x"], "nothing removed");
}

/// Everything hanging off the purged document, enumerated table by table.
async fn assert_nothing_left(w: &World) {
    let s = &w.s;
    let a = &w.a;
    // document row (raw store too)
    assert!(s.raw.get_document(a).await.is_err(), "document row");
    assert!(s
        .raw
        .list_documents_including_deleted()
        .await
        .unwrap()
        .iter()
        .all(|d| d.id_string().as_deref() != Some(a.as_str())));
    // commits
    assert!(s.raw.list_document_commits(a).await.unwrap_or_default().is_empty(), "commits");
    // document key (crypto-shredding)
    assert!(!s.doc_keys.read().await.contains(a), "document key");
    // edges, both directions
    assert!(s.raw.list_all_relationships().await.unwrap().iter().all(|r| !involves(a, &r.in_, &r.out)), "edges");
    // suggested links, any status, both directions
    assert!(s.raw.list_all_suggested_links().await.unwrap().iter().all(|l| !involves(a, &l.in_, &l.out)), "suggested links");
    // personal-data sources
    for r in s.raw.list_pii_records_including_deleted().await.unwrap() {
        assert!(r.sources.iter().all(|src| &src.source_id != a), "source left on {:?}", r.id);
    }
    // a discovered record left with no source is gone (not merely soft-deleted)
    assert!(s.raw.get_pii_record(&w.only_a).await.is_err(), "orphaned discovered record");
    // the shared record keeps B; the vault entry survives
    assert_eq!(sources_of(&s.edb, &w.shared).await, vec![w.b.clone()]);
    assert!(s.raw.get_pii_record(&w.vault_a).await.is_ok(), "vault entry kept");
    // nothing listed anywhere through the service
    assert!(s.svc.list(None).await.unwrap().iter().all(|d| &d.id != a));
    assert!(s.svc.list_trash().await.unwrap().iter().all(|d| &d.id != a));
    assert!(matches!(s.svc.read(a).await, Err(DocError::Purged(_))));
    assert!(s.svc.is_purged(a).await.unwrap());
    // the neighbour is untouched
    assert_eq!(texts(&s.stored(&w.b).await.1), vec!["beta"]);
    assert_eq!(s.commits(&w.b).await.len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g13_empty_trash_purges_the_document_and_everything_hanging_off_it() {
    let w = world("g13-purge").await;
    w.s.svc.trash(&w.a).await.unwrap();
    let report = w.s.svc.empty_trash().await.unwrap();
    assert_eq!(report.documents, vec![w.a.clone()]);
    assert!(report.commits >= 1 && report.relationships == 2 && report.suggested_links == 2);
    assert_eq!(report.pii_records, 1);
    assert_nothing_left(&w).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g13_purge_from_trash_purges_only_that_document() {
    let w = world("g13-purge-one").await;
    let s = &w.s;
    let other = s.create_text("Other", "o").await;
    s.svc.trash(&w.a).await.unwrap();
    s.svc.trash(&other).await.unwrap();
    s.svc.purge_from_trash(&w.a).await.unwrap();
    assert_nothing_left(&w).await;
    assert_eq!(s.svc.list_trash().await.unwrap().iter().map(|e| e.id.clone()).collect::<Vec<_>>(), vec![other]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g13_thirty_days_in_the_trash_purges_and_not_a_day_before() {
    let w = world("g13-expiry").await;
    let s = &w.s;
    s.svc.trash(&w.a).await.unwrap();
    s.clock.advance(Duration::days(30) - Duration::seconds(1));
    let r = s.svc.purge_expired().await.unwrap();
    assert!(r.documents.is_empty());
    assert!(s.svc.read(&w.a).await.is_ok());
    s.clock.advance(Duration::seconds(1));
    let r = s.svc.purge_expired().await.unwrap();
    assert_eq!(r.documents, vec![w.a.clone()]);
    assert_nothing_left(&w).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g13_nothing_live_is_ever_purged() {
    let w = world("g13-live").await;
    let s = &w.s;
    s.clock.advance(Duration::days(400));
    s.svc.purge_expired().await.unwrap();
    s.svc.empty_trash().await.unwrap();
    for id in [&w.a, &w.b] {
        assert!(s.svc.read(id).await.is_ok());
    }
    assert_eq!(sources_of(&s.edb, &w.shared).await.len(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g13_a_purged_document_is_never_restored_or_brought_back_by_sync() {
    let w = world("g13-sticks").await;
    let s = &w.s;
    let v1 = s.commits(&w.a).await[0].id_string().unwrap();
    s.svc.trash(&w.a).await.unwrap();
    s.svc.empty_trash().await.unwrap();
    assert!(matches!(s.svc.restore_from_trash(&w.a).await, Err(DocError::Purged(_))));
    assert!(matches!(s.svc.restore_version(&w.a, &v1).await, Err(DocError::Purged(_))));
    let base = sovereign_docs::Revision("anything".into());
    assert!(matches!(s.svc.save(&w.a, sovereign_docs::Edit {
        base,
        title: None,
        content: Some(sovereign_docs::EditContent::PlainText("x".into())),
        origin: sovereign_docs::Origin::Assistant,
        explicit: false,
        pasted_tokens: vec![],
    }).await, Err(DocError::Purged(_))));
    // A paired device that still holds A live sends it.
    let out = s
        .svc
        .apply_remote(RemoteDocument {
            id: w.a.clone(),
            title: "A".into(),
            content: v2(vec![text_block(1, "alpha from the phone")]),
            thread_id: "thread:t1".into(),
            trust: Some(Trust::Owned),
            provenance: None,
            modified_at: s.clock.get() + Duration::days(1),
            deleted_at: None,
            device: "phone".into(),
            trust_decision: None,
        })
        .await
        .unwrap();
    assert_eq!(out, RemoteOutcome::IgnoredPurged);
    assert_nothing_left(&w).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g13_the_purge_marker_carries_no_content() {
    // Whatever stays to make the purge stick holds neither title nor text:
    // no row of the raw store mentions them.
    let s = Stack::new("g13-marker").await;
    let id = s.create_text("Very private title", "very private body").await;
    s.svc.trash(&id).await.unwrap();
    s.svc.empty_trash().await.unwrap();
    for d in s.raw.list_documents_including_deleted().await.unwrap() {
        assert!(!d.title.contains("Very private") && !d.content.contains("very private"));
        assert!(d.id_string().as_deref() != Some(id.as_str()) || (d.title.is_empty() && d.content.is_empty()));
    }
    assert!(s.raw.list_document_commits(&id).await.unwrap_or_default().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g13_an_interrupted_purge_completes_when_run_again() {
    let mut completed = false;
    for k in 0..400 {
        let mut w = world(&format!("g13-purge-crash-{k}")).await;
        w.s.svc.trash(&w.a).await.unwrap();
        w.s.faulty.crash_after(k);
        let _ = w.s.svc.empty_trash().await;
        let crashed = w.s.faulty.crashed();
        w.s.restart();
        // B must be intact whatever happened.
        assert_eq!(texts(&w.s.stored(&w.b).await.1), vec!["beta"], "k={k}");
        // A is not listed among live documents, whatever happened.
        assert!(w.s.svc.list(None).await.unwrap().iter().all(|d| d.id != w.a), "k={k}");
        // Running the purge again finishes the job.
        w.s.svc.empty_trash().await.expect("second run");
        assert_nothing_left(&w).await;
        if !crashed {
            completed = true;
            break;
        }
    }
    assert!(completed);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g13_delete_never_removes_a_row_before_the_purge() {
    let s = Stack::new("g13-soft").await;
    let id = s.create_text("T", "x").await;
    s.svc.trash(&id).await.unwrap();
    let raw = s.raw.get_document(&id).await.expect("row still there");
    assert!(raw.deleted_at.is_some(), "a tombstone sync can carry (ch. 05)");
}
