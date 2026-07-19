//! Feature 1 — pre-login Guardian **Access** Recovery commands.
//!
//! Canonical design: `doc/spec/sovereign_os_specification.md` §Guardian Social
//! Recovery. Run BEFORE login (the user forgot their passphrase). They read the
//! pre-login `recovery_card.json` + `recovery.bundle` this device already
//! holds, gather Recovery-Key shares from the guardians (72h each), reconstruct
//! the Recovery Key, open the bundle, and re-install the account under a NEW
//! passphrase (`AuthStore::create_with_secrets`). Data is already present via
//! sync, so nothing is fetched but the shares.
//!
//! DECISION (2026-07-11, autonomous — recorded for review): `finalize`
//! **overwrites** the existing (forgotten) `auth.store` with one that wraps the
//! recovered secrets under the new passphrase. This is the recovery outcome and
//! is gated by having ≥threshold guardian shares (each requiring the guardian's
//! approval + the 72h window), so it is not an open overwrite. The recovered
//! KEK/AccountKey are the same secrets the old store held, so synced content
//! stays decryptable; only the passphrase wrapping changes.

use serde::Serialize;
use tauri::State;

use crate::tauri_state::AppState;

#[cfg(all(feature = "p2p", feature = "encryption"))]
use crate::err::ToStringErr;

#[derive(Serialize)]
pub struct AccessGuardianDto {
    pub guardian_id: String,
    pub released: bool,
}

#[derive(Serialize)]
pub struct AccessRecoveryStatusDto {
    pub recovery_id: String,
    pub phase: String,
    pub shares_collected: u8,
    pub threshold: u8,
    pub guardians: Vec<AccessGuardianDto>,
    pub error: Option<String>,
}

#[cfg(all(feature = "p2p", feature = "encryption"))]
fn access_dir() -> std::path::PathBuf {
    sovereign_core::sovereign_dir().join("recovery")
}

#[cfg(all(feature = "p2p", feature = "encryption"))]
fn status_of(rec: &sovereign_p2p::access_recovery::AccessRecovery) -> AccessRecoveryStatusDto {
    let phase = serde_json::to_value(rec.phase)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "awaiting_shares".into());
    AccessRecoveryStatusDto {
        recovery_id: rec.recovery_id.clone(),
        phase,
        shares_collected: rec.shares_collected() as u8,
        threshold: rec.threshold,
        guardians: rec
            .guardians
            .iter()
            .map(|g| AccessGuardianDto {
                guardian_id: g.guardian_id.clone(),
                released: g.released,
            })
            .collect(),
        error: rec.error.clone(),
    }
}

/// Can this device do guardian access recovery? True when a recovery card +
/// bundle are present (recovery was set up here). Lets the login screen show
/// "Forgot your password? Recover with guardians".
#[tauri::command]
pub async fn access_recovery_available() -> Result<bool, String> {
    #[cfg(all(feature = "p2p", feature = "encryption"))]
    {
        let store = crate::setup::recovery_store();
        Ok(store.read_recovery_card()?.is_some() && store.read_bundle()?.is_some())
    }
    #[cfg(not(all(feature = "p2p", feature = "encryption")))]
    Ok(false)
}

