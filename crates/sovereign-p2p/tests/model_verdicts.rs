//! C-WIRE 13, spec v2.0.22 (decided by Céline, 2026-10-15): the deterministic
//! checks run during sync; the model's verdict runs afterwards, in the
//! background, for every received document change (first arrivals
//! included), and raises the same alert when it finds something. Sync never
//! waits on the model, and a verdict not reached before a quit is taken up
//! at the next start (the queue is on the document row).
//!
//! Reuses the C-WIRE harness (`wire_common`) read-only; the model is a fake
//! judge, so "busy" and "finds something" are under the test's control.

mod wire_common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use sovereign_docs::{DocumentService, ServiceDeps};
use sovereign_p2p::wire::{ModelJudge, ModelPass, ModelVerdicts};
use wire_common::*;

/// A model that judges a change suspicious when its new text holds
/// "suspicious", optionally holds every call at a gate (a busy model), and
/// counts its calls.
struct Judge {
    calls: AtomicUsize,
    entered: tokio::sync::Notify,
    gate: Option<tokio::sync::Semaphore>,
    available: std::sync::atomic::AtomicBool,
}

impl Judge {
    fn new(busy: bool) -> Arc<Self> {
        Arc::new(Self {
            calls: AtomicUsize::new(0),
            entered: tokio::sync::Notify::new(),
            gate: busy.then(|| tokio::sync::Semaphore::new(0)),
            available: std::sync::atomic::AtomicBool::new(true),
        })
    }
}

#[async_trait]
impl ModelJudge for Judge {
    async fn judge(&self, _prior: &str, incoming: &str) -> Option<Option<String>> {
        if !self.available.load(Ordering::SeqCst) {
            return None;
        }
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.entered.notify_one();
        if let Some(g) = &self.gate {
            g.acquire().await.unwrap().forget();
        }
        Some(incoming.contains("suspicious").then(|| "the model judged the change suspicious".to_string()))
    }
}

