//! v0.0.12 item 18 in the native shell — the locked screens' key derivations
//! run off the UI thread (B1–B4, and B5b's fake half), driven with a fake
//! derivation that waits.
//!
//! Contract: `doc/plans/V0012-items18-20-first-run.md`. Written before the
//! code, by another author: whoever implements item 18 makes these pass
//! **without editing this file**; a test that looks wrong is reported, not
//! worked around. A child of `app`, like `first_run_tests.rs`, whose helpers
//! (child launches, frames, the tree as read) it uses.
//!
//! # The seams this file needs (it does not compile until they exist)
//!
//! 1. **The worker's work, behind a trait** — a new module
//!    `crate::auth_work` (`src/auth_work.rs`, declared in `main.rs`):
//!
//!    ```ignore
//!    /// What `install_session` returns.
//!    pub(crate) type Session = (
//!        sovereign_core::auth::PersonaKind,
//!        Arc<sovereign_db::encrypted::EncryptedGraphDB>,
//!        Arc<sovereign_crypto::account_key::AccountKey>,
//!        Arc<sovereign_crypto::device_key::DeviceKey>,
//!        Arc<sovereign_crypto::kek::Kek>,
//!    );
//!    /// The slow, key-deriving steps of the locked screens. `App` calls them
//!    /// **only on a worker thread**, never on the UI thread; each may take
//!    /// seconds.
//!    pub(crate) trait AuthWork: Send + Sync {
//!        /// First-device setup's store: today's `crypto::create_auth_store`.
//!        fn create_store(&self, primary: &[u8], duress: &[u8]) -> anyhow::Result<AuthStore>;
//!        /// The session over a store just created: today's `install_session`.
//!        fn install(&self, store: &AuthStore, password: &[u8]) -> anyhow::Result<Session>;
//!        /// Unlock (the login form, and the login that ends a join): load
//!        /// `auth.store`, then `install_session` — today's `try_login` up to
//!        /// its result.
//!        fn unlock(&self, password: &[u8]) -> anyhow::Result<Session>;
//!        /// Guardian recovery's new password: today's `recover_and_install`.
//!        fn recover(&self, new_password: &[u8]) -> anyhow::Result<Session>;
//!    }
//!    ```
//!
//!    and a field `pub(crate) auth_work: Arc<dyn crate::auth_work::AuthWork>`
//!    on `App`, set by `App::new()` to the real implementation. A test
//!    replaces it before submitting. The results come back to the UI thread
//!    and are applied while drawing frames (`build_scene`), as `pair_rx` is.
//!    The real calls live in that module, which is why
//!    `first_run_tests::b1_the_ui_paths_call_no_derivation_themselves` finds
//!    none in `App`'s UI-path methods.
//!
//! 2. **The locked screen's keyboard as a method**:
//!    `App::locked_key(&mut self, key: &winit::keyboard::Key, text: Option<&str>)`
//!    — the `if self.locked { … }` branch of the keyboard in `window_event`
//!    (wizard and auth form), moved into a method that `window_event` calls
//!    with the same `logical_key` and `text`. So "Enter does nothing" and
//!    "the fields cannot be edited" (B2) are tested on the real routing.
//!
//! Everything else is today's API: `handle_wiz_action(WizAction::Finish)`
//! (the last step's button), `wiz_enter`, `auth_submit` (Enter on the auth
//! form), `recovery_finalize`, `pair_tx` (the join handshake's result),
//! `build_a11y_tree`, `build_scene`.
//!
//! # How a test runs
//!
//! Each scenario is a launch in a child process on a fresh profile (see
//! `first_run_tests.rs`), because setup writes the profile and a success
//! starts P2P. The fake records each call and the thread it ran on, then
//! waits on a gate the test opens; a call still waiting after 20 s gives up
//! (so a derivation run on the UI thread fails the test instead of hanging
//! it). A fake `create_store` that succeeds writes an `auth.store` file (an
//! empty one: nothing reads it but the router), as the real one does.
//!
//! **Readings chosen** (reported with the tests): the status line is an
//! AccessKit `Status` node whose name is exactly the contract's words; the
//! error after a failure is an `Alert` node whose name contains the error;
//! the login that ends a join says "Unlocking…" (it is an unlock); a login
//! failure is not required to keep the password typed (today it clears it,
//! and the contract's "what was typed is kept, as today" reads as the
//! wizard's fields).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::ThreadId;
use std::time::Duration;

