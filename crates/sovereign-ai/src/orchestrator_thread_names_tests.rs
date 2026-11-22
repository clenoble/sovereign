//! v0.0.12 item 6 — **thread actions by exact name** (A1, A2, A3;
//! `doc/plans/V0012-lot-assistant.md`; cloud review v0.0.11, finding 7:
//! "Delete thread 'Work'" could trash every document of "Work Archive").
//!
//! - **A1** — a thread named in an assistant action resolves to a live thread
//!   whose name equals the given one, ignoring case and surrounding spaces, and
//!   to nothing else. No exact match: not done, the reply lists the close names
//!   (at most 5) and asks; none at all: it says no thread has that name. Two
//!   live threads with the same exact name: ambiguous, not done, both listed
//!   with their number of documents. Every caller (the orchestrator's thread
//!   actions, the tool layer, `docs_host`) resolves this way. The scan that no
//!   production caller uses the single-match lookup is
//!   `sovereign-docs/tests/a1_one_thread_resolution.rs`.
//! - **A2** — the thread is resolved before the approval prompt; the prompt
//!   names it and says what follows; after approval the action runs on the
//!   resolved id, never on a second lookup of the name.
//! - **A3** — the reply names what was done, for every thread action.
//!
//! "The reply" is what the user is told: chat messages, refusals and the
//! action's executed event (`assistant_harness::reply`, read through `Debug`
//! so it holds wherever the implementation puts the sentence). "Not done" is
//! read from the store (nothing changed) and from `ActionExecuted.success`.
//!
//! Written before the code by another author; the implementer does not edit
//! this file. Compiles against today's API (plus the harness's test-only
//! fake model).

use super::assistant_harness::*;
use super::*;

// ------------------------------------------------------------------ A1 --

/// A1: "Work" never resolves to "Work Archive". The review's case: only
/// "Work Archive" exists, the user deletes "Work" — nothing is deleted or
/// trashed, and the reply offers "Work Archive".
#[tokio::test]
async fn a1_delete_work_never_acts_on_work_archive() {
    let w = world("a1-del-archive", 4096, answer("")).await;
    let archive = w.thread("Work Archive").await;
    let docs = w.docs_in(&archive, "Archived", 3).await;

    let evs = w.act("delete_thread", Some("Work")).await;
    assert_eq!(executed(&evs), vec![false], "no exact match: not done");
    assert!(w.is_live(&archive).await, "Work Archive is not deleted");
    for d in &docs {
        assert!(!w.trashed(d).await, "no document of Work Archive goes to the trash");
    }
    assert!(reply(&evs).contains("Work Archive"), "the reply offers the close name: {}", reply(&evs));
}

/// A1: with both threads present, every spelling of "Work" (case, spaces)
/// acts on "Work" and never on "Work Archive". Each spelling in a fresh store,
/// with "Work Archive" created first.
#[tokio::test]
async fn a1_case_and_space_variants_resolve_only_to_the_exact_name() {
    for (i, typed) in ["Work", "work", "  WORK  ", "wOrK "].iter().enumerate() {
        let w = world(&format!("a1-variants-{i}"), 4096, answer("")).await;
        let archive = w.thread("Work Archive").await;
        let archived = w.docs_in(&archive, "Archived", 3).await;
        let work = w.thread("Work").await;
        let mine = w.docs_in(&work, "Mine", 2).await;

        let evs = w.act("delete_thread", Some(typed)).await;
        assert_eq!(executed(&evs), vec![true], "{typed:?}: Work is deleted");
        assert!(!w.is_live(&work).await, "{typed:?}: Work is deleted");
        for d in &mine {
            assert!(w.trashed(d).await, "{typed:?}: Work's documents go to the trash");
        }
        assert!(w.is_live(&archive).await, "{typed:?}: Work Archive stays");
        for d in &archived {
            assert!(!w.trashed(d).await, "{typed:?}: Work Archive's documents stay");
        }
    }
}

