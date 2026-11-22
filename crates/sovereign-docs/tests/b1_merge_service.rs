//! v0.0.13 lot L5, B1 — **C-MERGE** in the document service (spec ch. 03,
//! *Merging keeps the source thread — contract (C-MERGE)*, v2.0.109): MG1,
//! MG2's end date, MG3's refusals, restore and rename, MG5's undo.
//!
//! The merge is `DocumentService::thread_merge(target, source)` (built in
//! v0.0.12, to change); the undo is `DocumentService::thread_unmerge(source)`
//! and the three fields `Thread::{ended_at, merged_into, merge_stamp}`, all of
//! the « types and signatures » commit. The two-device half (MG4) is in
//! `sovereign-p2p/tests/b1_merge_sync.rs`; the assistant's words in
//! `sovereign-ai/src/orchestrator_b1_merge_tests.rs`.
//!
//! Written before the code by the B1 test author; the implementer does not
//! edit this file. A test that looks wrong is reported, not worked around.

mod common;

use chrono::Duration;
use common::*;
use sovereign_db::schema::Thread;
use sovereign_docs::{parse_thread_stamp, restore_to_thread, Stamp};

async fn thread(s: &Stack, name: &str) -> String {
    s.svc.thread_create(name, "").await.expect("thread created")
}

async fn doc_in(s: &Stack, thread_id: &str, title: &str, text: &str) -> String {
    let mut d = new_text_doc(title, text);
    d.thread_id = thread_id.to_string();
    s.svc.create(d).await.expect("create").id
}

async fn row(s: &Stack, id: &str) -> Thread {
    s.edb.get_thread(id).await.expect("the thread record")
}

/// A thread record as a whole, for « records nothing » and « nothing written ».
async fn whole(s: &Stack, id: &str) -> serde_json::Value {
    serde_json::to_value(row(s, id).await).unwrap()
}

/// What a document's state is: its thread, trashed or not, and its stamp.
async fn doc_state(s: &Stack, id: &str) -> (String, bool, Option<Stamp>) {
    let st = s.svc.stored_state(id).await.expect("stored state");
    (st.document.thread_id.clone(), st.document.deleted_at.is_some(), st.meta.stamp)
}

fn merge_stamp_of(t: &Thread) -> Stamp {
    parse_thread_stamp(t.merge_stamp.as_deref().expect("a merge stamp")).expect("a merge stamp that reads as one")
}

/// Garden (Seeds live, Bulbs in the trash) and Kitchen (Pans).
struct World {
    s: Stack,
    garden: String,
    kitchen: String,
    seeds: String,
    bulbs: String,
    pans: String,
}

async fn world(tag: &str) -> World {
    let s = Stack::new(tag).await;
    let garden = thread(&s, "Garden").await;
    let kitchen = thread(&s, "Kitchen").await;
    let seeds = doc_in(&s, &garden, "Seeds", "seed list").await;
    let bulbs = doc_in(&s, &garden, "Bulbs", "bulb list").await;
    s.svc.trash(&bulbs).await.expect("Bulbs to the trash");
    let pans = doc_in(&s, &kitchen, "Pans", "pan list").await;
    s.clock.advance(Duration::seconds(1));
    World { s, garden, kitchen, seeds, bulbs, pans }
}

// ------------------------------------------------------------------ MG1 --

/// MG1: « No document moves, and the source is not deleted. » Seeds (live) and
/// Bulbs (in the trash) stay in Garden as they were — same thread, same
/// trashed state, same stamp: nothing written to them — and Garden stays live.
///
/// Red today: every document of the source moves to the target, and the
/// source is deleted.
#[tokio::test]
async fn mg1_a_merge_moves_no_document_and_deletes_nothing() {
    let w = world("b1-mg1-nothing").await;
    let (seeds, bulbs, pans) =
        (doc_state(&w.s, &w.seeds).await, doc_state(&w.s, &w.bulbs).await, doc_state(&w.s, &w.pans).await);
    w.s.svc.thread_merge(&w.kitchen, &w.garden).await.expect("merged");
    assert_eq!(doc_state(&w.s, &w.seeds).await, seeds, "Seeds stays in Garden, unwritten");
    assert_eq!(doc_state(&w.s, &w.bulbs).await, bulbs, "Bulbs stays in Garden's trash, unwritten");
    assert_eq!(doc_state(&w.s, &w.pans).await, pans, "Pans is untouched");
    assert!(row(&w.s, &w.garden).await.deleted_at.is_none(), "the source is not deleted");
}