use accesskit::Role as AkRole;
use sovereign_core::auth::PersonaKind as CorePersona;
use sovereign_crypto::auth::AuthStore;
use winit::keyboard::{Key, NamedKey};

use super::first_run_tests::{
    fill_wizard, first_frame, frames_until, launch, login_settled, press_finish, profile, read_tree, setup_settled,
    shut_runtime, submit_login, CHILD_VAR, PRIMARY,
};
use super::App;
use crate::auth_work::{AuthWork, Session};
use crate::onboarding::STEP_CANARY;
use crate::panels::{RecoveryPhase, RecoveryWizard};

const CREATING: &str = "Creating your account\u{2026}";
const UNLOCKING: &str = "Unlocking\u{2026}";
const SETTING: &str = "Setting your new password\u{2026}";

/// How long a fake call waits for its gate before giving up.
const GATE_TIMEOUT: Duration = Duration::from_secs(20);

// ================================================================== the fake

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Create,
    Install,
    Unlock,
    Recover,
}

/// What a fake call returns once its gate opens.
#[derive(Clone, Copy, PartialEq)]
enum Out {
    Ok,
    Fail,
}

struct Call {
    kind: Kind,
    thread: ThreadId,
    password: String,
    finished: bool,
}

struct Fake {
    gate: Mutex<bool>,
    opened: Condvar,
    calls: Mutex<Vec<Call>>,
    timed_out: AtomicBool,
    create: Out,
    install: Out,
    unlock: Out,
    recover: Out,
    session: Session,
}

/// The message every failing fake call returns.
const FAKE_ERROR: &str = "simulated derivation failure";

impl Fake {
    fn new(app: &App, create: Out, install: Out, unlock: Out, recover: Out) -> Arc<Fake> {
        let db = app.rt.block_on(sovereign_ai::docs_host::testing::encrypted_memory_store("first-run-worker"));
        let mk = sovereign_crypto::master_key::MasterKey::generate();
        let session: Session = (
            CorePersona::Primary,
            db,
            sovereign_ai::docs_host::testing::account_key(),
            Arc::new(sovereign_crypto::device_key::DeviceKey::derive(&mk, "first-run-device").unwrap()),
            Arc::new(sovereign_crypto::kek::Kek::generate()),
        );
        Arc::new(Fake {
            gate: Mutex::new(false),
            opened: Condvar::new(),
            calls: Mutex::new(Vec::new()),
            timed_out: AtomicBool::new(false),
            create,
            install,
            unlock,
            recover,
            session,
        })
    }

    fn open_gate(&self) {
        *self.gate.lock().unwrap() = true;
        self.opened.notify_all();
    }

    /// Records the call, waits for the gate, marks it finished.
    fn run(&self, kind: Kind, password: &[u8]) {
        let idx = {
            let mut c = self.calls.lock().unwrap();
            c.push(Call {
                kind,
                thread: std::thread::current().id(),
                password: String::from_utf8_lossy(password).into_owned(),
                finished: false,
            });
            c.len() - 1
        };
        let g = self.gate.lock().unwrap();
        let (g, t) = self.opened.wait_timeout_while(g, GATE_TIMEOUT, |open| !*open).unwrap();
        if t.timed_out() && !*g {
            self.timed_out.store(true, Ordering::SeqCst);
        }
        drop(g);
        self.calls.lock().unwrap()[idx].finished = true;
    }

