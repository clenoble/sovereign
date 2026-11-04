//! v0.0.12 lot 1, item 4 — account creation on the phone (P1–P3, and P4 as
//! `complete_onboarding` reports it), as amended in b35e0ff (P2b, P2c).
//!
//! Contract: `doc/plans/V0012-lot1.md`. Written before the code, by another
//! author: whoever implements item 4 makes these pass **without editing this
//! file**; a test that looks wrong is reported, not worked around.
//!
//! The commands run for real: `check_auth_state`, `complete_onboarding` and
//! `validate_password` are called with a `State<AppState>` over an
//! `AppState` built here — an in-memory primary store, no orchestrator, no
//! P2P — over a fresh profile directory per test. A Tauri mock app
//! (`tauri::test::mock_app`) cannot be used: it links comctl32's
//! `TaskDialogIndirect` (Common Controls v6), and a Windows test binary has
//! no manifest asking for v6, so it does not load (0xc0000139). `State` has
//! no public constructor; `state()` below makes one from a reference (test
//! seam, see its comment). `complete_onboarding_paired` needs a live source
//! device for its handshake; it is checked for what can be reached without
//! one (P3's refusal comes before the handshake) and by reading its source
//! (P2c's order).
//!
//! **Data directory.** Some paths of these commands write under
//! `sovereign_dir()` (`crypto_dir()`: key databases, the device id) rather
//! than under `state.profile_dir`; in production both are the same directory.
//! So each test points `SOVEREIGN_DATA_DIR` at its own temporary directory
//! (under the one the run set, or the system temporary directory) and uses it
//! as `profile_dir`, one test at a time (`ENV_LOCK`): the variable is process
//! wide.
//!
//! **API this file names** (the contract names the fields; the rest is the
//! closest fit, listed in the lot's report):
//! - `OnboardingData { password_confirm, duress_confirm, canary_confirm }`
//!   and the same three on `CompletePairedOnboardingInput` (`Option<String>`);
//! - `complete_onboarding(..) -> Result<OnboardingOutcome, String>` with
//!   `OnboardingOutcome { incomplete: bool }` (P2b), and the same return type
//!   for `complete_onboarding_paired`;
//! - `AuthCheckResult { setup_incomplete: bool }`: true while `auth.store`
//!   exists and the setup did not finish (P2's notice, "once": false again
//!   after the first login has written the marker).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use crate::tauri_commands::auth::{check_auth_state, complete_onboarding, validate_password};
use crate::tauri_commands::OnboardingData;
use crate::tauri_state::{AppState, ModelAssignments};

pub(crate) const MAIN: &str = "Main!Pass1234";
pub(crate) const DURESS: &str = "Duress!Pass5678";
pub(crate) const PHRASE: &str = "the heron waits at dawn";

pub(crate) static ENV_LOCK: Mutex<()> = Mutex::new(());

