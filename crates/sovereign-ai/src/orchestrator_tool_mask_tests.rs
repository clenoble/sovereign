//! v0.0.13, spec ch. 09 SW3 (« masked where it is read, for every path to the
//! model »; content read for a prompt, titles included, is masked): **every
//! string that becomes a tool's turn for the model has passed the read mask**
//! (the Fable maille, by delegation of Céline, 2026-11-17).
//!
//! The case that shows it: a document title holding a detected personal-data
//! value with no record yet (before the sweep: the title written straight into
//! the store, so ingest never tokenised it). Its value never reaches the model
//! as written, in each of three places:
//!
//! 1. a read tool's result: `get_document` on a close title (the « no document
//!    is titled exactly … Close titles: … » sentence);
//! 2. a write tool's result: a move's reply (« Moved '…' to thread '…' », and
//!    « … is already in … »);
//! 3. a write tool that does not resolve: its sentence naming a close title.
//!
//! The control: a value she dismissed (SW5: its record in the state « PII
//! dismissed ») stays as written.
//!
//! What the model receives is read from the harness: the prompt of the model
//! call that follows the tool, and in it the tool's own turn. The value is
//! looked for in that turn only: the model's own tool call, which names the
//! title it chose, is the model's words, not the tool's.
//!
//! Written before the code by another author; the implementer does not edit
//! this file.

use super::assistant_harness::*;
use super::*;
use sovereign_db::schema::{Document, PiiKind, ReviewState, SourceKind, SourceRef};

/// A value the read mask's detection finds, with no record anywhere.
const MAIL: &str = "alice@example.com";
/// A value she dismissed.
const DISMISSED: &str = "bob@example.org";

fn key() -> Arc<sovereign_crypto::account_key::AccountKey> {
    crate::docs_host::testing::account_key()
}

/// The model's answer calling `name` with `args`.
fn tool_call(name: &str, args: serde_json::Value) -> String {
    format!("<tool_call>\n{}\n</tool_call>", serde_json::json!({ "name": name, "arguments": args }))
}

/// A model that calls the tool first, then answers plainly.
fn first_then_answer(first: String) -> Script {
    Arc::new(move |_: &str, _: u32, i: usize| if i == 0 { first.clone() } else { "All set.".to_string() })
}

/// A world whose orchestrator holds the account key, as after a login (so the
/// mask can read what she dismissed).
async fn keyed_world(tag: &str, script: Script) -> World {
    let w = world(tag, 4096, script).await;
    w.orch.set_pii_account_key(key());
    w
}

/// A document whose title holds `title` as written, straight in the store: no
/// ingest ran, so no value in it is tokenised and none has a record.
async fn unswept_doc(w: &World, title: &str, thread: &str) -> String {
    let mut d = Document::new(title.into(), thread.into(), true);
    d.content = "a note".into();
    let id = w.store.create_document(d).await.unwrap().id_string().unwrap();
    assert_eq!(w.store.get_document(&id).await.unwrap().title, title, "precondition: the title as written");
    let linked = w
        .store
        .list_pii_records(None, None, None)
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.sources.iter().any(|s| s.source_kind == SourceKind::Document && s.source_id == id))
        .count();
    assert_eq!(linked, 0, "precondition: no record for this document's values");
    id
}

/// An unswept document whose title holds `value`, with that value's record in
/// the state « PII dismissed », linked to it. The record is made from one the
/// save wrote for the same value (in a document titled « Scratch »), so it is
/// written as the store's records are.
async fn dismissed_doc(w: &World, title: &str, value: &str, thread: &str) -> String {
    let scratch = w.doc("Scratch", thread, &format!("written to {value}")).await;
    let base = w
        .store
        .list_pii_records(None, None, Some(false))
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.kind == PiiKind::Email && r.sources.iter().any(|s| s.source_id == scratch))
        .expect("precondition: the save recorded the value");
    let id = unswept_doc(w, title, thread).await;
    let start = title.find(value).unwrap();
    let source = SourceRef { source_id: id.clone(), span_start: start, span_end: start + value.len(), ..base.sources[0].clone() };
    w.store
        .create_pii_record(sovereign_db::schema::PiiRecord {
            id: None,
            sources: vec![source],
            review_state: ReviewState::Dismissed,
            ..base
        })
        .await
        .unwrap();
    id
}

/// Runs one chat turn on `message`, answering every proposal with `decision`.
/// Returns the turn's outcome.
async fn chat_turn(w: &mut World, message: &str, decision: ActionDecision) -> ChatOutcome {
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
    seen.into_iter()
        .find_map(|e| match e {
            OrchestratorEvent::ChatTurnEnded { outcome } => Some(outcome),
            _ => None,
        })
        .expect("the turn ended")
}

/// The tool's turn as the model received it: in the prompt of the model call
/// after the tool, the last turn that begins with `[<tool>]`.
fn tool_turn(w: &World, tool: &str) -> String {
    let calls = w.rec.calls();
    assert!(calls.len() >= 2, "precondition: the model was called again after the tool: {} call(s)", calls.len());
    let prompt = &calls[1].prompt;
    let marker = format!("[{tool}]");
    let at = prompt.rfind(&marker).unwrap_or_else(|| panic!("precondition: the tool's turn is in the prompt: {prompt}"));
    let rest = &prompt[at..];
    let end = rest.find("<|im_end|>").unwrap_or(rest.len());
    rest[..end].to_string()
}

