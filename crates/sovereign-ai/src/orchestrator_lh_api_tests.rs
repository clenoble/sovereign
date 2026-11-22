//! v0.0.13 lot LH — the C-HOST guarantees that **need the API added by the
//! « types and signatures » commit** (spec ch. 08, *Action targets*, v2.0.78:
//! AT1's action target, AT6's request id and deadline, AT7's undo).
//! Companion of `orchestrator_lh_tests.rs`.
//!
//! **The API these tests use** (signatures in place, not built):
//! - AT1 — `ProposedAction::target: sovereign_core::security::ActionTarget
//!   { kind: TargetKind, id, title, revision }`, today `ActionTarget::default()`
//!   (`TargetKind::None`).
//! - AT6 — `ProposedAction::request_id: RequestId` (today the default),
//!   `ProposedAction::deadline` (today `ProposedAction::NO_DEADLINE`), and
//!   `security::RequestDecision { request_id, decision }`, received through
//!   `Orchestrator::set_request_decision_rx` (today `unimplemented!`).
//! - AT7 — `Orchestrator::undo(&RequestId) -> ActionOutcome` (today
//!   `unimplemented!`), for the actions that have an undo: rename, move, merge
//!   (Céline, 2026-11-14).
//!
//! The AT1 and AT7 tests answer their proposals through the existing decision
//! channel (`set_decision_rx`, `orchestrator_lh_tests::drive`), so that each
//! fails on what it tests — the target, the undo — and not on AT6's channel.
//!
//! Written before the code by another author (PR-0011); the implementer does
//! not edit this file.

use super::assistant_harness::*;
use super::lh_tests::{drive, edit_text};
use super::*;
use sovereign_core::security::{RequestDecision, RequestId, TargetKind};

/// A router that classifies every query as `action` on `target`.
fn route_all(action: &str, target: Option<&str>) -> Script {
    let a = action.to_string();
    let t = target.map(str::to_string);
    Arc::new(move |_, _, _| intent_json(&a, t.as_deref()))
}