/// A fresh profile directory, which is also `sovereign_dir()` while the
/// returned guard is held.
pub(crate) fn fresh_profile(tag: &str) -> (std::sync::MutexGuard<'static, ()>, PathBuf) {
    let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    static BASE: OnceLock<PathBuf> = OnceLock::new();
    let base = BASE.get_or_init(|| {
        std::env::var_os("SOVEREIGN_DATA_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir)
    });
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = base.join(format!("lot1-onboarding-{}-{nanos}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("SOVEREIGN_DATA_DIR", &dir);
    (guard, dir)
}

/// The `State` a command receives, over `s`.
///
/// Test seam: `tauri::State<'r, T>` is a one-field tuple struct around
/// `&'r T` (tauri 2.11, `src/state.rs:21`) with no public constructor outside
/// a running app; `transmute` checks the sizes match. If tauri changes the
/// type, this stops compiling or must be revisited — it is not production
/// code.
pub(crate) fn state(s: &AppState) -> tauri::State<'_, AppState> {
    // SAFETY: `State<'r, T>` is `struct State<'r, T>(&'r T)`: one field, a
    // reference to `T`, with the same lifetime.
    unsafe { std::mem::transmute::<&AppState, tauri::State<'_, AppState>>(s) }
}

/// The `AppState` of a launch of the app over `profile_dir`.
pub(crate) fn launch(profile_dir: &Path) -> AppState {
    let raw = tauri::async_runtime::block_on(async {
        use sovereign_db::GraphDB;
        let db = sovereign_db::surreal::SurrealGraphDB::new(sovereign_db::surreal::StorageMode::Memory).await.unwrap();
        db.connect().await.unwrap();
        db.init_schema().await.unwrap();
        db
    });
    let db = Arc::new(sovereign_db::layered::LayeredGraphDB::new(Arc::new(raw)));
    let mut config = sovereign_core::config::AppConfig::default();
    config.database.mode = "memory".into();
    config.database.path = profile_dir.join("data").join("sovereign.db").to_string_lossy().into_owned();
    let (decision_tx, _decision_rx) = tokio::sync::mpsc::channel(8);
    let (feedback_tx, _feedback_rx) = tokio::sync::mpsc::channel(8);
    let (orch_tx, _orch_rx) = std::sync::mpsc::channel();
    let skill_db: Arc<dyn sovereign_skills::SkillDbAccess> = sovereign_skills::wrap_db(db.clone());
    AppState {
        db,
        orchestrator: None,
        config,
        skill_registry: Arc::new(sovereign_skills::SkillRegistry::new()),
        skill_db,
        skill_llm: None,
        decision_tx,
        feedback_tx,
        orch_tx,
        theme: Mutex::new("dark".into()),
        model_assignments: Mutex::new(ModelAssignments { router: String::new(), reasoning: String::new() }),
        profile_dir: profile_dir.to_path_buf(),
        account_key: tokio::sync::RwLock::new(None),
        kek: tokio::sync::RwLock::new(None),
        recovery_session: tokio::sync::RwLock::new(None),
        p2p_identity_key: tokio::sync::RwLock::new(None),
        pending_pairing: tokio::sync::RwLock::new(None),
        docs: tokio::sync::RwLock::new(None),
        migration_task: Mutex::new(None),
        edit_sessions: Mutex::new(std::collections::HashMap::new()),
        #[cfg(feature = "p2p")]
        p2p_command_tx: tokio::sync::RwLock::new(None),
        #[cfg(feature = "p2p")]
        pairing_manager: Arc::new(tokio::sync::RwLock::new(None)),
        #[cfg(feature = "p2p")]
        p2p_listen_addrs: Arc::new(std::sync::RwLock::new(Vec::new())),
        #[cfg(feature = "p2p")]
        backup_host: tokio::sync::RwLock::new(None),
        #[cfg(feature = "p2p")]
        connectivity: Arc::new(std::sync::atomic::AtomicU8::new(0)),
        #[cfg(feature = "voice-stt")]
        stt_engine: None,
    }
}

fn auth_store_path(dir: &Path) -> PathBuf {
    dir.join("crypto").join("auth.store")
}

fn marker(dir: &Path) -> PathBuf {
    dir.join("onboarding_done")
}

/// What the wizard sends for a first device, every confirmation matching.
pub(crate) fn data(password: &str, duress: Option<&str>, phrase: Option<&str>) -> OnboardingData {
    OnboardingData {
        nickname: Some("Tester".into()),
        bubble_style: None,
        seed_sample_data: false,
        password: Some(password.into()),
        password_confirm: Some(password.into()),
        duress_password: duress.map(Into::into),
        duress_confirm: duress.map(Into::into),
        canary_phrase: phrase.map(Into::into),
        canary_confirm: phrase.map(Into::into),
        keystrokes: vec![],
    }
}

fn check(app: &AppState) -> crate::tauri_commands::AuthCheckResult {
    tauri::async_runtime::block_on(check_auth_state(state(app))).unwrap()
}

/// Writes `auth.store` as `complete_onboarding` does, and nothing after it:
/// the state a kill leaves just after the store (P2).
fn store_written_then_killed(dir: &Path) {
    let crypto = dir.join("crypto");
    std::fs::create_dir_all(&crypto).unwrap();
    let salt: [u8; 32] = [3u8; 32];
    let store = sovereign_crypto::auth::AuthStore::create(MAIN.as_bytes(), DURESS.as_bytes(), &salt, "device-p2")
        .expect("create");
    store.save_new(&auth_store_path(dir)).expect("save_new");
}

/// Every file name under `dir`, recursively (what a refused request must
/// not have written).
fn files_under(dir: &Path) -> Vec<String> {
    let mut out = vec![];
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(files_under(&p));
            } else {
                out.push(p.to_string_lossy().into_owned());
            }
        }
    }
    out
}

