//! v0.0.12 item 3 — purges are local, on the wire (L3, L5, L7, L13, L14, and
//! the dedupe of `apply_purge_markers`).
//!
//! Contract: `doc/plans/V0012-item3-local-purges.md` (main `0e4eb0d`; L13
//! changed and L14 added at `3e2359d`); spec
//! v2.0.47 ch. 05 *Deletions and purges*, *Backup restore*; review of v0.0.11
//! findings 2 and 13. Written before the code, by another author: the
//! implementer makes these pass without editing this file.
//!
//! Devices and rounds are the C-WIRE harness (`wire_common`): real stores,
//! real sealed envelopes, no network. Each device's clock is its own and only
//! moves when a test moves it.

mod wire_common;

use chrono::Duration;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sovereign_db::schema::PurgeMarker;
use sovereign_docs::{DocumentV2, SourceForm, Stamp};
use sovereign_p2p::protocol::manifest::{PurgeMarkerEntry, WireStamp};
use sovereign_p2p::wire::plan_documents;
use wire_common::*;

fn wire(s: &Stamp) -> WireStamp {
    WireStamp { wall_ms: s.wall_ms, counter: s.counter, device: s.device.clone() }
}

fn enc(s: &Stamp) -> String {
    format!("{}:{}:{}", s.wall_ms, s.counter, s.device)
}

async fn stamp_of(d: &Device, id: &str) -> Stamp {
    d.docs.stored_state(id).await.unwrap().meta.stamp.unwrap()
}

/// Trashes and purges `id` on `d`; returns the deletion's stamp.
async fn delete_and_purge(d: &Device, id: &str) -> Stamp {
    d.docs.trash(id).await.unwrap();
    let deletion = stamp_of(d, id).await;
    d.docs.purge_from_trash(id).await.unwrap();
    deletion
}

async fn disk_marker(d: &Device, id: &str) -> Option<PurgeMarker> {
    d.raw.get_purge_marker(id).await.unwrap()
}

async fn advertised_stamp(d: &Device, id: &str) -> Option<Stamp> {
    d.docs.purge_markers().await.unwrap().into_iter().find(|m| m.id == id).map(|m| m.stamp)
}