/// Runs `query` through `handle_query` with the channel of bound decisions
/// (AT6); each proposal is handed to `answer`, which returns the decisions to
/// send for it (in order). Returns every event.
async fn drive_bound<F>(w: &mut World, query: &str, mut answer: F) -> Vec<OrchestratorEvent>
where
    F: FnMut(&ProposedAction) -> Vec<RequestDecision>,
{
    let (dtx, drx) = tokio::sync::mpsc::channel(8);
    w.orch.set_request_decision_rx(drx);
    w.rec.take_events();
    let rec = w.rec.clone();
    let done = std::sync::atomic::AtomicBool::new(false);
    let mut seen: Vec<OrchestratorEvent> = vec![];
    let orch = &w.orch;
    let turn = async {
        let r = orch.handle_query(query).await;
        done.store(true, std::sync::atomic::Ordering::SeqCst);
        r
    };
    let driver = async {
        for _ in 0..20_000 {
            let new = rec.take_events();
            let ps = proposals(&new);
            seen.extend(new);
            for p in ps {
                for d in answer(&p) {
                    let _ = dtx.send(d).await;
                }
            }
            if done.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    };
    let (res, ()) = tokio::join!(turn, driver);
    res.unwrap();
    seen.extend(w.rec.take_events());
    seen
}

fn bound(p: &ProposedAction, decision: ActionDecision) -> RequestDecision {
    RequestDecision { request_id: p.request_id, decision }
}

fn no() -> ActionDecision {
    ActionDecision::Reject("no".into())
}

// ------------------------------------------------------------------ AT1 --

/// AT1: a proposal carries its action target — kind, id, displayed title. A
/// thread deletion: the thread's id and name; a move: the document's id and
/// title.
#[tokio::test]
async fn at1_proposal_carries_its_action_target() {
    let mut w = world("lh-api-at1", 4096, route_all("delete_thread", Some("Work"))).await;
    let work = w.thread("Work").await;
    let ps = proposals(&drive(&mut w, "delete the thread Work", vec![no()], |_| async {}).await);
    assert_eq!(ps.len(), 1);
    assert_eq!(ps[0].target.kind, TargetKind::Thread, "{:?}", ps[0].target);
    assert_eq!(ps[0].target.id, work);
    assert_eq!(ps[0].target.title, "Work");

    let mut w = world("lh-api-at1-move", 4096, route_all("move_document", Some("Seeds to Kitchen"))).await;
    let garden = w.thread("Garden").await;
    w.thread("Kitchen").await;
    let seeds = w.doc("Seeds", &garden, "seed list").await;
    let ps = proposals(&drive(&mut w, "move 'Seeds' to 'Kitchen'", vec![no()], |_| async {}).await);
    assert_eq!(ps.len(), 1);
    assert_eq!(ps[0].target.kind, TargetKind::Document, "{:?}", ps[0].target);
    assert_eq!(ps[0].target.id, seeds);
    assert_eq!(ps[0].target.title, "Seeds");
}

/// AT1: the revision in the target is the one seen — two proposals on the same
/// document, one before and one after an edit, carry different revisions, and
/// neither is empty.
#[tokio::test]
async fn at1_target_revision_is_the_one_seen() {
    let mut w = world("lh-api-at1-rev", 4096, route_all("restore", Some("Notes"))).await;
    let t = w.thread("Garden").await;
    let notes = w.doc("Notes", &t, "first").await;
    edit_text(&w.docs, &notes, "second").await;
    let first = proposals(&drive(&mut w, "restore Notes", vec![no()], |_| async {}).await);
    edit_text(&w.docs, &notes, "third").await;
    let second = proposals(&drive(&mut w, "restore Notes", vec![no()], |_| async {}).await);
    assert_eq!((first.len(), second.len()), (1, 1));
    assert!(!first[0].target.revision.is_empty(), "the revision seen is recorded: {:?}", first[0].target);
    assert_ne!(first[0].target.revision, second[0].target.revision, "the revision seen changed with the edit");
}

// ------------------------------------------------------------------ AT6 --

/// AT6: each proposal has its own request id.
#[tokio::test]
async fn at6_each_proposal_has_its_own_request_id() {
    let mut w = world("lh-api-at6-ids", 4096, route_all("delete_thread", Some("Work"))).await;
    w.thread("Work").await;
    let a = proposals(&drive_bound(&mut w, "delete Work", |p| vec![bound(p, no())]).await);
    let b = proposals(&drive_bound(&mut w, "delete Work", |p| vec![bound(p, no())]).await);
    assert_eq!((a.len(), b.len()), (1, 1));
    assert_ne!(a[0].request_id, b[0].request_id);
}

/// AT6: a decision applies only to the proposal with its id. An approval
/// carrying an earlier proposal's id does not approve the one waiting; the
/// decision with the right id does.
#[tokio::test]
async fn at6_decision_applies_only_to_its_own_proposal() {
    let mut w = world("lh-api-at6-bind", 4096, route_all("delete_thread", Some("Work"))).await;
    let work = w.thread("Work").await;
    let earlier = proposals(&drive_bound(&mut w, "delete Work", |p| vec![bound(p, no())]).await);
    assert_eq!(earlier.len(), 1);
    let stale: RequestId = earlier[0].request_id;

    // An approval for the earlier request, then a rejection for this one.
    let evs = drive_bound(&mut w, "delete Work", |p| {
        vec![RequestDecision { request_id: stale, decision: ActionDecision::Approve }, bound(p, no())]
    })
    .await;
    assert_eq!(proposals(&evs).len(), 1);
    assert!(w.is_live(&work).await, "an approval bound to another request approves nothing");

    // The approval bound to this request is applied.
    let evs = drive_bound(&mut w, "delete Work", |p| vec![bound(p, ActionDecision::Approve)]).await;
    assert_eq!(proposals(&evs).len(), 1);
    assert!(!w.is_live(&work).await, "approved by its own id: done");
}

/// AT6: the deadline runs from when the proposal was issued (120 s, ch. 08,
/// *Action Gravity*), and the proposal carries it, so a host counts down to it
/// rather than starting its own clock when it displays the proposal.
#[tokio::test]
async fn at6_deadline_runs_from_issue() {
    let mut w = world("lh-api-at6-deadline", 4096, route_all("delete_thread", Some("Work"))).await;
    w.thread("Work").await;
    let before = chrono::Utc::now();
    let mut seen_at = None;
    let evs = drive_bound(&mut w, "delete Work", |p| {
        seen_at.get_or_insert_with(chrono::Utc::now);
        vec![bound(p, no())]
    })
    .await;
    let ps = proposals(&evs);
    assert_eq!(ps.len(), 1);
    let seen_at = seen_at.expect("the proposal was seen");
    let window = chrono::Duration::seconds(120);
    assert!(
        ps[0].deadline >= before + window && ps[0].deadline <= seen_at + window,
        "deadline {} is 120 s after issue (issued between {before} and {seen_at})",
        ps[0].deadline
    );
}

// ------------------------------------------------------------------ AT7 --

/// AT7: the undo of a rename addresses the renamed thread by its id. After
/// the rename, a new thread takes the new name; the undo gives the old name
/// back to the thread that was renamed, and leaves the newcomer alone.
#[tokio::test]
async fn at7_undo_of_a_rename_addresses_the_same_id() {
    let mut w = world("lh-api-at7-rename", 4096, route_all("rename_thread", Some("Plans"))).await;
    let plans = w.thread("Plans").await;
    let ps = proposals(&drive(&mut w, "rename 'Plans' to 'Roadmap'", vec![ActionDecision::Approve], |_| async {}).await);
    assert_eq!(ps.len(), 1);
    assert_eq!(w.thread_row(&plans).await.name, "Roadmap");
    let newcomer = w.thread("Roadmap").await;

    let outcome = w.orch.undo(&ps[0].request_id).await;
    assert!(outcome.succeeded(), "{outcome:?}");
    assert_eq!(w.thread_row(&plans).await.name, "Plans", "the renamed thread, by its id");
    assert_eq!(w.thread_row(&newcomer).await.name, "Roadmap", "the newcomer is not touched");
}

/// AT7: the undo of a move addresses the moved document by its id — not a
/// document with the same title that arrived in the destination since.
#[tokio::test]
async fn at7_undo_of_a_move_addresses_the_same_id() {
    let mut w = world("lh-api-at7-move", 4096, route_all("move_document", Some("Seeds to Kitchen"))).await;
    let garden = w.thread("Garden").await;
    let kitchen = w.thread("Kitchen").await;
    let seeds = w.doc("Seeds", &garden, "seed list").await;
    let ps = proposals(&drive(&mut w, "move 'Seeds' to 'Kitchen'", vec![ActionDecision::Approve], |_| async {}).await);
    assert_eq!(ps.len(), 1);
    assert_eq!(w.thread_of(&seeds).await, kitchen);
    let twin = w.doc("Seeds", &kitchen, "another seed list").await;

    let outcome = w.orch.undo(&ps[0].request_id).await;
    assert!(outcome.succeeded(), "{outcome:?}");
    assert_eq!(w.thread_of(&seeds).await, garden, "the moved document goes back, by its id");
    assert_eq!(w.thread_of(&twin).await, kitchen, "the document with the same title stays");
}

/// AT7 with C-MERGE (v2.0.109; replaced by the B1 test author): the undo of a
/// merge addresses the merged thread by its id. A new thread named like it,
/// created since, is left alone. (Until v2.0.109 this test held that the
/// merged-away thread lives again and its documents move back, contradicted by
/// MG1 and MG5: a merge deletes nothing and moves nothing; its undo clears two
/// fields. The rest of the merge's undo is in `orchestrator_b1_merge_tests.rs`.)
#[tokio::test]
async fn at7_undo_of_a_merge_addresses_the_same_id() {
    let mut w = world("lh-api-at7-merge", 4096, route_all("merge_threads", Some("Kitchen to Garden"))).await;
    let garden = w.thread("Garden").await;
    let kitchen = w.thread("Kitchen").await;
    let ps = proposals(&drive(&mut w, "merge 'Garden' into 'Kitchen'", vec![ActionDecision::Approve], |_| async {}).await);
    assert_eq!(ps.len(), 1);
    assert_eq!(w.thread_row(&garden).await.merged_into.as_deref(), Some(kitchen.as_str()), "merged (MG1)");
    let newcomer = w.thread("Garden").await;

    let outcome = w.orch.undo(&ps[0].request_id).await;
    assert!(outcome.succeeded(), "{outcome:?}");
    assert_eq!(w.thread_row(&garden).await.merged_into, None, "the merged thread, by its id, is live again");
    assert_eq!(w.thread_row(&newcomer).await.merged_into, None, "the newcomer is not touched");
    assert!(w.is_live(&newcomer).await, "the newcomer is not touched");
}

// ------------------------------------------- AT7, an undo after a change --
// "An undo is refused if the object changed since the action, and the reply
// says why (Céline, 2026-11-14: « Refuser et le dire »). Changed means renamed
// again, moved again, or, for a merge, a merged thread active again. Nothing
// overwrites a later change." (spec v2.0.86)

/// The reason of a refused undo, or a panic naming what came back instead.
fn refused(outcome: &ActionOutcome, what: &str) -> String {
    match outcome {
        ActionOutcome::Failed { reason } => reason.clone(),
        other => panic!("{what}: the undo must be refused, it came back {other:?}"),
    }
}

/// AT7: a rename undone after the thread was renamed again is refused, says
/// why (the thread was renamed since), and changes nothing.
#[tokio::test]
async fn at7_undo_refused_when_the_thread_was_renamed_again() {
    let mut w = world("lh-api-at7-renamed-again", 4096, route_all("rename_thread", Some("Plans"))).await;
    let plans = w.thread("Plans").await;
    let ps = proposals(&drive(&mut w, "rename 'Plans' to 'Roadmap'", vec![ActionDecision::Approve], |_| async {}).await);
    assert_eq!(ps.len(), 1);
    assert_eq!(w.thread_row(&plans).await.name, "Roadmap");
    w.docs.thread_update(&plans, Some("Projects"), None).await.unwrap();
    let before = snapshot(&w.store).await;

    let outcome = w.orch.undo(&ps[0].request_id).await;
    let why = refused(&outcome, "renamed again");
    assert!(why.contains("Projects") || why.to_lowercase().contains("renamed"), "says why: {why}");
    assert!(!says_done(&why), "{why}");
    assert_eq!(snapshot(&w.store).await, before, "nothing overwrites the later rename");
    assert_eq!(w.thread_row(&plans).await.name, "Projects");
}

/// AT7: a move undone after the document was moved again is refused, says
/// why, and leaves the document where the later move put it.
#[tokio::test]
async fn at7_undo_refused_when_the_document_was_moved_again() {
    let mut w = world("lh-api-at7-moved-again", 4096, route_all("move_document", Some("Seeds to Kitchen"))).await;
    let garden = w.thread("Garden").await;
    let kitchen = w.thread("Kitchen").await;
    let shed = w.thread("Shed").await;
    let seeds = w.doc("Seeds", &garden, "seed list").await;
    let ps = proposals(&drive(&mut w, "move 'Seeds' to 'Kitchen'", vec![ActionDecision::Approve], |_| async {}).await);
    assert_eq!(ps.len(), 1);
    assert_eq!(w.thread_of(&seeds).await, kitchen);
    w.docs.move_to_thread(&seeds, &shed).await.unwrap();
    let before = snapshot(&w.store).await;

    let outcome = w.orch.undo(&ps[0].request_id).await;
    let why = refused(&outcome, "moved again");
    assert!(why.contains("Seeds") && why.to_lowercase().contains("moved"), "says why: {why}");
    assert!(!says_done(&why), "{why}");
    assert_eq!(snapshot(&w.store).await, before, "nothing overwrites the later move");
    assert_eq!(w.thread_of(&seeds).await, shed);
}

/// AT7 with C-MERGE (v2.0.109; replaced by the B1 test author): a merge undone
/// after its merge was undone since (the thread active again) is refused, says
/// why, and changes nothing. (Until v2.0.109 the thread was made active again
/// by restoring a deleted thread, and the document stayed in the kept thread;
/// contradicted by MG1: the merge deletes and moves nothing.)
#[tokio::test]
async fn at7_undo_refused_when_the_merged_thread_is_active_again() {
    let mut w = world("lh-api-at7-merge-active", 4096, route_all("merge_threads", Some("Kitchen to Garden"))).await;
    let garden = w.thread("Garden").await;
    let kitchen = w.thread("Kitchen").await;
    let seeds = w.doc("Seeds", &garden, "seed list").await;
    let ps = proposals(&drive(&mut w, "merge 'Garden' into 'Kitchen'", vec![ActionDecision::Approve], |_| async {}).await);
    assert_eq!(ps.len(), 1);
    assert_eq!(w.thread_row(&garden).await.merged_into.as_deref(), Some(kitchen.as_str()), "merged (MG1)");
    w.docs.thread_unmerge(&garden).await.unwrap();
    assert_eq!(w.thread_row(&garden).await.merged_into, None, "active again");
    let before = snapshot(&w.store).await;

    let outcome = w.orch.undo(&ps[0].request_id).await;
    let why = refused(&outcome, "merged thread active again");
    assert!(why.contains("Garden"), "says why, naming the thread: {why}");
    assert!(!says_done(&why), "{why}");
    assert_eq!(snapshot(&w.store).await, before, "nothing overwrites the later change");
    assert_eq!(w.thread_of(&seeds).await, garden, "the document never left Garden");
}

// ------------------------- AT7, a move's undo when its thread is gone --
// "A move's undo is refused too when the thread it came from is gone since
// (deleted, or merged away). The reply says that the thread is gone, and the
// document stays where it is." (spec v2.0.89, Fable 0040)

/// The words a reply uses to say a thread is gone.
fn says_gone(why: &str) -> bool {
    let w = why.to_lowercase();
    ["gone", "deleted", "no longer exists", "merged", "removed"].iter().any(|s| w.contains(s))
}

/// AT7 (v2.0.89): a move undone after the thread it came from was deleted is
/// refused; the reply names the thread and says it is gone (spec v2.0.90,
/// Céline 2026-11-14: « Nommer le fil »); the document stays where the move put
/// it, and nothing else changes.
/// Red against fae2d23 on the name alone: the document service refuses the move
/// (a thread not active), and the reply says « That thread does not exist »,
/// with no name. The refusal itself has its mutation (lh-tests-v3):
/// `move_to_thread` skipping that check → the document goes back into the
/// deleted thread.
#[tokio::test]
async fn at7_move_undo_refused_when_the_thread_it_came_from_was_deleted() {
    let mut w = world("lh-api-at7-from-deleted", 4096, route_all("move_document", Some("Seeds to Kitchen"))).await;
    let garden = w.thread("Garden").await;
    let kitchen = w.thread("Kitchen").await;
    let seeds = w.doc("Seeds", &garden, "seed list").await;
    let ps = proposals(&drive(&mut w, "move 'Seeds' to 'Kitchen'", vec![ActionDecision::Approve], |_| async {}).await);
    assert_eq!(ps.len(), 1);
    assert_eq!(w.thread_of(&seeds).await, kitchen);
    sovereign_docs::delete_thread_to_trash(&w.docs, w.store.as_ref(), &garden).await.unwrap();
    assert!(!w.is_live(&garden).await);
    let before = snapshot(&w.store).await;

    let outcome = w.orch.undo(&ps[0].request_id).await;
    let why = refused(&outcome, "the thread it came from was deleted");
    assert!(why.contains("Garden") && says_gone(&why), "names the thread and says it is gone: {why}");
    assert!(!says_done(&why), "{why}");
    assert_eq!(snapshot(&w.store).await, before, "nothing changed");
    assert_eq!(w.thread_of(&seeds).await, kitchen, "the document stays where it is");
}

/// AT7 (v2.0.89): a move undone after the thread it came from was merged away
/// into another thread is refused, names that thread and says it is gone, and
/// the document stays where the move put it. Red against fae2d23 on the name, as
/// above.
#[tokio::test]
async fn at7_move_undo_refused_when_the_thread_it_came_from_was_merged_away() {
    let mut w = world("lh-api-at7-from-merged", 4096, route_all("move_document", Some("Seeds to Kitchen"))).await;
    let garden = w.thread("Garden").await;
    let kitchen = w.thread("Kitchen").await;
    let shed = w.thread("Shed").await;
    let seeds = w.doc("Seeds", &garden, "seed list").await;
    w.doc("Tools", &garden, "tool list").await;
    let ps = proposals(&drive(&mut w, "move 'Seeds' to 'Kitchen'", vec![ActionDecision::Approve], |_| async {}).await);
    assert_eq!(ps.len(), 1);
    assert_eq!(w.thread_of(&seeds).await, kitchen);
    w.docs.thread_merge(&shed, &garden).await.unwrap();
    // C-MERGE (v2.0.109, MG1; the B1 test author): merged away is ended, not
    // deleted. Until then this line read `!w.is_live(&garden)`.
    assert_eq!(w.thread_row(&garden).await.merged_into.as_deref(), Some(shed.as_str()), "merged away");
    let before = snapshot(&w.store).await;

    let outcome = w.orch.undo(&ps[0].request_id).await;
    let why = refused(&outcome, "the thread it came from was merged away");
    assert!(why.contains("Garden") && says_gone(&why), "names the thread and says it is gone: {why}");
    assert!(!says_done(&why), "{why}");
    assert_eq!(snapshot(&w.store).await, before, "nothing changed");
    assert_eq!(w.thread_of(&seeds).await, kitchen, "the document stays where it is");
}
