//! Login errors say what failed, on the desktop (spec ch. 07, « Login errors say what failed —
//! contract (LE) », LE2, LE3 and LE5). Specified 2026-11-19 (Céline, through the Fable maille).
//! Written before the code, by another author: whoever implements the contract makes these pass
//! without editing this file; a test that looks wrong is reported, not worked around.
//!
//! What the user reads on the login form:
//! - a wrong password: « Invalid password »;
//! - the key derivation failed: « Your password could not be checked: {cause}. This is not a
//!   wrong password. »;
//! - a probe opened, then the keys' unwrapping or the session's installation failed: « Your
//!   password is right, but your account could not be opened: {cause}. »;
//! - at the end of a first setup, « Your account was created, but it could not be opened:
//!   {error} », where `{error}` is the cause alone.
//!
//! `{cause}` names no path or file of the data directory (LE3), and the same failure made for the
//! primary and for the duress persona reads the same, character for character (LE5).
//!
//! A child of `app`, like `clog_login_tests`: each scenario is a launch of a real, headless `App`
//! in a child process, on a temporary profile of its own (`SOVEREIGN_DATA_DIR` is read by the
//! whole process), with the real store and the real derivations. The store is damaged on disk
//! between its creation and the login, as a damaged profile would be. Where the parent compares
//! two launches, the child leaves what it read in a file of its profile.

use std::path::Path;

use sovereign_crypto::auth::AuthStore;
use sovereign_crypto::master_key::Kdf;
use sovereign_db::surreal::{StorageMode, SurrealGraphDB};
use sovereign_db::GraphDB;

use super::first_run_tests::{
    fill_wizard, first_frame, frames_until, launch, login_settled, press_finish, profile, setup_settled, shut_runtime,
    submit_login, CHILD_VAR, DURESS, PRIMARY,
};
use super::App;

const WRONG: &str = "Wrong!Pass9012";

const INVALID: &str = "Invalid password";
const NOT_CHECKED: &str = "Your password could not be checked: ";
const NOT_CHECKED_END: &str = ". This is not a wrong password.";
const RIGHT_NOT_OPENED: &str = "Your password is right, but your account could not be opened: ";
const CREATED_NOT_OPENED: &str = "Your account was created, but it could not be opened: ";

fn module() -> &'static str {
    module_path!()
}

// ------------------------------------------------------------------ helpers (in the child)

/// Logs in with `password` on the login form, as the user does, and returns what the form says.
/// The screen must stay locked.
fn login_error(app: &mut App, password: &str) -> String {
    submit_login(app, password);
    assert!(frames_until(app, 240, login_settled), "the login ends");
    assert!(app.locked, "the login was meant to fail, and the account opened");
    app.auth_form.error.clone().expect("a failed login says why")
}

/// Loads `auth.store`, changes it with `damage`, and writes it back.
fn damage_store(damage: impl FnOnce(&mut AuthStore)) {
    let path = crate::crypto::auth_store_path();
    let mut store = AuthStore::load(&path).expect("load auth.store");
    damage(&mut store);
    store.save(&path).expect("write auth.store back");
}

fn flip(bytes: &mut [u8]) {
    assert!(!bytes.is_empty(), "nothing to damage");
    for b in bytes.iter_mut() {
        *b ^= 0xA5;
    }
}

/// A fresh App on a profile with an `auth.store`: the login form.
fn app_on_login_form() -> App {
    let mut app = App::new();
    first_frame(&mut app);
    assert!(app.wizard.is_none(), "a profile with auth.store shows the login form");
    app
}

fn done(mut app: App) {
    shut_runtime(&mut app);
    std::mem::forget(app);
}

/// Leaves `text` in the profile for the parent to read.
fn leave(name: &str, text: &str) {
    std::fs::write(sovereign_core::sovereign_dir().join(format!("seen-{name}.txt")), text).unwrap();
}

// ------------------------------------------------------------------ what is checked

/// LE3: what is shown names no path and no file of the data directory.
fn assert_names_no_file(text: &str, data_dir: &Path, what: &str) {
    let dir = data_dir.to_string_lossy();
    let dir_name = data_dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut forbidden: Vec<String> = vec![
        dir.into_owned(),
        "sovereign.db".into(),
        "sovereign-duress.db".into(),
        ".duress".into(),
        "LOCK".into(),
        "keys.".into(),
        "index.key".into(),
        "auth.store".into(),
        "/data/".into(),
        "/crypto".into(),
    ];
    if dir_name.len() > 3 {
        forbidden.push(dir_name);
    }
    for f in forbidden {
        assert!(!text.contains(&f), "LE3, {what}: the text names « {f} » of the data directory: « {text} »");
    }
}

