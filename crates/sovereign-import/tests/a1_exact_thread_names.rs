//! v0.0.12 item 6, **A1 reaches every caller** — the import's
//! folder-to-thread (`doc/plans/V0012-lot-assistant.md`, "Readings settled
//! with the test author", 1 and 3). `execute` resolved each folder's thread
//! with the single-match lookup: a folder "Work" could land in "Work Archive".
//! The import is a find-or-create caller: no exact match → the thread is
//! created; two live threads with the exact name → refused with an error.
//!
//! Written before the code, by another author; the implementer does not edit
//! this file.

use std::path::Path;

use sovereign_ai::docs_host::{build_document_service, testing};
use sovereign_db::GraphDB;
use sovereign_import::{execute, plan, ImportOptions};

fn write(dir: &Path, rel: &str, contents: &[u8]) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, contents).unwrap();
}

struct Db {
    store: std::sync::Arc<sovereign_db::encrypted::EncryptedGraphDB>,
    docs: sovereign_docs::DocumentService,
}

async fn fresh(tag: &str) -> Db {
    let store = testing::encrypted_memory_store(tag).await;
    let docs = build_document_service(store.clone(), Some(testing::account_key()), "test".into());
    Db { store, docs }
}

/// A folder "Work" holding one note, planned as the thread "Work".
fn work_folder() -> (tempfile::TempDir, sovereign_import::Manifest) {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "Work/memo.md", b"# Memo\nthe weekly memo");
    let m = plan(tmp.path(), &ImportOptions::default()).unwrap();
    assert_eq!(m.threads, vec!["Work".to_string()], "the folder plans the thread \"Work\"");
    (tmp, m)
}

async fn titles_in(db: &Db, thread: &str) -> Vec<String> {
    db.docs.list(Some(thread)).await.unwrap().into_iter().map(|d| d.title).collect()
}

/// A1/3: "Work Archive" present, no "Work": the import creates "Work" and
/// files the note there, never in "Work Archive".
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a1_import_folder_work_never_lands_in_work_archive() {
    let db = fresh("a1-import-archive").await;
    let archive = db.docs.thread_create("Work Archive", "").await.unwrap();
    let (_tmp, m) = work_folder();
    let out = execute(&*db.store, &db.docs, &m).await.unwrap();

    assert_eq!((out.threads_created, out.threads_reused), (1, 0), "\"Work\" is created, nothing reused");
    assert!(titles_in(&db, &archive).await.is_empty(), "nothing lands in Work Archive");
    let work: Vec<String> = db
        .store
        .list_threads()
        .await
        .unwrap()
        .into_iter()
        .filter(|t| t.deleted_at.is_none() && t.name == "Work")
        .filter_map(|t| t.id_string())
        .collect();
    assert_eq!(work.len(), 1, "one thread named Work");
    assert_eq!(titles_in(&db, &work[0]).await.len(), 1, "the note is in Work");
}

/// A1: with both present, the folder "Work" reuses "Work" (exact name).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a1_import_folder_work_reuses_the_exact_thread() {
    let db = fresh("a1-import-both").await;
    let archive = db.docs.thread_create("Work Archive", "").await.unwrap();
    let work = db.docs.thread_create("Work", "").await.unwrap();
    let (_tmp, m) = work_folder();
    let out = execute(&*db.store, &db.docs, &m).await.unwrap();

    assert_eq!((out.threads_created, out.threads_reused), (0, 1), "\"Work\" is reused");
    assert_eq!(titles_in(&db, &work).await.len(), 1, "the note is in Work");
    assert!(titles_in(&db, &archive).await.is_empty(), "nothing lands in Work Archive");
}

/// A1/3: two live threads named exactly "Work": the import is refused with an
/// error, and the note lands in neither.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a1_import_refuses_two_threads_with_the_exact_name() {
    let db = fresh("a1-import-two").await;
    let first = db.docs.thread_create("Work", "").await.unwrap();
    let second = db.docs.thread_create("Work", "").await.unwrap();
    let (_tmp, m) = work_folder();
    let res = execute(&*db.store, &db.docs, &m).await;

    assert!(res.is_err(), "two threads named Work: refused, none picked ({:?})", res.map(|o| (o.threads_reused, o.docs_created)));
    assert!(titles_in(&db, &first).await.is_empty() && titles_in(&db, &second).await.is_empty(), "the note lands in neither");
}
