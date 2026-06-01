//! Tauri commands for the Phase 2 device-pairing flow.
//!
//! Existing-device side:
//!   - `generate_pair_qr` — produces a base64url QR string + 6-digit PIN.
//!     The PairPayload (salt + AccountKey + this device's PeerId/name)
//!     is stored in `state.pending_pairing` so the existing device can
//!     later confirm the new device on first contact (Phase 3 work).
//!   - `forget_paired_device` — removes a peer from the paired list.
//!   - `list_paired_devices`, `get_local_peer_id` — read-side helpers
//!     for the Settings panel.
//!
//! New-device side:
//!   - `consume_pair_qr_preview` — decrypts the QR with the user-typed
//!     PIN and returns metadata (source device name, expiry) so the
//!     wizard can show a confirmation screen *before* asking the user
//!     for a local passphrase.
//!   - `complete_onboarding_paired` — full handoff: takes the QR + PIN
//!     + a fresh local passphrase + the standard onboarding inputs,
//!     persists `~/.sovereign/crypto/{salt, device_id, auth.store}`
//!     with the imported AccountKey, and writes the onboarding_done
//!     marker. After this returns, the new device is fully unlocked
//!     under the same AccountKey as the existing one.

use serde::{Deserialize, Serialize};
use tauri::State;

use sovereign_crypto::account_key::AccountKey;
use sovereign_crypto::auth::AuthStore;
use sovereign_crypto::pair_payload::{
    self as pp, EncryptedPairPayload, PairPayload,
};

use crate::err::ToStringErr;
use crate::tauri_state::AppState;

#[derive(Serialize)]
pub struct GeneratePairQrResult {
    /// Base64url-encoded `EncryptedPairPayload` for QR display.
    pub qr_payload_b64: String,
    /// 6-digit PIN to display alongside the QR. The user reads this
    /// out-of-band (verbally or by typing) into the new device.
    pub pin: String,
    /// Unix milliseconds — the QR is valid until this point. Frontend
    /// should display a countdown and call `generate_pair_qr` again on
    /// expiry.
    pub expires_at: i64,
}

#[derive(Deserialize)]
pub struct ConsumePairQrInput {
    pub qr_payload_b64: String,
    pub pin: String,
}

#[derive(Serialize)]
pub struct PairPayloadPreviewDto {
    pub source_peer_id: String,
    pub source_device_name: String,
    pub issued_at: i64,
    pub expires_at: i64,
}

#[derive(Deserialize)]
pub struct CompletePairedOnboardingInput {
    pub qr_payload_b64: String,
    pub pin: String,
    /// New local passphrase for THIS device. Different from the
    /// existing device's passphrase is fine — it only protects the
    /// wrapping of the imported AccountKey on this device.
    pub password: String,
    pub duress_password: Option<String>,
    /// Standard onboarding inputs.
    pub nickname: Option<String>,
    pub bubble_style: Option<String>,
    pub canary_phrase: Option<String>,
    pub seed_sample_data: bool,
}

#[derive(Serialize)]
pub struct PairedDeviceDto {
    pub peer_id: String,
    pub device_name: String,
    pub paired_at: String,
}

/// Existing device → frontend: produce a QR + PIN that lets a new
/// device import this user's AccountKey. Stores the unencrypted payload
/// in `state.pending_pairing` for v0.0.6 hand-off confirmation.
#[tauri::command]
pub async fn generate_pair_qr(
    state: State<'_, AppState>,
) -> Result<GeneratePairQrResult, String> {
    let account_key = state
        .account_key()
        .await
        .ok_or_else(|| "pairing unavailable: account key not loaded".to_string())?;
    // Salt: read from disk (the user's MasterKey salt). Device_id and
    // peer_id are derived from local state.
    let salt = std::fs::read(
        crate::setup::crypto_dir().join("salt"),
    )
    .map_err(|e| format!("read salt: {e}"))?;

    // PeerId from the per-device libp2p identity key. Empty string in
    // builds without the p2p feature (the source device's PeerId is
    // ignored on the new device until v0.0.6 wires up bidirectional
    // pair confirmation).
    let source_peer_id = peer_id_from_state(&state).await;

    let source_device_name = state.config.p2p.device_name.clone();

    let payload = PairPayload::new(
        salt,
        *account_key.as_bytes(),
        source_peer_id,
        source_device_name,
        pp::PAIR_TTL_SECONDS,
    );

    let pin = pp::generate_pairing_code();
    let encrypted = payload.encrypt(&pin).str_err()?;
    let qr_payload_b64 = encrypted.encode().str_err()?;
    let expires_at = payload.expires_at;

    *state.pending_pairing.write().await = Some((payload, pin.clone()));

    Ok(GeneratePairQrResult {
        qr_payload_b64,
        pin,
        expires_at,
    })
}

