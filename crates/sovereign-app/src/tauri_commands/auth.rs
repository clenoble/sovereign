use super::*;

// ---------------------------------------------------------------------------
// Phase 4: Auth, Onboarding, Settings, Document deletion
// ---------------------------------------------------------------------------

/// Authenticate a password against the AuthStore and install the
/// resulting keys across AppState + the orchestrator. Used by both
/// `validate_password` (login flow) and `complete_onboarding`
/// (first-run flow). Returns the persona that authenticated.
///
/// Two keys are installed:
///   - `account_key` (user-scoped) — consumed by vault, PII reveal,
///     PII ingest, and the encrypted session log. Same value on every
///     paired device.
///   - `p2p_identity_key` (per-device) — consumed by libp2p identity
///     derivation. Different on every device.
#[cfg(feature = "encryption")]
pub(crate) async fn install_session(
    state: &AppState,
    auth_store: &sovereign_crypto::auth::AuthStore,
    password: &[u8],
) -> Result<sovereign_crypto::auth::PersonaKind, String> {
    install_session_inner(state, auth_store, password, true).await
}

#[cfg(feature = "encryption")]
async fn install_session_inner(
    state: &AppState,
    auth_store: &sovereign_crypto::auth::AuthStore,
    password: &[u8],
    start_sync: bool,
) -> Result<sovereign_crypto::auth::PersonaKind, String> {
    let auth_result = auth_store
        .authenticate(password)
        .map_err(|_| "Invalid password".to_string())?;
    let persona = auth_result.persona;
    let device_key_arc = std::sync::Arc::new(auth_result.device_key);
    let account_key_arc = std::sync::Arc::new(auth_result.account_key);
    // v0.0.12 item 22 (D1, D6): this persona's device identity is the libp2p
    // peer id derived from its own device key — the id its P2P node announces —
    // never the configured display name. Its document service and, for the
    // duress persona, its decoy are stamped with it.
    let identity = sovereign_crypto::peer_identity::peer_id(&device_key_arc)
        .map_err(|e| format!("Could not derive this device's identity ({e})"))?;

    // 1. Call complete_auth and discard the returned key_db / kek —
    //    side effect: keys.duress.db is created on first duress login,
    //    even though no AppState consumer reads it today.
    if let Err(e) = crate::setup::complete_auth(persona, &device_key_arc, &auth_result.kek) {
        tracing::warn!("complete_auth side-effect failed: {e}");
    }

    // 2. Wire EncryptedGraphDB into the runtime DB stack — BEFORE the session
    //    is marked unlocked (step 3), so a failure here aborts the login rather
    //    than leaving an "unlocked" session that writes plaintext.
    //
    // The app boots with the raw SurrealGraphDB wrapped in a LayeredGraphDB
    // (see lib.rs::init_backend). Here we build the encryption decorator
    // around a fresh raw reference and swap it in atomically. After this
    // returns, every consumer that calls through `state.db` — orchestrator,
    // skill registry, all 60+ tauri commands — automatically goes through
    // EncryptedGraphDB. Mixed-row tolerance is built into the decrypt paths
    // (each `*_nonce` field guards a per-field decrypt branch), so pre-login
    // plaintext rows (seed data, v0.0.5 desktop state) continue to read
    // correctly; new writes from this point on are encrypted.
    {
        let kek_arc = std::sync::Arc::new(sovereign_crypto::kek::Kek::from_bytes(
            *auth_result.kek.as_bytes(),
        ));
        // CRYPTO-001 (v0.0.7): the raw store is PERSONA-SPECIFIC. The primary
        // persona wraps the boot DB (raw_inner); the DURESS persona gets a
        // SEPARATE database file (persona_db_path → `<db>-duress.db`) seeded
        // with innocuous decoy data, so a coerced login can never reach the
        // primary persona's real rows. The key DBs and blind index are likewise
        // persona-suffixed inside build_encrypted_db. Previously both personas
        // shared one raw DB + one key DB, so duress either failed to decrypt
        // (locking the decoy out) or exposed REAL data.
        let core_persona = match persona {
            sovereign_crypto::auth::PersonaKind::Primary => {
                sovereign_core::auth::PersonaKind::Primary
            }
            sovereign_crypto::auth::PersonaKind::Duress => {
                sovereign_core::auth::PersonaKind::Duress
            }
        };
        // SIDECHANNEL-001: time the persona-distinguishing DB setup so we can pad
        // it to a constant floor below. The primary persona reuses the already-
        // open boot DB; the duress persona opens + seeds a SEPARATE DB. Without
        // padding, that extra work makes a coerced (duress) login measurably
        // slower over IPC — defeating plausible deniability. Pad both to the same
        // wall-clock so login latency reveals nothing about which persona unlocked.
        let db_setup_start = std::time::Instant::now();
        let raw_for_persona: std::sync::Arc<dyn sovereign_db::GraphDB> = match core_persona {
            // Wrap the *raw* inner (the bootstrap SurrealGraphDB), not the layer
            // itself — otherwise EncryptedGraphDB.inner would be the layer whose
            // current points back at us, looping on every DB call. raw_inner()
            // is the bootstrap reference held since init_backend.
            sovereign_core::auth::PersonaKind::Primary => state.db.raw_inner(),
            sovereign_core::auth::PersonaKind::Duress => {
                let mut duress_config = state.config.clone();
                duress_config.database.path =
                    crate::setup::persona_db_path(&state.config, core_persona);
                match crate::setup::create_db(&duress_config).await {
                    Ok(ddb) => {
                        // Seed plausible decoy data on first duress login
                        // (seed_duress_db is idempotent — no-op once populated).
                        if let Err(e) = crate::duress::seed_duress_db(&ddb, &identity).await {
                            tracing::warn!("duress decoy seed failed (continuing): {e}");
                        }
                        std::sync::Arc::new(ddb)
                    }
                    Err(e) => {
                        tracing::error!("duress profile DB open failed; aborting login: {e}");
                        return Err(format!("Could not open the duress profile ({e})"));
                    }
                }
            }
        };
        let encrypted = match crate::setup::build_encrypted_db(
            raw_for_persona,
            device_key_arc.clone(),
            kek_arc.clone(),
            core_persona,
        ) {
            Ok(encrypted) => encrypted,
            Err(e) => {
                // CRYPTO-001 (v0.0.7): fail CLOSED. build_encrypted_db creates
                // fresh per-entity key DBs when the files are absent, so an
                // error here is a genuine failure (corrupt or wrong-device key
                // file, disk error) — never an expected first run. Continuing
                // would silently persist every later row in plaintext under a
                // UI that believes it is encrypted, so abort the login with a
                // visible error and leave the session locked (no account key is
                // installed below — step 3 is never reached).
                tracing::error!("EncryptedGraphDB install failed; aborting login: {e}");
                return Err(format!(
                    "Could not initialize at-rest encryption — login aborted to protect your data ({e})"
                ));
            }
        };
        // SIDECHANNEL-001: pad the persona-specific DB setup to a constant floor.
        // The floor comfortably exceeds opening + seeding the duress decoy DB, so
        // the primary path (which just reuses the boot DB) waits the same total —
        // duress and primary logins become indistinguishable by IPC latency.
        const DB_SETUP_FLOOR: std::time::Duration = std::time::Duration::from_millis(400);
        let elapsed = db_setup_start.elapsed();
        if elapsed < DB_SETUP_FLOOR {
            tokio::time::sleep(DB_SETUP_FLOOR - elapsed).await;
        }
        // C-DOC adoption step 2: the document service over this persona's
        // encrypted store (document saves, skill write-back and restores go
        // through it). Its personal-data hook needs the account key.
        // v0.0.12 item 7, V3: under the app's own build version, which the
        // one-time conversion records and compares (it retries what it could
        // not decrypt only under another version).
        let docs = std::sync::Arc::new(
            sovereign_ai::docs_host::build_document_service(
                encrypted.clone(),
                Some(account_key_arc.clone()),
                identity.clone(),
            )
            .with_build_version(env!("CARGO_PKG_VERSION")),
        );
        *state.docs.write().await = Some(docs.clone());
        if let Ok(mut sessions) = state.edit_sessions.lock() {
            sessions.clear();
        }
        if let Some(ref orch) = state.orchestrator {
            orch.set_document_service(docs);
        }
        state.db.swap(encrypted);
        tracing::info!("EncryptedGraphDB installed for {core_persona:?} persona");
        // Feature 1: retain the KEK so recovery-setup can seal it (+ AccountKey)
        // into the guardian Recovery Bundle. Only the PRIMARY persona's KEK —
        // the duress persona must never provision real-account recovery.
        if core_persona == sovereign_core::auth::PersonaKind::Primary {
            state.set_kek(kek_arc).await;
        }
    }

    // 3. Encryption is installed: mark the session unlocked by installing both
    //    keys into AppState. account_key serves vault / PII reveal / PII ingest;
    //    p2p_identity_key is consumed by the P2P startup hook below. Reaching
    //    this point means require_session_unlocked() will now return Ok.
    state.set_account_key(account_key_arc.clone()).await;
    state.set_p2p_identity_key(device_key_arc.clone()).await;

    // Feature 1 anti-fraud (invalidate-by-passphrase, spec §Guardian Social
    // Recovery): a successful login proves the owner has their passphrase, so
    // any access-recovery in progress ON THIS DEVICE is moot — clear it. This
    // is the LOCAL half of the spec's "the owner invalidates a concurrent
    // takeover by authenticating"; it defeats a fraudulent recovery started on
    // the owner's own device the moment they remember the passphrase.
    //
    // SCOPE (recorded 2026-07-11): the CROSS-DEVICE half — the owner's login
    // signalling guardians to auto-deny a recovery running on an *attacker's*
    // device — needs an owner→guardian liveness channel and is deferred. Until
    // then the cross-device case still relies on the (already-built) all-5
    // notification + 72h window + guardian deny.
    #[cfg(feature = "p2p")]
    if persona == sovereign_crypto::auth::PersonaKind::Primary {
        let access_dir = sovereign_core::sovereign_dir().join("recovery");
        if sovereign_p2p::access_recovery::AccessRecovery::load(&access_dir).is_some() {
            sovereign_p2p::access_recovery::AccessRecovery::cancel(&access_dir);
            tracing::info!("login cancelled an in-progress local access recovery (owner has the passphrase)");
        }
        // Finding 8: the held recovery secrets (seal key + new passphrase) leave
        // memory too, whether or not a recovery file was still on disk.
        state.clear_recovery_session().await;
    }

    // 3b. MODELTRUST-002: install the model-integrity unlock key + TOFU store
    //     path now that a session is unlocked. This enables trust-on-first-use
    //     verification of unlisted (custom / hot-swapped) models for the rest of
    //     the session; pinned models in the embedded manifest are verified
    //     regardless of login state. Keyed off the AccountKey so the TOFU store
    //     can't be forged by an attacker who can only write the model directory.
    //     MODELTRUST-003-PERSONA: one store per persona, as the shell has done
    //     since v0.0.9 (sovereign-shell/src/crypto.rs `persona_model_tofu_path`).
    //     The store is sealed under the persona's AccountKey, so a shared file
    //     can't be read across personas and the two clobber each other on write.
    //     This path had been missed in the Tauri app (found 2026-10-09).
    {
        let tofu_file = match persona {
            sovereign_crypto::auth::PersonaKind::Primary => "model_tofu.json",
            sovereign_crypto::auth::PersonaKind::Duress => "model_tofu.duress.json",
        };
        let tofu_path = state.profile_dir.join("crypto").join(tofu_file);
        sovereign_ai::model_integrity::set_unlock_key(*account_key_arc.as_bytes(), tofu_path);
    }

    // 3. Wire orchestrator: inline PII tokenization in chat I/O uses
    //    the account_key now (was device_key in v0.0.4).
    if let Some(ref orch) = state.orchestrator {
        orch.set_pii_account_key(account_key_arc.clone());
        // ai-safety M1: authenticate the persisted trust state (keyed off
        // the AccountKey, like the TOFU store) — auto-approval only works
        // on MAC-verified counts.
        orch.arm_trust_state(*account_key_arc.as_bytes());
    }

    // 4. Enable encrypted session log if the feature is on.
    #[cfg(feature = "encrypted-log")]
    if let Some(ref orch) = state.orchestrator {
        // v0.0.12 item 10, L6: this host's one orchestrator was built at
        // startup with the primary's paths, so the log is pointed at the
        // logging-in persona's own directory and beacon before its key is
        // installed. Otherwise a duress login would check, quarantine and
        // announce the primary's log: a duress leak.
        let log_persona = match persona {
            sovereign_crypto::auth::PersonaKind::Primary => sovereign_core::auth::PersonaKind::Primary,
            sovereign_crypto::auth::PersonaKind::Duress => sovereign_core::auth::PersonaKind::Duress,
        };
        let log_paths = sovereign_ai::OrchestratorStatePaths::for_persona(
            &sovereign_core::sovereign_dir(),
            log_persona,
        );
        // SEAM D (v0.0.9 audit): derivation lifted to sovereign-crypto so the
        // native shell shares it — same info string, same key.
        orch.set_session_log_key_for(&log_paths, account_key_arc.derive_session_log_key());
    }

    // 5. v0.0.4 → v0.0.5 migration: re-encrypt at-rest data under the
    //    AccountKey. Idempotent via marker file at
    //    ~/.sovereign/crypto/account_key.migrated. Best-effort — never
    //    fails the login flow on a bad row.
    {
        let db_dyn: std::sync::Arc<dyn sovereign_db::traits::GraphDB> = state.db.clone();
        if let Err(e) = crate::account_key_migration::migrate_to_account_key(
            db_dyn,
            &device_key_arc,
            &account_key_arc,
            &state.profile_dir,
        )
        .await
        {
            tracing::warn!("AccountKey migration failed (continuing): {e}");
        }
    }

    // 5b. C-DOC adoption step 6 (M1, M4): the one-time migration to document
    //     format v2, at every unlock, through the one seam. It runs in the
    //     background (the login returns without waiting for it) and only once
    //     per persona's store — the store's own record says so; a later
    //     unlock finds it complete, or resumes it. Its notice, once, goes to
    //     the frontend as the `documents-migrated` event. The same call for
    //     both personas: nothing here tells them apart (M5).
    if let Some(docs) = state.docs.read().await.clone() {
        let tx = state.orch_tx.clone();
        let mut slot = state.migration_task.lock().unwrap_or_else(|e| e.into_inner());
        sovereign_docs::start_unlock_migration(
            docs,
            |task| {
                if let Some(previous) = slot.replace(tauri::async_runtime::spawn(task)) {
                    previous.abort();
                }
            },
            move |notice| {
                let _ = tx.send(sovereign_core::interfaces::OrchestratorEvent::DocumentsMigrated {
                    text: notice.text(),
                });
            },
        );
    }

    // 6. P2P startup (Phase 3c): bring up the libp2p node, install the
    //    command channel on AppState + orchestrator, load paired
    //    devices, and spawn the event translator. Idempotent — re-login
    //    is a no-op on the second pass. Best-effort: a P2P bring-up
    //    failure must not fail the login. A backup recovery starts it
    //    itself, after the restore (`install_session_without_sync`).
    if start_sync {
        start_sync_now(state).await;
    }

    Ok(persona)
}

