//! Contract C-LOG, LG4's caps (spec ch. 08, « A line or a file above a named cap fails the
//! check »), written before the code by its test author.
//!
//! The contract names the caps without giving their values, so these tests read them from two
//! constants the code does not have yet, and do not compile until they exist:
//! - `crate::session_log::MAX_LINE_BYTES`, the largest line the check reads;
//! - `crate::session_log::MAX_FILE_BYTES`, the largest log file the check reads.
//! Any integer type will do. If the coder names them otherwise, the names here change with the
//! test author's agreement, not the assertions. The module line in `orchestrator.rs` is switched
//! off (`any()`) until then, so the crate's other tests keep running.

use super::clog_tests::*;
use std::fs;

const P: sovereign_core::auth::PersonaKind = sovereign_core::auth::PersonaKind::Primary;

/// A line one byte above the line cap, appended to a log that verified, fails the check, and
/// nothing panics.
#[tokio::test]
async fn a_line_above_the_line_cap_fails_the_check_without_panic() {
    // LG4
    let cap = crate::session_log::MAX_LINE_BYTES as u64;
    let a = area("lg4-line").await;
    sealed_log(&a, P, KEY, "lg4-line", 2);
    let mut line = vec![b'a'; (cap + 1) as usize];
    line.push(b'\n');
    append(&a.log(P), &line);

    let after = login(&a, P, KEY);
    assert_failed(&a, P, 0, &after, "a line above the line cap");
}

/// A log file one byte above the file cap fails the check, and nothing panics. The file is
/// extended, not written byte by byte: what the check must not do is read it whole.
#[tokio::test]
async fn a_file_above_the_file_cap_fails_the_check_without_panic() {
    // LG4
    let cap = crate::session_log::MAX_FILE_BYTES as u64;
    let a = area("lg4-file").await;
    sealed_log(&a, P, KEY, "lg4-file", 2);
    let f = fs::OpenOptions::new().write(true).open(a.log(P)).unwrap();
    f.set_len(cap + 1).unwrap();
    drop(f);

    let after = login(&a, P, KEY);
    assert_failed(&a, P, 0, &after, "a log file above the file cap");
}

/// C-LOG DG1 (spec v2.0.126), added with the third set: a line above its cap is diagnosed as such.
#[tokio::test]
async fn a_line_above_its_cap_is_found() {
    // DG1 (LG4)
    let cap = crate::session_log::MAX_LINE_BYTES as u64;
    let a = area("dg1-line-cap").await;
    sealed_log(&a, P, KEY, "dg1-line-cap", 2);
    let mut line = vec![b'a'; (cap + 1) as usize];
    line.push(b'\n');
    append(&a.log(P), &line);
    let after = login(&a, P, KEY);
    assert_failed(&a, P, 0, &after, "fixture: a line above the line cap");

    let d = crate::log_diagnosis::diagnose(&a.paths(P), &KEY).expect("it runs").expect("a quarantine");
    assert!(
        d.found.contains(&crate::log_diagnosis::Finding::AboveCapOrNotText),
        "a line above its cap: {:?}",
        d.found
    );
}
