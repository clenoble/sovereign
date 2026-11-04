//! v0.0.12 item 11b — "Done" only for a real change, in a conversation too
//! (`doc/plans/V0012-item11b-chat-no-change.md`, C1 and C3; Céline,
//! 2026-10-28). Item 11 made the direct-request path say "No change" for a
//! change that did not happen (`ActionOutcome::NoChange`); a chat turn's write
//! gets the same case here.
//!
//! **API named here (does not exist yet — this module does not compile until
//! it does):**
//!
//! ```ignore
//! pub enum WriteOutcome {
//!     Succeeded,
//!     Failed(String),
//!     Declined,
//!     NoChange(String), // nothing to change; the reason
//! }
//! ```
//!
//! - **C1** — `summary_line` writes a no-change write as
//!   "No change: <tool> (<reason>)", never "Done" or ✓.
//! - **C3** — the other lines are unchanged: "✓ Done: <tool>" for a write that
//!   succeeded, failures and declines as before, "Answered (no changes made)"
//!   for a turn with no write.
//!
//! Whether the chat loop fills the outcome correctly is tested through its
//! real tool path in `sovereign-ai/src/orchestrator_chat_no_change_tests.rs`.
//!
//! Written before the code; the implementer does not edit this file.

use super::{ChatEnd, ChatOutcome, WriteOutcome, WriteResult};

fn write(tool: &str, outcome: WriteOutcome) -> WriteResult {
    WriteResult { tool: tool.to_string(), outcome }
}

fn turn(writes: Vec<WriteResult>) -> ChatOutcome {
    ChatOutcome { writes, end: ChatEnd::Completed }
}

fn says_done(s: &str) -> bool {
    s.contains("Done") || s.contains('✓')
}

/// C1: a rename to the same name is said as no change, with its reason.
#[test]
fn c1_a_no_change_rename_reads_no_change_with_its_reason() {
    let o = turn(vec![write(
        "rename_thread",
        WriteOutcome::NoChange("the thread is already named 'Plans'".into()),
    )]);
    let s = o.summary_line();
    assert_eq!(s, "No change: rename_thread (the thread is already named 'Plans')");
    assert!(!says_done(&s), "{s:?}");
}

/// C1: a move into the thread the document is already in, likewise.
#[test]
fn c1_a_no_change_move_reads_no_change_with_its_reason() {
    let o = turn(vec![write(
        "move_document",
        WriteOutcome::NoChange("'Seeds' is already in 'Garden'".into()),
    )]);
    let s = o.summary_line();
    assert_eq!(s, "No change: move_document ('Seeds' is already in 'Garden')");
    assert!(!says_done(&s), "{s:?}");
}

/// C1: a no-change write is a write the turn ran: it is named, and the turn
/// is not said to have only answered.
#[test]
fn c1_a_turn_whose_only_write_changed_nothing_names_it() {
    let o = turn(vec![write("rename_thread", WriteOutcome::NoChange("the thread is already named 'Plans'".into()))]);
    let s = o.summary_line();
    assert!(s.contains("rename_thread"), "{s:?}");
    assert!(!s.contains("Answered"), "the write is named, not hidden behind 'Answered': {s:?}");
}

/// C1: a no-change write with no reason still never says Done, and shows no
/// empty parentheses.
#[test]
fn c1_a_no_change_without_a_reason_still_says_no_change() {
    let o = turn(vec![write("rename_thread", WriteOutcome::NoChange(String::new()))]);
    let s = o.summary_line();
    assert!(s.starts_with("No change: rename_thread"), "{s:?}");
    assert!(!s.contains("()"), "{s:?}");
    assert!(!says_done(&s), "{s:?}");
}

/// C1 with C3: in a turn of several writes, "Done" is said once, for the one
/// write that changed something; the no-change write keeps its own words.
#[test]
fn c1_mixed_writes_say_done_only_for_the_real_change() {
    let o = turn(vec![
        write("rename_thread", WriteOutcome::Succeeded),
        write("move_document", WriteOutcome::NoChange("'Seeds' is already in 'Garden'".into())),
        write("create_thread", WriteOutcome::Failed("the workspace is locked".into())),
        write("move_document", WriteOutcome::Declined),
    ]);
    let s = o.summary_line();
    assert_eq!(s.matches("Done").count(), 1, "{s:?}");
    assert_eq!(s.matches('✓').count(), 1, "{s:?}");
    assert!(s.contains("No change: move_document ('Seeds' is already in 'Garden')"), "{s:?}");
}

/// C3: the lines that are not about "no change" read exactly as before.
#[test]
fn c3_the_other_lines_are_unchanged() {
    assert_eq!(turn(vec![write("rename_thread", WriteOutcome::Succeeded)]).summary_line(), "✓ Done: rename_thread");
    assert_eq!(turn(vec![write("move_document", WriteOutcome::Succeeded)]).summary_line(), "✓ Done: move_document");
    assert_eq!(
        turn(vec![write("move_document", WriteOutcome::Failed("thread not found".into()))]).summary_line(),
        "✗ Failed: move_document (thread not found)"
    );
    assert_eq!(turn(vec![write("rename_thread", WriteOutcome::Declined)]).summary_line(), "Declined: rename_thread");
    assert_eq!(turn(vec![]).summary_line(), "Answered (no changes made)");
}

/// C1: a no-change write before a failed generation is still reported.
#[test]
fn c1_a_no_change_write_is_reported_whatever_ends_the_turn() {
    let o = ChatOutcome {
        writes: vec![write("rename_thread", WriteOutcome::NoChange("the thread is already named 'Plans'".into()))],
        end: ChatEnd::GenerationFailed("timeout".into()),
    };
    let s = o.summary_line();
    assert!(s.contains("No change: rename_thread") && s.contains("timeout"), "{s:?}");
    assert!(!says_done(&s), "{s:?}");
}
