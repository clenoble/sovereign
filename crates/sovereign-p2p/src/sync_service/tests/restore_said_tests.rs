//! A restore that fails is said (spec ch. 05, *Non-destructive peer
//! overwrite*, « A restore that fails is said »; v0.0.13, item C2).
//!
//! Until v0.0.13 a restore from Previous values that failed, a thread a peer
//! deleted among them, was only written to the log: the host handed it to the
//! sync node and heard nothing back. These tests hold the sync layer's answer,
//! which every host shows where it offered Restore: a failure names the row
//! and says why in plain words, and changes nothing.
//!
//! A child of the sync service's test module, for its two-device helpers.
//!
//! v2: a restore that answers late is not « Not restored » (the Fable maille,
//! by delegation of Céline, 2026-11-18): the host's wait for the node runs out,
//! and the notice says the node may still carry it out.

use super::*;

/// A host's restore through a node that takes the command and never
/// answers: the wait runs out (in paused time, so the test does not wait), the
/// row is named as it is now, the notice is « No answer from sync yet for
/// {what}. It may still be restored. », never « Not restored », and nothing has
/// changed.
#[tokio::test]
async fn c2_a_restore_that_answers_late_is_not_said_as_not_restored() {
    let (a, b) = service_pair("c2-late").await;
    let (tid, rec) = overwritten_thread(&a, &b, "Original", "Hijacked").await;

    let (tx, mut rx) = tokio::sync::mpsc::channel::<crate::node::P2pCommand>(4);
    let silent = tokio::spawn(async move {
        // Takes the command, keeps its reply, and never answers.
        let held = rx.recv().await;
        std::future::pending::<()>().await;
        drop(held);
    });
    tokio::time::pause();
    let outcome = crate::node::restore_previous_value(Some(&tx), b.db.as_ref(), &rec).await;
    tokio::time::resume();
    silent.abort();

    assert_eq!(outcome, RestoreOutcome::NoAnswerYet { what: "Hijacked".into() });
    let notice = outcome.notice().expect("a late answer is said");
    assert_eq!(notice, "No answer from sync yet for Hijacked. It may still be restored.");
    assert!(!notice.contains("Not restored"), "never « Not restored »: {notice}");
    assert_eq!(b.db.get_thread(&tid).await.unwrap().name, "Hijacked", "nothing has changed");
    assert!(still_offered(&b, &rec).await, "the entry stays in Previous values");
}

/// No node, or a channel the node has closed, is « sync is not running », at
/// once: it is not a late answer.
#[tokio::test]
async fn c2_no_node_or_a_closed_channel_is_sync_not_running_at_once() {
    let (a, b) = service_pair("c2-no-node").await;
    let (_tid, rec) = overwritten_thread(&a, &b, "Original", "Hijacked").await;
    let not_running = RestoreOutcome::NotRestored { what: "Hijacked".into(), why: NotRestoredReason::SyncNotRunning };

    let started = std::time::Instant::now();
    assert_eq!(crate::node::restore_previous_value(None, b.db.as_ref(), &rec).await, not_running);
    let (tx, rx) = tokio::sync::mpsc::channel::<crate::node::P2pCommand>(4);
    drop(rx);
    assert_eq!(crate::node::restore_previous_value(Some(&tx), b.db.as_ref(), &rec).await, not_running);
    assert!(
        started.elapsed() < crate::node::RESTORE_ANSWER_WAIT,
        "said at once, not after the wait: {:?}",
        started.elapsed()
    );
}

/// The three reasons, in the words of the spec.
#[test]
fn c2_the_reasons_are_said_in_plain_words() {
    assert_eq!(NotRestoredReason::SyncNotRunning.sentence(), "sync is not running");
    assert_eq!(NotRestoredReason::PreviousUnreadable.sentence(), "the previous value could not be read");
    assert_eq!(NotRestoredReason::WriteRefused.sentence(), "the write was refused");
}

/// « Not restored: {what} — {why} », and no notice for a restore that worked.
#[test]
fn c2_a_failure_is_a_notice_that_names_the_row() {
    let o = RestoreOutcome::NotRestored { what: "Work".into(), why: NotRestoredReason::SyncNotRunning };
    assert_eq!(o.notice().as_deref(), Some("Not restored: Work — sync is not running"));
    let o = RestoreOutcome::NotRestored { what: "Ana".into(), why: NotRestoredReason::WriteRefused };
    assert_eq!(o.notice().as_deref(), Some("Not restored: Ana — the write was refused"));
    assert_eq!(RestoreOutcome::Restored.notice(), None);
}

/// A thread `d` received from `from`, then overwritten by `from` under a new
/// name: Previous values holds its earlier name. Returns (thread id, recovery id).
async fn overwritten_thread(from: &ThreadDev, d: &ThreadDev, before: &str, after: &str) -> (String, String) {
    let tid = from.docs.thread_create(before, "").await.unwrap();
    assert_eq!(pull(from, d, SyncTable::Thread, &[tid.clone()]).await, (1, 0));
    from.docs.thread_update(&tid, Some(after), None).await.unwrap();
    assert_eq!(pull(from, d, SyncTable::Thread, &[tid.clone()]).await, (1, 0));
    assert_eq!(d.db.get_thread(&tid).await.unwrap().name, after);
    let rec = d
        .db
        .list_pending_row_recoveries()
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.row_id == tid && r.table == "thread")
        .expect("Previous values holds the thread's earlier name");
    (tid, rec.id_string().unwrap())
}

/// Whether Previous values still offers `recovery_id`.
async fn still_offered(d: &ThreadDev, recovery_id: &str) -> bool {
    d.db.list_pending_row_recoveries().await.unwrap().iter().any(|r| r.id_string().as_deref() == Some(recovery_id))
}

