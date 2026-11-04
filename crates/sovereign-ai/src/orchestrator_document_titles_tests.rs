//! v0.0.12 item 23 — **document actions by exact title; merging threads always
//! asks** (T1–T6; `doc/plans/V0012-item23-document-titles.md`). Item 6's rule
//! (exact thread names, `orchestrator_thread_names_tests.rs`) applied to
//! documents.
//!
//! - **T1** — a document named in an assistant action resolves to a document
//!   whose title equals the given one, ignoring case and surrounding spaces,
//!   and to nothing else: "Report" never resolves to "Quarterly Report Draft".
//!   Live documents only, for every action (`restore` brings a live document
//!   back to its previous version, as today; reading 1 of the contract). No
//!   exact match: not done, the close titles listed (at most 5) and a question;
//!   no candidate: "no document" has that title. Two or more exact: not done,
//!   at most 5 listed with their thread and last modified date (local time,
//!   History's `YYYY-MM-DD HH:MM`; reading 4), and the reply
//!   says the action can be done from the document on the canvas. Every
//!   caller: the orchestrator's `open`/`navigate`, `history`, `summarize`,
//!   `move_document`, `adopt`, `restore`, and the chat tools `move_document`
//!   and `get_document`. The scan that no production code takes a single
//!   document from `search_documents_by_title` is
//!   `sovereign-docs/tests/t1_one_document_resolution.rs`.
//! - **T2** — the writes (`move_document`, `adopt`, `restore`) resolve before
//!   the trust check and the proposal; the proposal names the real title (and
//!   carries the resolved id); after approval the action runs on that id; a
//!   document gone meanwhile (purged, or moved to the trash) is refused.
//!   **With trust auto-approval reached, a partial title still does
//!   not act.**
//! - **T3** — `search` and the `search_documents` tool keep their partial match
//!   and act on nothing.
//! - **T4** — `merge_threads` is never auto-approved, even after five
//!   approvals (its level, Destruct, is pinned in
//!   `sovereign-core/tests/t4_merge_threads_destruct.rs`).
//! - **T5** — every outcome is an `ActionOutcome`: a refusal is `Failed`, never
//!   "Done" or ✓; a read is `Reported`.
//! - **T6** — rename / move / merge take their two names from the person's own
//!   request when the router's `target` holds only the first one; `entities`
//!   are never the source; a request that splits more than one way is not
//!   done and asks for quotes; quoted names are taken as given.
//! - **T7** (amendment, Céline, 2026-10-25, claude-coord 0164) — a name read
//!   from unquoted words (T6's unquoted readings, and any split of a router
//!   `target` at " to " — `restore`'s, and rename's or move's old path;
//!   reading 7) always shows the proposal, whatever the trust state; a
//!   quoted name, or a router `target` taken whole, keeps the trust path. The
//!   name as written resolves first, then the stripped one ("the", "thread",
//!   "document" dropped); both resolving to different entities is two
//!   readings: not done, the reply asks for quotes. A no-match reply suggests
//!   quotes. No new API: read through behaviour.
//!
//! **API named here (does not exist yet — this module does not compile until
//! it does):**
//!
//! ```ignore
//! // sovereign_db (a module beside `thread_names`, re-exported at the crate root)
//! pub const MAX_CLOSE_TITLES: usize = 5;
//! #[derive(Debug, Clone)]
//! pub enum DocumentTitleMatch {
//!     Found(Document),                    // exactly one live document has that title
//!     NoMatch { close: Vec<Document> },   // none; the candidates, at most MAX_CLOSE_TITLES (empty: none came close)
//!     Ambiguous(Vec<Document>),           // two or more have that exact title (all of them): none is picked
//! }
//! impl DocumentTitleMatch { pub fn found(self) -> Option<Document>; }
//! /// Live documents only (none in the trash), for every caller.
//! pub async fn resolve_document_title<D: GraphDB + ?Sized>(
//!     db: &D, title: &str,
//! ) -> DbResult<DocumentTitleMatch>;
//! ```
//!
//! (Revised after the contract's correction of 2026-10-25: with every action
//! looking among live documents, the first version's `DocumentScope { Live,
//! Trash }` parameter had no caller for `Trash`, and is gone.)
//!
//! The candidates are **all** the blind index offers (not the first 20, not
//! the first one); their decrypted titles are compared. The wording, as item
//! 6's `resolve_thread_or_explain`, is the implementer's (suggested:
//! `crate::document_titles::resolve_document_or_explain(db, title) ->
//! Result<Document, String>`); these tests read it only through what the user
//! (or, for the chat tools, the model) is told.
//!
//! "The reply" is `assistant_harness::reply` (chat messages, refusals, the
//! executed event read through `Debug`). "Not done" is read from the store
//! (snapshot unchanged), from `ActionExecuted.success` and from the outcome
//! (`Failed`).
//!
//! Written before the code by another author; the implementer does not edit
//! this file.

use super::assistant_harness::*;
use super::*;
use sovereign_core::interfaces::ActionOutcome;
use sovereign_db::DocumentTitleMatch;

// ------------------------------------------------------------- helpers --

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

/// T5: not done — one executed event, `Failed`, `success == false`, and
/// neither its line nor any chat message says "Done" or ✓.
fn assert_not_done(events: &[OrchestratorEvent], what: &str) {
    let (success, o) = the_outcome(events, what);
    assert!(!success, "{what}: not done, yet success: {o:?}");
    assert!(matches!(o, ActionOutcome::Failed { .. }), "{what}: a refusal is Failed: {o:?}");
    assert!(!says_done(&o.summary_line()), "{what}: a refusal never says Done: {}", o.summary_line());
    for m in chat(events) {
        assert!(!says_done(&m), "{what}: a refusal never says Done: {m}");
    }
}

/// Events that act on a document (a read shown, or a write).
fn acted_on_documents(events: &[OrchestratorEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            OrchestratorEvent::DocumentOpened { doc_id } => Some(format!("opened {doc_id}")),
            OrchestratorEvent::VersionHistory { doc_id, .. } => Some(format!("history {doc_id}")),
            OrchestratorEvent::DocumentMoved { doc_id, .. } => Some(format!("moved {doc_id}")),
            OrchestratorEvent::AdoptionStarted { doc_id } => Some(format!("adopted {doc_id}")),
            _ => None,
        })
        .collect()
}

/// A pause so that documents created one after the other get distinct
/// creation times: the blind index lists the newest first, so a document
/// created after the exact one is what a first-match lookup picks.
async fn tick() {
    tokio::time::sleep(std::time::Duration::from_millis(15)).await;
}

/// The user's own document with two saved versions (a version restore would
/// change it), written straight to the store as `b_world` does.
async fn versioned_doc(w: &World, title: &str, thread: &str, text: &str) -> String {
    let mut d = sovereign_db::schema::Document::new(title.into(), thread.into(), true);
    d.content = format!("{text} (first)");
    let id = w.store.create_document(d).await.unwrap().id_string().unwrap();
    w.store.commit_document(&id, "one").await.unwrap();
    w.store.update_document(&id, None, Some(text)).await.unwrap();
    w.store.commit_document(&id, "two").await.unwrap();
    id
}

/// A versioned document, then put in the trash.
async fn trashed_doc(w: &World, title: &str, thread: &str, text: &str) -> String {
    let id = versioned_doc(w, title, thread, text).await;
    w.docs.trash(&id).await.unwrap();
    id
}

/// An external (never owned) document — what `adopt` acts on.
async fn web_doc(w: &World, title: &str, thread: &str, text: &str) -> String {
    crate::docs_host::create(
        &w.docs,
        crate::docs_host::new_web_document(title, thread, "https://example.org/page", text),
    )
    .await
    .unwrap()
    .id
}

/// The document's text as the service reads it (live or in the trash).
async fn text_of(w: &World, id: &str) -> String {
    w.docs.read(id).await.unwrap().plain_text()
}

async fn is_owned_now(store: &sovereign_db::encrypted::EncryptedGraphDB, id: &str) -> bool {
    store.get_document(id).await.unwrap().is_owned
}

/// Five approvals of `action` on the query path: the trust threshold, so a
/// Modify action is auto-approved from now on.
fn trust_five(w: &World, action: &str) {
    let mut t = w.orch.trust.lock().unwrap();
    for _ in 0..5 {
        t.record_approval(crate::trust::WORKFLOW_QUERY, action);
    }
}

/// The router's answer: `action`, `target`, `entities`, confident.
fn router_json(action: &str, target: Option<&str>, entities: &[(&str, &str)]) -> String {
    let ents: Vec<[&str; 2]> = entities.iter().map(|(k, v)| [*k, *v]).collect();
    serde_json::json!({ "action": action, "target": target, "confidence": 0.95, "entities": ents }).to_string()
}

/// A script whose first call (the router) answers `json`.
fn routed(json: String) -> Script {
    Arc::new(move |_: &str, _: u32, i: usize| if i == 0 { json.clone() } else { "A short summary.".to_string() })
}

/// Runs `query` through `handle_query` with a decision channel. When a
/// proposal arrives, runs `on_proposal` (which may change the store) and sends
/// `decision`. Ends as soon as `handle_query` has returned (no proposal: no
/// wait). Returns every event.
async fn run_with_decision<F, Fut>(w: &mut World, query: &str, decision: ActionDecision, on_proposal: F) -> Vec<OrchestratorEvent>
where
    F: FnOnce(ProposedAction) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let (dtx, drx) = tokio::sync::mpsc::channel(4);
    w.orch.set_decision_rx(drx);
    w.rec.take_events();
    let rec = w.rec.clone();
    let done = std::sync::atomic::AtomicBool::new(false);
    let mut seen: Vec<OrchestratorEvent> = vec![];
    let query_fut = async {
        let r = w.orch.handle_query(query).await;
        done.store(true, std::sync::atomic::Ordering::SeqCst);
        r
    };
    let driver = async {
        let mut on_proposal = Some(on_proposal);
        for _ in 0..4_000 {
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
            if done.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        drop(dtx);
    };
    let (res, ()) = tokio::join!(query_fut, driver);
    res.unwrap();
    seen.extend(w.rec.take_events());
    seen
}

