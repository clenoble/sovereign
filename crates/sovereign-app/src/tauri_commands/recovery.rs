//! B2 — pre-onboarding recovery commands (Workstream B).
//!
//! These run **before login**: a total-loss device has no unlocked DB and
//! no `AppState` session, so the commands are disk-backed — each loads the
//! persisted [`RecoveryState`] from `recovery_dir()`, acts, saves. This is
//! the same resumable engine the native shell drives in-process (see
//! `sovereign_p2p::recovery`), so the two frontends can't diverge.
//!
//! `recovery_finalize` is the install step: verify the passphrase against
//! the recovered material (wrong passphrase = retryable `wrong-passphrase:`
//! sentinel error, no disk writes), create the account FROM THE RECOVERED
//! SALT (login-derived keys must decrypt the restored content), install the
//! session, and restore the snapshot through the encrypting DB handle.

#[cfg(feature = "p2p")]
use std::time::Duration;

#[cfg(feature = "p2p")]
use sovereign_p2p::recovery::RecoveryState;

/// Wizard status (contract §2). Defined locally (house pattern, like
/// `BackupStatusDto`) so the always-compiled command signatures don't
/// depend on the `p2p`-gated engine crate; mapped from
/// `sovereign_p2p::recovery::RecoveryStatusDto` when `p2p` is on.
#[derive(serde::Serialize)]
pub struct RecoveryStatusDto {
    pub recovery_id: String,
    pub phase: String,
    pub shards_collected: u8,
    pub shards_needed: u8,
    pub fragments_collected: u8,
    pub fragments_needed: u8,
    pub guardians: Vec<GuardianStatusDto>,
    pub manifest_verified: Option<bool>,
    pub error: Option<String>,
}

#[derive(serde::Serialize)]
pub struct GuardianStatusDto {
    pub guardian_label: String,
    pub state: String,
    pub hours_remaining: Option<u32>,
}

#[cfg(feature = "p2p")]
impl From<sovereign_p2p::recovery::RecoveryStatusDto> for RecoveryStatusDto {
    fn from(s: sovereign_p2p::recovery::RecoveryStatusDto) -> Self {
        // phase serializes snake_case (contract §2) — reuse serde.
        let phase = serde_json::to_value(s.phase)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_else(|| "locating".into());
        Self {
            recovery_id: s.recovery_id,
            phase,
            shards_collected: s.shards_collected,
            shards_needed: s.shards_needed,
            fragments_collected: s.fragments_collected,
            fragments_needed: s.fragments_needed,
            guardians: s
                .guardians
                .into_iter()
                .map(|g| GuardianStatusDto {
                    guardian_label: g.guardian_label,
                    state: g.state.to_string(),
                    hours_remaining: g.hours_remaining,
                })
                .collect(),
            manifest_verified: s.manifest_verified,
            error: s.error,
        }
    }
}

/// `<sovereign_dir>/recovery` — where `recovery_state.json` lives across
/// the multi-day wait and app restarts.
#[cfg(feature = "p2p")]
fn recovery_dir() -> std::path::PathBuf {
    sovereign_core::sovereign_dir().join("recovery")
}

/// Handle returned by `start_recovery`.
#[derive(serde::Serialize)]
pub struct RecoveryHandle {
    pub recovery_id: String,
}