/// `text` with each run of ASCII digits replaced by one `#`.
fn digits_masked(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_digits = false;
    for c in text.chars() {
        if c.is_ascii_digit() {
            if !in_digits {
                out.push('#');
            }
            in_digits = true;
        } else {
            out.push(c);
            in_digits = false;
        }
    }
    out
}

/// The third sentence; returns its cause.
fn assert_right_not_opened(text: &str, what: &str) -> String {
    let cause = text.strip_prefix(RIGHT_NOT_OPENED).unwrap_or_else(|| {
        panic!("LE2, {what}: expected « {RIGHT_NOT_OPENED}{{cause}}. », got « {text} »")
    });
    let cause = cause
        .strip_suffix('.')
        .unwrap_or_else(|| panic!("LE2, {what}: the sentence ends with a full stop: « {text} »"));
    assert!(!cause.trim().is_empty(), "LE3, {what}: the cause is kept: « {text} »");
    cause.to_string()
}

/// The second sentence; returns its cause.
fn assert_not_checked(text: &str, what: &str) -> String {
    let cause = text
        .strip_prefix(NOT_CHECKED)
        .and_then(|c| c.strip_suffix(NOT_CHECKED_END))
        .unwrap_or_else(|| {
            panic!("LE2, {what}: expected « {NOT_CHECKED}{{cause}}{NOT_CHECKED_END} », got « {text} »")
        });
    assert!(!cause.trim().is_empty(), "LE3, {what}: the cause is kept: « {text} »");
    cause.to_string()
}

// ------------------------------------------------------------------ scenarios (in the child)

/// LE2: a wrong password reads « Invalid password », exactly.
fn wrong_password() {
    crate::crypto::create_auth_store(PRIMARY.as_bytes(), DURESS.as_bytes()).expect("create auth.store");
    let mut app = app_on_login_form();
    let text = login_error(&mut app, WRONG);
    assert_eq!(text, INVALID, "LE2: a wrong password reads « Invalid password »");
    done(app);
}

/// LE2, LE5: both entries' KEK damaged, their probes whole. Each password is right; its keys do
/// not unwrap. The two read the third sentence, the same.
fn kek_damaged() {
    crate::crypto::create_auth_store(PRIMARY.as_bytes(), DURESS.as_bytes()).expect("create auth.store");
    damage_store(|s| {
        for entry in s.personas.iter_mut() {
            flip(&mut entry.wrapped_kek.ciphertext);
        }
    });
    let mut app = app_on_login_form();
    let primary = login_error(&mut app, PRIMARY);
    let duress = login_error(&mut app, DURESS);
    assert_right_not_opened(&primary, "primary, KEK damaged");
    assert_right_not_opened(&duress, "duress, KEK damaged");
    let data = sovereign_core::sovereign_dir();
    assert_names_no_file(&primary, &data, "primary, KEK damaged");
    assert_eq!(primary, duress, "LE5: the same failure reads the same for the primary and the duress persona");
    // A wrong password on the same store still reads « Invalid password » (LE1: no probe opened).
    assert_eq!(login_error(&mut app, WRONG), INVALID, "LE2: a wrong password, on a store with damaged keys");
    done(app);
}

/// LE2, LE5: a derivation the store's parameters refuse. Whatever the password, it could not be
/// checked; never « Invalid password ».
fn derivation_refused() {
    crate::crypto::create_auth_store(PRIMARY.as_bytes(), DURESS.as_bytes()).expect("create auth.store");
    damage_store(|s| {
        s.kdf = Kdf::Argon2id {
            m_cost_kib: 0,
            t_cost: 0,
            p_cost: 0,
        }
    });
    let mut app = app_on_login_form();
    let primary = login_error(&mut app, PRIMARY);
    let duress = login_error(&mut app, DURESS);
    let wrong = login_error(&mut app, WRONG);
    assert_not_checked(&primary, "primary, derivation refused");
    assert_not_checked(&wrong, "a wrong password, derivation refused");
    assert_names_no_file(&primary, &sovereign_core::sovereign_dir(), "derivation refused");
    assert_eq!(primary, duress, "LE5: the same failure reads the same for the primary and the duress persona");
    done(app);
}