// ======================================================================= P1

// P1: "In a build with encryption, the app shows the login screen when
// `auth.store` exists and the setup wizard when it does not; the marker no
// longer decides." `auth.store` present, the marker absent (a setup killed
// after the store): login.
#[test]
fn p1_auth_store_without_the_marker_routes_to_login() {
    let (_g, dir) = fresh_profile("p1-login");
    store_written_then_killed(&dir);
    assert!(!marker(&dir).exists());
    let r = check(&launch(&dir));
    assert!(r.needs_login, "auth.store exists: login");
    assert!(!r.needs_onboarding, "not the wizard (its claim would refuse: 'Already onboarded')");
}

// P1, the other side: no `auth.store`, a stale marker: the wizard.
#[test]
fn p1_no_auth_store_routes_to_the_wizard_whatever_the_marker_says() {
    let (_g, dir) = fresh_profile("p1-wizard");
    std::fs::write(marker(&dir), "1").unwrap();
    let r = check(&launch(&dir));
    assert!(r.needs_onboarding, "no auth.store: the wizard");
    assert!(!r.needs_login);
}

// P1, unchanged cases (expected green today): neither file → wizard; both →
// login.
#[test]
fn p1_a_fresh_profile_and_a_finished_one_route_as_before() {
    let (_g, dir) = fresh_profile("p1-plain");
    let r = check(&launch(&dir));
    assert!(r.needs_onboarding && !r.needs_login, "fresh: the wizard");
    store_written_then_killed(&dir);
    std::fs::write(marker(&dir), "1").unwrap();
    let r = check(&launch(&dir));
    assert!(r.needs_login && !r.needs_onboarding, "finished: login");
}

// ======================================================================= P2

// P2: "After `auth.store` is written, a kill at any later step of
// first-device or paired setup leaves an account that opens with its
// password on the next launch. On that first login the marker is written. A
// notice says once …" — the kill.
#[test]
fn p2_a_setup_killed_after_the_store_opens_with_its_password_and_says_so_once() {
    let (_g, dir) = fresh_profile("p2-kill");
    store_written_then_killed(&dir);
    let app = launch(&dir);
    let r = check(&app);
    assert!(r.needs_login, "the login screen");
    assert!(r.setup_incomplete, "the notice is due");
    let persona = tauri::async_runtime::block_on(validate_password(state(&app), MAIN.into(), vec![]))
        .expect("the account opens with its password");
    assert_eq!(persona, "primary");
    assert!(marker(&dir).exists(), "the first login writes the marker");
    let again = check(&launch(&dir));
    assert!(again.needs_login);
    assert!(!again.setup_incomplete, "the notice is said once");
}

// P2b: "Once `auth.store` is written, a later failure (seeding, the marker,
// the safety phrase, the typing profile) does not return an error:
// `complete_onboarding` … succeed[s] and report[s] the setup as
// **incomplete** … A kill and an error end in the same state, one test
// each." — the error: the marker cannot be written (a directory stands where
// the file goes), the last step of the setup.
#[test]
fn p2b_an_error_after_the_store_is_an_incomplete_setup_not_a_failure() {
    let (_g, dir) = fresh_profile("p2b-error");
    std::fs::create_dir_all(marker(&dir).join("blocked")).unwrap(); // the marker's write fails
    let app = launch(&dir);
    let out = tauri::async_runtime::block_on(complete_onboarding(
        state(&app),
        data(MAIN, Some(DURESS), Some(PHRASE)),
    ))
    .expect("not an error: the account exists");
    assert!(out.incomplete, "reported as incomplete");
    assert!(auth_store_path(&dir).exists());
    // The same state as after a kill: the next launch shows the login screen
    // and the notice, and the password opens the account.
    std::fs::remove_dir_all(marker(&dir)).unwrap(); // the obstacle is gone at the next launch
    let next = launch(&dir);
    let r = check(&next);
    assert!(r.needs_login && !r.needs_onboarding, "the login screen");
    assert!(r.setup_incomplete, "the notice is due");
    tauri::async_runtime::block_on(validate_password(state(&next), MAIN.into(), vec![]))
        .expect("the account opens");
    assert!(marker(&dir).is_file(), "the first login writes the marker");
    assert!(!check(&launch(&dir)).setup_incomplete, "said once");
}