    fn count(&self, kind: Kind) -> usize {
        self.calls.lock().unwrap().iter().filter(|c| c.kind == kind).count()
    }
    fn total(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
    fn threads(&self) -> Vec<(Kind, ThreadId)> {
        self.calls.lock().unwrap().iter().map(|c| (c.kind, c.thread)).collect()
    }
    fn passwords(&self, kind: Kind) -> Vec<String> {
        self.calls.lock().unwrap().iter().filter(|c| c.kind == kind).map(|c| c.password.clone()).collect()
    }
    fn any_finished(&self) -> bool {
        self.calls.lock().unwrap().iter().any(|c| c.finished)
    }
    fn result(&self, out: Out) -> anyhow::Result<Session> {
        match out {
            Out::Ok => Ok(self.session.clone()),
            Out::Fail => Err(anyhow::anyhow!(FAKE_ERROR)),
        }
    }
}

/// A store with no persona: written so the profile routes to the login form,
/// never opened.
fn empty_store() -> AuthStore {
    AuthStore {
        salt: vec![7; 32],
        device_id: "first-run-device".into(),
        kdf: sovereign_crypto::master_key::Kdf::current(),
        personas: Vec::new(),
    }
}

fn write_empty_store() {
    std::fs::create_dir_all(crate::crypto::crypto_dir()).unwrap();
    empty_store().save_new(&crate::crypto::auth_store_path()).expect("write auth.store");
}

impl AuthWork for Fake {
    fn create_store(&self, primary: &[u8], _duress: &[u8]) -> anyhow::Result<AuthStore> {
        self.run(Kind::Create, primary);
        match self.create {
            Out::Ok => {
                write_empty_store();
                Ok(empty_store())
            }
            Out::Fail => Err(anyhow::anyhow!(FAKE_ERROR)),
        }
    }
    fn install(&self, _store: &AuthStore, password: &[u8]) -> anyhow::Result<Session> {
        self.run(Kind::Install, password);
        self.result(self.install)
    }
    fn unlock(&self, password: &[u8]) -> anyhow::Result<Session> {
        self.run(Kind::Unlock, password);
        self.result(self.unlock)
    }
    fn recover(&self, new_password: &[u8]) -> anyhow::Result<Session> {
        self.run(Kind::Recover, new_password);
        self.result(self.recover)
    }
}

fn install_fake(app: &mut App, fake: &Arc<Fake>) {
    app.auth_work = fake.clone();
}

// ============================================================== the checks

fn statuses(app: &mut App) -> Vec<String> {
    read_tree(app).names_with_role(AkRole::Status)
}

fn alerts(app: &mut App) -> Vec<String> {
    read_tree(app).names_with_role(AkRole::Alert)
}

/// Waits (drawing frames) until the fake has received `n` calls.
fn wait_calls(app: &mut App, fake: &Fake, n: usize) {
    assert!(
        frames_until(app, 10, |_| fake.total() >= n),
        "the derivation did not start: {} call(s) after 10 s of frames",
        fake.total()
    );
}

/// B1: every call ran on a thread other than this one (the UI thread), and
/// none was still waiting when the test opened its gate.
fn assert_off_ui_thread(fake: &Fake) {
    let ui = std::thread::current().id();
    for (kind, t) in fake.threads() {
        assert_ne!(t, ui, "{kind:?} ran on the UI thread (B1)");
    }
    assert!(!fake.timed_out.load(Ordering::SeqCst), "a derivation waited out its gate: it blocked the UI thread (B1)");
}

/// B1: while the derivation waits, frames keep being laid out — `build_scene`
/// returns, again and again, and nothing has finished.
fn assert_frames_while_waiting(app: &mut App, fake: &Fake) {
    for _ in 0..5 {
        app.build_scene(super::first_run_tests::W, super::first_run_tests::H);
    }
    assert!(!fake.any_finished(), "the derivation finished before its gate opened");
    assert!(app.locked, "still locked while it runs");
}

fn type_char(app: &mut App, c: &str) {
    app.locked_key(&Key::Character(c.into()), Some(c));
}

fn press(app: &mut App, k: NamedKey) {
    app.locked_key(&Key::Named(k), None);
}

// ================================================================ scenarios

fn module() -> &'static str {
    module_path!()
}

