//! C-WIRE guarantees 1, 3–9 — documents between two devices.
//!
//! Spec ch. 05, *Sync wire — contract (C-WIRE)* (draft 2026-10-15, decided
//! with Céline the same day). Each test names its guarantee. Two devices over
//! the real sealed, signed envelopes, without the network (`wire_common`).
//! Guarantee 2 (a missing field never overwrites a known one) is tested where
//! it lives, in the document service: `sovereign-docs/tests/g24_wire_fields.rs`.

mod wire_common;

use chrono::{Duration, TimeZone, Utc};
use sovereign_db::GraphDB;
use sovereign_docs::{Origin, Provenance, Trust, TrustDecision};
use wire_common::*;

/// 1. "Everything that defines a document travels: title, thread, content (v2;
/// a newer format verbatim), trash state, trust, the last trust decision with
/// its stamp, provenance (copies included), creation time, stamps."
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wire01_everything_that_defines_a_document_travels() {
    let (a, b) = (device("w1-a", 1).await, device("w1-b", 2).await);
    pair(&a, &b);
    let created = Utc.with_ymd_and_hms(2017, 5, 6, 7, 8, 9).unwrap();
    let new = sovereign_docs::NewDocument {
        title: "Imported".into(),
        thread_id: "thread:lane".into(),
        content: sovereign_docs::NewContent::Text("body".into()),
        trust: Trust::Undetermined,
        provenance: Provenance::Import { source_path: "D:/in/a.md".into() },
        origin: Origin::Import,
        created_at: Some(created),
        modified_at: Some(created),
    };
    let id = a.docs.create(new).await.unwrap().id;
    let other = create(&a, "Mine", "x").await;
    a.docs.set_trust(&other, Trust::External).await.unwrap();
    let copy = a
        .docs
        .duplicate(sovereign_docs::Duplicate { source: id.clone(), title: None, thread_id: None, origin: Origin::Editor { session: sovereign_docs::EditSession(2) } })
        .await
        .unwrap()
        .id;
    let trashed = create(&a, "Trashed", "t").await;
    a.docs.trash(&trashed).await.unwrap();
    settle(&a, &b, 4).await;
    for doc in [&id, &other, &copy, &trashed] {
        let (sa, sb) = (a.docs.stored_state(doc).await.unwrap(), b.docs.stored_state(doc).await.unwrap());
        assert_eq!(sa.normalized_hash, sb.normalized_hash, "{doc}: same state on both");
        let (va, vb) = (a.docs.read(doc).await.unwrap(), b.docs.read(doc).await.unwrap());
        assert_eq!((va.trust, &va.trust_decision, &va.provenance), (vb.trust, &vb.trust_decision, &vb.provenance), "{doc}");
        assert_eq!((va.created_at, &va.thread_id, va.trashed_at.is_some()), (vb.created_at, &vb.thread_id, vb.trashed_at.is_some()), "{doc}");
        assert_eq!(sa.meta.stamp, sb.meta.stamp, "{doc}: the stamp travels");
    }
    assert_eq!(b.docs.read(&id).await.unwrap().trust, Trust::Undetermined, "an undetermined import stays undetermined");
    assert_eq!(b.docs.read(&id).await.unwrap().created_at, created);
}

