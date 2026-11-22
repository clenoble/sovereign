//! C-SWEEP SW5 (spec ch. 09): a review's stamp received from another device is
//! read as a time before it can win. A review travels by sync and merges by its
//! own stamp, whichever copy of the record wins the rest; a later review wins
//! over an earlier one. So a stamp is only taken when it is in the form a review
//! writes (RFC 3339, UTC, to the microsecond, `Z`) and no later than the bound
//! on future times every synced row has (ch. 05, *Documents*: « Timestamps
//! beyond a bounded future skew are rejected »). Otherwise the review it
//! carries is ignored, and the local review state and stamp stand.
//!
//! Why: stamps compare as strings, so one that no review writes (« 9999-… », or
//! a string that sorts after every digit) would outrank every review she makes
//! after it; the bound is the one every other synced time already has.
//!
//! Each test: two of her devices, each a mock store and its row sync service.
//! A record is made on `a` and travels to `b`; `b` then holds a review whose
//! stamp `b` chose, and one round of row sync runs both ways.
//!
//! Written before the code by the test author of lot R3 (v0.0.13 rc2);
//! whoever implements does not edit this file.

use std::sync::Arc;

use chrono::{Duration, Utc};
use sovereign_db::mock::MockGraphDB;
use sovereign_db::schema::{PiiKind, PiiRecord, ReviewState};
use sovereign_db::GraphDB;
use sovereign_p2p::libp2p::identity::Keypair;
use sovereign_p2p::libp2p::PeerId;
use sovereign_p2p::{SyncService, VersionStore};

const PAIR_KEY: [u8; 32] = [0x5a; 32];

struct Device {
    db: Arc<MockGraphDB>,
    sync: SyncService,
    peer: PeerId,
}

fn device(seed: u8) -> Device {
    let db = Arc::new(MockGraphDB::new());
    let keypair = Keypair::ed25519_from_bytes([seed; 32]).unwrap();
    let peer = keypair.public().to_peer_id();
    let sync = SyncService::new(db.clone() as Arc<dyn GraphDB>, peer.to_string(), [7u8; 32], keypair, VersionStore::ephemeral());
    Device { db, sync, peer }
}

/// Two of her paired devices.
fn pair() -> (Device, Device) {
    let a = device(0xA1);
    let b = device(0xB2);
    a.sync.add_pair_key(b.peer.to_string(), PAIR_KEY);
    b.sync.add_pair_key(a.peer.to_string(), PAIR_KEY);
    (a, b)
}

/// One round of row sync between `a` and `b`, both ways, every table.
async fn round(a: &Device, b: &Device) {
    let ma = a.sync.build_manifest().await.unwrap();
    let mb = b.sync.build_manifest().await.unwrap();
    for (table, diff) in sovereign_p2p::sync_engine::compute_all_row_diffs(&ma, &mb) {
        if !diff.need_from_remote.is_empty() {
            let rows = b.sync.get_rows(table, &diff.need_from_remote, &a.peer).await.unwrap();
            a.sync.apply_rows(table, rows, &b.peer).await.unwrap();
        }
        if !diff.push_to_remote.is_empty() {
            let rows = a.sync.get_rows(table, &diff.push_to_remote, &b.peer).await.unwrap();
            b.sync.apply_rows(table, rows, &a.peer).await.unwrap();
        }
    }
}

/// A stamp in the form a review writes (`review_stamp_now`).
fn stamp(at: chrono::DateTime<Utc>) -> String {
    at.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()
}

/// A record on `a`, in the state ingest wrote at a save in the editor
/// (confirmed, no review stamp), travelled to `b`. Returns its id.
async fn record_on_both(a: &Device, b: &Device) -> String {
    let rec = a
        .db
        .create_pii_record(PiiRecord {
            id: None,
            kind: PiiKind::Iban,
            value_encrypted: "ZmFrZS1jaXBoZXJ0ZXh0".into(),
            value_nonce: String::new(),
            label: None,
            entity_id: None,
            stored_secret: false,
            confidence: 0.9,
            sources: vec![],
            discovered_at: Utc::now(),
            last_revealed_at: None,
            use_count: 0,
            review_state: ReviewState::Confirmed,
            review_stamp: None,
            deleted_at: None,
        })
        .await
        .unwrap();
    let id = rec.id_string().unwrap();
    round(a, b).await;
    assert!(b.db.get_pii_record(&id).await.is_ok(), "test setup: the record travelled to b");
    id
}

async fn review_on(d: &Device, id: &str) -> (ReviewState, Option<String>) {
    let r = d.db.get_pii_record(id).await.unwrap();
    (r.review_state, r.review_stamp)
}

/// `b` holds `state` under `forged`; after a round, `a`'s review stands as it was.
async fn a_keeps_its_review_against(a: &Device, b: &Device, id: &str, state: ReviewState, forged: &str, what: &str) {
    let before = review_on(a, id).await;
    b.db.update_pii_record_review(id, state, Some(forged)).await.unwrap();
    round(a, b).await;
    assert_eq!(review_on(a, id).await, before, "{what}: a stamp of {forged:?} must not move a's review");
}

// ================================================ stamps that are ignored

/// A stamp far in the future, in the review's own form: with no review of
/// hers on `a` yet (ingest's state only).
#[tokio::test]
async fn sw5_a_stamp_far_in_the_future_is_ignored_over_ingests_state() {
    let (a, b) = pair();
    let id = record_on_both(&a, &b).await;
    a_keeps_its_review_against(&a, &b, &id, ReviewState::Dismissed, "9999-12-31T23:59:59.999999Z", "no review on a").await;
}

