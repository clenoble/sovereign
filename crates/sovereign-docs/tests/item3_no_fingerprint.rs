//! v0.0.12 item 3 — no fingerprint on disk (L12; review of v0.0.11 finding
//! 13: "A purge marker stores an unsalted SHA-256 of the purged title and
//! text, in plaintext, forever").
//!
//! Contract: `doc/plans/V0012-item3-local-purges.md` (main `0e4eb0d`): "At the
//! first unlock with this build, every stored marker's fingerprint is erased
//! (both personas, each at its own unlock), before the first sync. A test
//! reads the store after the unlock: no marker carries a fingerprint.
//! `purge_fingerprint` is gone from the code, and so is the comment that
//! called a marker "nothing to seal" while it held one (`encrypted.rs` near
//! 2117); a marker now holds an id, times and a stamp only."
//!
//! Written before the code, by another author: the implementer makes these
//! pass without editing this file.
//!
//! **What "the unlock" is here.** Both hosts, at every unlock, once the
//! persona's document service is built, call the one-time migration's seam
//! (`start_unlock_migration` → `run_unlock_migration`) and run the purge tick
//! (`purge_tick`). This file runs exactly those two, in that order, and then
//! reads the raw store. CONTRACT-QUESTION: the contract does not name the
//! entry point; whichever of the two (or the service's first use) erases the
//! fingerprints satisfies these tests. Note that a v0.0.11 store has already
//! completed the format migration, so `run_unlock_migration` returns
//! `AlreadyComplete` there: an erasure placed after that early return would
//! never run on an upgraded store, which is the case that matters.

mod common;

use chrono::Duration;
use common::scan::{blank, crates_dir, rust_files};
use common::*;
use sovereign_db::schema::PurgeMarker;
use sovereign_docs::{purge_tick, run_unlock_migration, unlock_migration_complete, Stamp};

fn enc(s: &Stamp) -> String {
    format!("{}:{}:{}", s.wall_ms, s.counter, s.device)
}

/// A store as v0.0.11 left it: the format migration completed, and purge
/// markers with fingerprints — one this device purged, one received (no copy
/// here), one revoked, one still incomplete (a purge interrupted). Returns the
/// ids of the complete, unrevoked ones (they must still travel after the
/// erasure) and of every marker.
async fn v0011_store(s: &Stack) -> (Vec<String>, Vec<String>) {
    run_unlock_migration(&s.svc).await.unwrap();
    assert!(unlock_migration_complete(&s.svc).await.unwrap(), "setup: v0.0.11 completed the format migration");
    let at = s.clock.get();
    let st = |secs: i64| Stamp {
        wall_ms: (at - Duration::seconds(secs)).timestamp_millis(),
        counter: 0,
        device: "laptop".into(),
    };
    let fp = |n: u8| format!("{n:02x}").repeat(32);
    let m = |id: &str, stamp: &Stamp, fingerprint: String| PurgeMarker {
        id: None,
        document_id: id.to_string(),
        purged_at: at,
        complete: true,
        stamp: Some(enc(stamp)),
        fingerprint: Some(fingerprint),
        revoked: false,
    };
    s.edb.set_purge_marker(m("document:purgedhere", &st(600), fp(1))).await.unwrap();
    s.edb.set_purge_marker(m("document:received", &st(500), fp(2))).await.unwrap();
    s.edb.set_purge_marker(PurgeMarker { revoked: true, ..m("document:revoked", &st(400), fp(3)) }).await.unwrap();
    // An interrupted purge: its document is still there, in the trash.
    let half = s.create_text("Half purged", "x").await;
    s.svc.trash(&half).await.unwrap();
    s.edb.set_purge_marker(PurgeMarker { complete: false, ..m(&half, &st(300), fp(4)) }).await.unwrap();
    let travelling = vec!["document:purgedhere".to_string(), "document:received".to_string()];
    let all = vec![
        "document:purgedhere".to_string(),
        "document:received".to_string(),
        "document:revoked".to_string(),
        half,
    ];
    (travelling, all)
}

/// What both hosts run at every unlock (see the module comment).
async fn unlock(s: &Stack) {
    let _ = run_unlock_migration(&s.svc).await.unwrap();
    purge_tick(&s.svc).await.unwrap();
}

/// Every marker row on disk that still holds a fingerprint.
async fn fingerprinted(s: &Stack) -> Vec<String> {
    s.raw
        .list_purge_markers()
        .await
        .unwrap()
        .into_iter()
        .filter(|m| m.fingerprint.is_some())
        .map(|m| m.document_id)
        .collect()
}