#[test]
#[ignore = "a launch, run by its parent test in a child process on a profile of its own"]
fn child_entry() {
    let Ok(scenario) = std::env::var(CHILD_VAR) else { return };
    match scenario.as_str() {
        "setup_runs_off_the_ui_thread" => setup_runs_off_the_ui_thread(),
        "setup_is_inert_while_it_runs" => setup_is_inert_while_it_runs(),
        "setup_success" => setup_success(),
        "setup_failure_before_store" => setup_failure_before_store(),
        "setup_failure_after_store" => setup_failure_after_store(),
        "login_runs_off_the_ui_thread" => login_runs_off_the_ui_thread(),
        "login_is_inert_while_it_runs" => login_is_inert_while_it_runs(),
        "login_failure" => login_failure(),
        "join_login_runs_off_the_ui_thread" => join_login_runs_off_the_ui_thread(),
        "recovery_runs_off_the_ui_thread" => recovery_runs_off_the_ui_thread(),
        "recovery_failure" => recovery_failure(),
        other => panic!("unknown scenario {other}"),
    }
}

/// A fresh profile, the wizard filled to its last step, the fake installed.
fn setup_app(create: Out, install: Out) -> (App, Arc<Fake>) {
    let mut app = App::new();
    first_frame(&mut app);
    fill_wizard(&mut app, PRIMARY, "Duress!Pass5678");
    app.wizard.as_mut().unwrap().canary = "blue heron".into();
    app.wizard.as_mut().unwrap().canary_confirm = "blue heron".into();
    let fake = Fake::new(&app, create, install, Out::Ok, Out::Ok);
    install_fake(&mut app, &fake);
    (app, fake)
}

/// A profile with auth.store (the login form), the fake installed.
fn login_app(unlock: Out) -> (App, Arc<Fake>) {
    write_empty_store();
    let mut app = App::new();
    first_frame(&mut app);
    let fake = Fake::new(&app, Out::Ok, Out::Ok, unlock, Out::Ok);
    install_fake(&mut app, &fake);
    (app, fake)
}

// ---- setup

fn setup_runs_off_the_ui_thread() {
    let (mut app, fake) = setup_app(Out::Ok, Out::Ok);
    press_finish(&mut app);
    wait_calls(&mut app, &fake, 1);
    assert_frames_while_waiting(&mut app, &fake);
    assert_eq!(fake.count(Kind::Create), 1, "one store creation");
    assert_eq!(fake.passwords(Kind::Create), vec![PRIMARY.to_string()], "with the typed password");
    let s = statuses(&mut app);
    assert!(s.iter().any(|n| n == CREATING), "the status line {CREATING:?} is in the tree as a status (B2): {s:?}");
    let t = read_tree(&mut app);
    assert!(app.wizard.is_some(), "the wizard stays on screen while it runs (B2)");
    assert!(t.has_non_field_named("Canary phrase"), "the tree still describes the wizard's step (A1, B2)\n{}", t.dump());
    fake.open_gate();
    assert!(frames_until(&mut app, 20, setup_settled), "the setup ends once the derivation does");
    assert_off_ui_thread(&fake);
    assert_eq!(fake.count(Kind::Install), 1, "the session is installed once");
    assert!(!app.locked, "setup unlocks (B4)");
    shut_runtime(&mut app);
    std::mem::forget(app);
}

fn setup_is_inert_while_it_runs() {
    let (mut app, fake) = setup_app(Out::Ok, Out::Ok);
    press_finish(&mut app);
    wait_calls(&mut app, &fake, 1);
    let before = {
        let w = app.wizard.as_ref().expect("the wizard stays on screen while it runs");
        (w.step, w.focus, w.canary.clone(), w.canary_confirm.clone(), w.password.clone())
    };
    assert_eq!(before.0, STEP_CANARY, "still on the last step");
    // B3: the button again (its action, as `handle_click` dispatches it) and
    // Enter (through the keyboard's routing): nothing starts.
    press_finish(&mut app);
    press(&mut app, NamedKey::Enter);
    // B2: the fields cannot be edited.
    type_char(&mut app, "x");
    press(&mut app, NamedKey::Backspace);
    press(&mut app, NamedKey::Tab);
    let after = {
        let w = app.wizard.as_ref().expect("the wizard stays on screen while it runs");
        (w.step, w.focus, w.canary.clone(), w.canary_confirm.clone(), w.password.clone())
    };
    assert_eq!(after, before, "while it runs, the fields cannot be edited (B2)");
    assert_eq!(fake.count(Kind::Create), 1, "a second submit starts nothing: no second store write (B3)");
    fake.open_gate();
    assert!(frames_until(&mut app, 20, setup_settled), "the setup ends");
    frames_until(&mut app, 1, |_| false);
    assert_eq!(fake.count(Kind::Create), 1, "no second store write afterwards either (B3)");
    assert_eq!(fake.count(Kind::Install), 1, "no second derivation (B3)");
    assert_off_ui_thread(&fake);
    shut_runtime(&mut app);
    std::mem::forget(app);
}

