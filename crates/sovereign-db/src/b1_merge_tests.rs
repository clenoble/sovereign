//! v0.0.13 lot L5, B1 — **C-MERGE** in the store's name resolution (spec ch.
//! 03, *Merging keeps the source thread — contract (C-MERGE)*, v2.0.109, MG3;
//! *Find or create by name*).
//!
//! What these tests hold:
//! - « A drop on its lane, a find-or-create by its name and a duplicate of one
//!   of its documents go to its live end » — the live end being « the thread
//!   at the end of its chain of merges, followed within a bound (the number of
//!   threads) »;
//! - « With no live end (a loop that two devices' merges made, or a deleted
//!   end), the request or the drop is refused and said »;
//! - « An ended thread is not a live thread of its name, for find-or-create or
//!   for an ambiguity (AT2) ».
//!
//! An ended thread is written here as a merge leaves it (MG1): live
//! (`deleted_at` empty), with `ended_at`, `merged_into` and `merge_stamp`. The
//! fields are the « types and signatures » commit's; how the document service
//! writes them is tested in `sovereign-docs/tests/b1_merge_service.rs`.
//!
//! The words are the contract's, compared whole (the Fable maille, by
//! delegation of Céline, 2026-11-17).
//!
//! Written before the code by the B1 test author; the implementer does not
//! edit this file. A test that looks wrong is reported, not worked around.

use chrono::{Duration, Utc};

use crate::mock::MockGraphDB;
use crate::schema::{raw_to_thing, Thread};
use crate::thread_names::{existing_thread_for_find_or_create, resolve_thread_name, ThreadNameMatch};
use crate::traits::GraphDB;

/// « '{name}' has ended (merged into '{target}'), and no live thread takes
/// its place. Nothing was changed. »
const NO_LIVE_END: &str =
    "'{name}' has ended (merged into '{target}'), and no live thread takes its place. Nothing was changed.";

fn fill(template: &str, parts: &[(&str, &str)]) -> String {
    let mut s = template.to_string();
    for (k, v) in parts {
        s = s.replace(k, v);
    }
    s
}

/// A live thread named `name`, with the given raw id.
async fn live(db: &MockGraphDB, id: &str, name: &str) -> String {
    let mut t = Thread::new(name.into(), String::new());
    t.id = raw_to_thing(id);
    assert!(db.create_thread_with_id(t).await.unwrap(), "fixture: {id} created");
    id.to_string()
}

/// Ends thread `id` as a merge into `into` leaves it (MG1): still live, with
/// its end date, the thread it was merged into, and a merge stamp.
async fn end(db: &MockGraphDB, id: &str, into: &str, n: u32) {
    let mut t = db.get_thread(id).await.unwrap();
    let at = Utc::now() - Duration::minutes(10) + Duration::seconds(i64::from(n));
    t.ended_at = Some(at);
    t.merged_into = Some(into.to_string());
    t.merge_stamp = Some(format!("{}:{n}:device-a", at.timestamp_millis()));
    db.put_thread(t).await.unwrap();
}

/// Soft-deletes `id`, its name kept (a tombstone).
async fn delete(db: &MockGraphDB, id: &str) {
    let mut t = db.get_thread(id).await.unwrap();
    t.deleted_at = Some(Utc::now().to_rfc3339());
    db.put_thread(t).await.unwrap();
}

/// MG3: a find-or-create by an ended thread's name goes to its live end — the
/// thread it was merged into — and creates nothing.
///
/// Red today: the ended thread is live by `deleted_at`, so it is the one found.
#[tokio::test]
async fn mg3_find_or_create_by_an_ended_threads_name_goes_to_its_live_end() {
    let db = MockGraphDB::new();
    let garden = live(&db, "thread:garden", "Garden").await;
    let kitchen = live(&db, "thread:kitchen", "Kitchen").await;
    end(&db, &garden, &kitchen, 1).await;
    assert_eq!(
        existing_thread_for_find_or_create(&db, "Garden").await,
        Ok(Some(kitchen)),
        "a find-or-create by 'Garden' (ended, merged into Kitchen) goes to Kitchen"
    );
}