/// [`install_session`] without starting sync: for a backup recovery, which
/// restores into the empty workspace first and starts sync after, so a
/// paired device cannot fill the workspace before the restore (C-WIRE 17:
/// documents come back only onto an empty workspace; decided by Céline
/// 2026-10-15).
#[cfg(feature = "encryption")]
#[cfg_attr(not(feature = "p2p"), allow(dead_code))] // used by the backup recovery (p2p)
pub(crate) async fn install_session_without_sync(
    state: &AppState,
    auth_store: &sovereign_crypto::auth::AuthStore,
    password: &[u8],
) -> Result<sovereign_crypto::auth::PersonaKind, String> {
    install_session_inner(state, auth_store, password, false).await
}

/// Starts sync for the unlocked session (best-effort, idempotent).
#[allow(unused_variables)]
pub(crate) async fn start_sync_now(state: &AppState) {
    #[cfg(feature = "p2p")]
    {
        if let Err(e) = crate::sync_startup::start_p2p_node(state).await {
            tracing::warn!("P2P startup failed (continuing without sync): {e}");
        }
    }
}

/// Check whether the user needs onboarding or login.
///
/// v0.0.12 lot 1, P1: in a build with encryption the credential store
/// decides — the login screen when `crypto/auth.store` exists, the wizard
/// when it does not; the `onboarding_done` marker no longer routes (a setup
/// killed after the store would otherwise reopen the wizard, whose claim
/// refuses: "Already onboarded — log in instead", with no way out). The
/// marker still says whether the setup finished: `auth.store` without it is
/// P2's `setup_incomplete`. Without encryption the marker decides, as before.
#[tauri::command]
pub async fn check_auth_state(state: State<'_, AppState>) -> Result<AuthCheckResult, String> {
    let onboarding_done = state.profile_dir.join("onboarding_done").exists();

    #[cfg(feature = "encryption")]
    let (needs_onboarding, needs_login, setup_incomplete) = {
        let has_store = state.profile_dir.join("crypto").join("auth.store").exists();
        (!has_store, has_store, has_store && !onboarding_done)
    };

    #[cfg(not(feature = "encryption"))]
    let (needs_onboarding, needs_login, setup_incomplete) = (!onboarding_done, false, false);

    Ok(AuthCheckResult {
        needs_onboarding,
        needs_login,
        crypto_enabled: cfg!(feature = "encryption"),
        setup_incomplete,
    })
}