/// A1, the rename: "work to Office" renames "Work", not "Work Archive".
#[tokio::test]
async fn a1_rename_acts_only_on_the_exact_name() {
    let w = world("a1-rename", 4096, answer("")).await;
    let archive = w.thread("Work Archive").await;
    let work = w.thread("Work").await;
    let evs = w.act("rename_thread", Some("work to Office")).await;
    assert_eq!(executed(&evs), vec![true]);
    assert_eq!(w.thread_row(&work).await.name, "Office");
    assert_eq!(w.thread_row(&archive).await.name, "Work Archive");

    // Only "Work Archive" left (by name): renaming "Work" is not done.
    let w = world("a1-rename-none", 4096, answer("")).await;
    let archive = w.thread("Work Archive").await;
    let evs = w.act("rename_thread", Some("Work to Office")).await;
    assert_eq!(executed(&evs), vec![false], "no exact match: not done");
    assert_eq!(w.thread_row(&archive).await.name, "Work Archive", "Work Archive is not renamed");
    assert!(reply(&evs).contains("Work Archive"), "{}", reply(&evs));
}

/// A1, the target thread of a document action: "Seeds to Work" never moves
/// Seeds into "Work Archive".
#[tokio::test]
async fn a1_move_never_targets_a_thread_with_a_longer_name() {
    let w = world("a1-move", 4096, answer("")).await;
    let garden = w.thread("Garden").await;
    let archive = w.thread("Work Archive").await;
    let seeds = w.doc("Seeds", &garden, "seed list").await;

    let evs = w.act("move_document", Some("Seeds to Work")).await;
    assert_eq!(executed(&evs), vec![false], "no thread named Work: not done");
    assert_eq!(w.thread_of(&seeds).await, garden, "Seeds stays in Garden");
    assert!(reply(&evs).contains("Work Archive"), "{}", reply(&evs));

    let work = w.thread("Work").await;
    let evs = w.act("move_document", Some("Seeds to work")).await;
    assert_eq!(executed(&evs), vec![true]);
    assert_eq!(w.thread_of(&seeds).await, work, "moved into Work, not Work Archive");
    let _ = archive;
}

/// A1, merge (both names) and split: no exact match, nothing merged, nothing
/// created.
#[tokio::test]
async fn a1_merge_and_split_need_exact_names() {
    let w = world("a1-merge-split", 4096, answer("")).await;
    let archive = w.thread("Work Archive").await;
    let kitchen = w.thread("Kitchen").await;
    let pans = w.doc("Pans", &kitchen, "pans").await;
    let before = snapshot(&w.store).await;

    // "Work to Kitchen": Kitchen merged into "Work", which does not exist.
    let evs = w.act("merge_threads", Some("Work to Kitchen")).await;
    assert_eq!(executed(&evs), vec![false], "merge into a missing Work: not done");
    assert!(w.is_live(&kitchen).await);
    assert_eq!(w.thread_of(&pans).await, kitchen, "Pans not moved into Work Archive");
    // "Kitchen to Work": "Work" as the source — Work Archive is not merged away.
    let evs = w.act("merge_threads", Some("Kitchen to Work")).await;
    assert_eq!(executed(&evs), vec![false], "merge from a missing Work: not done");
    assert!(w.is_live(&archive).await, "Work Archive not merged away");

    let evs = w.act("split_thread", Some("Work to Work 2")).await;
    assert_eq!(executed(&evs), vec![false], "split of a missing Work: not done");
    assert_eq!(snapshot(&w.store).await, before, "nothing changed in the store");
}

/// A1, milestones: "Launch on Work" never lands on "Work Archive"; listing
/// "Work"'s milestones never lists "Work Archive"'s.
#[tokio::test]
async fn a1_milestones_need_an_exact_thread_name() {
    let w = world("a1-milestones", 4096, answer("")).await;
    let archive = w.thread("Work Archive").await;
    let evs = w.act("create_milestone", Some("Launch on Work")).await;
    assert_eq!(executed(&evs), vec![false], "no thread named Work: not done");
    assert!(w.store.list_milestones(&archive).await.unwrap().is_empty(), "no milestone on Work Archive");

    let evs = w.act("list_milestones", Some("Work")).await;
    assert_eq!(executed(&evs), vec![false], "no thread named Work: nothing listed");
    assert!(
        !evs.iter().any(|e| matches!(e, OrchestratorEvent::MilestonesListed { thread_id, .. } if *thread_id == archive)),
        "Work Archive's milestones are not listed for Work"
    );
}