/// The purge fingerprint as v0.0.11 computed it (`sovereign-docs/src/wire.rs`
/// at `acb3f8d`), copied so this file does not depend on `purge_fingerprint`,
/// which item 3 removes. What a v0.0.11 peer puts in its manifest entry.
fn v0011_fingerprint(id: &str, title: &str, content: &str) -> String {
    fn field(h: &mut Sha256, s: &str) {
        h.update((s.len() as u64).to_le_bytes());
        h.update(s.as_bytes());
    }
    let normalized = match DocumentV2::read_seeded(content, id) {
        Ok((doc, form)) if !matches!(form, SourceForm::NewerVersion { .. }) => doc.to_json_string(),
        _ => content.to_string(),
    };
    let mut h = Sha256::new();
    field(&mut h, "sovereign.purge/v1");
    field(&mut h, title);
    field(&mut h, &normalized);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

// ======================================================================= L3

/// The manifest types of v0.0.11 that matter here, copied from
/// `sovereign-p2p/src/protocol/manifest.rs` at `acb3f8d` (the v0.0.11 code):
/// `PurgeMarkerEntry.fingerprint` is a **required** `String`. The other fields
/// of the v0.0.11 `SyncManifest` are kept as opaque values (all of them are
/// `#[serde(default)]` there but `device_id` and `generated_at`), so a decode
/// error can only come from what this item changes.
#[derive(Deserialize)]
#[allow(dead_code)]
struct WireStampV0011 {
    wall_ms: i64,
    counter: u32,
    device: String,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct PurgeMarkerEntryV0011 {
    doc_id: String,
    stamp: WireStampV0011,
    fingerprint: String,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct SyncManifestV0011 {
    device_id: String,
    generated_at: String,
    #[serde(default)]
    documents: Vec<serde_json::Value>,
    #[serde(default)]
    threads: Vec<serde_json::Value>,
    #[serde(default)]
    contacts: Vec<serde_json::Value>,
    #[serde(default)]
    wire_version: Option<u32>,
    #[serde(default)]
    document_wire: Vec<serde_json::Value>,
    #[serde(default)]
    purge_markers: Vec<PurgeMarkerEntryV0011>,
}

// L3 — "The entry keeps the `fingerprint` field a v0.0.11 build requires,
// sent empty, so a v0.0.11 device still decodes the manifest (threads,
// contacts and documents keep syncing) and ignores the marker (an empty
// fingerprint never matches)." The manifest is JSON under the pair key
// (`SyncManifest::encrypt`); this decodes the JSON as v0.0.11 would.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn l3_a_v0011_device_decodes_a_manifest_carrying_a_marker_with_an_empty_fingerprint() {
    let a = device("l3-v0011", 61).await;
    let kept = create(&a, "Kept", "kept").await;
    let gone = create(&a, "Gone", "gone").await;
    a.docs.thread_create("A thread", "").await.unwrap();
    delete_and_purge(&a, &gone).await;
    let m = manifest(&a).await;

    let json = serde_json::to_value(&m).unwrap();
    let entry = json["purge_markers"]
        .as_array()
        .expect("purge_markers")
        .iter()
        .find(|e| e["doc_id"] == gone.as_str())
        .expect("the marker is advertised");
    assert_eq!(entry.get("fingerprint"), Some(&serde_json::json!("")), "the field is present, and empty");

    let bytes = serde_json::to_vec(&m).unwrap();
    let old: SyncManifestV0011 = serde_json::from_slice(&bytes).expect("a v0.0.11 device decodes the manifest");
    assert!(old.purge_markers.iter().any(|e| e.doc_id == gone && e.fingerprint.is_empty()));
    assert!(!old.threads.is_empty(), "threads still reach it");
    assert!(old.document_wire.iter().any(|e| e["doc_id"] == kept.as_str()), "and the documents' entries");
}

// ======================================================================= dedupe

// `apply_purge_markers` dedupes by id **and stamp**, not by fingerprint (the
// item's brief; L4 "If a marker for X is already here, the later stamp is
// kept"). Every marker now travels with an empty fingerprint, so a dedupe by
// (id, fingerprint) would drop every later deletion of an id already known
// here.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn dedupe_a_later_deletion_of_a_known_id_is_applied_and_a_repeat_is_not() {
    let a = device("dedupe-a", 63).await;
    let b = device("dedupe-b", 64).await;
    let id = create(&a, "Doc", "text").await;
    delete_and_purge(&a, &id).await;
    // Whatever stamp the local marker has (L2 fixes it elsewhere), the test
    // starts from it.
    let first = advertised_stamp(&a, &id).await.expect("setup: a marker is advertised");

    let later = Stamp { wall_ms: first.wall_ms + 60_000, counter: 0, device: b.peer.to_string() };
    let mut mb = manifest(&b).await;
    mb.purge_markers.push(PurgeMarkerEntry { doc_id: id.clone(), stamp: wire(&later), fingerprint: String::new() });
    a.sync.apply_purge_markers(&mb).await.unwrap();
    assert_eq!(advertised_stamp(&a, &id).await, Some(later.clone()), "the later deletion is applied: its stamp is kept");
    assert_eq!(disk_marker(&a, &id).await.unwrap().stamp.as_deref(), Some(enc(&later).as_str()));

    // The same (id, stamp) again: nothing to do.
    let changed = a.sync.apply_purge_markers(&mb).await.unwrap();
    assert_eq!(changed, 0, "an (id, stamp) already known changes nothing");
    // An earlier one for the same id: the later stamp stays.
    let mut older = manifest(&b).await;
    older.purge_markers.push(PurgeMarkerEntry { doc_id: id.clone(), stamp: wire(&first), fingerprint: String::new() });
    a.sync.apply_purge_markers(&older).await.unwrap();
    assert_eq!(advertised_stamp(&a, &id).await, Some(later));
}

// ======================================================================= L5

// L5 — "A v0.0.11 purge marker is a deletion. A marker that carries a
// fingerprint is handled by L4 exactly; its fingerprint is neither compared
// nor stored." A v0.0.11 peer's entry carries the fingerprint of the very
// copy B holds (the case v0.0.11 purged): B's older live copy goes to B's
// trash; a device without a copy records the deletion, without the
// fingerprint.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn l5_a_v0011_marker_with_a_matching_fingerprint_is_read_as_a_deletion() {
    let old = device("l5-v0011-peer", 65).await;
    let b = device("l5-b", 66).await;
    let c = device("l5-c", 67).await;
    let id = create(&b, "Doc", "the same text").await;
    let held = b.edb.get_document(&id).await.unwrap();
    let versions = b.edb.list_document_commits(&id).await.unwrap().len();
    let deletion = Stamp { wall_ms: stamp_of(&b, &id).await.wall_ms + 60_000, counter: 0, device: old.peer.to_string() };
    let mut m = manifest(&old).await; // as a v0.0.11 build sends it: wire version, entries with fingerprints
    m.purge_markers.push(PurgeMarkerEntry {
        doc_id: id.clone(),
        stamp: wire(&deletion),
        fingerprint: v0011_fingerprint(&id, &held.title, &held.content),
    });

    b.sync.apply_purge_markers(&m).await.unwrap();
    assert!(!b.docs.is_purged(&id).await.unwrap(), "never purged");
    let v = b.docs.read(&id).await.expect("B's copy is there");
    assert!(v.trashed_at.is_some(), "in B's trash");
    assert_eq!(stored_text(&b, &id).await, vec!["the same text"], "content kept");
    assert!(b.edb.list_document_commits(&id).await.unwrap().len() >= versions, "versions kept");
    assert_eq!(stamp_of(&b, &id).await, deletion, "its state takes the deletion's stamp (L4)");

    c.sync.apply_purge_markers(&m).await.unwrap();
    assert!(c.docs.is_purged(&id).await.unwrap(), "recorded where there is no copy");
    let disk = disk_marker(&c, &id).await.expect("a marker row");
    assert_eq!(disk.fingerprint, None, "the fingerprint is not stored");
    assert_eq!(disk.stamp.as_deref(), Some(enc(&deletion).as_str()));
}

