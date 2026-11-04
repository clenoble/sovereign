//! v0.0.12 item 11 — "Done" only for a real change (B1–B4,
//! `doc/plans/V0012-lot-assistant.md`; cloud review v0.0.11, S5). The action
//! path's outcome, as `ChatOutcome` is for a chat turn: what the executed
//! event carries and the one line every UI shows for it.
//!
//! **API named here (does not exist yet):**
//!
//! ```ignore
//! pub enum ActionOutcome {
//!     Changed { summary: String },   // the store changed; `summary` is the A3 sentence
//!     Reported { summary: String },  // read-only: what was found or produced (B1)
//!     NoChange { reason: String },   // nothing to change (B2); not done
//!     Failed { reason: String },     // not done: refused, not found, ambiguous, failed
//! }
//! impl ActionOutcome {
//!     pub fn summary_line(&self) -> String;
//!     pub fn changed(&self) -> bool;
//! }
//! OrchestratorEvent::ActionExecuted { action: String, success: bool, outcome: ActionOutcome }
//! ```
//!
//! Properties, not wording. Whether the orchestrator fills the outcome
//! correctly on each action is tested in
//! `sovereign-ai/src/orchestrator_outcome_words_tests.rs`.

use super::{ActionOutcome, OrchestratorEvent};

fn says_done(s: &str) -> bool {
    s.contains("Done") || s.contains('✓')
}

fn line(o: &ActionOutcome) -> String {
    let s = o.summary_line();
    assert!(!s.trim().is_empty(), "an outcome line is never empty: {o:?}");
    s
}

/// B4/A3: a change is shown with its sentence (it may say Done).
#[test]
fn a_change_is_shown_with_what_was_done() {
    let o = ActionOutcome::Changed { summary: "Deleted thread 'Work' (5 documents moved to the trash)".into() };
    let s = line(&o);
    assert!(s.contains("Deleted thread 'Work'") && s.contains("5 documents"), "{s}");
    assert!(o.changed());
}

/// B1: a read-only result is shown as what was found, never as Done.
#[test]
fn a_report_is_never_shown_as_done() {
    let o = ActionOutcome::Reported { summary: "Found 2 documents for 'seeds'".into() };
    let s = line(&o);
    assert!(s.contains("Found 2 documents"), "{s}");
    assert!(!says_done(&s), "{s}");
    assert!(!o.changed());
}

/// B2: no change is said as such, with its reason, never as Done.
#[test]
fn no_change_is_said_as_such() {
    let o = ActionOutcome::NoChange { reason: "'Seeds' is already in 'Garden'".into() };
    let s = line(&o);
    assert!(s.starts_with("No change"), "{s}");
    assert!(s.contains("'Seeds' is already in 'Garden'"), "{s}");
    assert!(!says_done(&s), "{s}");
    assert!(!o.changed());
}

/// B4: a failure (not found, ambiguous, refused) gives its reason and never
/// says Done — whatever the reason's wording.
#[test]
fn a_failure_gives_its_reason_and_never_says_done() {
    for reason in ["no thread is named 'Work' — did you mean 'Work Archive'?", "the workspace is locked", ""] {
        let o = ActionOutcome::Failed { reason: reason.into() };
        let s = line(&o);
        assert!(s.contains(reason), "{s}");
        assert!(!says_done(&s), "{s}");
        assert!(!o.changed());
    }
}

/// A read-only result whose own text happens to contain the word cannot be
/// turned into a Done line by the wrapper: only `Changed` adds the outcome
/// word. (The summary itself is the action's to word.)
#[test]
fn only_a_change_adds_the_outcome_word() {
    for o in [
        ActionOutcome::Reported { summary: "1 milestone".into() },
        ActionOutcome::NoChange { reason: "same name".into() },
        ActionOutcome::Failed { reason: "not found".into() },
    ] {
        assert!(!says_done(&o.summary_line()), "{o:?}: {}", o.summary_line());
    }
}

/// The executed event carries the outcome next to `success`, which keeps its
/// meaning for existing consumers.
#[test]
fn the_executed_event_carries_the_outcome() {
    let ev = OrchestratorEvent::ActionExecuted {
        action: "rename_thread".into(),
        success: false,
        outcome: ActionOutcome::NoChange { reason: "'Plans' already has that name".into() },
    };
    match ev.clone() {
        OrchestratorEvent::ActionExecuted { outcome, success, .. } => {
            assert!(!success);
            assert!(outcome.summary_line().starts_with("No change"));
        }
        other => panic!("{other:?}"),
    }
}
