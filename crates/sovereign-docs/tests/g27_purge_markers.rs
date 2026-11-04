//! G27 — purge markers: a purge is local; what travels is the deletion.
//!
//! Spec ch. 03, *Document service — contract (C-DOC)*, G27, **as amended
//! 2026-10-23** (Céline: purges are local; built in v0.0.12, item 3): "A
//! purge leaves a local marker — id and deletion stamp, no fingerprint, never
//! content. It travels as a deletion only: a peer's live copy with an earlier
//! stamp goes to that peer's trash, a trashed copy stays there; no peer copy
//! is purged. A purged id ignores a peer's trashed copy and any earlier live
//! state, and comes back, live, as a new arrival with its own first version,
//! from a live state with a later stamp (the exception to G13)."
//!
//! Rewritten by the item-3 test author (2026-10-24) from the rule of
//! 2026-10-15 (a marker carried the purged version's fingerprint and purged a
//! peer's identical copy). Each test names the old test it replaces and the
//! guarantee of `doc/plans/V0012-item3-local-purges.md` it now checks; the
//! wider cases are in `item3_local_purges.rs`.

mod common;

use common::wire::*;
use common::*;
use sovereign_docs::{RemoteOutcome, Trust, WireApplied};

/// Device A purges a document B may also hold; returns the marker A
/// advertises and the stamp of the deletion.
async fn purged_on_a(a: &Stack, id: &str) -> (sovereign_docs::PurgeMarkerWire, sovereign_docs::Stamp) {
    a.svc.trash(id).await.unwrap();
    let deletion = current_stamp(a, id).await;
    a.svc.purge_from_trash(id).await.unwrap();
    let m = a.svc.purge_markers().await.unwrap().into_iter().find(|m| m.id == id).expect("a marker for the purge");
    (m, deletion)
}

// Was `g27_a_purge_leaves_a_marker_with_the_fingerprint_of_the_purged_version_and_no_content`
// (asserted: the marker carries `purge_fingerprint(id, title, content)`, 64
// lowercase hex, no text). Now L2: the marker carries the id and the stamp of
// the deletion; on disk it has no fingerprint and no text.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g27_a_purge_leaves_a_marker_with_the_deletion_stamp_and_no_fingerprint_or_content() {
    let s = Stack::new("g27-marker").await;
    let id = s.create_text("Very private title", "very private body").await;
    let (m, deletion) = purged_on_a(&s, &id).await;
    assert_eq!(m.id, id);
    assert_eq!(m.stamp, deletion, "the deletion's stamp");
    let disk = s.raw.get_purge_marker(&id).await.unwrap().expect("a marker row");
    assert_eq!(disk.fingerprint, None, "no fingerprint (finding 13)");
    assert!(!format!("{disk:?}").contains("private"), "no title or content");
}

// Was `g27_an_identical_copy_is_purged_wherever_it_sits` (asserted:
// `PurgeApplied::Purged` and `is_purged` on B, for B's copy live, trashed and
// adopted). Now L1 + L4: B's copy is never purged; live (adopted or not) it
// goes to B's trash with its content, trashed it stays there.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g27_an_identical_copy_elsewhere_is_never_purged_only_moved_to_the_trash() {
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
            "trashed" => {
                // `trash` answers its outcome since v0.0.12 item 9 (T3).
                b.svc.trash(&id).await.unwrap();
            }
            "adopted" => b.svc.set_trust(&id, Trust::Owned).await.unwrap(),
            _ => {}
        }
        let trashed_at = b.svc.read(&id).await.unwrap().trashed_at;
        let content = b.stored(&id).await;
        let versions = b.commits(&id).await.len();
        a.clock.advance(chrono::Duration::minutes(1));
        let (marker, _) = purged_on_a(&a, &id).await;
        b.svc.apply_purge_marker(marker).await.unwrap();
        assert!(!b.svc.is_purged(&id).await.unwrap(), "{case}: never purged on B");
        let v = b.svc.read(&id).await.unwrap_or_else(|e| panic!("{case}: B's copy is there ({e})"));
        assert!(v.trashed_at.is_some(), "{case}: in B's trash");
        if case == "trashed" {
            assert_eq!(v.trashed_at, trashed_at, "{case}: kept as it was");
        }
        assert_eq!(b.stored(&id).await, content, "{case}: content kept");
        assert!(b.commits(&id).await.len() >= versions, "{case}: versions kept");
    }
}

