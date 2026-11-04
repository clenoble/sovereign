//! v0.0.12 item 3 — purges are local (L1–L9, L13).
//!
//! Contract: `doc/plans/V0012-item3-local-purges.md` (main `0e4eb0d`); the
//! rule is Céline's (2026-10-23), spec v2.0.47 ch. 05 *Deletions and purges*,
//! ch. 03 G13, G19, G27 (as amended), G28. Review of v0.0.11
//! (`doc/plans/review-2026-10-22-cloud/review-v0.0.11.md`), findings 2, 3, 13.
//!
//! Written before the code, by another author: whoever implements item 3
//! makes these pass **without editing this file**; a test that looks wrong is
//! reported, not worked around.
//!
//! Two stacks stand for two devices; what travels between them is what the
//! wire carries: a stored state (`stored_state` → `apply_wire`) and a purge
//! marker (`purge_markers` → `apply_purge_marker`). Each stack's clock is its
//! own and never moves by itself.
//!
//! This file does not name the fingerprint at all: it compiles whether
//! `PurgeMarkerWire` keeps a `fingerprint` field or loses it. Markers are
//! taken from a device's own `purge_markers()`, and only their `id` and
//! `stamp` are read or changed. What is on disk is read from the raw store
//! (`sovereign_db::schema::PurgeMarker`, whose `fingerprint: Option<String>`
//! field this file expects to stay, so a v0.0.11 row can still be read and
//! erased).
//!
//! L10–L11 are in `item3_trash_since.rs`, L12 in `item3_no_fingerprint.rs`,
//! the outcomes of `apply_purge_marker` in `item3_purge_outcomes.rs`.

mod common;

use chrono::{DateTime, Duration, Utc};
use common::wire::*;
use common::*;
use sha2::{Digest, Sha256};
use sovereign_db::schema::{PurgeMarker, RelationType};
use sovereign_docs::{
    list_trash_named, purge_marker_wire, purge_tick, BackupDocument, ChangeKind, DocumentV2, Provenance,
    PurgeMarkerWire, RemoteOutcome, SourceForm, Stamp, Trust, WireApplied, WireMeta,
};

// ------------------------------------------------------------------ helpers --

/// A stamp as stored (`wall_ms:counter:device`, schema doc comment).
fn enc(s: &Stamp) -> String {
    format!("{}:{}:{}", s.wall_ms, s.counter, s.device)
}

/// A stamp `ms` milliseconds after `s` (same device).
fn after(s: &Stamp, ms: i64) -> Stamp {
    Stamp { wall_ms: s.wall_ms + ms, counter: 0, device: s.device.clone() }
}

/// The marker `s` advertises for `id` (what the manifest carries).
async fn marker_of(s: &Stack, id: &str) -> PurgeMarkerWire {
    s.svc
        .purge_markers()
        .await
        .unwrap()
        .into_iter()
        .find(|m| m.id == id)
        .unwrap_or_else(|| panic!("{}: no marker advertised for {id}", s.device))
}

async fn advertises_marker(s: &Stack, id: &str) -> bool {
    s.svc.purge_markers().await.unwrap().iter().any(|m| m.id == id)
}

/// The marker row as it is on disk (raw store, below the encrypted layer).
async fn disk_marker(s: &Stack, id: &str) -> Option<PurgeMarker> {
    s.raw.get_purge_marker(id).await.unwrap()
}

/// Everything of `id` on `s` that a purge would remove, as counts and flags.
#[derive(Debug, PartialEq)]
struct Held {
    row: bool,
    versions: usize,
    key: bool,
    edges: usize,
    content: (String, String),
}

async fn held(s: &Stack, id: &str) -> Held {
    let edges = s
        .raw
        .list_all_relationships()
        .await
        .unwrap()
        .iter()
        .filter(|r| {
            let f = r.in_.as_ref().map(sovereign_db::schema::thing_to_raw);
            let t = r.out.as_ref().map(sovereign_db::schema::thing_to_raw);
            f.as_deref() == Some(id) || t.as_deref() == Some(id)
        })
        .count();
    Held {
        row: s.raw.get_document(id).await.is_ok(),
        versions: s.raw_commits(id).await.len(),
        key: s.doc_keys.read().await.contains(id),
        edges,
        content: s.edb.get_document(id).await.map(|d| (d.title, d.content)).unwrap_or_default(),
    }
}

/// The purge fingerprint as v0.0.11 computed it (`wire.rs` at `acb3f8d`): a
/// copy, so this file does not depend on `purge_fingerprint`, which item 3
/// removes (L12). Used only to build what a v0.0.11 device stored or sent.
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

/// A raw marker row, as some build stored it.
fn row(id: &str, stamp: Option<&Stamp>, fingerprint: Option<String>, at: DateTime<Utc>) -> PurgeMarker {
    PurgeMarker {
        id: None,
        document_id: id.to_string(),
        purged_at: at,
        complete: true,
        stamp: stamp.map(enc),
        fingerprint,
        revoked: false,
    }
}

/// The three purges (L1: "emptying the trash, purging one item, or the 30-day
/// purge").
#[derive(Clone, Copy, Debug)]
enum How {
    EmptyTrash,
    PurgeOne,
    ThirtyDays,
}

const HOWS: [How; 3] = [How::EmptyTrash, How::PurgeOne, How::ThirtyDays];

/// Deletes `id` on `s` and purges it the way `how` says; returns the stamp of
/// the deletion (the trashed state's stamp, read before the purge).
async fn delete_and_purge(s: &Stack, id: &str, how: How) -> Stamp {
    if matches!(how, How::ThirtyDays) {
        // The tick a host runs at unlock, before anything is trashed (L10).
        purge_tick(&s.svc).await.unwrap();
    }
    s.svc.trash(id).await.unwrap();
    let deletion = current_stamp(s, id).await;
    match how {
        How::EmptyTrash => {
            s.svc.empty_trash().await.unwrap();
        }
        How::PurgeOne => {
            s.svc.purge_from_trash(id).await.unwrap();
        }
        How::ThirtyDays => {
            s.clock.advance(Duration::days(31));
            let r = purge_tick(&s.svc).await.unwrap();
            assert!(r.documents.contains(&id.to_string()), "precondition: the 30-day purge took {id}");
        }
    }
    assert!(s.svc.is_purged(id).await.unwrap(), "precondition: {id} purged on {}", s.device);
    deletion
}

