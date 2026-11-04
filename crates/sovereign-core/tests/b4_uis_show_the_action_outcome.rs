//! v0.0.12 item 11, **B4** — "the outcome word follows the change … as the
//! action's outcome reports it" (`doc/plans/V0012-lot-assistant.md`). Today
//! both UIs turn `ActionExecuted { success: true }` into "Done: <action>"
//! (the shell's `"\u{2713} Done:"`, the frontend's `Done: ${action}`), so a
//! search or a rename to the same name is shown as done whatever the
//! orchestrator reports.
//!
//! A source scan (the UIs are read, not run — as the other scans of the
//! workspace do): each UI shows the outcome's own line,
//! `ActionOutcome::summary_line()` (named in
//! `sovereign-core/src/action_outcome_tests.rs`), and composes no "Done"
//! from `success` itself. The Tauri payload carries that line as `summary`,
//! as `ChatTurnEndedPayload` does for a chat turn.
//!
//! Written before the code by another author; the implementer does not edit
//! this file.

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap().to_path_buf()
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// The text from the first line containing `start` to the next line (after
/// it) that starts, once trimmed, with one of `ends`.
fn block<'a>(src: &'a str, start: &str, ends: &[&str]) -> &'a str {
    let at = src.find(start).unwrap_or_else(|| panic!("{start:?} not found"));
    let rest = &src[at..];
    let mut offset = rest.find('\n').map(|n| n + 1).unwrap_or(rest.len());
    for line in rest[offset..].split_inclusive('\n') {
        let t = line.trim_start();
        if ends.iter().any(|e| t.starts_with(e)) {
            break;
        }
        offset += line.len();
    }
    &rest[..offset]
}

/// The shell's arm for `ActionExecuted` shows the outcome's line.
#[test]
fn b4_the_shell_shows_the_actions_outcome_line() {
    let src = read("crates/sovereign-shell/src/app.rs");
    let arm = block(&src, "OrchestratorEvent::ActionExecuted", &["OrchestratorEvent::", "// "]);
    assert!(arm.contains("summary_line()"), "the shell shows outcome.summary_line():\n{arm}");
    assert!(!arm.contains("Done"), "the shell composes no \"Done\" of its own:\n{arm}");
}

/// The Tauri payload carries the outcome's line, and the frontend shows it.
#[test]
fn b4_the_phone_and_tauri_ui_show_the_actions_outcome_line() {
    let ev = read("crates/sovereign-app/src/tauri_events.rs");
    let payload = block(&ev, "pub struct ActionExecutedPayload", &["}"]);
    assert!(payload.contains("pub summary: String"), "the payload carries the outcome's line as `summary`:\n{payload}");
    let arm = block(&ev, "OrchestratorEvent::ActionExecuted", &["OrchestratorEvent::"]);
    assert!(arm.contains("summary_line()"), "the payload's summary is outcome.summary_line():\n{arm}");

    let ts = read("frontend/src/lib/api/events.ts");
    let listener = block(&ts, "'action-executed'", &["unlisteners.push("]);
    assert!(listener.contains("payload.summary"), "the frontend shows the payload's summary:\n{listener}");
    assert!(!listener.contains("Done"), "the frontend composes no \"Done\" of its own:\n{listener}");
}