fn setup_success() {
    let (mut app, fake) = setup_app(Out::Ok, Out::Ok);
    press_finish(&mut app);
    wait_calls(&mut app, &fake, 1);
    fake.open_gate();
    assert!(frames_until(&mut app, 20, setup_settled), "the setup ends");
    // Today's outcome.
    assert!(!app.locked, "unlocked (B4)");
    assert!(app.wizard.is_none(), "the wizard is gone (B4)");
    assert_eq!(app.persona, Some(CorePersona::Primary), "the session's persona (B4)");
    assert!(app.db.is_some() && app.docs.is_some(), "the store and the document service are installed (B1, B4)");
    assert!(app.kek.is_some() && app.account_key.is_some() && app.device_key.is_some(), "the session's keys (B4)");
    let profile = sovereign_core::profile::UserProfile::load(&sovereign_core::sovereign_dir()).expect("the profile is saved");
    assert_eq!(profile.nickname.as_deref(), Some("Ada"), "the profile is saved, as today (B4)");
    assert!(
        sovereign_core::sovereign_dir().join("crypto").join("canary.store").exists(),
        "the safety phrase is saved, as today (B4)"
    );
    assert_off_ui_thread(&fake);
    shut_runtime(&mut app);
    std::mem::forget(app);
}

fn setup_failure_before_store() {
    let (mut app, fake) = setup_app(Out::Fail, Out::Ok);
    press_finish(&mut app);
    wait_calls(&mut app, &fake, 1);
    fake.open_gate();
    assert!(frames_until(&mut app, 20, setup_settled), "the setup ends");
    assert!(app.locked, "a failed setup leaves the screen locked (B4)");
    assert_eq!(fake.count(Kind::Install), 0, "no installation without a store");
    let w = app.wizard.as_ref().expect("before the store, the wizard stays (B4)");
    assert!(w.error.as_deref().is_some_and(|e| e.contains(FAKE_ERROR)), "the error is on screen (B4): {:?}", w.error);
    assert_eq!((w.password.as_str(), w.password_confirm.as_str()), (PRIMARY, PRIMARY), "what was typed is kept (B4)");
    assert_eq!(w.canary, "blue heron", "what was typed is kept (B4)");
    let a = alerts(&mut app);
    assert!(a.iter().any(|n| n.contains(FAKE_ERROR)), "the error is in the tree as an alert (B4): {a:?}");
    let s = statuses(&mut app);
    assert!(!s.iter().any(|n| n == CREATING), "the status line is gone once it ended: {s:?}");
    // The form works again: typing reaches a field, and the button starts a
    // new attempt. (The test puts the wizard on its last step's first field
    // to type there, wherever the failure left it.)
    {
        let w = app.wizard.as_mut().unwrap();
        w.step = STEP_CANARY;
        w.focus = 0;
    }
    type_char(&mut app, "!");
    assert_eq!(app.wizard.as_ref().unwrap().canary, "blue heron!", "the fields work again (B4)");
    {
        let w = app.wizard.as_mut().unwrap();
        w.canary = "blue heron".into();
        w.error = None;
    }
    press_finish(&mut app);
    assert!(frames_until(&mut app, 10, |_| fake.count(Kind::Create) == 2), "the button works again: a new attempt starts (B4)");
    assert!(frames_until(&mut app, 20, setup_settled), "the second attempt ends");
    assert_off_ui_thread(&fake);
    shut_runtime(&mut app);
    std::mem::forget(app);
}

