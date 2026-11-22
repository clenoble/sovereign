//! v0.0.13, spec ch. 08 (*Conversational confirmation*): **an external
//! document is never adopted through earned trust** (Céline, 2026-11-17: «
//! jamais d'adoption de doc externe sans action explicite de l'utilisateur,
//! donc jamais via la confiance acquise »). `adopt` is always confirmed.
//!
//! On the direct-request path (the only one that adopts: `adopt` is not a chat
//! write tool), with a verified tracker and five fresh approvals of `adopt`, a
//! request to adopt still shows its proposal, and the document stays external
//! until it is approved. The control: `move_document`, another level-3 action
//! with the same approvals, is auto-approved as before.
//!
//! Written before the code by another author; the implementer does not edit
//! this file.

use super::assistant_harness::*;
use super::*;

/// Five fresh approvals of `action` on the direct-request path: the trust
/// threshold, so a level-3 action is auto-approved from now on.
fn trust_five(w: &World, action: &str) {
    let mut t = w.orch.trust.lock().unwrap();
    for _ in 0..5 {
        t.record_approval(crate::trust::WORKFLOW_QUERY, action);
    }
}

/// The router classifying the request as `action` on `target`.
fn routed(action: &str, target: &str) -> Script {
    let json = intent_json(action, Some(target));
    Arc::new(move |_: &str, _: u32, i: usize| if i == 0 { json.clone() } else { String::new() })
}

/// An external (never owned) document, as a page saved from the browser.
async fn web_doc(w: &World, title: &str, thread: &str) -> String {
    crate::docs_host::create(&w.docs, crate::docs_host::new_web_document(title, thread, "https://example.org/page", "a page"))
        .await
        .unwrap()
        .id
}

async fn is_owned(w: &World, id: &str) -> bool {
    w.store.get_document(id).await.unwrap().is_owned
}

/// Runs `query` through `handle_query`, answering every proposal with
/// `decision` after `on_proposal` (which reads the store at that moment).
/// Returns every event.
async fn run<F, Fut>(w: &mut World, query: &str, decision: ActionDecision, mut on_proposal: F) -> Vec<OrchestratorEvent>
where
    F: FnMut() -> Fut,
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
        for _ in 0..20_000 {
            let new = rec.take_events();
            let n = proposals(&new).len();
            seen.extend(new);
            for _ in 0..n {
                on_proposal().await;
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

/// Adopt is never taken through earned trust: with five fresh approvals of
/// `adopt` on a verified tracker, the request shows its proposal, the page is
/// still external while the proposal waits, and it is adopted once approved.
#[tokio::test]
async fn adopt_shows_its_proposal_whatever_the_trust_earned() {
    let mut w = world("adopt-trust", 4096, routed("adopt", "Page")).await;
    let reading = w.thread("Reading").await;
    let page = web_doc(&w, "Page", &reading).await;
    trust_five(&w, "adopt");

    let seen_external = Arc::new(Mutex::new(vec![]));
    let (store, id, log) = (w.store.clone(), page.clone(), seen_external.clone());
    let evs = run(&mut w, "adopt Page", ActionDecision::Approve, move || {
        let (store, id, log) = (store.clone(), id.clone(), log.clone());
        async move {
            let owned = store.get_document(&id).await.unwrap().is_owned;
            log.lock().unwrap().push(owned);
        }
    })
    .await;

    let ps = proposals(&evs);
    assert_eq!(ps.len(), 1, "adopt shows its proposal, whatever the trust earned: {evs:?}");
    assert_eq!(ps[0].action, "adopt", "{:?}", ps[0]);
    assert_eq!(*seen_external.lock().unwrap(), vec![false], "nothing adopted while the proposal waits");
    assert!(is_owned(&w, &page).await, "adopted once approved");
}

/// The control: another level-3 action with the same approvals on the same
/// verified tracker is auto-approved, as before — no proposal, and it runs.
#[tokio::test]
async fn another_level_3_action_with_the_same_trust_is_auto_approved() {
    let mut w = world("adopt-trust-control", 4096, routed("move_document", "Seeds to Kitchen")).await;
    let garden = w.thread("Garden").await;
    let kitchen = w.thread("Kitchen").await;
    let seeds = w.doc("Seeds", &garden, "seed list").await;
    trust_five(&w, "move_document");
    assert!(
        w.orch.trust.lock().unwrap().should_auto_approve(
            crate::trust::WORKFLOW_QUERY,
            "move_document",
            security::ActionLevel::Modify
        ),
        "precondition: the tracker is verified and the approvals are fresh"
    );

    let evs = run(&mut w, "move 'Seeds' to 'Kitchen'", ActionDecision::Reject("not asked".into()), || async {}).await;
    assert!(proposals(&evs).is_empty(), "auto-approved: no proposal: {evs:?}");
    assert_eq!(w.thread_of(&seeds).await, kitchen, "moved");
}