/// The same, over a review of hers on `a`.
#[tokio::test]
async fn sw5_a_stamp_far_in_the_future_is_ignored_over_her_review() {
    let (a, b) = pair();
    let id = record_on_both(&a, &b).await;
    a.db.update_pii_record_review(&id, ReviewState::Confirmed, Some(&stamp(Utc::now()))).await.unwrap();
    round(&a, &b).await;
    a_keeps_its_review_against(&a, &b, &id, ReviewState::Dismissed, "9999-12-31T23:59:59.999999Z", "her review on a").await;
}

/// A stamp in the review's form, two days ahead: beyond the bound (ch. 05).
#[tokio::test]
async fn sw5_a_stamp_beyond_the_bound_on_future_times_is_ignored() {
    let (a, b) = pair();
    let id = record_on_both(&a, &b).await;
    let ahead = stamp(Utc::now() + Duration::days(2));
    a_keeps_its_review_against(&a, &b, &id, ReviewState::Dismissed, &ahead, "two days ahead").await;
}

/// Strings that are not in the review's form: each sorts after every stamp a
/// review writes, or names a time in another form, and none is taken.
#[tokio::test]
async fn sw5_a_stamp_not_in_the_reviews_form_is_ignored() {
    let now = Utc::now();
    let others = [
        "~".to_string(),
        "zzzz".to_string(),
        String::new(),
        now.format("%Y-%m-%dT%H:%M:%S+00:00").to_string(),
        now.format("%Y-%m-%d %H:%M:%S%.6f").to_string(),
        format!("{}garbage", stamp(now)),
    ];
    for forged in &others {
        let (a, b) = pair();
        let id = record_on_both(&a, &b).await;
        a_keeps_its_review_against(&a, &b, &id, ReviewState::Dismissed, forged, "not in the review's form").await;
    }
}

// ======================================================= stamps that travel

/// A review of an hour ago travels as before (SW5: « The review state and the
/// dismissals travel by sync »).
#[tokio::test]
async fn sw5_a_review_in_the_past_still_travels() {
    let (a, b) = pair();
    let id = record_on_both(&a, &b).await;
    let s = stamp(Utc::now() - Duration::hours(1));
    b.db.update_pii_record_review(&id, ReviewState::Dismissed, Some(&s)).await.unwrap();
    round(&a, &b).await;
    assert_eq!(review_on(&a, &id).await, (ReviewState::Dismissed, Some(s)), "a past review travels");
}

/// A device whose clock is an hour fast is within the bound: its review
/// travels.
#[tokio::test]
async fn sw5_a_review_within_the_bound_travels() {
    let (a, b) = pair();
    let id = record_on_both(&a, &b).await;
    let s = stamp(Utc::now() + Duration::hours(1));
    b.db.update_pii_record_review(&id, ReviewState::Dismissed, Some(&s)).await.unwrap();
    round(&a, &b).await;
    assert_eq!(review_on(&a, &id).await, (ReviewState::Dismissed, Some(s)), "a review within the bound travels");
}

/// After an ignored stamp, her next review on `a` is taken as any review is
/// (the ignored one left nothing behind that outranks it), and the record's
/// value stays the one both devices hold.
#[tokio::test]
async fn sw5_after_an_ignored_stamp_her_next_review_stands() {
    let (a, b) = pair();
    let id = record_on_both(&a, &b).await;
    let value_before = a.db.get_pii_record(&id).await.unwrap().value_encrypted;
    a_keeps_its_review_against(&a, &b, &id, ReviewState::Dismissed, "9999-12-31T23:59:59.999999Z", "forged").await;
    let hers = stamp(Utc::now());
    a.db.update_pii_record_review(&id, ReviewState::Unreviewed, Some(&hers)).await.unwrap();
    round(&a, &b).await;
    assert_eq!(review_on(&a, &id).await, (ReviewState::Unreviewed, Some(hers)), "her review on a stands");
    assert_eq!(a.db.get_pii_record(&id).await.unwrap().value_encrypted, value_before, "the value is unchanged");
}

// ======================================================= a record that arrives new

/// A record that `a` does not hold yet arrives with a review whose stamp is
/// not taken: it is created unreviewed and with no stamp, for her to review on
/// `a`; and her review of it there then stands.
#[tokio::test]
async fn sw5_a_new_record_with_a_stamp_not_taken_arrives_unreviewed() {
    for forged in ["9999-12-31T23:59:59.999999Z".to_string(), stamp(Utc::now() + Duration::days(2)), "~".to_string()] {
        let (a, b) = pair();
        let rec = b
            .db
            .create_pii_record(PiiRecord {
                id: None,
                kind: PiiKind::Iban,
                value_encrypted: "ZmFrZS1jaXBoZXJ0ZXh0".into(),
                value_nonce: String::new(),
                label: None,
                entity_id: None,
                stored_secret: false,
                confidence: 0.9,
                sources: vec![],
                discovered_at: Utc::now(),
                last_revealed_at: None,
                use_count: 0,
                review_state: ReviewState::Dismissed,
                review_stamp: Some(forged.clone()),
                deleted_at: None,
            })
            .await
            .unwrap();
        let id = rec.id_string().unwrap();
        round(&a, &b).await;
        assert!(a.db.get_pii_record(&id).await.is_ok(), "test setup: the record arrived on a");
        assert_eq!(review_on(&a, &id).await, (ReviewState::Unreviewed, None), "a stamp of {forged:?}: arrives unreviewed, unstamped");

        let hers = stamp(Utc::now());
        a.db.update_pii_record_review(&id, ReviewState::Confirmed, Some(&hers)).await.unwrap();
        round(&a, &b).await;
        assert_eq!(review_on(&a, &id).await, (ReviewState::Confirmed, Some(hers)), "a stamp of {forged:?}: her review on a stands");
    }
}