fn setup_failure_after_store() {
    let (mut app, fake) = setup_app(Out::Ok, Out::Fail);
    press_finish(&mut app);
    wait_calls(&mut app, &fake, 1);
    fake.open_gate();
    assert!(frames_until(&mut app, 20, setup_settled), "the setup ends");
    assert!(crate::crypto::auth_store_exists(), "precondition: the store was written");
    assert!(app.locked, "a failed installation leaves the screen locked (B4)");
    assert!(app.wizard.is_none(), "once the store is written, a later failure drops the wizard (B5b)");
    assert!(!app.auth_form.joining && app.auth_form.labels == vec!["Password"], "the login form is shown (B5b)");
    assert!(
        app.auth_form.error.as_deref().is_some_and(|e| e.contains(FAKE_ERROR)),
        "with the error (B5b): {:?}",
        app.auth_form.error
    );
    let a = alerts(&mut app);
    assert!(a.iter().any(|n| n.contains(FAKE_ERROR)), "the error is in the tree as an alert (B4): {a:?}");
    frames_until(&mut app, 1, |_| false);
    assert!(app.wizard.is_none(), "the wizard does not come back (B5b)");
    assert_eq!(fake.count(Kind::Create), 1, "no second store write (B5b)");
    assert_off_ui_thread(&fake);
    shut_runtime(&mut app);
    std::mem::forget(app);
}

// ---- login

fn login_runs_off_the_ui_thread() {
    let (mut app, fake) = login_app(Out::Ok);
    submit_login(&mut app, PRIMARY);
    wait_calls(&mut app, &fake, 1);
    assert_frames_while_waiting(&mut app, &fake);
    assert_eq!(fake.passwords(Kind::Unlock), vec![PRIMARY.to_string()], "the unlock gets the typed password");
    let s = statuses(&mut app);
    assert!(s.iter().any(|n| n == UNLOCKING), "the status line {UNLOCKING:?} is in the tree as a status (B2): {s:?}");
    fake.open_gate();
    assert!(frames_until(&mut app, 20, login_settled), "the login ends");
    assert_off_ui_thread(&fake);
    assert!(!app.locked, "unlocked (B4)");
    assert_eq!(app.persona, Some(CorePersona::Primary));
    assert!(app.db.is_some() && app.docs.is_some(), "the store and the document service are installed (B1, B4)");
    shut_runtime(&mut app);
    std::mem::forget(app);
}

fn login_is_inert_while_it_runs() {
    let (mut app, fake) = login_app(Out::Ok);
    submit_login(&mut app, PRIMARY);
    wait_calls(&mut app, &fake, 1);
    let before = (app.auth_form.fields.clone(), app.auth_form.focus);
    app.auth_submit();
    press(&mut app, NamedKey::Enter);
    type_char(&mut app, "x");
    press(&mut app, NamedKey::Backspace);
    assert_eq!((app.auth_form.fields.clone(), app.auth_form.focus), before, "while it runs, the field cannot be edited (B2)");
    assert_eq!(fake.count(Kind::Unlock), 1, "a second submit starts nothing (B3)");
    fake.open_gate();
    assert!(frames_until(&mut app, 20, login_settled), "the login ends");
    frames_until(&mut app, 1, |_| false);
    assert_eq!(fake.count(Kind::Unlock), 1, "no second derivation afterwards either (B3)");
    assert_off_ui_thread(&fake);
    shut_runtime(&mut app);
    std::mem::forget(app);
}

fn login_failure() {
    let (mut app, fake) = login_app(Out::Fail);
    submit_login(&mut app, PRIMARY);
    wait_calls(&mut app, &fake, 1);
    fake.open_gate();
    assert!(frames_until(&mut app, 20, login_settled), "the login ends");
    assert!(app.locked, "a wrong password leaves the screen locked (B4, fail-closed)");
    assert!(app.db.is_none() && app.docs.is_none(), "nothing is installed (B4)");
    assert!(
        app.auth_form.error.as_deref().is_some_and(|e| e.contains(FAKE_ERROR)),
        "the error is on screen (B4): {:?}",
        app.auth_form.error
    );
    let a = alerts(&mut app);
    assert!(a.iter().any(|n| n.contains(FAKE_ERROR)), "the error is in the tree as an alert (B4): {a:?}");
    let s = statuses(&mut app);
    assert!(!s.iter().any(|n| n == UNLOCKING), "the status line is gone once it ended: {s:?}");
    // The form works again.
    type_char(&mut app, "z");
    assert!(app.auth_form.fields[0].ends_with('z'), "the field works again (B4): {:?}", app.auth_form.fields[0].len());
    app.auth_submit();
    assert!(frames_until(&mut app, 10, |_| fake.count(Kind::Unlock) == 2), "Enter works again: a new attempt starts (B4)");
    assert!(frames_until(&mut app, 20, login_settled), "the second attempt ends");
    assert_off_ui_thread(&fake);
    shut_runtime(&mut app);
    std::mem::forget(app);
}