// L12 — "At the first unlock with this build, every stored marker's
// fingerprint is erased … A test reads the store after the unlock: no marker
// carries a fingerprint." The erasure keeps the markers themselves: the
// purged ids stay purged and the complete ones still travel, with their
// stamps.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l12_after_the_first_unlock_no_stored_marker_carries_a_fingerprint() {
    let mut s = Stack::new("l12-erased").await;
    let (travelling, all) = v0011_store(&s).await;
    assert_eq!(fingerprinted(&s).await.len(), all.len(), "setup: every marker has a fingerprint");
    let stamps_before: Vec<(String, Option<String>)> =
        s.raw.list_purge_markers().await.unwrap().into_iter().map(|m| (m.document_id, m.stamp)).collect();

    s.restart(); // the first start of this build over the v0.0.11 store
    unlock(&s).await;

    assert!(fingerprinted(&s).await.is_empty(), "fingerprints left on disk: {:?}", fingerprinted(&s).await);
    for id in &travelling {
        assert!(s.svc.is_purged(id).await.unwrap(), "{id}: still purged");
        assert!(s.svc.purge_markers().await.unwrap().iter().any(|m| &m.id == id), "{id}: still travels");
    }
    for (id, stamp) in stamps_before {
        if let Some(m) = s.raw.get_purge_marker(&id).await.unwrap() {
            assert_eq!(m.stamp, stamp, "{id}: its stamp kept");
        }
    }
}

// L12 — "both personas, each at its own unlock": two stores (the main persona
// and the duress persona are two stores); each is erased at its own unlock,
// and unlocking one changes nothing in the other.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l12_each_persona_is_erased_at_its_own_unlock() {
    let mut main = Stack::new("l12-main").await;
    let mut duress = Stack::new("l12-duress").await;
    v0011_store(&main).await;
    v0011_store(&duress).await;
    main.restart();
    duress.restart();

    unlock(&main).await;
    assert!(fingerprinted(&main).await.is_empty(), "main: erased at its unlock");
    assert!(!fingerprinted(&duress).await.is_empty(), "duress: not yet unlocked, its store untouched");
    unlock(&duress).await;
    assert!(fingerprinted(&duress).await.is_empty(), "duress: erased at its own unlock");
}

// L12 — after the erasure, a purge made by this build writes none either (L2),
// and a later unlock finds nothing to do.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l12_later_purges_and_unlocks_write_no_fingerprint() {
    let mut s = Stack::new("l12-later").await;
    v0011_store(&s).await;
    s.restart();
    unlock(&s).await;
    let id = s.create_text("Doc", "text").await;
    s.svc.trash(&id).await.unwrap();
    s.svc.empty_trash().await.unwrap();
    s.restart();
    unlock(&s).await;
    assert!(fingerprinted(&s).await.is_empty(), "{:?}", fingerprinted(&s).await);
}

/// Lines of `src` (comments and string contents blanked) where `ident` appears
/// as a whole identifier.
fn identifier_lines(src: &str, ident: &str) -> Vec<usize> {
    let code = blank(src);
    let mut out = vec![];
    for (n, line) in code.lines().enumerate() {
        let mut from = 0;
        while let Some(rel) = line[from..].find(ident) {
            let at = from + rel;
            from = at + ident.len();
            let before = line[..at].chars().last();
            let after = line[from..].chars().next();
            let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
            if !word(before) && !word(after) {
                out.push(n + 1);
                break;
            }
        }
    }
    out
}

// L12 — "`purge_fingerprint` is gone from the code." Every crate, production
// and tests alike (comments and string contents do not count).
#[test]
fn l12_purge_fingerprint_is_gone_from_the_code() {
    let mut sites = vec![];
    let root = crates_dir();
    for entry in std::fs::read_dir(&root).unwrap().flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        for sub in ["src", "tests", "benches", "examples"] {
            for file in rust_files(&dir.join(sub)) {
                let src = std::fs::read_to_string(&file).unwrap_or_default();
                for line in identifier_lines(&src, "purge_fingerprint") {
                    sites.push(format!("{}:{line}", file.display()));
                }
            }
        }
    }
    assert!(sites.is_empty(), "purge_fingerprint is still in the code: {sites:#?}");
}

// L12 — "… and so is the comment that called a marker "nothing to seal" while
// it held one (`encrypted.rs` near 2117)". Read as: no comment in the
// encrypted layer's purge-marker code still says a marker has nothing to seal
// because it carries "an id and times only" while the row type holds more
// than that; the simplest check is the phrase itself, near `set_purge_marker`.
#[test]
fn l12_the_nothing_to_seal_comment_is_gone() {
    let file = crates_dir().join("sovereign-db").join("src").join("encrypted.rs");
    let src = std::fs::read_to_string(&file).expect("encrypted.rs");
    let lines: Vec<&str> = src.lines().collect();
    let Some(at) = lines.iter().position(|l| l.contains("async fn set_purge_marker")) else {
        panic!("set_purge_marker not found in {}", file.display());
    };
    let window = &lines[at.saturating_sub(5)..(at + 6).min(lines.len())];
    assert!(
        !window.iter().any(|l| l.to_lowercase().contains("nothing to seal")),
        "the comment is still there:\n{}",
        window.join("\n")
    );
}