/// MG1: « A merge gives the source two fields, its end date and the thread it
/// was merged into … The target records nothing. »
///
/// Red today: the source is deleted and has no such field.
#[tokio::test]
async fn mg1_the_source_gets_its_two_fields_and_the_target_records_nothing() {
    let w = world("b1-mg1-fields").await;
    let kitchen_before = whole(&w.s, &w.kitchen).await;
    w.s.svc.thread_merge(&w.kitchen, &w.garden).await.expect("merged");
    let g = row(&w.s, &w.garden).await;
    assert_eq!(g.merged_into.as_deref(), Some(w.kitchen.as_str()), "merged into Kitchen");
    assert!(g.ended_at.is_some(), "Garden has its end date");
    assert!(g.merge_stamp.is_some(), "and the merge's own stamp (MG4)");
    assert_eq!(g.name, "Garden");
    assert_eq!(whole(&w.s, &w.kitchen).await, kitchen_before, "the target records nothing");
}

// ------------------------------------------------------------------ MG2 --

/// MG2: « The end date is the merge stamp's time, so that every device draws
/// the same lane. » The merge stamp is issued after every stamp this device
/// has issued (it orders after the source's own stamp and its documents').
///
/// Red today: the source has no end date and no merge stamp.
#[tokio::test]
async fn mg2_the_end_date_is_the_merge_stamps_time() {
    let w = world("b1-mg2-date").await;
    let before = parse_thread_stamp(row(&w.s, &w.garden).await.stamp.as_deref().unwrap()).unwrap();
    let seeds = doc_state(&w.s, &w.seeds).await.2.expect("stamped");
    w.s.svc.thread_merge(&w.kitchen, &w.garden).await.expect("merged");
    let g = row(&w.s, &w.garden).await;
    let ms = merge_stamp_of(&g);
    assert_eq!(g.ended_at.map(|t| t.timestamp_millis()), Some(ms.wall_ms), "the end date is the merge stamp's time");
    assert!(ms > before && ms > seeds, "the merge stamp follows every stamp issued here: {ms:?}");
}

/// MG2: « Its documents stay in it, and stay editable. »
///
/// Red today: Seeds is moved to Kitchen by the merge.
#[tokio::test]
async fn mg2_the_documents_of_an_ended_thread_stay_editable() {
    let w = world("b1-mg2-edit").await;
    w.s.svc.thread_merge(&w.kitchen, &w.garden).await.expect("merged");
    w.s.clock.advance(Duration::seconds(5));
    w.s.assistant_save(&w.seeds, "seed list, edited after the merge").await.expect("an edit is saved");
    assert_eq!(texts(&w.s.stored(&w.seeds).await.1), vec!["seed list, edited after the merge".to_string()]);
    assert_eq!(doc_state(&w.s, &w.seeds).await.0, w.garden, "and it stays in Garden");
}

// ------------------------------------------------------------------ MG3 --

/// MG3: « An ended thread … is the target of nothing … a merge into it is
/// refused. » Garden is merged into Kitchen; then Attic into Garden, and
/// Kitchen into Garden (which would close a loop), are refused, and nothing is
/// written.
///
/// Red today: Garden is deleted by the first merge (its fields are checked
/// first).
#[tokio::test]
async fn mg3_a_merge_into_an_ended_thread_is_refused_and_writes_nothing() {
    let w = world("b1-mg3-into-ended").await;
    let attic = thread(&w.s, "Attic").await;
    w.s.svc.thread_merge(&w.kitchen, &w.garden).await.expect("merged");
    assert_eq!(row(&w.s, &w.garden).await.merged_into.as_deref(), Some(w.kitchen.as_str()), "setup: Garden ended");
    let (g, k, a) = (whole(&w.s, &w.garden).await, whole(&w.s, &w.kitchen).await, whole(&w.s, &attic).await);
    assert!(w.s.svc.thread_merge(&w.garden, &attic).await.is_err(), "Attic into the ended Garden: refused");
    assert!(w.s.svc.thread_merge(&w.garden, &w.kitchen).await.is_err(), "Kitchen into the ended Garden: refused");
    assert_eq!(
        (whole(&w.s, &w.garden).await, whole(&w.s, &w.kitchen).await, whole(&w.s, &attic).await),
        (g, k, a),
        "nothing written"
    );
}

/// MG3: « An ended thread takes no new or moved document. » A move into it
/// through the service's one move is refused, and nothing is written.
///
/// Red today: Garden is deleted by the merge (its fields are checked first).
#[tokio::test]
async fn mg3_no_document_is_moved_into_an_ended_thread() {
    let w = world("b1-mg3-move").await;
    w.s.svc.thread_merge(&w.kitchen, &w.garden).await.expect("merged");
    assert_eq!(row(&w.s, &w.garden).await.merged_into.as_deref(), Some(w.kitchen.as_str()), "setup: Garden ended");
    let pans = doc_state(&w.s, &w.pans).await;
    assert!(w.s.svc.move_to_thread(&w.pans, &w.garden).await.is_err(), "a move into the ended Garden is refused");
    assert_eq!(doc_state(&w.s, &w.pans).await, pans, "Pans stays in Kitchen, unwritten");
}