// ======================================================================= L7

// L7 — finding 2, over the wire: "X deleted on A; the deletion reaches B; X
// restored on B; before that restore reaches A, the trash is emptied on A.
// A's deletion reaches B: B's copy stays live, History included. B's live
// state reaches A: X is back on A, live."
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn l7_finding_2_over_the_wire_a_restore_on_b_survives_and_comes_back_to_a() {
    let (a, b) = (device("l7w-a", 68).await, device("l7w-b", 69).await);
    pair(&a, &b);
    let id = create(&a, "X", "written on A").await;
    settle(&a, &b, 4).await;
    b.clock.advance(Duration::minutes(1));
    edit(&b, &id, "edited on the phone").await;
    settle(&a, &b, 4).await;
    assert_eq!(stored_text(&a, &id).await, vec!["edited on the phone"], "setup");

    // 1. Deleted on A; the deletion reaches B.
    a.clock.advance(Duration::minutes(2));
    a.docs.trash(&id).await.unwrap();
    settle(&a, &b, 4).await;
    assert!(b.docs.read(&id).await.unwrap().trashed_at.is_some(), "setup: in B's trash");
    // 2. Restored on B, not yet synced.
    b.clock.advance(Duration::minutes(3));
    b.docs.restore_from_trash(&id).await.unwrap();
    let before = history(&b, &id).await;
    // 3. The trash emptied on A.
    a.docs.empty_trash().await.unwrap();
    // 4. A's deletion reaches B first.
    b.sync.apply_purge_markers(&manifest(&a).await).await.unwrap();
    assert!(b.docs.read(&id).await.unwrap().trashed_at.is_none(), "B's copy stays live");
    assert_eq!(stored_text(&b, &id).await, vec!["edited on the phone"]);
    let now = history(&b, &id).await;
    for h in &before {
        assert!(now.contains(h), "B's History kept");
    }
    // 5. Then B's live state reaches A.
    a.clock.advance(Duration::minutes(5));
    settle(&a, &b, 6).await;
    let back = a.docs.read(&id).await.expect("X is back on A");
    assert!(back.trashed_at.is_none(), "live on A");
    assert_eq!(stored_text(&a, &id).await, vec!["edited on the phone"]);
    assert!(!a.docs.is_purged(&id).await.unwrap());
    assert!(b.docs.read(&id).await.unwrap().trashed_at.is_none(), "still live on B");
}

// ====================================================================== L13

// L13 — "A guardian backup carries this device's markers (id, stamp), without
// fingerprints": the snapshot a device builds holds no fingerprint.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn l13_a_backup_snapshot_carries_markers_without_fingerprints() {
    let src = device("l13w-src", 70).await;
    let gone = create(&src, "Gone", "erased").await;
    let deletion = delete_and_purge(&src, &gone).await;
    let snapshot = sovereign_p2p::backup::build_snapshot(src.edb.as_ref(), "l13w-src").await.unwrap();
    let m = snapshot.purge_markers.iter().find(|m| m.document_id == gone).expect("the snapshot carries the marker");
    assert_eq!(m.stamp.as_deref(), Some(enc(&deletion).as_str()), "with the deletion's stamp");
    assert_eq!(m.fingerprint, None, "without a fingerprint");
    let json = serde_json::to_string(&snapshot.purge_markers).unwrap();
    assert!(!json.contains("erased") && !json.contains("Gone"), "no title or content");
}

