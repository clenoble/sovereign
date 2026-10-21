//! G26 — concurrent edits keep both, without asking.
//!
//! Spec ch. 03, *Document service — contract (C-DOC)*, G26 (specified
//! 2026-10-15, for C-WIRE): "A remote state whose previous stamp is not the
//! local stamp, while the local state changed since, is a concurrent edit:
//! the state with the greater stamp is current, the other is kept as a version
//! — `apply_wire` stores a remote state with the lower stamp as a version
//! without making it current — and the outcome says it was concurrent."
//!
//! C-WIRE guarantee 7 (spec ch. 05): "both devices end with the same current
//! state (the greater stamp) and the other state in History, and both show a
//! notice" — Céline's rule for save conflicts (2026-10-12), across devices.

mod common;

use common::wire::*;
use common::*;
use sovereign_docs::WireApplied;

/// A document with one shared state, then a local edit: returns its id, the
/// shared stamp and the local edit's state.
async fn edited_since_shared(s: &Stack) -> (String, sovereign_docs::Stamp, (String, String)) {
    let id = s.create_text("T", "shared").await;
    let shared = current_stamp(s, &id).await;
    s.editor_save(&id, 1, v2(vec![text_block(1, "local edit")])).await.unwrap();
    (id.clone(), shared, s.stored(&id).await)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g26_a_concurrent_state_with_the_greater_stamp_becomes_current_and_the_local_one_a_version() {
    let s = Stack::new("g26-greater").await;
    let (id, shared, local) = edited_since_shared(&s).await;
    let later = stamp(&s, 60, 0, "phone");
    let out = s
        .svc
        .apply_wire(remote_text(&id, "T", "phone edit", "phone"), meta(Some(later), Some(shared)))
        .await
        .unwrap();
    assert!(out.concurrent);
    assert!(matches!(out.applied, WireApplied::Applied(_)));
    assert_eq!(texts(&s.stored(&id).await.1), vec!["phone edit"]);
    assert!(s.committed_states(&id).await.contains(&local), "the local edit is in History");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g26_a_concurrent_state_with_the_lower_stamp_is_kept_as_a_version_only() {
    let s = Stack::new("g26-lower").await;
    let (id, shared, local) = edited_since_shared(&s).await;
    // Just below the local edit's stamp (same time and counter, a device name
    // that sorts first), and above the shared one.
    let l = current_stamp(&s, &id).await;
    let earlier = sovereign_docs::Stamp { device: "a-phone".into(), counter: l.counter, wall_ms: l.wall_ms };
    assert!(earlier < l && earlier > shared);
    let out = s
        .svc
        .apply_wire(remote_text(&id, "From peer", "phone edit", "phone"), meta(Some(earlier), Some(shared)))
        .await
        .unwrap();
    assert!(out.concurrent);
    assert_eq!(out.applied, WireApplied::StoredAsVersion);
    assert_eq!(s.stored(&id).await, local, "the current state does not change");
    assert!(
        s.committed_states(&id).await.iter().any(|(t, c)| t == "From peer" && texts(c) == vec!["phone edit".to_string()]),
        "the peer's edit is in History"
    );
}

/// Two devices edit the same document apart, then exchange both ways: both
/// end with the same current state and both states in History.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g26_two_devices_converge_and_both_keep_both_versions() {
    let a = Stack::with_device("g26-a", "laptop").await;
    let b = Stack::with_device("g26-b", "phone").await;
    let id = a.create_text("T", "shared").await;
    send(&a, &b, &id).await;
    a.editor_save(&id, 1, v2(vec![text_block(1, "laptop edit")])).await.unwrap();
    b.editor_save(&id, 1, v2(vec![text_block(1, "phone edit")])).await.unwrap();
    let a_edit = a.stored(&id).await.1;
    let b_edit = b.stored(&id).await.1;
    let to_b = send(&a, &b, &id).await;
    let to_a = send(&b, &a, &id).await;
    assert!(to_b.concurrent && to_a.concurrent, "both sides see a concurrent edit (for the notice)");
    let (ca, cb) = (a.stored(&id).await, b.stored(&id).await);
    assert_eq!(texts(&ca.1), texts(&cb.1), "same current state on both");
    for s in [&a, &b] {
        let history: Vec<String> = s.committed_states(&id).await.into_iter().map(|(_, c)| c).collect();
        let current = s.stored(&id).await.1;
        for edit in [&a_edit, &b_edit] {
            assert!(
                &current == edit || history.iter().any(|h| texts(h) == texts(edit)),
                "{}: an edit is missing",
                s.device
            );
        }
    }
}