/// Begin a recovery. `owner_tag` is the account's backup tag from the
/// recovery card — hosts index backups and shard windows by exact tag, so
/// recovery cannot bootstrap without it. `guardians` = (guardian_id,
/// dialable `/p2p/` addr) pairs and `hosts` = dialable fragment-host
/// addrs, all resolved by the caller (from the recovery card /
/// seed-relay resolution). Overwrites any prior in-progress recovery.
///
/// NOTE (contract §2 divergence, flagged to PANDA2): the contract sketched
/// `start_recovery(account_hint)`; the engine needs the owner tag +
/// resolved addresses, so the real signature carries them. Command name
/// unchanged.
#[tauri::command]
pub async fn start_recovery(
    owner_tag: String,
    guardians: Vec<(String, String)>,
    hosts: Vec<String>,
) -> Result<RecoveryHandle, String> {
    // NOTE: arg names must stay underscore-free — Tauri maps the JS
    // invoke keys (camelCase) onto these exact snake_case names, and a
    // `_`-prefixed name silently never matches (found prepping the e2e
    // run; the previous `_guardians`/`_hosts` could never bind).
    #[cfg(feature = "p2p")]
    {
        // A fixed id per recovery attempt; deterministic-ish from the clock
        // is fine (only needs to be stable for this attempt's request-ids).
        let recovery_id = format!(
            "rec-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        );
        let state = RecoveryState::new(recovery_id.clone(), owner_tag, guardians, hosts);
        state.save(&recovery_dir()).map_err(|e| e.to_string())?;
        Ok(RecoveryHandle { recovery_id })
    }
    #[cfg(not(feature = "p2p"))]
    {
        let _ = (owner_tag, guardians, hosts);
        Err("recovery requires the p2p feature".into())
    }
}

/// Current status for the wizard (contract §2). `None` when no recovery is
/// in progress.
#[tauri::command]
pub async fn recovery_status() -> Result<Option<RecoveryStatusDto>, String> {
    #[cfg(feature = "p2p")]
    {
        Ok(RecoveryState::load(&recovery_dir()).map(|s| s.status().into()))
    }
    #[cfg(not(feature = "p2p"))]
    Ok(None)
}

/// Drive one network round (poll guardians, fetch fragments), persist, and
/// return the fresh status. The frontend calls this on a 30–60s timer while
/// the wizard is visible.
#[tauri::command]
pub async fn recovery_poll() -> Result<Option<RecoveryStatusDto>, String> {
    #[cfg(feature = "p2p")]
    {
        let dir = recovery_dir();
        let Some(mut state) = RecoveryState::load(&dir) else {
            return Ok(None);
        };
        state.poll_round(Duration::from_secs(20)).await;
        state.save(&dir).map_err(|e| e.to_string())?;
        Ok(Some(state.status().into()))
    }
    #[cfg(not(feature = "p2p"))]
    Ok(None)
}

/// Cancel + clear the persisted recovery (local only; a guardian that
/// already released its shard stays released on its side).
#[tauri::command]
pub async fn cancel_recovery() -> Result<(), String> {
    #[cfg(feature = "p2p")]
    {
        RecoveryState::cancel(&recovery_dir());
    }
    Ok(())
}

/// Error prefix the wizard matches with `startsWith` to tell "retype your
/// passphrase" apart from a real failure (agreed with PANDA2, coord 0035).
#[allow(dead_code)] // referenced under cfg(all(p2p, encryption)) only
pub const WRONG_PASSPHRASE_SENTINEL: &str = "wrong-passphrase:";

/// The install step (contract §2 finalize). Requires a collected recovery
/// (`ready_to_assemble`). Verifies the passphrase FIRST — a mismatch
/// returns a [`WRONG_PASSPHRASE_SENTINEL`]-prefixed error and touches
/// nothing on disk (shards stay valid, the user retypes). On match:
/// account created from the recovered salt, session installed (encrypted
/// DB wired in), snapshot restored through it, duress decoy pre-seeded.
/// Returns the final status (phase `installed`).
#[tauri::command]
pub async fn recovery_finalize(
    state: tauri::State<'_, crate::tauri_state::AppState>,
    passphrase: String,
) -> Result<RecoveryStatusDto, String> {
    #[cfg(all(feature = "p2p", feature = "encryption"))]
    {
        use crate::err::ToStringErr;

        // Same bootstrap guard as complete_onboarding (IPC-006): a pre-login
        // IPC caller must not overwrite an existing account.
        let crypto_dir = state.profile_dir.join("crypto");
        if crypto_dir.join("auth.store").exists() {
            return Err("Already onboarded — log in instead.".to_string());
        }

        let dir = recovery_dir();
        let Some(mut rec) = RecoveryState::load(&dir) else {
            return Err("no recovery in progress".to_string());
        };
        if !rec.ready_to_assemble() {
            return Err("recovery not ready to assemble (missing shards or fragments)".to_string());
        }

        // 1. Passphrase check before ANY disk write.
        match rec.passphrase_matches(passphrase.as_bytes()) {
            Ok(true) => {}
            Ok(false) => {
                return Err(format!(
                    "{WRONG_PASSPHRASE_SENTINEL} that passphrase doesn't match this backup — \
                     your guardians' shards remain valid, just retype it"
                ));
            }
            Err(e) => return Err(e.to_string()),
        }

        // 2. Create the account from the RECOVERED salt — the restored
        // content was encrypted under keys derived from passphrase+this
        // salt, so the device must log in with the same derivation.
        let salt = rec.recovered_salt().str_err()?;
        std::fs::create_dir_all(&crypto_dir).str_err()?;
        let device_id = uuid::Uuid::new_v4().to_string();
        // Duress: RANDOM unreachable decoy (H-shell1) — a recovered account
        // can enroll a real duress password later from settings.
        let random_duress = sovereign_crypto::random_hex_32();
        let auth_store = sovereign_crypto::auth::AuthStore::create(
            passphrase.as_bytes(),
            random_duress.as_bytes(),
            &salt,
            &device_id,
        )
        .str_err()?;
        auth_store.save(&crypto_dir.join("auth.store")).str_err()?;
        // Pairing QR needs the MasterKey salt on disk (same as onboarding).
        std::fs::write(crypto_dir.join("salt"), &salt).str_err()?;

        // Boot profile so the app has one after install.
        let mut profile = sovereign_core::profile::UserProfile::load(&state.profile_dir)
            .unwrap_or_else(|_| sovereign_core::profile::UserProfile::default_new());
        profile.save(&state.profile_dir).str_err()?;

        // 3. Unlock: wires EncryptedGraphDB into state.db before restore.
        crate::tauri_commands::auth::install_session(&state, &auth_store, passphrase.as_bytes())
            .await?;

        // 4. Restore through the encrypting handle.
        rec.finalize(&*state.db, passphrase.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        rec.save(&dir).str_err()?;

        // 5. Duress decoy pre-seed, best-effort (sidechannel-001 parity
        // with onboarding — first duress login must pay no seed cost).
        {
            let mut duress_config = state.config.clone();
            duress_config.database.path = crate::setup::persona_db_path(
                &state.config,
                sovereign_core::auth::PersonaKind::Duress,
            );
            match crate::setup::create_db(&duress_config).await {
                Ok(ddb) => {
                    if let Err(e) = crate::duress::seed_duress_db(&ddb).await {
                        tracing::warn!("duress decoy pre-seed at recovery failed (continuing): {e}");
                    }
                }
                Err(e) => {
                    tracing::warn!("duress decoy DB pre-create at recovery failed (continuing): {e}");
                }
            }
        }

        Ok(rec.status().into())
    }
    #[cfg(not(all(feature = "p2p", feature = "encryption")))]
    {
        let _ = (state, passphrase);
        Err("recovery requires the p2p and encryption features".into())
    }
}
