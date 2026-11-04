//! v0.0.12 item 3 — what `apply_purge_marker` reports (L4, L5).
//!
//! Contract: `doc/plans/V0012-item3-local-purges.md` (main `0e4eb0d`), L4:
//! a received deletion moves an older live copy to the trash, keeps a later
//! live copy, keeps a trashed copy, records itself when there is no copy, is
//! refused beyond the stamp bound, and is **never** a purge.
//!
//! The contract names the behaviour, not the outcome type. This file fixes
//! the most natural names for it (API the implementer provides):
//!
//! - `PurgeApplied::Trashed` — a live copy older than the deletion went to
//!   this device's trash;
//! - `PurgeApplied::KeptLive` — a live copy later than the deletion stays;
//! - `PurgeApplied::KeptInTrash` — the copy was already in this device's trash;
//! - `PurgeApplied::Recorded` — no copy here: the deletion is recorded (or an
//!   earlier one here took the later stamp);
//! - `PurgeApplied::RefusedStampAhead` — beyond the bound (lot 1, C2).
//!
//! `Purged` and `KeptDiffers` are gone: the match below is exhaustive, so this
//! file does not compile while either exists (a received deletion is never a
//! purge, and nothing is compared by fingerprint). **It does not compile
//! today**, by construction.

mod common;

use chrono::Duration;
use common::wire::*;
use common::*;
use sovereign_docs::PurgeApplied;

/// Every outcome, exhaustively: a variant added or left over fails to compile.
fn name(o: PurgeApplied) -> &'static str {
    match o {
        PurgeApplied::Trashed => "trashed",
        PurgeApplied::KeptLive => "kept live",
        PurgeApplied::KeptInTrash => "kept in the trash",
        PurgeApplied::Recorded => "recorded",
        PurgeApplied::RefusedStampAhead => "refused, stamp ahead",
    }
}

async fn purged_marker(a: &Stack, id: &str) -> sovereign_docs::PurgeMarkerWire {
    a.svc.trash(id).await.unwrap();
    a.svc.empty_trash().await.unwrap();
    a.svc.purge_markers().await.unwrap().into_iter().find(|m| m.id == id).expect("a marker")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l4_each_case_reports_what_it_did_and_none_is_a_purge() {
    // Older live copy → Trashed.
    let a = Stack::with_device("out-older-a", "laptop").await;
    let b = Stack::with_device("out-older-b", "phone").await;
    let id = a.create_text("Doc", "text").await;
    send(&a, &b, &id).await;
    let m = purged_marker(&a, &id).await;
    let out = b.svc.apply_purge_marker(m.clone()).await.unwrap();
    assert_eq!(out, PurgeApplied::Trashed, "{}", name(out));
    // The same deletion again: the copy is in the trash now.
    let again = b.svc.apply_purge_marker(m).await.unwrap();
    assert_eq!(again, PurgeApplied::KeptInTrash, "{}", name(again));

    // Later live copy → KeptLive.
    let a = Stack::with_device("out-later-a", "laptop").await;
    let b = Stack::with_device("out-later-b", "phone").await;
    let id = a.create_text("Doc", "text").await;
    send(&a, &b, &id).await;
    let m = purged_marker(&a, &id).await;
    b.clock.advance(Duration::minutes(1));
    b.editor_save(&id, 1, v2(vec![text_block(1, "after the deletion")])).await.unwrap();
    let out = b.svc.apply_purge_marker(m).await.unwrap();
    assert_eq!(out, PurgeApplied::KeptLive, "{}", name(out));

    // No copy → Recorded; a later deletion of the same id → Recorded (the
    // later stamp taken).
    let c = Stack::with_device("out-none-c", "tablet").await;
    let a = Stack::with_device("out-none-a", "laptop").await;
    let id = a.create_text("Doc", "text").await;
    let m = purged_marker(&a, &id).await;
    let out = c.svc.apply_purge_marker(m.clone()).await.unwrap();
    assert_eq!(out, PurgeApplied::Recorded, "{}", name(out));
    let mut later = m.clone();
    later.stamp.wall_ms += 60_000;
    let out = c.svc.apply_purge_marker(later).await.unwrap();
    assert_eq!(out, PurgeApplied::Recorded, "{}", name(out));
}
