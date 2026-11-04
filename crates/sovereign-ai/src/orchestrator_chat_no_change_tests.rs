//! v0.0.12 item 11b — **"Done" only for a real change, in a conversation too**
//! (`doc/plans/V0012-item11b-chat-no-change.md`, C1–C4; Céline, 2026-10-28).
//! Item 11 made the direct-request path say "No change" for a change that did
//! not happen (`orchestrator_outcome_words_tests.rs`, `b2_no_change_is_said_as_such`);
//! the chat loop, where the assistant acts through its write tools inside a
//! conversation turn, still said "✓ Done: rename_thread" for a rename to the
//! same name.
//!
//! Every test here runs a whole chat turn through the chat loop's real tool
//! path: `execute_action("chat")` → the agent loop → the fake model answers a
//! tool call → the write is proposed (the workspace context arms the
//! data-plane gate, so every write is proposed) → the decision → the write
//! tool → the turn's `ChatTurnEnded` outcome. The store, the stamps and the
//! events are read before and after.
//!
//! - **C2** — `rename_thread` to the same name and `move_document` into the
//!   document's own thread: no write, no stamp, no rename/move event; the
//!   write's outcome is no change, with the reason the contract gives. A
//!   case-only rename is a real rename (item 11 compares the names exactly).
//! - **C3** — a real rename and a real move still read "✓ Done: <tool>";
//!   failures and declines read as before; a turn with no write still says
//!   "Answered (no changes made)".
//! - **C4** — the turn's trace records a no-change write as executed without
//!   change (never "approved, and executed"), whether it was proposed or not.
//!
//! **API named here (does not exist yet — this module does not compile until
//! it does):** `sovereign_core::interfaces::WriteOutcome::NoChange(String)`
//! (the reason), and `crate::chat_trace::TraceStep::NoChange(String)` (the
//! same reason, rendered as executed without change).
//!
//! Written before the code; the implementer does not edit this file.

use super::assistant_harness::*;
use super::*;
use crate::chat_trace::TraceStep;

// ------------------------------------------------------------- helpers --

/// The model's answer calling `name` with `args`.
fn tool_call(name: &str, args: serde_json::Value) -> String {
    format!("<tool_call>\n{}\n</tool_call>", serde_json::json!({ "name": name, "arguments": args }))
}

/// A model that answers the first call of the turn with `first`, and every
/// later call (after the tool's result) with a plain answer.
fn first_then_answer(first: String) -> Script {
    Arc::new(move |_: &str, _: u32, i: usize| if i == 0 { first.clone() } else { "All set.".to_string() })
}