/// MG3: the live end is the end of the chain: Attic into Garden, Garden into
/// Kitchen — a find-or-create by 'Attic' goes to Kitchen.
///
/// Red today: Attic is found.
#[tokio::test]
async fn mg3_find_or_create_follows_a_chain_to_its_live_end() {
    let db = MockGraphDB::new();
    let attic = live(&db, "thread:attic", "Attic").await;
    let garden = live(&db, "thread:garden", "Garden").await;
    let kitchen = live(&db, "thread:kitchen", "Kitchen").await;
    end(&db, &attic, &garden, 1).await;
    end(&db, &garden, &kitchen, 2).await;
    assert_eq!(existing_thread_for_find_or_create(&db, "Attic").await, Ok(Some(kitchen.clone())));
    assert_eq!(existing_thread_for_find_or_create(&db, "Garden").await, Ok(Some(kitchen)));
}

/// MG3: « followed within a bound (the number of threads) » — a chain that
/// goes through every thread still resolves: twelve threads, each merged
/// into the next, the last one live.
///
/// Red today: the first thread is found.
#[tokio::test]
async fn mg3_a_chain_through_every_thread_is_within_the_bound() {
    let db = MockGraphDB::new();
    let mut ids = vec![];
    for i in 0..12 {
        ids.push(live(&db, &format!("thread:t{i:02}"), &format!("Step {i:02}")).await);
    }
    for i in 0..11 {
        end(&db, &ids[i], &ids[i + 1], i as u32).await;
    }
    assert_eq!(existing_thread_for_find_or_create(&db, "Step 00").await, Ok(Some(ids[11].clone())));
}

/// MG3: « With no live end (a loop that two devices' merges made, or a
/// deleted end), the request … is refused and said. Nothing is changed. » A
/// find-or-create cannot ask, so it is refused with the contract's sentence,
/// and nothing is created (the caller creates only on `Ok(None)`).
///
/// Red today: the ended thread is found (`Ok(Some(..))`).
#[tokio::test]
async fn mg3_find_or_create_with_no_live_end_is_refused_with_its_sentence() {
    // A loop: Garden into Kitchen on one device, Kitchen into Garden on the other.
    let db = MockGraphDB::new();
    let garden = live(&db, "thread:garden", "Garden").await;
    let kitchen = live(&db, "thread:kitchen", "Kitchen").await;
    end(&db, &garden, &kitchen, 1).await;
    end(&db, &kitchen, &garden, 2).await;
    let before = db.list_threads_including_deleted().await.unwrap().len();
    assert_eq!(
        existing_thread_for_find_or_create(&db, "Garden").await,
        Err(fill(NO_LIVE_END, &[("{name}", "Garden"), ("{target}", "Kitchen")])),
        "a loop has no live end"
    );
    assert_eq!(db.list_threads_including_deleted().await.unwrap().len(), before, "nothing created");

    // A deleted end: Garden into Kitchen, then Kitchen deleted.
    let db = MockGraphDB::new();
    let garden = live(&db, "thread:garden", "Garden").await;
    let kitchen = live(&db, "thread:kitchen", "Kitchen").await;
    end(&db, &garden, &kitchen, 1).await;
    delete(&db, &kitchen).await;
    assert_eq!(
        existing_thread_for_find_or_create(&db, "Garden").await,
        Err(fill(NO_LIVE_END, &[("{name}", "Garden"), ("{target}", "Kitchen")])),
        "a deleted end is no live end"
    );
}

/// MG3: « An ended thread is not a live thread of its name, for
/// find-or-create or for an ambiguity (AT2). » An ended 'Garden' and a new,
/// live 'Garden': the name resolves to the live one, with no ambiguity, and a
/// find-or-create finds it.
///
/// Red today: two live threads named 'Garden' — ambiguous, and the
/// find-or-create is refused.
#[tokio::test]
async fn mg3_an_ended_thread_is_not_a_live_thread_of_its_name() {
    let db = MockGraphDB::new();
    let old = live(&db, "thread:garden-old", "Garden").await;
    let kitchen = live(&db, "thread:kitchen", "Kitchen").await;
    end(&db, &old, &kitchen, 1).await;
    let new = live(&db, "thread:garden-new", "Garden").await;

    match resolve_thread_name(&db, "garden").await.unwrap() {
        ThreadNameMatch::Found(t) => assert_eq!(t.id_string().as_deref(), Some(new.as_str()), "the live 'Garden'"),
        other => panic!("an ended thread is not a live thread of its name: {other:?}"),
    }
    assert_eq!(existing_thread_for_find_or_create(&db, "Garden").await, Ok(Some(new)));
}