/// A1: the close names are listed, at most 5, and the reply asks which one
/// is meant. Seven threads share the word "Work"; none is named "Work".
#[tokio::test]
async fn a1_no_exact_match_lists_at_most_five_close_names_and_asks() {
    let w = world("a1-close-names", 4096, answer("")).await;
    let names = ["Work Archive", "Work 2025", "Old Work", "Work notes", "Team Work", "Work log", "Work ideas"];
    for n in names {
        w.thread(n).await;
    }
    let before = snapshot(&w.store).await;
    let evs = w.act("delete_thread", Some("Work")).await;
    assert_eq!(executed(&evs), vec![false]);
    assert_eq!(snapshot(&w.store).await, before, "nothing deleted");
    let r = reply(&evs);
    let listed = names.iter().filter(|n| r.contains(*n)).count();
    assert!((1..=5).contains(&listed), "1 to 5 close names listed, got {listed}: {r}");
    // [choice] read as a question to the user: the reply asks which one.
    assert!(r.to_lowercase().contains("which"), "the reply asks which one is meant: {r}");
}

/// A1: with no candidate at all, the reply says no thread has that name.
#[tokio::test]
async fn a1_no_candidate_says_no_thread_has_that_name() {
    let w = world("a1-no-candidate", 4096, answer("")).await;
    w.thread("Garden").await;
    let evs = w.act("delete_thread", Some("Submarine")).await;
    assert_eq!(executed(&evs), vec![false]);
    let r = reply(&evs);
    assert!(r.to_lowercase().contains("no thread"), "says no thread has that name: {r}");
    assert!(!r.contains("Garden"), "no unrelated thread offered: {r}");
}

/// A1: two live threads with the same exact name are ambiguous — not done,
/// both listed with their number of documents.
#[tokio::test]
async fn a1_two_live_threads_with_the_same_name_are_ambiguous() {
    let w = world("a1-ambiguous", 4096, answer("")).await;
    let first = w.thread("Work").await;
    let a = w.docs_in(&first, "First", 2).await;
    let second = w.thread("Work").await;
    let b = w.docs_in(&second, "Second", 3).await;

    for action in ["delete_thread", "rename_thread", "split_thread"] {
        let target = if action == "delete_thread" { "Work" } else { "Work to Office" };
        let before = snapshot(&w.store).await;
        let evs = w.act(action, Some(target)).await;
        assert_eq!(executed(&evs), vec![false], "{action}: ambiguous, not done");
        assert_eq!(snapshot(&w.store).await, before, "{action}: nothing changed");
        let r = reply(&evs);
        assert!(r.contains("2 document") && r.contains("3 document"), "{action}: both listed with their counts: {r}");
    }
    assert!(w.is_live(&first).await && w.is_live(&second).await);
    for d in a.iter().chain(&b) {
        assert!(!w.trashed(d).await);
    }
}

/// A1: names equal ignoring case are the same name — "Work" and "work" both
/// live make "WORK" ambiguous.
#[tokio::test]
async fn a1_names_equal_ignoring_case_are_ambiguous_too() {
    let w = world("a1-ambiguous-case", 4096, answer("")).await;
    let upper = w.thread("Work").await;
    let lower = w.thread("work").await;
    let evs = w.act("delete_thread", Some("WORK")).await;
    assert_eq!(executed(&evs), vec![false], "ambiguous: not done");
    assert!(w.is_live(&upper).await && w.is_live(&lower).await);
}

/// A1: only live threads count. A deleted "Work" is not a match, and does not
/// make "Work Archive" one.
#[tokio::test]
async fn a1_a_deleted_thread_is_not_a_match() {
    let w = world("a1-deleted", 4096, answer("")).await;
    let old = w.thread("Work").await;
    sovereign_docs::delete_thread_to_trash(&w.docs, w.store.as_ref(), &old).await.unwrap();
    let archive = w.thread("Work Archive").await;
    let evs = w.act("rename_thread", Some("Work to Office")).await;
    assert_eq!(executed(&evs), vec![false]);
    assert_eq!(w.thread_row(&archive).await.name, "Work Archive");
    assert!(!w.is_live(&old).await, "the deleted thread stays deleted");
}