/// A modified date as the ambiguous reply shows it: local time, History's
/// format (contract, reading 4) — computed in local time, so it holds around
/// midnight in any zone.
fn local_stamp(d: chrono::DateTime<chrono::Utc>) -> String {
    d.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string()
}

// ------------------------------------------------ T1, the resolution --

/// T1: "Report" resolves to "Report", whatever its case and surrounding
/// spaces, never to "Quarterly Report Draft" (created after it, so listed
/// first by the blind index).
#[tokio::test]
async fn t1_resolver_exact_title_ignoring_case_and_spaces() {
    let w = world("t1-res-exact", 4096, answer("")).await;
    let t = w.thread("Office").await;
    let report = w.doc("Report", &t, "report body").await;
    tick().await;
    w.doc("Quarterly Report Draft", &t, "draft body").await;
    for typed in ["Report", "report", "  REPORT  ", " rEpOrT"] {
        let m = sovereign_db::resolve_document_title(w.store.as_ref(), typed).await.unwrap();
        let d = m.found().unwrap_or_else(|| panic!("{typed:?}: found"));
        assert_eq!(d.id_string().as_deref(), Some(report.as_str()), "{typed:?}: the exact title, not the draft");
        assert_eq!(d.title, "Report");
    }
}

/// T1: no exact title — `NoMatch`, the close titles offered (1 to 5, each a
/// candidate); nothing at all — `NoMatch` with no close titles.
#[tokio::test]
async fn t1_resolver_no_exact_match_offers_at_most_five_close_titles() {
    let w = world("t1-res-close", 4096, answer("")).await;
    let t = w.thread("Office").await;
    let titles = [
        "Quarterly Report Draft",
        "Report 2025",
        "Old Report",
        "Report notes",
        "Team Report",
        "Report log",
        "Report ideas",
    ];
    for title in titles {
        w.doc(title, &t, "text").await;
    }
    w.doc("Seeds", &t, "seed list").await;
    match sovereign_db::resolve_document_title(w.store.as_ref(), "Report").await.unwrap() {
        DocumentTitleMatch::NoMatch { close } => {
            assert!((1..=5).contains(&close.len()), "1 to 5 close titles, got {}", close.len());
            assert!(close.len() <= sovereign_db::MAX_CLOSE_TITLES);
            for d in &close {
                assert!(titles.contains(&d.title.as_str()), "a close title is a candidate: {}", d.title);
            }
        }
        other => panic!("no document is titled exactly Report: {other:?}"),
    }
    match sovereign_db::resolve_document_title(w.store.as_ref(), "Submarine").await.unwrap() {
        DocumentTitleMatch::NoMatch { close } => assert!(close.is_empty(), "nothing comes close: {close:?}"),
        other => panic!("{other:?}"),
    }
}

/// T1: two documents with the same title (ignoring case) are ambiguous: none
/// is picked, both are returned.
#[tokio::test]
async fn t1_resolver_two_exact_titles_are_ambiguous() {
    let w = world("t1-res-ambiguous", 4096, answer("")).await;
    let home = w.thread("Home").await;
    let office = w.thread("Office").await;
    w.doc("Daily journal", &home, "home").await;
    w.doc("daily Journal", &office, "office").await;
    match sovereign_db::resolve_document_title(w.store.as_ref(), "DAILY JOURNAL").await.unwrap() {
        DocumentTitleMatch::Ambiguous(v) => assert_eq!(v.len(), 2),
        other => panic!("{other:?}"),
    }
}

/// T1: live documents only — a document in the trash is never found, never
/// makes a title ambiguous, and is not offered as a close title.
#[tokio::test]
async fn t1_resolver_never_finds_a_document_in_the_trash() {
    let w = world("t1-res-live", 4096, answer("")).await;
    let t = w.thread("Office").await;
    let live = w.doc("Report", &t, "live").await;
    trashed_doc(&w, "Report", &t, "trashed").await;
    let found = sovereign_db::resolve_document_title(w.store.as_ref(), "Report").await.unwrap().found();
    assert_eq!(found.and_then(|d| d.id_string()).as_deref(), Some(live.as_str()), "the live one, not ambiguous with the trashed one");

    trashed_doc(&w, "Minutes", &t, "trashed").await;
    trashed_doc(&w, "Minutes draft", &t, "trashed").await;
    match sovereign_db::resolve_document_title(w.store.as_ref(), "Minutes").await.unwrap() {
        DocumentTitleMatch::NoMatch { close } => assert!(close.is_empty(), "nothing in the trash is offered: {close:?}"),
        other => panic!("only trashed documents are titled Minutes: {other:?}"),
    }
}

/// T1: the candidates are **all** the blind index offers. The exact "Report"
/// is the oldest of 25 documents carrying the word: a lookup that keeps the
/// newest 20 (today's `LIMIT 20`), or the first, never sees it.
#[tokio::test]
async fn t1_resolver_takes_every_candidate_not_the_first_twenty() {
    let w = world("t1-res-all", 4096, answer("")).await;
    let t = w.thread("Office").await;
    let report = w.doc("Report", &t, "the one").await;
    tick().await;
    for i in 1..=24 {
        w.doc(&format!("Report {i}"), &t, "another").await;
    }
    let m = sovereign_db::resolve_document_title(w.store.as_ref(), "Report").await.unwrap();
    assert_eq!(m.found().and_then(|d| d.id_string()).as_deref(), Some(report.as_str()), "the exact title, behind 24 newer candidates");
}

// ---------------------------------------- T1, the orchestrator's actions --

/// The actions of T1 run through `execute_action`, with the target that names
/// "Report" (move: into Kitchen).
const T1_ACTIONS: &[(&str, &str)] = &[
    ("open", "Report"),
    ("navigate", "Report"),
    ("history", "Report"),
    ("summarize", "Report"),
    ("adopt", "Report"),
    ("move_document", "Report to Kitchen"),
    ("restore", "Report"),
];

/// T1, the review's case for documents ("restore Report" restored "Quarterly
/// Report Draft"): only "Quarterly Report Draft" exists (external for
/// `adopt`, with two versions for `restore`); every action on "Report" is not
/// done, acts on nothing (no version restored), offers the close title and
/// asks which one.
#[tokio::test]
async fn t1_report_never_resolves_to_quarterly_report_draft() {
    for (i, (action, target)) in T1_ACTIONS.iter().enumerate() {
        let what = format!("{action} {target:?}");
        let w = world(&format!("t1-draft-{i}"), 4096, answer("A short summary.")).await;
        let office = w.thread("Office").await;
        w.thread("Kitchen").await;
        let close = match *action {
            "adopt" => {
                web_doc(&w, "Quarterly Report Draft", &office, "draft body").await;
                "Quarterly Report Draft"
            }
            "restore" => {
                versioned_doc(&w, "Quarterly Report Draft", &office, "draft body").await;
                "Quarterly Report Draft"
            }
            // Through the service, so scanned: today it would be summarized.
            _ => {
                w.doc("Quarterly Report Draft", &office, "draft body").await;
                "Quarterly Report Draft"
            }
        };
        let before = snapshot(&w.store).await;
        let calls_before = w.rec.calls().len();
        let evs = w.act(action, Some(target)).await;
        assert_not_done(&evs, &what);
        assert_eq!(snapshot(&w.store).await, before, "{what}: nothing changed");
        assert!(acted_on_documents(&evs).is_empty(), "{what}: no document acted on: {:?}", acted_on_documents(&evs));
        assert_eq!(w.rec.calls().len(), calls_before, "{what}: nothing was sent to the model (no summary of the draft)");
        let r = reply(&evs);
        assert!(r.contains(close), "{what}: the close title is offered: {r}");
        assert!(r.to_lowercase().contains("which"), "{what}: the reply asks which one is meant: {r}");
    }
}

