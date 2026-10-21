//! G5 — every document has a version from its creation.
//!
//! Spec ch. 03, *Document service — contract (C-DOC)*, G5: "**Every document
//! has a version from its creation**: create makes exactly one version, of the
//! state it stored, whatever the origin (editor, assistant, skill, import) and
//! whatever the commit policy."

mod common;

use chrono::{TimeZone, Utc};
use common::*;
use sovereign_docs::{EditSession, Origin, Provenance, Trust};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g05_create_makes_exactly_one_version_of_what_it_stored() {
    let s = Stack::new("g05-one").await;
    let out = s.svc.create(new_text_doc("First", "hello")).await.unwrap();
    let commits = s.commits(&out.id).await;
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].id_string().unwrap(), out.version, "the outcome names it");
    assert_eq!(
        (commits[0].snapshot.title.clone(), commits[0].snapshot.content.clone()),
        s.stored(&out.id).await
    );
    assert_eq!(s.stored_doc(&out.id).await.head_commit.as_deref(), Some(out.version.as_str()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g05_every_origin_gets_an_initial_version_even_under_a_never_policy() {
    let s = Stack::new("g05-origins").await;
    s.policy.set(PolicyMode::Never);
    for origin in [
        Origin::Editor { session: EditSession(1) },
        Origin::Assistant,
        Origin::Skill { name: "daily_journal".into() },
        Origin::Import,
    ] {
        let mut new = new_text_doc("T", "x");
        new.origin = origin.clone();
        let id = s.svc.create(new).await.unwrap().id;
        assert_eq!(s.commits(&id).await.len(), 1, "{origin:?}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g05_import_keeps_original_times_and_still_has_a_version() {
    let s = Stack::new("g05-import").await;
    let created = Utc.with_ymd_and_hms(2019, 5, 4, 3, 2, 1).unwrap();
    let modified = Utc.with_ymd_and_hms(2021, 6, 7, 8, 9, 10).unwrap();
    let mut new = new_text_doc("Old file", "from 2019");
    new.origin = Origin::Import;
    new.trust = Trust::Undetermined;
    new.provenance = Provenance::Import { source_path: "D:/archive/old.md".into() };
    new.created_at = Some(created);
    new.modified_at = Some(modified);
    let id = s.svc.create(new).await.unwrap().id;
    let d = s.stored_doc(&id).await;
    assert_eq!(d.created_at, created);
    assert_eq!(d.modified_at, modified);
    assert_eq!(s.commits(&id).await.len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g05_a_document_created_by_the_clock_is_dated_by_it() {
    let s = Stack::new("g05-clock").await;
    let id = s.create_text("T", "x").await;
    let d = s.stored_doc(&id).await;
    assert_eq!(d.created_at, s.clock.get());
    assert_eq!(d.modified_at, s.clock.get());
}
