//! C-SUP on the app's backend — what `AppState::node_refusal` answers
//! (DEL-0019, the Fable maille by delegation of Céline; spec ch. 05, *The
//! node's supervision*, SU2 and SU5), on a real `AppState`.
//!
//! Written before the code, by another author; the implementer does not edit
//! this file. The `AppState` is the one `lot1_onboarding_tests::launch` builds
//! (an in-memory store, no orchestrator, no node) over a fresh profile, with
//! `SOVEREIGN_DATA_DIR` pointed at it by `fresh_profile` (one test at a time,
//! the variable being process wide). The state goes into « stopped » through
//! `AppState::on_sync_stopped`, what the supervisor's `on_stop` calls.
//!
//! A stop ends with its session (spec ch. 05, C-SUP SU4, v2.0.105; the Fable
//! maille, by delegation of Céline, 2026-11-15; C-AREA AR4, ch. 07). The session's end is what a login (a
//! switch) runs: `install_session_without_sync`, the login path without
//! starting a node (no listening socket in a test), which runs
//! `end_previous_session` (it stops the node with `stop_p2p_node` and drops the
//! previous session's keys and services) before it installs the new session.
//!
//! The call sites of `node_refusal` (the three commands that need the node)
//! are held in `tests/csup_call_sites.rs`; this file holds what it answers,
//! so a body that always returns its argument fails here.

use sovereign_p2p::supervision::NodeEnd;

use crate::lot1_onboarding_tests::{fresh_profile, launch};

/// A command's own sentence for a sync never started (the pairing command's).
const NEVER_STARTED: &str = "pairing requires the P2P node \u{2014} enable sync in Settings first";
/// The stop's notice text, the same words as the shell and the store (SU2).
const STOP_TEXT: &str = "Sync stopped unexpectedly. Nothing syncs until you restart it.";

// DEL-0019: when sync was never started, node_refusal returns the command's own sentence, unchanged.
#[test]
fn node_refusal_when_sync_was_never_started_returns_the_given_sentence() {
    let (_guard, dir) = fresh_profile("csup-never");
    let st = launch(&dir);
    assert_eq!(st.node_refusal(NEVER_STARTED), NEVER_STARTED);
}

// DEL-0019, SU2: after the node's end was observed, node_refusal returns the stop's text, never the given sentence.
#[test]
fn node_refusal_after_the_node_s_end_was_observed_returns_the_stop_s_text() {
    let (_guard, dir) = fresh_profile("csup-stopped");
    let st = launch(&dir);
    st.on_sync_stopped(NodeEnd::Panicked("swarm poll failed".into()));
    let said = st.node_refusal(NEVER_STARTED);
    assert_eq!(said, STOP_TEXT, "while sync is stopped, the stop's notice text");
    assert!(!said.contains("enable sync"), "« enable sync … first » while logged in and stopped");
    assert!(!said.contains("swarm poll failed"), "why it stopped goes to the log, not to the user");
}

// SU5: after an end the session asked for (stop_p2p_node: a logout, a switch, a lock), no stop is said: node_refusal returns the given sentence.
#[test]
fn node_refusal_after_an_end_the_session_asked_for_returns_the_given_sentence() {
    let (_guard, dir) = fresh_profile("csup-asked");
    let st = launch(&dir);
    tauri::async_runtime::block_on(crate::sync_startup::stop_p2p_node(&st));
    assert_eq!(st.node_refusal(NEVER_STARTED), NEVER_STARTED, "an asked end was said as a stop");
}

const MAIN: &str = "Csup!Primary-Pass1";
const DURESS: &str = "Csup!Duress-Pass2";

// SU4, a stop ends with its session: after a stop, the session's end (a login, a switch) ends the stopped state: node_refusal gives back the command's own sentence, never the stop's text.
#[test]
fn node_refusal_after_a_stop_and_the_session_s_end_returns_the_given_sentence() {
    let (_guard, dir) = fresh_profile("csup-ended");
    let st = launch(&dir);
    st.on_sync_stopped(NodeEnd::Returned);
    // An account to log in to, as the setup leaves it.
    let crypto = crate::setup::crypto_dir();
    std::fs::create_dir_all(&crypto).unwrap();
    let salt: [u8; 32] = [5u8; 32];
    let store = sovereign_crypto::auth::AuthStore::create(MAIN.as_bytes(), DURESS.as_bytes(), &salt, "device-csup")
        .expect("an account");
    store.save_new(&crypto.join("auth.store")).expect("save_new");
    std::fs::write(crypto.join("salt"), salt).expect("the salt");
    // The session's end, as a login runs it.
    let r = tauri::async_runtime::block_on(crate::tauri_commands::auth::install_session_without_sync(
        &st,
        &store,
        MAIN.as_bytes(),
    ));
    assert!(r.is_ok(), "the login: {:?}", r.err());
    let said = st.node_refusal(NEVER_STARTED);
    assert_eq!(said, NEVER_STARTED, "the stopped state outlived its session");
    assert_ne!(said, STOP_TEXT);
}

// SU1 (Céline, 2026-11-16): a start that fails is said as a stop; on the app, a key missing at the start is an error of the start. With P2P on and no identity key, start_sync_now's start fails before any node is built, and node_refusal then gives the stop's text, never the reason.
#[test]
fn node_refusal_after_a_failed_start_returns_the_stop_s_text() {
    let (_guard, dir) = fresh_profile("csup-failed-start");
    let mut st = launch(&dir);
    st.config.p2p.enabled = true;
    tauri::async_runtime::block_on(crate::tauri_commands::auth::start_sync_now(&st));
    let said = st.node_refusal(NEVER_STARTED);
    assert_eq!(said, STOP_TEXT, "a failed start is said as a stop");
    assert!(!said.contains("identity key"), "the reason goes to the log, not to the user");
}
