//! G12 — trust and provenance are set at creation and never changed silently.
//!
//! Spec ch. 03, *Document service — contract (C-DOC)*, G12: "**Trust and
//! provenance are set at creation and never changed silently.** Provenance
//! never changes. Trust changes only by a user decision: `set_trust` here,
//! which records the decision (device, time), or such a decision made on a
//! paired device and received by sync — applied in either direction, the later
//! decision winning **[choice: by recorded time]**. No save, skill, migration,
//! restore or trash changes it, and nothing else in sync does: a remote state
//! that differs in trust without a decision leaves local trust unchanged.
//! Undetermined is fenced like external. Content from the web cannot be
//! created owned without a user decision **[choice]**. A document created by
//! sync takes the sender's trust and provenance (an older peer that sends no
//! trust gives undetermined). Rows from before the service read as owned or
//! external from `is_owned`, with provenance `Unrecorded`."
//!
//! Ch. 04, *Ownership is not decided at landing* (decided 2026-10-10): three
//! states, "can move between them in both directions by the user's explicit
//! gesture … Undetermined is treated as external". Across devices (decided by
//! Céline, 2026-10-11): trust follows who decided, not the direction.

mod common;

use chrono::Duration;
use common::*;
use sovereign_docs::{
    ChangeKind, DocError, EditContent, EditSession, Provenance, RemoteDocument, RemoteOutcome,
    Trust, TrustDecision,
};

fn decided(trust: Trust, device: &str, at: chrono::DateTime<chrono::Utc>) -> Option<TrustDecision> {
    Some(TrustDecision { trust, device: device.into(), decided_at: at })
}

fn remote(id: &str, trust: Option<Trust>, provenance: Option<Provenance>, at: chrono::DateTime<chrono::Utc>) -> RemoteDocument {
    RemoteDocument {
        id: id.into(),
        title: "Remote".into(),
        content: v2(vec![text_block(1, "remote text")]),
        thread_id: "thread:t1".into(),
        trust,
        provenance,
        modified_at: at,
        deleted_at: None,
        device: "phone".into(),
        trust_decision: None,
    }
}

