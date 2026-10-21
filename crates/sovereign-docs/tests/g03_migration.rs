//! G3 — migration (C-ENC).
//!
//! Spec ch. 03, *Document service — contract (C-DOC)*, G3: "**Migration commits
//! the prior state first.** A legacy document is converted at its next write
//! and by the one-time migration, each time after a version of its legacy
//! state. The conversion is deterministic — the same legacy content under the
//! same document id gives the same bytes on every device. Migration changes
//! nothing else (title, trust, provenance, thread, times), runs no personal-
//! data ingest, migrates trashed documents without restoring them, leaves
//! read-only documents untouched, and is idempotent."

mod common;

use chrono::{TimeZone, Utc};
use common::*;
use sovereign_db::schema::{raw_to_thing, Document};
use sovereign_db::traits::GraphDB;
use sovereign_docs::{ChangeKind, Trust};

const LEGACY: [&str; 5] = [
    r#"{"body":"envelope body","images":[]}"#,
    r#"{"body":"with media","images":[{"path":"/p/a.png","caption":"A"}],"videos":[{"path":"/p/v.mp4","caption":"V"}]}"#,
    "# raw markdown\n\nbody",
    "",
    "{not json at all",
];

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g03_migrate_all_converts_each_legacy_form_after_a_version_of_it() {
    let s = Stack::new("g03-all").await;
    let mut ids = vec![];
    for (i, c) in LEGACY.iter().enumerate() {
        ids.push((s.legacy(&format!("L{i}"), c).await, *c));
    }
    let report = s.svc.migrate_all().await.expect("migrate");
    for (id, legacy) in &ids {
        assert!(report.migrated.contains(id), "{legacy:?} migrated");
        let (title, stored) = s.stored(id).await;
        assert_v2(&stored);
        let states = s.committed_states(id).await;
        assert!(
            states.iter().any(|(t, c)| t == &title && c == legacy),
            "the legacy state {legacy:?} is a version"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g03_migration_keeps_the_text() {
    let s = Stack::new("g03-text").await;
    let a = s.legacy("A", LEGACY[0]).await;
    let b = s.legacy("B", LEGACY[2]).await;
    let c = s.legacy("C", LEGACY[4]).await;
    s.svc.migrate_all().await.unwrap();
    assert_eq!(texts(&s.stored(&a).await.1), vec!["envelope body"]);
    assert_eq!(texts(&s.stored(&b).await.1), vec!["# raw markdown\n\nbody"]);
    assert_eq!(texts(&s.stored(&c).await.1), vec!["{not json at all"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g03_migrated_media_keep_their_paths_and_need_a_description() {
    let s = Stack::new("g03-media").await;
    let id = s.legacy("M", LEGACY[1]).await;
    s.svc.migrate_all().await.unwrap();
    let blocks = assert_v2(&s.stored(&id).await.1);
    let kinds: Vec<&str> = blocks.iter().map(|b| b["type"].as_str().unwrap()).collect();
    assert_eq!(kinds, vec!["text", "image", "video"]);
    assert_eq!(blocks[1]["ref"]["path"], "/p/a.png");
    assert_eq!(blocks[1]["alt"], "");
    assert_eq!(blocks[2]["ref"]["path"], "/p/v.mp4");
    let v = s.svc.read(&id).await.unwrap();
    assert!(v.doc.as_ref().unwrap().blocks()[1].needs_description());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g03_migration_changes_nothing_but_the_content() {
    let s = Stack::new("g03-meta").await;
    let owned = s.legacy_with("Mine", LEGACY[0], true).await;
    let ext = s.legacy_with("Theirs", LEGACY[2], false).await;
    let before_o = s.stored_doc(&owned).await;
    let before_e = s.stored_doc(&ext).await;
    let view_before = s.svc.read(&ext).await.unwrap();
    s.clock.advance(chrono::Duration::days(3));
    s.svc.migrate_all().await.unwrap();
    for (id, before) in [(&owned, &before_o), (&ext, &before_e)] {
        let after = s.stored_doc(id).await;
        assert_eq!(after.title, before.title);
        assert_eq!(after.thread_id, before.thread_id);
        assert_eq!(after.is_owned, before.is_owned);
        assert_eq!(after.created_at, before.created_at, "creation time kept");
        assert_eq!(after.modified_at, before.modified_at, "timeline position kept");
    }
    let view_after = s.svc.read(&ext).await.unwrap();
    assert_eq!(view_after.trust, view_before.trust);
    assert_eq!(view_after.trust, Trust::External);
    assert_eq!(view_after.provenance, view_before.provenance);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g03_migration_runs_no_ingest() {
    let s = Stack::new("g03-no-ingest").await;
    for c in LEGACY {
        s.legacy("L", c).await;
    }
    s.svc.migrate_all().await.unwrap();
    assert!(s.pii.requests().is_empty(), "the text did not change; ingest is not run");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g03_migration_is_idempotent() {
    let s = Stack::new("g03-idem").await;
    for c in LEGACY {
        s.legacy("L", c).await;
    }
    s.svc.migrate_all().await.unwrap();
    let docs = s.edb.list_documents_including_deleted().await.unwrap();
    let mut before = vec![];
    for d in &docs {
        let id = d.id_string().unwrap();
        before.push((s.stored(&id).await, s.commits(&id).await.len()));
    }
    s.events.take();
    let second = s.svc.migrate_all().await.unwrap();
    assert!(second.migrated.is_empty(), "nothing left to migrate");
    assert_eq!(second.already_v2, docs.len());
    for (d, b) in docs.iter().zip(before) {
        let id = d.id_string().unwrap();
        assert_eq!((s.stored(&id).await, s.commits(&id).await.len()), b);
    }
    assert!(s.events.peek().is_empty(), "a second run emits nothing");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g03_documents_created_by_the_service_are_not_migrated() {
    let s = Stack::new("g03-native").await;
    let id = s.create_text("Native", "v2 from birth").await;
    let n = s.commits(&id).await.len();
    let report = s.svc.migrate_all().await.unwrap();
    assert!(!report.migrated.contains(&id));
    assert_eq!(s.commits(&id).await.len(), n);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g03_a_trashed_legacy_document_is_migrated_and_stays_trashed() {
    let s = Stack::new("g03-trashed").await;
    let id = s.legacy("Old", LEGACY[0]).await;
    s.svc.trash(&id).await.unwrap();
    let report = s.svc.migrate_all().await.unwrap();
    assert!(report.migrated.contains(&id));
    assert_v2(&s.stored(&id).await.1);
    let v = s.svc.read(&id).await.unwrap();
    assert!(v.trashed_at.is_some(), "still in the trash");
    assert!(s.svc.list(None).await.unwrap().iter().all(|d| d.id != id));
}

/// Two devices migrating the same legacy document independently must agree
/// byte for byte, or sync would see a conflict for every migrated document.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g03_two_devices_migrate_the_same_legacy_document_to_the_same_bytes() {
    let mut stored = vec![];
    for tag in ["g03-dev-a", "g03-dev-b"] {
        let s = Stack::new(tag).await;
        for (i, c) in LEGACY.iter().enumerate() {
            let mut d = Document::new(format!("L{i}"), "thread:t1".into(), true);
            d.id = raw_to_thing(&format!("document:shared{i}"));
            d.content = c.to_string();
            d.created_at = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
            d.modified_at = d.created_at;
            assert!(s.edb.create_document_with_id(d).await.unwrap());
        }
        s.svc.migrate_all().await.unwrap();
        let mut mine = vec![];
        for i in 0..LEGACY.len() {
            mine.push(s.stored(&format!("document:shared{i}")).await.1);
        }
        stored.push(mine);
    }
    assert_eq!(stored[0], stored[1]);
}

/// The lazy path: the first write to a legacy document converts it after a
/// version of the legacy state.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g03_first_write_to_a_legacy_document_versions_the_legacy_state_first() {
    let s = Stack::new("g03-lazy").await;
    for legacy in LEGACY {
        let id = s.legacy("Lazy", legacy).await;
        s.assistant_save(&id, "new text").await.expect("save");
        let states = s.committed_states(&id).await;
        assert!(
            states.iter().any(|(_, c)| c == legacy),
            "legacy state {legacy:?} kept as a version"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g03_migration_emits_one_migrated_event_per_migrated_document() {
    let s = Stack::new("g03-events").await;
    let a = s.legacy("A", LEGACY[0]).await;
    let b = s.legacy("B", LEGACY[2]).await;
    s.legacy("Future", r#"{"format":"sovereign.doc","v":4,"blocks":[]}"#).await;
    s.svc.migrate_all().await.unwrap();
    let mut ev: Vec<_> = s.events.take().into_iter().map(|e| (e.id, e.kind)).collect();
    ev.sort_by(|x, y| x.0.cmp(&y.0));
    let mut want = vec![(a, ChangeKind::Migrated), (b, ChangeKind::Migrated)];
    want.sort_by(|x, y| x.0.cmp(&y.0));
    assert_eq!(ev, want);
}