// P2b's other side (expected to need only the new return type): a setup that
// finishes reports complete, and the next launch has no notice.
#[test]
fn p2b_a_setup_that_finishes_is_complete_and_has_no_notice() {
    let (_g, dir) = fresh_profile("p2b-ok");
    let app = launch(&dir);
    let out = tauri::async_runtime::block_on(complete_onboarding(
        state(&app),
        data(MAIN, Some(DURESS), Some(PHRASE)),
    ))
    .expect("setup succeeds");
    assert!(!out.incomplete);
    assert!(marker(&dir).exists());
    let r = check(&launch(&dir));
    assert!(r.needs_login && !r.setup_incomplete);
}

// P2b in one process (PANDA2's second position, claude-coord 0137): the
// session is installed, a later step fails, the wizard goes to the login
// screen, and the login runs `install_session` a second time in the **same**
// `AppState` (the key databases and the blind index are opened again by
// `build_encrypted_db` while the first session's are still held).
//
// The failing step is the safety phrase: a directory stands where
// `crypto/canary.store` is written.
fn incomplete_setup_with_session(tag: &str) -> (std::sync::MutexGuard<'static, ()>, PathBuf, AppState) {
    let (g, dir) = fresh_profile(tag);
    std::fs::create_dir_all(dir.join("crypto").join("canary.store").join("blocked")).unwrap();
    let app = launch(&dir);
    let out = tauri::async_runtime::block_on(complete_onboarding(
        state(&app),
        data(MAIN, Some(DURESS), Some(PHRASE)),
    ))
    .expect("not an error: the account exists");
    assert!(out.incomplete, "the safety phrase failed: incomplete");
    assert!(auth_store_path(&dir).exists());
    assert!(!marker(&dir).exists(), "precondition: no marker after an incomplete setup");
    let installed = tauri::async_runtime::block_on(async { app.docs.read().await.is_some() });
    assert!(installed, "precondition: the setup installed the session before the failing step");
    (g, dir, app)
}

/// The document service the session installed can read its store.
fn assert_session_usable(app: &AppState, who: &str) {
    tauri::async_runtime::block_on(async {
        use sovereign_db::GraphDB;
        let docs = app.docs.read().await.clone().unwrap_or_else(|| panic!("{who}: no document service after the login"));
        docs.store().list_documents(None).await.unwrap_or_else(|e| panic!("{who}: the store cannot be read: {e}"));
    });
}

#[test]
fn p2b_an_incomplete_setup_then_a_login_in_the_same_process_opens_the_primary_persona() {
    let (_g, dir, app) = incomplete_setup_with_session("p2b-same-primary");
    let persona = tauri::async_runtime::block_on(validate_password(state(&app), MAIN.into(), vec![]))
        .expect("the login after an incomplete setup succeeds in the same process");
    assert_eq!(persona, "primary");
    assert!(marker(&dir).is_file(), "the login writes the marker");
    let r = check(&app);
    assert!(r.needs_login && !r.setup_incomplete, "the notice is said once");
    assert_session_usable(&app, "primary");
}

#[test]
fn p2b_an_incomplete_setup_then_a_duress_login_in_the_same_process_opens_the_duress_persona() {
    let (_g, dir, app) = incomplete_setup_with_session("p2b-same-duress");
    let persona = tauri::async_runtime::block_on(validate_password(state(&app), DURESS.into(), vec![]))
        .expect("the duress login after an incomplete setup succeeds in the same process");
    assert_eq!(persona, "duress");
    assert!(marker(&dir).is_file(), "either persona's login writes the marker");
    assert!(!check(&app).setup_incomplete, "the notice is said once");
    assert_session_usable(&app, "duress");
}

// ======================================================================= P3

/// P3's refusal: an error in plain words about the confirmation, nothing
/// written, no claim left behind.
fn assert_refused_before_anything<T>(dir: &Path, r: Result<T, String>, case: &str) {
    let err = match r {
        Ok(_) => panic!("{case}: accepted"),
        Err(e) => e,
    };
    let low = err.to_lowercase();
    // CONTRACT-QUESTION: P3 asks for "a plain message" without its words.
    // Read: it says the two entries do not match (or that the confirmation
    // is missing).
    assert!(
        low.contains("match") || low.contains("confirm"),
        "{case}: the message is about the confirmation: {err}"
    );
    assert!(!low.contains("already"), "{case}: refused before the claim: {err}");
    assert!(files_under(dir).is_empty(), "{case}: nothing written: {:?}", files_under(dir));
    assert!(!auth_store_path(dir).exists(), "{case}: no auth.store");
    // No claim left: a new claim on the same profile is granted.
    drop(crate::onboarding_claim::claim(dir).unwrap_or_else(|e| panic!("{case}: a claim was left behind: {e}")));
}

