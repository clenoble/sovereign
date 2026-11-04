//! v0.0.12 rc2 — a summary is shown on the desktop (contract R1, R2, R4,
//! `doc/plans/V0012-rc2-summary-shown.md`; found by Céline on rc1, checklist
//! §22, 2026-11-01).
//!
//! Written before the code, by another author: whoever makes the shell show a
//! summary makes these pass **without editing this file**; a test that looks
//! wrong is reported, not worked around.
//!
//! A child module of `app` (declared in `app.rs`), so it drives a real `App`
//! through the user's path: the orchestrator's event goes into `orch_tx`, as
//! `produce_summary` sends it, and one frame (`App::build_scene`) drains the
//! channel, as the running shell does. Nothing here calls a helper the fix
//! might add: the tests see only the chat windows and their messages.
//!
//! Stores: a real encrypted store over an in-memory SurrealDB
//! (`docs_host::testing`), unlocked as `try_login` does; the data directory
//! is `SOVEREIGN_DATA_DIR` when the run sets it (it should), otherwise a fresh
//! temporary one. Nothing touches a real workspace.
//!
//! Readings chosen where the contract leaves room (the implementer may report
//! them as wrong, not edit them):
//! - R1's "exactly one message": exactly one message holds the summary text;
//!   the chat gains exactly one message per summary event; and the chat window
//!   stays open, alone (a summary never toggles an open chat closed).
//! - R1's text: the title and the summary appear verbatim; the part count is
//!   read as "<N> parts" for N > 1, and no "1 part" for a single part. The exact
//!   punctuation around them is free.
//! - Settled in the contract's "Clarifications" (2026-11-01, `e91ba45`): a
//!   malformed summary with no chat open opens the chat too and shows the plain
//!   line; a `parts` field missing or not a number counts as 1 (not malformed).

use std::sync::{Arc, OnceLock};

use sovereign_ai::docs_host::testing;
use sovereign_core::interfaces::{ActionOutcome, OrchestratorEvent};
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::traits::GraphDB;

use super::{App, WindowContent, WindowKind};
use crate::panels::Role;

// ======================================================================= fixtures

/// `SOVEREIGN_DATA_DIR` when the run sets it, otherwise a fresh temporary
/// directory set once for the process — never the user's `~/.sovereign`.
fn ensure_test_data_dir() {
    static DIR: OnceLock<std::path::PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        if let Some(d) = std::env::var_os("SOVEREIGN_DATA_DIR") {
            return d.into();
        }
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let d = std::env::temp_dir().join(format!("sov-summary-shown-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::env::set_var("SOVEREIGN_DATA_DIR", &d);
        d
    });
}

/// A shell unlocked on an encrypted in-memory store (the unlock sequence of
/// `App::try_login`, no auth store, no P2P), with no window open.
fn unlocked(tag: &str) -> App {
    ensure_test_data_dir();
    let mut app = App::new();
    let store: Arc<EncryptedGraphDB> = app.rt.block_on(testing::encrypted_memory_store(tag));
    app.account_key = Some(testing::account_key());
    app.db = Some(store.clone() as Arc<dyn GraphDB>);
    app.persona = Some(sovereign_core::auth::PersonaKind::Primary);
    app.install_document_service(store);
    if let Some(task) = app.migration_task.take() {
        let _ = app.rt.block_on(task);
    }
    app.locked = false;
    app.load_workspace_now();
    frame(&mut app);
    assert_eq!(chat_windows(&app), 0, "the fixture starts with no chat open");
    app
}

/// One whole frame laid out: the orchestrator channel is drained.
fn frame(app: &mut App) {
    app.build_scene(1400.0, 900.0);
}

/// Opens the chat as the taskbar / bubble does.
fn open_chat(app: &mut App) {
    app.focus_or_toggle(WindowKind::Chat);
    assert_eq!(chat_windows(app), 1, "a chat is open");
}

fn chat_windows(app: &App) -> usize {
    app.windows.iter().filter(|w| matches!(&w.content, WindowContent::Chat(_))).count()
}

/// The messages of the first chat window: (is the assistant, text).
fn chat_msgs(app: &App) -> Vec<(bool, String)> {
    app.windows
        .iter()
        .find_map(|w| match &w.content {
            WindowContent::Chat(c) => Some(c.msgs.iter().map(|m| (m.role == Role::Assistant, m.text.clone())).collect()),
            _ => None,
        })
        .unwrap_or_default()
}