// ------------------------------------------------- 1. a read tool's result --

/// (1) `get_document` on « Transfer », which no document is titled exactly:
/// the close titles reach the model masked — the unswept value as its label —
/// and the value she dismissed in another title as written (the control).
#[tokio::test]
async fn a_read_tools_close_titles_reach_the_model_masked() {
    let script = first_then_answer(tool_call("get_document", serde_json::json!({ "title": "Transfer" })));
    let mut w = keyed_world("mask-read-close", script).await;
    let office = w.thread("Office").await;
    unswept_doc(&w, &format!("Transfer {MAIL}"), &office).await;
    dismissed_doc(&w, &format!("Transfer {DISMISSED}"), DISMISSED, &office).await;
    chat_turn(&mut w, "read the transfer", ActionDecision::Approve).await;
    let turn = tool_turn(&w, "get_document");
    assert!(turn.contains("Close titles"), "precondition: the close-titles sentence: {turn}");
    assert!(!turn.contains(MAIL), "the unswept value never reaches the model as written: {turn}");
    assert!(turn.contains(DISMISSED), "a value she dismissed stays as written (control): {turn}");
}

/// The control on a path masked today: `get_document` on the dismissed
/// document's exact title — its title reaches the model with the value she
/// dismissed as written.
///
/// Green on main. The mutation that turns it red: the orchestrator's read mask
/// loaded without the account key (`ReadMask::load(db, None)`), so no
/// dismissal can be read and every value is masked.
#[tokio::test]
async fn a_dismissed_value_in_a_title_read_by_a_tool_stays_as_written() {
    let title = format!("Refund {DISMISSED}");
    let script = first_then_answer(tool_call("get_document", serde_json::json!({ "title": title })));
    let mut w = keyed_world("mask-read-dismissed", script).await;
    let office = w.thread("Office").await;
    dismissed_doc(&w, &title, DISMISSED, &office).await;
    chat_turn(&mut w, "read the refund", ActionDecision::Approve).await;
    let turn = tool_turn(&w, "get_document");
    assert!(turn.contains("Title:"), "precondition: the document was read: {turn}");
    assert!(turn.contains(DISMISSED), "a value she dismissed stays as written: {turn}");
}

// ------------------------------------------------ 2. a write tool's result --

/// (2) `move_document` of the unswept document, approved: the move's reply
/// reaches the model with the value masked.
#[tokio::test]
async fn a_moves_reply_reaches_the_model_masked() {
    let title = format!("Transfer {MAIL}");
    let script =
        first_then_answer(tool_call("move_document", serde_json::json!({ "document_title": title, "thread_name": "Kitchen" })));
    let mut w = keyed_world("mask-move-reply", script).await;
    let office = w.thread("Office").await;
    let kitchen = w.thread("Kitchen").await;
    let id = unswept_doc(&w, &title, &office).await;
    let o = chat_turn(&mut w, "move it to the kitchen", ActionDecision::Approve).await;
    assert_eq!(w.thread_of(&id).await, kitchen, "precondition: moved: {}", o.summary_line());
    let turn = tool_turn(&w, "move_document");
    assert!(turn.contains("Kitchen"), "precondition: the move's reply: {turn}");
    assert!(!turn.contains(MAIL), "the unswept value never reaches the model as written: {turn}");
}

/// (2) `move_document` into the thread it is already in: the « already in »
/// reply reaches the model with the value masked.
#[tokio::test]
async fn an_already_in_reply_reaches_the_model_masked() {
    let title = format!("Transfer {MAIL}");
    let script =
        first_then_answer(tool_call("move_document", serde_json::json!({ "document_title": title, "thread_name": "Office" })));
    let mut w = keyed_world("mask-move-already", script).await;
    let office = w.thread("Office").await;
    unswept_doc(&w, &title, &office).await;
    chat_turn(&mut w, "move it to the office", ActionDecision::Approve).await;
    let turn = tool_turn(&w, "move_document");
    assert!(turn.contains("already in"), "precondition: the « already in » reply: {turn}");
    assert!(!turn.contains(MAIL), "the unswept value never reaches the model as written: {turn}");
}

// ------------------------------------- 3. a write tool that does not resolve --

/// (3) `move_document` of « Transfer », which no document is titled exactly:
/// the sentence that says why, naming the close titles, reaches the model
/// masked — and the dismissed value as written (the control).
#[tokio::test]
async fn an_unresolved_writes_close_titles_reach_the_model_masked() {
    let script =
        first_then_answer(tool_call("move_document", serde_json::json!({ "document_title": "Transfer", "thread_name": "Kitchen" })));
    let mut w = keyed_world("mask-move-why", script).await;
    let office = w.thread("Office").await;
    w.thread("Kitchen").await;
    unswept_doc(&w, &format!("Transfer {MAIL}"), &office).await;
    dismissed_doc(&w, &format!("Transfer {DISMISSED}"), DISMISSED, &office).await;
    chat_turn(&mut w, "move the transfer to the kitchen", ActionDecision::Approve).await;
    let turn = tool_turn(&w, "move_document");
    assert!(turn.contains("Close titles"), "precondition: the sentence naming the close titles: {turn}");
    assert!(!turn.contains(MAIL), "the unswept value never reaches the model as written: {turn}");
    assert!(turn.contains(DISMISSED), "a value she dismissed stays as written (control): {turn}");
}
