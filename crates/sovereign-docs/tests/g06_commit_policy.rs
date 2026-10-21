//! G6 — versions are made by amount of change, never by time.
//!
//! Spec ch. 03, *Document service — contract (C-DOC)*, G6 (decided by Céline,
//! 2026-10-11): "**Versions follow the amount of change, never time.** An
//! editor save becomes a version when the amount of change since the last
//! version — measured by `change_amount` between the two states — reaches the
//! policy's threshold, or when the user asked for one (Ctrl+S). Time passing
//! never makes a version, nor does the number of saves. The end of an editing
//! session makes a version of what it left; every non-editor write makes a
//! version of its result; no version repeats the one before it. The policy is
//! asked about editor saves only and can only add versions."
//!
//! The measure and the default threshold are "to be evaluated": the tests
//! never assume a number. They compare the service's decisions with the
//! policy's parameter and with `change_amount` itself.

mod common;

use chrono::Duration;
use common::*;
use sovereign_docs::{
    change_amount, CommitContext, CommitDecision, CommitPolicy, DefaultCommitPolicy, Edit,
    EditContent, EditSession, Origin,
};

async fn explicit_save(s: &Stack, id: &str, session: u64, text: &str) {
    let base = s.revision(id).await;
    s.svc
        .save(id, Edit {
            base,
            title: None,
            content: Some(EditContent::V2(v2(vec![text_block(1, text)]))),
            origin: Origin::Editor { session: EditSession(session) },
            explicit: true,
            pasted_tokens: vec![],
        })
        .await
        .unwrap();
}

fn no_adjacent_repeats(states: &[(String, String)]) -> bool {
    states.windows(2).all(|w| w[0] != w[1])
}

/// The state of the document's newest version.
async fn last_version(s: &Stack, id: &str) -> (String, String) {
    s.committed_states(id).await.into_iter().next().expect("a version")
}

