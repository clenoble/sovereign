//! When a chat turn reaches the round limit (5), it is **not a failure**: its outcome says the
//! assistant stopped after 5 steps without a final answer, lists what was done, invites the user
//! to rephrase or split the request, and says where the trace of the turn is — the thread
//! "Assistant traces" and the trace's exact title — or says the trace could not be saved, naming
//! no document (spec ch. 08, *Chat agent loop*, v2.0.35, v2.0.39; Céline 2026-10-17 and -18).
//! Nothing opens by itself, so the outcome offers nothing to open.
//!
//! `ChatEnd::RoundLimit { steps, trace }` with
//! `TraceLink::{Saved { doc_id, title }, NotSaved { reason }}`; `ChatOutcome::trace_doc_id()`
//! gives the saved document's id. Wording is checked by property.
//! (Until v2.0.35 the limit was reported as `GenerationFailed("no answer after N tool steps")`.)

use super::{ChatEnd, ChatOutcome, TraceLink, WriteOutcome, WriteResult};

fn at_limit(writes: Vec<WriteResult>, trace: TraceLink) -> ChatOutcome {
    ChatOutcome { writes, end: ChatEnd::RoundLimit { steps: 5, trace } }
}

const TITLE: &str = "Assistant trace — 2026-10-18 05:23:00 UTC";

fn saved() -> TraceLink {
    TraceLink::Saved { doc_id: "document:trace1".into(), title: TITLE.into() }
}

fn write(tool: &str, outcome: WriteOutcome) -> WriteResult {
    WriteResult { tool: tool.to_string(), outcome }
}

#[test]
fn the_limit_is_reported_as_a_stop_not_a_failure() {
    let s = at_limit(vec![], saved()).summary_line();
    let lower = s.to_lowercase();
    assert!(s.contains('5'), "says how many steps: {s:?}");
    assert!(lower.contains("stop"), "says it stopped: {s:?}");
    assert!(!lower.contains("failed") && !s.contains('✗'), "not a failure: {s:?}");
}

#[test]
fn the_limit_invites_a_rephrase_or_a_split() {
    let lower = at_limit(vec![], saved()).summary_line().to_lowercase();
    assert!(lower.contains("rephrase") || lower.contains("split"), "{lower:?}");
}

#[test]
fn what_was_done_before_the_limit_is_listed() {
    let o = at_limit(
        vec![
            write("create_thread", WriteOutcome::Succeeded),
            write("move_document", WriteOutcome::Failed("locked".into())),
        ],
        saved(),
    );
    let s = o.summary_line();
    assert!(s.contains("create_thread") && s.contains("move_document") && s.contains("locked"), "{s:?}");
    assert_eq!(s.matches("Done").count(), 1, "\"Done\" only for the success: {s:?}");
}

#[test]
fn a_saved_trace_is_named_by_its_thread_and_exact_title() {
    let o = at_limit(vec![], saved());
    assert_eq!(o.trace_doc_id(), Some("document:trace1"));
    let s = o.summary_line();
    assert!(s.contains("Assistant traces"), "names the thread: {s:?}");
    assert!(s.contains(TITLE), "gives the exact title: {s:?}");
}

#[test]
fn a_saved_trace_offers_nothing_to_open() {
    // Nothing opens by itself and chat lines carry no action (v2.0.39).
    let lower = at_limit(vec![], saved()).summary_line().to_lowercase();
    assert!(!lower.contains("open it") && !lower.contains("opened"), "{lower:?}");
}

#[test]
fn a_trace_that_could_not_be_saved_says_so_and_names_no_document() {
    let o = at_limit(vec![], TraceLink::NotSaved { reason: "document service unavailable".into() });
    assert_eq!(o.trace_doc_id(), None);
    let s = o.summary_line();
    assert!(s.contains("document service unavailable"), "gives the reason: {s:?}");
    assert!(s.to_lowercase().contains("trace"), "{s:?}");
    assert!(!s.contains("Assistant trace —"), "names no document: {s:?}");
}

#[test]
fn other_turn_ends_have_no_trace() {
    for end in [
        ChatEnd::Completed,
        ChatEnd::GenerationFailed("x".into()),
        ChatEnd::StoppedForInjection,
    ] {
        assert_eq!(ChatOutcome { writes: vec![], end }.trace_doc_id(), None);
    }
}