/// v0.0.12 lot 1, P2: the first successful login after an interrupted setup
/// writes the `onboarding_done` marker, so P2's notice is said once. Written
/// on either persona's login, so the two logins stay alike. A failure is
/// logged and does not fail the login (the notice would then show again).
#[cfg(feature = "encryption")]
fn mark_setup_finished_at_login(profile_dir: &std::path::Path) {
    let marker = profile_dir.join("onboarding_done");
    if marker.exists() {
        return;
    }
    match std::fs::write(&marker, "1") {
        Ok(()) => tracing::info!("setup had not finished; onboarding_done written at the first login"),
        Err(e) => tracing::warn!("could not write onboarding_done at login: {e}"),
    }
}

/// Return the orchestrator designation the onboarding wizard should display.
///
/// The designation is generated and owned by the backend (see
/// `UserProfile::default_new`). The wizard shows *this* value rather than
/// inventing its own client-side, so the name on the welcome screen is exactly
/// the one `complete_onboarding` persists and `get_profile` returns on every
/// later login. Creating + saving the profile here is safe pre-auth: the
/// onboarding gate keys off the `onboarding_done` marker, not `profile.json`.
#[tauri::command]
pub async fn onboarding_designation(state: State<'_, AppState>) -> Result<String, String> {
    let profile_dir = &state.profile_dir;
    let mut profile = sovereign_core::profile::UserProfile::load(profile_dir)
        .unwrap_or_else(|_| sovereign_core::profile::UserProfile::default_new());
    // Persist so complete_onboarding (and future launches) reuse this exact
    // designation instead of generating a fresh one.
    profile.save(profile_dir).str_err()?;
    Ok(profile.designation)
}