// Was `g27_a_copy_that_differs_is_kept_and_comes_back_to_the_purging_device`
// (asserted: `KeptDiffers` for a copy edited on B, its return to A as a new
// arrival, and a second purge of identical copies removing both). Now L4 +
// L6: what decides is the stamp — B's copy, edited after the deletion, stays
// live and comes back to A; a second deletion on A, after it came back, sends
// B's copy to B's trash (never purged).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g27_a_copy_edited_after_the_deletion_is_kept_and_comes_back_to_the_purging_device() {
    let a = Stack::with_device("g27-diff-a", "laptop").await;
    let b = Stack::with_device("g27-diff-b", "phone").await;
    let id = a.create_text("Doc", "first").await;
    send(&a, &b, &id).await;
    let (marker, deletion) = purged_on_a(&a, &id).await;
    b.clock.advance(chrono::Duration::minutes(1));
    b.editor_save(&id, 1, v2(vec![text_block(1, "edited on the phone")])).await.unwrap();
    assert!(current_stamp(&b, &id).await > deletion, "setup: B's edit is after the deletion");
    b.svc.apply_purge_marker(marker).await.unwrap();
    assert_eq!(texts(&b.stored(&id).await.1), vec!["edited on the phone"]);
    assert!(b.svc.read(&id).await.unwrap().trashed_at.is_none(), "kept live");
    // It travels back: recreated on the purging device as a new arrival.
    let out = send(&b, &a, &id).await;
    assert!(matches!(out.applied, WireApplied::Applied(RemoteOutcome::Created)), "{:?}", out.applied);
    assert!(!a.svc.is_purged(&id).await.unwrap(), "the marker is dropped there");
    assert_eq!(texts(&a.stored(&id).await.1), vec!["edited on the phone"]);
    assert_eq!(a.commits(&id).await.len(), 1, "its own first version");
    // Deleted and purged again on A: B's copy goes to B's trash, not further.
    a.clock.advance(chrono::Duration::minutes(2));
    let (marker, _) = purged_on_a(&a, &id).await;
    b.svc.apply_purge_marker(marker).await.unwrap();
    assert!(!b.svc.is_purged(&id).await.unwrap(), "never purged on B");
    assert!(b.svc.read(&id).await.unwrap().trashed_at.is_some(), "in B's trash");
}

// Was `g27_the_purged_version_never_comes_back_from_any_peer` (asserted: a
// state with the purged fingerprint is ignored). Now L6: a live state older
// than the deletion is ignored — here B's untouched copy, the same text.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g27_a_state_older_than_the_deletion_never_brings_the_id_back() {
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

// Was `g27_a_device_without_the_document_records_the_marker` (asserted:
// `PurgeApplied::Recorded`, the marker re-advertised equal to the received
// one, the purged version refused later). Now L4 ("No copy of X here: the
// marker is recorded (id, S, no fingerprint)") + L6.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g27_a_device_without_the_document_records_the_deletion() {
    let a = Stack::with_device("g27-none-a", "laptop").await;
    let c = Stack::with_device("g27-none-c", "tablet").await;
    let id = a.create_text("Doc", "text").await;
    let state = a.svc.stored_state(&id).await.unwrap();
    let (marker, deletion) = purged_on_a(&a, &id).await;
    c.svc.apply_purge_marker(marker.clone()).await.unwrap();
    assert!(c.svc.is_purged(&id).await.unwrap());
    assert!(c.svc.purge_markers().await.unwrap().contains(&marker), "re-advertised as received");
    let disk = c.raw.get_purge_marker(&id).await.unwrap().expect("a marker row");
    assert_eq!(disk.fingerprint, None, "no fingerprint recorded");
    assert_eq!(
        disk.stamp.as_deref(),
        Some(format!("{}:{}:{}", deletion.wall_ms, deletion.counter, deletion.device).as_str())
    );
    // The purged version (older than the deletion), arriving later from a
    // third device, is refused.
    let out = c.svc.apply_wire(state.document, state.meta).await.unwrap();
    assert_eq!(out.applied, WireApplied::Ignored);
    assert!(c.svc.read(&id).await.is_err());
}
