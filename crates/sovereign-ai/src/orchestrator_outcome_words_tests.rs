//! v0.0.12 item 11 — **"Done" only for a real change** (B1–B4;
//! `doc/plans/V0012-lot-assistant.md`; cloud review v0.0.11, S5: "Done" for
//! search, open, list, summarize, for "rename X" with no " to ", and for
//! `split_thread`, which only creates an empty thread).
//!
//! - **B1** — read-only actions report their result, never "Done"; when
//!   nothing is found, they say so.
//! - **B2** — no change is said as such ("No change: …") and counts as not done.
//! - **B3** — `split_thread` says it created an empty thread named X, into
//!   which documents can be moved; no silent split of documents.
//! - **B4** — the outcome word follows the change: "Done" (or the A3 sentence)
//!   only when the store changed, as the action's outcome reports it. Every
//!   action of the classifier list (`HANDLED_ACTIONS`) runs here through a
//!   store, and the word is checked against a snapshot of the store taken
//!   before and after.
//!
//! **API named here (does not exist yet):** the action's outcome, carried by
//! the executed event, as `ChatOutcome` is for a chat turn —
//!
//! ```ignore
//! // sovereign_core::interfaces
//! pub enum ActionOutcome {
//!     Changed { summary: String },   // the store changed; `summary` is the A3 sentence
//!     Reported { summary: String },  // read-only: what was found or produced
//!     NoChange { reason: String },   // nothing to change (B2); not done
//!     Failed { reason: String },     // not done: refused, not found, ambiguous, failed
//! }
//! impl ActionOutcome {
//!     pub fn summary_line(&self) -> String; // the one line every UI shows
//!     pub fn changed(&self) -> bool;        // Changed only
//! }
//! OrchestratorEvent::ActionExecuted { action: String, success: bool, outcome: ActionOutcome }
//! ```
//!
//! `success` keeps its meaning for existing consumers: true for `Changed` and
//! `Reported`, false for `NoChange` and `Failed`. The variant for "not done"
//! is deliberately not called `NotDone`: its `Debug` would say "Done". The
//! UIs show `summary_line()` (a scan in
//! `sovereign-core/tests/b4_uis_show_the_action_outcome.rs`); the line's own
//! properties are pinned in `sovereign-core/src/action_outcome_tests.rs`.
//!
//! Written before the code by another author; the implementer does not edit
//! this file. Does not compile until `ActionOutcome` exists.

use super::assistant_harness::*;
use super::*;
use sovereign_core::interfaces::ActionOutcome;

/// What an action is expected to report.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Expect {
    Change,
    Report,
    NoChange,
    NotDone,
}