// P3: "`complete_onboarding` refuses, before the claim and before any key
// derivation or write, with a plain message, when a given password, duress
// password or safety phrase has a confirmation that is absent or differs."
#[test]
fn p3_a_confirmation_absent_or_different_is_refused_before_anything_is_written() {
    type Edit = fn(&mut OnboardingData);
    let cases: Vec<(&str, Edit)> = vec![
        ("password confirmation absent", |d| d.password_confirm = None),
        ("password confirmation differs", |d| d.password_confirm = Some("Main!Pass1235".into())),
        ("duress confirmation absent", |d| d.duress_confirm = None),
        ("duress confirmation differs", |d| d.duress_confirm = Some("Duress!Pass5679".into())),
        ("phrase confirmation absent", |d| d.canary_confirm = None),
        ("phrase confirmation differs", |d| d.canary_confirm = Some("the heron waits at dusk".into())),
        // "The comparison is on the values as sent (the wizard trims them)":
        // the backend does not trim.
        ("password confirmation differs by a space", |d| d.password_confirm = Some(format!("{MAIN} "))),
    ];
    for (case, edit) in cases {
        let (_g, dir) = fresh_profile("p3");
        let app = launch(&dir);
        let mut d = data(MAIN, Some(DURESS), Some(PHRASE));
        edit(&mut d);
        let r = tauri::async_runtime::block_on(complete_onboarding(state(&app), d));
        assert_refused_before_anything(&dir, r, case);
    }
}

// P3 "before the claim": while another setup holds the claim, a mismatched
// confirmation is still refused for the mismatch (not "Setup is already in
// progress").
#[test]
fn p3_the_confirmation_is_checked_before_the_claim() {
    let (_g, dir) = fresh_profile("p3-claim");
    let held = crate::onboarding_claim::claim(&dir).expect("another setup in progress");
    let app = launch(&dir);
    let mut d = data(MAIN, Some(DURESS), None);
    d.duress_confirm = Some("not the same".into());
    let err = tauri::async_runtime::block_on(complete_onboarding(state(&app), d)).err().expect("refused");
    let low = err.to_lowercase();
    assert!(low.contains("match") || low.contains("confirm"), "the mismatch is reported first: {err}");
    drop(held);
}

// P3: an optional entry the user left empty needs no confirmation (no duress
// password, no phrase): accepted. Also the positive control of the P3 cases.
#[test]
fn p3_absent_optional_entries_need_no_confirmation() {
    let (_g, dir) = fresh_profile("p3-optional");
    let app = launch(&dir);
    let out = tauri::async_runtime::block_on(complete_onboarding(state(&app), data(MAIN, None, None)))
        .expect("accepted");
    assert!(!out.incomplete);
    assert!(auth_store_path(&dir).exists());
}

// ======================================================================= P4

// P4: "`complete_onboarding` reports it before deriving anything, with the
// desktop's words ('Duress password must differ from primary password')."
//
// CONTRACT-QUESTION: the desktop's words today are "Duress password must
// differ from your password" (sovereign-shell, onboarding.rs:186 and
// app.rs:6469); the contract quotes "… from primary password". Asserted: the
// common prefix, "Duress password must differ from".
#[test]
fn p4_complete_onboarding_refuses_a_duress_password_equal_to_the_main_one() {
    let (_g, dir) = fresh_profile("p4");
    let app = launch(&dir);
    let err = tauri::async_runtime::block_on(complete_onboarding(
        state(&app),
        data(MAIN, Some(MAIN), None),
    ))
    .err()
    .expect("refused");
    assert!(err.contains("Duress password must differ from"), "the desktop's words: {err}");
    assert!(files_under(&dir).is_empty(), "nothing written: {:?}", files_under(&dir));
    drop(crate::onboarding_claim::claim(&dir).expect("no claim left behind"));
}