/// 3. "Equal means equal after normalisation: two devices holding the same
/// document — one migrated to v2, one still legacy — compare equal and exchange
/// nothing; a title-only edit, a thread move, a trust decision or a trash
/// change makes them unequal."
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wire03_a_legacy_copy_and_a_migrated_copy_exchange_nothing() {
    let (a, b) = (device("w3-a", 3).await, device("w3-b", 4).await);
    pair(&a, &b);
    // The same document on both devices, as a build before the service left
    // it (same id, title, thread, text); B has migrated it to v2, A has not.
    let id = "document:sharedlegacy".to_string();
    for d in [&a, &b] {
        let mut doc = sovereign_db::schema::Document::new("T".into(), "thread:t1".into(), true);
        doc.id = sovereign_db::schema::raw_to_thing(&id);
        doc.content = r#"{"body":"x","images":[]}"#.into();
        doc.created_at = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        doc.modified_at = doc.created_at;
        assert!(d.edb.create_document_with_id(doc).await.unwrap());
    }
    b.docs.migrate_all().await.unwrap();
    let plan = round(&a, &b).await;
    assert!(plan.fetch.is_empty() && plan.push.is_empty() && plan.concurrent.is_empty(), "nothing to exchange: {plan:?}");
    // A title-only edit is a difference.
    let base = a.docs.read(&id).await.unwrap().revision;
    a.docs
        .save(&id, sovereign_docs::Edit { base, title: Some("Renamed".into()), content: None, origin: Origin::Editor { session: sovereign_docs::EditSession(3) }, explicit: false, pasted_tokens: vec![] })
        .await
        .unwrap();
    let plan = round(&a, &b).await;
    assert!(plan.push.contains(&id), "a title-only edit travels: {plan:?}");
    assert_eq!(b.edb.get_document(&id).await.unwrap().title, "Renamed");
}

/// 4. "Order is by stamp, not by clock: a later write wins whatever the two
/// devices' clocks say."
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wire04_a_later_edit_wins_even_from_a_device_whose_clock_is_behind() {
    let (a, b) = (device("w4-a", 5).await, device("w4-b", 6).await);
    pair(&a, &b);
    b.clock.advance(Duration::hours(-3)); // B's clock is three hours behind A's
    let id = create(&a, "T", "from a").await;
    settle(&a, &b, 4).await;
    edit(&b, &id, "later, from b").await; // later in fact, earlier by B's clock
    settle(&a, &b, 4).await;
    assert_eq!(stored_text(&a, &id).await, vec!["later, from b"]);
    assert_eq!(stored_text(&b, &id).await, vec!["later, from b"]);
}

/// 4. "… a stamp more than 5 minutes ahead of the receiver is refused."
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wire04_a_state_stamped_too_far_ahead_is_refused() {
    let (a, b) = (device("w4s-a", 7).await, device("w4s-b", 8).await);
    pair(&a, &b);
    a.clock.advance(Duration::minutes(30)); // A's clock runs 30 minutes ahead
    let id = create(&a, "T", "from the future").await;
    round(&a, &b).await;
    assert!(b.docs.read(&id).await.is_err(), "not applied on B");
}

/// 5. "Convergence: after both directions of a round, both devices hold equal
/// states; when the receiver's ingest rewrites the text, the rewritten state
/// travels back once and a further round with no edits exchanges nothing."
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wire05_a_rewrite_by_the_receivers_ingest_travels_back_once_then_nothing_moves() {
    let (a, b) = (device("w5-a", 9).await, device("w5-b", 10).await);
    pair(&a, &b);
    *b.pii.tokenize.lock().unwrap() = true; // B tokenizes what A left raw
    let id = create(&a, "T", &format!("write to {SECRET}")).await;
    let rounds = settle(&a, &b, 6).await;
    assert!(rounds <= 3, "converged in {rounds} rounds");
    let (sa, sb) = (a.docs.stored_state(&id).await.unwrap(), b.docs.stored_state(&id).await.unwrap());
    assert_eq!(sa.normalized_hash, sb.normalized_hash);
    assert!(!stored_text(&a, &id).await.join("").contains(SECRET), "the tokenized text came back");
    let plan = round(&a, &b).await;
    assert!(plan.fetch.is_empty() && plan.push.is_empty() && plan.concurrent.is_empty(), "no loop: {plan:?}");
}

/// 6. "Nothing is lost: every local state a remote state replaces is a version
/// first."
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wire06_a_replaced_local_state_is_a_version_first() {
    let (a, b) = (device("w6-a", 11).await, device("w6-b", 12).await);
    pair(&a, &b);
    let id = create(&a, "T", "start").await;
    settle(&a, &b, 4).await;
    edit(&b, &id, "b typed this").await;
    settle(&a, &b, 4).await;
    edit(&a, &id, "a typed this later").await;
    settle(&a, &b, 4).await;
    assert!(history(&b, &id).await.contains(&vec!["b typed this".to_string()]) || stored_text(&b, &id).await == vec!["b typed this"]);
}