#[test]
fn g12_undetermined_is_fenced_like_external() {
    assert!(!Trust::Owned.is_fenced());
    assert!(Trust::External.is_fenced());
    assert!(Trust::Undetermined.is_fenced());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g12_create_records_the_trust_and_provenance_given() {
    let s = Stack::new("g12-create").await;
    for (trust, prov) in [
        (Trust::Owned, Provenance::Written),
        (Trust::Owned, Provenance::Assistant),
        (Trust::External, Provenance::Web { url: "https://example.org/x".into() }),
        (Trust::Undetermined, Provenance::Import { source_path: "C:/in/a.md".into() }),
        (Trust::External, Provenance::Import { source_path: "C:/in/b.md".into() }),
    ] {
        let mut new = new_text_doc("T", "x");
        new.trust = trust;
        new.provenance = prov.clone();
        let id = s.svc.create(new).await.unwrap().id;
        let v = s.svc.read(&id).await.unwrap();
        assert_eq!((v.trust, v.provenance), (trust, prov));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g12_web_content_cannot_be_created_owned() {
    let s = Stack::new("g12-web").await;
    let mut new = new_text_doc("Clip", "from a page");
    new.trust = Trust::Owned;
    new.provenance = Provenance::Web { url: "https://example.org/".into() };
    let r = s.svc.create(new).await;
    assert!(matches!(r, Err(DocError::TrustNotAllowed { trust: Trust::Owned })));
    assert!(s.svc.list(None).await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g12_no_write_changes_trust_or_provenance() {
    let s = Stack::new("g12-writes").await;
    for trust in [Trust::External, Trust::Undetermined] {
        let mut new = new_text_doc("T", "x");
        new.trust = trust;
        new.provenance = Provenance::Import { source_path: "C:/a.md".into() };
        let id = s.svc.create(new).await.unwrap().id;
        let v1 = s.commits(&id).await[0].id_string().unwrap();
        s.editor_save(&id, 1, v2(vec![text_block(1, "edited")])).await.unwrap();
        s.assistant_save(&id, "assistant").await.unwrap();
        let base = s.revision(&id).await;
        s.svc.apply_skill_output(&id, base, "redactor", EditContent::PlainText("skill".into())).await.unwrap();
        s.svc.end_session(&id, EditSession(1)).await.unwrap();
        s.svc.restore_version(&id, &v1).await.unwrap();
        s.svc.trash(&id).await.unwrap();
        s.svc.restore_from_trash(&id).await.unwrap();
        s.svc.migrate_all().await.unwrap();
        s.svc.apply_remote(remote(&id, Some(trust), None, s.clock.get())).await.unwrap();
        let v = s.svc.read(&id).await.unwrap();
        assert_eq!(v.trust, trust);
        assert_eq!(v.provenance, Provenance::Import { source_path: "C:/a.md".into() });
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g12_set_trust_moves_in_both_directions_and_says_so() {
    let s = Stack::new("g12-set").await;
    let id = s.create_text("T", "x").await;
    s.events.take();
    for t in [Trust::External, Trust::Owned, Trust::Undetermined, Trust::Owned] {
        s.svc.set_trust(&id, t).await.unwrap();
        assert_eq!(s.svc.read(&id).await.unwrap().trust, t);
        let ev = s.events.take();
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].kind, ChangeKind::TrustChanged);
    }
    s.svc.set_trust(&id, Trust::Owned).await.unwrap();
    assert!(s.events.take().is_empty(), "no change, no event");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g12_set_trust_is_kept_where_the_fence_is_read() {
    // The flag the rest of the system reads today (`is_owned`) agrees with
    // the trust state: owned only when owned.
    let s = Stack::new("g12-is-owned").await;
    let id = s.create_text("T", "x").await;
    for (t, owned) in [(Trust::External, false), (Trust::Undetermined, false), (Trust::Owned, true)] {
        s.svc.set_trust(&id, t).await.unwrap();
        assert_eq!(s.stored_doc(&id).await.is_owned, owned, "{t:?}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g12_rows_from_before_the_service_read_from_is_owned_with_no_provenance() {
    let s = Stack::new("g12-legacy").await;
    let mine = s.legacy_with("Mine", "x", true).await;
    let theirs = s.legacy_with("Theirs", "y", false).await;
    let a = s.svc.read(&mine).await.unwrap();
    let b = s.svc.read(&theirs).await.unwrap();
    assert_eq!((a.trust, a.provenance), (Trust::Owned, Provenance::Unrecorded));
    assert_eq!((b.trust, b.provenance), (Trust::External, Provenance::Unrecorded));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g12_a_synced_document_keeps_the_senders_trust_and_provenance() {
    let s = Stack::new("g12-sync-new").await;
    let at = s.clock.get();
    let web = Provenance::Web { url: "https://example.org/clip".into() };
    s.svc.apply_remote(remote("document:ext1", Some(Trust::External), Some(web.clone()), at)).await.unwrap();
    let v = s.svc.read("document:ext1").await.unwrap();
    assert_eq!((v.trust, v.provenance), (Trust::External, web));
    assert!(!s.stored_doc("document:ext1").await.is_owned, "not landed owned (ch. 05 conflict)");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g12_a_peer_that_sends_no_trust_lands_undetermined() {
    let s = Stack::new("g12-sync-old-peer").await;
    s.svc.apply_remote(remote("document:old1", None, None, s.clock.get())).await.unwrap();
    let v = s.svc.read("document:old1").await.unwrap();
    assert_eq!(v.trust, Trust::Undetermined);
    assert_eq!(v.provenance, Provenance::Peer { device: "phone".into() });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g12_sync_never_raises_a_local_document_to_owned_without_a_decision() {
    let s = Stack::new("g12-sync-raise").await;
    for local in [Trust::External, Trust::Undetermined] {
        let mut new = new_text_doc("T", "x");
        new.trust = local;
        new.provenance = Provenance::Web { url: "https://example.org/".into() };
        let id = s.svc.create(new).await.unwrap().id;
        s.clock.advance(Duration::minutes(1));
        s.svc.apply_remote(remote(&id, Some(Trust::Owned), None, s.clock.get())).await.unwrap();
        let v = s.svc.read(&id).await.unwrap();
        assert_eq!(v.trust, local, "{local:?} changed by a peer without a decision");
        assert!(!s.stored_doc(&id).await.is_owned);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g12_sync_never_lowers_trust_without_a_decision_either() {
    let s = Stack::new("g12-sync-lower").await;
    let id = s.create_text("Mine", "x").await;
    for remote_trust in [Trust::External, Trust::Undetermined] {
        s.clock.advance(Duration::minutes(1));
        s.svc.apply_remote(remote(&id, Some(remote_trust), None, s.clock.get())).await.unwrap();
        assert_eq!(s.svc.read(&id).await.unwrap().trust, Trust::Owned);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g12_set_trust_records_the_users_decision_with_this_device_and_time() {
    let s = Stack::new("g12-record").await;
    let id = s.create_text("T", "x").await;
    assert_eq!(s.svc.read(&id).await.unwrap().trust_decision, None, "creation is not a decision");
    s.clock.advance(Duration::hours(2));
    s.svc.set_trust(&id, Trust::External).await.unwrap();
    let want = decided(Trust::External, DEVICE, s.clock.get());
    assert_eq!(s.svc.read(&id).await.unwrap().trust_decision, want);
    // Setting the trust it already has records nothing new.
    s.clock.advance(Duration::hours(1));
    s.svc.set_trust(&id, Trust::External).await.unwrap();
    assert_eq!(s.svc.read(&id).await.unwrap().trust_decision, want);
}

/// Céline's example: consulted, adopted on the phone to edit it → it arrives
/// owned on the desktop; lowered on the desktop → the phone follows.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g12_a_decision_on_a_paired_device_is_followed_in_both_directions() {
    let s = Stack::new("g12-follow").await;
    let mut new = new_text_doc("Consulted", "x");
    new.trust = Trust::External;
    new.provenance = Provenance::Web { url: "https://example.org/a".into() };
    let id = s.svc.create(new).await.unwrap().id;

    s.clock.advance(Duration::minutes(5));
    let mut r = remote(&id, Some(Trust::Owned), None, s.clock.get());
    r.trust_decision = decided(Trust::Owned, "phone", s.clock.get());
    s.svc.apply_remote(r.clone()).await.unwrap();
    let v = s.svc.read(&id).await.unwrap();
    assert_eq!(v.trust, Trust::Owned, "adopted on the phone, owned here");
    assert_eq!(v.trust_decision, r.trust_decision, "the decision is kept, so it travels on");
    assert!(s.stored_doc(&id).await.is_owned);

    s.clock.advance(Duration::minutes(5));
    let mut r = remote(&id, Some(Trust::Undetermined), None, s.clock.get());
    r.trust_decision = decided(Trust::Undetermined, "phone", s.clock.get());
    s.svc.apply_remote(r).await.unwrap();
    assert_eq!(s.svc.read(&id).await.unwrap().trust, Trust::Undetermined, "lowered on the phone, lowered here");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g12_the_later_decision_wins() {
    let s = Stack::new("g12-later").await;
    let id = s.create_text("T", "x").await;
    let t0 = s.clock.get();
    s.clock.advance(Duration::hours(1));
    s.svc.set_trust(&id, Trust::External).await.unwrap(); // decided here at t0 + 1h

    // An older decision from the phone (made before ours, arriving after).
    let mut older = remote(&id, Some(Trust::Owned), None, s.clock.get());
    older.trust_decision = decided(Trust::Owned, "phone", t0 + Duration::minutes(30));
    s.svc.apply_remote(older).await.unwrap();
    assert_eq!(s.svc.read(&id).await.unwrap().trust, Trust::External, "ours is later");

    // A newer one wins.
    s.clock.advance(Duration::hours(1));
    let mut newer = remote(&id, Some(Trust::Owned), None, s.clock.get());
    newer.trust_decision = decided(Trust::Owned, "phone", s.clock.get());
    s.svc.apply_remote(newer).await.unwrap();
    assert_eq!(s.svc.read(&id).await.unwrap().trust, Trust::Owned);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g12_a_synced_web_document_arrives_owned_only_with_a_decision() {
    let s = Stack::new("g12-web-sync").await;
    let web = Some(Provenance::Web { url: "https://example.org/c".into() });
    let out = s.svc.apply_remote(remote("document:w1", Some(Trust::Owned), web.clone(), s.clock.get())).await.unwrap();
    assert_eq!(out, RemoteOutcome::Created);
    assert_ne!(s.svc.read("document:w1").await.unwrap().trust, Trust::Owned, "creation rules hold over sync");
    let mut r = remote("document:w2", Some(Trust::Owned), web, s.clock.get());
    r.trust_decision = decided(Trust::Owned, "phone", s.clock.get());
    s.svc.apply_remote(r).await.unwrap();
    assert_eq!(s.svc.read("document:w2").await.unwrap().trust, Trust::Owned, "adopted by the user elsewhere");
}
