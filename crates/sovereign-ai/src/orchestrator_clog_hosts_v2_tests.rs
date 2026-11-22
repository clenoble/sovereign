//! Contract C-LOG, the host lot's library side, second set (spec ch. 08, DG3 and DG4), written
//! before the code by its test author, against the two seams its implementer named:
//! `OrchestratorEvent::QuarantinedLogDiagnosed` and `Orchestrator::set_log_choices`.
//!
//! - **The event** tells a host that a chat turn's diagnosis tool found a quarantine, so it can
//!   offer « Show what remains readable » below the assistant turn that ends next (the Fable
//!   maille, by delegation of Céline, 2026-11-18). It comes before that turn's `ChatTurnEnded`,
//!   and never when there was nothing to diagnose or the tool was not called.
//! - **The host's choices:** the model is offered only the choices the host carries out, in the
//!   host's order; all three without a call. The app offers Keep and Delete (Céline, 2026-11-18:
//!   « Mettre l'export du téléphone au backlog »).
//!
//! On `clog_tests`' fixtures and the assistant harness, unchanged.

use super::clog_tests::*;
use super::*;
use crate::log_diagnosis::{self as dg, Choice};
use std::fs;

/// The model calls the diagnosis tool, then answers.
fn tool_then_answer() -> super::assistant_harness::Script {
    Arc::new(|_: &str, _: u32, i: usize| {
        if i == 0 {
            format!("<tool_call>\n{}\n</tool_call>", serde_json::json!({ "name": dg::DIAGNOSIS_TOOL, "arguments": {} }))
        } else {
            "Here is what the check found.".to_string()
        }
    })
}

/// A world whose logged-in persona's log is quarantined: three turns, then a line that is not
/// sealed, then a login.
async fn quarantined(tag: &str, script: super::assistant_harness::Script) -> (super::assistant_harness::World, PathBuf) {
    let w = super::assistant_harness::world(tag, 4096, script).await;
    let dir = w.orch.profile_dir.clone();
    let paths = OrchestratorStatePaths { dir: dir.clone(), beacon: dir.with_extension("beacon") };
    w.orch.set_session_log_key_for(&paths, KEY);
    for t in turns(tag, 3) {
        w.orch.session_log.with_log(|l| l.log_user_input("chat", &t, "chat"));
    }
    append(&dir.join("session_log.jsonl"), b"not a sealed line\n");
    w.rec.take_events();
    w.orch.set_session_log_key_for(&paths, KEY);
    assert!(
        session_notices(&w.rec.take_events()).iter().any(|n| n.starts_with(NOTICE_START)),
        "fixture: the log is quarantined"
    );
    assert_eq!(quarantines(&dir).len(), 1, "fixture: one quarantine");
    (w, dir)
}

fn position(events: &[OrchestratorEvent], f: impl Fn(&OrchestratorEvent) -> bool) -> Option<usize> {
    events.iter().position(f)
}

fn is_diagnosed(e: &OrchestratorEvent) -> bool {
    matches!(e, OrchestratorEvent::QuarantinedLogDiagnosed)
}

fn is_turn_end(e: &OrchestratorEvent) -> bool {
    matches!(e, OrchestratorEvent::ChatTurnEnded { .. })
}

// ── the event ─────────────────────────────────────────────────────────────

/// Her question, the tool called over a quarantine: the event is sent once, before the turn's end.
#[tokio::test]
async fn a_turn_whose_tool_found_a_quarantine_says_so_before_it_ends() {
    // DG3: the control below the turn
    let (w, dir) = quarantined(&format!("diag-event-{}", nanos()), tool_then_answer()).await;
    let _ = w.orch.handle_chat("what happened to my log?").await;
    let events = w.rec.take_events();
    assert!(w.rec.calls().len() >= 2, "fixture: the tool was called, then the model answered");
    let diagnosed = events.iter().filter(|e| is_diagnosed(e)).count();
    assert_eq!(diagnosed, 1, "the event is sent once in the turn: {events:?}");
    let (d, end) = (position(&events, is_diagnosed), position(&events, is_turn_end));
    assert!(end.is_some(), "fixture: the turn ends: {events:?}");
    assert!(d < end, "the event comes before the turn's end: {events:?}");
    let _ = fs::remove_dir_all(&dir);
}