/// T1: with both "Report" and "Quarterly Report Draft" (the draft newer, so
/// first in the blind index), every spelling of "report" acts on "Report" —
/// and a read is `Reported`, a write `Changed`. `restore` brings "Report"
/// back to its previous version and leaves the draft as it is.
#[tokio::test]
async fn t1_case_and_space_variants_act_only_on_the_exact_title() {
    for (i, (action, target)) in T1_ACTIONS.iter().enumerate() {
        let typed = target.replace("Report", "  rEPORT ");
        let what = format!("{action} {typed:?}");
        let w = world(&format!("t1-variants-{i}"), 4096, answer("A short summary.")).await;
        let office = w.thread("Office").await;
        let kitchen = w.thread("Kitchen").await;
        let (report, draft) = match *action {
            "adopt" => {
                let r = web_doc(&w, "Report", &office, "report body").await;
                tick().await;
                (r, web_doc(&w, "Quarterly Report Draft", &office, "draft body").await)
            }
            // Summarize: documents created through the service, so scanned
            // for personal data (an unscanned one is never summarized).
            "summarize" => {
                let r = w.doc("Report", &office, "report body").await;
                tick().await;
                (r, w.doc("Quarterly Report Draft", &office, "draft body").await)
            }
            _ => {
                let r = versioned_doc(&w, "Report", &office, "report body").await;
                tick().await;
                (r, versioned_doc(&w, "Quarterly Report Draft", &office, "draft body").await)
            }
        };
        let evs = w.act(action, Some(&typed)).await;
        let (success, o) = the_outcome(&evs, &what);
        assert!(success, "{what}: {o:?}");
        match *action {
            "open" | "navigate" => {
                assert!(matches!(o, ActionOutcome::Reported { .. }), "{what}: a read is Reported: {o:?}");
                assert_eq!(acted_on_documents(&evs), vec![format!("opened {report}")], "{what}");
            }
            "history" => {
                assert!(matches!(o, ActionOutcome::Reported { .. }), "{what}: {o:?}");
                assert_eq!(acted_on_documents(&evs), vec![format!("history {report}")], "{what}");
            }
            "summarize" => {
                assert!(matches!(o, ActionOutcome::Reported { .. }), "{what}: {o:?}");
                let prompts: String = w.rec.calls().iter().map(|c| c.prompt.clone()).collect();
                assert!(prompts.contains("report body"), "{what}: Report's text was summarized");
                assert!(!prompts.contains("draft body"), "{what}: the draft's text was not");
            }
            "adopt" => {
                assert!(matches!(o, ActionOutcome::Changed { .. }), "{what}: {o:?}");
                assert!(is_owned_now(&w.store, &report).await, "{what}: Report adopted");
                assert!(!is_owned_now(&w.store, &draft).await, "{what}: the draft stays external");
            }
            "move_document" => {
                assert!(matches!(o, ActionOutcome::Changed { .. }), "{what}: {o:?}");
                assert_eq!(w.thread_of(&report).await, kitchen, "{what}: Report moved");
                assert_eq!(w.thread_of(&draft).await, office, "{what}: the draft stays");
            }
            "restore" => {
                assert!(matches!(o, ActionOutcome::Changed { .. }), "{what}: {o:?}");
                assert!(o.summary_line().contains("'Report'"), "{what}: names the document: {}", o.summary_line());
                assert_eq!(text_of(&w, &report).await, "report body (first)", "{what}: Report back to its previous version");
                assert_eq!(text_of(&w, &draft).await, "draft body", "{what}: the draft untouched");
                assert_eq!(w.store.list_document_commits(&draft).await.unwrap().len(), 2, "{what}: no version of the draft made");
            }
            _ => unreachable!(),
        }
    }
}

/// T1: with no candidate at all, the reply says no document has that title,
/// and offers nothing unrelated.
#[tokio::test]
async fn t1_no_candidate_says_no_document_has_that_title() {
    for (i, (action, target)) in T1_ACTIONS.iter().enumerate() {
        let target = target.replace("Report", "Submarine");
        let what = format!("{action} {target:?}");
        let w = world(&format!("t1-none-{i}"), 4096, answer("A short summary.")).await;
        let office = w.thread("Office").await;
        w.thread("Kitchen").await;
        web_doc(&w, "Seeds", &office, "seed list").await;
        trashed_doc(&w, "Pans", &office, "pans").await;
        let before = snapshot(&w.store).await;
        let evs = w.act(action, Some(&target)).await;
        assert_not_done(&evs, &what);
        assert_eq!(snapshot(&w.store).await, before, "{what}");
        let r = reply(&evs);
        assert!(r.to_lowercase().contains("no document"), "{what}: says no document has that title: {r}");
        assert!(!r.contains("Seeds") && !r.contains("Pans"), "{what}: nothing unrelated offered: {r}");
    }
}

/// T1: two documents with the exact title (external for `adopt`) are
/// ambiguous: not done, nothing changed, both listed with their thread and
/// last modified date (local, `YYYY-MM-DD HH:MM`), and the reply says the
/// action can be done from the document on the canvas.
#[tokio::test]
async fn t1_two_exact_titles_are_listed_with_thread_and_date_and_the_canvas() {
    for (i, (action, target)) in T1_ACTIONS.iter().enumerate() {
        let target = target.replace("Report", "Daily journal");
        let what = format!("{action} {target:?}");
        let w = world(&format!("t1-ambiguous-{i}"), 4096, answer("A short summary.")).await;
        let home = w.thread("Home").await;
        let office = w.thread("Office").await;
        w.thread("Kitchen").await;
        let mut ids = vec![];
        for t in [&home, &office] {
            ids.push(match *action {
                "adopt" => web_doc(&w, "Daily journal", t, "entry").await,
                _ => versioned_doc(&w, "Daily journal", t, "entry").await,
            });
        }
        let before = snapshot(&w.store).await;
        let calls_before = w.rec.calls().len();
        let evs = w.act(action, Some(&target)).await;
        assert_not_done(&evs, &what);
        assert_eq!(snapshot(&w.store).await, before, "{what}: nothing changed");
        assert!(acted_on_documents(&evs).is_empty(), "{what}: none picked: {:?}", acted_on_documents(&evs));
        assert_eq!(w.rec.calls().len(), calls_before, "{what}: nothing summarized");
        let r = reply(&evs);
        assert!(r.contains("Home") && r.contains("Office"), "{what}: each listed with its thread: {r}");
        for id in &ids {
            let stamp = local_stamp(w.store.get_document(id).await.unwrap().modified_at);
            assert!(r.contains(&stamp), "{what}: each with its last modified date ({stamp}, local): {r}");
        }
        assert!(r.to_lowercase().contains("canvas"), "{what}: says it can be done from the document on the canvas: {r}");
    }
}

/// T1: of seven documents with the exact title, at most five are listed.
#[tokio::test]
async fn t1_ambiguous_lists_at_most_five() {
    let w = world("t1-ambiguous-seven", 4096, answer("")).await;
    let names = ["Amber", "Birch", "Cedar", "Dune", "Elm", "Fjord", "Grove"];
    for n in names {
        let t = w.thread(n).await;
        w.doc("Daily journal", &t, "entry").await;
    }
    let evs = w.act("open", Some("Daily journal")).await;
    assert_not_done(&evs, "open Daily journal");
    let r = reply(&evs);
    let listed = names.iter().filter(|n| r.contains(*n)).count();
    assert!((2..=5).contains(&listed), "2 to 5 of the seven listed, got {listed}: {r}");
}

/// T1, where it looks: live documents, for every action. (a) A "Report" in
/// the trash is not opened, read, summarized, adopted, moved or restored. (b)
/// With a live "Report" and a trashed "Report" (newer), `restore` brings the
/// live one back to its previous version and leaves the trashed one as it is.
#[tokio::test]
async fn t1_live_only_for_every_action() {
    // (a)
    for (i, (action, target)) in T1_ACTIONS.iter().enumerate() {
        let w = world(&format!("t1-where-a-{i}"), 4096, answer("A short summary.")).await;
        let office = w.thread("Office").await;
        w.thread("Kitchen").await;
        let id = match *action {
            "adopt" => web_doc(&w, "Report", &office, "body").await,
            "summarize" => w.doc("Report", &office, "body").await,
            _ => versioned_doc(&w, "Report", &office, "body").await,
        };
        w.docs.trash(&id).await.unwrap();
        let before = snapshot(&w.store).await;
        let calls_before = w.rec.calls().len();
        let evs = w.act(action, Some(target)).await;
        assert_not_done(&evs, action);
        assert_eq!(snapshot(&w.store).await, before, "{action}: a trashed document is not acted on");
        assert!(acted_on_documents(&evs).is_empty(), "{action}: {:?}", acted_on_documents(&evs));
        assert_eq!(w.rec.calls().len(), calls_before, "{action}: nothing sent to the model");
    }

    // (b)
    let w = world("t1-where-b", 4096, answer("")).await;
    let office = w.thread("Office").await;
    let live = versioned_doc(&w, "Report", &office, "current text").await;
    tick().await;
    let gone = trashed_doc(&w, "Report", &office, "old text").await;
    let evs = w.act("restore", Some("Report")).await;
    let (success, o) = the_outcome(&evs, "restore with a live and a trashed Report");
    assert!(success && matches!(o, ActionOutcome::Changed { .. }), "{o:?}");
    assert_eq!(text_of(&w, &live).await, "current text (first)", "the live one back to its previous version");
    assert!(w.trashed(&gone).await, "the trashed one stays in the trash");
    assert_eq!(text_of(&w, &gone).await, "old text", "the trashed one untouched");
}

// -------------------------------------------------- T1, the chat tools --

fn call(name: &str, args: serde_json::Value) -> crate::tools::ToolCall {
    crate::tools::ToolCall { name: name.into(), arguments: args }
}

/// T1 through the chat loop's write tool `move_document`: only the exact
/// title moves; no exact match, not moved and the close title offered with a
/// question; two exact, not moved, both listed with their thread.
#[tokio::test]
async fn t1_tool_move_document_resolves_the_exact_title() {
    let w = world("t1-tool-move", 4096, answer("")).await;
    let office = w.thread("Office").await;
    let kitchen = w.thread("Kitchen").await;
    let draft = w.doc("Quarterly Report Draft", &office, "draft").await;

    let mv = |title: &str| call("move_document", serde_json::json!({"document_title": title, "thread_name": "Kitchen"}));
    let r = crate::tools::execute_write_tool(&mv("Report"), w.store.as_ref(), Some(&w.docs)).await;
    assert!(!r.success, "no document titled Report: not moved ({})", r.output);
    assert!(!says_done(&r.output), "{}", r.output);
    assert_eq!(w.thread_of(&draft).await, office, "the draft is not moved");
    assert!(r.output.contains("Quarterly Report Draft"), "the close title is offered: {}", r.output);
    assert!(r.output.to_lowercase().contains("which"), "and asks which: {}", r.output);

    let r = crate::tools::execute_write_tool(&mv("Submarine"), w.store.as_ref(), Some(&w.docs)).await;
    assert!(!r.success);
    assert!(r.output.to_lowercase().contains("no document"), "{}", r.output);

    let report = w.doc("Report", &office, "report").await;
    tick().await;
    let draft2 = w.doc("Quarterly Report Draft 2", &office, "draft").await;
    let r = crate::tools::execute_write_tool(&mv(" rePort "), w.store.as_ref(), Some(&w.docs)).await;
    assert!(r.success, "{}", r.output);
    assert_eq!(w.thread_of(&report).await, kitchen, "Report moved");
    assert_eq!(w.thread_of(&draft).await, office, "the drafts stay");
    assert_eq!(w.thread_of(&draft2).await, office, "the drafts stay");

    let home = w.thread("Home").await;
    let a = w.doc("Daily journal", &home, "a").await;
    let b = w.doc("Daily journal", &office, "b").await;
    let r = crate::tools::execute_write_tool(&mv("Daily journal"), w.store.as_ref(), Some(&w.docs)).await;
    assert!(!r.success, "ambiguous: not moved ({})", r.output);
    assert_eq!(w.thread_of(&a).await, home);
    assert_eq!(w.thread_of(&b).await, office);
    assert!(r.output.contains("Home") && r.output.contains("Office"), "both listed with their thread: {}", r.output);
}