/// A document written on `a` and received on `b`; the two devices hold the
/// same state.
async fn shared_doc(a: &Stack, b: &Stack, title: &str, text: &str) -> String {
    let id = a.create_text(title, text).await;
    let out = send(a, b, &id).await;
    assert!(matches!(out.applied, WireApplied::Applied(RemoteOutcome::Created)), "setup: {:?}", out.applied);
    id
}

// ======================================================================= L1

/// What device B holds of the document when A's purge reaches it.
#[derive(Clone, Copy, Debug)]
enum OnB {
    /// Live, the state A had (its stamp earlier than A's deletion).
    LiveSame,
    /// Live, edited on B before A's deletion (stamp earlier, content differs).
    LiveEditedBefore,
    /// Live, edited on B after A's deletion (stamp later).
    LiveEditedAfter,
    /// In B's own trash.
    InTrash,
}

// L1 — "A purge removes nothing elsewhere. Emptying the trash, purging one
// item, or the 30-day purge on device A never removes a document, a version, a
// key or an edge on device B, whatever B holds (live or in its trash, same
// content or not)."
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l1_a_purge_on_one_device_removes_nothing_on_the_other_whatever_it_holds() {
    for how in HOWS {
        for on_b in [OnB::LiveSame, OnB::LiveEditedBefore, OnB::LiveEditedAfter, OnB::InTrash] {
            let case = format!("{how:?}/{on_b:?}");
            let a = Stack::with_device(&format!("l1-a-{case}"), "laptop").await;
            let b = Stack::with_device(&format!("l1-b-{case}"), "phone").await;
            let id = shared_doc(&a, &b, "Doc", "the same text").await;
            // An edge on B, so "never an edge" is checked too.
            let other = b.create_text("Other", "o").await;
            b.edb.create_relationship(&id, &other, RelationType::References, 0.5).await.unwrap();
            match on_b {
                OnB::LiveEditedBefore => {
                    b.clock.advance(Duration::minutes(1));
                    b.editor_save(&id, 1, v2(vec![text_block(1, "edited on the phone first")])).await.unwrap();
                    a.clock.advance(Duration::minutes(2));
                }
                OnB::InTrash => {
                    b.clock.advance(Duration::minutes(1));
                    b.svc.trash(&id).await.unwrap();
                }
                _ => {}
            }
            delete_and_purge(&a, &id, how).await;
            if let OnB::LiveEditedAfter = on_b {
                b.clock.advance(Duration::minutes(1));
                b.editor_save(&id, 1, v2(vec![text_block(1, "edited on the phone after")])).await.unwrap();
            }
            let before = held(&b, &id).await;
            assert!(before.row && before.key && before.versions >= 1 && before.edges == 1, "{case}: setup {before:?}");

            let marker = marker_of(&a, &id).await;
            let _ = b.svc.apply_purge_marker(marker).await.unwrap();

            let now = held(&b, &id).await;
            assert!(now.row, "{case}: B's document row is still there");
            assert!(now.versions >= before.versions, "{case}: no version removed on B ({} → {})", before.versions, now.versions);
            assert!(now.key, "{case}: B's document key is still there");
            assert_eq!(now.edges, before.edges, "{case}: B's edges are still there");
            assert_eq!(now.content, before.content, "{case}: B's content is unchanged");
            assert!(!b.svc.is_purged(&id).await.unwrap(), "{case}: not purged on B");
            assert!(b.svc.read(&id).await.is_ok(), "{case}: still readable on B");
        }
    }
}

// ======================================================================= L2

// L2 — "A purge here records a marker with the document's id and the stamp of
// its deletion: the stamp of the trashed state that was purged (its current
// stamp), never a new stamp taken at purge time. No fingerprint, no title, no
// content." Each of the three purges, with the clock moved between the delete
// and the purge (so a stamp taken at purge time would differ).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l2_the_marker_holds_the_id_and_the_deletion_stamp_and_nothing_else() {
    for how in HOWS {
        let s = Stack::with_device(&format!("l2-{how:?}"), "laptop").await;
        let id = s.create_text("A very private title", "a very private body").await;
        if matches!(how, How::ThirtyDays) {
            purge_tick(&s.svc).await.unwrap();
        }
        s.svc.trash(&id).await.unwrap();
        let deletion = current_stamp(&s, &id).await;
        s.clock.advance(Duration::hours(5));
        match how {
            How::EmptyTrash => {
                s.svc.empty_trash().await.unwrap();
            }
            How::PurgeOne => {
                s.svc.purge_from_trash(&id).await.unwrap();
            }
            How::ThirtyDays => {
                s.clock.advance(Duration::days(30));
                purge_tick(&s.svc).await.unwrap();
            }
        }
        let m = marker_of(&s, &id).await;
        assert_eq!(m.id, id, "{how:?}");
        assert_eq!(m.stamp, deletion, "{how:?}: the deletion's stamp, not one taken at purge time");

        let disk = disk_marker(&s, &id).await.expect("a marker row");
        assert_eq!(disk.stamp.as_deref(), Some(enc(&deletion).as_str()), "{how:?}: stored with the deletion's stamp");
        assert_eq!(disk.fingerprint, None, "{how:?}: no fingerprint on disk (finding 13)");
        let text = format!("{disk:?}");
        assert!(!text.contains("private"), "{how:?}: no title or content in the marker");
    }
}

// L2, a document trashed by a peer's deletion and purged here: the marker's
// stamp is that deletion's stamp (the trashed state's current stamp), which
// this device took from the peer.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l2_a_document_trashed_by_a_peer_and_purged_here_keeps_the_peers_deletion_stamp() {
    let a = Stack::with_device("l2-peer-a", "laptop").await;
    let b = Stack::with_device("l2-peer-b", "phone").await;
    let id = shared_doc(&a, &b, "Doc", "text").await;
    a.svc.trash(&id).await.unwrap();
    let deletion = current_stamp(&a, &id).await;
    let out = send(&a, &b, &id).await;
    assert!(matches!(out.applied, WireApplied::Applied(RemoteOutcome::Trashed)), "setup: {:?}", out.applied);
    b.clock.advance(Duration::hours(1));
    b.svc.empty_trash().await.unwrap();
    assert_eq!(marker_of(&b, &id).await.stamp, deletion);
}

// ======================================================================= L3