/// 7. "Concurrent edits keep both without asking: both devices end with the
/// same current state (the greater stamp) and the other state in History."
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wire07_concurrent_edits_keep_both_on_both_devices() {
    let (a, b) = (device("w7-a", 13).await, device("w7-b", 14).await);
    pair(&a, &b);
    let id = create(&a, "T", "shared").await;
    settle(&a, &b, 4).await;
    edit(&a, &id, "a edit").await;
    edit(&b, &id, "b edit").await;
    let plan = round(&a, &b).await;
    assert!(plan.concurrent.contains(&id), "seen as concurrent: {plan:?}");
    settle(&a, &b, 4).await;
    let (ca, cb) = (stored_text(&a, &id).await, stored_text(&b, &id).await);
    assert_eq!(ca, cb, "same current state");
    for d in [&a, &b] {
        let mut all = history(d, &id).await;
        all.push(stored_text(d, &id).await);
        for e in ["a edit", "b edit"] {
            assert!(all.contains(&vec![e.to_string()]), "{}: {e} missing", d.name);
        }
    }
}

/// 8. "Trust follows decisions only: a later decision applies in either
/// direction; no other remote difference changes trust; a document new here
/// never arrives owned without a sender trust or decision that says so."
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wire08_trust_decisions_travel_both_ways_and_nothing_else_moves_trust() {
    let (a, b) = (device("w8-a", 15).await, device("w8-b", 16).await);
    pair(&a, &b);
    let new = sovereign_docs::NewDocument {
        title: "Clip".into(),
        thread_id: "thread:t1".into(),
        content: sovereign_docs::NewContent::Text("page".into()),
        trust: Trust::External,
        provenance: Provenance::Web { url: "https://example.org/".into() },
        origin: Origin::Editor { session: sovereign_docs::EditSession(1) },
        created_at: None,
        modified_at: None,
    };
    let id = a.docs.create(new).await.unwrap().id;
    settle(&a, &b, 4).await;
    assert_eq!(b.docs.read(&id).await.unwrap().trust, Trust::External, "not owned on arrival");
    b.docs.set_trust(&id, Trust::Owned).await.unwrap(); // adopted on the phone
    settle(&a, &b, 4).await;
    assert_eq!(a.docs.read(&id).await.unwrap().trust, Trust::Owned, "the adoption reached the laptop");
    a.clock.advance(Duration::minutes(1));
    a.docs.set_trust(&id, Trust::Undetermined).await.unwrap(); // lowered on the laptop
    settle(&a, &b, 4).await;
    let vb = b.docs.read(&id).await.unwrap();
    assert_eq!(vb.trust, Trust::Undetermined, "the phone follows");
    assert!(matches!(vb.trust_decision, Some(TrustDecision { trust: Trust::Undetermined, .. })));
    // A content edit alone never moves trust.
    edit(&a, &id, "edited").await;
    settle(&a, &b, 4).await;
    assert_eq!(b.docs.read(&id).await.unwrap().trust, Trust::Undetermined);
}

/// 9. "Tombstones: every user deletion reaches every paired device as a trash,
/// restorable; a later live state revives."
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wire09_a_deletion_reaches_the_other_device_as_a_restorable_trash() {
    let (a, b) = (device("w9-a", 17).await, device("w9-b", 18).await);
    pair(&a, &b);
    let id = create(&a, "T", "keep").await;
    settle(&a, &b, 4).await;
    a.docs.trash(&id).await.unwrap();
    settle(&a, &b, 4).await;
    assert!(b.docs.read(&id).await.unwrap().trashed_at.is_some(), "in the trash on B");
    assert!(!b.edb.get_document(&id).await.unwrap().peer_review_pending, "no review flag (13)");
    b.docs.restore_from_trash(&id).await.unwrap(); // restored on the phone: later, live
    settle(&a, &b, 4).await;
    assert!(a.docs.read(&id).await.unwrap().trashed_at.is_none(), "revived on A");
    assert_eq!(stored_text(&a, &id).await, vec!["keep"]);
}
