//! C-WIRE guarantee 10 — a purge is local; what travels is the deletion.
//!
//! Spec ch. 05, *Sync wire — contract (C-WIRE)*, *Deletions and purges*, **as
//! decided by Céline on 2026-10-23** (replaces the rule of 2026-10-15, under
//! which "a device whose copy has the purged fingerprint purges it, wherever it
//! sits"): "Emptying the trash, purging one item, or 30 days in the trash
//! removes the document from this device only … What travels after a purge
//! is the deletion, never the purge … A purged id comes back only live, by a
//! later action."
//!
//! Rewritten by the item-3 test author (2026-10-24,
//! `doc/plans/V0012-item3-local-purges.md`). Each test names the old test it
//! replaces and the guarantee it now checks; the other wire cases of item 3
//! are in `item3_local_purges.rs`.

mod wire_common;

use chrono::Duration;
use wire_common::*;

async fn purge(d: &Device, id: &str) {
    d.docs.trash(id).await.unwrap();
    d.docs.purge_from_trash(id).await.unwrap();
}

// Was `wire10_an_identical_copy_is_purged_on_the_other_device_live_or_trashed`
// (asserted: B's copy purged, its row gone). Now L1 + L4: B's copy is never
// purged; live it goes to B's trash, trashed it stays there as it was.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wire10_a_purge_never_removes_the_other_devices_copy_live_or_trashed() {
    for trashed_on_b in [false, true] {
        let (a, b) = (device(&format!("w10s-a-{trashed_on_b}"), 21).await, device(&format!("w10s-b-{trashed_on_b}"), 22).await);
        pair(&a, &b);
        let id = create(&a, "Doc", "same").await;
        settle(&a, &b, 4).await;
        let mut trashed_at = None;
        if trashed_on_b {
            b.docs.trash(&id).await.unwrap();
            trashed_at = b.docs.read(&id).await.unwrap().trashed_at;
            settle(&a, &b, 4).await;
        }
        let versions = b.edb.list_document_commits(&id).await.unwrap().len();
        a.clock.advance(Duration::minutes(1));
        purge(&a, &id).await;
        b.clock.advance(Duration::minutes(2));
        settle(&a, &b, 4).await;
        assert!(!b.docs.is_purged(&id).await.unwrap(), "never purged on B (trashed there: {trashed_on_b})");
        assert!(b.edb.get_document(&id).await.is_ok(), "B's row is there (trashed there: {trashed_on_b})");
        assert!(b.edb.list_document_commits(&id).await.unwrap().len() >= versions, "B's versions kept");
        let v = b.docs.read(&id).await.unwrap();
        assert!(v.trashed_at.is_some(), "in B's trash (trashed there: {trashed_on_b})");
        if trashed_on_b {
            assert_eq!(v.trashed_at, trashed_at, "kept as it was");
        }
        assert_eq!(stored_text(&b, &id).await, vec!["same"], "content kept");
        assert!(a.docs.is_purged(&id).await.unwrap(), "A's purge stays: B's copy does not bring it back");
    }
}

// Was `wire10_a_differing_copy_is_kept_comes_back_and_a_second_purge_removes_both`
// (asserted: a copy edited on B before the purge was kept by fingerprint, came
// back to A, and a second purge on B removed it from both). Now L4 + L6 by
// stamp, in both orders:
// - edited on B **before** A's deletion: the deletion wins; B's copy goes to
//   B's trash with its edit, and does not come back to A;
// - edited on B **after** A's deletion: kept live on B, back on A; a purge on
//   B then sends A's copy to A's trash — and purges nothing on A.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wire10_a_copy_edited_elsewhere_is_judged_by_its_stamp_and_a_second_purge_stays_local() {
    // Edited before the deletion.
    let (a, b) = (device("w10d-before-a", 23).await, device("w10d-before-b", 24).await);
    pair(&a, &b);
    let id = create(&a, "Doc", "first").await;
    settle(&a, &b, 4).await;
    b.clock.advance(Duration::minutes(1));
    edit(&b, &id, "edited on the phone").await; // not yet synced
    a.clock.advance(Duration::minutes(2));
    purge(&a, &id).await;
    settle(&a, &b, 6).await;
    assert_eq!(stored_text(&b, &id).await, vec!["edited on the phone"], "kept on B, with its edit");
    assert!(b.docs.read(&id).await.unwrap().trashed_at.is_some(), "in B's trash: the deletion is later");
    assert!(a.docs.is_purged(&id).await.unwrap(), "not back on A: B's copy is in its trash");

    // Edited after the deletion.
    let (a, b) = (device("w10d-after-a", 25).await, device("w10d-after-b", 26).await);
    pair(&a, &b);
    let id = create(&a, "Doc", "first").await;
    settle(&a, &b, 4).await;
    purge(&a, &id).await;
    b.clock.advance(Duration::minutes(1));
    edit(&b, &id, "edited on the phone").await;
    settle(&a, &b, 6).await;
    assert_eq!(stored_text(&b, &id).await, vec!["edited on the phone"], "kept on B");
    assert!(b.docs.read(&id).await.unwrap().trashed_at.is_none(), "live on B");
    assert_eq!(stored_text(&a, &id).await, vec!["edited on the phone"], "came back to A, edits included");
    assert!(!a.docs.is_purged(&id).await.unwrap());
    assert!(a.docs.read(&id).await.unwrap().trashed_at.is_none(), "live on A");
    // Deleted again, on the other device this time, and purged there.
    b.clock.advance(Duration::minutes(1));
    purge(&b, &id).await;
    a.clock.advance(Duration::minutes(2));
    settle(&a, &b, 6).await;
    assert!(b.docs.is_purged(&id).await.unwrap(), "purged on B");
    assert!(!a.docs.is_purged(&id).await.unwrap(), "not purged on A");
    assert!(a.docs.read(&id).await.unwrap().trashed_at.is_some(), "in A's trash");
    assert_eq!(stored_text(&a, &id).await, vec!["edited on the phone"]);
}

// Was `wire10_the_manifest_marker_carries_no_title_or_content` (asserted also:
// the marker's fingerprint is 64 hex digits). Now L3: the entry carries the
// id and the deletion's stamp, its `fingerprint` field is sent empty, and no
// title or content is in the manifest.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wire10_the_manifest_marker_carries_an_empty_fingerprint_and_no_title_or_content() {
    let (a, b) = (device("w10m-a", 27).await, device("w10m-b", 28).await);
    pair(&a, &b);
    let id = create(&a, "A very private title", "a very private body").await;
    settle(&a, &b, 4).await;
    a.docs.trash(&id).await.unwrap();
    let deletion = a.docs.stored_state(&id).await.unwrap().meta.stamp.unwrap();
    a.clock.advance(Duration::hours(1));
    a.docs.purge_from_trash(&id).await.unwrap();
    let m = manifest(&a).await;
    let marker = m.purge_markers.iter().find(|p| p.doc_id == id).expect("the marker is in the manifest");
    assert_eq!(marker.fingerprint, "", "sent empty (L3)");
    assert_eq!(
        (marker.stamp.wall_ms, marker.stamp.counter, marker.stamp.device.as_str()),
        (deletion.wall_ms, deletion.counter, deletion.device.as_str()),
        "the deletion's stamp (L2)"
    );
    let json = serde_json::to_string(&m).unwrap();
    assert!(!json.contains("private"), "no title or content in the manifest");
}
