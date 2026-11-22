//! Contract C-LOG, LG5 in the native shell (spec ch. 08, « Checked at login, on every host,
//! before the assistant is used »), written before the code by its test author: whoever builds
//! C-LOG makes this pass without editing this file.
//!
//! Today the shell installs the log's key, and so checks the log, when it builds the assistant:
//! at the first chat message after login, or at the first reliability check of a web page (spec
//! ch. 08, *Session log*). LG5 moves that check to the login itself.
//!
//! A child of `app`, like `device_names_tests`, so it drives `App`'s own login on a real,
//! headless `App`. The log lives under `SOVEREIGN_DATA_DIR`, read by the whole process, so the
//! launch runs in a child process on a temporary profile of its own (the `launch` of
//! `first_run_tests`). The unlock is faked (`AuthWork::unlock` hands back a session over an
//! in-memory store and the test account key); the profile still gets a real `auth.store`, so the
//! shell shows its login form as after any setup.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use sovereign_ai::docs_host::testing;
use sovereign_ai::orchestrator::OrchestratorStatePaths;
use sovereign_core::auth::PersonaKind;

use super::first_run_tests::{
    first_frame, frames_until, launch, login_settled, profile, shut_runtime, submit_login, CHILD_VAR, DURESS, PRIMARY,
};
use super::App;
use crate::auth_work::{AuthWork, Session};

fn module() -> &'static str {
    module_path!()
}

/// A key that is not the session's: a log written under it does not decrypt under the
/// session's log key, whatever the format.
const STRANGER_KEY: [u8; 32] = [0x53; 32];

/// The login, faked: `unlock` opens the primary's session over an in-memory store, with the
/// test account key, whatever the password.
struct FakeUnlock {
    session: Session,
}

impl FakeUnlock {
    fn new(app: &App) -> Arc<FakeUnlock> {
        let db = app.rt.block_on(testing::encrypted_memory_store("clog-login"));
        let mk = sovereign_crypto::master_key::MasterKey::generate();
        Arc::new(FakeUnlock {
            session: (
                PersonaKind::Primary,
                db,
                testing::account_key(),
                Arc::new(sovereign_crypto::device_key::DeviceKey::derive(&mk, "clog-login").unwrap()),
                Arc::new(sovereign_crypto::kek::Kek::generate()),
            ),
        })
    }
}

impl AuthWork for FakeUnlock {
    fn create_store(&self, _primary: &[u8], _duress: &[u8]) -> anyhow::Result<sovereign_crypto::auth::AuthStore> {
        Err(anyhow::anyhow!("not used by these tests"))
    }
    fn install(&self, _store: &sovereign_crypto::auth::AuthStore, _password: &[u8]) -> anyhow::Result<Session> {
        Err(anyhow::anyhow!("not used by these tests"))
    }
    fn unlock(&self, _password: &[u8]) -> anyhow::Result<Session> {
        Ok(self.session.clone())
    }
    fn recover(&self, _new_password: &[u8]) -> anyhow::Result<Session> {
        Err(anyhow::anyhow!("not used by these tests"))
    }
}

fn quarantines(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.is_dir()
                        && p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("quarantine-"))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// One launch: a primary's log written under another key, then a login and nothing else: no
/// chat message, no web page. The log is checked at the login, so it is quarantined without
/// the assistant being used.
fn checked_at_login() {
    let base = sovereign_core::sovereign_dir();
    let paths = OrchestratorStatePaths::for_persona(&base, PersonaKind::Primary);
    {
        let mut log =
            sovereign_ai::session_log::SessionLog::open_encrypted(&paths.dir, STRANGER_KEY, Some(&paths.beacon)).unwrap();
        log.log_user_input("chat", "a turn under another key", "chat");
    }
    drop(sovereign_ai::session_log::SessionLog::open_encrypted(&paths.dir, STRANGER_KEY, Some(&paths.beacon)).unwrap());
    assert!(quarantines(&paths.dir).is_empty(), "precondition: no quarantine before the login");
    crate::crypto::create_auth_store(PRIMARY.as_bytes(), DURESS.as_bytes()).expect("create auth.store");

    let mut app = App::new();
    first_frame(&mut app);
    app.auth_work = FakeUnlock::new(&app);
    submit_login(&mut app, PRIMARY);
    assert!(frames_until(&mut app, 60, login_settled), "the login ends");
    assert!(!app.locked, "precondition: the session opens: {:?}", app.auth_form.error);

    let checked = frames_until(&mut app, 15, |_| !quarantines(&paths.dir).is_empty());
    assert!(
        checked,
        "LG5: the log is checked at login, before the assistant is used: a log written under another \
         key is quarantined after the login alone, with no chat message (the directory holds {:?})",
        std::fs::read_dir(&paths.dir).map(|rd| rd.flatten().map(|e| e.file_name()).collect::<Vec<_>>()).unwrap_or_default()
    );
    shut_runtime(&mut app);
    std::mem::forget(app);
}

/// One launch on the profile of `SOVEREIGN_DATA_DIR` (set by the parent). Ignored in ordinary
/// runs.
#[test]
#[ignore = "a launch, run by its parent test in a child process on a profile of its own"]
fn child_entry() {
    let Ok(scenario) = std::env::var(CHILD_VAR) else { return };
    match scenario.as_str() {
        "checked_at_login" => checked_at_login(),
        other => panic!("unknown scenario {other}"),
    }
}

#[test]
fn the_log_is_checked_at_login_before_the_assistant_is_used() {
    // LG5
    let p = profile();
    launch(module(), "checked_at_login", p.path());
}
