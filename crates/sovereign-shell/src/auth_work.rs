//! v0.0.12 item 18 (`doc/plans/V0012-items18-20-first-run.md`, B1–B4): the
//! locked screens' slow, key-deriving steps, run on a worker thread.
//!
//! Setup, unlock (the login form, and the login that ends a join) and guardian
//! recovery's new password each derive keys (Argon2id, seconds in a debug
//! build) and open the persona's store. `App` used to `block_on` them from the
//! event handler, so no frame was drawn meanwhile. Now `App` hands them to
//! [`start`], keeps drawing (with a status line), and applies the result while
//! drawing a frame (`App::auth_drain`, called from `build_scene`), as it does
//! for the pairing handshake (`pair_rx`). Everything that needs the app's
//! state (the document service, the workspace, P2P) stays on the UI thread.
//!
//! The work itself is behind [`AuthWork`], so a test can replace it with a
//! derivation that waits. [`RealAuthWork`] holds the only calls to
//! `create_auth_store`, `install_session` and `recover_and_install` reached
//! from the window.

use std::sync::mpsc;
use std::sync::Arc;

use sovereign_crypto::auth::AuthStore;

/// What `install_session` returns: the persona, its encrypted store, and the
/// session's keys.
pub(crate) type Session = (
    sovereign_core::auth::PersonaKind,
    Arc<sovereign_db::encrypted::EncryptedGraphDB>,
    Arc<sovereign_crypto::account_key::AccountKey>,
    Arc<sovereign_crypto::device_key::DeviceKey>,
    Arc<sovereign_crypto::kek::Kek>,
);

/// The slow, key-deriving steps of the locked screens. `App` calls them
/// **only on a worker thread**, never on the UI thread; each may take seconds.
pub(crate) trait AuthWork: Send + Sync {
    /// First-device setup's store: `crypto::create_auth_store`.
    fn create_store(&self, primary: &[u8], duress: &[u8]) -> anyhow::Result<AuthStore>;
    /// The session over a store just created: `crypto::install_session`.
    fn install(&self, store: &AuthStore, password: &[u8]) -> anyhow::Result<Session>;
    /// Unlock: load `auth.store`, then `install_session`.
    fn unlock(&self, password: &[u8]) -> anyhow::Result<Session>;
    /// Guardian recovery's new password: `crypto::recover_and_install`.
    fn recover(&self, new_password: &[u8]) -> anyhow::Result<Session>;
}

/// The real derivations, over the app's tokio runtime (the store opens on it).
pub(crate) struct RealAuthWork {
    rt: tokio::runtime::Handle,
}

impl RealAuthWork {
    pub(crate) fn new(rt: tokio::runtime::Handle) -> Self {
        Self { rt }
    }
}

impl AuthWork for RealAuthWork {
    fn create_store(&self, primary: &[u8], duress: &[u8]) -> anyhow::Result<AuthStore> {
        crate::crypto::create_auth_store(primary, duress)
    }

    fn install(&self, store: &AuthStore, password: &[u8]) -> anyhow::Result<Session> {
        // A worker thread is outside the runtime, so it may block on it.
        self.rt.block_on(crate::crypto::install_session(store, password))
    }

    fn unlock(&self, password: &[u8]) -> anyhow::Result<Session> {
        let store = AuthStore::load(&crate::crypto::auth_store_path())
            .map_err(|e| anyhow::anyhow!("Auth store unreadable: {e}"))?;
        self.install(&store, password)
    }

    fn recover(&self, new_password: &[u8]) -> anyhow::Result<Session> {
        self.rt.block_on(crate::crypto::recover_and_install(new_password))
    }
}

/// B2's status lines, in plain words.
pub(crate) const CREATING: &str = "Creating your account\u{2026}";
pub(crate) const UNLOCKING: &str = "Unlocking\u{2026}";
pub(crate) const SETTING_PASSWORD: &str = "Setting your new password\u{2026}";

/// Which derivation is running (B3: one at a time).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum AuthJob {
    /// First-device setup (the wizard's last step).
    Setup,
    /// The login form, or the login that ends a join.
    Unlock,
    /// Guardian recovery's new password.
    Recover,
}

impl AuthJob {
    /// The status line shown, and announced, while it runs (B2).
    pub(crate) fn status(self) -> &'static str {
        match self {
            AuthJob::Setup => CREATING,
            AuthJob::Unlock => UNLOCKING,
            AuthJob::Recover => SETTING_PASSWORD,
        }
    }
}

/// A setup that failed, and whether `auth.store` had been written by then
/// (B5b: after the store, the wizard gives way to the login form).
pub(crate) struct SetupFailed {
    pub(crate) store_written: bool,
    pub(crate) error: anyhow::Error,
}

/// What a worker sends back.
pub(crate) enum AuthDone {
    Setup(Result<Session, SetupFailed>),
    Unlock(anyhow::Result<Session>),
    Recover(anyhow::Result<Session>),
}

impl AuthDone {
    /// The result of a worker that ended without sending one (it panicked).
    pub(crate) fn stopped(job: AuthJob) -> Self {
        let error = anyhow::anyhow!("the operation stopped unexpectedly");
        match job {
            AuthJob::Setup => {
                AuthDone::Setup(Err(SetupFailed { store_written: crate::crypto::auth_store_exists(), error }))
            }
            AuthJob::Unlock => AuthDone::Unlock(Err(error)),
            AuthJob::Recover => AuthDone::Recover(Err(error)),
        }
    }
}

/// First-device setup on the worker: the setup-started record (B5b as
/// amended, ff959de), the store, then the session over it.
pub(crate) fn setup(work: &dyn AuthWork, primary: &[u8], duress: &[u8]) -> Result<Session, SetupFailed> {
    // Best-effort: without it, a setup cut short after the store is taken for
    // an account from before this build (no notice), never the reverse.
    if let Err(e) = crate::crypto::mark_setup_started() {
        eprintln!("sovereign-shell: could not record that the setup started: {e}");
    }
    let store = work.create_store(primary, duress).map_err(|error| SetupFailed { store_written: false, error })?;
    work.install(&store, primary).map_err(|error| SetupFailed { store_written: true, error })
}

/// A derivation in flight: what it is, and where its result arrives.
pub(crate) struct Running {
    pub(crate) job: AuthJob,
    pub(crate) rx: mpsc::Receiver<AuthDone>,
}

/// Runs `run` on a worker thread of its own. Only owned data crosses: the
/// passwords are moved in, the session (all `Arc`s of `Send + Sync` types)
/// comes back over the channel.
pub(crate) fn start(
    work: Arc<dyn AuthWork>,
    job: AuthJob,
    run: impl FnOnce(&dyn AuthWork) -> AuthDone + Send + 'static,
) -> std::io::Result<Running> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new().name("sovereign-auth-work".into()).spawn(move || {
        let _ = tx.send(run(work.as_ref()));
    })?;
    Ok(Running { job, rx })
}
