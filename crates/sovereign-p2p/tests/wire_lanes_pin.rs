//! v0.0.12 lanes lot, item 15 — **M4: the pin stays on each device**
//! (`doc/plans/V0012-lot-lanes.md`).
//!
//! Céline, 2026-10-24: "Propre à chaque appareil". "The pin is a display
//! preference of this device: not stamped, not synced, never part of the
//! compared state. `set_document_pinned` stays a direct store write. A test
//! pins that a sync neither sends nor changes it."
//!
//! Two devices over the real sealed, signed envelopes, without the network
//! (`wire_common`). Uses only what exists; written before the code by another
//! author, the implementer does not edit this file.

mod wire_common;

use sovereign_db::GraphDB;
use wire_common::*;

async fn pinned(d: &Device, id: &str) -> bool {
    d.edb.get_document(id).await.expect("stored").pinned
}

fn quiet(plan: &sovereign_p2p::wire::DocumentPlan) -> bool {
    plan.fetch.is_empty() && plan.push.is_empty() && plan.concurrent.is_empty()
}

/// M4: a pin is not sent — pinning is no difference between two devices
/// (never part of the compared state), and the other device stays unpinned.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn m4_a_pin_is_not_sent_and_is_no_difference() {
    let (a, b) = (device("m4-send-a", 71).await, device("m4-send-b", 72).await);
    pair(&a, &b);
    let id = create(&a, "Pinned here", "text").await;
    settle(&a, &b, 4).await;
    let before = a.docs.stored_state(&id).await.unwrap();

    a.edb.set_document_pinned(&id, true).await.expect("pinned on the laptop");
    let after = a.docs.stored_state(&id).await.unwrap();
    assert_eq!(after.meta.stamp, before.meta.stamp, "a pin is not stamped");
    assert_eq!(after.normalized_hash, before.normalized_hash, "nor part of the compared state");

    let plan = round(&a, &b).await;
    assert!(quiet(&plan), "a pin alone gives the sync nothing to exchange: {plan:?}");
    settle(&a, &b, 4).await;
    assert!(pinned(&a, &id).await, "pinned on the laptop");
    assert!(!pinned(&b, &id).await, "not on the phone");
}

/// M4: a sync never changes the pin — a later state arriving from
/// the other device leaves each device's pin as it is, pinned or not.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn m4_a_sync_does_not_change_the_pin_on_either_device() {
    let (a, b) = (device("m4-keep-a", 73).await, device("m4-keep-b", 74).await);
    pair(&a, &b);
    let id = create(&a, "Note", "start").await;
    settle(&a, &b, 4).await;

    a.edb.set_document_pinned(&id, true).await.unwrap();
    // The phone writes; its state replaces the laptop's.
    edit(&b, &id, "phone edit").await;
    settle(&a, &b, 4).await;
    assert_eq!(stored_text(&a, &id).await, vec!["phone edit".to_string()], "setup: the phone's edit arrived");
    assert!(pinned(&a, &id).await, "the laptop's pin survives a state arriving from the phone");
    assert!(!pinned(&b, &id).await, "the phone stays unpinned");

    // Now the phone pins and the laptop unpins; the laptop writes.
    b.edb.set_document_pinned(&id, true).await.unwrap();
    a.edb.set_document_pinned(&id, false).await.unwrap();
    edit(&a, &id, "laptop edit").await;
    settle(&a, &b, 4).await;
    assert_eq!(stored_text(&b, &id).await, vec!["laptop edit".to_string()], "setup: the laptop's edit arrived");
    assert!(pinned(&b, &id).await, "the phone's pin survives a state arriving from the laptop");
    assert!(!pinned(&a, &id).await, "the laptop stays unpinned");
}
