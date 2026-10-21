//! S5-5, service side — `TrashEntry` gains the thread id (C-DOC adoption step
//! 5, `doc/plans/C-DOC-adoption.md`).
//!
//! "Trash view. Lists `list_trash` newest first with title, thread name (or
//! the fallback lane's name if the thread is gone), date trashed, days left
//! before purge — and no content. `TrashEntry` gains the thread id
//! (additive)."
//!
//! The hosts' trash views (shell, mobile) name the thread from this id; their
//! tests are in `sovereign-shell` and `sovereign-app` (`step5_tests.rs`).

mod common;

use chrono::Duration;
use common::*;
use sovereign_db::schema::Thread;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_5_a_trash_entry_carries_the_thread_the_document_was_in() {
    let s = Stack::new("s5-5-thread").await;
    let research = s.edb.create_thread(Thread::new("Research".into(), String::new())).await.unwrap();
    let research = research.id_string().unwrap();
    let admin = s.edb.create_thread(Thread::new("Admin".into(), String::new())).await.unwrap();
    let admin = admin.id_string().unwrap();

    let mut a = new_text_doc("A", "alpha");
    a.thread_id = research.clone();
    let a = s.svc.create(a).await.unwrap().id;
    let mut b = new_text_doc("B", "beta");
    b.thread_id = admin.clone();
    let b = s.svc.create(b).await.unwrap().id;

    s.svc.trash(&a).await.unwrap();
    s.clock.advance(Duration::minutes(1));
    s.svc.trash(&b).await.unwrap();

    let trash = s.svc.list_trash().await.unwrap();
    let got: Vec<(String, String)> = trash.iter().map(|e| (e.id.clone(), e.thread_id.clone())).collect();
    assert_eq!(got, vec![(b, admin), (a, research)], "newest first, each with its thread");
}

/// The thread id is kept even when the thread itself is gone: the view then
/// shows the fallback lane, and restoring brings the document back to the same
/// `thread_id` (spec ch. 03, *Thread deletion and the fallback lane*:
/// documents "keep their `thread_id` and are not moved").
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s5_5_a_trash_entry_keeps_the_thread_id_of_a_deleted_thread() {
    let s = Stack::new("s5-5-gone").await;
    let lane = s.edb.create_thread(Thread::new("Lane".into(), String::new())).await.unwrap();
    let lane = lane.id_string().unwrap();
    let mut a = new_text_doc("A", "alpha");
    a.thread_id = lane.clone();
    let a = s.svc.create(a).await.unwrap().id;
    s.svc.trash(&a).await.unwrap();
    s.edb.soft_delete_thread(&lane).await.unwrap();

    let trash = s.svc.list_trash().await.unwrap();
    assert_eq!(trash.len(), 1);
    assert_eq!(trash[0].id, a);
    assert_eq!(trash[0].thread_id, lane);
}
