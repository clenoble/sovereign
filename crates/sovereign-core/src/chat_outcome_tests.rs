//! A chat turn always ends with an explicit outcome (spec ch. 08, *Action Gravity*, decided by
//! Céline 2026-10-16): the writes that ran and their result; that the assistant only answered;
//! or that the turn failed or stopped, with the reason. "Done" is said only of a write that
//! succeeded. In v0.0.10 every chat turn was shown as "✓ Done: chat", including failed
//! generation, a declined write, a failed write and an injection stop (cloud review, finding 14).
//!
//! These tests pin the outcome type and the one sentence every UI shows for it
//! (`ChatOutcome::summary_line`), by property, not by exact wording. Whether the orchestrator
//! fills the outcome correctly on each path is checked at review: there is no harness yet that
//! runs a chat turn without a real model (that needs C-HOST's host wiring).

use super::{ChatEnd, ChatOutcome, WriteOutcome, WriteResult};

fn write(tool: &str, outcome: WriteOutcome) -> WriteResult {
    WriteResult { tool: tool.to_string(), outcome }
}

fn line(o: &ChatOutcome) -> String {
    let s = o.summary_line();
    assert!(!s.trim().is_empty(), "an outcome line is never empty: {o:?}");
    s
}

fn says_done(s: &str) -> bool {
    s.contains("Done") || s.contains('✓')
}

#[test]
fn a_turn_that_only_answered_is_not_shown_as_done() {
    let o = ChatOutcome { writes: vec![], end: ChatEnd::Completed };
    let s = line(&o);
    assert!(!says_done(&s), "answered only, yet: {s:?}");
}

#[test]
fn a_successful_write_is_shown_as_done_with_its_name() {
    let o = ChatOutcome {
        writes: vec![write("rename_thread", WriteOutcome::Succeeded)],
        end: ChatEnd::Completed,
    };
    let s = line(&o);
    assert!(says_done(&s), "{s:?}");
    assert!(s.contains("rename_thread"), "{s:?}");
}

#[test]
fn a_declined_write_is_not_shown_as_done() {
    let o = ChatOutcome {
        writes: vec![write("delete_thread", WriteOutcome::Declined)],
        end: ChatEnd::Completed,
    };
    let s = line(&o);
    assert!(!says_done(&s), "{s:?}");
    assert!(s.contains("delete_thread"), "the declined write is named: {s:?}");
}

#[test]
fn a_failed_write_shows_its_reason_and_is_not_done() {
    let o = ChatOutcome {
        writes: vec![write("move_document", WriteOutcome::Failed("thread not found".into()))],
        end: ChatEnd::Completed,
    };
    let s = line(&o);
    assert!(!says_done(&s), "{s:?}");
    assert!(s.contains("thread not found"), "{s:?}");
}

#[test]
fn a_failed_generation_shows_its_reason_and_is_not_done() {
    let o = ChatOutcome { writes: vec![], end: ChatEnd::GenerationFailed("model not loaded".into()) };
    let s = line(&o);
    assert!(!says_done(&s), "{s:?}");
    assert!(s.contains("model not loaded"), "{s:?}");
}

#[test]
fn a_turn_stopped_for_flagged_content_says_so_and_is_not_done() {
    let o = ChatOutcome { writes: vec![], end: ChatEnd::StoppedForInjection };
    let s = line(&o);
    assert!(!says_done(&s), "{s:?}");
    let lower = s.to_lowercase();
    assert!(lower.contains("stop"), "the line says the turn was stopped: {s:?}");
}

#[test]
fn mixed_results_show_each_write_and_done_only_for_the_success() {
    let o = ChatOutcome {
        writes: vec![
            write("rename_thread", WriteOutcome::Succeeded),
            write("move_document", WriteOutcome::Failed("locked".into())),
            write("delete_thread", WriteOutcome::Declined),
        ],
        end: ChatEnd::Completed,
    };
    let s = line(&o);
    assert!(s.contains("rename_thread") && s.contains("move_document") && s.contains("delete_thread"), "{s:?}");
    assert!(s.contains("locked"), "{s:?}");
    assert_eq!(s.matches("Done").count(), 1, "\"Done\" once, for the one success: {s:?}");
}

#[test]
fn writes_before_a_failed_generation_are_still_reported() {
    let o = ChatOutcome {
        writes: vec![write("create_thread", WriteOutcome::Succeeded)],
        end: ChatEnd::GenerationFailed("timeout".into()),
    };
    let s = line(&o);
    assert!(s.contains("create_thread") && s.contains("timeout"), "{s:?}");
}

#[test]
fn the_outcome_travels_as_an_event() {
    // The UI receives the outcome itself, not a bare success flag.
    let o = ChatOutcome { writes: vec![], end: ChatEnd::Completed };
    let ev = super::OrchestratorEvent::ChatTurnEnded { outcome: o.clone() };
    match ev {
        super::OrchestratorEvent::ChatTurnEnded { outcome } => assert_eq!(outcome, o),
        _ => unreachable!(),
    }
}
