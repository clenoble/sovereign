//! `sovereign_core::clock::test_clock` is one clock per process, and cargo
//! runs these tests on several threads (claude-coord from-fable-2nde-position
//! /0027 §2). With the guard, a test that moves the clock and a test that
//! reads the wall clock never see each other: run this binary without
//! `--test-threads=1`, many times.
//!
//! Runs with the `test-support` feature (`cargo test -p sovereign-core
//! --features test-support --test clock_guard`; CI's `--all-features` run of
//! sovereign-core includes it).
#![cfg(feature = "test-support")]

use std::time::Duration as StdDuration;

use chrono::{Duration, Utc};
use sovereign_core::clock::{now, test_clock};

const TOLERANCE_SECS: i64 = 5;

fn near_wall_clock() -> bool {
    (now() - Utc::now()).num_seconds().abs() <= TOLERANCE_SECS
}

#[test]
fn a_test_that_moves_the_clock_holds_the_guard() {
    for _ in 0..20 {
        let clock = test_clock::hold();
        clock.advance(Duration::days(400));
        std::thread::sleep(StdDuration::from_millis(2));
        let ahead = (now() - Utc::now()).num_days();
        assert!((399..=401).contains(&ahead), "the moved clock reads {ahead} days ahead");
        clock.set(Some(Utc::now() - Duration::days(30)));
        assert!((now() - Utc::now()).num_days() <= -29, "set backward");
        drop(clock);
        std::thread::sleep(StdDuration::from_millis(1));
    }
}

#[test]
fn a_test_that_reads_the_clock_beside_it_sees_the_wall_clock() {
    for _ in 0..20 {
        let _clock = test_clock::hold();
        for _ in 0..5 {
            assert!(near_wall_clock(), "read {} while the wall clock is {}", now(), Utc::now());
            std::thread::sleep(StdDuration::from_millis(1));
        }
    }
}

#[test]
fn a_test_that_panics_with_the_clock_moved_leaves_the_wall_clock() {
    let r = std::panic::catch_unwind(|| {
        let clock = test_clock::hold();
        clock.advance(Duration::days(3650));
        panic!("a failing test, with the clock moved");
    });
    assert!(r.is_err());
    let _clock = test_clock::hold();
    assert!(near_wall_clock(), "after the panic, the clock is back on the wall clock");
}
