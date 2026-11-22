//! v0.0.13 lot LH — **contract C-HOST as a whole** (spec ch. 08, *Action
//! targets — identity, not position or name*, AT1–AT12, and *C-HOST as a
//! whole, for v0.0.13 (lot LH)*; ch. 11, *A skill asked for in the chat*).
//!
//! Only the guarantees **not built in v0.0.12** are tested here; what v0.0.12
//! built (exact names, the proposal naming the resolved thread or document,
//! the action run on the resolved id) has its tests in
//! `orchestrator_thread_names_tests.rs` and `orchestrator_document_titles_tests.rs`.
//!
//! Everything here fails at run time until the behaviour exists. The
//! guarantees that use the API of the « types and signatures » commit (AT1's
//! action target, AT6's request id and deadline, AT7's undo) are in
//! `orchestrator_lh_api_tests.rs`. The dates of AT1 and AT11 are rendered at a
//! fixed offset (+05:30) through `crate::local_time`, so they fail at UTC too.
//!
//! Per D16 (ch. 01: no behaviour in an interface), every test runs in the crate
//! the hosts link, without a window.
//!
//! "The reply" is what the user is told (`assistant_harness::reply`: chat
//! messages, refusals and executed events, read through `Debug`). "Not done"
//! is read from the store.
//!
//! Written before the code by another author (PR-0011); the implementer does
//! not edit this file. A test that looks wrong is reported, not worked around.

use super::assistant_harness::*;
use super::*;

// ------------------------------------------------------------- helpers --

/// A router that classifies the first call as `action` on `target`, and
/// answers every later call with an empty string.
fn route(action: &str, target: Option<&str>) -> Script {
    let a = action.to_string();
    let t = target.map(str::to_string);
    Arc::new(move |_, _, i| if i == 0 { intent_json(&a, t.as_deref()) } else { String::new() })
}