// L13 — "restored onto an empty workspace they act as L6 says … A backup taken
// by v0.0.11 may hold markers with fingerprints: they are restored without
// them." Two snapshot rows: one as v0.0.11 stored it (with a fingerprint), one
// as this build stores it (without; v0.0.11's restore dropped such a row).
// Both are restored, without fingerprints, and act as deletions: an older
// live copy is ignored, a later one comes back.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn l13_a_backups_markers_with_or_without_fingerprints_are_restored_as_deletions() {
    let src = device("l13w-v0011-src", 71).await;
    let peer = device("l13w-v0011-peer", 72).await;
    pair(&src, &peer);
    let with = create(&src, "With", "with").await;
    let without = create(&src, "Without", "without").await;
    settle(&src, &peer, 4).await; // the peer keeps copies, older than the deletions
    src.clock.advance(Duration::minutes(1));
    let d_with = delete_and_purge(&src, &with).await;
    let d_without = delete_and_purge(&src, &without).await;
    let mut snapshot = sovereign_p2p::backup::build_snapshot(src.edb.as_ref(), "l13w-v0011-src").await.unwrap();
    for m in snapshot.purge_markers.iter_mut() {
        m.fingerprint = if m.document_id == with { Some("77".repeat(32)) } else { None };
    }

    let n = device("l13w-v0011-new", 73).await;
    n.clock.advance(Duration::minutes(1));
    sovereign_p2p::wire::restore_backup(&n.docs, n.edb.as_ref(), &snapshot).await.unwrap();
    for (id, deletion) in [(&with, &d_with), (&without, &d_without)] {
        assert!(n.docs.is_purged(id).await.unwrap(), "{id}: restored as a deletion");
        let disk = disk_marker(&n, id).await.expect("a marker row");
        assert_eq!(disk.fingerprint, None, "{id}: restored without a fingerprint");
        assert_eq!(disk.stamp.as_deref(), Some(enc(deletion).as_str()), "{id}: with its stamp");
    }
    // The peer's copies (older than the deletions) do not bring them back.
    pair(&n, &peer);
    settle(&n, &peer, 4).await;
    assert!(n.docs.is_purged(&with).await.unwrap() && n.docs.is_purged(&without).await.unwrap(), "older copies ignored");
    // The restored deletions reached the peer as deletions (L4): its older
    // copies went to its trash. Restored there and edited, later than the
    // deletion, one comes back, live.
    assert!(peer.docs.read(&without).await.unwrap().trashed_at.is_some(), "the peer's copy is in its trash");
    peer.clock.advance(Duration::minutes(5));
    peer.docs.restore_from_trash(&without).await.unwrap();
    edit(&peer, &without, "edited after the deletion").await;
    n.clock.advance(Duration::minutes(5));
    settle(&n, &peer, 4).await;
    assert!(!n.docs.is_purged(&without).await.unwrap(), "a later live state brings it back");
    assert_eq!(stored_text(&n, &without).await, vec!["edited after the deletion"]);
}

// ====================================================================== L14

// L14 (added 2026-10-24, main `3e2359d`, on the test author's question 7) —
// "When this device holds a marker for X, the document plan does not fetch a
// peer's X whose manifest stamp is not later than the marker's (L6 would
// ignore it). A peer's X with a later stamp is fetched and judged by L6."
//
// Not later: B's untouched copy (earlier), and B's copy once A's deletion has
// reached it (in B's trash, at the deletion's own stamp: equal).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn l14_the_plan_does_not_fetch_a_copy_not_later_than_the_marker() {
    let (a, b) = (device("l14-skip-a", 74).await, device("l14-skip-b", 75).await);
    pair(&a, &b);
    let id = create(&a, "Doc", "text").await;
    settle(&a, &b, 4).await;
    a.clock.advance(Duration::minutes(1));
    delete_and_purge(&a, &id).await;
    let marker = advertised_stamp(&a, &id).await.expect("setup: a marker");

    // Earlier: B never touched its copy.
    assert!(stamp_of(&b, &id).await < marker, "setup: B's copy is older");
    let plan = plan_documents(&manifest(&a).await, &manifest(&b).await);
    assert!(!plan.fetch.contains(&id), "an older copy is not fetched: {:?}", plan.fetch);
    assert!(!plan.concurrent.contains(&id), "nor exchanged as concurrent");

    // Equal: B's copy at the deletion's stamp, in B's trash.
    b.clock.advance(Duration::minutes(2));
    b.sync.apply_purge_markers(&manifest(&a).await).await.unwrap();
    assert!(b.docs.read(&id).await.unwrap().trashed_at.is_some(), "setup: in B's trash");
    let plan = plan_documents(&manifest(&a).await, &manifest(&b).await);
    assert!(!plan.fetch.contains(&id), "a copy at the marker's stamp is not fetched: {:?}", plan.fetch);
    assert!(!plan.concurrent.contains(&id));
}