// ---- the login that ends a join

fn join_login_runs_off_the_ui_thread() {
    let (mut app, fake) = login_app(Out::Ok);
    app.auth_form = crate::panels::AuthForm::join();
    app.pending_join_password = Some(PRIMARY.into());
    app.pair_tx.send(Ok(())).unwrap();
    wait_calls(&mut app, &fake, 1);
    assert_frames_while_waiting(&mut app, &fake);
    assert_eq!(fake.passwords(Kind::Unlock), vec![PRIMARY.to_string()], "the join ends in an unlock with the password set");
    let s = statuses(&mut app);
    assert!(s.iter().any(|n| n == UNLOCKING), "the status line {UNLOCKING:?} is in the tree as a status (B2): {s:?}");
    fake.open_gate();
    assert!(frames_until(&mut app, 20, login_settled), "the login ends");
    assert_off_ui_thread(&fake);
    assert!(!app.locked, "unlocked (B4)");
    shut_runtime(&mut app);
    std::mem::forget(app);
}

// ---- guardian recovery's new password

fn recovery_app(recover: Out) -> (App, Arc<Fake>) {
    write_empty_store();
    let mut app = App::new();
    first_frame(&mut app);
    let fake = Fake::new(&app, Out::Ok, Out::Ok, Out::Ok, recover);
    install_fake(&mut app, &fake);
    let mut w = RecoveryWizard::fresh();
    w.phase = RecoveryPhase::Ready;
    w.new_password = PRIMARY.into();
    app.recovery_wizard = Some(w);
    (app, fake)
}

fn recovery_runs_off_the_ui_thread() {
    let (mut app, fake) = recovery_app(Out::Ok);
    app.recovery_finalize();
    wait_calls(&mut app, &fake, 1);
    assert_frames_while_waiting(&mut app, &fake);
    assert_eq!(fake.passwords(Kind::Recover), vec![PRIMARY.to_string()], "the new password");
    let s = statuses(&mut app);
    assert!(s.iter().any(|n| n == SETTING), "the status line {SETTING:?} is in the tree as a status (B2): {s:?}");
    app.recovery_finalize();
    assert_eq!(fake.count(Kind::Recover), 1, "a second submit starts nothing (B3)");
    fake.open_gate();
    assert!(frames_until(&mut app, 20, |a| !a.locked || a.recovery_wizard.as_ref().is_some_and(|w| w.error.is_some())), "recovery ends");
    assert_off_ui_thread(&fake);
    assert!(!app.locked, "recovery unlocks (B4)");
    assert!(app.recovery_wizard.is_none(), "the recovery wizard closes (B4)");
    assert_eq!(fake.count(Kind::Recover), 1, "no second derivation (B3)");
    shut_runtime(&mut app);
    std::mem::forget(app);
}

fn recovery_failure() {
    let (mut app, fake) = recovery_app(Out::Fail);
    app.recovery_finalize();
    wait_calls(&mut app, &fake, 1);
    fake.open_gate();
    assert!(
        frames_until(&mut app, 20, |a| !a.locked || a.recovery_wizard.as_ref().is_some_and(|w| w.error.is_some())),
        "recovery ends"
    );
    assert!(app.locked, "a failed recovery leaves the screen locked (B4)");
    let w = app.recovery_wizard.as_ref().expect("the recovery wizard stays");
    assert!(w.error.as_deref().is_some_and(|e| e.contains(FAKE_ERROR)), "the error is on screen (B4): {:?}", w.error);
    assert_eq!(w.new_password, PRIMARY, "what was typed is kept (B4)");
    let a = alerts(&mut app);
    assert!(a.iter().any(|n| n.contains(FAKE_ERROR)), "the error is in the tree as an alert (B4): {a:?}");
    app.recovery_finalize();
    assert!(frames_until(&mut app, 10, |_| fake.count(Kind::Recover) == 2), "the button works again (B4)");
    assert_off_ui_thread(&fake);
    shut_runtime(&mut app);
    std::mem::forget(app);
}