/// The event exactly as `produce_summary` (`sovereign-ai/src/orchestrator.rs`)
/// sends it.
fn summary_event(title: &str, summary: &str, parts: usize) -> OrchestratorEvent {
    let json = serde_json::json!({ "doc_title": title, "summary": summary, "parts": parts });
    OrchestratorEvent::SkillResult {
        skill: "summarizer".into(),
        action: "summarize".into(),
        kind: "summary".into(),
        data: json.to_string(),
    }
}

/// A summary event whose `data` is `data` as given.
fn summary_event_raw(data: &str) -> OrchestratorEvent {
    OrchestratorEvent::SkillResult {
        skill: "summarizer".into(),
        action: "summarize".into(),
        kind: "summary".into(),
        data: data.into(),
    }
}

fn send(app: &App, ev: OrchestratorEvent) {
    app.orch_tx.send(ev).unwrap();
}

/// A summary as the model gives it, PII already resolved to labels: two
/// lines, punctuation, non-ASCII — to be shown unchanged.
const SUMMARY: &str = "Le texte décrit la réunion du 3 mars avec [Person 1] : budget révisé de 12 %.\n\
                       Deux décisions — reporter l'essai, écrire au fournisseur.";

/// The messages that hold `needle`.
fn holding(msgs: &[(bool, String)], needle: &str) -> Vec<(bool, String)> {
    msgs.iter().filter(|(_, t)| t.contains(needle)).cloned().collect()
}

// ======================================================================= R1

// R1: "On SkillResult of kind summary, the shell adds one assistant message to
// the chat. It holds the document's title and the summary text, as produced
// […]." A single part: no part count.
#[test]
fn r1_a_one_part_summary_is_one_assistant_message_with_title_and_text() {
    let mut app = unlocked("r1-one-part");
    open_chat(&mut app);
    let before = chat_msgs(&app);
    send(&app, summary_event("Texte A", SUMMARY, 1));
    frame(&mut app);

    let after = chat_msgs(&app);
    assert_eq!(chat_windows(&app), 1, "the chat stays open, and alone");
    assert_eq!(after.len(), before.len() + 1, "exactly one message added: {after:?}");
    let shown = holding(&after, SUMMARY);
    assert_eq!(shown.len(), 1, "exactly one message holds the summary, unchanged: {after:?}");
    let (assistant, text) = &shown[0];
    assert!(*assistant, "the summary is an assistant message");
    assert!(text.contains("Texte A"), "the message names the document: {text}");
    assert!(!text.contains("1 part"), "no part count for a single part: {text}");
    assert!(!text.contains("doc_title") && !text.contains("{\""), "no raw JSON: {text}");
}

// R1: "… and the number of parts when there were more than one. Example form:
// `Summary of 'Texte B' (4 parts):` then the text."
#[test]
fn r1_a_summary_in_parts_says_how_many() {
    let mut app = unlocked("r1-parts");
    open_chat(&mut app);
    let before = chat_msgs(&app);
    send(&app, summary_event("Texte B", SUMMARY, 4));
    frame(&mut app);

    let after = chat_msgs(&app);
    assert_eq!(after.len(), before.len() + 1, "exactly one message added: {after:?}");
    let shown = holding(&after, SUMMARY);
    assert_eq!(shown.len(), 1, "exactly one message holds the summary, unchanged: {after:?}");
    let (assistant, text) = &shown[0];
    assert!(*assistant, "the summary is an assistant message");
    assert!(text.contains("Texte B"), "the message names the document: {text}");
    assert!(text.contains("4 parts"), "the message says the part count: {text}");
    assert!(!text.contains("doc_title") && !text.contains("{\""), "no raw JSON: {text}");
}