/// Validate a password against the auth store and install the session.
/// Returns persona ("primary" or "duress"). After this call returns Ok,
/// AppState.device_key is populated and the orchestrator has its PII /
/// session-log keys installed (vault, PII reveal, encrypted log work).
#[tauri::command]
pub async fn validate_password(
    state: State<'_, AppState>,
    password: String,
    keystrokes: Vec<KeystrokeSampleDto>,
) -> Result<String, String> {
    #[cfg(feature = "encryption")]
    {
        let _ = &keystrokes; // keystroke comparison deferred to future phase
        let crypto_dir = state.profile_dir.join("crypto");
        let auth_path = crypto_dir.join("auth.store");
        let store = sovereign_crypto::auth::AuthStore::load(&auth_path)
            .str_err()?;

        // --- CRYPTO-002: server-side login lockout ---------------------------
        // The login command historically forwarded max_login_attempts /
        // lockout_seconds to the UI but enforced nothing in Rust, so scripted
        // IPC could guess passwords unthrottled, amplifying the at-rest
        // brute-force surface. We now enforce the lockout here, BEFORE touching
        // the auth store.
        //
        // The tracker (`crypto/login_attempts.json`) is a plaintext file that
        // defends against online/scripted guessing via IPC. An attacker with
        // filesystem access already holds auth.store and attacks it directly
        // (covered by the Argon2id at-rest fix CRYPTO-001), so FS-tamper of the
        // counter is out of scope — clearing it only resets the online throttle.
        let max = state.config.crypto.max_login_attempts;
        let lockout_secs = state.config.crypto.lockout_seconds;
        let mut attempts = crate::login_throttle::LoginAttempts::load(&crypto_dir);

        match attempts.is_locked(max, lockout_secs) {
            Some(remaining) => {
                return Err(format!(
                    "Too many failed login attempts — locked for {remaining} seconds"
                ));
            }
            None if max > 0 && attempts.failed_count >= max => {
                // We hit the limit on a previous window, but that window has
                // now fully elapsed (is_locked returned None). Clear the stale
                // window so a fresh count starts on the next failure.
                attempts.reset();
                if let Err(e) = attempts.save(&crypto_dir) {
                    tracing::warn!("login_throttle: failed to persist window reset: {e}");
                }
            }
            None => {}
        }

        // Constant per-attempt delay to throttle scripted guessing regardless
        // of outcome.
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;

        // authenticate (via install_session) returns Ok for BOTH the primary
        // AND the duress persona — both are "success" and must reset the
        // counter identically. The only difference is the downstream persona
        // string we return; the throttle path must NOT leak which one unlocked.
        match install_session(&state, &store, password.as_bytes()).await {
            Ok(persona) => {
                attempts.reset();
                if let Err(e) = attempts.save(&crypto_dir) {
                    tracing::warn!("login_throttle: failed to persist reset on success: {e}");
                }
                mark_setup_finished_at_login(&state.profile_dir);
                match persona {
                    sovereign_crypto::auth::PersonaKind::Primary => Ok("primary".into()),
                    sovereign_crypto::auth::PersonaKind::Duress => Ok("duress".into()),
                }
            }
            Err(e) => {
                attempts.record_failure();
                if let Err(save_err) = attempts.save(&crypto_dir) {
                    tracing::warn!("login_throttle: failed to persist failure: {save_err}");
                }
                // Return the existing auth error UNCHANGED.
                Err(e)
            }
        }
    }
    #[cfg(not(feature = "encryption"))]
    {
        let _ = (&state, &password, &keystrokes);
        Ok("primary".into())
    }
}