/// The tool called with nothing to diagnose (it answers that there is no quarantine): no event.
#[tokio::test]
async fn a_turn_whose_tool_found_nothing_sends_no_event() {
    // DG3
    let w = super::assistant_harness::world(&format!("diag-none-{}", nanos()), 4096, tool_then_answer()).await;
    let dir = w.orch.profile_dir.clone();
    let paths = OrchestratorStatePaths { dir: dir.clone(), beacon: dir.with_extension("beacon") };
    w.orch.set_session_log_key_for(&paths, KEY);
    assert!(w.orch.log_diagnosis().is_none(), "fixture: no quarantine");
    w.rec.take_events();
    let _ = w.orch.handle_chat("what happened to my log?").await;
    let events = w.rec.take_events();
    assert!(!events.iter().any(is_diagnosed), "no quarantine, no event: {events:?}");
    let _ = fs::remove_dir_all(&dir);
}

/// A quarantine exists, but the model answers without calling the tool: no event.
#[tokio::test]
async fn a_turn_that_did_not_call_the_tool_sends_no_event() {
    // DG3
    let answer: super::assistant_harness::Script = Arc::new(|_: &str, _: u32, _: usize| "I cannot say.".to_string());
    let (w, dir) = quarantined(&format!("diag-notool-{}", nanos()), answer).await;
    let _ = w.orch.handle_chat("what happened to my log?").await;
    let events = w.rec.take_events();
    assert!(!events.iter().any(is_diagnosed), "the tool was not called, no event: {events:?}");
    let _ = fs::remove_dir_all(&dir);
}

// ── the host's choices ────────────────────────────────────────────────────

/// Without a call, the three choices, Keep the default.
#[tokio::test]
async fn without_a_call_the_three_choices_are_offered() {
    // DG4
    let (w, dir) = quarantined(&format!("choices-all-{}", nanos()), tool_then_answer()).await;
    let d = w.orch.log_diagnosis().expect("a diagnosis");
    assert_eq!(d.choices, vec![Choice::Keep, Choice::ExportForAnExpert, Choice::Delete]);
    assert_eq!(d.default_choice, Choice::Keep);
    let _ = fs::remove_dir_all(&dir);
}

/// The app's list, Keep and Delete: the diagnosis offers those two, and the model's prompt, after
/// the tool, names them and offers no export.
#[tokio::test]
async fn the_hosts_list_is_what_the_model_is_offered() {
    // DG4, the app (Céline, 2026-11-18)
    let (w, dir) = quarantined(&format!("choices-app-{}", nanos()), tool_then_answer()).await;
    w.orch.set_log_choices(vec![Choice::Keep, Choice::Delete]);
    let d = w.orch.log_diagnosis().expect("a diagnosis");
    assert_eq!(d.choices, vec![Choice::Keep, Choice::Delete], "the host's list");
    assert_eq!(d.default_choice, Choice::Keep, "Keep stays the default");

    let _ = w.orch.handle_chat("what happened to my log?").await;
    let calls = w.rec.calls();
    let last = &calls.last().expect("the model was asked").prompt;
    for c in [Choice::Keep, Choice::Delete] {
        let s = dg::choice_sentence(c);
        assert!(last.contains(&s), "{c:?} is offered, « {s} »:\n{last}");
    }
    let export = dg::choice_sentence(Choice::ExportForAnExpert);
    for (i, c) in calls.iter().enumerate() {
        assert!(!c.prompt.contains(&export), "call {i}: no export is offered:\n{}", c.prompt);
    }
    let _ = fs::remove_dir_all(&dir);
}

/// The host's order is kept.
#[tokio::test]
async fn the_hosts_order_is_kept() {
    // DG4
    let (w, dir) = quarantined(&format!("choices-order-{}", nanos()), tool_then_answer()).await;
    w.orch.set_log_choices(vec![Choice::Delete, Choice::Keep]);
    assert_eq!(w.orch.log_diagnosis().expect("a diagnosis").choices, vec![Choice::Delete, Choice::Keep]);
    let _ = fs::remove_dir_all(&dir);
}
