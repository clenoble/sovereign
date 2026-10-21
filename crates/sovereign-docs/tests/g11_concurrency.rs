//! G11 — writers never silently replace each other; identical saves do nothing.
//!
//! Spec ch. 03, *Document service — contract (C-DOC)*, G11: "**Writers never
//! silently replace each other.** A save carries the revision it was based on;
//! a save based on a revision that is no longer current is refused and writes
//! nothing. Of two concurrent saves from the same revision, exactly one lands.
//! A save identical to the stored state writes nothing, makes no version,
//! changes no time and emits nothing."

mod common;

use std::sync::Arc;

use common::*;
use sovereign_docs::{DocError, Edit, EditContent, EditSession, Origin};

fn edit(base: sovereign_docs::Revision, session: u64, text: &str) -> Edit {
    Edit {
        base,
        title: None,
        content: Some(EditContent::V2(v2(vec![text_block(1, text)]))),
        origin: Origin::Editor { session: EditSession(session) },
        explicit: false,
        pasted_tokens: vec![],
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g11_a_save_from_a_stale_revision_is_refused_and_writes_nothing() {
    let s = Stack::new("g11-stale").await;
    let id = s.create_text("T", "x").await;
    let stale = s.revision(&id).await;
    s.editor_save(&id, 1, v2(vec![text_block(1, "first writer")])).await.unwrap();
    let after_first = s.stored(&id).await;
    let n = s.commits(&id).await.len();
    s.events.take();
    let r = s.svc.save(&id, edit(stale, 2, "second writer")).await;
    match r {
        Err(DocError::Conflict { current, .. }) => assert_eq!(current, s.revision(&id).await),
        other => panic!("expected a conflict, got {:?}", other.map(|o| o.changed)),
    }
    assert_eq!(s.stored(&id).await, after_first);
    assert_eq!(s.commits(&id).await.len(), n);
    assert!(s.events.peek().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g11_of_concurrent_saves_from_one_revision_exactly_one_lands() {
    for round in 0..10 {
        let s = Stack::new(&format!("g11-race-{round}")).await;
        let id = s.create_text("T", "x").await;
        let base = s.revision(&id).await;
        let svc = s.svc.clone();
        let mut handles = vec![];
        for w in 0..4u64 {
            let svc = Arc::clone(&svc);
            let (id, base) = (id.clone(), base.clone());
            handles.push(tokio::spawn(async move {
                (w, svc.save(&id, edit(base, 10 + w, &format!("writer {w}"))).await)
            }));
        }
        let mut winners = vec![];
        for h in handles {
            let (w, r) = h.await.unwrap();
            match r {
                Ok(_) => winners.push(w),
                Err(DocError::Conflict { .. }) => {}
                Err(e) => panic!("round {round}: writer {w}: unexpected {e}"),
            }
        }
        assert_eq!(winners.len(), 1, "round {round}: winners {winners:?}");
        assert_eq!(texts(&s.stored(&id).await.1), vec![format!("writer {}", winners[0])]);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g11_each_successful_write_moves_the_revision_and_reading_does_not() {
    let s = Stack::new("g11-revision").await;
    let id = s.create_text("T", "x").await;
    let r0 = s.revision(&id).await;
    assert_eq!(s.revision(&id).await, r0, "stable across reads");
    let out = s.editor_save(&id, 1, v2(vec![text_block(1, "y")])).await.unwrap();
    assert_ne!(out.revision, r0);
    assert_eq!(s.revision(&id).await, out.revision);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g11_an_identical_save_writes_nothing() {
    let s = Stack::new("g11-identical").await;
    s.policy.set(PolicyMode::Always);
    let id = s.create_text("T", "x").await;
    s.editor_save(&id, 1, v2(vec![text_block(1, "same")])).await.unwrap();
    let before = s.stored_doc(&id).await;
    let n = s.commits(&id).await.len();
    let rev = s.revision(&id).await;
    s.clock.advance(chrono::Duration::minutes(10));
    s.events.take();
    s.pii.clear();

    let out = s.editor_save(&id, 1, v2(vec![text_block(1, "same")])).await.unwrap();
    assert!(!out.changed);
    assert!(out.version.is_none());
    assert_eq!(out.revision, rev);
    let after = s.stored_doc(&id).await;
    assert_eq!(after.content, before.content);
    assert_eq!(after.modified_at, before.modified_at, "the timeline does not move");
    assert_eq!(s.commits(&id).await.len(), n);
    assert!(s.events.peek().is_empty());
    assert!(s.pii.requests().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g11_an_identical_save_from_a_stale_revision_is_still_refused() {
    // Identical to what *is* stored, but based on an older revision: the
    // writer did not see the current state, so it is a conflict, not a no-op.
    let s = Stack::new("g11-identical-stale").await;
    let id = s.create_text("T", "x").await;
    let stale = s.revision(&id).await;
    s.editor_save(&id, 1, v2(vec![text_block(1, "now")])).await.unwrap();
    let r = s.svc.save(&id, edit(stale, 2, "now")).await;
    assert!(matches!(r, Err(DocError::Conflict { .. })));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g11_writes_on_trashed_or_unknown_documents_are_refused() {
    let s = Stack::new("g11-refusals").await;
    let id = s.create_text("T", "x").await;
    let base = s.revision(&id).await;
    s.svc.trash(&id).await.unwrap();
    let r = s.svc.save(&id, edit(base.clone(), 1, "y")).await;
    assert!(matches!(r, Err(DocError::InTrash(_))));
    let r = s.svc.save("document:nope", edit(base, 1, "y")).await;
    assert!(matches!(r, Err(DocError::NotFound(_))));
}
