//! rc2 — account creation is **single-flight** (spec ch. 07, *Auth store*,
//! "Created once, never replaced by account creation", v2.0.46): "a request
//! that arrives while one is in progress is refused ("Setup is already in
//! progress") and one that arrives after one has completed is refused
//! ("Already onboarded — log in instead"); a refused request changes no file."
//!
//! Written before the code, by another author: whoever implements
//! `crate::onboarding_claim` makes these pass without editing this file.
//!
//! The contract, as named for the implementer:
//! - `pub(crate) enum ClaimRefused { InProgress, AlreadyOnboarded }` (Debug,
//!   PartialEq), whose `Display` gives exactly the two sentences of the spec;
//! - `pub(crate) fn claim(profile_dir: &Path) -> Result<OnboardingClaim, ClaimRefused>`:
//!   `AlreadyOnboarded` if `profile_dir/crypto/auth.store` exists, `InProgress`
//!   if another claim on the same `profile_dir` is held, otherwise a guard;
//!   dropping the guard releases the claim.
//!
//! Scope of these tests: claims **within one process** (the Tauri app on a
//! phone, or the desktop app, holds its account creation in one process). They
//! say nothing about two processes on the same profile directory.

use std::path::Path;
use std::sync::{Arc, Barrier};

use crate::onboarding_claim::{claim, ClaimRefused, OnboardingClaim};

fn write_auth_store(profile_dir: &Path) {
    let crypto = profile_dir.join("crypto");
    std::fs::create_dir_all(&crypto).unwrap();
    std::fs::write(crypto.join("auth.store"), b"{}").unwrap();
}

/// The guard is held across `.await`s inside a `#[tauri::command] async fn`,
/// whose future must be `Send`.
#[test]
fn the_claim_guard_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<OnboardingClaim>();
}

#[test]
fn a_second_claim_while_the_first_is_held_is_refused_in_progress() {
    let dir = tempfile::tempdir().unwrap();
    let first = claim(dir.path()).expect("the first claim on a fresh profile succeeds");
    let second = claim(dir.path());
    assert!(
        matches!(second, Err(ClaimRefused::InProgress)),
        "a claim while another is held must be refused InProgress, got {:?}",
        second.as_ref().err()
    );
    drop(first);
}

#[test]
fn after_the_guard_drops_without_an_auth_store_a_new_claim_succeeds() {
    // A failed onboarding (wrong pairing code, derivation error…) writes no
    // auth.store; the user must be able to try again.
    let dir = tempfile::tempdir().unwrap();
    let first = claim(dir.path()).expect("first claim");
    drop(first);
    let again = claim(dir.path());
    assert!(
        again.is_ok(),
        "after the guard is dropped and no auth.store was written, a new claim must succeed, got {:?}",
        again.as_ref().err()
    );
}

#[test]
fn an_existing_auth_store_is_refused_already_onboarded() {
    let dir = tempfile::tempdir().unwrap();
    write_auth_store(dir.path());
    let r = claim(dir.path());
    assert!(
        matches!(r, Err(ClaimRefused::AlreadyOnboarded)),
        "with crypto/auth.store present, claim must refuse AlreadyOnboarded, got {:?}",
        r.as_ref().err()
    );
}

#[test]
fn after_a_completed_onboarding_a_new_claim_is_refused_already_onboarded() {
    let dir = tempfile::tempdir().unwrap();
    let guard = claim(dir.path()).expect("first claim");
    write_auth_store(dir.path()); // the onboarding completed
    drop(guard);
    let r = claim(dir.path());
    assert!(
        matches!(r, Err(ClaimRefused::AlreadyOnboarded)),
        "once an onboarding has completed (auth.store written, claim released), claim must refuse AlreadyOnboarded, got {:?}",
        r.as_ref().err()
    );
}