/// Validate a password against the password policy (strength/complexity).
#[tauri::command]
pub async fn validate_password_policy(
    state: State<'_, AppState>,
    password: String,
) -> Result<PasswordValidationDto, String> {
    #[cfg(feature = "encryption")]
    {
        let _ = &state;
        let policy = sovereign_crypto::auth::PasswordPolicy::default_policy();
        let result = policy.validate(&password);
        Ok(PasswordValidationDto {
            valid: result.valid,
            errors: result.errors,
        })
    }
    #[cfg(not(feature = "encryption"))]
    {
        let _ = (&state, &password);
        Ok(PasswordValidationDto {
            valid: true,
            errors: vec![],
        })
    }
}

/// The words of P4's refusal: the desktop wizard's (`sovereign-shell`,
/// `onboarding.rs`), used by every account-creation command.
pub(crate) const DURESS_MUST_DIFFER: &str = "Duress password must differ from your password.";

/// v0.0.12 lot 1, P3 and P4: what the user typed twice, checked in the
/// backend **before the claim and before any key derivation or write**.
///
/// P3: a given value (`Some`, as sent: the wizard trims, this does not) whose
/// confirmation is absent or differs is refused, in the wizard's register.
/// An entry left empty (`None`) needs no confirmation. P4: a given duress
/// password equal to the password is refused with the desktop's words (the
/// store refuses it too, in all three `AuthStore` constructors).
pub(crate) fn check_typed_twice(
    password: Option<&str>,
    password_confirm: Option<&str>,
    duress: Option<&str>,
    duress_confirm: Option<&str>,
    phrase: Option<&str>,
    phrase_confirm: Option<&str>,
) -> Result<(), String> {
    let entries = [
        ("password", password, password_confirm),
        ("duress password", duress, duress_confirm),
        ("safety phrase", phrase, phrase_confirm),
    ];
    for (what, value, confirm) in entries {
        if let Some(value) = value {
            if confirm != Some(value) {
                return Err(format!("The {what} and its confirmation don't match."));
            }
        }
    }
    if let (Some(p), Some(d)) = (password, duress) {
        if p == d {
            return Err(DURESS_MUST_DIFFER.to_string());
        }
    }
    Ok(())
}