// ================================================================== tests

#[test]
fn b1_setup_derivation_runs_off_the_ui_thread_with_its_status_line() {
    launch(module(), "setup_runs_off_the_ui_thread", profile().path());
}

// ---- for `first_run_tests.rs` (B5b amended, ff959de): a join's and a
// recovery's end on the profile's real `auth.store`, the derivation faked.
// What these exercise is `App`'s side (what it records when a join or a
// recovery completes); the next login, in another launch, is real.

/// The end of a join: the handshake's result arrives, the login it ends in
/// runs (faked), and the account opens.
pub(super) fn complete_a_join_faked(app: &mut App) {
    let fake = Fake::new(app, Out::Ok, Out::Ok, Out::Ok, Out::Ok);
    fake.open_gate();
    install_fake(app, &fake);
    app.auth_form = crate::panels::AuthForm::join();
    app.pending_join_password = Some(PRIMARY.into());
    app.pair_tx.send(Ok(())).unwrap();
    assert!(frames_until(app, 20, login_settled), "the join's login ends");
    assert!(!app.locked, "the joined account opens: {:?}", app.auth_form.error);
    assert_eq!(fake.count(Kind::Unlock), 1, "the join ended in one unlock");
}

/// Guardian recovery's last step (the new password), faked, and the account
/// opens.
pub(super) fn complete_a_recovery_faked(app: &mut App) {
    let fake = Fake::new(app, Out::Ok, Out::Ok, Out::Ok, Out::Ok);
    fake.open_gate();
    install_fake(app, &fake);
    let mut w = RecoveryWizard::fresh();
    w.phase = RecoveryPhase::Ready;
    w.new_password = PRIMARY.into();
    app.recovery_wizard = Some(w);
    app.recovery_finalize();
    assert!(
        frames_until(app, 20, |a| !a.locked || a.recovery_wizard.as_ref().is_some_and(|w| w.error.is_some())),
        "recovery ends"
    );
    assert!(!app.locked, "the recovered account opens");
    assert_eq!(fake.count(Kind::Recover), 1, "one recovery");
}

#[test]
fn b2_b3_setup_is_inert_while_it_runs_and_a_second_submit_starts_nothing() {
    launch(module(), "setup_is_inert_while_it_runs", profile().path());
}

#[test]
fn b4_setup_success_is_todays_outcome() {
    launch(module(), "setup_success", profile().path());
}

#[test]
fn b4_setup_failure_before_the_store_reenables_the_wizard_and_keeps_what_was_typed() {
    launch(module(), "setup_failure_before_store", profile().path());
}

#[test]
fn b5b_setup_failure_after_the_store_shows_the_login_form_with_the_error() {
    launch(module(), "setup_failure_after_store", profile().path());
}

#[test]
fn b1_unlock_runs_off_the_ui_thread_with_its_status_line() {
    launch(module(), "login_runs_off_the_ui_thread", profile().path());
}

#[test]
fn b2_b3_unlock_is_inert_while_it_runs_and_a_second_submit_starts_nothing() {
    launch(module(), "login_is_inert_while_it_runs", profile().path());
}

#[test]
fn b4_unlock_failure_stays_locked_shows_an_alert_and_reenables_the_form() {
    launch(module(), "login_failure", profile().path());
}

#[test]
fn b1_the_login_that_ends_a_join_runs_off_the_ui_thread() {
    launch(module(), "join_login_runs_off_the_ui_thread", profile().path());
}

#[test]
fn b1_b3_recovery_new_password_runs_off_the_ui_thread_once() {
    launch(module(), "recovery_runs_off_the_ui_thread", profile().path());
}

#[test]
fn b4_recovery_failure_keeps_the_wizard_with_an_alert() {
    launch(module(), "recovery_failure", profile().path());
}
