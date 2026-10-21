//! G29 — the stored state, for sending.
//!
//! Spec ch. 03, *Document service — contract (C-DOC)*, G29 (specified
//! 2026-10-15, for C-WIRE): "`stored_state` gives sync everything that defines
//! a document — title, thread, content as stored, trash state, trust, the last
//! trust decision with its stamp, provenance, creation time, stamps, author —
//! without sync reading `content` itself. Applied on a device that does not
//! have the document, it reproduces it: same normalised hash, same trust,
//! decision, provenance and creation time. Trashed documents are included;
//! a purged one is `Purged`."
//!
//! C-WIRE guarantee 1 (spec ch. 05): "Everything that defines a document
//! travels."

mod common;

use chrono::{TimeZone, Utc};
use common::wire::*;
use common::*;
use sovereign_docs::{DocError, Origin, Provenance, Trust};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g29_a_state_sent_to_an_empty_device_arrives_whole() {
    let a = Stack::with_device("g29-a", "laptop").await;
    let b = Stack::with_device("g29-b", "phone").await;
    let created = Utc.with_ymd_and_hms(2017, 6, 1, 0, 0, 0).unwrap();
    let mut new = new_text_doc("Imported", "body");
    new.trust = Trust::Undetermined;
    new.provenance = Provenance::Import { source_path: "D:/in/a.md".into() };
    new.origin = Origin::Import;
    new.created_at = Some(created);
    let id = a.svc.create(new).await.unwrap().id;
    a.svc.set_trust(&id, Trust::Owned).await.unwrap();
    let copy = a.svc.duplicate(sovereign_docs::Duplicate {
        source: id.clone(),
        title: None,
        thread_id: None,
        origin: Origin::Editor { session: sovereign_docs::EditSession(1) },
    })
    .await
    .unwrap()
    .id;
    for doc in [&id, &copy] {
        send(&a, &b, doc).await;
        let (sa, sb) = (a.svc.stored_state(doc).await.unwrap(), b.svc.stored_state(doc).await.unwrap());
        assert_eq!(sa.normalized_hash, sb.normalized_hash, "{doc}");
        let (va, vb) = (a.svc.read(doc).await.unwrap(), b.svc.read(doc).await.unwrap());
        assert_eq!(va.trust, vb.trust, "{doc}");
        assert_eq!(va.trust_decision, vb.trust_decision, "{doc}");
        assert_eq!(va.provenance, vb.provenance, "{doc}: provenance travels, copies included");
        assert_eq!(va.created_at, vb.created_at, "{doc}");
        assert_eq!(sa.meta.decision_stamp, sb.meta.decision_stamp, "{doc}");
    }
    assert_eq!(b.svc.read(&id).await.unwrap().created_at, created);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g29_trashed_documents_are_exported_purged_ones_are_not() {
    let s = Stack::new("g29-trash").await;
    let t = s.create_text("Trashed", "x").await;
    s.svc.trash(&t).await.unwrap();
    assert!(s.svc.stored_state(&t).await.unwrap().document.deleted_at.is_some());
    let p = s.create_text("Purged", "x").await;
    s.svc.trash(&p).await.unwrap();
    s.svc.purge_from_trash(&p).await.unwrap();
    assert!(matches!(s.svc.stored_state(&p).await, Err(DocError::Purged(_))));
    let ids: Vec<String> = s.svc.stored_states().await.unwrap().into_iter().map(|w| w.document.id).collect();
    assert!(ids.contains(&t) && !ids.contains(&p));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g29_the_export_is_this_device_speaking() {
    let s = Stack::with_device("g29-device", "desktop").await;
    let id = s.create_text("T", "x").await;
    assert_eq!(s.svc.stored_state(&id).await.unwrap().document.device, "desktop");
}