// L3 — "The manifest advertises each complete, unrevoked marker with a stamp,
// with or without a fingerprint." At the service: `purge_markers()` is the
// manifest's source. A row without a fingerprint travels (today it does not:
// `purge_marker_wire` requires one); a v0.0.11 row with one travels too; a
// row without a stamp, an incomplete one and a revoked one do not.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l3_every_complete_unrevoked_stamped_marker_travels_with_or_without_a_fingerprint() {
    let s = Stack::new("l3-travels").await;
    let at = s.clock.get();
    let st = stamp(&s, -60, 0, "laptop");
    let without = "document:markerwithoutfp";
    let with = "document:markerwithfp";
    let unstamped = "document:markerunstamped";
    let incomplete = "document:markerincomplete";
    let revoked = "document:markerrevoked";
    s.edb.set_purge_marker(row(without, Some(&st), None, at)).await.unwrap();
    s.edb.set_purge_marker(row(with, Some(&st), Some("ab".repeat(32)), at)).await.unwrap();
    s.edb.set_purge_marker(row(unstamped, None, None, at)).await.unwrap();
    s.edb.set_purge_marker(PurgeMarker { complete: false, ..row(incomplete, Some(&st), None, at) }).await.unwrap();
    s.edb.set_purge_marker(PurgeMarker { revoked: true, ..row(revoked, Some(&st), None, at) }).await.unwrap();

    let ids: Vec<String> = s.svc.purge_markers().await.unwrap().into_iter().map(|m| m.id).collect();
    assert!(ids.contains(&without.to_string()), "a stamped marker without a fingerprint travels: {ids:?}");
    assert!(ids.contains(&with.to_string()), "a v0.0.11 marker with a fingerprint travels: {ids:?}");
    for no in [unstamped, incomplete, revoked] {
        assert!(!ids.contains(&no.to_string()), "{no} must not travel: {ids:?}");
    }
    // As it travels: the id and the stamp as stored.
    let m = s.svc.purge_markers().await.unwrap().into_iter().find(|m| m.id == without).unwrap();
    assert_eq!(m.stamp, st);
}

// L3, the marker this build writes: a purge here is advertised (it has a
// stamp and, under L2, no fingerprint).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l3_a_purge_made_here_is_advertised_without_a_fingerprint_on_disk() {
    let s = Stack::new("l3-own").await;
    let id = s.create_text("Doc", "text").await;
    let deletion = delete_and_purge(&s, &id, How::EmptyTrash).await;
    assert_eq!(marker_of(&s, &id).await.stamp, deletion);
    assert_eq!(disk_marker(&s, &id).await.unwrap().fingerprint, None);
}

// ======================================================================= L4

// L4 — "X live here with a stamp earlier than S: moved to this device's trash,
// its content and History kept, as a peer's deletion does (same event and
// rules); its state's stamp becomes S, so both devices order it alike. It
// stays until this device's own 30 days or a manual purge here."
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l4_a_live_copy_older_than_the_deletion_goes_to_the_trash_with_its_content_and_history() {
    let a = Stack::with_device("l4-older-a", "laptop").await;
    let b = Stack::with_device("l4-older-b", "phone").await;
    let id = shared_doc(&a, &b, "Doc", "first").await;
    // B edits before A deletes: B's copy differs and is older than the deletion.
    b.clock.advance(Duration::minutes(1));
    b.editor_save(&id, 1, v2(vec![text_block(1, "edited on the phone")])).await.unwrap();
    a.clock.advance(Duration::minutes(2));
    let deletion = delete_and_purge(&a, &id, How::EmptyTrash).await;
    let before = held(&b, &id).await;
    let history: Vec<(String, String)> = b.committed_states(&id).await;
    b.clock.advance(Duration::minutes(5));
    b.events.take();

    let marker = marker_of(&a, &id).await;
    assert_eq!(marker.stamp, deletion);
    b.svc.apply_purge_marker(marker).await.unwrap();

    let v = b.svc.read(&id).await.expect("still there");
    assert_eq!(v.trashed_at, Some(b.clock.get()), "in B's trash, dated by B's clock (L8)");
    assert!(b.svc.list(None).await.unwrap().iter().all(|d| d.id != id), "no longer listed");
    assert!(b.svc.list_trash().await.unwrap().iter().any(|e| e.id == id), "in the trash view");
    assert_eq!(b.stored(&id).await, before.content, "content kept");
    assert_eq!(texts(&b.stored(&id).await.1), vec!["edited on the phone"]);
    for state in &history {
        assert!(b.committed_states(&id).await.contains(state), "History kept");
    }
    assert_eq!(current_stamp(&b, &id).await, deletion, "its state's stamp becomes the deletion's");
    assert!(b.events.peek().iter().any(|e| e.id == id && e.kind == ChangeKind::Trashed), "the event of a peer's deletion");
    assert!(!b.events.peek().iter().any(|e| e.kind == ChangeKind::Purged), "never a purge");
    assert!(!b.svc.is_purged(&id).await.unwrap());
    assert!(!advertises_marker(&b, &id).await, "B holds a copy: no marker recorded there");
    // Restorable, as any trash.
    b.svc.restore_from_trash(&id).await.expect("restorable");
    assert_eq!(texts(&b.stored(&id).await.1), vec!["edited on the phone"]);
}

// L4 — "It stays until this device's own 30 days": the trash state it is in
// counts from B's clock, and a manual purge on B is a purge of B's own.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l4_the_trashed_copy_stays_until_this_devices_own_30_days() {
    let a = Stack::with_device("l4-term-a", "laptop").await;
    let b = Stack::with_device("l4-term-b", "phone").await;
    purge_tick(&b.svc).await.unwrap(); // B's unlock tick (L10)
    let id = shared_doc(&a, &b, "Doc", "text").await;
    delete_and_purge(&a, &id, How::EmptyTrash).await;
    b.clock.advance(Duration::days(3));
    b.svc.apply_purge_marker(marker_of(&a, &id).await).await.unwrap();
    assert!(b.svc.read(&id).await.unwrap().trashed_at.is_some(), "setup: in B's trash");
    b.clock.advance(Duration::days(30) - Duration::seconds(1));
    let r = purge_tick(&b.svc).await.unwrap();
    assert!(r.documents.is_empty(), "not a second before B's own 30 days");
    assert!(b.svc.read(&id).await.is_ok());
    b.clock.advance(Duration::seconds(1));
    let r = purge_tick(&b.svc).await.unwrap();
    assert_eq!(r.documents, vec![id.clone()], "purged by B's own term");
}