/// T1 through the read tool `get_document`: the body of a document whose
/// title only contains the given one never reaches the model; the exact title
/// (case and spaces ignored) does; two exact, neither body, both listed.
#[tokio::test]
async fn t1_tool_get_document_resolves_the_exact_title() {
    let w = world("t1-tool-get", 4096, answer("")).await;
    let office = w.thread("Office").await;
    w.doc("Quarterly Report Draft", &office, "draft secret body").await;
    let get = |title: &str| call("get_document", serde_json::json!({ "title": title }));

    let out = crate::tools::execute_tool(&get("Report"), w.store.as_ref()).await.output;
    assert!(!out.contains("draft secret body"), "the draft's body is not read for Report: {out}");
    assert!(out.contains("Quarterly Report Draft"), "the close title is offered: {out}");

    let out = crate::tools::execute_tool(&get("Submarine"), w.store.as_ref()).await.output;
    assert!(out.to_lowercase().contains("no document"), "{out}");

    w.doc("Report", &office, "report plain body").await;
    tick().await;
    w.doc("Quarterly Report Draft 2", &office, "second draft body").await;
    let out = crate::tools::execute_tool(&get("  REPORT "), w.store.as_ref()).await.output;
    assert!(out.contains("report plain body"), "the exact title is read: {out}");
    assert!(!out.contains("draft secret body") && !out.contains("second draft body"), "{out}");

    let home = w.thread("Home").await;
    w.doc("Daily journal", &home, "home entry body").await;
    w.doc("Daily journal", &office, "office entry body").await;
    let out = crate::tools::execute_tool(&get("Daily journal"), w.store.as_ref()).await.output;
    assert!(!out.contains("home entry body") && !out.contains("office entry body"), "none picked: {out}");
    assert!(out.contains("Home") && out.contains("Office"), "both listed with their thread: {out}");
}

// ----------------------------------------------------------------- T2 --

/// A world for the writes: Garden (Seeds), Kitchen, Office (the external
/// Clipping, and "Report" with two versions).
struct Writes {
    seeds: String,
    clipping: String,
    report: String,
    kitchen: String,
}

async fn writes_world(w: &World) -> Writes {
    let garden = w.thread("Garden").await;
    let kitchen = w.thread("Kitchen").await;
    let office = w.thread("Office").await;
    let seeds = w.doc("Seeds", &garden, "seed list").await;
    let clipping = web_doc(w, "Clipping", &office, "a clipped page").await;
    let report = versioned_doc(w, "Report", &office, "report text").await;
    Writes { seeds, clipping, report, kitchen }
}

/// The three writes: (action, what the router puts in `target`, typed in
/// lower case).
const WRITES: &[(&str, &str)] = &[("move_document", "seeds to kitchen"), ("adopt", "clipping"), ("restore", "report")];

/// A newcomer with the title of a resolved document, created through the
/// store and the service while the proposal waits: live for move, external
/// for adopt, live with two versions for restore (so a lookup by title would
/// restore it). Returns its id.
async fn newcomer_for(action: &str, docs: &sovereign_docs::DocumentService, store: &sovereign_db::encrypted::EncryptedGraphDB, thread: &str) -> String {
    match action {
        "move_document" => crate::docs_host::create(docs, crate::docs_host::new_user_document("Seeds", thread, "newcomer")).await.unwrap().id,
        "adopt" => {
            crate::docs_host::create(docs, crate::docs_host::new_web_document("Clipping", thread, "https://example.org/new", "newcomer"))
                .await
                .unwrap()
                .id
        }
        _ => {
            let mut d = sovereign_db::schema::Document::new("Report".into(), thread.into(), true);
            d.content = "newcomer (first)".into();
            let id = store.create_document(d).await.unwrap().id_string().unwrap();
            store.commit_document(&id, "one").await.unwrap();
            store.update_document(&id, None, Some("newcomer")).await.unwrap();
            store.commit_document(&id, "two").await.unwrap();
            id
        }
    }
}

/// The newcomer was not acted on.
async fn assert_newcomer_untouched(w: &World, action: &str, fresh: &str, kitchen: &str) {
    match action {
        "move_document" => assert_ne!(w.thread_of(fresh).await, kitchen, "{action}: the newcomer is not moved"),
        "adopt" => assert!(!is_owned_now(&w.store, fresh).await, "{action}: the newcomer is not adopted"),
        _ => assert_eq!(text_of(w, fresh).await, "newcomer", "{action}: no version of the newcomer is restored"),
    }
}

async fn thread_id_named(store: &sovereign_db::encrypted::EncryptedGraphDB, name: &str) -> String {
    store.list_threads().await.unwrap().into_iter().find(|t| t.name == name).unwrap().id_string().unwrap()
}

/// T2: the writes resolve before the proposal; the proposal names the
/// document by its real title (typed in lower case), says what follows (move:
/// Kitchen; restore: its previous version), and carries the resolved id.
/// Rejected: nothing done.
#[tokio::test]
async fn t2_the_proposal_names_the_resolved_document() {
    for (i, (action, target)) in WRITES.iter().enumerate() {
        let mut w = world(&format!("t2-names-{i}"), 4096, routed(intent_json(action, Some(target)))).await;
        let ws = writes_world(&w).await;
        let before = snapshot(&w.store).await;
        let evs = run_with_decision(&mut w, target, ActionDecision::Reject("no".into()), |_| async {}).await;
        let ps = proposals(&evs);
        assert_eq!(ps.len(), 1, "{action}: one proposal: {evs:?}");
        let d = &ps[0].description;
        let (title, id) = match *action {
            "move_document" => {
                assert!(d.contains("Kitchen"), "{action}: names the thread: {d}");
                ("Seeds", &ws.seeds)
            }
            "adopt" => ("Clipping", &ws.clipping),
            _ => {
                assert!(d.to_lowercase().contains("version"), "{action}: says it restores a version: {d}");
                ("Report", &ws.report)
            }
        };
        assert!(d.contains(&format!("'{title}'")) || d.contains(&format!("\"{title}\"")), "{action}: names the real title, quoted: {d}");
        assert_eq!(ps[0].doc_id.as_deref(), Some(id.as_str()), "{action}: the proposal carries the resolved id");
        assert_eq!(snapshot(&w.store).await, before, "{action}: rejected, nothing done");
    }
}

/// T2 with T1: a partial title gets no proposal — the user is never asked to
/// approve an action on a document that is not the one named.
#[tokio::test]
async fn t2_a_partial_title_gets_no_proposal() {
    for (i, (action, target)) in [("move_document", "report to kitchen"), ("adopt", "report"), ("restore", "report")].iter().enumerate() {
        let mut w = world(&format!("t2-partial-{i}"), 4096, routed(intent_json(action, Some(target)))).await;
        let office = w.thread("Office").await;
        w.thread("Kitchen").await;
        match *action {
            "adopt" => {
                web_doc(&w, "Quarterly Report Draft", &office, "draft").await;
            }
            _ => {
                versioned_doc(&w, "Quarterly Report Draft", &office, "draft").await;
            }
        }
        let before = snapshot(&w.store).await;
        let evs = run_with_decision(&mut w, target, ActionDecision::Approve, |_| async {}).await;
        assert!(proposals(&evs).is_empty(), "{action}: no proposal for an unresolved title: {evs:?}");
        assert_not_done(&evs, action);
        assert_eq!(snapshot(&w.store).await, before, "{action}: nothing changed");
    }
}

/// T2: after approval the action runs on the resolved id, never on a second
/// lookup of the title. Between the proposal and the approval a newcomer with
/// the same exact title appears (newer, so first for a title lookup): the
/// resolved document is acted on, the newcomer — never proposed — is
/// untouched.
#[tokio::test]
async fn t2_after_approval_the_action_runs_on_the_resolved_id() {
    for (i, (action, target)) in WRITES.iter().enumerate() {
        let mut w = world(&format!("t2-resolved-{i}"), 4096, routed(intent_json(action, Some(target)))).await;
        let ws = writes_world(&w).await;
        let docs = w.docs.clone();
        let store = w.store.clone();
        let newcomer: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let slot = newcomer.clone();
        let act = action.to_string();
        let evs = run_with_decision(&mut w, target, ActionDecision::Approve, move |_| async move {
            let garden = thread_id_named(&store, "Garden").await;
            let id = newcomer_for(&act, &docs, &store, &garden).await;
            *slot.lock().unwrap() = Some(id);
        })
        .await;
        assert_eq!(proposals(&evs).len(), 1, "{action}");
        let fresh = newcomer.lock().unwrap().clone().expect("the proposal arrived");
        let (success, o) = the_outcome(&evs, action);
        assert!(success && matches!(o, ActionOutcome::Changed { .. }), "{action}: done on the approved id: {o:?}");
        match *action {
            "move_document" => assert_eq!(w.thread_of(&ws.seeds).await, ws.kitchen, "the proposed Seeds moved"),
            "adopt" => assert!(is_owned_now(&w.store, &ws.clipping).await, "the proposed Clipping adopted"),
            _ => assert_eq!(text_of(&w, &ws.report).await, "report text (first)", "the proposed Report back to its previous version"),
        }
        assert_newcomer_untouched(&w, action, &fresh, &ws.kitchen).await;
    }
}