/// MG3: « A restore from the trash goes back to the ended thread: it is the
/// document's own. » Bulbs, trashed in Garden before the merge, comes back
/// into Garden, through the hosts' restore (`restore_to_thread`, no thread
/// chosen).
///
/// Red today: the merge moved Bulbs to Kitchen and deleted Garden.
#[tokio::test]
async fn mg3_a_restore_from_the_trash_goes_back_to_the_ended_thread() {
    let w = world("b1-mg3-restore").await;
    w.s.svc.thread_merge(&w.kitchen, &w.garden).await.expect("merged");
    restore_to_thread(&w.s.svc, w.s.edb.as_ref(), &w.bulbs, None).await.expect("restored into its own thread");
    assert_eq!(doc_state(&w.s, &w.bulbs).await.0, w.garden, "back in Garden");
    assert!(!doc_state(&w.s, &w.bulbs).await.1, "out of the trash");
}

/// MG3: « An ended thread can still be renamed. » The rename keeps the merge:
/// the two fields and the merge stamp are as the merge left them.
///
/// Red today: the merge deleted Garden, and a deleted thread is not renamed.
#[tokio::test]
async fn mg3_an_ended_thread_can_be_renamed_and_stays_ended() {
    let w = world("b1-mg3-rename").await;
    w.s.svc.thread_merge(&w.kitchen, &w.garden).await.expect("merged");
    let before = row(&w.s, &w.garden).await;
    w.s.clock.advance(Duration::seconds(1));
    w.s.svc.thread_update(&w.garden, Some("Garden 2025"), None).await.expect("renamed");
    let after = row(&w.s, &w.garden).await;
    assert_eq!(after.name, "Garden 2025");
    assert_eq!(
        (after.ended_at, after.merged_into, after.merge_stamp),
        (before.ended_at, before.merged_into, before.merge_stamp),
        "still ended, into Kitchen, under the same merge stamp"
    );
}

// ------------------------------------------------------------------ MG5 --

/// MG5: « Undoing it clears the two fields under a new merge stamp. »
///
/// Red today: `thread_unmerge` is not built (`unimplemented!`), and the merge
/// deleted Garden.
#[tokio::test]
async fn mg5_the_undo_clears_the_two_fields_under_a_new_merge_stamp() {
    let w = world("b1-mg5-undo").await;
    w.s.svc.thread_merge(&w.kitchen, &w.garden).await.expect("merged");
    let merged = merge_stamp_of(&row(&w.s, &w.garden).await);
    w.s.clock.advance(Duration::seconds(1));
    w.s.svc.thread_unmerge(&w.garden).await.expect("undone");
    let g = row(&w.s, &w.garden).await;
    assert_eq!((g.ended_at, g.merged_into.as_deref()), (None, None), "the two fields are cleared");
    let undone = merge_stamp_of(&g);
    assert!(undone > merged, "under a new merge stamp: {undone:?} after {merged:?}");
    assert!(g.deleted_at.is_none());
    assert_eq!(doc_state(&w.s, &w.seeds).await.0, w.garden, "its documents never left");
}

/// MG5: « the undo needs no record kept in memory, and it outlives a
/// restart. » The merge, then a new service over the same store, then the
/// undo.
///
/// Red today: `thread_unmerge` is not built.
#[tokio::test]
async fn mg5_the_undo_outlives_a_restart() {
    let mut w = world("b1-mg5-restart").await;
    w.s.svc.thread_merge(&w.kitchen, &w.garden).await.expect("merged");
    w.s.restart();
    w.s.svc.thread_unmerge(&w.garden).await.expect("undone after a restart");
    assert_eq!(row(&w.s, &w.garden).await.merged_into, None);
}

/// MG5: « It is refused if the thread is not ended. » A thread never merged,
/// and a merge undone already: refused, and nothing written.
///
/// Red today: `thread_unmerge` is not built.
#[tokio::test]
async fn mg5_the_undo_is_refused_when_the_thread_is_not_ended() {
    let w = world("b1-mg5-refused").await;
    let kitchen = whole(&w.s, &w.kitchen).await;
    assert!(w.s.svc.thread_unmerge(&w.kitchen).await.is_err(), "Kitchen was never merged");
    assert_eq!(whole(&w.s, &w.kitchen).await, kitchen, "nothing written");

    w.s.svc.thread_merge(&w.kitchen, &w.garden).await.expect("merged");
    w.s.svc.thread_unmerge(&w.garden).await.expect("the first undo");
    let garden = whole(&w.s, &w.garden).await;
    assert!(w.s.svc.thread_unmerge(&w.garden).await.is_err(), "a second undo is refused");
    assert_eq!(whole(&w.s, &w.garden).await, garden, "nothing written");
}