/// New device: decrypt-only preview of a pairing QR. Returns metadata
/// for a confirmation screen ("you are about to pair with 'Alice's
/// laptop'"). Doesn't persist anything; that's `complete_onboarding_paired`.
#[tauri::command]
pub async fn consume_pair_qr_preview(
    input: ConsumePairQrInput,
) -> Result<PairPayloadPreviewDto, String> {
    let parsed = EncryptedPairPayload::decode(&input.qr_payload_b64).str_err()?;
    let payload = parsed.decrypt(&input.pin).str_err()?;
    Ok(PairPayloadPreviewDto {
        source_peer_id: payload.source_peer_id,
        source_device_name: payload.source_device_name,
        issued_at: payload.issued_at,
        expires_at: payload.expires_at,
    })
}

/// New device: full onboarding using an imported AccountKey. Persists
/// salt + device_id + auth.store under the user's local passphrase, and
/// writes the onboarding_done marker. After this returns Ok, the next
/// `validate_password` on the new device will unlock the imported
/// AccountKey and the user lands in a fully synced workspace.
///
/// Idempotency: refuses to run if `onboarding_done` already exists, to
/// avoid clobbering a working install.
#[tauri::command]
pub async fn complete_onboarding_paired(
    state: State<'_, AppState>,
    input: CompletePairedOnboardingInput,
) -> Result<(), String> {
    let profile_dir = &state.profile_dir;

    if profile_dir.join("onboarding_done").exists() {
        return Err("onboarding already complete on this device".to_string());
    }

    // 1. Decrypt the QR payload to get the imported AccountKey + salt.
    let parsed = EncryptedPairPayload::decode(&input.qr_payload_b64).str_err()?;
    let payload = parsed.decrypt(&input.pin).str_err()?;
    let imported_account_key = AccountKey::from_bytes(payload.account_key_bytes);

    // 2. Persist salt + device_id (load_or_create_device_id mints a fresh
    //    UUID for this device — different PeerId from the source).
    let crypto_dir = profile_dir.join("crypto");
    std::fs::create_dir_all(&crypto_dir).str_err()?;
    std::fs::write(crypto_dir.join("salt"), &payload.salt).str_err()?;
    let device_id = crate::setup::load_or_create_device_id().str_err()?;

    // 3. Build the AuthStore with the imported AccountKey wrapped under
    //    the new local passphrase's DeviceKey.
    let duress = input
        .duress_password
        .as_deref()
        .unwrap_or("duress-fallback-unused");
    let auth_store = AuthStore::create_with_imported_account_key(
        input.password.as_bytes(),
        duress.as_bytes(),
        &payload.salt,
        &device_id,
        &imported_account_key,
    )
    .str_err()?;
    auth_store
        .save(&crypto_dir.join("auth.store"))
        .str_err()?;

    // 4. Save user profile (nickname, bubble_style, theme — same as
    //    fresh onboarding).
    let mut profile = sovereign_core::profile::UserProfile::load(profile_dir)
        .unwrap_or_else(|_| sovereign_core::profile::UserProfile::default_new());
    if let Some(ref nick) = input.nickname {
        profile.nickname = Some(nick.clone());
    }
    if let Some(ref style) = input.bubble_style {
        profile.bubble_style =
            serde_json::from_str(&format!("\"{style}\"")).unwrap_or_default();
    }
    if let Ok(theme_guard) = state.theme.lock() {
        profile.theme = theme_guard.clone();
    }
    profile.save(profile_dir).str_err()?;

    // 5. Optional canary phrase — same shape as fresh onboarding.
    if let Some(ref phrase) = input.canary_phrase {
        if let Ok(auth_result) = auth_store.authenticate(input.password.as_bytes()) {
            let canary = sovereign_crypto::canary::CanaryStore::encrypt(
                phrase,
                auth_result.kek.as_bytes(),
            )
            .str_err()?;
            canary
                .save(&crypto_dir.join("canary.store"))
                .str_err()?;
        }
    }

    // 6. Don't seed sample data on a paired device — its real data
    //    arrives through sync. The flag is accepted for API symmetry
    //    but ignored.
    let _ = input.seed_sample_data;

    // 7. Persist a PairedDevice record for the source so the post-login
    //    P2P startup can auto-sync with it. The pair_key_b64 is left
    //    empty in v0.0.5 — wire-level encryption arrives in v0.0.5.x;
    //    until then, both sides derive the pair-transport key from the
    //    shared AccountKey on demand.
    #[cfg(feature = "p2p")]
    if !payload.source_peer_id.is_empty() {
        let paired_path = crypto_dir.join("paired_devices.json");
        let mut manager = if paired_path.exists() {
            sovereign_p2p::pairing::PairingManager::load(&paired_path)
                .unwrap_or_else(|_| sovereign_p2p::pairing::PairingManager::new(paired_path.clone()))
        } else {
            sovereign_p2p::pairing::PairingManager::new(paired_path.clone())
        };
        manager.add_device(sovereign_p2p::pairing::PairedDevice {
            peer_id: payload.source_peer_id.clone(),
            device_name: payload.source_device_name.clone(),
            pair_key_b64: String::new(),
            paired_at: chrono::Utc::now().to_rfc3339(),
        });
        if let Err(e) = manager.save() {
            tracing::warn!("Failed to persist paired_devices.json: {e}");
        }
    }

    // 8. Mark onboarding_done.
    std::fs::write(profile_dir.join("onboarding_done"), "1").str_err()?;
    tracing::info!(
        "Paired onboarding complete (source: {})",
        payload.source_device_name
    );
    Ok(())
}