/// T2: a document gone between the proposal and the approval is refused, and
/// a newcomer with its title is not taken in its place. Gone: purged (all
/// three writes), or moved to the trash (all three: it is not acted on in the
/// trash, and none of its versions is restored).
#[tokio::test]
async fn t2_a_document_gone_meanwhile_is_refused() {
    for purge in [true, false] {
        for (i, (action, target)) in WRITES.iter().enumerate() {
            let what = format!("{action}, {} meanwhile", if purge { "purged" } else { "moved to the trash" });
            let mut w = world(&format!("t2-gone-{purge}-{i}"), 4096, routed(intent_json(action, Some(target)))).await;
            let ws = writes_world(&w).await;
            let resolved = match *action {
                "move_document" => ws.seeds.clone(),
                "adopt" => ws.clipping.clone(),
                _ => ws.report.clone(),
            };
            let docs = w.docs.clone();
            let store = w.store.clone();
            let act = action.to_string();
            let gone = resolved.clone();
            let newcomer: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
            let slot = newcomer.clone();
            let evs = run_with_decision(&mut w, target, ActionDecision::Approve, move |_| async move {
                docs.trash(&gone).await.unwrap();
                if purge {
                    docs.purge_from_trash(&gone).await.unwrap();
                }
                let office = thread_id_named(&store, "Office").await;
                let id = newcomer_for(&act, &docs, &store, &office).await;
                *slot.lock().unwrap() = Some(id);
            })
            .await;
            assert_eq!(proposals(&evs).len(), 1, "{what}");
            assert_not_done(&evs, &what);
            let fresh = newcomer.lock().unwrap().clone().unwrap();
            assert_newcomer_untouched(&w, action, &fresh, &ws.kitchen).await;
            if !purge {
                assert!(w.trashed(&resolved).await, "{what}: it stays in the trash");
                match *action {
                    "move_document" => assert_ne!(w.thread_of(&resolved).await, ws.kitchen, "{what}: not moved"),
                    "adopt" => assert!(!is_owned_now(&w.store, &resolved).await, "{what}: not adopted"),
                    _ => assert_eq!(text_of(&w, &resolved).await, "report text", "{what}: no version restored"),
                }
            }
        }
    }
}

/// T2, the risk this item closes: with trust auto-approval reached (five
/// approvals of the action), a partial title still does not act — no
/// proposal, nothing changed, not done. Then, as the control that the five
/// approvals did reach auto-approval, the exact title acts without a
/// proposal.
#[tokio::test]
async fn t2_with_auto_approval_reached_a_partial_title_still_does_not_act() {
    let cases: &[(&str, &str, &str)] = &[
        // T7 reading 7: a router target split at " to " loses trust, so the
        // move's control names both in quotes.
        ("move_document", "report to kitchen", "'Report' to 'Kitchen'"),
        ("adopt", "report", "Report"),
        ("restore", "report", "Report"),
    ];
    for (i, (action, partial, exact)) in cases.iter().enumerate() {
        let (a, p, e) = (action.to_string(), partial.to_string(), exact.to_string());
        let script: Script = Arc::new(move |prompt: &str, _: u32, _: usize| {
            // The router sees the query; the second query is the exact one
            // ("EXACT", or for the move the quoted names).
            if prompt.contains("EXACT") || prompt.contains("'Report' to 'Kitchen'") {
                intent_json(&a, Some(&e))
            } else {
                intent_json(&a, Some(&p))
            }
        });
        let mut w = world(&format!("t2-trust-{i}"), 4096, script).await;
        let office = w.thread("Office").await;
        let kitchen = w.thread("Kitchen").await;
        match *action {
            "adopt" => {
                web_doc(&w, "Quarterly Report Draft", &office, "draft").await;
            }
            _ => {
                versioned_doc(&w, "Quarterly Report Draft", &office, "draft").await;
            }
        }
        trust_five(&w, action);
        let before = snapshot(&w.store).await;
        let evs = run_with_decision(&mut w, "do it", ActionDecision::Approve, |_| async {}).await;
        assert!(proposals(&evs).is_empty(), "{action}: no proposal: {evs:?}");
        assert_not_done(&evs, &format!("{action}, auto-approval reached, partial title"));
        assert_eq!(snapshot(&w.store).await, before, "{action}: a partial title never acts, even trusted");

        // Control: the exact title, trusted, acts with no proposal.
        let id = match *action {
            "adopt" => web_doc(&w, "Report", &office, "r").await,
            _ => versioned_doc(&w, "Report", &office, "r").await,
        };
        let control = if *action == "move_document" { "move 'Report' to 'Kitchen'" } else { "EXACT" };
        let evs = run_with_decision(&mut w, control, ActionDecision::Reject("not asked".into()), |_| async {}).await;
        assert!(proposals(&evs).is_empty(), "{action}: auto-approved, so no proposal (control): {evs:?}");
        let (success, o) = the_outcome(&evs, action);
        assert!(success && matches!(o, ActionOutcome::Changed { .. }), "{action}: the exact title acts (control): {o:?}");
        match *action {
            "adopt" => assert!(is_owned_now(&w.store, &id).await),
            "restore" => assert_eq!(text_of(&w, &id).await, "r (first)"),
            _ => assert_eq!(w.thread_of(&id).await, kitchen),
        }
    }
}

// ----------------------------------------------------------------- T3 --

/// T3: `search` keeps its partial match — "Report" finds "Report" and
/// "Quarterly Report Draft" — reports, and changes nothing.
#[tokio::test]
async fn t3_search_finds_partial_titles_and_acts_on_nothing() {
    let w = world("t3-search", 4096, answer("")).await;
    let office = w.thread("Office").await;
    let report = w.doc("Report", &office, "r").await;
    let draft = web_doc(&w, "Quarterly Report Draft", &office, "d").await;
    let before = snapshot(&w.store).await;
    let evs = w.act("search", Some("Report")).await;
    let (success, o) = the_outcome(&evs, "search");
    assert!(success && matches!(o, ActionOutcome::Reported { .. }), "{o:?}");
    assert!(!says_done(&o.summary_line()), "{}", o.summary_line());
    let found: Vec<String> = evs
        .iter()
        .filter_map(|e| match e {
            OrchestratorEvent::SearchResults { doc_ids, .. } => Some(doc_ids.clone()),
            _ => None,
        })
        .flatten()
        .collect();
    assert!(found.contains(&report) && found.contains(&draft), "both listed: {found:?}");
    assert_eq!(snapshot(&w.store).await, before, "a search acts on nothing");
    assert!(acted_on_documents(&evs).is_empty());
}

/// T3: the chat tool `search_documents` keeps its partial match.
#[tokio::test]
async fn t3_search_documents_tool_finds_partial_titles() {
    let w = world("t3-tool", 4096, answer("")).await;
    let office = w.thread("Office").await;
    w.doc("Report", &office, "r").await;
    w.doc("Quarterly Report Draft", &office, "d").await;
    let before = snapshot(&w.store).await;
    let out = crate::tools::execute_tool(&call("search_documents", serde_json::json!({"query": "report"})), w.store.as_ref()).await;
    assert!(out.output.contains("Quarterly Report Draft") && out.output.contains("- Report"), "{}", out.output);
    assert_eq!(snapshot(&w.store).await, before);
}

// ----------------------------------------------------------------- T4 --

/// T4: whatever the trust state, `merge_threads` is never auto-approved —
/// five approvals on either workflow change nothing.
#[test]
fn t4_merge_threads_is_never_auto_approved_by_the_tracker() {
    let mut t = crate::trust::TrustTracker::new();
    for wf in [crate::trust::WORKFLOW_QUERY, crate::trust::WORKFLOW_CHAT] {
        for _ in 0..6 {
            t.record_approval(wf, "merge_threads");
        }
        assert!(
            !t.should_auto_approve(wf, "merge_threads", security::action_level("merge_threads")),
            "{wf}: merge_threads auto-approved after six approvals"
        );
    }
}

/// T4 through the orchestrator: after five approvals of `merge_threads`, a
/// merge is still proposed (rejected: nothing merged). Control: five
/// approvals of `rename_thread` (Modify) do auto-approve it.
#[tokio::test]
async fn t4_merge_threads_always_asks_even_after_five_approvals() {
    let script: Script = Arc::new(|prompt: &str, _: u32, _: usize| {
        if prompt.contains("RENAME") {
            intent_json("rename_thread", Some("Plans to Roadmap"))
        } else {
            intent_json("merge_threads", Some("Kitchen to Garden"))
        }
    });
    let mut w = world("t4-merge", 4096, script).await;
    let garden = w.thread("Garden").await;
    let kitchen = w.thread("Kitchen").await;
    let plans = w.thread("Plans").await;
    let seeds = w.doc("Seeds", &garden, "seed list").await;
    trust_five(&w, "merge_threads");
    trust_five(&w, "rename_thread");
    let before = snapshot(&w.store).await;

    let evs = run_with_decision(&mut w, "merge garden into kitchen", ActionDecision::Reject("no".into()), |_| async {}).await;
    assert_eq!(proposals(&evs).len(), 1, "a merge always asks: {evs:?}");
    assert_eq!(snapshot(&w.store).await, before, "rejected: nothing merged");
    assert!(w.is_live(&garden).await && w.is_live(&kitchen).await);
    assert_eq!(w.thread_of(&seeds).await, garden);

    // T7 reading 7: the control names both in quotes (a router target split
    // at " to " would lose trust).
    let evs = run_with_decision(&mut w, "RENAME 'Plans' to 'Roadmap'", ActionDecision::Reject("not asked".into()), |_| async {}).await;
    assert!(proposals(&evs).is_empty(), "control: a trusted rename is auto-approved: {evs:?}");
    assert_eq!(w.thread_row(&plans).await.name, "Roadmap", "control: the rename ran");
}