// R1 with R4 ("The outcome line still comes, once"): the orchestrator sends the
// summary, then the outcome line in `ActionExecuted`; both are shown, each once.
#[test]
fn r1_the_summary_and_then_the_outcome_line_each_show_once() {
    let mut app = unlocked("r1-outcome");
    open_chat(&mut app);
    let before = chat_msgs(&app);
    let outcome = ActionOutcome::Reported { summary: "Summarized 'Texte B' in 4 parts".into() };
    let line = outcome.summary_line();
    send(&app, summary_event("Texte B", SUMMARY, 4));
    send(&app, OrchestratorEvent::ActionExecuted { action: "summarize".into(), success: true, outcome });
    frame(&mut app);

    let after = chat_msgs(&app);
    assert_eq!(after.len(), before.len() + 2, "the summary and the outcome line: {after:?}");
    assert_eq!(holding(&after, SUMMARY).len(), 1, "the summary once: {after:?}");
    assert_eq!(after.iter().filter(|(_, t)| *t == line).count(), 1, "the outcome line once: {after:?}");
}

// ======================================================================= R2

// R2: "A summary that arrives with no chat open is not lost: the shell opens
// the chat and shows it."
#[test]
fn r2_a_summary_with_no_chat_open_opens_the_chat_and_shows_it() {
    let mut app = unlocked("r2-opens");
    send(&app, summary_event("Texte B", SUMMARY, 4));
    frame(&mut app);

    assert_eq!(chat_windows(&app), 1, "the chat opened, once");
    let msgs = chat_msgs(&app);
    let shown = holding(&msgs, SUMMARY);
    assert_eq!(shown.len(), 1, "the summary is in the chat that opened, once: {msgs:?}");
    let (assistant, text) = &shown[0];
    assert!(*assistant, "the summary is an assistant message");
    assert!(text.contains("Texte B") && text.contains("4 parts"), "title and part count: {text}");
}

// R2, then the outcome line that follows it: it lands in the chat the summary
// opened (it is no longer dropped, the chat being open), once.
#[test]
fn r2_the_outcome_line_after_it_lands_in_the_opened_chat() {
    let mut app = unlocked("r2-outcome");
    let outcome = ActionOutcome::Reported { summary: "Summarized 'Texte A'".into() };
    let line = outcome.summary_line();
    send(&app, summary_event("Texte A", SUMMARY, 1));
    send(&app, OrchestratorEvent::ActionExecuted { action: "summarize".into(), success: true, outcome });
    frame(&mut app);

    assert_eq!(chat_windows(&app), 1, "one chat");
    let msgs = chat_msgs(&app);
    assert_eq!(holding(&msgs, SUMMARY).len(), 1, "the summary once: {msgs:?}");
    assert_eq!(msgs.iter().filter(|(_, t)| *t == line).count(), 1, "the outcome line once: {msgs:?}");
}

// ======================================================================= R4

// R4: "`model_list` and `model_swapped` stay unshown on the desktop" — with a
// chat open, nothing is added.
#[test]
fn r4_model_list_and_model_swapped_add_nothing_to_an_open_chat() {
    let mut app = unlocked("r4-models-open");
    open_chat(&mut app);
    let before = chat_msgs(&app);
    send(
        &app,
        OrchestratorEvent::SkillResult {
            skill: "model_manager".into(),
            action: "list_models".into(),
            kind: "model_list".into(),
            data: serde_json::json!({ "models": [{ "name": "qwen2.5-3b.gguf", "size_mb": 1900 }] }).to_string(),
        },
    );
    send(
        &app,
        OrchestratorEvent::SkillResult {
            skill: "model_manager".into(),
            action: "swap_model".into(),
            kind: "model_swapped".into(),
            data: serde_json::json!({ "model": "qwen2.5-3b.gguf", "prompt_format": "chatml" }).to_string(),
        },
    );
    frame(&mut app);

    assert_eq!(chat_windows(&app), 1, "the chat stays open, and alone");
    assert_eq!(chat_msgs(&app).len(), before.len(), "nothing shown: {:?}", chat_msgs(&app));
}

// R4 with R2's scope ("R2 covers the summary only"): a model list with no chat
// open opens nothing.
#[test]
fn r4_model_list_with_no_chat_open_opens_nothing() {
    let mut app = unlocked("r4-models-closed");
    send(
        &app,
        OrchestratorEvent::SkillResult {
            skill: "model_manager".into(),
            action: "list_models".into(),
            kind: "model_list".into(),
            data: serde_json::json!({ "models": [] }).to_string(),
        },
    );
    frame(&mut app);
    assert_eq!(chat_windows(&app), 0, "no chat opened for a model list");
}