// L4 — "X live here with a stamp later than S (restored or edited after the
// deletion): kept live." Nothing on B changes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l4_a_live_copy_later_than_the_deletion_is_kept_live_untouched() {
    let a = Stack::with_device("l4-later-a", "laptop").await;
    let b = Stack::with_device("l4-later-b", "phone").await;
    let id = shared_doc(&a, &b, "Doc", "first").await;
    let deletion = delete_and_purge(&a, &id, How::PurgeOne).await;
    b.clock.advance(Duration::minutes(1));
    b.editor_save(&id, 1, v2(vec![text_block(1, "edited after the deletion")])).await.unwrap();
    let before_row = b.stored_doc(&id).await;
    let before_stamp = current_stamp(&b, &id).await;
    assert!(before_stamp > deletion, "setup: B's state is later than the deletion");
    let versions = b.commits(&id).await.len();
    b.events.take();

    b.svc.apply_purge_marker(marker_of(&a, &id).await).await.unwrap();

    assert!(b.svc.read(&id).await.unwrap().trashed_at.is_none(), "kept live");
    let after_row = b.stored_doc(&id).await;
    assert_eq!((after_row.title, after_row.content, after_row.deleted_at), (before_row.title, before_row.content, before_row.deleted_at));
    assert_eq!(current_stamp(&b, &id).await, before_stamp, "its stamp unchanged");
    assert_eq!(b.commits(&id).await.len(), versions);
    assert!(b.events.peek().is_empty(), "nothing happened on B");
    assert!(!advertises_marker(&b, &id).await, "no marker recorded on B");
}

// L4 — "X in this device's trash: kept as it is (its trash date unchanged)."
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l4_a_copy_already_in_this_devices_trash_is_kept_as_it_is() {
    let a = Stack::with_device("l4-trash-a", "laptop").await;
    let b = Stack::with_device("l4-trash-b", "phone").await;
    let id = shared_doc(&a, &b, "Doc", "text").await;
    b.svc.trash(&id).await.unwrap();
    let trashed_at = b.svc.read(&id).await.unwrap().trashed_at;
    let before_row = b.stored_doc(&id).await;
    a.clock.advance(Duration::minutes(1));
    delete_and_purge(&a, &id, How::EmptyTrash).await;
    b.clock.advance(Duration::days(2));
    b.events.take();

    b.svc.apply_purge_marker(marker_of(&a, &id).await).await.unwrap();

    assert_eq!(b.svc.read(&id).await.unwrap().trashed_at, trashed_at, "its trash date unchanged");
    let after_row = b.stored_doc(&id).await;
    assert_eq!(after_row.deleted_at, before_row.deleted_at);
    assert_eq!(after_row.content, before_row.content);
    // Reading settled 2026-10-24 (main `3e2359d`, 9): it keeps its stamp too.
    assert_eq!(after_row.stamp, before_row.stamp, "its stamp unchanged");
    assert!(!b.svc.is_purged(&id).await.unwrap(), "never a purge");
    assert!(!b.events.peek().iter().any(|e| e.kind == ChangeKind::Purged));
    assert!(!advertises_marker(&b, &id).await, "no marker recorded on B");
}

// L4 — "No copy of X here: the marker is recorded (id, S, no fingerprint), so
// a copy older than the deletion arriving later from a third device is
// ignored (L6)."
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l4_without_a_copy_the_deletion_is_recorded_and_an_older_copy_from_a_third_device_is_ignored() {
    let a = Stack::with_device("l4-none-a", "laptop").await;
    let b = Stack::with_device("l4-none-b", "phone").await;
    let c = Stack::with_device("l4-none-c", "tablet").await;
    let id = shared_doc(&a, &b, "Doc", "text").await;
    let deletion = delete_and_purge(&a, &id, How::EmptyTrash).await;

    c.svc.apply_purge_marker(marker_of(&a, &id).await).await.unwrap();
    assert!(c.svc.is_purged(&id).await.unwrap(), "recorded: the id is known as deleted here");
    assert_eq!(marker_of(&c, &id).await.stamp, deletion, "recorded with the deletion's stamp");
    let disk = disk_marker(&c, &id).await.expect("a marker row");
    assert_eq!(disk.stamp.as_deref(), Some(enc(&deletion).as_str()));
    assert_eq!(disk.fingerprint, None, "no fingerprint recorded");

    // B's copy (older than the deletion) reaches C: ignored.
    let out = send(&b, &c, &id).await;
    assert_eq!(out.applied, WireApplied::Ignored);
    assert!(c.svc.read(&id).await.is_err(), "nothing created on C");
}

// L4 — "If a marker for X is already here, the later stamp is kept." In both
// orders of arrival.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l4_two_deletions_of_one_id_keep_the_later_stamp_in_either_order() {
    let a = Stack::with_device("l4-two-a", "laptop").await;
    let id = a.create_text("Doc", "text").await;
    delete_and_purge(&a, &id, How::EmptyTrash).await;
    let earlier = marker_of(&a, &id).await;
    let mut later = earlier.clone();
    later.stamp = Stamp { wall_ms: earlier.stamp.wall_ms + 60_000, counter: 0, device: "phone".into() };

    for (tag, first, second) in [("earlier-first", &earlier, &later), ("later-first", &later, &earlier)] {
        let c = Stack::with_device(&format!("l4-two-c-{tag}"), "tablet").await;
        c.svc.apply_purge_marker(first.clone()).await.unwrap();
        c.svc.apply_purge_marker(second.clone()).await.unwrap();
        assert_eq!(marker_of(&c, &id).await.stamp, later.stamp, "{tag}: the later stamp is kept");
        assert_eq!(disk_marker(&c, &id).await.unwrap().stamp.as_deref(), Some(enc(&later.stamp).as_str()), "{tag}");
    }
}

// L4 — "Never a purge: no version, key or edge is removed by a received
// deletion." The copy here is identical to the one purged elsewhere (the case
// v0.0.11 purged).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l4_a_received_deletion_never_purges_an_identical_copy() {
    let a = Stack::with_device("l4-never-a", "laptop").await;
    let b = Stack::with_device("l4-never-b", "phone").await;
    let id = shared_doc(&a, &b, "Doc", "the same text").await;
    let other = b.create_text("Other", "o").await;
    b.edb.create_relationship(&other, &id, RelationType::Supports, 0.5).await.unwrap();
    let before = held(&b, &id).await;
    delete_and_purge(&a, &id, How::EmptyTrash).await;
    b.svc.apply_purge_marker(marker_of(&a, &id).await).await.unwrap();
    let now = held(&b, &id).await;
    assert_eq!(now, before, "nothing removed: row, versions, key, edges, content");
    assert!(b.svc.read(&id).await.unwrap().trashed_at.is_some(), "in B's trash");
}