// -------------------------------------------------- A1, the tool layer --

fn call(name: &str, args: serde_json::Value) -> crate::tools::ToolCall {
    crate::tools::ToolCall { name: name.into(), arguments: args }
}

/// A1 through the chat loop's write tools (`tools.rs`): rename and move
/// resolve thread names exactly.
#[tokio::test]
async fn a1_tool_rename_and_move_resolve_exact_names() {
    let w = world("a1-tools", 4096, answer("")).await;
    let garden = w.thread("Garden").await;
    let archive = w.thread("Work Archive").await;
    let seeds = w.doc("Seeds", &garden, "seed list").await;

    let r = crate::tools::execute_write_tool(
        &call("rename_thread", serde_json::json!({"old_name": "Work", "new_name": "Office"})),
        w.store.as_ref(),
        Some(&w.docs),
    )
    .await;
    assert!(!r.success, "no thread named Work: not done ({})", r.output);
    assert_eq!(w.thread_row(&archive).await.name, "Work Archive");
    assert!(r.output.contains("Work Archive"), "the close name is offered: {}", r.output);

    let r = crate::tools::execute_write_tool(
        &call("move_document", serde_json::json!({"document_title": "Seeds", "thread_name": "Work"})),
        w.store.as_ref(),
        Some(&w.docs),
    )
    .await;
    assert!(!r.success, "no thread named Work: not moved ({})", r.output);
    assert_eq!(w.thread_of(&seeds).await, garden);

    let work = w.thread("Work").await;
    let r = crate::tools::execute_write_tool(
        &call("move_document", serde_json::json!({"document_title": "Seeds", "thread_name": " work"})),
        w.store.as_ref(),
        Some(&w.docs),
    )
    .await;
    assert!(r.success, "{}", r.output);
    assert_eq!(w.thread_of(&seeds).await, work, "moved into Work, not Work Archive");
}

/// A1 through the create-document tool: a named thread with no exact match
/// is not replaced by a thread with a longer name.
///
/// Settled (contract, "Readings settled with the test author", 2): today a
/// thread name that finds nothing falls back to the first thread. A1 governs
/// a thread *named* in the action — no exact match, not done (nothing
/// created), the close names offered. The fallback stays only for a call that
/// names no thread.
#[tokio::test]
async fn a1_tool_create_document_never_files_into_a_longer_name() {
    let w = world("a1-tool-create", 4096, answer("")).await;
    let archive = w.thread("Work Archive").await;
    let before = snapshot(&w.store).await;
    let r = crate::tools::execute_write_tool(
        &call("create_document", serde_json::json!({"title": "Minutes", "thread_name": "Work"})),
        w.store.as_ref(),
        Some(&w.docs),
    )
    .await;
    assert!(!r.success, "no thread named Work: not done ({})", r.output);
    assert_eq!(snapshot(&w.store).await, before, "no document created, in Work Archive or anywhere");
    assert!(r.output.contains("Work Archive"), "{}", r.output);
    let _ = archive;
}

/// A1 through the read tool `list_documents`: "Work" lists Work's documents
/// only, never Work Archive's.
#[tokio::test]
async fn a1_read_tool_lists_only_the_exact_threads_documents() {
    let w = world("a1-tool-list", 4096, answer("")).await;
    let archive = w.thread("Work Archive").await;
    w.doc("Archived memo", &archive, "old").await;
    let out = crate::tools::execute_tool(&call("list_documents", serde_json::json!({"thread": "Work"})), w.store.as_ref()).await;
    assert!(!out.output.contains("Archived memo"), "Work Archive's documents are not Work's: {}", out.output);

    let work = w.thread("Work").await;
    w.doc("Current memo", &work, "new").await;
    let out = crate::tools::execute_tool(&call("list_documents", serde_json::json!({"thread": "work"})), w.store.as_ref()).await;
    assert!(out.output.contains("Current memo"), "{}", out.output);
    assert!(!out.output.contains("Archived memo"), "{}", out.output);
}

