//! v0.0.12 item 22 — the shell fails closed when this device's identity
//! cannot be had (PANDA2's second position on item 22, suggestion 2; the
//! branch added by `b6df6b2`).
//!
//! When `App::stamp_identity` has no device key, or the derivation fails,
//! `install_document_service` installs no document service rather than stamp
//! with a shared stand-in (item 22's defect back, silently): the outgoing
//! service is dropped, with its purge and migration tasks and from the
//! orchestrator, and a notice says so. Test builds keep a fallback identity
//! for the fixtures that build a store without a key; these tests turn it
//! off to reach the production branch.
//!
//! Written before the seams, by another author: whoever adds them makes these
//! pass **without editing this file**.
//!
//! # The seams this file needs (it does not compile until they exist)
//!
//! 1. **On `App`, test builds only:**
//!
//!    ```ignore
//!    /// Whether `stamp_identity` falls back to the test-only identity when
//!    /// there is no device key or the derivation fails. `true` by default
//!    /// (`App::new`), so the fixtures that build a store without a key keep
//!    /// working; `false` takes the production branch (fail closed).
//!    #[cfg(test)]
//!    pub(crate) test_identity_fallback: bool,
//!    ```
//!
//! 2. **On `sovereign_ai::Orchestrator`**, so a shell test can hold an
//!    orchestrator and read whether it holds a document service:
//!
//!    ```ignore
//!    /// An orchestrator with no model loaded (as the crate's own `for_test`),
//!    /// for host tests. Compiled for the shell's tests: behind a
//!    /// `test-support` feature its dev-dependency enables, or `#[doc(hidden)]`
//!    /// — the implementer's choice.
//!    pub fn for_host_tests(
//!        db: Arc<dyn sovereign_db::GraphDB>,
//!        event_tx: std::sync::mpsc::Sender<sovereign_core::interfaces::OrchestratorEvent>,
//!        profile_dir: std::path::PathBuf,
//!    ) -> Orchestrator;
//!    /// Whether a document service is set (`set_document_service`) and not
//!    /// cleared (`clear_document_service`).
//!    pub fn has_document_service(&self) -> bool;
//!    ```
//!
//! Everything else is today's API (`install_document_service`, `docs`,
//! `orch`, `purge_task`, `migration_task`, `notice`), and the fixtures of
//! `item22_identity_tests` (`unlocked_as`, `fixed_device_key`, `written_by`).

use std::sync::Arc;

use sovereign_ai::Orchestrator;
use sovereign_db::traits::GraphDB;

use super::first_run_tests::notice_text;
use super::item22_identity_tests::{fixed_device_key, unlocked_as, written_by};
use super::App;

/// The notice of the fail-closed branch, as the coordinator gave it (and as
/// `b6df6b2` writes it, `IDENTITY_UNAVAILABLE`).
const IDENTITY_UNAVAILABLE: &str = "Your documents could not be opened: this device's identity could not be \
derived. Restart Sovereign; if it persists, report it.";

/// Installs an orchestrator (no model) holding the app's current document
/// service, as a second unlock finds one.
fn install_orchestrator(app: &mut App) -> Arc<Orchestrator> {
    let db = app.db.clone().expect("unlocked: a store");
    let dir = tempfile::tempdir().unwrap().keep();
    let o = Arc::new(Orchestrator::for_host_tests(db as Arc<dyn GraphDB>, app.orch_tx.clone(), dir));
    o.set_document_service(app.docs.clone().expect("unlocked: a document service"));
    let cell = app.orch.clone();
    let held = o.clone();
    app.rt.block_on(async move {
        *cell.lock().await = Some(held);
    });
    assert!(o.has_document_service(), "precondition: the orchestrator holds the service");
    o
}

// Fail closed: with the test-only fallback off and no device key, the
// unlock's `install_document_service` installs no document service — the
// outgoing one is dropped from the app and from the orchestrator, its purge
// and migration tasks end — and the notice says so in plain words.
#[test]
fn item22_without_a_device_key_no_document_service_is_installed_and_a_notice_says_so() {
    let mut u = unlocked_as("item22-fail-closed", fixed_device_key("fail-closed"), sovereign_core::auth::PersonaKind::Primary);
    assert!(u.app.test_identity_fallback, "precondition: the fallback is on by default (the fixtures need it)");
    let _ = written_by(&u.app, &u.thread, "Before");
    let orch = install_orchestrator(&mut u.app);
    u.app.notice = None;

    u.app.test_identity_fallback = false;
    u.app.device_key = None;
    u.app.install_document_service(u.store.clone());

    assert!(u.app.docs.is_none(), "fail closed: no document service is installed");
    assert!(!orch.has_document_service(), "fail closed: the orchestrator holds no document service");
    let held = u.app.rt.block_on(async { u.app.orch.lock().await.clone() });
    if let Some(o) = held {
        assert!(!o.has_document_service(), "fail closed: the app's orchestrator holds no document service");
    }
    assert!(u.app.purge_task.is_none(), "fail closed: the outgoing purge task is ended");
    assert!(u.app.migration_task.is_none(), "fail closed: the outgoing migration task is ended");
    let said = notice_text(&u.app).expect("fail closed: a notice is shown");
    assert!(said.contains(IDENTITY_UNAVAILABLE), "fail closed: the notice's words: {said:?}");
    std::mem::forget(u);
}

// Fail closed on a first unlock too: no service was there before, none is
// installed, the notice is shown, and nothing names a stand-in identity.
#[test]
fn item22_a_first_unlock_without_a_device_key_installs_nothing() {
    let mut u = unlocked_as("item22-fail-closed-first", fixed_device_key("fail-closed-first"), sovereign_core::auth::PersonaKind::Primary);
    // Back to a store with no service, as before an unlock.
    u.app.docs = None;
    u.app.notice = None;
    u.app.test_identity_fallback = false;
    u.app.device_key = None;
    u.app.install_document_service(u.store.clone());
    assert!(u.app.docs.is_none(), "fail closed: no document service is installed");
    let said = notice_text(&u.app).expect("fail closed: a notice is shown");
    assert!(said.contains(IDENTITY_UNAVAILABLE), "fail closed: the notice's words: {said:?}");
    std::mem::forget(u);
}

// The switch only turns the fallback off: with a device key, the service is
// installed as usual (the peer id), fallback on or off.
#[test]
fn item22_with_a_device_key_the_switch_changes_nothing() {
    let dk = fixed_device_key("fail-closed-key");
    let expected = sovereign_crypto::peer_identity::peer_id(&dk).unwrap();
    let mut u = unlocked_as("item22-fail-closed-key", dk, sovereign_core::auth::PersonaKind::Primary);
    u.app.notice = None;
    u.app.test_identity_fallback = false;
    u.app.install_document_service(u.store.clone());
    if let Some(task) = u.app.migration_task.take() {
        let _ = u.app.rt.block_on(task);
    }
    assert!(u.app.docs.is_some(), "with a device key, a document service is installed");
    let (_, written) = written_by(&u.app, &u.thread, "With a key");
    assert_eq!(written.0, expected, "it stamps with the persona's peer id");
    assert!(
        notice_text(&u.app).map_or(true, |s| !s.contains(IDENTITY_UNAVAILABLE)),
        "no identity notice when the identity is had"
    );
    std::mem::forget(u);
}