/// LE2, LE3, LE5: each persona's database held open elsewhere in this process, so the session's
/// own open of it fails (a database that is locked).
fn database_locked() {
    crate::crypto::create_auth_store(PRIMARY.as_bytes(), DURESS.as_bytes()).expect("create auth.store");
    let mut app = app_on_login_form();
    // Held through the database's own type, not the shell's opening function, whose signature
    // the contract changes (it returns its error).
    let mut held = Vec::new();
    for persona in [sovereign_core::auth::PersonaKind::Primary, sovereign_core::auth::PersonaKind::Duress] {
        let path = crate::crypto::persona_raw_db_path(persona);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let db = app
            .rt
            .block_on(SurrealGraphDB::new(StorageMode::Persistent(path.to_string_lossy().into_owned())))
            .unwrap_or_else(|e| panic!("precondition: the test opens the {persona:?} database: {e}"));
        app.rt
            .block_on(db.connect())
            .unwrap_or_else(|e| panic!("precondition: the test holds the {persona:?} database open: {e}"));
        held.push(db);
    }
    let primary = login_error(&mut app, PRIMARY);
    let duress = login_error(&mut app, DURESS);
    let cause = assert_right_not_opened(&primary, "primary, database locked");
    assert_right_not_opened(&duress, "duress, database locked");
    let data = sovereign_core::sovereign_dir();
    assert_names_no_file(&primary, &data, "primary, database locked");
    assert_names_no_file(&duress, &data, "duress, database locked");
    // On Linux, RocksDB's lock error carries the time the lock was taken and the thread that took
    // it (« lock hold by current process, acquire time N acquiring thread N »). Those depend on the
    // moment, not on the persona, so the two texts are compared with their runs of digits masked
    // (decided 2026-11-19, the Fable maille, by delegation of Céline). Everything else must match.
    assert_eq!(
        digits_masked(&primary),
        digits_masked(&duress),
        "LE5: a locked database reads the same for the primary and the duress persona, but for the lock's own time"
    );
    leave("database_locked", &cause);
    drop(held);
    done(app);
}

/// LE2, LE3, LE5: a regular file where each persona's database must be (a database that cannot
/// be opened for another reason than a lock).
fn database_not_openable() {
    crate::crypto::create_auth_store(PRIMARY.as_bytes(), DURESS.as_bytes()).expect("create auth.store");
    for persona in [sovereign_core::auth::PersonaKind::Primary, sovereign_core::auth::PersonaKind::Duress] {
        let path = crate::crypto::persona_raw_db_path(persona);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"not a database").unwrap();
    }
    let mut app = app_on_login_form();
    let primary = login_error(&mut app, PRIMARY);
    let duress = login_error(&mut app, DURESS);
    let cause = assert_right_not_opened(&primary, "primary, database not openable");
    assert_right_not_opened(&duress, "duress, database not openable");
    let data = sovereign_core::sovereign_dir();
    assert_names_no_file(&primary, &data, "primary, database not openable");
    assert_names_no_file(&duress, &data, "duress, database not openable");
    assert_eq!(primary, duress, "LE5: the same failure reads the same for the primary and the duress persona");
    leave("database_not_openable", &cause);
    done(app);
}

/// LE2, LE3, LE5: each persona's key database (the documents' one) is not a key database: the
/// session's installation fails after the probe opened.
fn key_database_damaged() {
    crate::crypto::create_auth_store(PRIMARY.as_bytes(), DURESS.as_bytes()).expect("create auth.store");
    let crypto = crate::crypto::crypto_dir();
    for persona in [sovereign_core::auth::PersonaKind::Primary, sovereign_core::auth::PersonaKind::Duress] {
        let name = crate::crypto::persona_key_db_filename(persona, "keys.db");
        std::fs::write(crypto.join(name), b"not a key database").unwrap();
    }
    let mut app = app_on_login_form();
    let primary = login_error(&mut app, PRIMARY);
    let duress = login_error(&mut app, DURESS);
    assert_right_not_opened(&primary, "primary, key database damaged");
    assert_right_not_opened(&duress, "duress, key database damaged");
    let data = sovereign_core::sovereign_dir();
    assert_names_no_file(&primary, &data, "primary, key database damaged");
    assert_names_no_file(&duress, &data, "duress, key database damaged");
    assert_eq!(primary, duress, "LE5: the same failure reads the same for the primary and the duress persona");
    done(app);
}