/// List devices this device has paired with. Reads through the
/// `PairingManager` installed by the post-login P2P startup; returns
/// an empty list if the manager isn't loaded yet (encryption-only
/// builds without the p2p feature, or pre-login).
#[tauri::command]
pub async fn list_paired_devices(
    state: State<'_, AppState>,
) -> Result<Vec<PairedDeviceDto>, String> {
    state.require_unlocked().await?;
    #[cfg(feature = "p2p")]
    {
        if let Some(ref manager) = *state.pairing_manager.read().await {
            return Ok(manager
                .list_devices()
                .into_iter()
                .map(|d| PairedDeviceDto {
                    peer_id: d.peer_id.clone(),
                    device_name: d.device_name.clone(),
                    paired_at: d.paired_at.clone(),
                })
                .collect());
        }
    }
    let _ = &state;
    Ok(Vec::new())
}

/// Remove a paired device from this device's records.
#[tauri::command]
pub async fn forget_paired_device(
    state: State<'_, AppState>,
    peer_id: String,
) -> Result<(), String> {
    state.require_unlocked().await?;
    #[cfg(feature = "p2p")]
    {
        {
            let mut guard = state.pairing_manager.write().await;
            if let Some(manager) = guard.as_mut() {
                manager.remove_device(&peer_id);
                manager
                    .save()
                    .map_err(|e| format!("save paired_devices.json: {e}"))?;
            }
        } // drop the write guard before refreshing (which read-locks it)
          // P2P-001: push the shrunken allow-list to the running node so
          // the forgotten device can no longer sync this session.
        crate::sync_startup::refresh_paired_peers(&state).await;
        return Ok(());
    }
    #[allow(unreachable_code)]
    {
        let _ = (&state, &peer_id);
        Ok(())
    }
}

/// Trigger a sync with every paired peer. Frontend invokes this from
/// the document `visibilitychange` listener (when the window comes back
/// to the foreground) and from a manual "Sync now" button. Returns the
/// number of `StartSync` commands queued (0 if the P2P node isn't
/// running, or the user has no paired peers).
#[tauri::command]
pub async fn trigger_sync_now(state: State<'_, AppState>) -> Result<u32, String> {
    state.require_unlocked().await?;
    #[cfg(feature = "p2p")]
    {
        return Ok(crate::sync_startup::trigger_sync_for_all_paired(&state).await);
    }
    #[allow(unreachable_code)]
    {
        let _ = &state;
        Ok(0)
    }
}

/// Return this device's libp2p PeerId for display in the Settings
/// panel. Empty string if the p2p identity isn't loaded yet OR the
/// build doesn't include the p2p feature.
#[tauri::command]
pub async fn get_local_peer_id(state: State<'_, AppState>) -> Result<String, String> {
    state.require_unlocked().await?;
    Ok(peer_id_from_state(&state).await)
}

#[cfg(feature = "p2p")]
async fn peer_id_from_state(state: &AppState) -> String {
    match state.p2p_identity_key().await {
        Some(p2p_key) => match sovereign_p2p::identity::derive_keypair(&p2p_key) {
            Ok(kp) => kp.public().to_peer_id().to_string(),
            Err(_) => String::new(),
        },
        None => String::new(),
    }
}

#[cfg(not(feature = "p2p"))]
async fn peer_id_from_state(_state: &AppState) -> String {
    String::new()
}
