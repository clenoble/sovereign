//! v0.0.12 item 3 — the 30 days at an upgrade (L10) and the days shown (L11).
//!
//! Contract: `doc/plans/V0012-item3-local-purges.md` (main `0e4eb0d`), review
//! of v0.0.11 finding 14 ("Upgrading purges documents soft-deleted more than
//! 30 days ago, at the first unlock, without notice"). Written before the
//! code, by another author: the implementer makes these pass without editing
//! this file.
//!
//! - L10 **[choice]** "At its first purge tick, a persona's store records when
//!   this build started counting ("trash since"), if not already recorded. A
//!   document whose trash date is earlier than that is purged 30 days after
//!   "trash since", not before; the trash view shows it, with its days left
//!   computed on the same term."
//! - L11 "The trash view's days left (`hosts::days_left`, the hosts'
//!   listings) use the same date as the purge: the local trash date, floored
//!   by L10."
//!
//! The purge tick is `sovereign_docs::purge_tick` (what both hosts run at
//! unlock and hourly); the trash view is `list_trash_named` (what both hosts'
//! trash views list). "Trash since" is observed only through them: no API is
//! assumed for it.
//!
//! Reading of "earlier than": strictly earlier. A document trashed at the
//! instant of the first tick is not floored (its own 30 days and the floor's
//! are the same term anyway).
//!
//! CONTRACT-QUESTION (not tested here): what the trash view shows **before**
//! the first tick has recorded "trash since". The hosts run the tick at
//! unlock, before a trash view can open, so the case is not reachable in a
//! host; the hosts' own tests (`sovereign-shell` and `sovereign-app`
//! `step5_tests.rs`, `s5_5_*`/`s5_9_*`) list the trash with no tick and
//! expect days counted from the trash date, which this file reads as "no
//! floor until one is recorded".

mod common;

use chrono::{DateTime, Duration, Utc};
use common::*;
use sovereign_db::schema::DocumentPatch;
use sovereign_docs::{list_trash_named, purge_tick};

/// A document soft-deleted below the service, at `at`, as an older build or
/// the earlier Tauri desktop left it (finding 14: "never visible in a trash").
async fn soft_deleted_long_ago(s: &Stack, title: &str, at: DateTime<Utc>) -> String {
    let id = s.legacy(title, &v2(vec![text_block(1, title)])).await;
    let p = DocumentPatch { deleted_at: Some(Some(at.to_rfc3339())), ..Default::default() };
    s.edb.update_document_fields(&id, p).await.unwrap();
    id
}

/// Days left for `id` in the trash view as of the stack's clock.
async fn days_left_of(s: &Stack, id: &str) -> i64 {
    list_trash_named(&s.svc, s.edb.as_ref(), s.clock.get())
        .await
        .unwrap()
        .into_iter()
        .find(|l| l.entry.id == id)
        .unwrap_or_else(|| panic!("{id} is not in the trash view"))
        .days_left
}

async fn tick(s: &Stack) -> Vec<String> {
    let mut d = purge_tick(&s.svc).await.expect("tick").documents;
    d.sort();
    d
}

// L10 — finding 14: documents soft-deleted long before the upgrade (100 days
// and 31 days) are not purged at the first tick; they are shown with 30 days
// left, and purged 30 days after the first tick, not before.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l10_nothing_is_purged_at_the_first_tick_after_an_upgrade() {
    let s = Stack::new("l10-upgrade").await;
    let now = s.clock.get();
    let ancient = soft_deleted_long_ago(&s, "Ancient", now - Duration::days(100)).await;
    // And one trashed through the service 31 days ago, also before any tick
    // (the service's clock is moved back for that delete only).
    let trashed_31 = s.create_text("Trashed 31 days ago", "t").await;
    s.clock.advance(Duration::days(-31));
    s.svc.trash(&trashed_31).await.unwrap();
    s.clock.advance(Duration::days(31));
    assert_eq!(s.svc.read(&trashed_31).await.unwrap().trashed_at, Some(now - Duration::days(31)), "setup");
    assert!(s.svc.list_trash().await.unwrap().len() == 2, "setup: both are in the trash");

    let first = tick(&s).await;
    assert!(first.is_empty(), "the first tick after the upgrade purges nothing: {first:?}");
    for id in [&ancient, &trashed_31] {
        assert_eq!(days_left_of(&s, id).await, 30, "{id}: shown with 30 days left");
        assert!(s.svc.read(id).await.is_ok(), "{id}: still there");
    }

    s.clock.advance(Duration::days(29));
    assert!(tick(&s).await.is_empty(), "29 days after the first tick: kept");
    for id in [&ancient, &trashed_31] {
        assert_eq!(days_left_of(&s, id).await, 1, "{id}: one day left");
    }
    s.clock.advance(Duration::days(1) - Duration::seconds(1));
    assert!(tick(&s).await.is_empty(), "a second before 30 days after the first tick: kept");
    s.clock.advance(Duration::seconds(1));
    let mut want = vec![ancient.clone(), trashed_31.clone()];
    want.sort();
    assert_eq!(tick(&s).await, want, "30 days after the first tick: purged");
}

// L10 — "if not already recorded": the record is the store's, so it survives a
// restart; a later tick (after a restart) does not move it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l10_trash_since_is_recorded_once_and_survives_a_restart() {
    let mut s = Stack::new("l10-restart").await;
    let id = soft_deleted_long_ago(&s, "Old", s.clock.get() - Duration::days(60)).await;
    assert!(tick(&s).await.is_empty(), "first tick: recorded, nothing purged");
    s.clock.advance(Duration::days(10));
    s.restart();
    assert!(tick(&s).await.is_empty(), "after a restart, ten days on: kept");
    assert_eq!(days_left_of(&s, &id).await, 20, "the term did not restart with the service");
    s.clock.advance(Duration::days(20));
    s.restart();
    assert_eq!(tick(&s).await, vec![id.clone()], "30 days after the first tick ever, not after the last restart");
}