#[test]
fn a_refused_claim_changes_no_file() {
    let dir = tempfile::tempdir().unwrap();
    write_auth_store(dir.path());
    let before = snapshot(dir.path());
    let _ = claim(dir.path());
    assert_eq!(snapshot(dir.path()), before, "an AlreadyOnboarded refusal must change no file");

    let fresh = tempfile::tempdir().unwrap();
    let held = claim(fresh.path()).expect("first claim");
    let before = snapshot(fresh.path());
    let _ = claim(fresh.path());
    assert_eq!(snapshot(fresh.path()), before, "an InProgress refusal must change no file");
    drop(held);
}

/// Every file under `dir`, with its bytes.
fn snapshot(dir: &Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let bytes = std::fs::read(&p).unwrap_or_default();
                out.push((p, bytes));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn concurrent_claims_on_one_fresh_profile_let_exactly_one_through() {
    const N: usize = 8;
    let dir = tempfile::tempdir().unwrap();
    let path = Arc::new(dir.path().to_path_buf());
    let start = Arc::new(Barrier::new(N));
    // Every thread keeps its guard until all have tried, so a fast thread
    // cannot release its claim before the others ask.
    let tried = Arc::new(Barrier::new(N));

    let handles: Vec<_> = (0..N)
        .map(|_| {
            let path = Arc::clone(&path);
            let start = Arc::clone(&start);
            let tried = Arc::clone(&tried);
            std::thread::spawn(move || {
                start.wait();
                let r = claim(&path);
                let outcome = match &r {
                    Ok(_) => Ok(()),
                    Err(e) => Err(format!("{e:?}")),
                };
                let in_progress = matches!(r, Err(ClaimRefused::InProgress));
                tried.wait();
                drop(r);
                (outcome, in_progress)
            })
        })
        .collect();

    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let oks = results.iter().filter(|(o, _)| o.is_ok()).count();
    let in_progress = results.iter().filter(|(_, ip)| *ip).count();
    assert_eq!(
        oks, 1,
        "exactly one of {N} concurrent claims must succeed, got {oks}: {:?}",
        results.iter().map(|(o, _)| o).collect::<Vec<_>>()
    );
    assert_eq!(
        in_progress,
        N - 1,
        "the other {} concurrent claims must be refused InProgress: {:?}",
        N - 1,
        results.iter().map(|(o, _)| o).collect::<Vec<_>>()
    );
}

#[test]
fn claims_on_different_profile_dirs_do_not_block_each_other() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let ga = claim(a.path()).expect("claim on profile a");
    let gb = claim(b.path());
    assert!(
        gb.is_ok(),
        "a claim held on one profile directory must not block another, got {:?}",
        gb.as_ref().err()
    );
    drop(ga);
    drop(gb);
}

/// Spec ch. 07 as extended (`7506579`): "Account creation and backup recovery
/// are single-flight on the phone, one claim excluding the other." The claim is
/// keyed by the profile directory alone — no kind of flow — so a recovery that
/// arrives during an onboarding (or the reverse) is refused `InProgress`; the
/// order scan checks that both commands claim on the profile directory.
#[test]
fn an_onboarding_and_a_backup_recovery_on_one_profile_exclude_each_other() {
    let dir = tempfile::tempdir().unwrap();
    let onboarding = claim(dir.path()).expect("the onboarding's claim");
    let recovery = claim(dir.path());
    assert!(
        matches!(recovery, Err(ClaimRefused::InProgress)),
        "a recovery during an onboarding must be refused InProgress, got {:?}",
        recovery.as_ref().err()
    );
    drop(onboarding);

    let recovery = claim(dir.path()).expect("the recovery's claim, once the onboarding failed");
    let onboarding = claim(dir.path());
    assert!(
        matches!(onboarding, Err(ClaimRefused::InProgress)),
        "an onboarding during a recovery must be refused InProgress, got {:?}",
        onboarding.as_ref().err()
    );
    drop(recovery);
}

#[test]
fn the_refusals_display_the_spec_sentences() {
    assert_eq!(ClaimRefused::InProgress.to_string(), "Setup is already in progress");
    assert_eq!(
        ClaimRefused::AlreadyOnboarded.to_string(),
        "Already onboarded — log in instead"
    );
    assert_ne!(ClaimRefused::InProgress, ClaimRefused::AlreadyOnboarded);
}