/// LE2, LE3: a first setup whose installation fails after the store is written (a key database
/// that is not one, as `first_run_tests`' failure after the store). The setup's sentence keeps its
/// words, and its `{error}` is the cause alone: none of the login's sentences inside it.
fn setup_fails_after_store() {
    let crypto = crate::crypto::crypto_dir();
    std::fs::create_dir_all(&crypto).unwrap();
    std::fs::write(crypto.join("keys.db"), b"not a key database").unwrap();
    let mut app = App::new();
    first_frame(&mut app);
    fill_wizard(&mut app, PRIMARY, DURESS);
    press_finish(&mut app);
    assert!(frames_until(&mut app, 240, setup_settled), "the setup ends");
    assert!(crate::crypto::auth_store_exists(), "precondition: the store was written before the failure");
    assert!(app.locked, "precondition: the installation failed");
    let text = app
        .auth_form
        .error
        .clone()
        .or_else(|| app.wizard.as_ref().and_then(|w| w.error.clone()))
        .expect("the failure is said");
    let error = text
        .strip_prefix(CREATED_NOT_OPENED)
        .unwrap_or_else(|| panic!("LE2: the setup's sentence keeps its words: « {CREATED_NOT_OPENED}{{error}} », got « {text} »"));
    for nested in ["Your password", "This is not a wrong password", INVALID] {
        assert!(
            !error.contains(nested),
            "LE2: the setup's {{error}} is the cause alone, without « {nested} »: « {text} »"
        );
    }
    assert!(!error.trim().is_empty(), "LE3: the cause is kept: « {text} »");
    assert_names_no_file(&text, &sovereign_core::sovereign_dir(), "setup failed after the store");
    done(app);
}

/// One launch on the profile of `SOVEREIGN_DATA_DIR` (set by the parent). Ignored in ordinary
/// runs.
#[test]
#[ignore = "a launch, run by its parent test in a child process on a profile of its own"]
fn child_entry() {
    let Ok(scenario) = std::env::var(CHILD_VAR) else { return };
    match scenario.as_str() {
        "wrong_password" => wrong_password(),
        "kek_damaged" => kek_damaged(),
        "derivation_refused" => derivation_refused(),
        "database_locked" => database_locked(),
        "database_not_openable" => database_not_openable(),
        "key_database_damaged" => key_database_damaged(),
        "setup_fails_after_store" => setup_fails_after_store(),
        other => panic!("unknown scenario {other}"),
    }
}

// ------------------------------------------------------------------ the tests (the parent)

#[test]
fn a_wrong_password_reads_invalid_password() {
    let p = profile();
    launch(module(), "wrong_password", p.path());
}

#[test]
fn the_right_password_whose_keys_do_not_unwrap_reads_that_the_password_is_right() {
    let p = profile();
    launch(module(), "kek_damaged", p.path());
}

#[test]
fn a_derivation_that_fails_reads_that_the_password_could_not_be_checked() {
    let p = profile();
    launch(module(), "derivation_refused", p.path());
}

#[test]
fn a_locked_persona_database_says_why_names_no_file_and_reads_the_same_for_both_personas() {
    let p = profile();
    launch(module(), "database_locked", p.path());
}

#[test]
fn an_unopenable_persona_database_says_why_names_no_file_and_reads_the_same_for_both_personas() {
    let p = profile();
    launch(module(), "database_not_openable", p.path());
}

#[test]
fn a_damaged_key_database_reads_that_the_password_is_right_the_same_for_both_personas() {
    let p = profile();
    launch(module(), "key_database_damaged", p.path());
}

/// LE3: the cause is the error's own text, never a fixed word in its place: two different
/// failures of the persona database (a lock, a file in its place) do not read the same cause.
#[test]
fn two_different_database_failures_keep_two_different_causes() {
    let locked = profile();
    launch(module(), "database_locked", locked.path());
    let not_openable = profile();
    launch(module(), "database_not_openable", not_openable.path());
    let a = std::fs::read_to_string(locked.path().join("seen-database_locked.txt")).unwrap();
    let b = std::fs::read_to_string(not_openable.path().join("seen-database_not_openable.txt")).unwrap();
    assert_ne!(a, b, "LE3: the cause is kept, not replaced by a fixed word: both read « {a} »");
}

#[test]
fn a_setup_that_fails_after_the_store_names_the_cause_alone() {
    let p = profile();
    launch(module(), "setup_fails_after_store", p.path());
}