// ----------------------------------------------------------------- T6 --
//
// The fixtures are the 3B router's real outputs (PANDA2, claude-coord 0161):
// for rename / move / merge it puts only the first name in `target`, the
// second in `entities` (swapped for moves). The request is the person's text,
// sent as the query; the router answers the fixture.

/// T6: each request does what the person wrote — the two names come from
/// their words; `entities` (swapped for the moves, and a decoy thread named
/// like the wrong entity) are never the source. Approved: done.
#[tokio::test]
async fn t6_router_fixtures_act_on_the_names_the_person_wrote() {
    // (request, action, target, entities)
    let fixtures: &[(&str, &str, &str, &[(&str, &str)])] = &[
        ("rename thread Alpha to Beta", "rename_thread", "Alpha", &[("old_name", "Alpha"), ("new_name", "Beta")]),
        ("rename the Work thread to Projects", "rename_thread", "Work", &[("old_name", "Work"), ("new_name", "Projects")]),
        ("rename Trip to Holidays", "rename_thread", "Trip", &[("old_name", "Trip"), ("new_name", "Holidays")]),
        ("move the API Spec to Development", "move_document", "API Spec", &[("old_thread", "Development"), ("new_thread", "API Spec")]),
        (
            "move Budget 2026 into the Finance thread",
            "move_document",
            "Budget 2026",
            &[("old_thread", "Finance"), ("new_thread", "Budget 2026")],
        ),
        ("merge thread Drafts into Archive", "merge_threads", "Drafts", &[("source_thread", "Drafts"), ("destination_thread", "Archive")]),
        // Constructed (not a router output): the entities disagree with the
        // words; the words win.
        ("rename thread Alpha to Beta", "rename_thread", "Alpha", &[("old_name", "Alpha"), ("new_name", "Gamma")]),
    ];
    for (i, (request, action, target, entities)) in fixtures.iter().enumerate() {
        let mut w = world(&format!("t6-fixture-{i}"), 4096, routed(router_json(action, Some(target), entities))).await;
        let alpha = w.thread("Alpha").await;
        let work = w.thread("Work").await;
        let trip = w.thread("Trip").await;
        let specs = w.thread("Specs").await;
        let development = w.thread("Development").await;
        let decoy_api = w.thread("API Spec").await;
        let inbox = w.thread("Inbox").await;
        let finance = w.thread("Finance").await;
        let decoy_budget = w.thread("Budget 2026").await;
        let drafts = w.thread("Drafts").await;
        let archive = w.thread("Archive").await;
        let api = w.doc("API Spec", &specs, "the api").await;
        let budget = w.doc("Budget 2026", &inbox, "the budget").await;
        let in_drafts = w.docs_in(&drafts, "Draft note", 2).await;

        let evs = run_with_decision(&mut w, request, ActionDecision::Approve, |_| async {}).await;
        assert_eq!(proposals(&evs).len(), 1, "{request:?}: one proposal: {evs:?}");
        let (success, o) = the_outcome(&evs, request);
        assert!(success && matches!(o, ActionOutcome::Changed { .. }), "{request:?}: done: {o:?}");
        match i {
            0 => assert_eq!(w.thread_row(&alpha).await.name, "Beta"),
            1 => assert_eq!(w.thread_row(&work).await.name, "Projects"),
            2 => assert_eq!(w.thread_row(&trip).await.name, "Holidays"),
            3 => {
                assert_eq!(w.thread_of(&api).await, development, "API Spec moved into Development");
                assert!(w.store.list_documents(Some(&decoy_api)).await.unwrap().is_empty(), "never into the entity's thread");
            }
            4 => {
                assert_eq!(w.thread_of(&budget).await, finance, "Budget 2026 moved into Finance");
                assert!(w.store.list_documents(Some(&decoy_budget)).await.unwrap().is_empty());
            }
            5 => {
                assert!(!w.is_live(&drafts).await, "Drafts merged away");
                assert!(w.is_live(&archive).await, "Archive kept");
                for d in &in_drafts {
                    assert_eq!(w.thread_of(d).await, archive, "Drafts' documents now in Archive");
                }
            }
            6 => assert_eq!(w.thread_row(&alpha).await.name, "Beta", "the words, not the entities (Gamma)"),
            _ => unreachable!(),
        }
        assert!(w.live_threads_named("Gamma").await.is_empty(), "{request:?}: no entity name used");
    }
}

/// T6: a request whose words split more than one way — a name containing
/// " to " — is not done: no proposal, nothing changed, the reply names the
/// readings and asks for the names in quotes. Threads "Trip" and "Trip to
/// Rome" both exist. Router answer constructed on the pattern of the real
/// fixtures (first name only in `target`).
#[tokio::test]
async fn t6_words_that_split_two_ways_are_not_done_and_ask_for_quotes() {
    let json = router_json("rename_thread", Some("Trip"), &[("old_name", "Trip"), ("new_name", "Rome to Holidays")]);
    let mut w = world("t6-two-ways", 4096, routed(json)).await;
    w.thread("Trip").await;
    w.thread("Trip to Rome").await;
    let before = snapshot(&w.store).await;
    let evs = run_with_decision(&mut w, "rename Trip to Rome to Holidays", ActionDecision::Approve, |_| async {}).await;
    assert!(proposals(&evs).is_empty(), "no proposal: {evs:?}");
    assert_not_done(&evs, "two readings");
    assert_eq!(snapshot(&w.store).await, before, "nothing renamed");
    let r = reply(&evs);
    assert!(r.contains("Trip to Rome") && r.contains("Holidays"), "names the readings: {r}");
    assert!(r.contains("Rome to Holidays"), "both readings, the other one too: {r}");
    assert!(r.to_lowercase().contains("quot"), "asks for the names in quotes: {r}");
}

/// T6: quoted names are taken as given — `'Trip to Rome'` is one name. Single
/// and double quotes. Router answer constructed: a 3B router following its
/// observed pattern would put the first name, unquoted, in `target`
/// ("Trip to Rome"), which itself contains " to ".
#[tokio::test]
async fn t6_quoted_names_are_taken_as_given() {
    for (i, request) in ["rename 'Trip to Rome' to 'Holidays'", "rename \"Trip to Rome\" to \"Holidays\""].iter().enumerate() {
        let json = router_json("rename_thread", Some("Trip to Rome"), &[("old_name", "Trip to Rome"), ("new_name", "Holidays")]);
        let mut w = world(&format!("t6-quoted-{i}"), 4096, routed(json)).await;
        let trip = w.thread("Trip").await;
        let rome = w.thread("Trip to Rome").await;
        let evs = run_with_decision(&mut w, request, ActionDecision::Approve, |_| async {}).await;
        assert_eq!(proposals(&evs).len(), 1, "{request}: {evs:?}");
        let (success, o) = the_outcome(&evs, request);
        assert!(success && matches!(o, ActionOutcome::Changed { .. }), "{request}: {o:?}");
        assert_eq!(w.thread_row(&rome).await.name, "Holidays", "{request}: 'Trip to Rome' renamed");
        assert_eq!(w.thread_row(&trip).await.name, "Trip", "{request}: Trip untouched");
    }
}

// ----------------------------------------------------------------- T7 --
//
// Amendment of 2026-10-25 (Céline, on PANDA2's second position, claude-coord
// 0164). "Parsed from unquoted text" is T6's reading of the person's request
// when a name is not quoted, and `restore`'s split of its router `target` at
// " to ". A name the person put in quotes, or a router `target` taken whole
// (no split, no stripping), keeps the trust path as today. Every case below
// first reaches trust (five approvals of the same action), so "a proposal" is
// the guarantee and "no proposal" the control that trust was reached. Router
// answers follow the 3B router's observed pattern (0161): the first name only
// in `target`, the second in `entities` (labels swapped for moves).

/// The router answer for a rename, on the 0161 pattern.
fn rename_route(old: &str, new: &str) -> String {
    router_json("rename_thread", Some(old), &[("old_name", old), ("new_name", new)])
}

/// The router answer for a move, on the 0161 pattern (labels swapped).
fn move_route(doc: &str, thread: &str) -> String {
    router_json("move_document", Some(doc), &[("old_thread", thread), ("new_thread", doc)])
}

/// T7: with trust reached for `action`, `request` is proposed — never run
/// unseen — and nothing in the store changes before the decision. The
/// proposal is then approved (the run that follows is the caller's to check).
/// Returns the events and the one proposal.
async fn t7_proposed_under_trust(w: &mut World, action: &str, request: &str) -> (Vec<OrchestratorEvent>, ProposedAction) {
    trust_five(w, action);
    let before = snapshot(&w.store).await;
    let store = w.store.clone();
    let what = request.to_string();
    let evs = run_with_decision(w, request, ActionDecision::Approve, move |_| async move {
        assert_eq!(snapshot(&store).await, before, "{what:?}: nothing is written before the approval");
    })
    .await;
    let ps = proposals(&evs);
    assert_eq!(
        ps.len(),
        1,
        "{request:?}: a name read from unquoted words always shows the proposal, even with trust reached (T7): {evs:?}"
    );
    (evs.clone(), ps[0].clone())
}