/// Begin access recovery from this device's own recovery card + bundle. Builds
/// the guardian dial addresses (relay-circuit form) and starts polling.
#[tauri::command]
pub async fn start_access_recovery(
    state: State<'_, AppState>,
    new_passphrase: String,
) -> Result<AccessRecoveryStatusDto, String> {
    #[cfg(all(feature = "p2p", feature = "encryption"))]
    {
        use crate::tauri_state::RecoverySession;
        use sovereign_p2p::access_recovery::AccessRecovery;

        if new_passphrase.is_empty() {
            return Err("a new passphrase is required to start recovery".into());
        }

        let store = crate::setup::recovery_store();
        let card = store
            .read_recovery_card()?
            .ok_or_else(|| "no recovery card on this device".to_string())?;
        let bundle = store
            .read_bundle()?
            .ok_or_else(|| "no recovery bundle on this device".to_string())?;
        if card.relays.is_empty() {
            return Err("recovery card has no relay to reach guardians".into());
        }
        // Build one dialable circuit address per guardian via the first relay:
        // <relay .../p2p/RELAY>/p2p-circuit/p2p/<guardian>.
        let relay = &card.relays[0];
        let guardians: Vec<(String, String)> = card
            .guardian_peer_ids
            .iter()
            .map(|gid| (gid.clone(), format!("{relay}/p2p-circuit/p2p/{gid}")))
            .collect();
        if guardians.len() < card.threshold as usize {
            return Err(format!(
                "recovery card lists {} guardian(s) but {} are needed",
                guardians.len(),
                card.threshold
            ));
        }

        let recovery_id = format!(
            "acc-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        );
        let mut rec = AccessRecovery::new(
            recovery_id,
            card.owner_tag,
            card.epoch,
            card.threshold,
            guardians,
            bundle,
        );

        // RECOVERY-001: seal the in-progress shares under the NEW passphrase,
        // set now — before any guardian is contacted. A fresh random salt +
        // Argon2id (same hardness as the account passphrase) derives the seal
        // key; the on-disk `access_recovery.json` is then worthless without the
        // passphrase, so the "≥threshold cleartext shares reconstruct the whole
        // account" window goes from 72h to zero. The seal key is held in memory
        // (RecoverySession) for the rest of this app run; after a restart the
        // user calls `resume_access_recovery` to re-derive it.
        let salt = sovereign_crypto::random_hex_32().into_bytes();
        let seal_key =
            sovereign_crypto::recovery_seal::derive_seal_key(new_passphrase.as_bytes(), &salt)
                .map_err(|e| e.to_string())?;
        rec.set_seal_salt(salt);
        rec.save(&access_dir(), &seal_key).str_err()?;

        state
            .set_recovery_session(RecoverySession {
                seal_key,
                new_passphrase: sovereign_crypto::zeroize::Zeroizing::new(new_passphrase),
            })
            .await;

        Ok(status_of(&rec))
    }
    #[cfg(not(all(feature = "p2p", feature = "encryption")))]
    {
        let _ = (state, new_passphrase);
        Err("access recovery requires the p2p + encryption features".into())
    }
}

/// Resume an in-progress access recovery after an app restart (RECOVERY-001).
/// The recovery window spans days and the app process does not stay alive across
/// it, so on each launch the user re-enters the new passphrase they set at
/// `start`; we re-derive the share-sealing key from it + the stored salt, verify
/// it against the probe, and rehydrate the in-memory session. Returns
/// `Err("wrong-passphrase")` (the frozen SEAM-A sentinel) if it doesn't match —
/// caught even before any share has arrived.
#[tauri::command]
pub async fn resume_access_recovery(
    state: State<'_, AppState>,
    passphrase: String,
) -> Result<AccessRecoveryStatusDto, String> {
    #[cfg(all(feature = "p2p", feature = "encryption"))]
    {
        use crate::tauri_state::RecoverySession;
        use sovereign_p2p::access_recovery::AccessRecovery;

        let rec = AccessRecovery::load(&access_dir())
            .ok_or_else(|| "no access recovery in progress".to_string())?;
        let salt = rec.seal_salt();
        if salt.is_empty() {
            return Err("recovery state has no seal salt — cannot resume".into());
        }
        let seal_key =
            sovereign_crypto::recovery_seal::derive_seal_key(passphrase.as_bytes(), salt)
                .map_err(|e| e.to_string())?;
        if !rec.verify_probe(&seal_key) {
            return Err("wrong-passphrase".into());
        }
        state
            .set_recovery_session(RecoverySession {
                seal_key,
                new_passphrase: sovereign_crypto::zeroize::Zeroizing::new(passphrase),
            })
            .await;
        Ok(status_of(&rec))
    }
    #[cfg(not(all(feature = "p2p", feature = "encryption")))]
    {
        let _ = (state, passphrase);
        Err("access recovery requires the p2p + encryption features".into())
    }
}

/// One network round: poll guardians for their shares, persist, return status.
/// The wizard calls this on a slow timer across the multi-day wait.
#[tauri::command]
pub async fn access_recovery_poll(
    state: State<'_, AppState>,
) -> Result<Option<AccessRecoveryStatusDto>, String> {
    #[cfg(all(feature = "p2p", feature = "encryption"))]
    {
        use std::time::Duration;
        // Need the seal key to unseal prior shares before appending (RECOVERY-001).
        let seal_key = state.recovery_seal_key().await.ok_or_else(|| {
            "recovery session not open — resume with your new passphrase first".to_string()
        })?;
        let dir = access_dir();
        let Some(mut rec) = sovereign_p2p::access_recovery::AccessRecovery::load(&dir) else {
            return Ok(None);
        };
        // Decrypt shares gathered in earlier rounds so this round appends to them
        // rather than re-sealing over a truncated set.
        rec.unseal(&seal_key).str_err()?;
        rec.poll_round(Duration::from_secs(20)).await;
        rec.save(&dir, &seal_key).str_err()?;
        Ok(Some(status_of(&rec)))
    }
    #[cfg(not(all(feature = "p2p", feature = "encryption")))]
    {
        let _ = state;
        Ok(None)
    }
}

/// Current status without polling. `None` if no access recovery is in progress.
#[tauri::command]
pub async fn access_recovery_status() -> Result<Option<AccessRecoveryStatusDto>, String> {
    #[cfg(all(feature = "p2p", feature = "encryption"))]
    {
        Ok(sovereign_p2p::access_recovery::AccessRecovery::load(&access_dir())
            .as_ref()
            .map(status_of))
    }
    #[cfg(not(all(feature = "p2p", feature = "encryption")))]
    Ok(None)
}

/// Install step: with ≥threshold shares, reconstruct the Recovery Key, open the
/// bundle, and re-create the account under the new passphrase held in the
/// recovery session (set at `start`/`resume`). Overwrites the forgotten
/// `auth.store` (see module note) and logs the session in.
#[tauri::command]
pub async fn access_recovery_finalize(
    state: State<'_, AppState>,
) -> Result<AccessRecoveryStatusDto, String> {
    #[cfg(all(feature = "p2p", feature = "encryption"))]
    {
        use sovereign_p2p::access_recovery::{AccessRecovery, AccessRecoveryPhase};

        // The new passphrase + seal key were set at `start`/`resume` and held in
        // the in-memory session (RECOVERY-001) — no passphrase travels on this
        // call. A closed session means the wizard skipped start/resume.
        let seal_key = state.recovery_seal_key().await.ok_or_else(|| {
            "recovery session not open — resume with your new passphrase first".to_string()
        })?;
        let new_passphrase = state.recovery_new_passphrase().await.ok_or_else(|| {
            "recovery session not open — resume with your new passphrase first".to_string()
        })?;

        let dir = access_dir();
        let mut rec =
            AccessRecovery::load(&dir).ok_or_else(|| "no access recovery in progress".to_string())?;
        // Decrypt the gathered shares before reconstructing (open reads them).
        rec.unseal(&seal_key).str_err()?;
        if !rec.have_enough() {
            return Err(format!(
                "not enough guardian shares yet ({}/{})",
                rec.shares_collected(),
                rec.threshold
            ));
        }

        // Reconstruct → open the bundle → recovered account secrets.
        let (kek, account_key) = rec.open().map_err(|e| {
            rec.error = Some(e.to_string());
            rec.phase = AccessRecoveryPhase::Failed;
            let _ = rec.save(&dir, &seal_key);
            e.to_string()
        })?;

        // Verify-before-commit + install, via the ONE shared core in
        // sovereign-crypto (spec §Guardian Social Recovery, step 9). It verifies
        // the recovered KEK opens the on-disk content-key stores BEFORE writing
        // anything, then re-creates auth.store under the new passphrase; a KEK
        // that can't decrypt returns Err with the prior auth.store untouched, so
        // a failed recovery never bricks an otherwise-recoverable account. This
        // is the same check the shell's recover_and_install uses — one copy of
        // the brick-the-account ordering, not two (coord from-windows/0061).
        let crypto_dir = crate::setup::crypto_dir();
        let auth_store = sovereign_crypto::recovery_store::install_recovered_auth_store(
            &crypto_dir,
            &kek,
            &account_key,
            new_passphrase.as_bytes(),
        )
        .map_err(|e| {
            rec.error = Some(e.clone());
            rec.phase = AccessRecoveryPhase::Failed;
            let _ = rec.save(&dir, &seal_key);
            e
        })?;

        // Unlock: wires EncryptedGraphDB + installs keys. Synced content now
        // decrypts under the recovered KEK/AccountKey.
        crate::tauri_commands::auth::install_session(&state, &auth_store, new_passphrase.as_bytes())
            .await?;

        // Duress decoy pre-seed (sidechannel-001 parity with onboarding).
        {
            let mut duress_config = state.config.clone();
            duress_config.database.path = crate::setup::persona_db_path(
                &state.config,
                sovereign_core::auth::PersonaKind::Duress,
            );
            match crate::setup::create_db(&duress_config).await {
                Ok(ddb) => {
                    if let Err(e) = crate::duress::seed_duress_db(&ddb).await {
                        tracing::warn!("duress decoy pre-seed at access recovery failed: {e}");
                    }
                }
                Err(e) => tracing::warn!("duress decoy DB pre-create at access recovery failed: {e}"),
            }
        }

        rec.phase = AccessRecoveryPhase::Installed;
        let status = status_of(&rec);
        AccessRecovery::cancel(&dir); // done — clear the in-progress state
        state.clear_recovery_session().await; // drop the held passphrase + seal key
        Ok(status)
    }
    #[cfg(not(all(feature = "p2p", feature = "encryption")))]
    {
        let _ = state;
        Err("access recovery requires the p2p + encryption features".into())
    }
}

/// Abandon an in-progress access recovery (local only). Also drops any held
/// recovery session (passphrase + seal key).
#[tauri::command]
pub async fn cancel_access_recovery(state: State<'_, AppState>) -> Result<(), String> {
    #[cfg(all(feature = "p2p", feature = "encryption"))]
    {
        sovereign_p2p::access_recovery::AccessRecovery::cancel(&access_dir());
        state.clear_recovery_session().await;
    }
    #[cfg(not(all(feature = "p2p", feature = "encryption")))]
    {
        let _ = state;
    }
    Ok(())
}