// ------------------------------------------------- the service obeys the policy --

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g06_policy_never_same_session_saves_make_no_version() {
    let s = Stack::new("g06-never").await;
    s.policy.set(PolicyMode::Never);
    let id = s.create_text("T", "x").await;
    for t in ["a", "ab", "abc"] {
        let out = s.editor_save(&id, 1, v2(vec![text_block(1, t)])).await.unwrap();
        assert!(out.version.is_none());
    }
    assert_eq!(s.commits(&id).await.len(), 1, "only the initial version");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g06_policy_always_every_save_is_a_version_of_what_was_saved() {
    let s = Stack::new("g06-always").await;
    s.policy.set(PolicyMode::Always);
    let id = s.create_text("T", "x").await;
    for (i, t) in ["a", "ab", "abc"].iter().enumerate() {
        let out = s.editor_save(&id, 1, v2(vec![text_block(1, t)])).await.unwrap();
        let commits = s.commits(&id).await;
        assert_eq!(commits.len(), i + 2);
        assert_eq!(out.version.as_deref(), commits[0].id_string().as_deref());
        assert_eq!(texts(&commits[0].snapshot.content), vec![t.to_string()]);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g06_the_policy_is_told_the_change_since_the_last_version_as_measured() {
    let s = Stack::new("g06-context").await;
    s.policy.set(PolicyMode::Never);
    let id = s.create_text("T", "x").await;
    s.policy.asked.lock().unwrap().clear();
    let mut text = String::new();
    let mut previous = 0;
    for i in 0..12 {
        text.push_str("word ");
        s.clock.advance(Duration::minutes(7 * i));
        s.editor_save(&id, 1, v2(vec![text_block(1, &text)])).await.unwrap();
        let ctx = s.policy.asked.lock().unwrap().last().cloned().unwrap();
        let (vt, vc) = last_version(&s, &id).await;
        let (nt, nc) = s.stored(&id).await;
        assert_eq!(ctx.document_id, id);
        assert_eq!(ctx.change_since_version, change_amount(&vt, &vc, &nt, &nc), "save {i}");
        assert!(ctx.change_since_version >= previous, "appending never shrinks the change");
        assert!(!ctx.explicit);
        previous = ctx.change_since_version;
    }
}

// --------------------------------------------- the threshold, never the time --

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g06_an_autosave_is_a_version_exactly_when_the_change_reaches_the_threshold() {
    for threshold in [1u64, 7, 40, 300] {
        let s = Stack::new(&format!("g06-threshold-{threshold}")).await;
        s.policy.set(PolicyMode::Threshold(threshold));
        let id = s.create_text("T", "start").await;
        let mut text = "start".to_string();
        for i in 0..60 {
            let before_versions = s.commits(&id).await.len();
            let (vt, vc) = last_version(&s, &id).await;
            text.push_str(if i % 3 == 0 { " lorem" } else { "x" });
            s.editor_save(&id, 1, v2(vec![text_block(1, &text)])).await.unwrap();
            let (nt, nc) = s.stored(&id).await;
            let change = change_amount(&vt, &vc, &nt, &nc);
            let made = s.commits(&id).await.len() > before_versions;
            assert_eq!(made, change >= threshold, "threshold {threshold}, save {i}, change {change}");
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g06_time_passing_alone_never_makes_a_version() {
    let s = Stack::new("g06-time").await;
    s.policy.set(PolicyMode::Threshold(DefaultCommitPolicy::DEFAULT_THRESHOLD));
    let id = s.create_text("T", "a stable paragraph").await;
    // Hours with no edits at all.
    s.clock.advance(Duration::hours(30));
    s.svc.read(&id).await.unwrap();
    assert_eq!(s.commits(&id).await.len(), 1);
    // Hours apart, tiny edits that never add up to the threshold.
    let mut text = "a stable paragraph".to_string();
    let (vt, vc) = last_version(&s, &id).await;
    for _ in 0..6 {
        s.clock.advance(Duration::hours(5));
        let mut next = text.clone();
        next.push('.');
        let candidate = v2(vec![text_block(1, &next)]);
        if change_amount(&vt, &vc, "T", &candidate) >= DefaultCommitPolicy::DEFAULT_THRESHOLD {
            break; // would reach the threshold by amount of change, not by time
        }
        text = next;
        s.editor_save(&id, 1, v2(vec![text_block(1, &text)])).await.unwrap();
    }
    assert_eq!(s.commits(&id).await.len(), 1, "a day of small edits makes no version by itself");
}

/// More change never yields fewer versions: two editing histories from the
/// same start, the second doing everything the first did and more.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g06_more_change_never_yields_fewer_versions() {
    let mut counts = vec![];
    for (tag, steps) in [("short", 20usize), ("long", 45)] {
        let s = Stack::new(&format!("g06-monotonic-{tag}")).await;
        s.policy.set(PolicyMode::Threshold(25));
        let id = s.create_text("T", "").await;
        let mut text = String::new();
        for i in 0..steps {
            text.push_str(&format!("{i} "));
            s.editor_save(&id, 1, v2(vec![text_block(1, &text)])).await.unwrap();
        }
        counts.push(s.commits(&id).await.len());
    }
    assert!(counts[1] >= counts[0], "{counts:?}");
}

// --------------------------------------------------------- the other versions --

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g06_an_explicit_save_is_a_version_whatever_the_policy() {
    let s = Stack::new("g06-explicit").await;
    s.policy.set(PolicyMode::Never);
    let id = s.create_text("T", "x").await;
    explicit_save(&s, &id, 1, "kept on purpose").await;
    assert_eq!(texts(&s.commits(&id).await[0].snapshot.content), vec!["kept on purpose"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g06_end_of_session_versions_once_and_only_if_needed() {
    let s = Stack::new("g06-end").await;
    s.policy.set(PolicyMode::Never);
    let id = s.create_text("T", "x").await;
    s.editor_save(&id, 4, v2(vec![text_block(1, "unsaved as version")])).await.unwrap();
    s.svc.end_session(&id, EditSession(4)).await.unwrap();
    let n = s.commits(&id).await.len();
    assert_eq!(n, 2);
    s.svc.end_session(&id, EditSession(4)).await.unwrap();
    assert_eq!(s.commits(&id).await.len(), n, "nothing new to version");
    s.svc.end_session(&id, EditSession(99)).await.unwrap();
    assert_eq!(s.commits(&id).await.len(), n, "a session that wrote nothing makes nothing");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g06_end_of_session_after_an_explicit_save_makes_no_repeat() {
    let s = Stack::new("g06-no-repeat").await;
    s.policy.set(PolicyMode::Never);
    let id = s.create_text("T", "x").await;
    explicit_save(&s, &id, 2, "done").await;
    let n = s.commits(&id).await.len();
    s.svc.end_session(&id, EditSession(2)).await.unwrap();
    assert_eq!(s.commits(&id).await.len(), n);
    assert!(no_adjacent_repeats(&s.committed_states(&id).await));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g06_non_editor_writes_always_version_their_result_and_are_not_asked() {
    let s = Stack::new("g06-non-editor").await;
    s.policy.set(PolicyMode::Never);
    let id = s.create_text("T", "x").await;
    s.policy.asked.lock().unwrap().clear();
    let out = s.assistant_save(&id, "from the assistant").await.unwrap();
    let head = s.commits(&id).await;
    assert_eq!(out.version.as_deref(), head[0].id_string().as_deref());
    assert_eq!(texts(&head[0].snapshot.content), vec!["from the assistant"]);
    assert!(s.policy.asked.lock().unwrap().is_empty(), "the policy is for editor saves");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g06_no_version_repeats_the_one_before_it_across_mixed_writes() {
    let s = Stack::new("g06-mixed").await;
    s.policy.set(PolicyMode::Always);
    let id = s.create_text("T", "same").await;
    explicit_save(&s, &id, 1, "same").await;
    s.assistant_save(&id, "same").await.unwrap();
    s.svc.end_session(&id, EditSession(1)).await.unwrap();
    s.assistant_save(&id, "different").await.unwrap();
    s.svc.end_session(&id, EditSession(1)).await.unwrap();
    assert!(no_adjacent_repeats(&s.committed_states(&id).await));
}

// ----------------------------------------------------------------- the parts --

#[test]
fn g06_default_policy_honours_its_threshold_and_explicit_saves() {
    let ctx = |change, explicit| CommitContext {
        document_id: "document:x".into(),
        origin: Origin::Editor { session: EditSession(1) },
        explicit,
        change_since_version: change,
    };
    for t in [1u64, 10, 500, 10_000] {
        let p = DefaultCommitPolicy::new(t);
        assert_eq!(p.decide(&ctx(t - 1, false)), CommitDecision::NotYet, "t={t}");
        assert_eq!(p.decide(&ctx(t, false)), CommitDecision::Version, "t={t}");
        assert_eq!(p.decide(&ctx(t * 3, false)), CommitDecision::Version, "t={t}");
        assert_eq!(p.decide(&ctx(0, true)), CommitDecision::Version, "t={t}");
    }
    assert_eq!(DefaultCommitPolicy::default().threshold, DefaultCommitPolicy::DEFAULT_THRESHOLD);
}

#[test]
fn g06_the_measure_is_zero_exactly_for_equal_states_and_symmetric() {
    let a = v2(vec![text_block(1, "alpha beta"), image_block(2, "/a.png", Some("ab"))]);
    let b = v2(vec![text_block(1, "alpha gamma beta"), image_block(2, "/a.png", Some("ab"))]);
    let c = v2(vec![text_block(1, "alpha beta"), image_block(2, "/b.png", Some("cd"))]);
    let d = v2(vec![image_block(2, "/a.png", Some("ab")), text_block(1, "alpha beta")]);
    assert_eq!(change_amount("T", &a, "T", &a), 0);
    for (x, y) in [(&a, &b), (&a, &c), (&a, &d), (&b, &c)] {
        assert!(change_amount("T", x, "T", y) > 0, "different states, non-zero");
        assert_eq!(change_amount("T", x, "T", y), change_amount("T", y, "T", x), "symmetric");
    }
    assert!(change_amount("Title", &a, "Title 2", &a) > 0, "a title change counts");
}

#[test]
fn g06_the_measure_never_shrinks_as_a_change_grows() {
    let base = v2(vec![text_block(1, "The quick brown fox")]);
    let mut prev = 0;
    let mut grown = "The quick brown fox".to_string();
    for word in ["jumps", "over", "the", "lazy", "dog", "and", "runs", "away"] {
        grown.push(' ');
        grown.push_str(word);
        let m = change_amount("T", &base, "T", &v2(vec![text_block(1, &grown)]));
        assert!(m >= prev, "appending more never measures less");
        prev = m;
    }
    // Adding blocks one after another grows the change too.
    let mut blocks = vec![text_block(1, "The quick brown fox")];
    let mut prev = 0;
    for n in 2..8u8 {
        blocks.push(unknown_block(n));
        let m = change_amount("T", &base, "T", &v2(blocks.clone()));
        assert!(m > prev, "every added block counts");
        prev = m;
    }
}