/// The one executed event of `events`: (success, outcome).
fn the_outcome(events: &[OrchestratorEvent], what: &str) -> (bool, ActionOutcome) {
    let all: Vec<(bool, ActionOutcome)> = events
        .iter()
        .filter_map(|e| match e {
            OrchestratorEvent::ActionExecuted { success, outcome, .. } => Some((*success, outcome.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(all.len(), 1, "{what}: exactly one executed event, got {events:?}");
    all.into_iter().next().unwrap()
}

/// The chat messages among `events`.
fn chat(events: &[OrchestratorEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            OrchestratorEvent::ChatResponse { text } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// The standard world for B: threads Garden (Seeds, Tomatoes, the external
/// Clipping), Kitchen (Pans), Plans (empty), Spare (empty); a document Notes
/// with two versions (restorable); a fake model answering summaries.
async fn b_world(tag: &str) -> World {
    let w = world(tag, 4096, answer("A short summary.")).await;
    let garden = w.thread("Garden").await;
    let kitchen = w.thread("Kitchen").await;
    w.thread("Plans").await;
    w.thread("Spare").await;
    w.doc("Seeds", &garden, "seed list").await;
    w.doc("Tomatoes", &garden, "tomato notes").await;
    w.doc("Pans", &kitchen, "pans").await;
    crate::docs_host::create(
        &w.docs,
        crate::docs_host::new_web_document("Clipping", &garden, "https://example.org/clip", "a clipped page"),
    )
    .await
    .unwrap();
    let mut d = sovereign_db::schema::Document::new("Notes".into(), garden.clone(), true);
    d.content = "first".into();
    let id = w.store.create_document(d).await.unwrap().id_string().unwrap();
    w.store.commit_document(&id, "one").await.unwrap();
    w.store.update_document(&id, None, Some("second")).await.unwrap();
    w.store.commit_document(&id, "two").await.unwrap();
    w
}

/// Every action of the classifier list, with a target, and what it must
/// report in the B world. `chat` and `unknown` are the chat loop's (they end
/// with a `ChatTurnEnded`, tested below).
const CASES: &[(&str, Option<&str>, Expect)] = &[
    ("search", Some("Seeds"), Expect::Report),
    ("search", Some("Zeppelin"), Expect::Report),
    ("open", Some("Seeds"), Expect::Report),
    ("navigate", Some("Seeds"), Expect::Report),
    ("create_thread", Some("Fresh"), Expect::Change),
    ("rename_thread", Some("Plans to Roadmap"), Expect::Change),
    ("rename_thread", Some("Plans"), Expect::NoChange),
    ("rename_thread", Some("Plans to Plans"), Expect::NoChange),
    ("delete_thread", Some("Spare"), Expect::Change),
    ("delete_thread", Some("Ghost"), Expect::NotDone),
    ("move_document", Some("Pans to Garden"), Expect::Change),
    ("move_document", Some("Seeds to Garden"), Expect::NoChange),
    ("history", Some("Seeds"), Expect::Report),
    ("summarize", Some("Seeds"), Expect::Report),
    ("list_models", None, Expect::Report),
    ("swap_model", Some("no-such-model"), Expect::NotDone),
    ("merge_threads", Some("Garden to Kitchen"), Expect::Change),
    ("split_thread", Some("Garden to Orchard"), Expect::Change),
    ("adopt", Some("Clipping"), Expect::Change),
    // Read (B2's principle, B4): adopting what is already owned changes
    // nothing, so it is said as no change.
    ("adopt", Some("Seeds"), Expect::NoChange),
    ("create_milestone", Some("Launch on Garden"), Expect::Change),
    ("list_milestones", Some("Garden"), Expect::Report),
    ("list_contacts", None, Expect::Report),
    ("view_messages", None, Expect::Report),
    ("create_document", Some("New note"), Expect::Change),
    ("restore", Some("Notes"), Expect::Change),
    ("restore", Some("Seeds"), Expect::NotDone),
    ("open_pii_dashboard", None, Expect::Report),
    ("open_models", None, Expect::Report),
    ("open_inbox", None, Expect::Report),
    ("browse", None, Expect::Report),
    ("open_settings", None, Expect::Report),
];

/// B4 (with B1, B2): every action of the classifier list, run through the
/// store; "Done" (or the A3 sentence, `Changed`) exactly when the store
/// changed; every other outcome never says "Done", in its line or in a chat
/// message; `success` follows the outcome.
#[tokio::test]
async fn b4_every_action_of_the_classifier_list_says_done_only_for_a_real_change() {
    for a in HANDLED_ACTIONS {
        if ["chat", "unknown"].contains(a) {
            continue;
        }
        assert!(CASES.iter().any(|(c, _, _)| c == a), "{a} is in the classifier list but has no case here");
    }
    for (i, (action, target, expect)) in CASES.iter().enumerate() {
        let what = format!("{action} {target:?}");
        let w = b_world(&format!("b4-{i}")).await;
        let before = snapshot(&w.store).await;
        let evs = w.act(action, *target).await;
        let changed = snapshot(&w.store).await != before;
        let (success, outcome) = the_outcome(&evs, &what);
        let line = outcome.summary_line();

        assert!(!line.trim().is_empty(), "{what}: an outcome line is never empty");
        assert_eq!(outcome.changed(), changed, "{what}: the outcome says changed={} but the store changed={changed}: {line}", outcome.changed());
        let got = match &outcome {
            ActionOutcome::Changed { .. } => Expect::Change,
            ActionOutcome::Reported { .. } => Expect::Report,
            ActionOutcome::NoChange { .. } => Expect::NoChange,
            ActionOutcome::Failed { .. } => Expect::NotDone,
        };
        assert_eq!(got, *expect, "{what}: {line}");
        assert_eq!(success, matches!(got, Expect::Change | Expect::Report), "{what}: success follows the outcome");
        if got != Expect::Change {
            assert!(!says_done(&line), "{what}: not a change, yet the line says Done: {line}");
            for m in chat(&evs) {
                assert!(!says_done(&m), "{what}: not a change, yet a chat message says Done: {m}");
            }
        }
    }
}

/// B4: `chat` and `unknown` end with the chat turn's own outcome, never with
/// an executed action.
#[tokio::test]
async fn b4_chat_turns_end_with_their_own_outcome() {
    for action in ["chat", "unknown"] {
        let w = b_world(&format!("b4-{action}")).await;
        let evs = w.act(action, None).await;
        assert!(!evs.iter().any(|e| matches!(e, OrchestratorEvent::ActionExecuted { .. })), "{action}");
        assert!(evs.iter().any(|e| matches!(e, OrchestratorEvent::ChatTurnEnded { .. })), "{action}");
    }
}

/// B1: a read-only action says what it found; when nothing is found, it says
/// so.
#[tokio::test]
async fn b1_read_only_actions_report_what_they_found() {
    let w = b_world("b1-found").await;
    let evs = w.act("search", Some("Seeds")).await;
    let (_, o) = the_outcome(&evs, "search Seeds");
    assert!(matches!(o, ActionOutcome::Reported { .. }));
    assert!(o.summary_line().contains("Seeds") || o.summary_line().contains('1'), "says what it found: {}", o.summary_line());

    for (action, target) in [("search", Some("Zeppelin")), ("list_contacts", None), ("view_messages", None), ("list_models", None)] {
        let evs = w.act(action, target).await;
        let (_, o) = the_outcome(&evs, action);
        let line = o.summary_line().to_lowercase();
        assert!(
            line.contains("no ") || line.contains("nothing") || line.contains("none") || line.contains(" 0 ") || line.starts_with("0 "),
            "{action}: nothing found, and it says so: {line}"
        );
    }
}

/// B2: a rename to the same name — with " to " or without — and a move into
/// the thread the document is already in say "No change: …", name what was
/// asked, and count as not done.
#[tokio::test]
async fn b2_no_change_is_said_as_such() {
    let w = b_world("b2").await;
    for (action, target, names) in [
        ("rename_thread", "Plans", &["Plans"][..]),
        ("rename_thread", "Plans to Plans", &["Plans"][..]),
        ("move_document", "Seeds to Garden", &["Seeds", "Garden"][..]),
    ] {
        let before = snapshot(&w.store).await;
        let evs = w.act(action, Some(target)).await;
        assert_eq!(snapshot(&w.store).await, before, "{action} {target}: nothing changed");
        let (success, o) = the_outcome(&evs, target);
        assert!(matches!(o, ActionOutcome::NoChange { .. }), "{action} {target}: {o:?}");
        assert!(!success, "no change counts as not done");
        let line = o.summary_line();
        assert!(line.starts_with("No change"), "{line}");
        for n in names {
            assert!(line.contains(n), "{action}: the line names {n:?}: {line}");
        }
    }
}

/// B3: split creates an empty thread and says so — its name, that it is
/// empty, that documents can be moved into it — and moves no document.
#[tokio::test]
async fn b3_split_says_it_created_an_empty_thread() {
    let w = b_world("b3").await;
    let placement = |docs: Vec<sovereign_db::schema::Document>| -> Vec<String> {
        let mut v: Vec<String> = docs.into_iter().filter_map(|d| d.id_string().map(|id| format!("{id}@{}", d.thread_id))).collect();
        v.sort();
        v
    };
    let before = placement(w.store.list_documents(None).await.unwrap());
    let evs = w.act("split_thread", Some("Garden to Orchard")).await;
    let (success, o) = the_outcome(&evs, "split");
    assert!(success && o.changed(), "a thread was created: {o:?}");
    let line = o.summary_line();
    assert!(line.contains("Orchard"), "names the new thread: {line}");
    assert!(line.to_lowercase().contains("empty"), "says it is empty: {line}");
    assert!(line.to_lowercase().contains("move"), "says documents can be moved into it: {line}");
    let orchard = w.live_threads_named("Orchard").await;
    assert_eq!(orchard.len(), 1, "one new thread");
    assert!(w.store.list_documents(Some(&orchard[0])).await.unwrap().is_empty(), "it is empty");
    assert_eq!(placement(w.store.list_documents(None).await.unwrap()), before, "no document moved (no silent split)");
}

/// B4 without a document service (a locked workspace): every write is not
/// done, and never says "Done".
#[tokio::test]
async fn b4_locked_writes_are_not_done() {
    for (i, (action, target)) in [
        ("create_thread", "Fresh"),
        ("rename_thread", "Plans to Roadmap"),
        ("delete_thread", "Spare"),
        ("move_document", "Pans to Garden"),
        ("merge_threads", "Garden to Kitchen"),
        ("split_thread", "Garden to Orchard"),
        ("create_document", "New note"),
    ]
    .iter()
    .enumerate()
    {
        let open = b_world(&format!("b4-locked-{i}")).await;
        open.orch.clear_document_service();
        let before = snapshot(&open.store).await;
        let evs = open.act(action, Some(target)).await;
        assert_eq!(snapshot(&open.store).await, before, "{action}: locked, nothing written");
        let (success, o) = the_outcome(&evs, action);
        assert!(!success && matches!(o, ActionOutcome::Failed { .. }), "{action}: {o:?}");
        assert!(!says_done(&o.summary_line()), "{action}: {}", o.summary_line());
    }
}