/// T7, the control: with trust reached for `action`, `request` runs with no
/// proposal (the decision channel answers a rejection that is never asked
/// for) and its outcome is a change.
async fn t7_runs_trusted(w: &mut World, action: &str, request: &str) -> Vec<OrchestratorEvent> {
    trust_five(w, action);
    let evs = run_with_decision(w, request, ActionDecision::Reject("not asked".into()), |_| async {}).await;
    assert!(proposals(&evs).is_empty(), "{request:?}: trusted, so no proposal (control): {evs:?}");
    let (success, o) = the_outcome(&evs, request);
    assert!(success && matches!(o, ActionOutcome::Changed { .. }), "{request:?}: runs trusted (control): {o:?}");
    evs
}

/// `name` appears in `text` at least once not preceded by `prefix` (ignoring
/// case): the stripped name named on its own, not only inside the name as
/// written ("Archive" besides "The Archive").
fn mentions_alone(text: &str, name: &str, prefix: &str) -> bool {
    let (t, n, p) = (text.to_lowercase(), name.to_lowercase(), prefix.to_lowercase());
    let mut from = 0;
    while let Some(rel) = t[from..].find(&n) {
        let at = from + rel;
        if !t[..at].ends_with(&p) {
            return true;
        }
        from = at + n.len();
    }
    false
}

/// T7, case 1: "rename The Archive to Old", only "The Archive" existing — the
/// name as written resolves (today "The" is dropped first and "Archive" is
/// not found), and since it was read from unquoted words the proposal is
/// shown even with trust reached; approved, "The Archive" becomes "Old".
/// Control: the same request with quoted names runs trusted, unseen.
#[tokio::test]
async fn t7_unquoted_the_archive_resolves_as_written_and_is_proposed_even_trusted() {
    let mut w = world("t7-archive", 4096, routed(rename_route("The Archive", "Old"))).await;
    let archive = w.thread("The Archive").await;
    let (evs, p) = t7_proposed_under_trust(&mut w, "rename_thread", "rename The Archive to Old").await;
    assert!(p.description.contains("'The Archive'"), "the proposal names the thread as written: {}", p.description);
    assert!(p.description.contains("Old"), "and the new name: {}", p.description);
    assert_eq!(p.thread_id.as_deref(), Some(archive.as_str()), "the proposal carries 'The Archive'");
    let (success, o) = the_outcome(&evs, "rename The Archive to Old");
    assert!(success && matches!(o, ActionOutcome::Changed { .. }), "approved: done: {o:?}");
    assert_eq!(w.thread_row(&archive).await.name, "Old");

    let mut w = world("t7-archive-quoted", 4096, routed(rename_route("The Archive", "Old"))).await;
    let archive = w.thread("The Archive").await;
    t7_runs_trusted(&mut w, "rename_thread", "rename 'The Archive' to 'Old'").await;
    assert_eq!(w.thread_row(&archive).await.name, "Old", "control: the quoted rename ran");
}

/// T7, case 2: "rename Alpha to Beta please" — the new name runs to the end of
/// the words, so the reading is "Beta please". Read from unquoted words: the
/// proposal is shown, even with trust reached, and names 'Beta please' so the
/// person sees it; nothing is written before the approval. Control:
/// "rename 'Alpha' to 'Beta'" runs trusted, unseen.
#[tokio::test]
async fn t7_unquoted_new_name_running_to_the_end_is_proposed_even_trusted() {
    let mut w = world("t7-please", 4096, routed(rename_route("Alpha", "Beta"))).await;
    let alpha = w.thread("Alpha").await;
    let (_evs, p) = t7_proposed_under_trust(&mut w, "rename_thread", "rename Alpha to Beta please").await;
    assert!(p.description.contains("'Beta please'"), "the proposal names the new name as read: {}", p.description);
    assert!(p.description.contains("'Alpha'"), "and the thread: {}", p.description);
    assert_eq!(p.thread_id.as_deref(), Some(alpha.as_str()));

    let mut w = world("t7-please-quoted", 4096, routed(rename_route("Alpha", "Beta"))).await;
    let alpha = w.thread("Alpha").await;
    t7_runs_trusted(&mut w, "rename_thread", "rename 'Alpha' to 'Beta'").await;
    assert_eq!(w.thread_row(&alpha).await.name, "Beta", "control: the quoted rename ran");
}

/// T7, case 3: "restore Trip to Rome" with "Trip" and "Trip to Rome" both live
/// and versioned; the router's `target` is "Trip to Rome", which `restore`
/// splits at " to ". That split is a name parsed from unquoted text: the
/// proposal is shown, even with trust reached (today "Trip" is restored
/// unseen), and nothing is restored before the approval. Approved, the
/// document the proposal carries — and only it — goes back to its previous
/// version. Which of the two it is, is not pinned here (the contract's split
/// is unchanged: today's proposal would name 'Trip').
#[tokio::test]
async fn t7_restore_split_of_its_target_is_proposed_even_trusted() {
    let mut w = world("t7-restore", 4096, routed(intent_json("restore", Some("Trip to Rome")))).await;
    let t = w.thread("Travel").await;
    let trip = versioned_doc(&w, "Trip", &t, "trip text").await;
    let rome = versioned_doc(&w, "Trip to Rome", &t, "rome text").await;
    let (evs, p) = t7_proposed_under_trust(&mut w, "restore", "restore Trip to Rome").await;
    let named = p.doc_id.clone().expect("the proposal carries the resolved document");
    assert!(named == trip || named == rome, "the proposal names one of the two documents: {p:?}");
    assert!(p.description.contains("'Trip"), "the proposal names its document: {}", p.description);
    let (success, o) = the_outcome(&evs, "restore Trip to Rome");
    assert!(success && matches!(o, ActionOutcome::Changed { .. }), "approved: done: {o:?}");
    let (restored, other, restored_text, other_text) =
        if named == trip { (&trip, &rome, "trip text (first)", "rome text") } else { (&rome, &trip, "rome text (first)", "trip text") };
    assert_eq!(text_of(&w, restored).await, restored_text, "the proposed document is the one restored");
    assert_eq!(text_of(&w, other).await, other_text, "the other one is untouched");
}

/// T7, case 4: "move The Plan to Work", only the document "The Plan" existing
/// — the title as written resolves (today "The" is dropped and "Plan" is not
/// found), and the proposal is shown even with trust reached; approved, it
/// moves. Control: the same request with quoted names runs trusted, unseen.
#[tokio::test]
async fn t7_unquoted_move_the_plan_resolves_as_written_and_is_proposed_even_trusted() {
    let mut w = world("t7-plan", 4096, routed(move_route("The Plan", "Work"))).await;
    let inbox = w.thread("Inbox").await;
    let work = w.thread("Work").await;
    let plan = w.doc("The Plan", &inbox, "the plan").await;
    let (evs, p) = t7_proposed_under_trust(&mut w, "move_document", "move The Plan to Work").await;
    assert!(p.description.contains("'The Plan'"), "the proposal names the title as written: {}", p.description);
    assert!(p.description.contains("Work"), "and the thread: {}", p.description);
    assert_eq!(p.doc_id.as_deref(), Some(plan.as_str()));
    let (success, o) = the_outcome(&evs, "move The Plan to Work");
    assert!(success && matches!(o, ActionOutcome::Changed { .. }), "approved: done: {o:?}");
    assert_eq!(w.thread_of(&plan).await, work);

    let mut w = world("t7-plan-quoted", 4096, routed(move_route("The Plan", "Work"))).await;
    let inbox = w.thread("Inbox").await;
    let work = w.thread("Work").await;
    let plan = w.doc("The Plan", &inbox, "the plan").await;
    t7_runs_trusted(&mut w, "move_document", "move 'The Plan' to 'Work'").await;
    assert_eq!(w.thread_of(&plan).await, work, "control: the quoted move ran");
}

/// T7, as written first, then stripped — threads: "rename The Archive to Old"
/// with threads "The Archive" and "Archive" both existing. Both readings
/// resolve, to different threads: not done (no proposal, nothing renamed,
/// `Failed`), and the reply names both and asks for quotes. With and without
/// trust reached.
#[tokio::test]
async fn t7_thread_as_written_and_stripped_both_resolving_is_two_readings() {
    for trusted in [false, true] {
        let mut w = world(&format!("t7-two-threads-{trusted}"), 4096, routed(rename_route("The Archive", "Old"))).await;
        w.thread("The Archive").await;
        w.thread("Archive").await;
        if trusted {
            trust_five(&w, "rename_thread");
        }
        let before = snapshot(&w.store).await;
        let evs = run_with_decision(&mut w, "rename The Archive to Old", ActionDecision::Approve, |_| async {}).await;
        let what = format!("two readings (trusted: {trusted})");
        assert!(proposals(&evs).is_empty(), "{what}: no proposal: {evs:?}");
        assert_not_done(&evs, &what);
        assert_eq!(snapshot(&w.store).await, before, "{what}: nothing renamed");
        let r = reply(&evs);
        assert!(r.contains("The Archive"), "{what}: names the thread as written: {r}");
        assert!(mentions_alone(&r, "Archive", "the "), "{what}: and the stripped one, 'Archive': {r}");
        assert!(r.to_lowercase().contains("quot"), "{what}: asks for quotes: {r}");
    }
}

/// T7, as written first, then stripped — documents: "move The Plan to Work"
/// with documents "The Plan" and "Plan" both existing. Two readings: not
/// done, nothing moved, the reply names both and asks for quotes. With and
/// without trust reached.
#[tokio::test]
async fn t7_document_as_written_and_stripped_both_resolving_is_two_readings() {
    for trusted in [false, true] {
        let mut w = world(&format!("t7-two-docs-{trusted}"), 4096, routed(move_route("The Plan", "Work"))).await;
        let inbox = w.thread("Inbox").await;
        w.thread("Work").await;
        w.doc("The Plan", &inbox, "the plan").await;
        w.doc("Plan", &inbox, "a plan").await;
        if trusted {
            trust_five(&w, "move_document");
        }
        let before = snapshot(&w.store).await;
        let evs = run_with_decision(&mut w, "move The Plan to Work", ActionDecision::Approve, |_| async {}).await;
        let what = format!("two readings (trusted: {trusted})");
        assert!(proposals(&evs).is_empty(), "{what}: no proposal: {evs:?}");
        assert_not_done(&evs, &what);
        assert_eq!(snapshot(&w.store).await, before, "{what}: nothing moved");
        let r = reply(&evs);
        assert!(r.contains("The Plan"), "{what}: names the title as written: {r}");
        assert!(mentions_alone(&r, "Plan", "the "), "{what}: and the stripped one, 'Plan': {r}");
        assert!(r.to_lowercase().contains("quot"), "{what}: asks for quotes: {r}");
    }
}