// R4: "Unreadable or malformed data shows one plain line ("The summary could
// not be shown") rather than nothing or the raw text." Not JSON.
#[test]
fn r4_unreadable_summary_data_shows_one_plain_line() {
    let mut app = unlocked("r4-unreadable");
    open_chat(&mut app);
    let before = chat_msgs(&app);
    let raw = "zebra-not-json <<summary>>";
    send(&app, summary_event_raw(raw));
    frame(&mut app);

    let after = chat_msgs(&app);
    assert_eq!(after.len(), before.len() + 1, "exactly one line: {after:?}");
    let (_, text) = after.last().unwrap();
    assert!(text.contains("The summary could not be shown"), "the plain line: {text}");
    assert!(!text.contains("zebra-not-json"), "never the raw text: {text}");
}

// R4: malformed — JSON, but no summary text in it.
#[test]
fn r4_summary_data_without_a_summary_shows_one_plain_line() {
    let mut app = unlocked("r4-malformed");
    open_chat(&mut app);
    let before = chat_msgs(&app);
    let raw = serde_json::json!({ "doc_title": "zebra-title", "parts": 2 }).to_string();
    send(&app, summary_event_raw(&raw));
    frame(&mut app);

    let after = chat_msgs(&app);
    assert_eq!(after.len(), before.len() + 1, "exactly one line: {after:?}");
    let (_, text) = after.last().unwrap();
    assert!(text.contains("The summary could not be shown"), "the plain line: {text}");
    assert!(!text.contains("{\"") && !text.contains("doc_title"), "never the raw text: {text}");
}

// R4 with R2 (contract, "Clarifications", 2026-11-01): "A malformed summary
// with no chat open opens the chat too, as R2 does, and shows that line."
#[test]
fn r4_unreadable_summary_with_no_chat_open_opens_the_chat_and_shows_the_plain_line() {
    let mut app = unlocked("r4-unreadable-closed");
    send(&app, summary_event_raw("zebra-not-json <<summary>>"));
    frame(&mut app);

    assert_eq!(chat_windows(&app), 1, "the chat opened, once");
    let msgs = chat_msgs(&app);
    let shown = holding(&msgs, "The summary could not be shown");
    assert_eq!(shown.len(), 1, "the plain line is in the chat that opened, once: {msgs:?}");
    assert!(msgs.iter().all(|(_, t)| !t.contains("zebra-not-json")), "never the raw text: {msgs:?}");
}

// R4 (contract, "Clarifications", 2026-11-01): "A `parts` field that is
// missing or not a number counts as 1. It is not malformed data." The summary
// is shown, as for one part: no part count, not the plain line.
#[test]
fn r4_missing_or_non_numeric_parts_counts_as_one_part() {
    let datas = [
        serde_json::json!({ "doc_title": "Texte C", "summary": SUMMARY }).to_string(),
        serde_json::json!({ "doc_title": "Texte C", "summary": SUMMARY, "parts": "four" }).to_string(),
    ];
    for (i, data) in datas.iter().enumerate() {
        let mut app = unlocked(&format!("r4-parts-{i}"));
        open_chat(&mut app);
        let before = chat_msgs(&app);
        send(&app, summary_event_raw(data));
        frame(&mut app);

        let after = chat_msgs(&app);
        assert_eq!(after.len(), before.len() + 1, "exactly one message added ({data}): {after:?}");
        let shown = holding(&after, SUMMARY);
        assert_eq!(shown.len(), 1, "the summary is shown, unchanged ({data}): {after:?}");
        let (assistant, text) = &shown[0];
        assert!(*assistant, "the summary is an assistant message");
        assert!(text.contains("Texte C"), "the message names the document: {text}");
        let low = text.to_lowercase();
        assert!(!low.contains("parts") && !low.contains("1 part"), "no part count ({data}): {text}");
        assert!(!text.contains("could not be shown"), "not malformed ({data}): {text}");
        // The raw value quoted, not the bare word: the summary itself holds
        // "fournisseur" (caught by the implementer, 2026-11-01).
        assert!(!text.contains("\"four\"") && !text.contains("{\""), "no raw data ({data}): {text}");
    }
}