async fn two_devices(tag: &str, seed: u8) -> (Device, Device) {
    let (a, b) = (device(&format!("{tag}-a"), seed).await, device(&format!("{tag}-b"), seed + 1).await);
    pair(&a, &b);
    (a, b)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sync_completes_while_the_model_is_busy() {
    let (a, b) = two_devices("mv-busy", 41).await;
    let judge = Judge::new(true);
    b.sync.set_model_verdicts(ModelVerdicts::spawn(b.docs.clone(), judge.clone()));

    let first = create(&a, "First", "ordinary notes").await;
    round(&a, &b).await;
    // The worker took the first change and is now held inside the model.
    tokio::time::timeout(Duration::from_secs(10), judge.entered.notified()).await.expect("the model was asked");

    // While the model is busy, sync goes on: a new document and an edit.
    let second = create(&a, "Second", "more notes").await;
    edit(&a, &first, "ordinary notes, edited").await;
    tokio::time::timeout(Duration::from_secs(10), settle(&a, &b, 4)).await.expect("sync did not wait on the model");
    assert_eq!(stored_text(&b, &second).await, vec!["more notes"]);
    assert_eq!(stored_text(&b, &first).await, vec!["ordinary notes, edited"]);
    assert_eq!(judge.calls.load(Ordering::SeqCst), 1, "still on the first change");

    // Released, the model gets through the queue.
    judge.gate.as_ref().unwrap().add_permits(100);
    tokio::time::timeout(Duration::from_secs(10), async {
        while !b.docs.pending_model_checks().await.unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the queue drains");
    assert!(b.docs.alerts().await.unwrap().is_empty(), "nothing suspicious, no alert");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_background_verdict_raises_an_alert_on_a_first_arrival_and_on_an_edit() {
    let (a, b) = two_devices("mv-alert", 43).await;
    let judge = Judge::new(false);
    let verdicts = ModelVerdicts::new(b.docs.clone(), judge.clone());
    b.sync.set_model_verdicts(verdicts.clone());
    let told = Arc::new(AtomicUsize::new(0));
    let t = told.clone();
    verdicts.set_on_alert(Arc::new(move || {
        t.fetch_add(1, Ordering::SeqCst);
    }));

    // A first arrival the deterministic checks let through.
    let fresh = create(&a, "Fresh", "a suspicious looking note").await;
    let plain = create(&a, "Plain", "an ordinary note").await;
    settle(&a, &b, 4).await;
    assert!(b.docs.alerts().await.unwrap().is_empty(), "the deterministic checks found nothing");
    assert_eq!(b.docs.pending_model_checks().await.unwrap().len(), 2, "both queued, none judged during sync");

    let pass = verdicts.run_pending().await;
    assert_eq!(pass, ModelPass { judged: 2, alerts: 1, superseded: 0, waiting: 0 });
    let alerts = b.docs.alerts().await.unwrap();
    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0].id, fresh);
    assert_eq!(alerts[0].device, a.peer.to_string());
    assert_eq!(alerts[0].previous_version, None, "a first arrival has no previous version");
    assert!(!alerts[0].finding.contains("note"), "no document text in the finding");
    assert_eq!(told.load(Ordering::SeqCst), 1, "the host is told once per alert");

    // An edit of a document already here: the alert names the version before it.
    edit(&a, &plain, "an ordinary note, now suspicious").await;
    settle(&a, &b, 4).await;
    let pass = verdicts.run_pending().await;
    assert_eq!(pass.alerts, 1);
    let on_plain = b.docs.alerts().await.unwrap().into_iter().find(|x| x.id == plain).expect("alert on the edit");
    assert!(on_plain.previous_version.is_some(), "points at the previous version");
    assert!(b.docs.pending_model_checks().await.unwrap().is_empty());
    // Nothing is judged twice.
    assert_eq!(verdicts.run_pending().await, ModelPass::default());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_verdict_not_reached_before_a_quit_is_taken_up_at_the_next_start() {
    let (a, b) = two_devices("mv-quit", 45).await;
    let judge = Judge::new(false);
    // Sync queues; the worker never runs (the app quits first).
    b.sync.set_model_verdicts(ModelVerdicts::new(b.docs.clone(), judge.clone()));
    let id = create(&a, "Late", "a suspicious looking note").await;
    settle(&a, &b, 4).await;
    assert_eq!(judge.calls.load(Ordering::SeqCst), 0);

    // Next start: a new service over the same store, a new worker.
    let restarted = Arc::new(DocumentService::new(ServiceDeps {
        store: b.edb.clone(),
        commit_policy: Arc::new(NeverPolicy),
        pii: b.pii.clone(),
        media: Arc::new(NoMedia),
        clock: b.clock.clone(),
        events: Arc::new(Events::default()),
        device: b.peer.to_string(),
    }));
    let pending = restarted.pending_model_checks().await.unwrap();
    assert_eq!(pending.len(), 1, "the queue survived the quit");
    assert_eq!(pending[0].id, id);
    let pass = ModelVerdicts::new(restarted.clone(), judge.clone()).run_pending().await;
    assert_eq!(pass.judged, 1);
    assert_eq!(restarted.alerts().await.unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn without_a_model_the_queue_waits_and_a_replaced_state_is_noted_not_judged() {
    let (a, b) = two_devices("mv-wait", 47).await;
    let judge = Judge::new(false);
    judge.available.store(false, Ordering::SeqCst);
    let verdicts = ModelVerdicts::new(b.docs.clone(), judge.clone());
    b.sync.set_model_verdicts(verdicts.clone());
    let kept = create(&a, "Kept", "first note").await;
    let replaced = create(&a, "Replaced", "second note").await;
    settle(&a, &b, 4).await;

    // No model yet: nothing judged, nothing dropped.
    let pass = verdicts.run_pending().await;
    assert_eq!(pass.judged, 0);
    assert_eq!(pass.waiting, 2);
    assert_eq!(b.docs.pending_model_checks().await.unwrap().len(), 2);

    // B writes over one received state before any model is there.
    edit(&b, &replaced, "rewritten here").await;
    judge.available.store(true, Ordering::SeqCst);
    let pass = verdicts.run_pending().await;
    assert_eq!(pass.judged, 1, "the state still current is judged");
    assert_eq!(pass.superseded, 1, "the replaced one is noted as not analysed by the model");
    assert!(b.docs.pending_model_checks().await.unwrap().is_empty());
    let _ = kept;
}