/// T7: a no-match reply suggests quotes — for a thread action (rename) and a
/// document action (move), with no candidate and with close names/titles.
/// The `Failed` reason contains "quot" ("…, or put the exact name in quotes."
/// passes, as does any wording that asks for quotes).
#[tokio::test]
async fn t7_no_match_reply_suggests_quotes() {
    // (request, router answer, what the world holds)
    let cases: &[(&str, String)] = &[
        ("rename Gamma to Delta", rename_route("Gamma", "Delta")),
        ("rename Work to Jobs", rename_route("Work", "Jobs")),
        ("move Submarine to Kitchen", move_route("Submarine", "Kitchen")),
        ("move Report to Kitchen", move_route("Report", "Kitchen")),
    ];
    for (i, (request, route)) in cases.iter().enumerate() {
        let mut w = world(&format!("t7-nomatch-{i}"), 4096, routed(route.clone())).await;
        w.thread("Work Archive").await; // close to "Work"
        let office = w.thread("Office").await;
        w.thread("Kitchen").await;
        w.doc("Quarterly Report Draft", &office, "draft").await; // close to "Report"
        let before = snapshot(&w.store).await;
        let evs = run_with_decision(&mut w, request, ActionDecision::Approve, |_| async {}).await;
        assert!(proposals(&evs).is_empty(), "{request}: no proposal: {evs:?}");
        assert_not_done(&evs, request);
        assert_eq!(snapshot(&w.store).await, before, "{request}: nothing changed");
        let (_, o) = the_outcome(&evs, request);
        let ActionOutcome::Failed { reason } = o else { unreachable!("assert_not_done checked Failed") };
        assert!(reason.to_lowercase().contains("quot"), "{request}: a no-match reply suggests quotes: {reason}");
    }
}

/// T7: quoted names keep trust — `rename 'Alpha' to 'Beta'` after five
/// approvals runs with no proposal, as today.
#[tokio::test]
async fn t7_quoted_names_keep_trust() {
    for (i, request) in ["rename 'Alpha' to 'Beta'", "rename \"Alpha\" to \"Beta\""].iter().enumerate() {
        let mut w = world(&format!("t7-quoted-trust-{i}"), 4096, routed(rename_route("Alpha", "Beta"))).await;
        let alpha = w.thread("Alpha").await;
        t7_runs_trusted(&mut w, "rename_thread", request).await;
        assert_eq!(w.thread_row(&alpha).await.name, "Beta", "{request}: renamed, trusted");
    }
}

/// T7, the boundary, one way: a router `target` taken whole (no split, no
/// stripping) keeps trust — `restore` on "Report" and `adopt` on "Clipping"
/// run with no proposal after five approvals, whatever the request's words.
#[tokio::test]
async fn t7_boundary_router_target_taken_whole_keeps_trust() {
    let mut w = world("t7-whole-restore", 4096, routed(intent_json("restore", Some("Report")))).await;
    let office = w.thread("Office").await;
    let report = versioned_doc(&w, "Report", &office, "report text").await;
    t7_runs_trusted(&mut w, "restore", "restore the report").await;
    assert_eq!(text_of(&w, &report).await, "report text (first)", "restored, trusted");

    let mut w = world("t7-whole-adopt", 4096, routed(intent_json("adopt", Some("Clipping")))).await;
    let office = w.thread("Office").await;
    let clipping = web_doc(&w, "Clipping", &office, "a clipped page").await;
    t7_runs_trusted(&mut w, "adopt", "adopt the clipping").await;
    assert!(is_owned_now(&w.store, &clipping).await, "adopted, trusted");
}

/// T7, the boundary, the other way: `restore`'s split of its target at " to "
/// is a name parsed from unquoted text, even when only one reading exists —
/// target "Trip to Rome", only "Trip" live: the proposal is shown after five
/// approvals (today "Trip" is restored unseen), nothing restored before it.
#[tokio::test]
async fn t7_boundary_restore_target_split_at_to_loses_trust() {
    let mut w = world("t7-split-restore", 4096, routed(intent_json("restore", Some("Trip to Rome")))).await;
    let t = w.thread("Travel").await;
    let trip = versioned_doc(&w, "Trip", &t, "trip text").await;
    let (evs, p) = t7_proposed_under_trust(&mut w, "restore", "restore Trip to Rome").await;
    assert_eq!(p.doc_id.as_deref(), Some(trip.as_str()), "the proposal names 'Trip', the one reading: {p:?}");
    let (success, o) = the_outcome(&evs, "restore Trip to Rome");
    assert!(success && matches!(o, ActionOutcome::Changed { .. }), "approved: done: {o:?}");
    assert_eq!(text_of(&w, &trip).await, "trip text (first)");
}

/// T7, the boundary, the stricter side (reading 7, contract author's
/// decision): any split of unquoted text by the action loses trust, including
/// a rename's router `target` split at " to " (the old path, when the router
/// put both names in `target` and the request holds no quotes nor the
/// action's shape). Target "Alpha to Beta": after five approvals the proposal
/// is shown (today it runs unseen), nothing is written before it; approved,
/// it runs.
#[tokio::test]
async fn t7_boundary_rename_router_target_split_at_to_loses_trust() {
    let mut w = world("t7-split-rename", 4096, routed(intent_json("rename_thread", Some("Alpha to Beta")))).await;
    let alpha = w.thread("Alpha").await;
    let (evs, p) = t7_proposed_under_trust(&mut w, "rename_thread", "rename it").await;
    assert_eq!(p.thread_id.as_deref(), Some(alpha.as_str()), "the proposal carries 'Alpha': {p:?}");
    assert!(p.description.contains("'Beta'"), "and names the new name: {}", p.description);
    let (success, o) = the_outcome(&evs, "rename it");
    assert!(success && matches!(o, ActionOutcome::Changed { .. }), "approved: done: {o:?}");
    assert_eq!(w.thread_row(&alpha).await.name, "Beta");
}

/// T7, the boundary, the stricter side (reading 7), for a move: router
/// `target` "Report to Kitchen", request "move it" — the split at " to " loses
/// trust: the proposal is shown after five approvals (today it runs unseen),
/// nothing is written before it; approved, it runs.
#[tokio::test]
async fn t7_boundary_move_router_target_split_at_to_loses_trust() {
    let mut w = world("t7-split-move", 4096, routed(intent_json("move_document", Some("Report to Kitchen")))).await;
    let office = w.thread("Office").await;
    let kitchen = w.thread("Kitchen").await;
    let report = w.doc("Report", &office, "report text").await;
    let (evs, p) = t7_proposed_under_trust(&mut w, "move_document", "move it").await;
    assert_eq!(p.doc_id.as_deref(), Some(report.as_str()), "the proposal carries 'Report': {p:?}");
    assert!(p.description.contains("Kitchen"), "and names the thread: {}", p.description);
    let (success, o) = the_outcome(&evs, "move it");
    assert!(success && matches!(o, ActionOutcome::Changed { .. }), "approved: done: {o:?}");
    assert_eq!(w.thread_of(&report).await, kitchen);
}

/// T7 follow-up (claude-coord 0163, 0165): only rename, move and merge read
/// their request with quotes, so only their no-match reply suggests quotes
/// (`t7_no_match_reply_suggests_quotes`). The actions that take their target
/// whole — here `open`, `adopt` and `delete_thread` — gain nothing from
/// quotes: their no-match reply, with no candidate and with a close one, does
/// not suggest them (the `Failed` reason does not contain "quot").
#[tokio::test]
async fn t7_no_match_reply_of_a_whole_target_action_does_not_suggest_quotes() {
    // (action, target): "Submarine" has no candidate; "Report" and "Work"
    // have a close one ("Quarterly Report Draft", "Work Archive").
    let cases: &[(&str, &str)] = &[
        ("open", "Submarine"),
        ("open", "Report"),
        ("adopt", "Submarine"),
        ("adopt", "Report"),
        ("delete_thread", "Submarine"),
        ("delete_thread", "Work"),
    ];
    for (i, (action, target)) in cases.iter().enumerate() {
        let what = format!("{action} {target:?}");
        let w = world(&format!("t7-noquote-{i}"), 4096, answer("")).await;
        w.thread("Work Archive").await; // close to "Work"
        let office = w.thread("Office").await;
        if *action == "adopt" {
            web_doc(&w, "Quarterly Report Draft", &office, "draft").await; // close to "Report"
        } else {
            w.doc("Quarterly Report Draft", &office, "draft").await; // close to "Report"
        }
        let before = snapshot(&w.store).await;
        let evs = w.act(action, Some(target)).await;
        assert_not_done(&evs, &what);
        assert_eq!(snapshot(&w.store).await, before, "{what}: nothing changed");
        let (_, o) = the_outcome(&evs, &what);
        let ActionOutcome::Failed { reason } = o else { unreachable!("assert_not_done checked Failed") };
        if *target != "Submarine" {
            assert!(reason.to_lowercase().contains("which"), "{what}: the close case offers a candidate and asks: {reason}");
        }
        assert!(
            !reason.to_lowercase().contains("quot"),
            "{what}: the target is taken whole, so the no-match reply does not suggest quotes: {reason}"
        );
    }
}
