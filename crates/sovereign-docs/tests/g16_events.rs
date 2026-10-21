//! G16 — `DocumentChanged{id, kind}`.
//!
//! Spec ch. 03, *Document service — contract (C-DOC)*, G16: "**One event per
//! change.** `DocumentChanged{id, kind}` is emitted exactly once per completed
//! change, after the change is stored — a listener that reads the document on
//! the event sees it — and never for a read, a refused, failed or identical
//! write."

mod common;

use std::sync::{Arc, Mutex};

use common::*;
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::traits::GraphDB;
use sovereign_docs::{
    ChangeKind, DocumentChanged, DocumentEvents, DocumentService, EditContent, RemoteDocument,
    ServiceDeps, Trust,
};

fn kinds(ev: Vec<DocumentChanged>, id: &str) -> Vec<ChangeKind> {
    ev.into_iter().filter(|e| e.id == id).map(|e| e.kind).collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g16_each_operation_emits_its_one_event() {
    let s = Stack::new("g16-each").await;
    let id = s.create_text("T", "x").await;
    assert_eq!(kinds(s.events.take(), &id), vec![ChangeKind::Created]);

    s.editor_save(&id, 1, v2(vec![text_block(1, "y")])).await.unwrap();
    assert_eq!(kinds(s.events.take(), &id), vec![ChangeKind::Saved]);

    let base = s.revision(&id).await;
    s.svc.apply_skill_output(&id, base, "redactor", EditContent::PlainText("z".into())).await.unwrap();
    assert_eq!(kinds(s.events.take(), &id), vec![ChangeKind::Saved]);

    let v1 = s.commits(&id).await.last().unwrap().id_string().unwrap();
    s.svc.restore_version(&id, &v1).await.unwrap();
    assert_eq!(kinds(s.events.take(), &id), vec![ChangeKind::VersionRestored]);

    s.svc.set_trust(&id, Trust::External).await.unwrap();
    assert_eq!(kinds(s.events.take(), &id), vec![ChangeKind::TrustChanged]);

    s.svc
        .apply_remote(RemoteDocument {
            id: id.clone(),
            title: "T".into(),
            content: v2(vec![text_block(1, "remote")]),
            thread_id: "thread:t1".into(),
            trust: Some(Trust::External),
            provenance: None,
            modified_at: s.clock.get(),
            deleted_at: None,
            device: "phone".into(),
            trust_decision: None,
        })
        .await
        .unwrap();
    assert_eq!(kinds(s.events.take(), &id), vec![ChangeKind::RemoteApplied]);

    s.svc.trash(&id).await.unwrap();
    assert_eq!(kinds(s.events.take(), &id), vec![ChangeKind::Trashed]);
    s.svc.restore_from_trash(&id).await.unwrap();
    assert_eq!(kinds(s.events.take(), &id), vec![ChangeKind::RestoredFromTrash]);
    s.svc.trash(&id).await.unwrap();
    s.events.take();
    s.svc.empty_trash().await.unwrap();
    assert_eq!(kinds(s.events.take(), &id), vec![ChangeKind::Purged]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g16_a_document_created_by_sync_emits_created() {
    let s = Stack::new("g16-sync-create").await;
    s.svc
        .apply_remote(RemoteDocument {
            id: "document:new1".into(),
            title: "N".into(),
            content: v2(vec![text_block(1, "n")]),
            thread_id: "thread:t1".into(),
            trust: Some(Trust::Owned),
            provenance: None,
            modified_at: s.clock.get(),
            deleted_at: None,
            device: "phone".into(),
            trust_decision: None,
        })
        .await
        .unwrap();
    assert_eq!(kinds(s.events.take(), "document:new1"), vec![ChangeKind::Created]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g16_nothing_is_emitted_for_reads_refusals_failures_or_identical_writes() {
    let mut s = Stack::new("g16-silence").await;
    let id = s.create_text("T", "x").await;
    let stale = s.revision(&id).await;
    s.editor_save(&id, 1, v2(vec![text_block(1, "y")])).await.unwrap();
    s.events.take();
    // reads
    s.svc.read(&id).await.unwrap();
    s.svc.list(None).await.unwrap();
    s.svc.versions(&id).await.unwrap();
    s.svc.list_trash().await.unwrap();
    // identical
    s.editor_save(&id, 1, v2(vec![text_block(1, "y")])).await.unwrap();
    s.svc.set_trust(&id, Trust::Owned).await.unwrap(); // already owned
    // refused
    let _ = s
        .svc
        .apply_skill_output(&id, stale, "x", EditContent::PlainText("q".into()))
        .await;
    let _ = s.svc.restore_from_trash(&id).await;
    // failed
    let base = s.revision(&id).await;
    s.faulty.crash_after(0);
    let r = s
        .svc
        .save(&id, sovereign_docs::Edit {
            base,
            title: None,
            content: Some(EditContent::PlainText("never".into())),
            origin: sovereign_docs::Origin::Assistant,
            explicit: false,
            pasted_tokens: vec![],
        })
        .await;
    assert!(r.is_err(), "every store call fails");
    s.restart();
    assert!(s.events.peek().is_empty(), "{:?}", s.events.peek());
}

/// A listener that reads the document when told it changed.
struct Probe {
    edb: Arc<EncryptedGraphDB>,
    seen: Mutex<Vec<(ChangeKind, Option<String>)>>,
}

impl DocumentEvents for Probe {
    fn emit(&self, event: DocumentChanged) {
        let edb = self.edb.clone();
        let id = event.id.clone();
        let content = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move { edb.get_document(&id).await.ok().map(|d| d.content) })
        });
        self.seen.lock().unwrap().push((event.kind, content));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g16_the_event_comes_after_the_change_is_stored() {
    let s = Stack::new("g16-after").await;
    let probe = Arc::new(Probe { edb: s.edb.clone(), seen: Mutex::new(vec![]) });
    let svc = DocumentService::new(ServiceDeps {
        store: s.edb.clone(),
        commit_policy: s.policy.clone(),
        pii: s.pii.clone(),
        media: s.media.clone(),
        clock: s.clock.clone(),
        events: probe.clone(),
        device: DEVICE.into(),
    });
    let id = svc.create(new_text_doc("T", "first")).await.unwrap().id;
    let base = svc.read(&id).await.unwrap().revision;
    svc.save(&id, sovereign_docs::Edit {
        base,
        title: None,
        content: Some(EditContent::PlainText("second".into())),
        origin: sovereign_docs::Origin::Assistant,
        explicit: false,
        pasted_tokens: vec![],
    })
    .await
    .unwrap();
    let seen = probe.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].0, ChangeKind::Created);
    assert_eq!(texts(seen[0].1.as_deref().expect("created doc readable on the event")), vec!["first"]);
    assert_eq!(seen[1].0, ChangeKind::Saved);
    assert_eq!(texts(seen[1].1.as_deref().unwrap()), vec!["second"]);
}
