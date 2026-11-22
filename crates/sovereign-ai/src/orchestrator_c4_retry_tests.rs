//! v0.0.13 item C4 — **a write retried after a no-match always asks** (spec ch. 08, *Action
//! targets*, RT1-RT5; Céline, 2026-11-18, choosing « A retried write always asks »; RT4's words, the
//! Fable maille, by delegation of Céline, 2026-11-18). Written before the code by its test author
//! (PR-0011); the implementer does not edit this file. A test that looks wrong is reported, not
//! worked around.
//!
//! Every test runs one chat turn through `execute_action("chat")`, the model scripted to call
//! tools in order, each proposal answered as the test says.
//!
//! **RT3, where the turn is put.** As built, a chat write is forced to confirmation whenever the
//! turn has read workspace content, and the initial workspace context counts as read when it holds
//! any thread or document title (*Trust calibration*). So the tests that hold RT2 on its own run in
//! an empty workspace: the no-match is a rename of a thread that does not exist, and the write after
//! it creates a thread, under earned trust. A control holds that, there, the trusted write runs
//! unseen: nothing else forces it. RT1 and RT4's « both reasons » need threads, so the workspace is
//! not empty there.

use super::assistant_harness::*;
use super::*;

/// RT4's words (the Fable maille, by delegation of Céline, 2026-11-18).
const ASKED_BECAUSE: &str = "Asked because a name in this request was not found.";

fn tool_call(name: &str, args: serde_json::Value) -> String {
    format!("<tool_call>\n{}\n</tool_call>", serde_json::json!({ "name": name, "arguments": args }))
}

/// The model calls `calls` in order, then answers in words.
fn calls(calls: Vec<String>) -> Script {
    Arc::new(move |_, _, i| calls.get(i).cloned().unwrap_or_else(|| "Done.".to_string()))
}

/// Five approvals of `action` on the chat path: the trust threshold.
fn trust_five(w: &World, action: &str) {
    let mut t = w.orch.trust.lock().unwrap();
    for _ in 0..5 {
        t.record_approval(crate::trust::WORKFLOW_CHAT, action);
    }
}

fn approvals(w: &World, action: &str) -> u32 {
    w.orch.trust.lock().unwrap().approval_count(crate::trust::WORKFLOW_CHAT, action)
}

