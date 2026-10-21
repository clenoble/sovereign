//! G4 — never overwrite without a restore point; a write lands whole or not at all.
//!
//! Spec ch. 03, *Document service — contract (C-DOC)*, G4: "**Never overwrite
//! without a restore point.** Before a stored state is replaced by a write from
//! anything other than the editing session that wrote it — another session,
//! the assistant, a skill, import, sync, a restore, the migration — that state
//! is a version. A write lands whole (title and content together) or not at
//! all: after a failure at any point, the document reads as its prior state or
//! its new state, and the prior state is recoverable."
//!
//! The crash tests use `FaultyDb` under the encrypted layer: from call k on,
//! every store call fails — the process "stops" at an arbitrary point. Every k
//! is tried until the write completes without reaching it.

mod common;

use common::*;
use sovereign_docs::{Edit, EditContent, EditSession, Origin};

/// Is (title, content) the current state or one of the versions?
async fn recoverable(s: &Stack, id: &str, state: &(String, String)) -> bool {
    &s.stored(id).await == state || s.committed_states(id).await.contains(state)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g04_a_save_from_another_session_versions_the_previous_session_state() {
    let s = Stack::new("g04-sessions").await;
    let id = s.create_text("T", "start").await;
    s.editor_save(&id, 1, v2(vec![text_block(1, "session one")])).await.unwrap();
    let s1 = s.stored(&id).await;
    s.editor_save(&id, 2, v2(vec![text_block(1, "session two")])).await.unwrap();
    assert!(s.committed_states(&id).await.contains(&s1));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g04_every_non_editor_writer_versions_what_it_replaces() {
    let s = Stack::new("g04-writers").await;
    let id = s.create_text("T", "start").await;

    s.editor_save(&id, 1, v2(vec![text_block(1, "typed by the user")])).await.unwrap();
    let before_assistant = s.stored(&id).await;
    s.assistant_save(&id, "assistant").await.unwrap();
    assert!(s.committed_states(&id).await.contains(&before_assistant), "assistant");

    s.editor_save(&id, 3, v2(vec![text_block(1, "typed again")])).await.unwrap();
    let before_skill = s.stored(&id).await;
    let base = s.revision(&id).await;
    s.svc
        .apply_skill_output(&id, base, "find_replace", EditContent::PlainText("skill".into()))
        .await
        .unwrap();
    assert!(s.committed_states(&id).await.contains(&before_skill), "skill");

    s.editor_save(&id, 4, v2(vec![text_block(1, "typed a third time")])).await.unwrap();
    let before_restore = s.stored(&id).await;
    let oldest = s.commits(&id).await.last().unwrap().id_string().unwrap();
    s.svc.restore_version(&id, &oldest).await.unwrap();
    assert!(s.committed_states(&id).await.contains(&before_restore), "restore");

    s.editor_save(&id, 5, v2(vec![text_block(1, "typed before sync")])).await.unwrap();
    let before_sync = s.stored(&id).await;
    s.svc
        .apply_remote(sovereign_docs::RemoteDocument {
            id: id.clone(),
            title: "T".into(),
            content: v2(vec![text_block(1, "from the phone")]),
            thread_id: "thread:t1".into(),
            trust: Some(sovereign_docs::Trust::Owned),
            provenance: None,
            modified_at: s.clock.get() + chrono::Duration::minutes(1),
            deleted_at: None,
            device: "phone".into(),
            trust_decision: None,
        })
        .await
        .unwrap();
    assert!(s.committed_states(&id).await.contains(&before_sync), "sync");
}

/// A sequence of writes from alternating origins; afterwards every state that
/// was current when a different writer replaced it is recoverable.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g04_no_state_replaced_by_another_writer_is_ever_lost() {
    let s = Stack::new("g04-sequence").await;
    let id = s.create_text("T", "0").await;
    let mut must_survive = vec![s.stored(&id).await];
    let plan: Vec<u64> = vec![1, 1, 2, 0, 1, 0, 0, 3, 3, 1];
    for (i, who) in plan.iter().enumerate() {
        let current = s.stored(&id).await;
        let last_writer_was_editor = i > 0 && plan[i - 1] != 0;
        let same_session = i > 0 && plan[i - 1] == *who && *who != 0;
        if !(same_session && last_writer_was_editor) {
            must_survive.push(current);
        }
        let text = format!("state {i}");
        if *who == 0 {
            s.assistant_save(&id, &text).await.unwrap();
        } else {
            s.editor_save(&id, *who, v2(vec![text_block(1, &text)])).await.unwrap();
        }
    }
    for st in &must_survive {
        assert!(recoverable(&s, &id, st).await, "lost: {:?}", st.0);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g04_a_skill_that_empties_a_document_leaves_the_text_recoverable() {
    let s = Stack::new("g04-empty").await;
    let id = s.create_text("T", "precious text").await;
    let before = s.stored(&id).await;
    let base = s.revision(&id).await;
    s.svc
        .apply_skill_output(&id, base, "text_editor", EditContent::PlainText(String::new()))
        .await
        .unwrap();
    assert!(recoverable(&s, &id, &before).await);
}

/// Crash at every point of a save that changes title and content and must
/// version the prior state: afterwards the document is wholly old or wholly
/// new, and the old state is recoverable.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g04_a_crash_anywhere_in_a_save_leaves_old_or_new_whole_and_old_recoverable() {
    let mut completed = false;
    for k in 0..400 {
        let mut s = Stack::new(&format!("g04-crash-{k}")).await;
        let id = s.create_text("Old title", "old text").await;
        s.editor_save(&id, 1, v2(vec![text_block(1, "edited by the user")])).await.unwrap();
        let old = s.stored(&id).await;
        let base = s.revision(&id).await;
        s.events.take();

        s.faulty.crash_after(k);
        let r = s
            .svc
            .save(&id, Edit {
                base,
                title: Some("New title".into()),
                content: Some(EditContent::PlainText("assistant text".into())),
                origin: Origin::Assistant,
                explicit: false,
                pasted_tokens: vec![],
            })
            .await;
        let crashed = s.faulty.crashed();
        s.restart();

        let now = s.stored(&id).await;
        let is_old = now == old;
        let is_new = now.0 == "New title"
            && serde_json::from_str::<serde_json::Value>(&now.1).is_ok()
            && texts(&now.1) == vec!["assistant text".to_string()];
        assert!(is_old || is_new, "k={k}: document is neither old nor new: title {:?}", now.0);
        assert!(recoverable(&s, &id, &old).await, "k={k}: old state lost");
        if r.is_err() {
            assert!(s.events.peek().is_empty(), "k={k}: a failed save emits nothing");
            // The service still works after the failure.
            s.svc.read(&id).await.expect("readable after a failed save");
        } else {
            assert!(is_new, "k={k}: Ok means the new state is stored");
        }
        if !crashed {
            assert!(r.is_ok(), "k={k}: no crash, the save succeeds");
            completed = true;
            break;
        }
    }
    assert!(completed, "the save never completed within the crash budget");
}

/// Crash at every point of a create: either the create succeeded, or no
/// document with that title is listed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g04_a_crash_anywhere_in_a_create_leaves_no_half_document() {
    let mut completed = false;
    for k in 0..400 {
        let mut s = Stack::new(&format!("g04-create-crash-{k}")).await;
        s.faulty.crash_after(k);
        let r = s.svc.create(new_text_doc("Half", "body")).await;
        let crashed = s.faulty.crashed();
        s.restart();
        let listed: Vec<_> = s.svc.list(None).await.unwrap().into_iter().filter(|d| d.title == "Half").collect();
        match r {
            Ok(out) => {
                assert_eq!(listed.len(), 1, "k={k}");
                assert_eq!(texts(&s.stored(&out.id).await.1), vec!["body"]);
                assert!(!s.commits(&out.id).await.is_empty(), "k={k}: initial version");
            }
            Err(_) => {
                assert!(listed.is_empty(), "k={k}: a failed create leaves no listed document");
                assert!(s.events.peek().is_empty(), "k={k}");
            }
        }
        if !crashed {
            completed = true;
            break;
        }
    }
    assert!(completed);
}

/// The skill path under crash: the prior state is never lost.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g04_a_crash_anywhere_in_a_skill_output_never_loses_the_prior_state() {
    let mut completed = false;
    for k in 0..400 {
        let mut s = Stack::new(&format!("g04-skill-crash-{k}")).await;
        let id = s.legacy("Legacy", "raw legacy body").await;
        let old = s.stored(&id).await;
        let base = s.revision(&id).await;
        s.faulty.crash_after(k);
        let _ = s
            .svc
            .apply_skill_output(&id, base, "redactor", EditContent::PlainText("redacted".into()))
            .await;
        let crashed = s.faulty.crashed();
        s.restart();
        assert!(recoverable(&s, &id, &old).await, "k={k}: legacy state lost");
        if !crashed {
            completed = true;
            break;
        }
    }
    assert!(completed);
}

/// Saves of one session may replace each other (the policy decides), but the
/// end of the session versions what it leaves.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g04_end_of_session_versions_the_last_save() {
    let s = Stack::new("g04-end").await;
    let id = s.create_text("T", "x").await;
    s.editor_save(&id, 7, v2(vec![text_block(1, "a")])).await.unwrap();
    s.editor_save(&id, 7, v2(vec![text_block(1, "ab")])).await.unwrap();
    let last = s.stored(&id).await;
    s.svc.end_session(&id, EditSession(7)).await.unwrap();
    assert_eq!(s.committed_states(&id).await.first(), Some(&last));
}
