//! The spec's one shared assertion (ch. 03, *What cannot be read is never
//! shown as empty*): "a session's own unreadable item is counted, and the
//! count is shown". One helper for both hosts' tests, so the shell, the app,
//! C-AREA's AR3 and the phone's History hold the same rule.
//!
//! Not a module of this crate's suite (`common/mod.rs` does not declare it):
//! the hosts' tests include it by path, as they include `faulty.rs`:
//!
//! ```ignore
//! #[path = "../../sovereign-docs/tests/common/counted_and_shown.rs"]
//! mod counted_and_shown;
//! ```
//!
//! Written 2026-11-17 for the Alerts' lists that cannot be read (C8), by the
//! test author, before the code.

/// Asserts that what could not be read is kept, counted and said, never
/// shown as nothing:
/// - `counted`: how many unreadable items the host counted (at least one);
/// - `shown`: every text the host shows or reads out for them (a window's
///   lines, a status line, what a command hands the frontend);
/// - `said`: the sentence that says so, which one of `shown` must hold;
/// - `empty_words`: what the host says when there is nothing, none of which
///   may be shown instead.
///
/// `what` names the case in the failure messages.
#[track_caller]
pub fn assert_counted_and_shown(what: &str, counted: usize, shown: &[String], said: &str, empty_words: &[&str]) {
    assert!(
        counted >= 1,
        "{what}: what could not be read is counted, never dropped (counted {counted}); shown: {shown:#?}"
    );
    assert!(
        shown.iter().any(|s| s.contains(said)),
        "{what}: the count is shown, in words: {said:?} is in none of {shown:#?}"
    );
    for empty in empty_words {
        assert!(
            !shown.iter().any(|s| s.contains(empty)),
            "{what}: what could not be read is never shown as nothing: {empty:?} is shown in {shown:#?}"
        );
    }
}