// ======================================================================= L5

// L5 — "A v0.0.11 purge marker is a deletion. A marker that carries a
// fingerprint is handled by L4 exactly; its fingerprint is neither compared
// nor stored." At the service, a v0.0.11 marker is what a v0.0.11 build
// stored (the row below) before this build's first unlock; the wire side is
// in `sovereign-p2p/tests/item3_local_purges.rs`. Neither compared: an
// identical copy with a later stamp comes back although the fingerprints
// match; a differing copy with an earlier stamp is ignored although they
// differ.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l5_a_v0011_markers_fingerprint_is_never_compared() {
    let c = Stack::with_device("l5-stored", "tablet").await;
    let id = "document:purgedbyv0011";
    let deletion = stamp(&c, -120, 0, "laptop");
    let identical = remote_text(id, "Doc", "the purged text", "phone");
    let fp = v0011_fingerprint(id, &identical.title, &identical.content);
    c.edb.set_purge_marker(row(id, Some(&deletion), Some(fp), c.clock.get())).await.unwrap();
    assert!(c.svc.is_purged(id).await.unwrap(), "setup");

    // Earlier, and different: ignored (v0.0.11 brought it back).
    let different = remote_text(id, "Doc", "a different text", "phone");
    let out = c.svc.apply_wire(different, meta(Some(after(&deletion, -60_000)), None)).await.unwrap();
    assert_eq!(out.applied, WireApplied::Ignored, "an earlier live state is ignored, whatever its content");
    assert!(c.svc.read(id).await.is_err());

    // Later, and identical: comes back (v0.0.11 ignored it).
    let out = c.svc.apply_wire(identical, meta(Some(stamp(&c, 60, 0, "phone")), None)).await.unwrap();
    assert!(matches!(out.applied, WireApplied::Applied(RemoteOutcome::Created)), "{:?}", out.applied);
    assert_eq!(texts(&c.stored(id).await.1), vec!["the purged text"]);
    assert!(!c.svc.is_purged(id).await.unwrap());
}

// L5 with L3 — a v0.0.11 marker row reads as a deletion: `purge_marker_wire`
// (what the backup restore and the manifest read rows through) gives its id
// and stamp, and a row without a fingerprint reads the same way. A row with
// no stamp, or revoked, is not a deletion that travels.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l5_a_v0011_marker_row_and_a_row_without_fingerprint_both_read_as_deletions() {
    let s = Stack::new("l5-row").await;
    let st = stamp(&s, -60, 0, "laptop");
    let with = row("document:v0011row", Some(&st), Some("cd".repeat(32)), s.clock.get());
    let without = row("document:v0012row", Some(&st), None, s.clock.get());
    let w = purge_marker_wire(&with).expect("a v0.0.11 row travels as a deletion");
    assert_eq!((w.id.as_str(), &w.stamp), ("document:v0011row", &st));
    let w = purge_marker_wire(&without).expect("a row without a fingerprint travels as a deletion (L3)");
    assert_eq!((w.id.as_str(), &w.stamp), ("document:v0012row", &st));
    assert!(purge_marker_wire(&row("document:nostamp", None, Some("ef".repeat(32)), s.clock.get())).is_none(), "no stamp: does not travel");
    assert!(purge_marker_wire(&PurgeMarker { revoked: true, ..with.clone() }).is_none(), "revoked: does not travel");
}

// ======================================================================= L6

/// Device A purges a document B also holds: (a, b, id, deletion stamp).
async fn purged_on_a_held_on_b(tag: &str) -> (Stack, Stack, String, Stamp) {
    let a = Stack::with_device(&format!("{tag}-a"), "laptop").await;
    let b = Stack::with_device(&format!("{tag}-b"), "phone").await;
    let mut new = new_text_doc("Doc", "imported text");
    new.trust = Trust::Undetermined;
    new.provenance = Provenance::Import { source_path: "D:/notes/a.md".into() };
    new.origin = sovereign_docs::Origin::Import;
    let id = a.svc.create(new).await.unwrap().id;
    send(&a, &b, &id).await;
    let deletion = delete_and_purge(&a, &id, How::EmptyTrash).await;
    (a, b, id, deletion)
}

// L6 — "a live state whose stamp is later than the marker's: applied as a new
// arrival (its own first version, the sender's trust and provenance), and the
// marker is dropped (revoked, so it no longer travels)". B's copy is the same
// text (v0.0.11 refused it by fingerprint); it is later because B restored it
// from its trash after the deletion reached it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l6_a_later_live_state_brings_a_purged_id_back_as_a_new_arrival_and_drops_the_marker() {
    let (a, b, id, deletion) = purged_on_a_held_on_b("l6-back").await;
    b.svc.apply_purge_marker(marker_of(&a, &id).await).await.unwrap();
    b.clock.advance(Duration::minutes(1));
    b.svc.restore_from_trash(&id).await.unwrap();
    assert!(current_stamp(&b, &id).await > deletion, "setup: B's live state is later");
    let theirs = b.svc.read(&id).await.unwrap();

    let out = send(&b, &a, &id).await;
    assert!(matches!(out.applied, WireApplied::Applied(RemoteOutcome::Created)), "a new arrival: {:?}", out.applied);
    let mine = a.svc.read(&id).await.expect("back on A");
    assert!(mine.trashed_at.is_none(), "live");
    assert_eq!(mine.plain_text(), "imported text");
    assert_eq!(a.commits(&id).await.len(), 1, "its own first version");
    assert_eq!((mine.trust, mine.provenance.clone()), (theirs.trust, theirs.provenance), "the sender's trust and provenance");
    assert!(!a.svc.is_purged(&id).await.unwrap(), "the marker is dropped");
    assert!(!advertises_marker(&a, &id).await, "it no longer travels");
    if let Some(m) = disk_marker(&a, &id).await {
        assert!(m.revoked, "a marker row left behind is revoked");
    }
}

