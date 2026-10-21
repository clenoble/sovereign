//! G30 — a backup restores only onto an empty workspace.
//!
//! Spec ch. 03, *Document service — contract (C-DOC)*, G30 (specified
//! 2026-10-15, for C-WIRE; Céline's decision the same day): "`restore_backup`
//! refuses a workspace that holds any document (trash included), writes nothing
//! and says why. On an empty workspace every backed-up document comes back with
//! exactly one version, 'Restored from backup', its trust, decision, provenance,
//! times and trash state as backed up; the purge markers in the backup are
//! recorded."
//!
//! C-WIRE guarantee 17 (spec ch. 05).

mod common;

use chrono::{TimeZone, Utc};
use common::*;
use sovereign_docs::{BackupDocument, BackupError, Provenance, Trust, WireApplied};

/// A backup of two documents, one of them trashed, made on another device.
async fn a_backup() -> (Vec<BackupDocument>, Vec<sovereign_docs::PurgeMarkerWire>, Vec<String>, sovereign_docs::WireState) {
    let src = Stack::with_device("g30-source", "laptop").await;
    let mut new = new_text_doc("Imported", "kept");
    new.trust = Trust::Undetermined;
    new.provenance = Provenance::Import { source_path: "D:/a.md".into() };
    new.origin = sovereign_docs::Origin::Import;
    new.created_at = Some(Utc.with_ymd_and_hms(2016, 1, 1, 0, 0, 0).unwrap());
    let a = src.svc.create(new).await.unwrap().id;
    let b = src.create_text("In the trash", "x").await;
    src.svc.trash(&b).await.unwrap();
    let gone = src.create_text("Purged", "erased").await;
    let gone_state = src.svc.stored_state(&gone).await.unwrap();
    src.svc.trash(&gone).await.unwrap();
    src.svc.purge_from_trash(&gone).await.unwrap();
    let docs = src
        .svc
        .stored_states()
        .await
        .unwrap()
        .into_iter()
        .map(|w| BackupDocument { document: w.document, meta: w.meta })
        .collect();
    let markers = src.svc.purge_markers().await.unwrap();
    let _ = gone;
    (docs, markers, vec![a, b], gone_state)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g30_a_workspace_that_holds_any_document_refuses_the_restore() {
    let (docs, markers, ids, _) = a_backup().await;
    for trashed in [false, true] {
        let s = Stack::new(&format!("g30-refused-{trashed}")).await;
        let mine = s.create_text("Mine", "x").await;
        if trashed {
            s.svc.trash(&mine).await.unwrap();
        }
        s.events.take();
        let r = s.svc.restore_backup(docs.clone(), markers.clone()).await;
        match r {
            Err(BackupError::NotEmpty { documents }) => assert_eq!(documents, 1),
            other => panic!("expected a refusal, got {:?}", other.map(|r| r.restored)),
        }
        assert!(BackupError::NotEmpty { documents: 1 }.to_string().contains("sync"), "it says why");
        for id in &ids {
            assert!(s.svc.read(id).await.is_err(), "nothing restored");
        }
        assert!(s.events.peek().is_empty());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g30_on_an_empty_workspace_everything_comes_back_with_one_version() {
    let (docs, markers, ids, _) = a_backup().await;
    let s = Stack::new("g30-empty").await;
    let report = s.svc.restore_backup(docs.clone(), markers).await.unwrap();
    let mut restored = report.restored.clone();
    restored.sort();
    let mut want = ids.clone();
    want.sort();
    assert_eq!(restored, want);
    for d in &docs {
        let id = &d.document.id;
        let commits = s.commits(id).await;
        assert_eq!(commits.len(), 1, "{id}: one version");
        assert!(commits[0].message.to_lowercase().contains("backup"), "{}", commits[0].message);
        let v = s.svc.read(id).await.unwrap();
        assert_eq!(Some(v.trust), d.document.trust);
        assert_eq!(Some(v.provenance.clone()), d.document.provenance);
        assert_eq!(v.trashed_at.is_some(), d.document.deleted_at.is_some(), "{id}: trash state as backed up");
        assert_eq!(Some(v.created_at), d.meta.created_at);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g30_the_backups_purge_markers_are_recorded() {
    let (docs, markers, _, gone) = a_backup().await;
    assert!(!markers.is_empty());
    let s = Stack::new("g30-markers").await;
    s.svc.restore_backup(docs, markers.clone()).await.unwrap();
    let id = gone.document.id.clone();
    assert!(s.svc.is_purged(&id).await.unwrap());
    for m in markers {
        assert!(s.svc.purge_markers().await.unwrap().contains(&m));
    }
    // The purged version, offered again later by a peer, is ignored.
    let out = s.svc.apply_wire(gone.document, gone.meta).await.unwrap();
    assert_eq!(out.applied, WireApplied::Ignored);
    assert!(s.svc.read(&id).await.is_err());
}