// ======================================================================= paired

/// The paired input, every confirmation matching; the offer is not a real
/// one (no source device in a test): anything that reaches the handshake
/// fails on it.
#[cfg(feature = "p2p")]
fn paired_input() -> crate::tauri_commands::pairing::CompletePairedOnboardingInput {
    crate::tauri_commands::pairing::CompletePairedOnboardingInput {
        qr_payload_b64: "not-a-real-offer".into(),
        pin: "000000".into(),
        password: MAIN.into(),
        password_confirm: Some(MAIN.into()),
        duress_password: Some(DURESS.into()),
        duress_confirm: Some(DURESS.into()),
        nickname: None,
        bubble_style: None,
        canary_phrase: Some(PHRASE.into()),
        canary_confirm: Some(PHRASE.into()),
        seed_sample_data: false,
    }
}

// P3 on the paired input: "`OnboardingData` (and the paired input) carries
// the confirmations". The refusal comes before the claim and the handshake,
// so it is reachable without a source device: the error is the mismatch,
// not the (bad) offer.
#[cfg(feature = "p2p")]
#[test]
fn p3_paired_setup_refuses_a_mismatched_confirmation_before_the_handshake() {
    type Edit = fn(&mut crate::tauri_commands::pairing::CompletePairedOnboardingInput);
    let cases: Vec<(&str, Edit)> = vec![
        ("password confirmation absent", |d| d.password_confirm = None),
        ("password confirmation differs", |d| d.password_confirm = Some("x".into())),
        ("duress confirmation differs", |d| d.duress_confirm = Some("x".into())),
        ("phrase confirmation absent", |d| d.canary_confirm = None),
    ];
    for (case, edit) in cases {
        let (_g, dir) = fresh_profile("p3-paired");
        let app = launch(&dir);
        let mut input = paired_input();
        edit(&mut input);
        let r = tauri::async_runtime::block_on(crate::tauri_commands::pairing::complete_onboarding_paired(
            state(&app),
            input,
        ));
        assert_refused_before_anything(&dir, r, case);
    }
}

// P2c by reading the source (the handshake needs a live source device):
// "the record of the source device (`paired_devices.json`, step 7 …) is
// written **before** `auth.store`, and a failure to write it fails the setup
// (nothing else written yet)." In `complete_onboarding_paired`: the
// `PairingManager` record is saved before the first `.save_new(`, and that
// save's error is returned (`?` or `return Err`), not only logged.
#[test]
fn p2c_the_source_record_is_written_before_auth_store_and_its_failure_fails_the_setup() {
    use crate::onboarding_order_tests::{blank_comments, fn_body, read};
    let src = blank_comments(&read("src/tauri_commands/pairing.rs"));
    let (_, body) = fn_body(&src, "complete_onboarding_paired", "pairing.rs");
    let store = body.find(".save_new(").expect("auth.store is written with .save_new(");
    let record = body.find("PairingManager::").expect("the source record is built with PairingManager");
    assert!(record < store, "the source record comes before auth.store");
    let save = body[record..].find(".save(").map(|i| i + record).expect("the record is saved");
    assert!(save < store, "and is saved before auth.store");
    let after_save = &body[save..store];
    let stmt_end = after_save.find(';').unwrap_or(after_save.len());
    let stmt = &after_save[..stmt_end];
    let propagated = stmt.contains('?') || after_save[..after_save.len().min(stmt_end + 200)].contains("return Err");
    assert!(propagated, "a failure to save the record fails the setup: `{}`", stmt.trim());
    assert!(
        !after_save[..after_save.len().min(stmt_end + 200)].contains("tracing::warn!(\"Failed to persist paired_devices"),
        "not only logged"
    );
}

// P2b on the paired command, by its signature (the command cannot run without
// a source device): it reports an outcome, as `complete_onboarding` does.
#[test]
fn p2b_paired_setup_reports_an_outcome_too() {
    use crate::onboarding_order_tests::{blank_comments, read};
    let src = blank_comments(&read("src/tauri_commands/pairing.rs"));
    let at = src.find("fn complete_onboarding_paired(").expect("the command");
    let sig = &src[at..at + src[at..].find('{').unwrap()];
    assert!(sig.contains("OnboardingOutcome"), "it returns the outcome: {sig}");
}