/// A1 through `docs_host` (what skills use to find their thread): "Work"
/// never returns "Work Archive".
///
/// Settled (contract, "Readings settled with the test author", 3): a skill
/// cannot be asked "which one?" — no exact match → the thread is created; two
/// live threads with the exact name → refused (an error), never one picked.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a1_docs_host_find_or_create_resolves_the_exact_name() {
    use sovereign_skills::SkillDbAccess;
    let w = world("a1-docs-host", 4096, answer("")).await;
    let archive = w.thread("Work Archive").await;
    let db = crate::docs_host::ServiceSkillDb::new(sovereign_skills::wrap_db(w.store.clone()), w.docs.clone(), "daily-journal");

    let id = db.find_or_create_thread("Work", "").unwrap();
    assert_ne!(id, archive, "never Work Archive");
    assert_eq!(w.thread_row(&id).await.name, "Work", "a thread named Work was created");
    let again = db.find_or_create_thread(" work ", "").unwrap();
    assert_eq!(again, id, "found by its exact name (case and spaces ignored)");

    w.thread("Work").await;
    assert!(db.find_or_create_thread("Work", "").is_err(), "two live threads named Work: refused, none picked");
}

/// A1 through the skills' bare store bridge (`sovereign-skills/src/db_bridge.rs`,
/// settled reading 1: every caller). The bridge finds and never creates
/// (THREAD-sync S1): exact name only — "Work" never returns "Work Archive",
/// nothing is created, and two live "Work" are refused.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a1_skills_bridge_finds_only_the_exact_name() {
    use sovereign_skills::SkillDbAccess;
    let w = world("a1-skills-bridge", 4096, answer("")).await;
    let archive = w.thread("Work Archive").await;
    let bridge = sovereign_skills::wrap_db(w.store.clone());
    let before = snapshot(&w.store).await;

    let found = bridge.find_or_create_thread("Work", "");
    assert!(found.as_ref().map_or(true, |id| *id != archive), "never Work Archive: {found:?}");
    assert!(found.is_err(), "no thread named Work: an error (the bridge never creates): {found:?}");
    assert_eq!(snapshot(&w.store).await, before, "nothing created");

    let work = w.thread("Work").await;
    assert_eq!(bridge.find_or_create_thread(" work ", "").unwrap(), work, "the exact name (case and spaces ignored)");

    w.thread("Work").await;
    assert!(bridge.find_or_create_thread("Work", "").is_err(), "two live threads named Work: refused, none picked");
}

// ------------------------------------------------------------------ A2 --