// L14 — "A peer's X with a later stamp is fetched and judged by L6": live, it
// comes back.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn l14_the_plan_fetches_a_copy_later_than_the_marker() {
    let (a, b) = (device("l14-fetch-a", 76).await, device("l14-fetch-b", 77).await);
    pair(&a, &b);
    let id = create(&a, "Doc", "text").await;
    settle(&a, &b, 4).await;
    delete_and_purge(&a, &id).await;
    let marker = advertised_stamp(&a, &id).await.expect("setup: a marker");
    b.clock.advance(Duration::minutes(1));
    edit(&b, &id, "edited after the deletion").await;
    assert!(stamp_of(&b, &id).await > marker, "setup: B's copy is later");
    let plan = plan_documents(&manifest(&a).await, &manifest(&b).await);
    assert!(plan.fetch.contains(&id), "a later copy is fetched: {:?}", plan.fetch);
    a.clock.advance(Duration::minutes(2));
    settle(&a, &b, 4).await;
    assert!(!a.docs.is_purged(&id).await.unwrap(), "judged by L6: a later live state comes back");
    assert_eq!(stored_text(&a, &id).await, vec!["edited after the deletion"]);
}

// L14 — the observation behind it: after a purge, two devices settle. The
// device that purged initiates the rounds (it is its plan that would fetch).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn l14_two_devices_settle_after_a_purge() {
    let (a, b) = (device("l14-settle-a", 78).await, device("l14-settle-b", 79).await);
    pair(&a, &b);
    let id = create(&a, "Doc", "text").await;
    settle(&a, &b, 4).await;
    a.clock.advance(Duration::minutes(1));
    delete_and_purge(&a, &id).await;
    b.clock.advance(Duration::minutes(2));
    let worked = settle(&a, &b, 6).await;
    assert!(worked < 6, "the rounds converge after a purge ({worked} of 6 had document work)");
    assert_eq!(settle(&a, &b, 6).await, 0, "then nothing is left to exchange");
    assert!(b.docs.read(&id).await.unwrap().trashed_at.is_some(), "B's copy is in B's trash");
    assert!(a.docs.is_purged(&id).await.unwrap(), "A's purge stays");
}

// CONTRACT-QUESTION (L14, the other direction): L14 names the fetch only. When
// the device holding the copy initiates the round, its plan **pushes** X to
// the device holding the marker, which ignores it, at every round until the
// copy's own 30 days: the same waste, from the other side. Best reading: the
// plan does not push X to a peer whose manifest carries a marker for X with a
// stamp not earlier than the local X's (a manifest carries its markers,
// `SyncManifest::purge_markers`), so the two devices settle whoever initiates.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn l14_two_devices_settle_after_a_purge_whoever_initiates() {
    let (a, b) = (device("l14-settle-rev-a", 80).await, device("l14-settle-rev-b", 81).await);
    pair(&a, &b);
    let id = create(&a, "Doc", "text").await;
    settle(&a, &b, 4).await;
    a.clock.advance(Duration::minutes(1));
    delete_and_purge(&a, &id).await;
    b.clock.advance(Duration::minutes(2));
    let plan = plan_documents(&manifest(&b).await, &manifest(&a).await);
    assert!(!plan.push.contains(&id), "B does not push an older copy to the device that purged it: {:?}", plan.push);
    let worked = settle(&b, &a, 6).await;
    assert!(worked < 6, "the rounds converge with B initiating ({worked} of 6 had document work)");
    assert_eq!(settle(&b, &a, 6).await, 0);
    assert!(b.docs.read(&id).await.unwrap().trashed_at.is_some(), "B's copy is in B's trash");
    assert!(a.docs.is_purged(&id).await.unwrap());
}
