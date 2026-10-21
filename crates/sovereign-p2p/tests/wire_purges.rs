//! C-WIRE guarantee 10 — a purge removes only the same version.
//!
//! Spec ch. 05, *Sync wire — contract (C-WIRE)*, guarantee 10 (Céline,
//! 2026-10-15): "a device whose copy has the purged fingerprint purges it,
//! wherever it sits; a device whose copy differs keeps it, and that copy comes
//! back to the purging device as a new arrival; a second purge of now-identical
//! copies removes them everywhere; the marker carries no title or content, only
//! the id, stamp and fingerprint."

mod wire_common;

use wire_common::*;

async fn purge(d: &Device, id: &str) {
    d.docs.trash(id).await.unwrap();
    d.docs.purge_from_trash(id).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wire10_an_identical_copy_is_purged_on_the_other_device_live_or_trashed() {
    for trashed_on_b in [false, true] {
        let (a, b) = (device(&format!("w10s-a-{trashed_on_b}"), 21).await, device(&format!("w10s-b-{trashed_on_b}"), 22).await);
        pair(&a, &b);
        let id = create(&a, "Doc", "same").await;
        settle(&a, &b, 4).await;
        if trashed_on_b {
            b.docs.trash(&id).await.unwrap();
            settle(&a, &b, 4).await;
        }
        purge(&a, &id).await;
        settle(&a, &b, 4).await;
        assert!(b.docs.is_purged(&id).await.unwrap(), "purged on B (trashed there: {trashed_on_b})");
        assert!(b.edb.get_document(&id).await.is_err());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wire10_a_differing_copy_is_kept_comes_back_and_a_second_purge_removes_both() {
    let (a, b) = (device("w10d-a", 23).await, device("w10d-b", 24).await);
    pair(&a, &b);
    let id = create(&a, "Doc", "first").await;
    settle(&a, &b, 4).await;
    edit(&b, &id, "edited on the phone").await; // B's copy now differs; not yet synced
    purge(&a, &id).await;
    settle(&a, &b, 6).await;
    assert_eq!(stored_text(&b, &id).await, vec!["edited on the phone"], "kept on B");
    assert_eq!(stored_text(&a, &id).await, vec!["edited on the phone"], "came back to A, edits included");
    assert!(!a.docs.is_purged(&id).await.unwrap());
    purge(&b, &id).await; // deleted again, on the other device this time
    settle(&a, &b, 6).await;
    assert!(a.docs.is_purged(&id).await.unwrap() && b.docs.is_purged(&id).await.unwrap(), "gone from both");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wire10_the_manifest_marker_carries_no_title_or_content() {
    let (a, b) = (device("w10m-a", 25).await, device("w10m-b", 26).await);
    pair(&a, &b);
    let id = create(&a, "A very private title", "a very private body").await;
    settle(&a, &b, 4).await;
    purge(&a, &id).await;
    let m = manifest(&a).await;
    let marker = m.purge_markers.iter().find(|p| p.doc_id == id).expect("the marker is in the manifest");
    assert_eq!(marker.fingerprint.len(), 64);
    let json = serde_json::to_string(&m).unwrap();
    assert!(!json.contains("private"), "no title or content in the manifest");
}