/// Runs `query` through `handle_query` with a decision channel; when the
/// proposal arrives, runs `on_proposal` (which may change the store) and
/// sends `decision`. Returns every event. Without a proposal the channel is
/// closed after a short wait, so a stray wait for approval ends at once.
async fn run_with_decision<F, Fut>(w: &mut World, query: &str, decision: ActionDecision, on_proposal: F) -> Vec<OrchestratorEvent>
where
    F: FnOnce(ProposedAction) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let (dtx, drx) = tokio::sync::mpsc::channel(4);
    w.orch.set_decision_rx(drx);
    w.rec.take_events();
    let rec = w.rec.clone();
    let mut seen: Vec<OrchestratorEvent> = vec![];
    let driver = async {
        let mut on_proposal = Some(on_proposal);
        for _ in 0..2_000 {
            let new = rec.take_events();
            let proposal = proposals(&new).into_iter().next();
            seen.extend(new);
            if let Some(p) = proposal {
                if let Some(f) = on_proposal.take() {
                    f(p).await;
                }
                let _ = dtx.send(decision.clone()).await;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        drop(dtx);
    };
    let (res, ()) = tokio::join!(w.orch.handle_query(query), driver);
    res.unwrap();
    seen.extend(w.rec.take_events());
    seen
}

/// A2: the proposal names the resolved thread — its exact name, not what the
/// user typed — and what follows ("its 5 documents go to the trash").
/// `thread_id` carries the resolved id (read: the proposal's thread field is
/// the thread acted on, no longer the typed text).
#[tokio::test]
async fn a2_the_proposal_names_the_resolved_thread_and_its_consequence() {
    let script: Script = Arc::new(|_, _, i| if i == 0 { intent_json("delete_thread", Some("work")) } else { String::new() });
    let mut w = world("a2-names", 4096, script).await;
    let archive = w.thread("Work Archive").await;
    w.docs_in(&archive, "Archived", 2).await;
    let work = w.thread("Work").await;
    w.docs_in(&work, "Mine", 5).await;

    let evs = run_with_decision(&mut w, "delete the work thread", ActionDecision::Reject("no".into()), |_| async {}).await;
    let ps = proposals(&evs);
    assert_eq!(ps.len(), 1, "one proposal");
    let d = &ps[0].description;
    assert!(d.contains("Work"), "names the thread by its exact name (typed: \"work\"): {d}");
    assert!(!d.contains("Work Archive"), "{d}");
    assert!(d.contains('5') && d.to_lowercase().contains("trash"), "says what follows: its 5 documents go to the trash: {d}");
    assert_eq!(ps[0].thread_id.as_deref(), Some(work.as_str()), "the proposal carries the resolved thread's id");
    assert!(w.is_live(&work).await, "rejected: nothing done");
}

/// A2: the proposals of rename, move and merge name the resolved threads (and
/// the document moved). Typed in lower case; the stored names are capitalised.
#[tokio::test]
async fn a2_rename_move_and_merge_proposals_name_the_resolved_threads() {
    let cases: &[(&str, &str, &[&str])] = &[
        ("rename_thread", "plans to Plans 2026", &["Plans", "Plans 2026"]),
        ("move_document", "Seeds to kitchen", &["Seeds", "Kitchen"]),
        ("merge_threads", "garden to kitchen", &["Garden", "Kitchen"]),
    ];
    for (i, (action, target, names)) in cases.iter().enumerate() {
        let (a, t) = (action.to_string(), target.to_string());
        let script: Script = Arc::new(move |_, _, n| if n == 0 { intent_json(&a, Some(&t)) } else { String::new() });
        let mut w = world(&format!("a2-proposals-{i}"), 4096, script).await;
        w.thread("Plans").await;
        let garden = w.thread("Garden").await;
        w.thread("Kitchen").await;
        w.doc("Seeds", &garden, "seed list").await;
        let evs = run_with_decision(&mut w, target, ActionDecision::Reject("no".into()), |_| async {}).await;
        let ps = proposals(&evs);
        assert_eq!(ps.len(), 1, "{action}: one proposal");
        for n in *names {
            assert!(ps[0].description.contains(n), "{action}: the proposal names {n:?}: {}", ps[0].description);
        }
    }
}

/// A2 with A1: no exact match, no proposal — the user is never asked to
/// approve an action on a thread that is not the one named.
#[tokio::test]
async fn a2_no_exact_match_means_no_proposal() {
    let script: Script = Arc::new(|_, _, i| if i == 0 { intent_json("delete_thread", Some("Work")) } else { String::new() });
    let mut w = world("a2-no-match", 4096, script).await;
    let archive = w.thread("Work Archive").await;
    let docs = w.docs_in(&archive, "Archived", 2).await;
    let evs = run_with_decision(&mut w, "delete work", ActionDecision::Approve, |_| async {}).await;
    assert!(proposals(&evs).is_empty(), "no proposal for an unresolved name");
    assert!(w.is_live(&archive).await);
    for d in &docs {
        assert!(!w.trashed(d).await);
    }
    assert!(reply(&evs).contains("Work Archive"), "{}", reply(&evs));
}

/// A2: after approval the action runs on the resolved id, never on a second
/// lookup of the name. Between the proposal and the approval, "Work" is
/// renamed "Office" and a new thread "Work" is created: the new "Work" — never
/// proposed — is untouched.
///
/// Updated for C-HOST AT4 (spec ch. 08, v2.0.78; LH test author,
/// 2026-11-14). It accepted before that the action "either runs or is
/// refused". A thread deletion (Destruct) whose thread was renamed since the
/// proposal is not executed: it is proposed again, within the same request,
/// with its own request id, and the new proposal shows the thread as it is now
/// ("Office", the approved id). The new "Work" is never acted on.
#[tokio::test]
async fn a2_after_approval_the_action_runs_on_the_resolved_id() {
    let script: Script = Arc::new(|_, _, i| if i == 0 { intent_json("delete_thread", Some("Work")) } else { String::new() });
    let mut w = world("a2-resolved-id", 4096, script).await;
    let work = w.thread("Work").await;
    let originals = w.docs_in(&work, "Original", 2).await;
    let docs = w.docs.clone();
    let store = w.store.clone();
    let newcomer: Arc<Mutex<Option<(String, String)>>> = Arc::new(Mutex::new(None));
    let slot = newcomer.clone();
    let work_id = work.clone();

    let evs = run_with_decision(&mut w, "delete work", ActionDecision::Approve, move |_| async move {
        docs.thread_update(&work_id, Some("Office"), None).await.unwrap();
        let fresh = docs.thread_create("Work", "").await.unwrap();
        let d = crate::docs_host::create(&docs, crate::docs_host::new_user_document("Fresh note", &fresh, "new")).await.unwrap().id;
        *slot.lock().unwrap() = Some((fresh, d));
        let _ = store;
    })
    .await;
    let (fresh, fresh_doc) = newcomer.lock().unwrap().clone().expect("the proposal arrived");
    assert!(w.is_live(&fresh).await, "the new Work, never proposed, is not deleted");
    assert!(!w.trashed(&fresh_doc).await, "its document is not trashed");

    let ps = proposals(&evs);
    assert_eq!(ps.len(), 2, "AT4: proposed again after the change: {ps:?}");
    assert_ne!(ps[0].request_id, ps[1].request_id, "AT4: the new proposal has its own request id");
    assert_eq!(ps[1].target.id, work, "the new proposal is for the approved id");
    assert!(ps[1].description.contains("Office"), "it shows the thread as it is now: {}", ps[1].description);
    assert!(executed(&evs).iter().all(|ok| !ok), "not executed: {evs:?}");
    assert!(w.is_live(&work).await, "nothing done to the approved id");
    for d in &originals {
        assert!(!w.trashed(d).await, "its documents are not trashed");
    }
}

// ------------------------------------------------------------------ A3 --

/// A3: every thread action's reply names the thread(s) it acted on — and the
/// delete says how many documents went to the trash ("Deleted thread 'Work'
/// (5 documents moved to the trash)").
#[tokio::test]
async fn a3_each_thread_actions_reply_names_what_was_done() {
    let cases: &[(&str, &str, &[&str])] = &[
        ("create_thread", "Fresh", &["Fresh"]),
        ("rename_thread", "Plans to Roadmap", &["Plans", "Roadmap"]),
        ("delete_thread", "Work", &["Work", "5 document"]),
        ("move_document", "Seeds to Kitchen", &["Seeds", "Kitchen"]),
        ("merge_threads", "Garden to Kitchen", &["Garden", "Kitchen"]),
        ("split_thread", "Garden to Orchard", &["Garden", "Orchard"]),
        ("create_milestone", "Launch on Garden", &["Launch", "Garden"]),
        ("list_milestones", "Garden", &["Garden"]),
    ];
    for (i, (action, target, names)) in cases.iter().enumerate() {
        let w = world(&format!("a3-reply-{i}"), 4096, answer("")).await;
        w.thread("Plans").await;
        let work = w.thread("Work").await;
        w.docs_in(&work, "Mine", 5).await;
        let garden = w.thread("Garden").await;
        w.thread("Kitchen").await;
        w.doc("Seeds", &garden, "seed list").await;

        let evs = w.act(action, Some(target)).await;
        assert_eq!(executed(&evs), vec![true], "{action} {target:?}");
        let r = reply(&evs);
        for n in *names {
            assert!(r.contains(n), "{action}: the reply names {n:?}: {r}");
        }
    }
}
