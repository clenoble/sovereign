//! S5-3 — every host purges (C-DOC adoption step 5,
//! `doc/plans/C-DOC-adoption.md`).
//!
//! "One host-side purge tick (`purge_expired`), run at unlock and hourly by
//! shell and mobile. With an injected clock: trashed 30 days ago or more →
//! purged (versions, key, edges, a purge marker left); less → kept. The old
//! `purge_deleted` loop is gone."
//!
//! Spec ch. 03, G13 ("… 30 days in it purges: the document, its versions, its
//! key (crypto-shredding), its edges … What remains is a purge marker") and
//! *Soft delete and 30-day purge*.
//!
//! The tick is `sovereign_docs::purge_tick` (a step-5 stub in `src/hosts.rs`).
//! What a test can check of "shell and mobile run it": both hosts' production
//! code calls it (a source scan). *When* they call it (at unlock, then every
//! hour) is wiring inside the running hosts, which no test here can drive; the
//! interval itself is `PURGE_TICK_INTERVAL`. "The old `purge_deleted` loop is
//! gone" is S5-1's scan (`s5_one_way_to_delete.rs`).

mod common;

use chrono::Duration;
use common::scan::{crates_dir, production_files};
use common::*;
use sovereign_db::schema::RelationType;
use sovereign_docs::{purge_tick, DocError, PURGE_TICK_INTERVAL};

fn involves(id: &str, from: &Option<sovereign_db::schema::Thing>, to: &Option<sovereign_db::schema::Thing>) -> bool {
    let f = from.as_ref().map(sovereign_db::schema::thing_to_raw);
    let t = to.as_ref().map(sovereign_db::schema::thing_to_raw);
    f.as_deref() == Some(id) || t.as_deref() == Some(id)
}

async fn assert_purged(s: &Stack, id: &str) {
    assert!(s.raw.get_document(id).await.is_err(), "document row of {id}");
    assert!(s.raw.list_document_commits(id).await.unwrap_or_default().is_empty(), "versions of {id}");
    assert!(!s.doc_keys.read().await.contains(id), "key of {id}");
    assert!(
        s.raw.list_all_relationships().await.unwrap().iter().all(|r| !involves(id, &r.in_, &r.out)),
        "edges of {id}"
    );
    assert!(s.svc.is_purged(id).await.unwrap(), "a purge marker is left for {id}");
    assert!(matches!(s.svc.read(id).await, Err(DocError::Purged(_))));
}

async fn assert_kept(s: &Stack, id: &str, text: &str) {
    assert_eq!(texts(&s.stored(id).await.1), vec![text.to_string()], "content of {id} kept");
    assert!(!s.commits(id).await.is_empty(), "versions of {id} kept");
    assert!(s.doc_keys.read().await.contains(id), "key of {id} kept");
    assert!(!s.svc.is_purged(id).await.unwrap(), "no purge marker for {id}");
}

/// Trashed 30 days ago or more → purged with everything hanging off it;
/// trashed less than 30 days ago → kept; live → kept.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_3_the_tick_purges_what_is_30_days_in_the_trash_and_keeps_the_rest() {
    let s = Stack::new("s5-3-tick").await;
    let old = s.create_text("Old", "old text").await;
    let older = s.create_text("Older", "older text").await;
    let recent = s.create_text("Recent", "recent text").await;
    let live = s.create_text("Live", "live text").await;
    s.edb.create_relationship(&old, &live, RelationType::References, 0.5).await.unwrap();
    s.edb.create_relationship(&live, &old, RelationType::Supports, 0.5).await.unwrap();

    s.svc.trash(&older).await.unwrap();
    s.clock.advance(Duration::days(5));
    s.svc.trash(&old).await.unwrap();
    s.clock.advance(Duration::days(26));
    s.svc.trash(&recent).await.unwrap();
    // Now: `older` 31 days in the trash, `old` exactly 26 days, `recent` 0.
    s.clock.advance(Duration::days(4));
    // `older` 35 days, `old` exactly 30 days, `recent` 4 days.

    let report = purge_tick(&s.svc).await.expect("tick");
    let mut purged = report.documents.clone();
    purged.sort();
    let mut expected = vec![old.clone(), older.clone()];
    expected.sort();
    assert_eq!(purged, expected, "exactly what is 30 days or more in the trash");

    assert_purged(&s, &old).await;
    assert_purged(&s, &older).await;
    assert_kept(&s, &recent, "recent text").await;
    assert_kept(&s, &live, "live text").await;
    let trash: Vec<String> = s.svc.list_trash().await.unwrap().into_iter().map(|e| e.id).collect();
    assert_eq!(trash, vec![recent.clone()], "the recent one stays in the trash");
    assert!(s.svc.list(None).await.unwrap().iter().any(|d| d.id == live), "the live one stays listed");
}

/// One second short of 30 days is kept; a later tick purges it once the 30
/// days are reached.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_3_a_later_tick_purges_what_an_earlier_one_kept() {
    let s = Stack::new("s5-3-later").await;
    let id = s.create_text("Doc", "text").await;
    s.svc.trash(&id).await.unwrap();
    s.clock.advance(Duration::days(30) - Duration::seconds(1));
    let first = purge_tick(&s.svc).await.expect("first tick");
    assert!(first.documents.is_empty(), "not a second before 30 days");
    assert_kept(&s, &id, "text").await;
    s.clock.advance(Duration::hours(1));
    let second = purge_tick(&s.svc).await.expect("second tick");
    assert_eq!(second.documents, vec![id.clone()]);
    assert_purged(&s, &id).await;
}

/// "Hourly."
#[test]
fn s5_3_the_tick_interval_is_an_hour() {
    assert_eq!(PURGE_TICK_INTERVAL, std::time::Duration::from_secs(3600));
}

/// Lines calling `purge_tick(…)` — qualified (`sovereign_docs::purge_tick(`)
/// or imported and called bare — but not its definition (`fn purge_tick(`).
fn tick_calls(prod: &str) -> Vec<usize> {
    let mut out = vec![];
    for (ln, line) in prod.lines().enumerate() {
        let mut from = 0;
        while let Some(rel) = line[from..].find("purge_tick") {
            let at = from + rel;
            from = at + "purge_tick".len();
            let before = &line[..at];
            let ident_before = before.chars().last().is_some_and(|c| c.is_alphanumeric() || c == '_');
            let is_def = before.trim_end().ends_with("fn");
            if !ident_before && !is_def && line[from..].trim_start().starts_with('(') {
                out.push(ln + 1);
            }
        }
    }
    out
}

/// "Run … by shell and mobile": both hosts' production code calls the tick.
/// (The mobile app is the `sovereign-app` crate's Tauri host.)
#[test]
fn s5_3_shell_and_mobile_call_the_purge_tick() {
    for host in ["sovereign-shell", "sovereign-app"] {
        let dir = crates_dir().join(host);
        let mut sites = vec![];
        for (file, prod) in production_files(&dir) {
            for line in tick_calls(&prod) {
                sites.push(format!("{}:{line}", file.display()));
            }
        }
        assert!(!sites.is_empty(), "{host} never calls sovereign_docs::purge_tick");
    }
}