/// One chat turn on `message`; the n-th proposal is answered with `decisions[n]`, any beyond with
/// a rejection. Every event.
async fn chat_turn(w: &mut World, message: &str, decisions: Vec<ActionDecision>) -> Vec<OrchestratorEvent> {
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
        let mut answered = 0usize;
        for _ in 0..20_000 {
            let new = rec.take_events();
            let n = proposals(&new).len();
            seen.extend(new);
            for _ in 0..n {
                let d = decisions.get(answered).cloned().unwrap_or_else(|| ActionDecision::Reject("test: no more".into()));
                answered += 1;
                let _ = dtx.send(d).await;
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
    seen
}

async fn thread_names(w: &World) -> Vec<String> {
    let mut v: Vec<String> = w.store.list_threads().await.unwrap().into_iter().map(|t| t.name).collect();
    v.sort();
    v
}

/// The chat messages sent before the first proposal, in order.
fn said_before_the_proposal(evs: &[OrchestratorEvent]) -> String {
    let mut out = vec![];
    for e in evs {
        match e {
            OrchestratorEvent::ActionProposed { .. } => break,
            OrchestratorEvent::ChatResponse { text } => out.push(text.clone()),
            _ => {}
        }
    }
    out.join("\n")
}

fn rename_missing() -> String {
    tool_call("rename_thread", serde_json::json!({ "old_name": "Marketting", "new_name": "Sales" }))
}

fn create(name: &str) -> String {
    tool_call("create_thread", serde_json::json!({ "name": name }))
}

// ── RT3: the control ──────────────────────────────────────────────────────

/// In an empty workspace, with trust reached, a write with no no-match before it runs unseen:
/// nothing else forces its confirmation here.
#[tokio::test]
async fn rt3_control_a_trusted_write_runs_unseen_in_an_empty_workspace() {
    let mut w = world("c4-control", 4096, calls(vec![create("Ops")])).await;
    assert!(thread_names(&w).await.is_empty(), "fixture: an empty workspace");
    trust_five(&w, "create_thread");
    let evs = chat_turn(&mut w, "make a thread Ops", vec![]).await;
    assert!(proposals(&evs).is_empty(), "control: trusted, it runs unseen: {}", reply(&evs));
    assert_eq!(thread_names(&w).await, vec!["Ops".to_string()], "control: it ran");
}

// ── RT2, RT3 ──────────────────────────────────────────────────────────────

/// After a chat tool answered that a name did not resolve, the next write of the turn, of another
/// object and another action, is shown for approval even under earned trust; rejected, nothing is
/// written.
#[tokio::test]
async fn rt2_after_a_no_match_a_trusted_write_is_shown() {
    let mut w = world("c4-shown", 4096, calls(vec![rename_missing(), create("Marketing")])).await;
    trust_five(&w, "create_thread");
    let evs = chat_turn(&mut w, "rename Marketting to Sales", vec![ActionDecision::Reject("no".into())]).await;
    let ps = proposals(&evs);
    assert_eq!(ps.len(), 1, "RT2: the write after the no-match is shown: {}", reply(&evs));
    assert!(thread_names(&w).await.is_empty(), "rejected: nothing is written");
}

/// Approved, it runs.
#[tokio::test]
async fn rt2_after_a_no_match_the_write_runs_once_approved() {
    let mut w = world("c4-approved", 4096, calls(vec![rename_missing(), create("Marketing")])).await;
    trust_five(&w, "create_thread");
    let evs = chat_turn(&mut w, "rename Marketting to Sales", vec![ActionDecision::Approve]).await;
    assert_eq!(proposals(&evs).len(), 1, "RT2: shown: {}", reply(&evs));
    assert_eq!(thread_names(&w).await, vec!["Marketing".to_string()], "approved: it ran");
}

/// RT2 holds for the rest of the turn: two writes after the no-match are both shown.
#[tokio::test]
async fn rt2_every_write_in_the_rest_of_the_turn_is_shown() {
    let mut w = world("c4-every", 4096, calls(vec![rename_missing(), create("A"), create("B")])).await;
    trust_five(&w, "create_thread");
    let evs = chat_turn(&mut w, "rename Marketting to Sales", vec![ActionDecision::Approve, ActionDecision::Approve]).await;
    assert_eq!(proposals(&evs).len(), 2, "RT2: both writes are shown: {}", reply(&evs));
}

/// The turn is RT2's scope: in the next turn, the person's own reply, a trusted write runs unseen
/// again.
#[tokio::test]
async fn rt2_the_next_turn_is_a_new_request() {
    // The first turn: the no-match, then a write, then words. The second turn, her reply: a write,
    // then words. Read from the request, so the script does not depend on how many calls a turn makes.
    let (missing, marketing, ops) = (rename_missing(), create("Marketing"), create("Ops"));
    let script: Script = Arc::new(move |prompt: &str, _, i| {
        if let Some(asked) = prompt.rfind("yes, Ops") {
            if prompt.rfind("[create_thread]").is_some_and(|done| done > asked) {
                "Done.".to_string()
            } else {
                ops.clone()
            }
        } else {
            match i {
                0 => missing.clone(),
                1 => marketing.clone(),
                _ => "Not done.".to_string(),
            }
        }
    });
    let mut w = world("c4-next", 4096, script).await;
    trust_five(&w, "create_thread");
    let first = chat_turn(&mut w, "rename Marketting to Sales", vec![ActionDecision::Reject("no".into())]).await;
    assert_eq!(proposals(&first).len(), 1, "precondition: the first turn's write was shown");
    assert!(thread_names(&w).await.is_empty(), "precondition: nothing written");
    trust_five(&w, "create_thread");
    let next = chat_turn(&mut w, "yes, Ops", vec![]).await;
    assert!(proposals(&next).is_empty(), "a later turn is a new request: RT2 does not reach it: {}", reply(&next));
    assert_eq!(thread_names(&w).await, vec!["Ops".to_string()], "it ran, trusted");
}

// ── RT1 ───────────────────────────────────────────────────────────────────

/// The model still sees the close names: the tool's reply to the name that did not resolve, which
/// the model reads next, offers « Marketing plan » for « Marketing » (the store offers the threads
/// that share a word with the name).
#[tokio::test]
async fn rt1_the_model_still_sees_the_close_names() {
    let call = tool_call("rename_thread", serde_json::json!({ "old_name": "Marketing", "new_name": "Sales" }));
    let mut w = world("c4-close", 4096, calls(vec![call])).await;
    w.thread("Marketing plan").await;
    let _ = chat_turn(&mut w, "rename Marketing to Sales", vec![]).await;
    let cs = w.rec.calls();
    assert!(cs.len() >= 2, "fixture: the model was asked again after the tool");
    let next = &cs[1].prompt;
    let at = next.rfind("[rename_thread]").expect("the tool's reply is in what the model reads next");
    assert!(next[at..].contains("Marketing plan"), "the close name is offered:\n{}", &next[at..]);
    assert_eq!(thread_names(&w).await, vec!["Marketing plan".to_string()], "nothing renamed");
}

// ── RT4 ───────────────────────────────────────────────────────────────────

/// The proposal is preceded by the reason, in the decided words.
#[tokio::test]
async fn rt4_the_proposal_says_why() {
    let mut w = world("c4-why", 4096, calls(vec![rename_missing(), create("Marketing")])).await;
    trust_five(&w, "create_thread");
    let evs = chat_turn(&mut w, "rename Marketting to Sales", vec![ActionDecision::Reject("no".into())]).await;
    assert_eq!(proposals(&evs).len(), 1, "fixture: shown");
    let before = said_before_the_proposal(&evs);
    assert!(before.contains(ASKED_BECAUSE), "RT4: « {ASKED_BECAUSE} » before the proposal:\n{before}");
}

/// When the turn's content also forces it (a workspace with a thread counts as read), both reasons
/// are said, this one first.
#[tokio::test]
async fn rt4_both_reasons_this_one_first() {
    let mut w = world("c4-both", 4096, calls(vec![rename_missing(), create("Ops")])).await;
    w.thread("Marketing").await;
    trust_five(&w, "create_thread");
    let evs = chat_turn(&mut w, "rename Marketting to Sales", vec![ActionDecision::Reject("no".into())]).await;
    assert_eq!(proposals(&evs).len(), 1, "fixture: shown: {}", reply(&evs));
    let before = said_before_the_proposal(&evs);
    let ours = before.find(ASKED_BECAUSE).unwrap_or_else(|| panic!("RT4: our reason is said:\n{before}"));
    let theirs = before.find("external content").unwrap_or_else(|| panic!("the content's reason is said too:\n{before}"));
    assert!(ours < theirs, "RT4: this reason first:\n{before}");
}

// ── RT5 ───────────────────────────────────────────────────────────────────

/// An approval counts toward the key, as for any proposal.
#[tokio::test]
async fn rt5_an_approval_counts() {
    let mut w = world("c4-count", 4096, calls(vec![rename_missing(), create("Marketing")])).await;
    trust_five(&w, "create_thread");
    let before = approvals(&w, "create_thread");
    let _ = chat_turn(&mut w, "rename Marketting to Sales", vec![ActionDecision::Approve]).await;
    assert_eq!(approvals(&w, "create_thread"), before + 1, "RT5: the approval counts");
}

/// A rejection resets the key, as for any proposal.
#[tokio::test]
async fn rt5_a_rejection_resets() {
    let mut w = world("c4-reset", 4096, calls(vec![rename_missing(), create("Marketing")])).await;
    trust_five(&w, "create_thread");
    let _ = chat_turn(&mut w, "rename Marketting to Sales", vec![ActionDecision::Reject("no".into())]).await;
    assert_eq!(approvals(&w, "create_thread"), 0, "RT5: the rejection resets");
}