// L10 — "nothing in the trash is purged earlier than before (only later, for
// these)": a document trashed after "trash since" keeps its own 30 days,
// exactly.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l10_a_document_trashed_after_the_first_tick_keeps_its_own_30_days() {
    let s = Stack::new("l10-after").await;
    assert!(tick(&s).await.is_empty(), "the unlock tick");
    s.clock.advance(Duration::days(5));
    let id = s.create_text("Later", "x").await;
    s.svc.trash(&id).await.unwrap();
    assert_eq!(days_left_of(&s, &id).await, 30);
    s.clock.advance(Duration::days(30) - Duration::seconds(1));
    assert!(tick(&s).await.is_empty(), "not a second before its 30 days");
    assert_eq!(days_left_of(&s, &id).await, 1);
    s.clock.advance(Duration::seconds(1));
    assert_eq!(tick(&s).await, vec![id.clone()], "at its own 30 days, not later");
}

// L10 — a document trashed 10 days before the upgrade: purged 30 days after
// the first tick (later than its old term, never earlier).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l10_a_document_trashed_shortly_before_the_upgrade_is_purged_later_never_earlier() {
    let s = Stack::new("l10-shortly").await;
    let id = soft_deleted_long_ago(&s, "Ten days", s.clock.get() - Duration::days(10)).await;
    assert!(tick(&s).await.is_empty());
    assert_eq!(days_left_of(&s, &id).await, 30, "the floor gives it 30 days, not 20");
    s.clock.advance(Duration::days(20));
    assert!(tick(&s).await.is_empty(), "its old term (20 days on) no longer purges it");
    s.clock.advance(Duration::days(10));
    assert_eq!(tick(&s).await, vec![id.clone()]);
}

// L10 — "a persona's store records": each store has its own "trash since". A
// second store whose first tick comes ten days later counts from its own tick.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l10_each_personas_store_records_its_own_trash_since() {
    let main = Stack::new("l10-main").await;
    let duress = Stack::new("l10-duress").await;
    let t0 = main.clock.get();
    let m = soft_deleted_long_ago(&main, "Main", t0 - Duration::days(90)).await;
    let d = soft_deleted_long_ago(&duress, "Duress", t0 - Duration::days(90)).await;
    assert!(tick(&main).await.is_empty(), "main's first tick");
    duress.clock.advance(Duration::days(10));
    assert!(tick(&duress).await.is_empty(), "duress's first tick, ten days later");
    main.clock.advance(Duration::days(30));
    duress.clock.advance(Duration::days(20));
    assert_eq!(tick(&main).await, vec![m.clone()], "main: 30 days after its own first tick");
    assert!(tick(&duress).await.is_empty(), "duress: 20 days after its own first tick, kept");
    duress.clock.advance(Duration::days(10));
    assert_eq!(tick(&duress).await, vec![d.clone()]);
}

// L11 — "The days shown are the days used": for a floored document, at every
// day of its term the trash view's days left is the number of whole days
// until the tick that purges it, and the tick purges exactly when the view
// would reach 0.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l11_the_days_shown_for_a_floored_document_are_the_days_until_its_purge() {
    let s = Stack::new("l11-floored").await;
    let id = soft_deleted_long_ago(&s, "Old", s.clock.get() - Duration::days(45)).await;
    assert!(tick(&s).await.is_empty());
    for day in 0..30 {
        assert_eq!(days_left_of(&s, &id).await, 30 - day, "day {day} after the first tick");
        assert!(tick(&s).await.is_empty(), "day {day}: not purged while days are left");
        s.clock.advance(Duration::days(1));
    }
    assert_eq!(tick(&s).await, vec![id.clone()], "purged when the view would show 0");
}

// L11 — the local trash date (L8) is the one shown: a document a peer trashed
// with a date 40 days old shows 30 days left here.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l11_a_document_trashed_by_a_peer_shows_30_days_from_its_arrival_here() {
    let s = Stack::new("l11-peer").await;
    assert!(tick(&s).await.is_empty(), "the unlock tick");
    s.clock.advance(Duration::days(3));
    let mut r = common::wire::remote_text("document:trashedbyapeer", "T", "x", "phone");
    r.deleted_at = Some(s.clock.get() - Duration::days(40));
    s.svc
        .apply_wire(r, common::wire::meta(Some(common::wire::stamp(&s, 1, 0, "phone")), None))
        .await
        .unwrap();
    assert_eq!(days_left_of(&s, "document:trashedbyapeer").await, 30, "30 days from its arrival here");
    s.clock.advance(Duration::days(29));
    assert_eq!(days_left_of(&s, "document:trashedbyapeer").await, 1);
    assert!(tick(&s).await.is_empty());
}

// L11 — `hosts::days_left` itself is unchanged (a pure function of a date and
// now); the floor is in the date the listing gives it. (Green today: guard.)
#[test]
fn l11_days_left_counts_whole_days_from_the_date_it_is_given() {
    let t = Utc::now();
    assert_eq!(sovereign_docs::days_left(t, t), 30);
    assert_eq!(sovereign_docs::days_left(t, t + Duration::days(29)), 1);
    assert_eq!(sovereign_docs::days_left(t, t + Duration::days(30)), 0);
}