// L6 — "a state in the sender's trash … : ignored." Even with a stamp later
// than the marker's (B trashed its copy itself after the deletion).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l6_a_copy_in_the_senders_trash_never_brings_a_purged_id_back() {
    let (a, b, id, deletion) = purged_on_a_held_on_b("l6-trash").await;
    b.clock.advance(Duration::minutes(1));
    b.svc.trash(&id).await.unwrap();
    assert!(current_stamp(&b, &id).await > deletion, "setup: B's trashed state is later than the deletion");
    let out = send(&b, &a, &id).await;
    assert_eq!(out.applied, WireApplied::Ignored);
    assert!(a.svc.is_purged(&id).await.unwrap(), "still purged on A");
    assert!(a.svc.read(&id).await.is_err());
    assert!(advertises_marker(&a, &id).await, "the marker still travels");
}

// L6 — "a live state whose stamp is earlier than or equal to the marker's:
// ignored." Earlier: B never touched its copy. Equal: a state stamped with the
// deletion's own stamp.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l6_a_live_state_not_later_than_the_deletion_is_ignored() {
    let (a, b, id, deletion) = purged_on_a_held_on_b("l6-older").await;
    assert!(current_stamp(&b, &id).await < deletion, "setup: B's copy is older");
    let out = send(&b, &a, &id).await;
    assert_eq!(out.applied, WireApplied::Ignored, "earlier");
    assert!(a.svc.is_purged(&id).await.unwrap());

    let equal = remote_text(&id, "Doc", "a state at the deletion's stamp", "phone");
    let out = a.svc.apply_wire(equal, WireMeta { stamp: Some(deletion.clone()), ..WireMeta::default() }).await.unwrap();
    assert_eq!(out.applied, WireApplied::Ignored, "equal");
    assert!(a.svc.is_purged(&id).await.unwrap());
    assert!(a.svc.read(&id).await.is_err());
}

// L6 — the identical, older copy: the purged version never comes back (the
// old G27 test `g27_the_purged_version_never_comes_back_from_any_peer`, now
// by stamp).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l6_the_purged_version_never_comes_back_from_any_peer() {
    let (a, b, id, _) = purged_on_a_held_on_b("l6-same").await;
    let identical = b.svc.stored_state(&id).await.unwrap();
    let out = a.svc.apply_wire(identical.document, identical.meta).await.unwrap();
    assert_eq!(out.applied, WireApplied::Ignored);
    assert!(a.svc.is_purged(&id).await.unwrap());
}

// ======================================================================= L7

// L7 — "The scenario of finding 2, as a two-device test. X deleted on A; the
// deletion reaches B; X restored on B; before that restore reaches A, the
// trash is emptied on A. A's deletion reaches B: B's copy stays live, History
// included. B's live state reaches A: X is back on A, live."
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l7_finding_2_a_restore_on_b_survives_emptying_the_trash_on_a_and_comes_back_to_a() {
    let a = Stack::with_device("l7-a", "laptop").await;
    let b = Stack::with_device("l7-b", "phone").await;
    let id = shared_doc(&a, &b, "X", "written on A").await;
    // History that exists only on B: every save there is a version.
    b.policy.set(PolicyMode::Always);
    b.clock.advance(Duration::minutes(1));
    b.editor_save(&id, 2, v2(vec![text_block(1, "only on the phone")])).await.unwrap();
    b.editor_save(&id, 3, v2(vec![text_block(1, "only on the phone, again")])).await.unwrap();
    let out = send(&b, &a, &id).await; // A has B's edits too
    assert!(matches!(out.applied, WireApplied::Applied(_)), "setup: {:?}", out.applied);

    // 1. X deleted on A; the deletion reaches B.
    a.clock.advance(Duration::minutes(2));
    a.svc.trash(&id).await.unwrap();
    let out = send(&a, &b, &id).await;
    assert!(matches!(out.applied, WireApplied::Applied(RemoteOutcome::Trashed)), "setup: {:?}", out.applied);
    // 2. X restored on B (not yet sent to A).
    b.clock.advance(Duration::minutes(3));
    b.svc.restore_from_trash(&id).await.unwrap();
    let history_on_b = b.committed_states(&id).await;
    let versions_on_b = b.commits(&id).await.len();
    // 3. The trash is emptied on A.
    a.svc.empty_trash().await.unwrap();
    assert!(a.svc.is_purged(&id).await.unwrap());

    // 4. A's deletion reaches B: B's copy stays live, History included.
    b.svc.apply_purge_marker(marker_of(&a, &id).await).await.unwrap();
    let v = b.svc.read(&id).await.expect("B's copy is there");
    assert!(v.trashed_at.is_none(), "B's copy stays live");
    assert_eq!(v.plain_text(), "only on the phone, again");
    assert!(b.commits(&id).await.len() >= versions_on_b, "no version lost on B");
    for state in &history_on_b {
        assert!(b.committed_states(&id).await.contains(state), "B's History kept");
    }
    assert!(b.doc_keys.read().await.contains(&id), "B's key kept");

    // 5. B's live state reaches A: X is back on A, live.
    let out = send(&b, &a, &id).await;
    assert!(matches!(out.applied, WireApplied::Applied(RemoteOutcome::Created)), "{:?}", out.applied);
    let back = a.svc.read(&id).await.expect("X is back on A");
    assert!(back.trashed_at.is_none(), "live");
    assert_eq!(back.plain_text(), "only on the phone, again");
    assert!(!a.svc.is_purged(&id).await.unwrap());
    assert!(!advertises_marker(&a, &id).await);
    // And the two devices now agree: nothing more to exchange.
    let (sa, sb) = (a.svc.stored_state(&id).await.unwrap(), b.svc.stored_state(&id).await.unwrap());
    assert_eq!(sa.normalized_hash, sb.normalized_hash, "same state on both");
}

// ======================================================================= L8

// L8 — "When a document enters this device's trash by a peer's deletion (a
// trash state applied …), its trash date is this device's now, whatever date
// the peer sent."
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l8_a_trash_state_applied_here_is_dated_by_this_devices_clock() {
    let s = Stack::new("l8-applied").await;
    let id = s.create_text("T", "x").await;
    let local = current_stamp(&s, &id).await;
    s.clock.advance(Duration::hours(2));
    let mut tomb = remote_text(&id, "T", "x", "phone");
    tomb.content = s.stored(&id).await.1;
    tomb.deleted_at = Some(s.clock.get() - Duration::days(40));
    let out = s.svc.apply_wire(tomb, meta(Some(stamp(&s, 1, 0, "phone")), Some(local))).await.unwrap();
    assert!(matches!(out.applied, WireApplied::Applied(RemoteOutcome::Trashed)), "{:?}", out.applied);
    assert_eq!(s.svc.read(&id).await.unwrap().trashed_at, Some(s.clock.get()), "this device's now, not the peer's date");
    let entry = s.svc.list_trash().await.unwrap().into_iter().find(|e| e.id == id).unwrap();
    assert_eq!(entry.trashed_at, s.clock.get());
}

