use super::*;

// ---------------------------------------------------------------------------
// Phase 4: Auth, Onboarding, Settings, Document deletion
// ---------------------------------------------------------------------------

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

/// Validate a password against the auth store. Returns persona ("primary" or "duress").
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
        let result = store
            .authenticate(password.as_bytes())
            .map_err(|_| "Invalid password".to_string())?;
        match result.persona {
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
        crate::seed::seed_if_empty(&state.db)
            .await
            .str_err()?;
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
