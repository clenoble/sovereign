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

    // 1. Install both keys into AppState. account_key serves vault /
    //    PII reveal / PII ingest. p2p_identity_key is consumed later by
    //    the P2P startup hook.
    state.set_account_key(account_key_arc.clone()).await;
    state.set_p2p_identity_key(device_key_arc.clone()).await;

    // 2. Call complete_auth and discard the returned key_db / kek —
    //    side effect: keys.duress.db is created on first duress login,
    //    even though no AppState consumer reads it today.
    if let Err(e) = crate::setup::complete_auth(persona, &device_key_arc, &auth_result.kek) {
        tracing::warn!("complete_auth side-effect failed: {e}");
    }

    // 2a. Wire EncryptedGraphDB into the runtime DB stack.
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
    //
    // Best-effort: a failure here would mean encryption is OFF for this
    // session (the raw inner stays). Logged but does not fail the login —
    // the user can still use the app, just without at-rest encryption.
    {
        let kek_arc = std::sync::Arc::new(sovereign_crypto::kek::Kek::from_bytes(
            *auth_result.kek.as_bytes(),
        ));
        // Wrap the *raw* inner (the bootstrap SurrealGraphDB), not the layer
        // itself — otherwise EncryptedGraphDB.inner would be the layer whose
        // current points back at us, looping on every DB call. raw_inner()
        // is the bootstrap reference held since init_backend.
        let raw_inner = state.db.raw_inner();
        match crate::setup::build_encrypted_db(raw_inner, &device_key_arc, kek_arc) {
            Ok(encrypted) => {
                state.db.swap(encrypted);
                tracing::info!("EncryptedGraphDB installed for this session");
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to install EncryptedGraphDB (continuing with plaintext inner): {e}"
                );
            }
        }
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
        let auth_path = state.profile_dir.join("crypto/auth.store");
        let store = sovereign_crypto::auth::AuthStore::load(&auth_path)
            .str_err()?;
        let persona = install_session(&state, &store, password.as_bytes()).await?;
        match persona {
            sovereign_crypto::auth::PersonaKind::Primary => Ok("primary".into()),
            sovereign_crypto::auth::PersonaKind::Duress => Ok("duress".into()),
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
