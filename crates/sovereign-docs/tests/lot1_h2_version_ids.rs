//! v0.0.12 lot 1, item 1a — H2: a malformed id is not "no such version".
//!
//! Contract: `doc/plans/V0012-lot1.md`, H2: "`version_of` refuses an id that
//! does not parse as a commit id with its own reason ('the version id is
//! malformed'), distinct from a well-formed id that is not found ('no such
//! version'). The same split wherever the service maps `InvalidId` together
//! with `NotFound` for a commit (`read_version`, `restore_version`)."
//!
//! Written before the code, by another author: whoever implements item 1a
//! makes these pass **without editing this file**.
//!
//! The reasons are compared exactly: they are the words the contract gives,
//! and they reach the user ("This version can't be restored: <reason>.").

mod common;

use common::*;
use sovereign_docs::DocError;

const MALFORMED: &str = "the version id is malformed";
const NOT_FOUND: &str = "no such version";

/// The reason of a refusal, or a panic naming what came instead.
fn reason<T: std::fmt::Debug>(r: Result<T, DocError>, what: &str) -> String {
    match r {
        Err(DocError::VersionRefused { reason, .. }) => reason,
        other => panic!("{what}: expected VersionRefused, got {other:?}"),
    }
}

/// Ids that do not parse as a commit id. The first is the shape the desktop's
/// History built with `Thing`'s `Display` (review v0.0.11, "The History bug"):
/// the reserved word `commit` escaped in backticks — written out by hand here,
/// as the malformed input it is.
fn malformed_ids(real_key: &str) -> Vec<String> {
    vec![
        format!("`commit`:{real_key}"),
        "commit".into(),
        String::new(),
        "not an id at all".into(),
        format!("document:{real_key}"),
    ]
}

/// A document with two versions; returns (document id, key of a real commit).
async fn two_versions(s: &Stack) -> (String, String) {
    let id = s.create_text("T", "first").await;
    s.assistant_save(&id, "second").await.unwrap();
    let commits = s.commits(&id).await;
    assert!(commits.len() >= 2, "precondition: two versions");
    let raw = commits.last().unwrap().id_string().unwrap();
    let key = raw.strip_prefix("commit:").expect("a raw commit id is commit:<key>").to_string();
    (id, key)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn h2_restore_version_says_a_malformed_id_is_malformed_and_writes_nothing() {
    let s = Stack::new("h2-restore").await;
    let (id, key) = two_versions(&s).await;
    let before = s.stored(&id).await;
    let versions = s.commits(&id).await.len();
    for bad in malformed_ids(&key) {
        let r = s.svc.restore_version(&id, &bad).await;
        assert_eq!(reason(r, &format!("restore {bad:?}")), MALFORMED, "restore_version({bad:?})");
        assert_eq!(s.stored(&id).await, before, "nothing written for {bad:?}");
        assert_eq!(s.commits(&id).await.len(), versions, "no version for {bad:?}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn h2_read_version_says_a_malformed_id_is_malformed() {
    let s = Stack::new("h2-read").await;
    let (id, key) = two_versions(&s).await;
    for bad in malformed_ids(&key) {
        let r = s.svc.read_version(&id, &bad).await;
        assert_eq!(reason(r, &format!("read {bad:?}")), MALFORMED, "read_version({bad:?})");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn h2_a_well_formed_id_that_is_not_found_is_no_such_version() {
    let s = Stack::new("h2-missing").await;
    let (id, _) = two_versions(&s).await;
    let missing = "commit:zzzzzzzzzzzzzzzzzzzz";
    assert_eq!(reason(s.svc.restore_version(&id, missing).await, "restore missing"), NOT_FOUND);
    assert_eq!(reason(s.svc.read_version(&id, missing).await, "read missing"), NOT_FOUND);
}

// H2's other side: the real raw id restores (the split does not refuse good
// ids). Expected green today.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn h2_the_raw_id_of_a_real_version_still_restores() {
    let s = Stack::new("h2-real").await;
    let (id, key) = two_versions(&s).await;
    s.svc.restore_version(&id, &format!("commit:{key}")).await.expect("a real version restores");
    assert_eq!(texts(&s.stored(&id).await.1), vec!["first"]);
}
