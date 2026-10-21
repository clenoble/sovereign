//! G27 — purge markers: the same version is purged everywhere, a different one
//! comes back.
//!
//! Spec ch. 03, *Document service — contract (C-DOC)*, G27 (specified
//! 2026-10-15, for C-WIRE): "A purge leaves a marker — id, stamp, and the
//! fingerprint of the purged version (title and content), never the content.
//! A received marker purges this device's copy only if it has the same
//! fingerprint, wherever it sits (live or in the trash, with any trust); a copy that differs is kept. A purged id ignores a state with
//! the purged fingerprint and comes back, as a new arrival, from a state that
//! differs — the one exception to G13."
//!
//! C-WIRE guarantee 10 (spec ch. 05; Céline, 2026-10-15): "A purge removes
//! only the same version."

mod common;

use common::wire::*;
use common::*;
use sovereign_docs::{purge_fingerprint, PurgeApplied, Trust, WireApplied};

/// Device A purges a document B also holds; returns the marker.
async fn purged_on_a(a: &Stack, id: &str) -> sovereign_docs::PurgeMarkerWire {
    a.svc.trash(id).await.unwrap();
    a.svc.purge_from_trash(id).await.unwrap();
    a.svc.purge_markers().await.unwrap().into_iter().find(|m| m.id == id).expect("a marker for the purge")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g27_a_purge_leaves_a_marker_with_the_fingerprint_of_the_purged_version_and_no_content() {
    let s = Stack::new("g27-marker").await;
    let id = s.create_text("Very private title", "very private body").await;
    let (title, content) = s.stored(&id).await;
    let m = purged_on_a(&s, &id).await;
    assert_eq!(m.fingerprint, purge_fingerprint(&id, &title, &content));
    assert_eq!(m.fingerprint.len(), 64);
    assert!(m.fingerprint.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
    assert!(!m.fingerprint.contains("private"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g27_an_identical_copy_is_purged_wherever_it_sits() {
    for case in ["live", "trashed", "adopted"] {
        let a = Stack::with_device(&format!("g27-same-a-{case}"), "laptop").await;
        let b = Stack::with_device(&format!("g27-same-b-{case}"), "phone").await;
        let mut new = new_text_doc("Doc", "the same text");
        new.trust = Trust::Undetermined;
        new.provenance = sovereign_docs::Provenance::Import { source_path: "D:/a.md".into() };
        new.origin = sovereign_docs::Origin::Import;
        let id = a.svc.create(new).await.unwrap().id;
        send(&a, &b, &id).await;
        match case {
            "trashed" => b.svc.trash(&id).await.unwrap(),
            "adopted" => b.svc.set_trust(&id, Trust::Owned).await.unwrap(),
            _ => {}
        }
        let marker = purged_on_a(&a, &id).await;
        let out = b.svc.apply_purge_marker(marker).await.unwrap();
        assert_eq!(out, PurgeApplied::Purged, "{case}");
        assert!(b.svc.is_purged(&id).await.unwrap(), "{case}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g27_a_copy_that_differs_is_kept_and_comes_back_to_the_purging_device() {
    let a = Stack::with_device("g27-diff-a", "laptop").await;
    let b = Stack::with_device("g27-diff-b", "phone").await;
    let id = a.create_text("Doc", "first").await;
    send(&a, &b, &id).await;
    b.editor_save(&id, 1, v2(vec![text_block(1, "edited on the phone")])).await.unwrap();
    let marker = purged_on_a(&a, &id).await;
    assert_eq!(b.svc.apply_purge_marker(marker).await.unwrap(), PurgeApplied::KeptDiffers);
    assert_eq!(texts(&b.stored(&id).await.1), vec!["edited on the phone"]);
    assert!(b.svc.read(&id).await.unwrap().trashed_at.is_none(), "kept as it was");
    // It travels back: recreated on the purging device as a new arrival.
    let out = send(&b, &a, &id).await;
    assert!(matches!(out.applied, WireApplied::Applied(sovereign_docs::RemoteOutcome::Created)), "{:?}", out.applied);
    assert!(!a.svc.is_purged(&id).await.unwrap(), "the marker is dropped there");
    assert_eq!(texts(&a.stored(&id).await.1), vec!["edited on the phone"]);
    assert_eq!(a.commits(&id).await.len(), 1, "its own first version");
    // Deleted again on either device, now identical: gone from both.
    let marker = purged_on_a(&a, &id).await;
    assert_eq!(b.svc.apply_purge_marker(marker).await.unwrap(), PurgeApplied::Purged);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g27_the_purged_version_never_comes_back_from_any_peer() {
    let a = Stack::with_device("g27-same-back-a", "laptop").await;
    let b = Stack::with_device("g27-same-back-b", "phone").await;
    let id = a.create_text("Doc", "text").await;
    send(&a, &b, &id).await;
    let identical = b.svc.stored_state(&id).await.unwrap();
    purged_on_a(&a, &id).await;
    let out = a.svc.apply_wire(identical.document, identical.meta).await.unwrap();
    assert_eq!(out.applied, WireApplied::Ignored);
    assert!(a.svc.is_purged(&id).await.unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g27_a_device_without_the_document_records_the_marker() {
    let a = Stack::with_device("g27-none-a", "laptop").await;
    let c = Stack::with_device("g27-none-c", "tablet").await;
    let id = a.create_text("Doc", "text").await;
    let state = a.svc.stored_state(&id).await.unwrap();
    let marker = purged_on_a(&a, &id).await;
    assert_eq!(c.svc.apply_purge_marker(marker.clone()).await.unwrap(), PurgeApplied::Recorded);
    assert!(c.svc.purge_markers().await.unwrap().contains(&marker));
    // The purged version, arriving later from a third device, is refused.
    let out = c.svc.apply_wire(state.document, state.meta).await.unwrap();
    assert_eq!(out.applied, WireApplied::Ignored);
    assert!(c.svc.read(&id).await.is_err());
}