// L8 — "… or a new arrival already in the sender's trash": dated by this
// device's now.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l8_a_new_arrival_already_in_the_senders_trash_is_dated_by_this_devices_clock() {
    let s = Stack::new("l8-arrival").await;
    let id = "document:arrivesintrash";
    let mut r = remote_text(id, "T", "x", "phone");
    r.deleted_at = Some(s.clock.get() - Duration::days(40));
    let out = s.svc.apply_wire(r, meta(Some(stamp(&s, 1, 0, "phone")), None)).await.unwrap();
    assert!(matches!(out.applied, WireApplied::Applied(RemoteOutcome::Created)), "{:?}", out.applied);
    assert_eq!(s.svc.read(id).await.unwrap().trashed_at, Some(s.clock.get()));
}

// L8 — "A document already in this device's trash keeps its date." A peer's
// later trash state (other content) arrives two days after this device
// trashed it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l8_a_document_already_in_this_devices_trash_keeps_its_date() {
    let a = Stack::with_device("l8-kept-a", "laptop").await;
    let b = Stack::with_device("l8-kept-b", "phone").await;
    let id = shared_doc(&a, &b, "Doc", "text").await;
    b.svc.trash(&id).await.unwrap();
    let date = b.svc.read(&id).await.unwrap().trashed_at;
    a.clock.advance(Duration::minutes(1));
    a.editor_save(&id, 1, v2(vec![text_block(1, "edited on A")])).await.unwrap();
    a.svc.trash(&id).await.unwrap();
    b.clock.advance(Duration::days(2));
    let _ = send(&a, &b, &id).await;
    let v = b.svc.read(&id).await.unwrap();
    assert!(v.trashed_at.is_some(), "still in the trash");
    assert_eq!(v.trashed_at, date, "its trash date unchanged");
}

// L8, the same rule through `apply_remote` (G19: "`apply_remote` keeps these
// rules for other callers").
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l8_apply_remote_dates_a_peers_tombstone_by_this_devices_clock() {
    let s = Stack::new("l8-apply-remote").await;
    let id = s.create_text("T", "x").await;
    s.clock.advance(Duration::minutes(10));
    let mut tomb = remote_text(&id, "T", "x", "phone");
    tomb.content = s.stored(&id).await.1;
    tomb.modified_at = s.clock.get();
    tomb.deleted_at = Some(s.clock.get() - Duration::days(40));
    assert_eq!(s.svc.apply_remote(tomb).await.unwrap(), RemoteOutcome::Trashed);
    assert_eq!(s.svc.read(&id).await.unwrap().trashed_at, Some(s.clock.get()));
}

// L8 — "A local delete is unchanged (now)." (Green today: a guard.)
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l8_a_local_delete_is_dated_now() {
    let s = Stack::new("l8-local").await;
    let id = s.create_text("T", "x").await;
    s.clock.advance(Duration::hours(3));
    s.svc.trash(&id).await.unwrap();
    assert_eq!(s.svc.read(&id).await.unwrap().trashed_at, Some(s.clock.get()));
}

// ======================================================================= L9

// L9 — "The scenario of finding 3, as a test. A peer whose clock is 40 days
// behind trashes X; this device applies it; the next purge ticks do not purge
// X before 30 days of this device's clock."
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l9_finding_3_a_peer_40_days_behind_cannot_shorten_the_30_days_here() {
    let a = Stack::with_device("l9-a", "laptop").await;
    let p = Stack::with_device("l9-phone", "phone").await;
    p.clock.advance(Duration::days(-40));
    purge_tick(&a.svc).await.unwrap(); // A's unlock tick
    // X written on the phone (stamps 40 days in A's past are accepted).
    let id = p.create_text("X", "from the late phone").await;
    let out = send(&p, &a, &id).await;
    assert!(matches!(out.applied, WireApplied::Applied(RemoteOutcome::Created)), "setup: {:?}", out.applied);
    // The phone trashes it; A applies it.
    p.svc.trash(&id).await.unwrap();
    let applied_at = a.clock.get();
    let out = send(&p, &a, &id).await;
    assert!(matches!(out.applied, WireApplied::Applied(RemoteOutcome::Trashed)), "setup: {:?}", out.applied);

    // Hourly ticks on A for 30 days minus a second: X is never purged.
    let r = purge_tick(&a.svc).await.unwrap();
    assert!(r.documents.is_empty(), "the next tick does not purge it (it purged within the hour in v0.0.11)");
    for day in 1..30 {
        a.clock.advance(Duration::days(1));
        let r = purge_tick(&a.svc).await.unwrap();
        assert!(r.documents.is_empty(), "day {day}: not before 30 days of A's clock");
    }
    a.clock.advance(Duration::days(1) - Duration::seconds(1));
    assert!(purge_tick(&a.svc).await.unwrap().documents.is_empty(), "a second before 30 days");
    assert!(a.svc.read(&id).await.unwrap().trashed_at == Some(applied_at), "dated by A's clock");
    a.clock.advance(Duration::seconds(1));
    assert_eq!(purge_tick(&a.svc).await.unwrap().documents, vec![id.clone()], "purged at 30 days of A's clock");
}

// ====================================================================== L13

/// A backup made on device `src`: a document trashed 40 days before the
/// backup, a live one, and one purged (its marker). Returns (documents,
/// markers, trashed id, live id, purged id, the purged one's state before its
/// purge, the backed-up trash date).
async fn backup_with_old_trash(
    src: &Stack,
) -> (Vec<BackupDocument>, Vec<PurgeMarkerWire>, String, String, String, sovereign_docs::WireState, DateTime<Utc>) {
    let trashed = src.create_text("Old trash", "trashed long ago").await;
    src.svc.trash(&trashed).await.unwrap();
    let trashed_at = src.svc.read(&trashed).await.unwrap().trashed_at.unwrap();
    src.clock.advance(Duration::days(40));
    let live = src.create_text("Live", "kept").await;
    let gone = src.create_text("Purged", "erased").await;
    let gone_state = src.svc.stored_state(&gone).await.unwrap();
    src.svc.trash(&gone).await.unwrap();
    src.svc.purge_from_trash(&gone).await.unwrap();
    let docs = src
        .svc
        .stored_states()
        .await
        .unwrap()
        .into_iter()
        .map(|w| BackupDocument { document: w.document, meta: w.meta })
        .collect();
    let markers = src.svc.purge_markers().await.unwrap();
    (docs, markers, trashed, live, gone, gone_state, trashed_at)
}

