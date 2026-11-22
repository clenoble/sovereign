//! v0.0.13 rc2, lot R5 (`doc/plans/V0013-rc2-R5.md`, private): the v0.0.12 log under an anchor of count 0. Céline,
//! 2026-11-20: « 17. solution 1 », the refusal v0.0.12 had restored.
//!
//! - **What the spec says, which this lot makes true** (ch. 08, C-LOG, LG6): « A log in the v0.0.12 format that
//!   verifies under its own rules and holds no plaintext line is carried into the new format once. One that does
//!   not is quarantined, as today. »
//! - **v0.0.12's rule kept here:** an anchor of count 0 records an empty log. A log with entries under it does not
//!   verify: it is a roll-back.
//! - **So:** at a login, a v0.0.12 log with entries under a v0.0.12 anchor of count 0 fails the check. It is
//!   quarantined, the person is told, and none of its turns is carried or given back as history. An empty v0.0.12
//!   log under an anchor of count 0 still passes.
//!
//! The fixtures are `clog_tests`' (v1), unchanged: the v0.0.12 format as it shipped, and a login through the seam
//! the hosts call (`set_session_log_key_for`).
//!
//! Written before the code by R5's test author; whoever implements makes these pass without editing this file. A
//! test that looks wrong is reported on the channel.

use super::clog_tests::*;
use super::*;
use sovereign_core::auth::PersonaKind;
use std::fs;

const P: PersonaKind = PersonaKind::Primary;

/// A v0.0.12 log of `n` turns, then its anchor replaced by a v0.0.12 anchor of count 0, sealed under the persona's
/// key: the anchor such a log had when it was empty (at a rotation, or a first open), put back.
fn under_an_anchor_of_count_0(a: &Area, tag: &str, n: usize) -> Vec<String> {
    let texts = turns(tag, n);
    let v = v0012_log(a, P, KEY, &texts);
    assert_eq!(v.lines.len(), n, "fixture: the v0.0.12 log holds {n} entries");
    fs::write(a.dir(P).join("session_log.anchor"), v0012_anchor(KEY, 0, GENESIS)).unwrap();
    texts
}

/// A v0.0.12 log with entries under an anchor of count 0 is quarantined, not carried, and none of its turns comes
/// back as history.
#[tokio::test]
async fn r5_a_v0012_log_with_entries_under_an_anchor_of_count_0_is_quarantined_not_carried() {
    let a = area("r5-count0").await;
    let texts = under_an_anchor_of_count_0(&a, "r5-count0", 3);

    let after = login(&a, P, KEY);
    assert_failed(&a, P, 0, &after, "a v0.0.12 log of 3 entries under an anchor of count 0");
    let history = after.history();
    for t in &texts {
        assert!(!history.contains(t), "« {t} » is not carried: the history is {history:?}");
    }
}

/// The smallest case: one entry under an anchor of count 0.
#[tokio::test]
async fn r5_one_entry_under_an_anchor_of_count_0_is_enough_to_fail() {
    let a = area("r5-count0-one").await;
    under_an_anchor_of_count_0(&a, "r5-count0-one", 1);

    let after = login(&a, P, KEY);
    assert_failed(&a, P, 0, &after, "a v0.0.12 log of 1 entry under an anchor of count 0");
    assert_eq!(after.history(), Vec::<String>::new(), "nothing of it is carried");
}

/// The quarantine is once: the next login starts a fresh log and says nothing more.
#[tokio::test]
async fn r5_after_the_quarantine_the_next_login_passes() {
    let a = area("r5-count0-next").await;
    under_an_anchor_of_count_0(&a, "r5-count0-next", 2);
    let first = login(&a, P, KEY);
    assert_failed(&a, P, 0, &first, "fixture: the first login quarantines");
    drop(first);

    let next = login(&a, P, KEY);
    assert_passed(&a, P, 1, &next, "the login after the quarantine");
}

/// What v0.0.12's rule allowed stays allowed: an empty v0.0.12 log under an anchor of count 0, and no beacon (it
/// was written only once the log had content), passes, with nothing to carry.
#[tokio::test]
async fn r5_an_empty_v0012_log_under_an_anchor_of_count_0_still_passes() {
    let a = area("r5-count0-empty").await;
    let dir = a.dir(P);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("session_log.jsonl"), b"").unwrap();
    fs::write(dir.join("session_log.anchor"), v0012_anchor(KEY, 0, GENESIS)).unwrap();

    let after = login(&a, P, KEY);
    assert_passed(&a, P, 0, &after, "an empty v0.0.12 log under an anchor of count 0");
    assert_eq!(after.history(), Vec::<String>::new(), "nothing to carry");
}
