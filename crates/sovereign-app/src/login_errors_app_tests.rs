//! Login errors say what failed, in the app (spec ch. 07, « Login errors say what failed —
//! contract (LE) », LE2, LE4, and LE5 for the words). Specified 2026-11-19 (Céline, through the
//! Fable maille). Written before the code, by another author: whoever implements the contract
//! makes these pass without editing this file; a test that looks wrong is reported, not worked
//! around.
//!
//! The login command runs for real, as in `lot1_onboarding_tests` (whose harness it shares): an
//! `AppState` over a temporary profile, a real `auth.store` written there, damaged on disk where a
//! test needs it, and the persisted attempt tracker read back from the profile after each login.
//!
//! What is held:
//! - a wrong password reads « Invalid password » and the tracker records one failure;
//! - the right password whose keys do not unwrap reads « Your password is right, but your account
//!   could not be opened: {cause}. », the same for the primary and the duress persona, and the
//!   tracker records nothing;
//! - a derivation that fails reads « Your password could not be checked: {cause}. This is not a
//!   wrong password. », and the tracker records nothing.
//!
//! The command line's « authentication failed: wrong password » (LE2) is not tested here: the
//! command line reads the passphrase from a terminal (`rpassword`), which a test does not have.

use std::path::Path;

use crate::lot1_onboarding_tests::{fresh_profile, launch, state, DURESS, MAIN};
use crate::login_throttle::LoginAttempts;
use crate::tauri_commands::auth::validate_password;
use sovereign_crypto::auth::AuthStore;
use sovereign_crypto::master_key::Kdf;

const WRONG: &str = "Wrong!Pass9012";
const INVALID: &str = "Invalid password";
const NOT_CHECKED: &str = "Your password could not be checked: ";
const NOT_CHECKED_END: &str = ". This is not a wrong password.";
const RIGHT_NOT_OPENED: &str = "Your password is right, but your account could not be opened: ";

fn block_on<F: std::future::Future>(f: F) -> F::Output {
    tauri::async_runtime::block_on(f)
}

fn store_path(dir: &Path) -> std::path::PathBuf {
    dir.join("crypto").join("auth.store")
}

/// Writes an `auth.store` with a primary and a duress entry in the profile.
fn write_store(dir: &Path) {
    std::fs::create_dir_all(dir.join("crypto")).unwrap();
    let store = AuthStore::create(MAIN.as_bytes(), DURESS.as_bytes(), b"login-errors-salt-32-bytes-long!", "login-errors")
        .expect("a store");
    store.save(&store_path(dir)).expect("write auth.store");
}

fn damage_store(dir: &Path, damage: impl FnOnce(&mut AuthStore)) {
    let mut store = AuthStore::load(&store_path(dir)).expect("load auth.store");
    damage(&mut store);
    store.save(&store_path(dir)).expect("write auth.store back");
}

fn flip(bytes: &mut [u8]) {
    assert!(!bytes.is_empty(), "nothing to damage");
    for b in bytes.iter_mut() {
        *b ^= 0xA5;
    }
}

/// The failures the persisted tracker holds for this profile.
fn failures(dir: &Path) -> u32 {
    LoginAttempts::load(&dir.join("crypto")).failed_count
}

/// A login on a fresh launch of the app over `dir`; it must fail. Returns its text.
fn login_error(dir: &Path, password: &str) -> String {
    let app = launch(dir);
    match block_on(validate_password(state(&app), password.into(), vec![])) {
        Ok(persona) => panic!("the login was meant to fail, and it opened the {persona} persona"),
        Err(e) => e,
    }
}

fn assert_right_not_opened(text: &str, what: &str) {
    let cause = text
        .strip_prefix(RIGHT_NOT_OPENED)
        .and_then(|c| c.strip_suffix('.'))
        .unwrap_or_else(|| panic!("LE2, {what}: expected « {RIGHT_NOT_OPENED}{{cause}}. », got « {text} »"));
    assert!(!cause.trim().is_empty(), "LE3, {what}: the cause is kept: « {text} »");
}

fn assert_not_checked(text: &str, what: &str) {
    let cause = text
        .strip_prefix(NOT_CHECKED)
        .and_then(|c| c.strip_suffix(NOT_CHECKED_END))
        .unwrap_or_else(|| panic!("LE2, {what}: expected « {NOT_CHECKED}{{cause}}{NOT_CHECKED_END} », got « {text} »"));
    assert!(!cause.trim().is_empty(), "LE3, {what}: the cause is kept: « {text} »");
}

// LE2, LE4: a wrong password reads « Invalid password », and each one is counted.
#[test]
fn a_wrong_password_reads_invalid_password_and_is_counted() {
    let (_g, dir) = fresh_profile("login-errors-wrong");
    write_store(&dir);
    assert_eq!(failures(&dir), 0, "precondition: no failure recorded");
    assert_eq!(login_error(&dir, WRONG), INVALID, "LE2: a wrong password reads « Invalid password »");
    assert_eq!(failures(&dir), 1, "LE4: a wrong password is recorded as a failure");
    assert_eq!(login_error(&dir, WRONG), INVALID, "LE2: a second wrong password");
    assert_eq!(failures(&dir), 2, "LE4: every wrong guess is counted");
}

// LE2, LE4, LE5: the right password, its keys damaged: the third sentence, the same for both
// personas, and nothing counted.
#[test]
fn the_right_password_whose_keys_do_not_unwrap_reads_that_it_is_right_and_is_not_counted() {
    let (_g, dir) = fresh_profile("login-errors-kek");
    write_store(&dir);
    damage_store(&dir, |s| {
        for entry in s.personas.iter_mut() {
            flip(&mut entry.wrapped_kek.ciphertext);
        }
    });
    let primary = login_error(&dir, MAIN);
    assert_right_not_opened(&primary, "primary, KEK damaged");
    assert_eq!(failures(&dir), 0, "LE4: a failure after the password matched is not counted");
    let duress = login_error(&dir, DURESS);
    assert_right_not_opened(&duress, "duress, KEK damaged");
    assert_eq!(failures(&dir), 0, "LE4: nor for the duress persona");
    assert_eq!(primary, duress, "LE5: the same failure reads the same for the primary and the duress persona");
    // A wrong password on the same store is still a wrong password, and is counted.
    assert_eq!(login_error(&dir, WRONG), INVALID, "LE2: a wrong password, on a store with damaged keys");
    assert_eq!(failures(&dir), 1, "LE4: a wrong guess is still counted");
}

// LE2, LE4: the derivation fails: the second sentence, whatever the password, and nothing counted.
#[test]
fn a_derivation_that_fails_reads_that_the_password_could_not_be_checked_and_is_not_counted() {
    let (_g, dir) = fresh_profile("login-errors-kdf");
    write_store(&dir);
    damage_store(&dir, |s| {
        s.kdf = Kdf::Argon2id {
            m_cost_kib: 0,
            t_cost: 0,
            p_cost: 0,
        }
    });
    let primary = login_error(&dir, MAIN);
    assert_not_checked(&primary, "primary, derivation refused");
    let wrong = login_error(&dir, WRONG);
    assert_not_checked(&wrong, "a wrong password, derivation refused");
    assert_eq!(failures(&dir), 0, "LE4: a derivation that fails is not counted, whatever was typed");
    assert_eq!(login_error(&dir, DURESS), primary, "LE5: the same for the duress persona's password");
}