/// v0.0.12 lot 1, P2b: a step that failed after `auth.store` was written. The
/// account exists, so the setup is reported incomplete instead of failing;
/// the failure is logged. Steps that fail before the store still fail the
/// command (`store_written == false` returns the error).
fn after_store(
    step: &str,
    result: Result<(), String>,
    store_written: bool,
    incomplete: &mut bool,
) -> Result<(), String> {
    match result {
        Ok(()) => Ok(()),
        Err(e) if store_written => {
            tracing::warn!("setup step '{step}' failed after auth.store was written (setup incomplete): {e}");
            *incomplete = true;
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// Complete the onboarding wizard (save profile, optional crypto setup, seed data).
///
/// Returns [`OnboardingOutcome`]: once `auth.store` is written, a later
/// failure makes the setup `incomplete` rather than an error (P2b), and the
/// `onboarding_done` marker is written only when every step succeeded, so
/// the first login shows P2's notice.
#[tauri::command]
pub async fn complete_onboarding(
    state: State<'_, AppState>,
    data: OnboardingData,
) -> Result<OnboardingOutcome, String> {
    let profile_dir = &state.profile_dir;

    // v0.0.12 lot 1, P3/P4: the confirmations and duress ≠ password, before
    // the claim and before anything is derived or written.
    check_typed_twice(
        data.password.as_deref(),
        data.password_confirm.as_deref(),
        data.duress_password.as_deref(),
        data.duress_confirm.as_deref(),
        data.canary_phrase.as_deref(),
        data.canary_confirm.as_deref(),
    )?;

    // rc2 (spec ch. 07, *Auth store*, "Created once"): account creation is
    // single-flight. The claim is taken first — before the key derivation and
    // before any file is written — and held until auth.store is written (to
    // the end of this command), so a second tap during the derivation is
    // refused ("Setup is already in progress") instead of replacing the store.
    // It also carries IPC-006: this is a BOOTSTRAP (pre-auth) command, and an
    // existing auth.store refuses ("Already onboarded — log in instead"), so a
    // pre-login IPC caller cannot re-onboard with its own password.
    let _claim = crate::onboarding_claim::claim(profile_dir).map_err(|e| e.to_string())?;

    // Save user profile
    let mut profile = sovereign_core::profile::UserProfile::load(profile_dir)
        .unwrap_or_else(|_| sovereign_core::profile::UserProfile::default_new());
    if let Some(ref nick) = data.nickname {
        profile.nickname = Some(nick.clone());
    }
    if let Some(ref style) = data.bubble_style {
        profile.bubble_style =
            serde_json::from_str(&format!("\"{style}\"")).unwrap_or_default();
    }
    // Persist the theme picked during the wizard (the wizard toggles
    // state.theme via the toggle_theme command, but at that point the
    // profile may not exist yet, so we capture the current value here).
    if let Ok(theme_guard) = state.theme.lock() {
        profile.theme = theme_guard.clone();
    }
    profile.save(profile_dir).str_err()?;

    // P2b: set once auth.store is written; from then on a failed step marks
    // the setup incomplete instead of failing the command.
    #[cfg_attr(not(feature = "encryption"), allow(unused_mut))]
    let mut store_written = false;
    let mut incomplete = false;

    // Create crypto stores if encryption enabled and password provided
    #[cfg(feature = "encryption")]
    if let Some(ref password) = data.password {
        let crypto_dir = profile_dir.join("crypto");
        std::fs::create_dir_all(&crypto_dir).str_err()?;

        let salt: [u8; 32] = rand::random();
        let device_id = uuid::Uuid::new_v4().to_string();
        // Duress optional → RANDOM unreachable decoy, never the shared
        // source-visible literal (H-shell1 theme; matches the wizard). An
        // empty duress password counts as none: it would open the decoy with
        // an empty string.
        let random_duress = sovereign_crypto::random_hex_32();
        let duress = data
            .duress_password
            .as_deref()
            .filter(|d| !d.is_empty())
            .unwrap_or(&random_duress);

        let auth_store = sovereign_crypto::auth::AuthStore::create(
            password.as_bytes(),
            duress.as_bytes(),
            &salt,
            &device_id,
        )
        .str_err()?;
        // Exclusive create: never replaces an existing auth.store (the claim
        // already refused one; this holds against another process too).
        // The mode (hard link or in-place) is logged: whether the platform's
        // sandbox allows hard links is observed on the device, not assumed.
        let mode = auth_store
            .save_new(&crypto_dir.join("auth.store"))
            .map_err(crate::onboarding_claim::save_new_error)?;
        tracing::info!("auth.store published (onboarding): {mode:?}");
        // P2b: the account exists from here on.
        store_written = true;

        // Persist the MasterKey salt so this device can later generate a
        // pairing QR (generate_pair_qr releases it to the new device). The
        // paired-onboarding path writes this too; first-device onboarding
        // previously omitted it, which broke "Pair a new device". The salt
        // is not secret — it travels in the pairing offer. Written atomically
        // and owner-only; if a crash loses it, the next start restores it
        // from auth.store (setup::load_or_create_salt).
        after_store(
            "salt",
            sovereign_crypto::fs_private::write_private(&crypto_dir.join("salt"), &salt).str_err(),
            store_written,
            &mut incomplete,
        )?;

        // Install the session immediately so the user lands in a fully
        // unlocked state (vault, PII pipeline, encrypted session log).
        after_store(
            "session",
            install_session(&state, &auth_store, password.as_bytes()).await.map(|_| ()),
            store_written,
            &mut incomplete,
        )?;

        // sidechannel-001-floor-not-equalizer: pre-create + pre-seed the duress
        // decoy DB now, at onboarding, so the FIRST duress login pays no seed
        // cost. The login-time DB_SETUP_FLOOR pads the persona-distinguishing
        // work UP to a constant but cannot CAP it, so a first coerced login that
        // also seeded the decoy could overrun the floor and leak the persona via
        // latency. With the seed done here, both personas' login paths only OPEN
        // an existing DB (well within the floor). Best-effort — on failure the
        // first duress login falls back to seeding (the prior behavior), so a
        // failure here does not make the setup incomplete (P2: "The decoy is
        // created at the first duress login if it was not").
        {
            let mut duress_config = state.config.clone();
            duress_config.database.path = crate::setup::persona_db_path(
                &state.config,
                sovereign_core::auth::PersonaKind::Duress,
            );
            // v0.0.12 item 22 (D6): stamped with the duress persona's own peer
            // id, derived from its device key. Without it, no pre-seed: the
            // first duress login seeds the decoy with that id.
            let duress_identity = crate::duress::duress_identity(&auth_store, duress.as_bytes());
            match (crate::setup::create_db(&duress_config).await, duress_identity) {
                (Ok(ddb), Some(identity)) => {
                    if let Err(e) = crate::duress::seed_duress_db(&ddb, &identity).await {
                        tracing::warn!("duress decoy pre-seed at onboarding failed (continuing): {e}");
                    }
                }
                (Ok(_), None) => {
                    tracing::warn!("duress decoy pre-seed at onboarding skipped: no duress identity (continuing)");
                }
                (Err(e), _) => {
                    tracing::warn!("duress decoy DB pre-create at onboarding failed (continuing): {e}");
                }
            }
        }

        // The safety phrase and the typing profile are encrypted under the
        // KEK: one authentication serves both. Its failure is a failure of
        // both steps (before, it skipped them silently).
        let wants_canary = data.canary_phrase.is_some();
        let wants_keystrokes = !data.keystrokes.is_empty();
        if wants_canary || wants_keystrokes {
            match auth_store.authenticate(password.as_bytes()) {
                Ok(auth_result) => {
                    // Save canary phrase if provided
                    if let Some(ref phrase) = data.canary_phrase {
                        let saved = sovereign_crypto::canary::CanaryStore::encrypt(
                            phrase,
                            auth_result.kek.as_bytes(),
                        )
                        .and_then(|canary| canary.save(&crypto_dir.join("canary.store")))
                        .str_err();
                        after_store("safety phrase", saved, store_written, &mut incomplete)?;
                    }

                    // Save keystroke reference if enrollment data provided
                    if wants_keystrokes {
                        let saved =
                            save_typing_profile(&data.keystrokes, &auth_result.kek, &crypto_dir);
                        after_store("typing profile", saved, store_written, &mut incomplete)?;
                    }
                }
                Err(e) => {
                    after_store(
                        "safety phrase / typing profile",
                        Err(format!("authentication for the KEK failed: {e}")),
                        store_written,
                        &mut incomplete,
                    )?;
                }
            }
        }
    }

    // Seed sample data if requested. This is the ONLY seeding path: startup
    // no longer auto-seeds, so a fresh install that becomes a pairing target
    // comes up empty and receives its workspace via sync. The paired
    // onboarding flow (complete_onboarding_paired) never seeds.
    if data.seed_sample_data {
        // Documents are created through the document service (C-DOC adoption
        // step 3), installed by install_session above. Without one (a build
        // without the encryption feature) there are no sample documents.
        let docs = state.docs().await;
        match &docs {
            Ok(docs) => after_store(
                "sample documents",
                crate::seed::seed_if_empty(state.db.as_ref(), docs).await.str_err(),
                store_written,
                &mut incomplete,
            )?,
            // With an account, sample documents asked for and not seeded
            // (the session did not install) make the setup incomplete.
            Err(e) if store_written => after_store(
                "sample documents",
                Err(format!("no document service: {e}")),
                store_written,
                &mut incomplete,
            )?,
            Err(e) => tracing::warn!("sample documents not seeded: {e}"),
        }

        // Orchestrator context: fake "Alex" profile + multi-day session-log
        // history, kept consistent with the DB sample data seeded above.
        // (Previously seeded unconditionally at startup in init_backend.)
        let orchestrator_profile_dir = sovereign_core::auth::orchestrator_state_dir(
            &profile_dir,
            sovereign_core::auth::PersonaKind::Primary,
        );
        if let Err(e) = crate::seed::seed_profile_and_history(&orchestrator_profile_dir) {
            if store_written {
                after_store("sample history", Err(e.to_string()), store_written, &mut incomplete)?;
            } else {
                tracing::warn!("Profile/history seed failed: {e}");
            }
        }

        // PII seed needs the AccountKey to encrypt vault values, so it
        // can only run on builds with the encryption feature and once
        // install_session has populated AppState.account_key.
        #[cfg(feature = "encryption")]
        if let (Some(ak), Ok(docs)) = (state.account_key().await, &docs) {
            after_store(
                "sample PII",
                crate::seed::seed_pii_if_empty(state.db.as_ref(), docs, &ak).await.str_err(),
                store_written,
                &mut incomplete,
            )?;
        }
    }

    // Write onboarding_done marker — only when every step succeeded: its
    // absence beside auth.store is what makes the first login show P2's
    // notice (and write it then).
    if !incomplete {
        let marker = std::fs::create_dir_all(profile_dir)
            .and_then(|()| std::fs::write(profile_dir.join("onboarding_done"), "1"))
            .str_err();
        after_store("marker", marker, store_written, &mut incomplete)?;
    }

    Ok(OnboardingOutcome { incomplete })
}

/// Encrypt and write the typing profile enrolled during onboarding
/// (`crypto/keystroke.store`). Nothing is written when no sample is usable.
#[cfg(feature = "encryption")]
fn save_typing_profile(
    keystrokes: &[Vec<KeystrokeSampleDto>],
    kek: &sovereign_crypto::kek::Kek,
    crypto_dir: &std::path::Path,
) -> Result<(), String> {
    let profiles: Vec<sovereign_crypto::keystroke::TypingProfile> = keystrokes
        .iter()
        .map(|samples| sovereign_crypto::keystroke::TypingProfile {
            // Finding 20: a malformed sample decodes as unusable and is skipped.
            samples: samples
                .iter()
                .filter(|s| s.is_usable())
                .map(|s| sovereign_crypto::keystroke::KeystrokeSample {
                    key: s.key.clone(),
                    press_ms: s.press_ms,
                    release_ms: s.release_ms,
                })
                .collect(),
        })
        .collect();
    let reference = sovereign_crypto::keystroke::KeystrokeReference::from_enrollments(&profiles);
    if let Some(ref reference) = reference {
        let encrypted = reference.encrypt(kek.as_bytes()).str_err()?;
        let ks_json = serde_json::to_string(&encrypted).str_err()?;
        std::fs::write(crypto_dir.join("keystroke.store"), ks_json).str_err()?;
    }
    Ok(())
}

/// Get the current user profile.
#[tauri::command]
pub async fn get_profile(state: State<'_, AppState>) -> Result<UserProfileDto, String> {
    let profile = sovereign_core::profile::UserProfile::load(&state.profile_dir)
        .str_err()?;
    Ok(UserProfileDto {
        user_id: profile.user_id,
        designation: profile.designation,
        nickname: profile.nickname,
        bubble_style: serde_json::to_string(&profile.bubble_style)
            .unwrap_or_else(|_| "\"icon\"".into())
            .trim_matches('"')
            .to_string(),
        display_name: profile.display_name,
    })
}

/// Update user profile fields.
#[tauri::command]
pub async fn save_profile(
    state: State<'_, AppState>,
    data: SaveProfileDto,
) -> Result<(), String> {
    let mut profile = sovereign_core::profile::UserProfile::load(&state.profile_dir)
        .str_err()?;
    if let Some(ref nick) = data.nickname {
        profile.nickname = Some(nick.clone());
    }
    if let Some(ref style) = data.bubble_style {
        profile.bubble_style =
            serde_json::from_str(&format!("\"{style}\"")).unwrap_or_default();
    }
    if let Some(ref name) = data.display_name {
        profile.display_name = Some(name.clone());
    }
    profile
        .save(&state.profile_dir)
        .str_err()?;
    Ok(())
}

/// Get the flattened application configuration.
#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<AppConfigDto, String> {
    let config = &state.config;
    Ok(AppConfigDto {
        ai_model_dir: config.ai.model_dir.clone(),
        ai_router_model: config.ai.router_model.clone(),
        ai_reasoning_model: config.ai.reasoning_model.clone(),
        ai_n_gpu_layers: config.ai.n_gpu_layers,
        ai_n_ctx: config.ai.n_ctx,
        ai_prompt_format: config.ai.prompt_format.clone(),
        crypto_enabled: cfg!(feature = "encryption"),
        crypto_keystroke_enabled: config.crypto.keystroke_enabled,
        crypto_max_login_attempts: config.crypto.max_login_attempts,
        crypto_lockout_seconds: config.crypto.lockout_seconds,
        ui_theme: config.ui.theme.clone(),
    })
}