/// Runs one chat turn on `message` through `execute_action("chat")`,
/// answering every proposal with `decision`. Returns every event and the
/// turn's one outcome.
async fn chat_turn(w: &mut World, message: &str, decision: ActionDecision) -> (Vec<OrchestratorEvent>, ChatOutcome) {
    let (dtx, drx) = tokio::sync::mpsc::channel(8);
    w.orch.set_decision_rx(drx);
    w.rec.take_events();
    let rec = w.rec.clone();
    let done = std::sync::atomic::AtomicBool::new(false);
    let mut seen: Vec<OrchestratorEvent> = vec![];
    let orch = &w.orch;
    let turn = async {
        let r = orch.execute_action("chat", None, message).await;
        done.store(true, std::sync::atomic::Ordering::SeqCst);
        r
    };
    let driver = async {
        for _ in 0..20_000 {
            let new = rec.take_events();
            let n = proposals(&new).len();
            seen.extend(new);
            for _ in 0..n {
                let _ = dtx.send(decision.clone()).await;
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
    let outcomes: Vec<ChatOutcome> = seen
        .iter()
        .filter_map(|e| match e {
            OrchestratorEvent::ChatTurnEnded { outcome } => Some(outcome.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(outcomes.len(), 1, "one ChatTurnEnded per turn: {seen:?}");
    let o = outcomes.into_iter().next().unwrap();
    (seen, o)
}

fn renamed_events(evs: &[OrchestratorEvent]) -> usize {
    evs.iter().filter(|e| matches!(e, OrchestratorEvent::ThreadRenamed { .. })).count()
}

fn moved_events(evs: &[OrchestratorEvent]) -> usize {
    evs.iter().filter(|e| matches!(e, OrchestratorEvent::DocumentMoved { .. })).count()
}

/// The turn's one write.
fn the_write(o: &ChatOutcome, what: &str) -> WriteResult {
    assert_eq!(o.writes.len(), 1, "{what}: one write: {o:?}");
    o.writes[0].clone()
}

fn rename(old: &str, new: &str) -> String {
    tool_call("rename_thread", serde_json::json!({ "old_name": old, "new_name": new }))
}

fn move_doc(title: &str, thread: &str) -> String {
    tool_call("move_document", serde_json::json!({ "document_title": title, "thread_name": thread }))
}

// ----------------------------------------------------------------- C2 --

/// C2: `rename_thread` to the name the thread already has — no write, the
/// thread's stamp unchanged, no rename event; the outcome is no change, its
/// reason naming the thread; the line says "No change", never "Done".
#[tokio::test]
async fn c2_chat_rename_to_the_same_name_is_no_change() {
    let mut w = world("c2-rename-same", 4096, first_then_answer(rename("Plans", "Plans"))).await;
    w.thread("Garden").await;
    let plans = w.thread("Plans").await;
    let stamp = w.docs.thread_stamp(&plans).await.unwrap();
    let before = snapshot(&w.store).await;

    let (evs, o) = chat_turn(&mut w, "rename the thread Plans to Plans", ActionDecision::Approve).await;

    assert_eq!(snapshot(&w.store).await, before, "nothing written");
    assert_eq!(w.docs.thread_stamp(&plans).await.unwrap(), stamp, "no stamp: nothing travels");
    assert_eq!(renamed_events(&evs), 0, "no rename event for a rename that did not happen: {evs:?}");
    let wr = the_write(&o, "rename same");
    assert_eq!(wr.tool, "rename_thread");
    match &wr.outcome {
        WriteOutcome::NoChange(reason) => {
            assert!(reason.contains("the thread is already named 'Plans'"), "the reason names the thread: {reason:?}")
        }
        other => panic!("a rename to the same name is no change, got {other:?}"),
    }
    let line = o.summary_line();
    assert!(line.starts_with("No change: rename_thread"), "{line}");
    assert!(line.contains("the thread is already named 'Plans'"), "{line}");
    assert!(!says_done(&line), "{line}");
}

/// C2: a rename that changes only the case is a real rename (item 11 compares
/// the names exactly): written, stamped, said as done.
#[tokio::test]
async fn c2_chat_case_only_rename_is_a_real_change() {
    let mut w = world("c2-rename-case", 4096, first_then_answer(rename("Plans", "plans"))).await;
    w.thread("Garden").await;
    let plans = w.thread("Plans").await;
    let stamp = w.docs.thread_stamp(&plans).await.unwrap();

    let (evs, o) = chat_turn(&mut w, "rename the thread Plans to plans", ActionDecision::Approve).await;

    assert_eq!(w.thread_row(&plans).await.name, "plans", "renamed");
    assert!(w.docs.thread_stamp(&plans).await.unwrap() > stamp, "a new stamp: the rename travels");
    assert_eq!(renamed_events(&evs), 1, "{evs:?}");
    assert_eq!(the_write(&o, "rename case").outcome, WriteOutcome::Succeeded);
    assert_eq!(o.summary_line(), "✓ Done: rename_thread");
}

/// C2: `move_document` into the thread the document is already in — no
/// write, the document's stamp unchanged, no move event; the outcome is no
/// change, its reason naming the document and the thread.
#[tokio::test]
async fn c2_chat_move_into_its_own_thread_is_no_change() {
    let mut w = world("c2-move-same", 4096, first_then_answer(move_doc("Seeds", "Garden"))).await;
    let garden = w.thread("Garden").await;
    w.thread("Kitchen").await;
    let seeds = w.doc("Seeds", &garden, "seed list").await;
    let stamp = w.docs.stored_state(&seeds).await.unwrap().meta.stamp;
    let before = snapshot(&w.store).await;

    let (evs, o) = chat_turn(&mut w, "move Seeds to Garden", ActionDecision::Approve).await;

    assert_eq!(snapshot(&w.store).await, before, "nothing written");
    assert_eq!(w.docs.stored_state(&seeds).await.unwrap().meta.stamp, stamp, "no stamp: nothing travels");
    assert_eq!(w.thread_of(&seeds).await, garden);
    assert_eq!(moved_events(&evs), 0, "no move event for a move that did not happen: {evs:?}");
    let wr = the_write(&o, "move same");
    assert_eq!(wr.tool, "move_document");
    match &wr.outcome {
        WriteOutcome::NoChange(reason) => {
            assert!(reason.contains("'Seeds' is already in 'Garden'"), "the reason names both: {reason:?}")
        }
        other => panic!("a move into its own thread is no change, got {other:?}"),
    }
    let line = o.summary_line();
    assert!(line.starts_with("No change: move_document"), "{line}");
    assert!(line.contains("'Seeds' is already in 'Garden'"), "{line}");
    assert!(!says_done(&line), "{line}");
}

// ----------------------------------------------------------------- C3 --

/// C3: a real rename still reads "✓ Done: rename_thread".
#[tokio::test]
async fn c3_chat_real_rename_still_says_done() {
    let mut w = world("c3-rename", 4096, first_then_answer(rename("Plans", "Roadmap"))).await;
    w.thread("Garden").await;
    let plans = w.thread("Plans").await;
    let stamp = w.docs.thread_stamp(&plans).await.unwrap();

    let (evs, o) = chat_turn(&mut w, "rename Plans to Roadmap", ActionDecision::Approve).await;

    assert_eq!(w.thread_row(&plans).await.name, "Roadmap");
    assert!(w.docs.thread_stamp(&plans).await.unwrap() > stamp);
    assert_eq!(renamed_events(&evs), 1, "{evs:?}");
    assert_eq!(the_write(&o, "rename").outcome, WriteOutcome::Succeeded);
    assert_eq!(o.summary_line(), "✓ Done: rename_thread");
}

/// C3: a real move still reads "✓ Done: move_document".
#[tokio::test]
async fn c3_chat_real_move_still_says_done() {
    let mut w = world("c3-move", 4096, first_then_answer(move_doc("Pans", "Garden"))).await;
    let garden = w.thread("Garden").await;
    let kitchen = w.thread("Kitchen").await;
    let pans = w.doc("Pans", &kitchen, "pans").await;
    let stamp = w.docs.stored_state(&pans).await.unwrap().meta.stamp;

    let (evs, o) = chat_turn(&mut w, "move Pans to Garden", ActionDecision::Approve).await;

    assert_eq!(w.thread_of(&pans).await, garden);
    assert!(w.docs.stored_state(&pans).await.unwrap().meta.stamp > stamp);
    assert_eq!(moved_events(&evs), 1, "{evs:?}");
    assert_eq!(the_write(&o, "move").outcome, WriteOutcome::Succeeded);
    assert_eq!(o.summary_line(), "✓ Done: move_document");
}

/// C3: a failed write still reads "✗ Failed: <tool> (<reason>)", and is not
/// taken for a no-change.
#[tokio::test]
async fn c3_chat_failure_reads_as_before() {
    let mut w = world("c3-fail", 4096, first_then_answer(rename("Ghost", "Roadmap"))).await;
    w.thread("Garden").await;
    let before = snapshot(&w.store).await;

    let (_evs, o) = chat_turn(&mut w, "rename Ghost to Roadmap", ActionDecision::Approve).await;

    assert_eq!(snapshot(&w.store).await, before);
    let wr = the_write(&o, "fail");
    assert!(matches!(wr.outcome, WriteOutcome::Failed(_)), "{wr:?}");
    let line = o.summary_line();
    assert!(line.starts_with("✗ Failed: rename_thread ("), "{line}");
    assert!(!says_done(&line), "{line}");
}

/// C3: a declined write still reads "Declined: <tool>", nothing written.
#[tokio::test]
async fn c3_chat_decline_reads_as_before() {
    let mut w = world("c3-decline", 4096, first_then_answer(rename("Plans", "Roadmap"))).await;
    w.thread("Garden").await;
    let plans = w.thread("Plans").await;

    let (_evs, o) = chat_turn(&mut w, "rename Plans to Roadmap", ActionDecision::Reject("not now".into())).await;

    assert_eq!(w.thread_row(&plans).await.name, "Plans");
    assert_eq!(the_write(&o, "decline").outcome, WriteOutcome::Declined);
    assert_eq!(o.summary_line(), "Declined: rename_thread");
}

/// C3: a turn that ran no write still says "Answered (no changes made)".
#[tokio::test]
async fn c3_chat_no_write_still_says_answered() {
    let mut w = world("c3-answer", 4096, answer("Your threads are Garden and Plans.")).await;
    w.thread("Garden").await;
    w.thread("Plans").await;

    let (_evs, o) = chat_turn(&mut w, "what threads do I have?", ActionDecision::Approve).await;

    assert!(o.writes.is_empty(), "{o:?}");
    assert_eq!(o.summary_line(), "Answered (no changes made)");
}

// ----------------------------------------------------------------- C4 --

/// C4: a turn whose every round is a no-change rename reaches the round
/// limit; its saved trace records each of those writes as executed without
/// change, with the reason — never as "approved, and executed" — and the
/// turn's line names no "Done".
#[tokio::test]
async fn c4_chat_trace_records_a_no_change_write() {
    let call = rename("Plans", "Plans");
    let mut w = world("c4-trace", 4096, Arc::new(move |_: &str, _: u32, _: usize| call.clone())).await;
    w.thread("Garden").await;
    w.thread("Plans").await;

    let (_evs, o) = chat_turn(&mut w, "rename Plans to Plans", ActionDecision::Approve).await;

    assert!(!o.writes.is_empty(), "{o:?}");
    for wr in &o.writes {
        assert!(matches!(wr.outcome, WriteOutcome::NoChange(_)), "{wr:?}");
    }
    assert!(!says_done(&o.summary_line()), "{}", o.summary_line());
    let id = o.trace_doc_id().expect("the turn reached the round limit and its trace was saved").to_string();
    let text = w.docs.read(&id).await.unwrap().plain_text();
    let came_of_it: Vec<&str> = text.lines().filter(|l| l.starts_with("What came of it:")).collect();
    assert_eq!(came_of_it.len(), o.writes.len(), "one line per round:\n{text}");
    for l in came_of_it {
        let lower = l.to_lowercase();
        assert!(lower.contains("without change"), "executed without change: {l:?}");
        assert!(l.contains("the thread is already named 'Plans'"), "with its reason: {l:?}");
        assert!(!lower.contains("approved, and executed"), "{l:?}");
    }
}

/// C4: the trace step of a no-change write is the same whether the write was
/// proposed and approved or ran without a proposal (trust is untouched: an
/// approval is the person's decision at the proposal, whatever follows).
#[test]
fn c4_trace_step_of_a_no_change_write() {
    let w = WriteResult {
        tool: "rename_thread".into(),
        outcome: WriteOutcome::NoChange("the thread is already named 'Plans'".into()),
    };
    for approved in [true, false] {
        assert_eq!(
            trace_step_of(&w, approved),
            TraceStep::NoChange("the thread is already named 'Plans'".into()),
            "approved={approved}"
        );
    }
}