/// Runs `query` through `handle_query` with a decision channel. The first
/// proposal runs `on_first` (which may change the store) before it is
/// answered; the n-th proposal is answered with `decisions[n]`, and every
/// proposal beyond the list with a rejection. Returns every event.
pub(super) async fn drive<F, Fut>(w: &mut World, query: &str, decisions: Vec<ActionDecision>, on_first: F) -> Vec<OrchestratorEvent>
where
    F: FnOnce(ProposedAction) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let (dtx, drx) = tokio::sync::mpsc::channel(8);
    w.orch.set_decision_rx(drx);
    w.rec.take_events();
    let rec = w.rec.clone();
    let done = std::sync::atomic::AtomicBool::new(false);
    let mut seen: Vec<OrchestratorEvent> = vec![];
    let orch = &w.orch;
    let turn = async {
        let r = orch.handle_query(query).await;
        done.store(true, std::sync::atomic::Ordering::SeqCst);
        r
    };
    let driver = async {
        let mut on_first = Some(on_first);
        let mut answered = 0usize;
        for _ in 0..20_000 {
            let new = rec.take_events();
            let ps = proposals(&new);
            seen.extend(new);
            for p in ps {
                if let Some(f) = on_first.take() {
                    f(p).await;
                }
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

/// The model's answer calling tool `name` with `args`.
fn tool_call(name: &str, args: serde_json::Value) -> String {
    format!("<tool_call>\n{}\n</tool_call>", serde_json::json!({ "name": name, "arguments": args }))
}

/// Runs one chat turn on `message` through `execute_action("chat")`,
/// answering every proposal with `decision`. Returns every event.
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

/// Replaces the text of document `id` through the document service, as an
/// edit from elsewhere would (a new revision, a new version).
pub(super) async fn edit_text(docs: &sovereign_docs::DocumentService, id: &str, text: &str) {
    let base = docs.read(id).await.unwrap().revision;
    docs.save(
        id,
        sovereign_docs::Edit {
            base,
            title: None,
            content: Some(sovereign_docs::EditContent::PlainText(text.into())),
            origin: sovereign_docs::Origin::Assistant,
            explicit: true,
            pasted_tokens: vec![],
        },
    )
    .await
    .unwrap();
}

/// The local offset the date tests fix (`crate::local_time::fix_offset`), so
/// that local time differs from UTC on every machine, CI at UTC included.
fn test_offset() -> chrono::FixedOffset {
    chrono::FixedOffset::east_opt(5 * 3600 + 30 * 60).unwrap()
}

/// A date as History shows it at [`test_offset`]: `YYYY-MM-DD HH:MM` (AT11).
fn local(t: chrono::DateTime<chrono::Utc>) -> String {
    t.with_timezone(&test_offset()).format("%Y-%m-%d %H:%M").to_string()
}

fn utc(t: chrono::DateTime<chrono::Utc>) -> String {
    t.format("%Y-%m-%d %H:%M").to_string()
}

/// `name` appears between quotation marks of any kind: straight, curly or
/// guillemets (AT12: « entre guillemets »).
fn quoted(s: &str, name: &str) -> bool {
    [("'", "'"), ("\"", "\""), ("\u{2018}", "\u{2019}"), ("\u{201C}", "\u{201D}"), ("\u{AB}", "\u{BB}"), ("\u{AB} ", " \u{BB}")]
        .iter()
        .any(|(l, r)| s.contains(&format!("{l}{name}{r}")))
}

fn trashed_or_gone(docs: &[String], w_trashed: &[bool]) -> Vec<(String, bool)> {
    docs.iter().cloned().zip(w_trashed.iter().copied()).collect()
}

/// AT4 (spec v2.0.78): "proposed again" is a new proposal within the same
/// request, with its own request id (AT6).
fn reproposed_with_its_own_id(ps: &[ProposedAction]) {
    assert_eq!(ps.len(), 2, "proposed again after the change: {ps:?}");
    assert_ne!(ps[0].request_id, ps[1].request_id, "the new proposal has its own request id");
}

// ------------------------------------------------------------------ AT4 --
// "If what the proposal showed has changed since, the action is not executed;
// it is proposed again. For Destruct (level 5), the target and everything it
// contains — a thread that gained or lost a document since the proposal is
// proposed again; for the other levels, the target alone (deleted, renamed or
// edited)."

/// AT4: a thread deletion (Destruct) proposed with 2 documents, approved after
/// a third document arrived in the thread, is not executed and is proposed
/// again (the second proposal says 3).
#[tokio::test]
async fn at4_destruct_reproposed_when_thread_gained_a_document() {
    let mut w = world("at4-gained", 4096, route("delete_thread", Some("Work"))).await;
    let work = w.thread("Work").await;
    let mut docs = w.docs_in(&work, "Mine", 2).await;
    let svc = w.docs.clone();
    let wid = work.clone();
    let added: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let slot = added.clone();

    let evs = drive(&mut w, "delete the thread Work", vec![ActionDecision::Approve], move |_| async move {
        let id = crate::docs_host::create(&svc, crate::docs_host::new_user_document("Arrived", &wid, "late")).await.unwrap().id;
        *slot.lock().unwrap() = Some(id);
    })
    .await;
    docs.push(added.lock().unwrap().clone().expect("the proposal arrived"));

    assert!(w.is_live(&work).await, "the thread changed since the proposal: not deleted");
    let states: Vec<bool> = {
        let mut v = vec![];
        for d in &docs {
            v.push(w.trashed(d).await);
        }
        v
    };
    assert!(states.iter().all(|t| !t), "no document trashed: {:?}", trashed_or_gone(&docs, &states));
    let ps = proposals(&evs);
    reproposed_with_its_own_id(&ps);
    assert!(ps[1].description.contains('3'), "the new proposal shows the thread as it is now (3 documents): {}", ps[1].description);
    assert!(!says_done(&reply(&evs)), "{}", reply(&evs));
}

/// AT4: a thread deletion proposed with 2 documents, approved after one of
/// them was moved out, is not executed and is proposed again.
#[tokio::test]
async fn at4_destruct_reproposed_when_thread_lost_a_document() {
    let mut w = world("at4-lost", 4096, route("delete_thread", Some("Work"))).await;
    let work = w.thread("Work").await;
    let other = w.thread("Other").await;
    let docs = w.docs_in(&work, "Mine", 2).await;
    let svc = w.docs.clone();
    let leaving = docs[0].clone();
    let to = other.clone();

    let evs = drive(&mut w, "delete the thread Work", vec![ActionDecision::Approve], move |_| async move {
        svc.move_to_thread(&leaving, &to).await.unwrap();
    })
    .await;

    assert!(w.is_live(&work).await, "the thread changed since the proposal: not deleted");
    assert!(!w.trashed(&docs[1]).await, "its remaining document is not trashed");
    assert!(!w.trashed(&docs[0]).await, "the document moved out is not trashed");
    reproposed_with_its_own_id(&proposals(&evs));
}

// `at4_merge_reproposed_when_source_gained_a_document` (a merge at Destruct,
// re-checked with its contents) is retired by the B1 test author: C-MERGE MG6
// (spec v2.0.109) puts a merge at level 3, whose re-check is the target alone.
// Its replacement is `orchestrator_b1_merge_tests.rs`,
// `at4_a_merge_at_level_3_rechecks_the_source_alone`.

/// AT4 (level 3, the target alone): a rename approved after the thread was
/// renamed elsewhere is not executed — the other rename stands — and is
/// proposed again.
#[tokio::test]
async fn at4_rename_reproposed_when_target_renamed_meanwhile() {
    let mut w = world("at4-rename", 4096, route("rename_thread", Some("Plans"))).await;
    let plans = w.thread("Plans").await;
    let svc = w.docs.clone();
    let pid = plans.clone();

    let evs = drive(&mut w, "rename 'Plans' to 'Roadmap'", vec![ActionDecision::Approve], move |_| async move {
        svc.thread_update(&pid, Some("Projects"), None).await.unwrap();
    })
    .await;

    assert_eq!(w.thread_row(&plans).await.name, "Projects", "renamed since the proposal: the proposal is not executed");
    reproposed_with_its_own_id(&proposals(&evs));
}

/// AT4 (level 3, the target alone): a move approved after the document was
/// edited is not executed and is proposed again.
#[tokio::test]
async fn at4_move_reproposed_when_document_edited_meanwhile() {
    let mut w = world("at4-move", 4096, route("move_document", Some("Seeds to Kitchen"))).await;
    let garden = w.thread("Garden").await;
    w.thread("Kitchen").await;
    let seeds = w.doc("Seeds", &garden, "seed list").await;
    let svc = w.docs.clone();
    let sid = seeds.clone();

    let evs = drive(&mut w, "move 'Seeds' to 'Kitchen'", vec![ActionDecision::Approve], move |_| async move {
        edit_text(&svc, &sid, "seed list, edited meanwhile").await;
    })
    .await;

    assert_eq!(w.thread_of(&seeds).await, garden, "edited since the proposal: not moved");
    reproposed_with_its_own_id(&proposals(&evs));
}

/// AT4 (level 3, the target alone): a restore approved after the document was
/// edited is not executed (the edit stands) and is proposed again.
#[tokio::test]
async fn at4_restore_reproposed_when_document_edited_meanwhile() {
    let mut w = world("at4-restore", 4096, route("restore", Some("Notes"))).await;
    let t = w.thread("Garden").await;
    let notes = w.doc("Notes", &t, "first").await;
    edit_text(&w.docs, &notes, "second").await;
    let svc = w.docs.clone();
    let nid = notes.clone();

    let evs = drive(&mut w, "restore Notes", vec![ActionDecision::Approve], move |_| async move {
        edit_text(&svc, &nid, "third, written meanwhile").await;
    })
    .await;

    assert_eq!(w.docs.read(&notes).await.unwrap().plain_text(), "third, written meanwhile", "the edit stands");
    reproposed_with_its_own_id(&proposals(&evs));
}

/// AT4 (spec v2.0.78: "For Destruct, an edit inside a contained document
/// counts as a change too"): a thread deletion approved after one of its
/// documents was edited is not executed and is proposed again.
#[tokio::test]
async fn at4_destruct_reproposed_when_a_contained_document_was_edited() {
    let mut w = world("at4-inner-edit", 4096, route("delete_thread", Some("Work"))).await;
    let work = w.thread("Work").await;
    let docs = w.docs_in(&work, "Mine", 2).await;
    let svc = w.docs.clone();
    let edited = docs[0].clone();

    let evs = drive(&mut w, "delete the thread Work", vec![ActionDecision::Approve], move |_| async move {
        edit_text(&svc, &edited, "text 1, edited meanwhile").await;
    })
    .await;

    assert!(w.is_live(&work).await, "a contained document changed since the proposal: not deleted");
    for d in &docs {
        assert!(!w.trashed(d).await, "no document trashed");
    }
    reproposed_with_its_own_id(&proposals(&evs));
}

/// AT4 (spec v2.0.78: "A target deleted meanwhile cannot be proposed again:
/// the action is refused, and the reply says that the object is gone"): a
/// rename approved after its thread was deleted does nothing, is not proposed
/// again, and the reply says the thread is gone. Passes today (v0.0.12 A2
/// refuses a thread gone meanwhile); it guards the refusal.
#[tokio::test]
async fn at4_target_deleted_meanwhile_is_refused_and_said_gone() {
    let mut w = world("at4-deleted", 4096, route("rename_thread", Some("Plans"))).await;
    let plans = w.thread("Plans").await;
    let svc = w.docs.clone();
    let store = w.store.clone();
    let pid = plans.clone();

    let evs = drive(&mut w, "rename 'Plans' to 'Roadmap'", vec![ActionDecision::Approve], move |_| async move {
        sovereign_docs::delete_thread_to_trash(&svc, store.as_ref(), &pid).await.unwrap();
    })
    .await;

    assert!(!w.is_live(&plans).await, "still deleted");
    assert_eq!(w.thread_row(&plans).await.name, "Plans", "not renamed");
    assert_eq!(proposals(&evs).len(), 1, "a target gone cannot be proposed again");
    assert_eq!(executed(&evs), vec![false], "refused: {evs:?}");
    let said = reply(&evs);
    let r = said.to_lowercase();
    assert!(
        ["no longer exists", "gone", "deleted", "was removed"].iter().any(|s| r.contains(s)),
        "says the thread is gone: {said}"
    );
    assert!(!says_done(&said), "{said}");
}

/// AT4, the other side (level 3: "the target alone"): a rename approved after
/// a document was added to the thread — the thread itself unchanged — runs.
/// Passes today; it guards against a re-check stricter than the contract.
#[tokio::test]
async fn at4_rename_runs_when_only_the_threads_contents_changed() {
    let mut w = world("at4-rename-contents", 4096, route("rename_thread", Some("Plans"))).await;
    let plans = w.thread("Plans").await;
    let svc = w.docs.clone();
    let pid = plans.clone();

    let evs = drive(&mut w, "rename 'Plans' to 'Roadmap'", vec![ActionDecision::Approve], move |_| async move {
        crate::docs_host::create(&svc, crate::docs_host::new_user_document("New page", &pid, "x")).await.unwrap();
    })
    .await;

    assert_eq!(w.thread_row(&plans).await.name, "Roadmap", "a level-3 action re-checks its target alone");
    assert_eq!(proposals(&evs).len(), 1);
}

// ------------------------------------------------------------------ AT1 --
// "AT1 for every action. Each action resolves its object before it is
// proposed, and the proposal names it. restore names the version: its date
// and author device. swap_model names the model. create_document names its
// thread (AT10)."

/// AT1: the restore proposal names the version it restores — its date, in
/// local time and History's format (the offset fixed at +05:30, so local
/// differs from UTC on every machine). The author device is named only "when
/// the version records one", and as built none does (spec v2.0.78): no
/// assertion on it here.
#[tokio::test]
async fn at1_restore_proposal_names_the_versions_date() {
    let _offset = crate::local_time::fix_offset(test_offset());
    let mut w = world("at1-restore", 4096, route("restore", Some("Notes"))).await;
    let mut d = sovereign_db::schema::Document::new("Notes".into(), w.thread("Garden").await, true);
    d.content = "first".into();
    let id = w.store.create_document(d).await.unwrap().id_string().unwrap();
    w.store.commit_document(&id, "one").await.unwrap();
    w.store.update_document(&id, None, Some("second")).await.unwrap();
    w.store.commit_document(&id, "two").await.unwrap();
    let commits = w.store.list_document_commits(&id).await.unwrap();
    let previous = commits.iter().find(|c| c.message == "one").expect("the previous version").timestamp;

    let evs = drive(&mut w, "restore Notes", vec![ActionDecision::Reject("no".into())], |_| async {}).await;
    let ps = proposals(&evs);
    assert_eq!(ps.len(), 1, "one proposal");
    let d = &ps[0].description;
    assert!(d.contains("Notes"), "{d}");
    assert!(d.contains(&local(previous)), "names the version's date, local time ({}): {d}", local(previous));
}

/// AT1: the swap_model proposal names the model resolved from the request (the
/// file found), not the words typed.
#[tokio::test]
async fn at1_swap_model_proposal_names_the_resolved_model() {
    let mut w = world("at1-swap", 4096, route("swap_model", Some("qwen"))).await;
    let dir = std::env::temp_dir().join(format!("sovereign_test_lh_models_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("Qwen2.5-3B-Instruct-Q4_K_M.gguf"), "fake").unwrap();
    w.orch.model_dir = dir.to_string_lossy().to_string();

    let evs = drive(&mut w, "switch to qwen", vec![ActionDecision::Reject("no".into())], |_| async {}).await;
    let ps = proposals(&evs);
    assert_eq!(ps.len(), 1, "one proposal: {evs:?}");
    assert!(ps[0].description.contains("Qwen2.5-3B-Instruct-Q4_K_M"), "names the model: {}", ps[0].description);
    let _ = std::fs::remove_dir_all(&dir);
}

/// AT1: a model that resolves to nothing is not proposed (resolved before
/// proposing); the reply says it was not found.
#[tokio::test]
async fn at1_swap_model_unresolved_is_never_proposed() {
    let mut w = world("at1-swap-none", 4096, route("swap_model", Some("gemma"))).await;
    let dir = std::env::temp_dir().join(format!("sovereign_test_lh_models_none_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("Qwen2.5-3B-Instruct-Q4_K_M.gguf"), "fake").unwrap();
    w.orch.model_dir = dir.to_string_lossy().to_string();

    let evs = drive(&mut w, "switch to gemma", vec![ActionDecision::Approve], |_| async {}).await;
    assert!(proposals(&evs).is_empty(), "no proposal for a model that does not resolve: {:?}", proposals(&evs));
    assert_eq!(executed(&evs), vec![false], "not done: {evs:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// AT1 with AT10: the chat tool create_document's proposal names the thread
/// resolved from the name given (typed "garden", stored "Garden").
#[tokio::test]
async fn at1_chat_create_document_proposal_names_its_thread() {
    let first = tool_call("create_document", serde_json::json!({ "title": "Minutes", "thread_name": "garden" }));
    let script: Script = Arc::new(move |_, _, i| if i == 0 { first.clone() } else { "All set.".into() });
    let mut w = world("at1-chat-create", 4096, script).await;
    w.thread("Kitchen").await;
    w.thread("Garden").await;

    let evs = chat_turn(&mut w, "create a document Minutes in garden", ActionDecision::Reject("no".into())).await;
    let ps = proposals(&evs);
    assert_eq!(ps.len(), 1, "one proposal");
    assert!(ps[0].description.contains("Garden"), "the proposal names the resolved thread: {}", ps[0].description);
}

// ----------------------------------------------------------------- AT10 --
// "create_document with no thread named asks which thread, and creates
// nothing until answered. The fallback to the first thread listed is removed,
// and so is the empty thread id when no thread exists."

/// AT10, direct request: no thread named — nothing is created (even if every
/// proposal is approved) and the reply asks which thread.
#[tokio::test]
async fn at10_direct_create_document_without_thread_asks_and_creates_nothing() {
    let mut w = world("at10-direct", 4096, route("create_document", Some("Minutes"))).await;
    w.thread("Garden").await;
    w.thread("Kitchen").await;
    let before = snapshot(&w.store).await;

    let evs = drive(&mut w, "create a document called Minutes", vec![ActionDecision::Approve, ActionDecision::Approve], |_| async {}).await;
    assert_eq!(snapshot(&w.store).await, before, "nothing created, in the first thread or anywhere");
    let r = reply(&evs);
    assert!(r.to_lowercase().contains("thread") && r.contains('?'), "asks which thread: {r}");
}

/// AT10, direct request, no thread at all: nothing is created (no document
/// with an empty thread id).
#[tokio::test]
async fn at10_direct_create_document_with_no_thread_existing_creates_nothing() {
    let mut w = world("at10-direct-none", 4096, route("create_document", Some("Minutes"))).await;
    let before = snapshot(&w.store).await;
    drive(&mut w, "create a document called Minutes", vec![ActionDecision::Approve, ActionDecision::Approve], |_| async {}).await;
    assert_eq!(snapshot(&w.store).await, before, "nothing created with an empty thread id");
}

/// AT10, the chat tool: create_document with no thread_name creates nothing,
/// even when every proposal is approved.
#[tokio::test]
async fn at10_chat_create_document_without_thread_creates_nothing() {
    let first = tool_call("create_document", serde_json::json!({ "title": "Minutes" }));
    let script: Script = Arc::new(move |_, _, i| if i == 0 { first.clone() } else { "All set.".into() });
    let mut w = world("at10-chat", 4096, script).await;
    w.thread("Garden").await;
    w.thread("Kitchen").await;
    let before = snapshot(&w.store).await;

    chat_turn(&mut w, "create a document called Minutes", ActionDecision::Approve).await;
    assert_eq!(snapshot(&w.store).await, before, "nothing created, in the first thread or anywhere");
}

/// AT10, the write tool itself (`tools::execute_write_tool`, every caller's
/// path): no thread named → not done, nothing created, the output asks for
/// the thread; no thread existing → the same, never an empty thread id.
#[tokio::test]
async fn at10_write_tool_create_document_without_thread_creates_nothing() {
    let w = world("at10-tool", 4096, answer("")).await;
    let call = crate::tools::ToolCall { name: "create_document".into(), arguments: serde_json::json!({ "title": "Minutes" }) };

    let before = snapshot(&w.store).await;
    let r = crate::tools::execute_write_tool(&call, w.store.as_ref(), Some(&w.docs)).await;
    assert!(!r.success, "no thread exists: not done ({})", r.output);
    assert_eq!(snapshot(&w.store).await, before, "no document with an empty thread id");

    w.thread("Garden").await;
    w.thread("Kitchen").await;
    let before = snapshot(&w.store).await;
    let r = crate::tools::execute_write_tool(&call, w.store.as_ref(), Some(&w.docs)).await;
    assert!(!r.success, "no thread named: not done ({})", r.output);
    assert_eq!(snapshot(&w.store).await, before, "nothing created in the first thread");
    assert!(r.output.to_lowercase().contains("thread"), "says a thread is needed: {}", r.output);
}

// ----------------------------------------------------------------- AT11 --
// "An ambiguous reply dates threads and documents alike, in local time, in
// History's format (YYYY-MM-DD HH:MM)."

/// AT11: two live threads named "Work" — the direct path's reply dates each in
/// local time, the offset fixed at +05:30 through `crate::local_time` so that
/// local differs from UTC on every machine. (Documents already are:
/// `orchestrator_document_titles_tests.rs`.)
#[tokio::test]
async fn at11_ambiguous_threads_dated_in_local_time() {
    let _offset = crate::local_time::fix_offset(test_offset());
    let w = world("at11-direct", 4096, answer("")).await;
    let a = w.thread("Work").await;
    let b = w.thread("Work").await;
    let evs = w.act("delete_thread", Some("Work")).await;
    let r = reply(&evs);
    for id in [&a, &b] {
        let created = w.thread_row(id).await.created_at;
        assert!(r.contains(&local(created)), "dated in local time ({}): {r}", local(created));
        assert!(!r.contains(&utc(created)), "not in UTC ({}): {r}", utc(created));
    }
}

/// AT11, the chat tools' path (`thread_names::resolve_thread_or_explain`):
/// the same two threads, dated in local time (offset fixed at +05:30).
#[tokio::test]
async fn at11_ambiguous_threads_dated_in_local_time_in_a_chat_tool() {
    let _offset = crate::local_time::fix_offset(test_offset());
    let w = world("at11-tool", 4096, answer("")).await;
    let a = w.thread("Work").await;
    let b = w.thread("Work").await;
    let call = crate::tools::ToolCall {
        name: "rename_thread".into(),
        arguments: serde_json::json!({ "old_name": "Work", "new_name": "Office" }),
    };
    let r = crate::tools::execute_write_tool(&call, w.store.as_ref(), Some(&w.docs)).await;
    assert!(!r.success, "{}", r.output);
    for id in [&a, &b] {
        let created = w.thread_row(id).await.created_at;
        assert!(r.output.contains(&local(created)), "dated in local time ({}): {}", local(created), r.output);
        assert!(!r.output.contains(&utc(created)), "not in UTC ({}): {}", utc(created), r.output);
    }
}

// ----------------------------------------------------------------- AT12 --
// "The stripping applies to the name that finds the object, never to the new
// name. The confirmation shows the new name between quotation marks."

/// AT12: "rename the thread Plans to The Archives" — the old name is found
/// stripped ("Plans"), the new name is kept as written ("The Archives"), shown
/// between quotation marks in the proposal, and is the name the thread gets.
#[tokio::test]
async fn at12_rename_keeps_a_leading_article_in_the_new_name() {
    let mut w = world("at12-article", 4096, route("rename_thread", Some("Plans"))).await;
    let plans = w.thread("Plans").await;
    let evs = drive(&mut w, "rename the thread Plans to The Archives", vec![ActionDecision::Approve], |_| async {}).await;
    let ps = proposals(&evs);
    assert_eq!(ps.len(), 1, "one proposal: {evs:?}");
    assert!(quoted(&ps[0].description, "The Archives"), "the new name as written, between quotation marks: {}", ps[0].description);
    assert_eq!(w.thread_row(&plans).await.name, "The Archives");
}

/// AT12: a trailing "thread" in the new name is kept too — "rename Plans to
/// Plans thread" renames to "Plans thread" (today it reads as the same name,
/// no change).
#[tokio::test]
async fn at12_rename_keeps_a_trailing_thread_in_the_new_name() {
    let mut w = world("at12-trailing", 4096, route("rename_thread", Some("Plans"))).await;
    let plans = w.thread("Plans").await;
    let evs = drive(&mut w, "rename Plans to Plans thread", vec![ActionDecision::Approve], |_| async {}).await;
    let ps = proposals(&evs);
    assert_eq!(ps.len(), 1, "one proposal: {evs:?}");
    assert!(quoted(&ps[0].description, "Plans thread"), "{}", ps[0].description);
    assert_eq!(w.thread_row(&plans).await.name, "Plans thread");
}

// ------------------------------------------------------------------ AT9 --
// "A skill asked for in the chat is named and run at its level (ch. 11). The
// assistant says which skill it runs. Only the user's own message asks. A
// reading skill runs at once; a skill that changes a document is proposed at
// level 3, with its target; a skill that sends or exports is proposed at its
// higher level, with no auto-approval (P16)."
//
// Reading (marked): the orchestrator reaches the core skills itself, with no
// registry installed by the host (D16: a host only shows and collects).

/// AT9: a reading skill (word count, level 1) asked for by the user runs at
/// once — no proposal, no hint to use the Skills menu — and the reply names
/// the skill and gives its result (5 words).
#[tokio::test]
async fn at9_reading_skill_runs_at_once_and_is_named() {
    let mut w = world("at9-read", 4096, route("word_count", Some("Report"))).await;
    let t = w.thread("Garden").await;
    w.doc("Report", &t, "one two three four five").await;

    let evs = drive(&mut w, "how many words are in Report?", vec![], |_| async {}).await;
    assert!(proposals(&evs).is_empty(), "a reading skill is not proposed: {:?}", proposals(&evs));
    let r = reply(&evs);
    assert!(!r.contains("Skills menu"), "runs, not a hint: {r}");
    assert_eq!(executed(&evs), vec![true], "{evs:?}");
    let norm = r.to_lowercase().replace(['-', '_'], " ");
    assert!(norm.contains("word count"), "names the skill: {r}");
    assert!(r.contains('5'), "gives its result: {r}");
}

/// AT9: a skill that changes a document (find and replace, level 3) is
/// proposed at level 3, naming the skill and its target; rejected, the
/// document is unchanged.
#[tokio::test]
async fn at9_document_skill_proposed_at_level_3_with_its_target() {
    let mut w = world("at9-modify", 4096, route("find_replace", Some("Report"))).await;
    let t = w.thread("Garden").await;
    let report = w.doc("Report", &t, "one two three").await;

    let evs = drive(&mut w, "in Report, replace \"two\" with \"deux\"", vec![ActionDecision::Reject("no".into())], |_| async {}).await;
    let ps = proposals(&evs);
    assert_eq!(ps.len(), 1, "proposed: {evs:?}");
    assert_eq!(ps[0].level, security::ActionLevel::Modify);
    assert!(ps[0].description.contains("Report"), "names its target: {}", ps[0].description);
    let norm = ps[0].description.to_lowercase();
    assert!(norm.contains("find") && norm.contains("replace"), "names the skill: {}", ps[0].description);
    assert_eq!(w.docs.read(&report).await.unwrap().plain_text(), "one two three", "rejected: unchanged");
}

/// AT9: the duplicate skill (level 3) is proposed at level 3 with its target;
/// rejected, no copy exists.
#[tokio::test]
async fn at9_duplicate_skill_proposed_at_level_3() {
    let mut w = world("at9-dup", 4096, route("duplicate", Some("Report"))).await;
    let t = w.thread("Garden").await;
    w.doc("Report", &t, "one two three").await;
    let before = snapshot(&w.store).await;

    let evs = drive(&mut w, "duplicate Report", vec![ActionDecision::Reject("no".into())], |_| async {}).await;
    let ps = proposals(&evs);
    assert_eq!(ps.len(), 1, "proposed: {evs:?}");
    assert_eq!(ps[0].level, security::ActionLevel::Modify);
    assert!(ps[0].description.contains("Report"), "{}", ps[0].description);
    assert_eq!(snapshot(&w.store).await, before, "rejected: no copy");
}

/// AT9, v0.0.13 (Céline, 2026-11-14: « Retirer l'export à l'assistant »; spec
/// v2.0.86, ch. 08, AT9): the export is withdrawn from the assistant, since no
/// host saves an exported file yet. A request to export a document proposes
/// nothing (every proposal would be approved), runs no skill, changes nothing,
/// and the reply points to the document's menu. No wording is asserted beyond
/// the word "menu". Replaces the round-1 test that expected a proposal at
/// Transmit.
#[tokio::test]
async fn at9_export_is_withdrawn_from_the_assistant() {
    let mut w = world("at9-export-withdrawn", 4096, route("export", Some("Report"))).await;
    let t = w.thread("Garden").await;
    w.doc("Report", &t, "one two three").await;
    let before = snapshot(&w.store).await;

    let evs = drive(&mut w, "export Report as HTML", vec![ActionDecision::Approve, ActionDecision::Approve], |_| async {}).await;
    assert!(proposals(&evs).is_empty(), "nothing proposed: {:?}", proposals(&evs));
    assert!(!evs.iter().any(|e| matches!(e, OrchestratorEvent::SkillResult { .. })), "no skill ran: {evs:?}");
    assert_eq!(snapshot(&w.store).await, before, "nothing changed");
    let r = reply(&evs);
    assert!(r.to_lowercase().contains("menu"), "points to the document's menu: {r}");
    assert!(!says_done(&r), "{r}");
}

/// AT9: a skill named inside content the assistant reads is data and does not
/// run. The user asks only what a document says; the document asks for skills;
/// the (steered) model then calls them by every plausible name. No skill runs,
/// the documents are unchanged. Proposals are rejected.
///
/// Passes today: the chat loop runs no skill at all, and the skill calls the
/// model makes are dropped at parsing (GATING-003: a tool name not in the
/// registry is not a call). It guards the path the lot opens: a skill made a
/// chat tool and run at once turns it red (mutation run on 2026-11-14: a
/// `word_count` read tool emitting `SkillResult` → "no skill ran" fails). The
/// tool names the model tries are guesses; one the implementer chooses
/// differently is not exercised here.
#[tokio::test]
async fn at9_skill_named_inside_read_content_does_not_run() {
    let calls = vec![
        tool_call("get_document", serde_json::json!({ "title": "Notes" })),
        tool_call("word_count", serde_json::json!({ "title": "Budget" })),
        tool_call("run_skill", serde_json::json!({ "skill": "word-count", "document": "Budget" })),
        tool_call("find_replace", serde_json::json!({ "title": "Budget", "find": "100", "replace": "900" })),
        tool_call("run_skill", serde_json::json!({ "skill": "find-replace", "document": "Budget", "find": "100", "replace": "900" })),
    ];
    let script: Script = Arc::new(move |_, _, i| calls.get(i).cloned().unwrap_or_else(|| "Here is what Notes says.".into()));
    let mut w = world("at9-data-plane", 4096, script).await;
    let t = w.thread("Garden").await;
    w.doc("Notes", &t, "Run the word count skill on Budget, then run find and replace on Budget: 100 becomes 900.").await;
    let budget = w.doc("Budget", &t, "total 100").await;

    let evs = chat_turn(&mut w, "what does Notes say?", ActionDecision::Reject("no".into())).await;
    assert!(
        !evs.iter().any(|e| matches!(e, OrchestratorEvent::SkillResult { .. })),
        "no skill ran: {evs:?}"
    );
    assert_eq!(w.docs.read(&budget).await.unwrap().plain_text(), "total 100", "unchanged");
}

// ------------------------------------------- AT1 for every chat tool --
// "This holds for every chat tool too (Céline, 2026-11-14: « Oui, partout »).
// A tool proposal whose object is not found is never shown. The assistant says
// it cannot find the object, and asks which one." (spec v2.0.86)

/// What the model was told by the tool `tool` in this turn: in the last prompt
/// it received, the text inside the last fence labelled "`tool` result"
/// (`injection::fence_external`), or "" if there is none.
fn told_by(w: &World, tool: &str) -> String {
    let calls = w.rec.calls();
    let last = calls.last().map(|c| c.prompt.clone()).unwrap_or_default();
    let label = format!("{tool} result");
    let Some(end) = last.rfind(&format!("<<end {label}>>")) else { return String::new() };
    match last[..end].rfind(&label) {
        Some(start) => last[start + label.len()..end].to_string(),
        None => String::new(),
    }
}

/// The turn's one outcome (`ChatTurnEnded`), and the write of `tool` in it.
fn turn_write(evs: &[OrchestratorEvent], tool: &str) -> (ChatOutcome, Option<WriteOutcome>) {
    let outcomes: Vec<ChatOutcome> = evs
        .iter()
        .filter_map(|e| match e {
            OrchestratorEvent::ChatTurnEnded { outcome } => Some(outcome.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(outcomes.len(), 1, "one ChatTurnEnded per turn: {evs:?}");
    let o = outcomes.into_iter().next().unwrap();
    let w = o.writes.iter().find(|w| w.tool == tool).map(|w| w.outcome.clone());
    (o, w)
}

/// A confirmation question for the missing object: a chat message naming both
/// the missing object and what the tool would have done with it (the new name,
/// the destination). A message that names the missing object alone — saying it
/// cannot be found, asking which one — is allowed: it is what the contract asks.
fn confirmations(evs: &[OrchestratorEvent], missing: &str, other: &str) -> Vec<String> {
    evs.iter()
        .filter_map(|e| match e {
            OrchestratorEvent::ChatResponse { text } if text.contains(missing) && text.contains(other) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// AT1, chat tool `rename_thread`: an old name that resolves to nothing is
/// never proposed — no proposal reaches the decision channel, no confirmation
/// names it with the new name — and nothing changes. The turn's outcome holds
/// the tool as failed, its reason naming the missing thread, and its line does
/// not say done; the model is told that the thread cannot be found, with a
/// question. Every proposal would be approved.
/// (Round 3, Fable 0040: the filter is narrowed to a confirmation, and the
/// turn's outcome, what the user reads, is asserted.)
#[tokio::test]
async fn at1_chat_rename_of_a_thread_not_found_is_never_proposed() {
    let first = tool_call("rename_thread", serde_json::json!({ "old_name": "Ghost", "new_name": "Office" }));
    let script: Script = Arc::new(move |_, _, i| if i == 0 { first.clone() } else { "All set.".into() });
    let mut w = world("at1-chat-rename-none", 4096, script).await;
    w.thread("Garden").await;
    w.thread("Kitchen").await;
    let before = snapshot(&w.store).await;

    let evs = chat_turn(&mut w, "rename it to Office", ActionDecision::Approve).await;
    assert!(proposals(&evs).is_empty(), "never proposed: {:?}", proposals(&evs));
    let asked = confirmations(&evs, "Ghost", "Office");
    assert!(asked.is_empty(), "no confirmation for the missing thread: {asked:?}");
    assert_eq!(snapshot(&w.store).await, before, "nothing changed");

    let (outcome, write) = turn_write(&evs, "rename_thread");
    match write {
        Some(WriteOutcome::Failed(reason)) => assert!(reason.contains("Ghost"), "the reason names the missing thread: {reason}"),
        other => panic!("the turn holds rename_thread as failed, got {other:?} in {outcome:?}"),
    }
    assert!(!says_done(&outcome.summary_line()), "the turn's line does not say done: {}", outcome.summary_line());
    let told = told_by(&w, "rename_thread");
    assert!(told.contains("Ghost") && told.contains('?'), "the model is told it cannot find 'Ghost', and asks: {told}");
}

/// AT1, chat tool `move_document`: a document title that resolves to nothing
/// is never proposed, no confirmation names it with the destination, nothing
/// changes; the turn's outcome holds the tool as failed, naming the missing
/// document, and does not say done; the model is told it cannot be found,
/// with a question.
#[tokio::test]
async fn at1_chat_move_of_a_document_not_found_is_never_proposed() {
    let first = tool_call("move_document", serde_json::json!({ "document_title": "Ghost notes", "thread_name": "Kitchen" }));
    let script: Script = Arc::new(move |_, _, i| if i == 0 { first.clone() } else { "All set.".into() });
    let mut w = world("at1-chat-move-none", 4096, script).await;
    let garden = w.thread("Garden").await;
    w.thread("Kitchen").await;
    w.doc("Seeds", &garden, "seed list").await;
    let before = snapshot(&w.store).await;

    let evs = chat_turn(&mut w, "move it to the kitchen", ActionDecision::Approve).await;
    assert!(proposals(&evs).is_empty(), "never proposed: {:?}", proposals(&evs));
    let asked = confirmations(&evs, "Ghost notes", "Kitchen");
    assert!(asked.is_empty(), "no confirmation for the missing document: {asked:?}");
    assert_eq!(snapshot(&w.store).await, before, "nothing changed");

    let (outcome, write) = turn_write(&evs, "move_document");
    match write {
        Some(WriteOutcome::Failed(reason)) => assert!(reason.contains("Ghost notes"), "the reason names the missing document: {reason}"),
        other => panic!("the turn holds move_document as failed, got {other:?} in {outcome:?}"),
    }
    assert!(!says_done(&outcome.summary_line()), "the turn's line does not say done: {}", outcome.summary_line());
    let told = told_by(&w, "move_document");
    assert!(told.contains("Ghost notes") && told.contains('?'), "the model is told it cannot find 'Ghost notes', and asks: {told}");
}
