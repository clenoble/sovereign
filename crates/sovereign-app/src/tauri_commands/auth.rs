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
async fn install_session(
    state: &AppState,
    auth_store: &sovereign_crypto::auth::AuthStore,
    password: &[u8],
) -> Result<sovereign_crypto::auth::PersonaKind, String> {
    let auth_result = auth_store
        .authenticate(password)
        .map_err(|_| "Invalid password".to_string())?;
    let persona = auth_result.persona;
    let device_key_arc = std::sync::Arc::new(auth_result.device_key);
    let account_key_arc = std::sync::Arc::new(auth_result.account_key);

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
                        if let Err(e) = crate::duress::seed_duress_db(&ddb).await {
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
            kek_arc,
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
        state.db.swap(encrypted);
        tracing::info!("EncryptedGraphDB installed for {core_persona:?} persona");
    }

    // 3. Encryption is installed: mark the session unlocked by installing both
    //    keys into AppState. account_key serves vault / PII reveal / PII ingest;
    //    p2p_identity_key is consumed by the P2P startup hook below. Reaching
    //    this point means require_session_unlocked() will now return Ok.
    state.set_account_key(account_key_arc.clone()).await;
    state.set_p2p_identity_key(device_key_arc.clone()).await;

    // 3b. MODELTRUST-002: install the model-integrity unlock key + TOFU store
    //     path now that a session is unlocked. This enables trust-on-first-use
    //     verification of unlisted (custom / hot-swapped) models for the rest of
    //     the session; pinned models in the embedded manifest are verified
    //     regardless of login state. Keyed off the AccountKey so the TOFU store
    //     can't be forged by an attacker who can only write the model directory.
    {
        let tofu_path = state.profile_dir.join("crypto").join("model_tofu.json");
        sovereign_ai::model_integrity::set_unlock_key(*account_key_arc.as_bytes(), tofu_path);
    }

    // 3. Wire orchestrator: inline PII tokenization in chat I/O uses
    //    the account_key now (was device_key in v0.0.4).
    if let Some(ref orch) = state.orchestrator {
        orch.set_pii_account_key(account_key_arc.clone());
    }

    // 4. Enable encrypted session log if the feature is on.
    #[cfg(feature = "encrypted-log")]
    if let Some(ref orch) = state.orchestrator {
        let session_key = crate::setup::derive_session_log_key(&account_key_arc);
        orch.set_session_log_key(session_key);
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

    // 6. P2P startup (Phase 3c): bring up the libp2p node, install the
    //    command channel on AppState + orchestrator, load paired
    //    devices, and spawn the event translator. Idempotent — re-login
    //    is a no-op on the second pass. Best-effort: a P2P bring-up
    //    failure must not fail the login.
    #[cfg(feature = "p2p")]
    {
        if let Err(e) = crate::sync_startup::start_p2p_node(state).await {
            tracing::warn!("P2P startup failed (continuing without sync): {e}");
        }
    }

    Ok(persona)
}

/// Check whether the user needs onboarding or login.
#[tauri::command]
pub async fn check_auth_state(state: State<'_, AppState>) -> Result<AuthCheckResult, String> {
    let onboarding_done = state.profile_dir.join("onboarding_done").exists();

    #[cfg(feature = "encryption")]
    let needs_login = onboarding_done && state.profile_dir.join("crypto/auth.store").exists();

    #[cfg(not(feature = "encryption"))]
    let needs_login = false;

    let _ = &state; // suppress unused warning in non-encryption build

    Ok(AuthCheckResult {
        needs_onboarding: !onboarding_done,
        needs_login,
        crypto_enabled: cfg!(feature = "encryption"),
    })
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

/// Complete the onboarding wizard (save profile, optional crypto setup, seed data).
#[tauri::command]
pub async fn complete_onboarding(
    state: State<'_, AppState>,
    data: OnboardingData,
) -> Result<(), String> {
    let profile_dir = &state.profile_dir;

    // IPC-006: this is a BOOTSTRAP (pre-auth) command — without this guard a
    // pre-login IPC caller could re-onboard with its own password, overwriting
    // auth.store and locking the real user out of their encrypted data.
    if profile_dir.join("crypto").join("auth.store").exists() {
        return Err("Already onboarded — log in instead.".to_string());
    }

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

    // Create crypto stores if encryption enabled and password provided
    #[cfg(feature = "encryption")]
    if let Some(ref password) = data.password {
        let crypto_dir = profile_dir.join("crypto");
        std::fs::create_dir_all(&crypto_dir).str_err()?;

        let salt: [u8; 32] = rand::random();
        let device_id = uuid::Uuid::new_v4().to_string();
        let duress = data
            .duress_password
            .as_deref()
            .unwrap_or("duress-fallback-unused");

        let auth_store = sovereign_crypto::auth::AuthStore::create(
            password.as_bytes(),
            duress.as_bytes(),
            &salt,
            &device_id,
        )
        .str_err()?;
        auth_store
            .save(&crypto_dir.join("auth.store"))
            .str_err()?;

        // Install the session immediately so the user lands in a fully
        // unlocked state (vault, PII pipeline, encrypted session log).
        install_session(&state, &auth_store, password.as_bytes()).await?;

        // Save canary phrase if provided
        if let Some(ref phrase) = data.canary_phrase {
            if let Ok(auth_result) = auth_store.authenticate(password.as_bytes()) {
                let canary =
                    sovereign_crypto::canary::CanaryStore::encrypt(phrase, auth_result.kek.as_bytes())
                        .str_err()?;
                canary
                    .save(&crypto_dir.join("canary.store"))
                    .str_err()?;
            }
        }

        // Save keystroke reference if enrollment data provided
        if !data.keystrokes.is_empty() {
            if let Ok(auth_result) = auth_store.authenticate(password.as_bytes()) {
                let profiles: Vec<sovereign_crypto::keystroke::TypingProfile> = data
                    .keystrokes
                    .iter()
                    .map(|samples| sovereign_crypto::keystroke::TypingProfile {
                        samples: samples
                            .iter()
                            .map(|s| sovereign_crypto::keystroke::KeystrokeSample {
                                key: s.key.clone(),
                                press_ms: s.press_ms,
                                release_ms: s.release_ms,
                            })
                            .collect(),
                    })
                    .collect();
                let reference =
                    sovereign_crypto::keystroke::KeystrokeReference::from_enrollments(&profiles);
                if let Some(ref reference) = reference {
                    let encrypted = reference.encrypt(auth_result.kek.as_bytes())
                        .str_err()?;
                    let ks_json =
                        serde_json::to_string(&encrypted).str_err()?;
                    std::fs::write(crypto_dir.join("keystroke.store"), ks_json)
                        .str_err()?;
                }
            }
        }
    }

    // Seed sample data if requested
    if data.seed_sample_data {
        crate::seed::seed_if_empty(state.db.as_ref())
            .await
            .str_err()?;

        // PII seed needs the AccountKey to encrypt vault values, so it
        // can only run on builds with the encryption feature and once
        // install_session has populated AppState.account_key.
        #[cfg(feature = "encryption")]
        if let Some(ak) = state.account_key().await {
            crate::seed::seed_pii_if_empty(state.db.as_ref(), &ak)
                .await
                .str_err()?;
        }
    }

    // Write onboarding_done marker
    std::fs::create_dir_all(profile_dir).str_err()?;
    std::fs::write(profile_dir.join("onboarding_done"), "1")
        .str_err()?;

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
