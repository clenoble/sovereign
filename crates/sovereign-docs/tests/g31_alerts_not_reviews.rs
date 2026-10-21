//! G31 — no review flag; alerts raised by the analysis, listed until
//! dismissed.
//!
//! Spec ch. 03, *Document service — contract (C-DOC)*, G31 (specified
//! 2026-10-15, for C-WIRE; Céline's decision the same day, option (b)):
//! "Applying a remote state raises no review flag. An alert from the
//! incoming-change analysis is recorded with the document, the device, the
//! finding and the previous version; it is listed until dismissed;
//! dismissing changes nothing in the document."
//!
//! C-WIRE guarantee 13 (spec ch. 05): "Alerts, not reviews."

mod common;

use common::wire::*;
use common::*;
use sovereign_docs::IncomingAlert;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g31_applying_a_remote_state_raises_no_review_flag() {
    let s = Stack::new("g31-no-flag").await;
    let id = s.create_text("T", "x").await;
    let local = current_stamp(&s, &id).await;
    s.svc
        .apply_wire(remote_text(&id, "T", "changed on the phone", "phone"), meta(Some(stamp(&s, 1, 0, "phone")), Some(local)))
        .await
        .unwrap();
    assert!(!s.svc.read(&id).await.unwrap().peer_review_pending);
    assert!(!s.stored_doc(&id).await.peer_review_pending);
    let local = current_stamp(&s, &id).await;
    let mut tomb = remote_text(&id, "T", "changed on the phone", "phone");
    tomb.deleted_at = Some(s.clock.get());
    s.svc.apply_wire(tomb, meta(Some(stamp(&s, 2, 0, "phone")), Some(local))).await.unwrap();
    assert!(!s.stored_doc(&id).await.peer_review_pending, "a deletion raises no flag either");
    assert!(s.svc.alerts().await.unwrap().is_empty(), "nothing raises an alert by itself");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g31_an_alert_is_listed_until_dismissed_and_dismissing_changes_nothing() {
    let s = Stack::new("g31-alert").await;
    let id = s.create_text("T", "x").await;
    let prior = s.commits(&id).await[0].id_string().unwrap();
    let before = s.stored_doc(&id).await;
    let versions = s.commits(&id).await.len();
    let alert = IncomingAlert {
        id: id.clone(),
        device: "phone".into(),
        finding: "incoming content contains prompt-injection patterns".into(),
        previous_version: Some(prior),
    };
    s.svc.raise_alert(alert.clone()).await.unwrap();
    assert_eq!(s.svc.alerts().await.unwrap(), vec![alert]);
    s.svc.dismiss_alert(&id).await.unwrap();
    assert!(s.svc.alerts().await.unwrap().is_empty());
    let after = s.stored_doc(&id).await;
    assert_eq!((after.title, after.content, after.modified_at), (before.title, before.content, before.modified_at));
    assert_eq!(s.commits(&id).await.len(), versions);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g31_an_alert_for_a_purged_document_disappears_with_it() {
    let s = Stack::new("g31-purged").await;
    let id = s.create_text("T", "x").await;
    s.svc
        .raise_alert(IncomingAlert { id: id.clone(), device: "phone".into(), finding: "rewrite".into(), previous_version: None })
        .await
        .unwrap();
    s.svc.trash(&id).await.unwrap();
    s.svc.empty_trash().await.unwrap();
    assert!(s.svc.alerts().await.unwrap().iter().all(|a| a.id != id));
}
