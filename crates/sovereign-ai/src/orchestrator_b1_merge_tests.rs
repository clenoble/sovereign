//! v0.0.13 lot L5, B1 — **C-MERGE in the assistant** (spec ch. 03, *Merging
//! keeps the source thread — contract (C-MERGE)*, v2.0.109; ch. 08, AT4 and
//! AT7, v2.0.109): the merge's words (MG1), what a request naming an ended
//! thread is answered (MG3), the chat tools and the duplicate (MG3), the undo
//! by naming the thread (MG5), level 3 and earned trust (MG6, AT7).
//!
//! **The API this file names:** `security::UNDO_MERGE_ACTION`, the action the
//! router names for « undo the merge of {thread} » (« types and signatures »
//! commit; its handler, its level and the router's prompt are the
//! implementer's), and the three fields `Thread::{ended_at, merged_into,
//! merge_stamp}`. Everything else is today's surface. The harness gained
//! `world_reopened` (a new orchestrator over the same store) and the merge
//! fields in `snapshot`.
//!
//! **Level-3 actions whose auto-approved undo is held here** (AT7, « An
//! action approved by earned trust keeps its undo »): `rename_thread`,
//! `move_document`, `merge_threads`. The test reads the orchestrator's undo
//! record (`undo_log`) to find the request id of an action that showed no
//! proposal; it then undoes through `Orchestrator::undo`.
//!
//! The words are the contract's, compared whole (the Fable maille, by
//! delegation of Céline, 2026-11-17). With one document, « ({n} documents stay
//! in '{from}') » reads « (1 document stays in '{from}') »: the test author's
//! reading in v1, agreed by the Fable maille (2026-11-17).
//!
//! b1-tests-v2 adds what the model is shown of an ended thread (the Fable
//! maille, by delegation of Céline, 2026-11-17): `list_threads` lists it with
//! its mark, « - {name} ({n} documents; merged into '{target}' on
//! {YYYY-MM-DD}) »; the workspace context names live threads only, and counts
//! all threads.
//! A request's reply is read from the action's outcome, whatever its kind
//! (`Failed`, `Reported` or `NoChange`), and is never a change.
//!
//! Written before the code by the B1 test author; the implementer does not
//! edit this file. A test that looks wrong is reported, not worked around.

use super::assistant_harness::*;
use super::lh_tests::drive;
use super::*;
use sovereign_core::security::{RequestId, UNDO_MERGE_ACTION};

// --------------------------------------------------------------- words --

/// « Merge thread '{from}' into '{into}': '{from}' ends now and keeps its {n}
/// documents; new ones go to '{into}' »
const PROPOSAL: &str = "Merge thread '{from}' into '{into}': '{from}' ends now and keeps its {n} documents; new ones go to '{into}'";
/// The same with one document.
const PROPOSAL_ONE: &str = "Merge thread '{from}' into '{into}': '{from}' ends now and keeps its 1 document; new ones go to '{into}'";
/// « …: '{from}' ends now; it holds no documents »
const PROPOSAL_NONE: &str = "Merge thread '{from}' into '{into}': '{from}' ends now; it holds no documents";
/// « Merged '{from}' into '{into}' ({n} documents stay in '{from}') »
const OUTCOME: &str = "Merged '{from}' into '{into}' ({n} documents stay in '{from}')";
/// The same with one document (the test author's reading: the verb agrees).
const OUTCOME_ONE: &str = "Merged '{from}' into '{into}' (1 document stays in '{from}')";
/// « Merged '{from}' into '{into}' »
const OUTCOME_NONE: &str = "Merged '{from}' into '{into}'";
/// « '{name}' was merged into '{target}' on {YYYY-MM-DD} and takes no new
/// document. Use '{live end}' instead? Nothing was changed. »
const ENDED: &str =
    "'{name}' was merged into '{target}' on {date} and takes no new document. Use '{live end}' instead? Nothing was changed.";
/// « '{name}' has ended (merged into '{target}'), and no live thread takes
/// its place. Nothing was changed. »
const NO_LIVE_END: &str =
    "'{name}' has ended (merged into '{target}'), and no live thread takes its place. Nothing was changed.";
/// « '{into}' has ended (merged into '{target}'): merge into '{live end}'
/// instead. Nothing was changed. »
const INTO_ENDED: &str = "'{into}' has ended (merged into '{target}'): merge into '{live end}' instead. Nothing was changed.";
/// « Undid the merge of '{name}' into '{target}': '{name}' is live again »
const UNDONE: &str = "Undid the merge of '{name}' into '{target}': '{name}' is live again";
/// « thread '{name}' is not merged into another thread; nothing was undone »
const UNDO_REFUSED: &str = "thread '{name}' is not merged into another thread; nothing was undone";

fn fill(template: &str, parts: &[(&str, &str)]) -> String {
    let mut s = template.to_string();
    for (k, v) in parts {
        s = s.replace(k, v);
    }
    s
}

/// The local offset the date tests fix, so that local time differs from UTC
/// on a machine at UTC (as the LH tests do).
fn test_offset() -> chrono::FixedOffset {
    chrono::FixedOffset::east_opt(5 * 3600 + 30 * 60).unwrap()
}