// L13 — "A guardian backup carries this device's markers (id, stamp), without
// fingerprints; restored onto an empty workspace they act as L6 says."
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l13_backup_markers_are_restored_without_fingerprints_and_act_as_deletions() {
    let src = Stack::with_device("l13-src", "laptop").await;
    let (docs, markers, _, _, gone, gone_state, _) = backup_with_old_trash(&src).await;
    let deletion = markers.iter().find(|m| m.id == gone).expect("the backup carries the marker").stamp.clone();
    let n = Stack::with_device("l13-new", "tablet").await;
    n.clock.advance(Duration::days(40)); // the new device's clock is the real time too
    n.svc.restore_backup(docs, markers).await.unwrap();

    assert!(n.svc.is_purged(&gone).await.unwrap(), "the marker is recorded");
    let disk = disk_marker(&n, &gone).await.expect("a marker row");
    assert_eq!(disk.stamp.as_deref(), Some(enc(&deletion).as_str()), "with its stamp");
    assert_eq!(disk.fingerprint, None, "restored without a fingerprint");

    // L6: the purged version (older than the deletion) is ignored…
    let out = n.svc.apply_wire(gone_state.document.clone(), gone_state.meta.clone()).await.unwrap();
    assert_eq!(out.applied, WireApplied::Ignored);
    // …a live state with a later stamp, same text, brings it back.
    let later = Stamp { wall_ms: deletion.wall_ms + 60_000, counter: 0, device: "phone".into() };
    let mut doc = gone_state.document.clone();
    doc.device = "phone".into();
    let out = n.svc.apply_wire(doc, meta(Some(later), Some(gone_state.meta.stamp.clone().unwrap()))).await.unwrap();
    assert!(matches!(out.applied, WireApplied::Applied(RemoteOutcome::Created)), "{:?}", out.applied);
    assert!(!n.svc.is_purged(&gone).await.unwrap());
}

// L13 — "A backup taken by v0.0.11 may hold markers with fingerprints: they
// are restored without them." The backup's markers as rows (what a v0.0.11
// snapshot holds), through `purge_marker_wire` as the restore reads them.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l13_a_v0011_backups_markers_are_restored_without_their_fingerprints() {
    let n = Stack::new("l13-v0011").await;
    let st = stamp(&n, -60, 0, "laptop");
    let rows = vec![
        row("document:v0011backup", Some(&st), Some("12".repeat(32)), n.clock.get()),
        row("document:v0012backup", Some(&st), None, n.clock.get()),
    ];
    let markers: Vec<PurgeMarkerWire> = rows.iter().filter_map(purge_marker_wire).collect();
    assert_eq!(markers.len(), 2, "both rows are deletions");
    n.svc.restore_backup(vec![], markers).await.unwrap();
    for id in ["document:v0011backup", "document:v0012backup"] {
        assert!(n.svc.is_purged(id).await.unwrap(), "{id} recorded");
        let disk = disk_marker(&n, id).await.unwrap();
        assert_eq!(disk.fingerprint, None, "{id}: no fingerprint restored");
        assert_eq!(disk.stamp.as_deref(), Some(enc(&st).as_str()));
    }
}

// L13, as changed on 2026-10-24 (main `3e2359d`, on the test author's
// question 4): "A document restored in the trash enters this device's trash
// at the restore (its trash date is the restore's now, as in L8), so it has 30
// days on the new device whatever date the backup held." The document was 40
// days in the trash when the backup was taken. Two device histories:
// - a new device, restored before its first tick;
// - a device whose "trash since" was recorded three weeks before the restore
//   (the case the first text, relying on L10's floor, did not cover).
// Either way: dated by the restore, not purged by the next ticks, 30 days
// shown, purged 30 days after the restore and not a second before.
// (Was `l13_a_document_restored_in_the_trash_keeps_its_date_and_gets_30_days_on_the_new_device`.)
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l13_a_document_restored_in_the_trash_enters_this_devices_trash_at_the_restore() {
    for ticked_weeks_before in [false, true] {
        let case = if ticked_weeks_before { "trash since three weeks before" } else { "new device" };
        let src = Stack::with_device(&format!("l13-trash-src-{ticked_weeks_before}"), "laptop").await;
        let (docs, markers, trashed, live, _, _, backed_up_at) = backup_with_old_trash(&src).await;
        let n = Stack::with_device(&format!("l13-trash-new-{ticked_weeks_before}"), "tablet").await;
        if ticked_weeks_before {
            // Unlocked (its first tick records "trash since"), then restored
            // three weeks later.
            n.clock.advance(Duration::days(19));
            assert!(purge_tick(&n.svc).await.unwrap().documents.is_empty(), "{case}: setup");
            n.clock.advance(Duration::days(21));
        } else {
            n.clock.advance(Duration::days(40));
        }
        let restored_at = n.clock.get();
        n.svc.restore_backup(docs, markers).await.unwrap();
        let v = n.svc.read(&trashed).await.unwrap();
        assert_ne!(v.trashed_at, Some(backed_up_at), "{case}: not the backed-up date");
        assert_eq!(v.trashed_at, Some(restored_at), "{case}: dated by the restore");

        // The next tick (the new device's first, or a later one): nothing purged.
        let r = purge_tick(&n.svc).await.unwrap();
        assert!(r.documents.is_empty(), "{case}: not purged after the restore: {:?}", r.documents);
        let line = list_trash_named(&n.svc, n.edb.as_ref(), n.clock.get())
            .await
            .unwrap()
            .into_iter()
            .find(|l| l.entry.id == trashed)
            .expect("in the trash view");
        assert_eq!(line.days_left, 30, "{case}: 30 days left on this device");
        n.clock.advance(Duration::days(30) - Duration::seconds(1));
        assert!(purge_tick(&n.svc).await.unwrap().documents.is_empty(), "{case}: not before 30 days");
        n.clock.advance(Duration::seconds(1));
        assert_eq!(purge_tick(&n.svc).await.unwrap().documents, vec![trashed.clone()], "{case}: 30 days after the restore");
        assert!(n.svc.read(&live).await.is_ok(), "{case}: the live one stays");
    }
}