/// A previous value whose sealed bytes cannot be opened, for row `row_id` of
/// `table` on `d`. Returns the recovery's id.
async fn unreadable_recovery(d: &ThreadDev, row_id: &str, table: &str, from: &PeerId) -> String {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD;
    let ct = b64.encode(b"not a sealed previous value, only bytes");
    let nonce = b64.encode([0u8; 24]);
    d.db.stash_row_recovery(row_id, table, &ct, &nonce, &from.to_string()).await.unwrap()
}

/// The control: a restore that works says so and leaves Previous values.
#[tokio::test]
async fn c2_a_restore_that_works_is_restored() {
    let (a, b) = service_pair("c2-works").await;
    let (tid, rec) = overwritten_thread(&a, &b, "Original", "Hijacked").await;

    assert_eq!(b.svc.restore_previous_value(&rec).await, RestoreOutcome::Restored);
    assert_eq!(b.db.get_thread(&tid).await.unwrap().name, "Original");
    assert!(!still_offered(&b, &rec).await, "a restored entry leaves Previous values");
}

/// A previous value that cannot be read: said, named by the thread's name as
/// it is now, and nothing changes.
#[tokio::test]
async fn c2_an_unreadable_previous_value_is_said_and_changes_nothing() {
    let (a, b) = service_pair("c2-unreadable").await;
    let tid = a.docs.thread_create("Work", "").await.unwrap();
    assert_eq!(pull(&a, &b, SyncTable::Thread, &[tid.clone()]).await, (1, 0));
    let rec = unreadable_recovery(&b, &tid, "thread", &a.peer).await;

    let outcome = b.svc.restore_previous_value(&rec).await;
    assert_eq!(outcome, RestoreOutcome::NotRestored { what: "Work".into(), why: NotRestoredReason::PreviousUnreadable });
    assert_eq!(outcome.notice().as_deref(), Some("Not restored: Work — the previous value could not be read"));
    assert_eq!(b.db.get_thread(&tid).await.unwrap().name, "Work", "the current value stays");
    assert!(still_offered(&b, &rec).await, "the entry stays in Previous values");
}

/// A contact's previous value that cannot be read: named by the contact's name.
#[tokio::test]
async fn c2_a_contact_is_named_by_its_name() {
    let (a, b) = service_pair("c2-contact").await;
    let c = b.db.create_contact(Contact::new("Ana Reyes".into(), false)).await.unwrap();
    let cid = c.id_string().unwrap();
    let rec = unreadable_recovery(&b, &cid, "contact", &a.peer).await;

    let outcome = b.svc.restore_previous_value(&rec).await;
    assert_eq!(outcome, RestoreOutcome::NotRestored { what: "Ana Reyes".into(), why: NotRestoredReason::PreviousUnreadable });
    assert!(still_offered(&b, &rec).await);
}

/// A write that is refused (here, a device whose sync layer has no document
/// service, so no thread can be written): said as « the write was refused »,
/// never with the store's own error, and nothing changes.
#[tokio::test]
async fn c2_a_refused_write_is_said_in_plain_words_and_changes_nothing() {
    let (a, b) = service_pair("c2-refused").await;
    let (tid, rec) = overwritten_thread(&a, &b, "Original", "Hijacked").await;
    let without_docs = SyncService::new(
        b.db.clone() as Arc<dyn GraphDB>,
        b.peer.to_string(),
        [7u8; 32],
        test_keypair(0xB2),
        VersionStore::ephemeral(),
    );

    let outcome = without_docs.restore_previous_value(&rec).await;
    assert_eq!(outcome, RestoreOutcome::NotRestored { what: "Hijacked".into(), why: NotRestoredReason::WriteRefused });
    let notice = outcome.notice().unwrap();
    assert_eq!(notice, "Not restored: Hijacked — the write was refused");
    assert_eq!(b.db.get_thread(&tid).await.unwrap().name, "Hijacked", "the current value stays");
    assert!(still_offered(&b, &rec).await, "the entry stays in Previous values");
}

/// The case the item names: a thread a peer deleted, whose restore fails. It
/// is said, the thread stays deleted, and the entry stays.
#[tokio::test]
async fn c2_a_peer_deleted_thread_that_cannot_come_back_is_said() {
    let (a, b) = service_pair("c2-deleted").await;
    let tid = a.docs.thread_create("Work", "the work thread").await.unwrap();
    assert_eq!(pull(&a, &b, SyncTable::Thread, &[tid.clone()]).await, (1, 0));
    sovereign_docs::delete_thread_to_trash(&a.docs, a.db.as_ref(), &tid).await.unwrap();
    assert_eq!(pull(&a, &b, SyncTable::Thread, &[tid.clone()]).await, (1, 0));
    assert!(b.db.get_thread(&tid).await.unwrap().deleted_at.is_some(), "the deletion applied on B");
    let rec = b
        .db
        .list_pending_row_recoveries()
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.row_id == tid && r.table == "thread")
        .expect("Previous values offers the deleted thread")
        .id_string()
        .unwrap();
    let without_docs = SyncService::new(
        b.db.clone() as Arc<dyn GraphDB>,
        b.peer.to_string(),
        [7u8; 32],
        test_keypair(0xB2),
        VersionStore::ephemeral(),
    );

    let outcome = without_docs.restore_previous_value(&rec).await;
    assert_eq!(outcome, RestoreOutcome::NotRestored { what: "Work".into(), why: NotRestoredReason::WriteRefused });
    assert!(b.db.get_thread(&tid).await.unwrap().deleted_at.is_some(), "the thread stays deleted");
    assert!(still_offered(&b, &rec).await, "the entry stays in Previous values");
}