/// The day of `t` in the device's local time (MG2: « the day shown is in the
/// device's local time »), at the fixed offset.
fn day(t: chrono::DateTime<chrono::Utc>) -> String {
    t.with_timezone(&test_offset()).format("%Y-%m-%d").to_string()
}

// ------------------------------------------------------------- helpers --

/// A router that classifies every query as `action` on `target`.
fn route_all(action: &str, target: Option<&str>) -> Script {
    let a = action.to_string();
    let t = target.map(str::to_string);
    Arc::new(move |_, _, _| intent_json(&a, t.as_deref()))
}

/// The outcome of the one `ActionExecuted` among `events`.
fn the_outcome(events: &[OrchestratorEvent]) -> ActionOutcome {
    let outs: Vec<ActionOutcome> = events
        .iter()
        .filter_map(|e| match e {
            OrchestratorEvent::ActionExecuted { outcome, .. } => Some(outcome.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(outs.len(), 1, "one action executed: {events:?}");
    outs[0].clone()
}

/// The words of an outcome, whatever its kind.
fn words(o: &ActionOutcome) -> String {
    match o {
        ActionOutcome::Changed { summary } | ActionOutcome::Reported { summary } => summary.clone(),
        ActionOutcome::NoChange { reason } | ActionOutcome::Failed { reason } => reason.clone(),
    }
}

/// Merges `source` into `into` through the service (setup), and checks that
/// the source is ended.
async fn merged(w: &World, source: &str, into: &str) {
    w.docs.thread_merge(into, source).await.expect("merged");
    assert_eq!(w.thread_row(source).await.merged_into.as_deref(), Some(into), "MG1: the source is ended");
}

fn ended_at(t: &Thread) -> chrono::DateTime<chrono::Utc> {
    t.ended_at.expect("an end date")
}

fn merge_stamp(t: &Thread) -> sovereign_docs::Stamp {
    sovereign_docs::parse_thread_stamp(t.merge_stamp.as_deref().expect("a merge stamp")).expect("reads as a stamp")
}

/// Runs one chat turn on `message`, answering every proposal with `decision`.
async fn chat_turn(w: &mut World, message: &str, decision: ActionDecision) -> Vec<OrchestratorEvent> {
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
    seen
}

fn tool_call(name: &str, args: serde_json::Value) -> String {
    format!("<tool_call>\n{}\n</tool_call>", serde_json::json!({ "name": name, "arguments": args }))
}

/// Five approvals of `action` on the direct path (earned trust).
fn trust_five(w: &World, action: &str) {
    let mut t = w.orch.trust.lock().unwrap();
    for _ in 0..5 {
        t.record_approval(crate::trust::WORKFLOW_QUERY, action);
    }
}

/// The request ids of the undos the orchestrator holds.
fn undo_ids(w: &World) -> Vec<RequestId> {
    w.orch.undo_log.lock().unwrap().keys().copied().collect()
}

// ------------------------------------------------------------------ MG1 --

/// MG1, the words: the proposal says that the source ends and keeps its
/// documents — two, one, none — and where new ones go. Rejected: nothing
/// changes.
///
/// Red today: the proposal says the documents move and the source is deleted.
#[tokio::test]
async fn mg1_the_merge_proposal_says_the_source_ends_and_keeps_its_documents() {
    for (i, (n, expected)) in [(2usize, PROPOSAL), (1, PROPOSAL_ONE), (0, PROPOSAL_NONE)].into_iter().enumerate() {
        let mut w = world(&format!("b1-mg1-proposal-{i}"), 4096, route_all("merge_threads", Some("Kitchen to Garden"))).await;
        let garden = w.thread("Garden").await;
        let kitchen = w.thread("Kitchen").await;
        w.docs_in(&garden, "Seed", n).await;
        w.doc("Pans", &kitchen, "pans").await;
        let before = snapshot(&w.store).await;
        let ps = proposals(
            &drive(&mut w, "merge 'Garden' into 'Kitchen'", vec![ActionDecision::Reject("no".into())], |_| async {}).await,
        );
        assert_eq!(ps.len(), 1, "{n} documents: one proposal");
        let want = fill(expected, &[("{from}", "Garden"), ("{into}", "Kitchen"), ("{n}", &n.to_string())]);
        assert_eq!(ps[0].description, want, "{n} documents");
        assert_eq!(snapshot(&w.store).await, before, "rejected: nothing changed");
    }
}

/// MG1, approved: the outcome's words — two, one, none — and the merge as the
/// contract has it: no document moves, the source is not deleted, it gets
/// its two fields, the target records nothing.
///
/// Red today: the documents move, the source is deleted, the words differ.
#[tokio::test]
async fn mg1_an_approved_merge_moves_nothing_and_says_so() {
    for (i, (n, expected)) in [(2usize, OUTCOME), (1, OUTCOME_ONE), (0, OUTCOME_NONE)].into_iter().enumerate() {
        let mut w = world(&format!("b1-mg1-outcome-{i}"), 4096, route_all("merge_threads", Some("Kitchen to Garden"))).await;
        let garden = w.thread("Garden").await;
        let kitchen = w.thread("Kitchen").await;
        let seeds = w.docs_in(&garden, "Seed", n).await;
        let kitchen_row = serde_json::to_value(w.thread_row(&kitchen).await).unwrap();
        let evs = drive(&mut w, "merge 'Garden' into 'Kitchen'", vec![ActionDecision::Approve], |_| async {}).await;
        let o = the_outcome(&evs);
        assert!(o.changed(), "{n} documents: a change: {o:?}");
        assert_eq!(words(&o), fill(expected, &[("{from}", "Garden"), ("{into}", "Kitchen"), ("{n}", &n.to_string())]));
        for s in &seeds {
            assert_eq!(w.thread_of(s).await, garden, "no document moves");
        }
        let g = w.thread_row(&garden).await;
        assert!(g.deleted_at.is_none(), "the source is not deleted");
        assert_eq!(g.merged_into.as_deref(), Some(kitchen.as_str()), "merged into Kitchen");
        assert!(g.ended_at.is_some());
        assert_eq!(serde_json::to_value(w.thread_row(&kitchen).await).unwrap(), kitchen_row, "the target records nothing");
    }
}

// ------------------------------------------------------------------ MG3 --

/// MG3: « A request that names it is answered that it was merged, on its
/// date, and its live end is proposed. Nothing is changed. » A move into the
/// ended Garden, and a new document in it.
///
/// Red today: the merge deletes Garden, so the request finds no thread
/// named 'Garden'.
#[tokio::test]
async fn mg3_a_request_that_names_an_ended_thread_is_answered_and_changes_nothing() {
    let _offset = crate::local_time::fix_offset(test_offset());
    let w = world("b1-mg3-named", 4096, answer("")).await;
    let garden = w.thread("Garden").await;
    let kitchen = w.thread("Kitchen").await;
    let shed = w.thread("Shed").await;
    w.doc("Pans", &shed, "pans").await;
    merged(&w, &garden, &kitchen).await;
    let want = fill(
        ENDED,
        &[("{name}", "Garden"), ("{target}", "Kitchen"), ("{date}", &day(ended_at(&w.thread_row(&garden).await))), ("{live end}", "Kitchen")],
    );
    let before = snapshot(&w.store).await;
    for (action, target) in [("move_document", "Pans to Garden"), ("create_document", "Minutes in Garden")] {
        let o = the_outcome(&w.act(action, Some(target)).await);
        assert!(!o.changed(), "{action}: never a change: {o:?}");
        assert_eq!(words(&o), want, "{action}");
        assert_eq!(snapshot(&w.store).await, before, "{action}: nothing changed");
    }
}

/// MG3, a chain: Attic into Garden, Garden into Kitchen. A request naming
/// Attic is answered with the thread it was merged into (Garden), that
/// merge's date, and the live end (Kitchen).
///
/// Red today: the merges delete Attic and Garden.
#[tokio::test]
async fn mg3_with_a_chain_the_answer_names_the_live_end() {
    let _offset = crate::local_time::fix_offset(test_offset());
    let w = world("b1-mg3-chain", 4096, answer("")).await;
    let attic = w.thread("Attic").await;
    let garden = w.thread("Garden").await;
    let kitchen = w.thread("Kitchen").await;
    let shed = w.thread("Shed").await;
    w.doc("Pans", &shed, "pans").await;
    merged(&w, &attic, &garden).await;
    merged(&w, &garden, &kitchen).await;
    let want = fill(
        ENDED,
        &[("{name}", "Attic"), ("{target}", "Garden"), ("{date}", &day(ended_at(&w.thread_row(&attic).await))), ("{live end}", "Kitchen")],
    );
    let before = snapshot(&w.store).await;
    let o = the_outcome(&w.act("move_document", Some("Pans to Attic")).await);
    assert!(!o.changed(), "{o:?}");
    assert_eq!(words(&o), want);
    assert_eq!(snapshot(&w.store).await, before, "nothing changed");
}

/// MG3: « With no live end (… a deleted end), the request … is refused and
/// said. » Garden into Kitchen, then Kitchen deleted.
///
/// Red today: the merge deletes Garden.
#[tokio::test]
async fn mg3_with_no_live_end_the_request_is_refused_with_its_sentence() {
    let w = world("b1-mg3-no-end", 4096, answer("")).await;
    let garden = w.thread("Garden").await;
    let kitchen = w.thread("Kitchen").await;
    let shed = w.thread("Shed").await;
    w.doc("Pans", &shed, "pans").await;
    merged(&w, &garden, &kitchen).await;
    sovereign_docs::delete_thread_to_trash(&w.docs, w.store.as_ref(), &kitchen).await.expect("Kitchen deleted");
    let before = snapshot(&w.store).await;
    let o = the_outcome(&w.act("move_document", Some("Pans to Garden")).await);
    assert!(!o.changed(), "{o:?}");
    assert_eq!(words(&o), fill(NO_LIVE_END, &[("{name}", "Garden"), ("{target}", "Kitchen")]));
    assert_eq!(snapshot(&w.store).await, before, "nothing changed");
}

/// MG3: « a merge into it is refused », with « '{into}' has ended (merged
/// into '{target}'): merge into '{live end}' instead. Nothing was changed. »
///
/// Red today: the merge deletes Garden.
#[tokio::test]
async fn mg3_a_merge_into_an_ended_thread_is_refused_with_its_sentence() {
    let w = world("b1-mg3-merge-into", 4096, answer("")).await;
    let garden = w.thread("Garden").await;
    let kitchen = w.thread("Kitchen").await;
    w.thread("Shed").await;
    merged(&w, &garden, &kitchen).await;
    let before = snapshot(&w.store).await;
    // The router's "Garden to Shed": Shed into Garden.
    let o = the_outcome(&w.act("merge_threads", Some("Garden to Shed")).await);
    assert!(!o.changed(), "{o:?}");
    assert_eq!(words(&o), fill(INTO_ENDED, &[("{into}", "Garden"), ("{target}", "Kitchen"), ("{live end}", "Kitchen")]));
    assert_eq!(snapshot(&w.store).await, before, "nothing changed");
}

/// MG3 through the chat tools (`tools::execute_write_tool`, and the chat
/// loop): a document is never created in, nor moved to, an ended thread;
/// the tool's output carries the contract's answer, which the model is
/// given.
///
/// Red today: the merge deletes Garden.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mg3_the_chat_tools_never_place_a_document_in_an_ended_thread() {
    let _offset = crate::local_time::fix_offset(test_offset());
    let w = world("b1-mg3-tools", 4096, answer("")).await;
    let garden = w.thread("Garden").await;
    let kitchen = w.thread("Kitchen").await;
    let shed = w.thread("Shed").await;
    w.doc("Pans", &shed, "pans").await;
    merged(&w, &garden, &kitchen).await;
    let want = fill(
        ENDED,
        &[("{name}", "Garden"), ("{target}", "Kitchen"), ("{date}", &day(ended_at(&w.thread_row(&garden).await))), ("{live end}", "Kitchen")],
    );
    let before = snapshot(&w.store).await;
    for call in [
        crate::tools::ToolCall {
            name: "create_document".into(),
            arguments: serde_json::json!({ "title": "Minutes", "thread_name": "Garden" }),
        },
        crate::tools::ToolCall {
            name: "move_document".into(),
            arguments: serde_json::json!({ "document_title": "Pans", "thread_name": "Garden" }),
        },
    ] {
        let r = crate::tools::execute_write_tool(&call, w.store.as_ref(), Some(&w.docs)).await;
        assert!(!r.success, "{}: not done ({})", call.name, r.output);
        assert!(r.output.contains(&want), "{}: the output answers as the contract does: {}", call.name, r.output);
        assert_eq!(snapshot(&w.store).await, before, "{}: nothing changed", call.name);
    }
}

/// MG3 through the chat loop: the model asks to create a document in the
/// ended Garden; nothing is created, and the model's next prompt carries the
/// answer.
///
/// Red today: the merge deletes Garden.
#[tokio::test]
async fn mg3_the_chat_loop_never_creates_a_document_in_an_ended_thread() {
    let _offset = crate::local_time::fix_offset(test_offset());
    let first = tool_call("create_document", serde_json::json!({ "title": "Minutes", "thread_name": "Garden" }));
    let script: Script = Arc::new(move |_, _, i| if i == 0 { first.clone() } else { "All set.".into() });
    let mut w = world("b1-mg3-chat", 4096, script).await;
    let garden = w.thread("Garden").await;
    let kitchen = w.thread("Kitchen").await;
    merged(&w, &garden, &kitchen).await;
    let want = fill(
        ENDED,
        &[("{name}", "Garden"), ("{target}", "Kitchen"), ("{date}", &day(ended_at(&w.thread_row(&garden).await))), ("{live end}", "Kitchen")],
    );
    let before = snapshot(&w.store).await;
    chat_turn(&mut w, "create a document Minutes in Garden", ActionDecision::Approve).await;
    assert_eq!(snapshot(&w.store).await, before, "nothing created");
    let calls = w.rec.calls();
    assert!(calls.len() >= 2, "the model was asked again after the tool: {} calls", calls.len());
    assert!(calls[1].prompt.contains(&want), "the model is given the answer");
}

/// MG3: « a duplicate of one of its documents go[es] to its live end » — the
/// assistant's duplicate.
///
/// Red today: the merge moves Seeds to Kitchen, so its copy lands there for
/// the wrong reason; the setup's check (Seeds stays in Garden) fails first.
#[tokio::test]
async fn mg3_the_assistants_duplicate_goes_to_the_live_end() {
    let w = world("b1-mg3-dup", 4096, answer("")).await;
    let garden = w.thread("Garden").await;
    let kitchen = w.thread("Kitchen").await;
    let seeds = w.doc("Seeds", &garden, "seed list").await;
    merged(&w, &garden, &kitchen).await;
    assert_eq!(w.thread_of(&seeds).await, garden, "setup: Seeds stays in Garden (MG1)");
    let o = the_outcome(&w.act("duplicate", Some("Seeds")).await);
    assert!(o.changed(), "{o:?}");
    let copies: Vec<String> = w
        .store
        .list_documents(None)
        .await
        .unwrap()
        .into_iter()
        .filter(|d| d.title == "Copy of Seeds")
        .map(|d| d.thread_id)
        .collect();
    assert_eq!(copies, vec![kitchen], "the copy is in the live end");
}

/// MG3: the duplicate a skill makes (the shell's document Skills menu runs
/// the core skill through `docs_host::ServiceSkillDb`, which reads the
/// document and creates the copy in the thread it read) goes to the live
/// end too.
///
/// Red today: the merge moves Seeds to Kitchen; the setup's check fails
/// first.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mg3_a_skills_duplicate_goes_to_the_live_end() {
    use sovereign_skills::SkillDbAccess;
    let w = world("b1-mg3-skill-dup", 4096, answer("")).await;
    let garden = w.thread("Garden").await;
    let kitchen = w.thread("Kitchen").await;
    let seeds = w.doc("Seeds", &garden, "seed list").await;
    merged(&w, &garden, &kitchen).await;
    assert_eq!(w.thread_of(&seeds).await, garden, "setup: Seeds stays in Garden (MG1)");
    let db = crate::docs_host::ServiceSkillDb::new(sovereign_skills::wrap_db(w.store.clone()), w.docs.clone(), "duplicate-document");
    // What the duplicate skill does (`duplicate_document.rs`).
    let (title, thread_id, content) = db.get_document(&seeds).unwrap();
    let copy = db.create_document(&format!("Copy of {title}"), &thread_id, &content).expect("copied");
    assert_eq!(w.thread_of(&copy).await, kitchen, "the copy is in the live end");
}

/// « - {name} ({n} documents; merged into '{target}' on {YYYY-MM-DD}) »
const LISTED_ENDED: &str = "- {name} ({n} documents; merged into '{target}' on {date})";

/// MG3 and MG5, what the model is shown (b1-tests-v2; the Fable maille, by
/// delegation of Céline, 2026-11-17): the `list_threads` tool lists an ended
/// thread with its mark — « - {name} ({n} documents; merged into '{target}' on
/// {YYYY-MM-DD}) », the day in local time — so that the model can say what it
/// holds and undo the merge by name; a live thread keeps today's line.
///
/// Red today: the merge deletes Garden, so it is not listed.
#[tokio::test]
async fn mg3_list_threads_lists_an_ended_thread_with_its_mark() {
    let _offset = crate::local_time::fix_offset(test_offset());
    let w = world("b1-mg3-list", 4096, answer("")).await;
    let garden = w.thread("Garden").await;
    let kitchen = w.thread("Kitchen").await;
    w.docs_in(&garden, "Seed", 2).await;
    w.docs_in(&kitchen, "Pan", 3).await;
    merged(&w, &garden, &kitchen).await;
    let want = fill(
        LISTED_ENDED,
        &[("{name}", "Garden"), ("{n}", "2"), ("{target}", "Kitchen"), ("{date}", &day(ended_at(&w.thread_row(&garden).await)))],
    );
    let call = crate::tools::ToolCall { name: "list_threads".into(), arguments: serde_json::json!({}) };
    let r = crate::tools::execute_tool(&call, w.store.as_ref()).await;
    assert!(r.success, "{}", r.output);
    let lines: Vec<&str> = r.output.lines().collect();
    assert!(lines.contains(&want.as_str()), "the ended thread, with its mark: {want:?} among {lines:?}");
    assert!(lines.contains(&"- Kitchen (3 documents)"), "a live thread keeps its line: {lines:?}");
    assert_eq!(lines.len(), 2, "one line per thread: {lines:?}");
}

/// MG3 (b1-tests-v2; the Fable maille, by delegation of Céline, 2026-11-17):
/// « The workspace context names live threads only; its count is of all
/// threads » — the names are what the model places documents in.
///
/// Red today: the merge deletes Garden (the setup's check fails first).
#[tokio::test]
async fn mg3_the_workspace_context_names_live_threads_only_and_counts_all() {
    let w = world("b1-mg3-context", 4096, answer("")).await;
    let garden = w.thread("Garden").await;
    let kitchen = w.thread("Kitchen").await;
    w.thread("Shed").await;
    merged(&w, &garden, &kitchen).await;
    let ctx = crate::llm::context::gather_workspace_context(w.store.as_ref()).await;
    let mut names = ctx.thread_names.clone();
    names.sort();
    assert_eq!(names, vec!["Kitchen".to_string(), "Shed".to_string()], "live threads only");
    assert_eq!(ctx.thread_count, 3, "the count is of all threads, the ended one included");
}

// ------------------------------------------------------------------ MG5 --

/// MG5: « At any time and from any device, by naming the thread … it
/// outlives a restart. » The merge approved, then a new orchestrator over the
/// same store, then « undo the merge of 'Garden' »: the two fields cleared
/// under a new merge stamp, the contract's words. Any proposal it makes is
/// approved.
///
/// Red today: the merge deletes Garden; no action undoes a merge by name.
#[tokio::test]
async fn mg5_the_undo_by_name_outlives_a_restart() {
    let mut w = world("b1-mg5-name", 4096, route_all("merge_threads", Some("Kitchen to Garden"))).await;
    let garden = w.thread("Garden").await;
    let kitchen = w.thread("Kitchen").await;
    let seeds = w.doc("Seeds", &garden, "seed list").await;
    drive(&mut w, "merge 'Garden' into 'Kitchen'", vec![ActionDecision::Approve], |_| async {}).await;
    let g = w.thread_row(&garden).await;
    assert_eq!(g.merged_into.as_deref(), Some(kitchen.as_str()), "setup: merged (MG1)");
    let first = merge_stamp(&g);

    let mut w2 = world_reopened("b1-mg5-name-2", 4096, route_all(UNDO_MERGE_ACTION, Some("Garden")), w.store.clone()).await;
    drop(w);
    let evs = drive(&mut w2, "undo the merge of 'Garden'", vec![ActionDecision::Approve], |_| async {}).await;
    let o = the_outcome(&evs);
    assert!(o.changed(), "{o:?}");
    assert_eq!(words(&o), fill(UNDONE, &[("{name}", "Garden"), ("{target}", "Kitchen")]));
    let g = w2.thread_row(&garden).await;
    assert_eq!((g.ended_at, g.merged_into.as_deref()), (None, None), "the two fields cleared");
    assert!(merge_stamp(&g) > first, "under a new merge stamp");
    assert_eq!(w2.thread_of(&seeds).await, garden);
}

/// MG5: « « Undo that », after the merge (AT7), is the same operation. » The
/// undo of the merge's request clears the two fields under a new merge stamp,
/// with the same words, and moves no document.
///
/// Red today: the undo restores a deleted thread and moves the documents
/// back, with other words.
#[tokio::test]
async fn mg5_undo_that_after_the_merge_is_the_same_operation() {
    let mut w = world("b1-mg5-that", 4096, route_all("merge_threads", Some("Kitchen to Garden"))).await;
    let garden = w.thread("Garden").await;
    let kitchen = w.thread("Kitchen").await;
    w.doc("Seeds", &garden, "seed list").await;
    let ps = proposals(&drive(&mut w, "merge 'Garden' into 'Kitchen'", vec![ActionDecision::Approve], |_| async {}).await);
    assert_eq!(ps.len(), 1);
    let g = w.thread_row(&garden).await;
    assert_eq!(g.merged_into.as_deref(), Some(kitchen.as_str()), "setup: merged (MG1)");
    let first = merge_stamp(&g);
    let documents = snapshot(&w.store).await.documents;

    let o = w.orch.undo(&ps[0].request_id).await;
    assert!(o.changed(), "{o:?}");
    assert_eq!(words(&o), fill(UNDONE, &[("{name}", "Garden"), ("{target}", "Kitchen")]));
    let g = w.thread_row(&garden).await;
    assert_eq!((g.ended_at, g.merged_into.as_deref()), (None, None), "the two fields cleared");
    assert!(merge_stamp(&g) > first, "under a new merge stamp");
    assert_eq!(snapshot(&w.store).await.documents, documents, "no document moved or written");
}

/// MG5: « It is refused if the thread is not ended, and the reply says so »:
/// a thread never merged; then, after an undo, a second undo by name; then
/// the undo of the merge's request, its merge undone since (AT7).
///
/// Red today: no action undoes a merge by name.
#[tokio::test]
async fn mg5_the_undo_is_refused_when_the_thread_is_not_ended() {
    let script: Script = Arc::new(|prompt: &str, _, _| {
        if prompt.contains("MERGE") {
            intent_json("merge_threads", Some("Kitchen to Garden"))
        } else if prompt.contains("KITCHEN") {
            intent_json(UNDO_MERGE_ACTION, Some("Kitchen"))
        } else {
            intent_json(UNDO_MERGE_ACTION, Some("Garden"))
        }
    });
    let mut w = world("b1-mg5-refused", 4096, script).await;
    let garden = w.thread("Garden").await;
    w.thread("Kitchen").await;

    let before = snapshot(&w.store).await;
    let o = the_outcome(&drive(&mut w, "undo the merge of 'KITCHEN'", vec![ActionDecision::Approve], |_| async {}).await);
    assert!(!o.changed(), "{o:?}");
    assert_eq!(words(&o), fill(UNDO_REFUSED, &[("{name}", "Kitchen")]), "never merged");
    assert_eq!(snapshot(&w.store).await, before, "nothing changed");

    let ps = proposals(&drive(&mut w, "MERGE 'Garden' into 'Kitchen'", vec![ActionDecision::Approve], |_| async {}).await);
    assert_eq!(ps.len(), 1);
    assert!(w.thread_row(&garden).await.merged_into.is_some(), "setup: merged (MG1)");
    let o = the_outcome(&drive(&mut w, "undo the merge of 'Garden'", vec![ActionDecision::Approve], |_| async {}).await);
    assert!(o.changed(), "the first undo: {o:?}");
    let before = snapshot(&w.store).await;
    let o = the_outcome(&drive(&mut w, "undo the merge of 'Garden'", vec![ActionDecision::Approve], |_| async {}).await);
    assert_eq!(words(&o), fill(UNDO_REFUSED, &[("{name}", "Garden")]), "a second undo");
    assert_eq!(snapshot(&w.store).await, before, "nothing changed");

    let o = w.orch.undo(&ps[0].request_id).await;
    assert!(!o.changed(), "the merge's request, its merge undone since: refused: {o:?}");
    assert_eq!(snapshot(&w.store).await, before, "nothing changed");
}

/// AT7: « An undo is refused if the object changed since the action … for a
/// merge, its merge changed since (undone, or redone). » Merged by a request,
/// undone, merged again (another merge): the first request's undo is
/// refused, and the second merge stays.
///
/// Red today: the merge deletes Garden (`thread_unmerge` is not built).
#[tokio::test]
async fn at7_the_undo_of_a_merge_redone_since_is_refused() {
    let mut w = world("b1-at7-redone", 4096, route_all("merge_threads", Some("Kitchen to Garden"))).await;
    let garden = w.thread("Garden").await;
    let kitchen = w.thread("Kitchen").await;
    let ps = proposals(&drive(&mut w, "merge 'Garden' into 'Kitchen'", vec![ActionDecision::Approve], |_| async {}).await);
    assert_eq!(ps.len(), 1);
    assert!(w.thread_row(&garden).await.merged_into.is_some(), "setup: merged (MG1)");
    w.docs.thread_unmerge(&garden).await.expect("undone");
    merged(&w, &garden, &kitchen).await;
    let before = snapshot(&w.store).await;
    let o = w.orch.undo(&ps[0].request_id).await;
    assert!(!o.changed(), "{o:?}");
    assert!(words(&o).contains("Garden"), "the reply names the thread: {}", words(&o));
    assert_eq!(snapshot(&w.store).await, before, "the second merge stays");
}

// ------------------------------------------------------- MG6 and AT7 --

/// MG6: a merge follows level 3's rules, earned trust included: after five
/// approvals, a merge with both names quoted runs without a proposal. AT7:
/// « an auto-approved merge keeps its undo »: the orchestrator holds its
/// undo, and undoing it clears the merge.
///
/// Red today: a merge is Destruct (never auto-approved).
#[tokio::test]
async fn mg6_a_merge_approved_by_earned_trust_runs_and_keeps_its_undo() {
    let mut w = world("b1-mg6-trust", 4096, route_all("merge_threads", Some("Kitchen to Garden"))).await;
    let garden = w.thread("Garden").await;
    let kitchen = w.thread("Kitchen").await;
    trust_five(&w, "merge_threads");
    let evs = drive(&mut w, "merge 'Garden' into 'Kitchen'", vec![ActionDecision::Reject("not asked".into())], |_| async {}).await;
    assert!(proposals(&evs).is_empty(), "auto-approved by earned trust: no proposal: {evs:?}");
    assert_eq!(w.thread_row(&garden).await.merged_into.as_deref(), Some(kitchen.as_str()), "merged");
    let ids = undo_ids(&w);
    assert_eq!(ids.len(), 1, "its undo is kept");
    let o = w.orch.undo(&ids[0]).await;
    assert_eq!(words(&o), fill(UNDONE, &[("{name}", "Garden"), ("{target}", "Kitchen")]));
    assert_eq!(w.thread_row(&garden).await.merged_into, None, "undone");
}

/// AT7 (v2.0.109): « An action approved by earned trust keeps its undo, as
/// one the user approved does. » A rename auto-approved; its undo gives the
/// old name back.
///
/// Red today: an auto-approved action records no undo.
#[tokio::test]
async fn at7_a_rename_approved_by_earned_trust_keeps_its_undo() {
    let mut w = world("b1-at7-rename", 4096, route_all("rename_thread", Some("Plans to Roadmap"))).await;
    let plans = w.thread("Plans").await;
    trust_five(&w, "rename_thread");
    let evs = drive(&mut w, "rename 'Plans' to 'Roadmap'", vec![ActionDecision::Reject("not asked".into())], |_| async {}).await;
    assert!(proposals(&evs).is_empty(), "auto-approved: no proposal");
    assert_eq!(w.thread_row(&plans).await.name, "Roadmap", "renamed");
    let ids = undo_ids(&w);
    assert_eq!(ids.len(), 1, "its undo is kept");
    let o = w.orch.undo(&ids[0]).await;
    assert!(o.changed(), "{o:?}");
    assert_eq!(w.thread_row(&plans).await.name, "Plans", "the old name back");
}

/// AT7: the same for a move auto-approved: its undo moves the document back.
///
/// Red today: an auto-approved action records no undo.
#[tokio::test]
async fn at7_a_move_approved_by_earned_trust_keeps_its_undo() {
    let mut w = world("b1-at7-move", 4096, route_all("move_document", Some("Seeds to Kitchen"))).await;
    let garden = w.thread("Garden").await;
    let kitchen = w.thread("Kitchen").await;
    let seeds = w.doc("Seeds", &garden, "seed list").await;
    trust_five(&w, "move_document");
    let evs = drive(&mut w, "move 'Seeds' to 'Kitchen'", vec![ActionDecision::Reject("not asked".into())], |_| async {}).await;
    assert!(proposals(&evs).is_empty(), "auto-approved: no proposal");
    assert_eq!(w.thread_of(&seeds).await, kitchen, "moved");
    let ids = undo_ids(&w);
    assert_eq!(ids.len(), 1, "its undo is kept");
    let o = w.orch.undo(&ids[0]).await;
    assert!(o.changed(), "{o:?}");
    assert_eq!(w.thread_of(&seeds).await, garden, "moved back");
}

/// The trust file as v0.0.12 wrote it (`trust.rs` at the tag `v0.0.12`): its
/// entries in field order, the MAC over `(entries, threshold)`.
#[derive(serde::Serialize)]
struct V0012Entry {
    consecutive_approvals: u32,
    last_rejection: Option<String>,
    last_approval: Option<String>,
}

#[derive(serde::Serialize)]
struct V0012File {
    mac: String,
    entries: std::collections::BTreeMap<String, V0012Entry>,
    auto_approve_threshold: u32,
}

/// MG6: « The approvals counted while a merge was level 5 do not count: its
/// trust starts at zero in v0.0.13. » A trust file written by v0.0.12 with
/// five fresh approvals of `merge_threads` (and, as a control, of
/// `rename_thread`), under the session key: after login, the rename
/// auto-approves (the file was accepted), the merge does not. Five approvals
/// made in v0.0.13 then count, and survive a save and a login.
///
/// Red today: the merge's five approvals of v0.0.12 count.
#[test]
fn mg6_the_approvals_counted_while_merge_was_level_5_do_not_count() {
    use sovereign_core::security::ActionLevel;
    let dir = std::env::temp_dir().join(format!("sovereign_test_b1_trust_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let key = [9u8; 32];
    let fresh = Some(chrono::Utc::now().to_rfc3339());
    let mut entries = std::collections::BTreeMap::new();
    for action in ["merge_threads", "rename_thread"] {
        entries.insert(
            format!("{}:{action}", crate::trust::WORKFLOW_QUERY),
            V0012Entry { consecutive_approvals: 5, last_rejection: None, last_approval: fresh.clone() },
        );
    }
    let mac = sovereign_crypto::mac::keyed_mac(
        &key,
        b"sovereign-trust-state:v1",
        &serde_json::to_vec(&(&entries, 5u32)).unwrap(),
    );
    let file = V0012File { mac, entries, auto_approve_threshold: 5 };
    std::fs::write(dir.join("trust_state.json"), serde_json::to_string_pretty(&file).unwrap()).unwrap();

    let mut t = crate::trust::TrustTracker::load(&dir).unwrap();
    t.arm_key(&dir, key);
    let q = crate::trust::WORKFLOW_QUERY;
    assert!(t.should_auto_approve(q, "rename_thread", ActionLevel::Modify), "control: the v0.0.12 file is accepted");
    assert!(!t.should_auto_approve(q, "merge_threads", ActionLevel::Modify), "the merge's approvals of v0.0.12 do not count");

    for _ in 0..5 {
        t.record_approval(q, "merge_threads");
    }
    assert!(t.should_auto_approve(q, "merge_threads", ActionLevel::Modify), "five approvals made in v0.0.13 count");
    t.save(&dir).unwrap();
    let mut again = crate::trust::TrustTracker::load(&dir).unwrap();
    again.arm_key(&dir, key);
    assert!(again.should_auto_approve(q, "merge_threads", ActionLevel::Modify), "and survive a login");
    let _ = std::fs::remove_dir_all(&dir);
}

/// MG5: the router knows the undo of a merge (the person asks the assistant;
/// Céline, 2026-11-17: « Assistant seul »), and the orchestrator handles it.
///
/// Red today: neither names it.
#[test]
fn mg5_the_router_and_the_orchestrator_know_the_undo_of_a_merge() {
    assert!(crate::llm::prompt::build_router_system_prompt().contains(UNDO_MERGE_ACTION), "the router's prompt names it");
    assert!(HANDLED_ACTIONS.contains(&UNDO_MERGE_ACTION), "the orchestrator has a handler for it");
}

// ----------------------------------------------------------- AT4, level 3 --

/// AT4 at level 3 (MG6: « It follows the level's usual rules »): for a level
/// other than Destruct, the re-check is « the target alone ». A merge approved
/// after the source was renamed elsewhere is proposed again; a merge approved
/// after the source gained a document runs. Replaces
/// `at4_merge_reproposed_when_source_gained_a_document`, which held the
/// Destruct re-check, contradicted by MG6.
///
/// Red today: a merge is Destruct; the source's documents count.
#[tokio::test]
async fn at4_a_merge_at_level_3_rechecks_the_source_alone() {
    // Renamed since the proposal: proposed again.
    let mut w = world("b1-at4-renamed", 4096, route_all("merge_threads", Some("Kitchen to Garden"))).await;
    let garden = w.thread("Garden").await;
    w.thread("Kitchen").await;
    let svc = w.docs.clone();
    let gid = garden.clone();
    let evs = drive(&mut w, "merge 'Garden' into 'Kitchen'", vec![ActionDecision::Approve], move |_| async move {
        svc.thread_update(&gid, Some("Garden 2"), None).await.unwrap();
    })
    .await;
    assert_eq!(proposals(&evs).len(), 2, "renamed since: proposed again");
    assert_eq!(w.thread_row(&garden).await.merged_into, None, "not merged on the first approval");

    // A document added since the proposal: the merge runs.
    let mut w = world("b1-at4-gained", 4096, route_all("merge_threads", Some("Kitchen to Garden"))).await;
    let garden = w.thread("Garden").await;
    let kitchen = w.thread("Kitchen").await;
    let svc = w.docs.clone();
    let gid = garden.clone();
    let evs = drive(&mut w, "merge 'Garden' into 'Kitchen'", vec![ActionDecision::Approve], move |_| async move {
        crate::docs_host::create(&svc, crate::docs_host::new_user_document("Bulbs", &gid, "late")).await.unwrap();
    })
    .await;
    assert_eq!(proposals(&evs).len(), 1, "a document added is not a change of the target at level 3");
    assert_eq!(w.thread_row(&garden).await.merged_into.as_deref(), Some(kitchen.as_str()), "merged");
}
