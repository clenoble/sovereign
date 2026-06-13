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
use sovereign_crypto::pair_payload as pp;
// `GraphDB` trait must be in scope for `resolve_sync_conflict_keep_mine`'s
// `state.db.update_document(...)` call (pre-existing build break in the p2p
// feature — pairing.rs uses explicit imports, not a glob, so the trait method
// was never resolvable here).
use sovereign_db::GraphDB;

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

/// Existing device → frontend: produce a pairing QR + code (P3.1).
///
/// The QR carries only a plaintext, short-lived `PairingOffer` (peer id,
/// dial hints, expiry) — no AccountKey, no salt, nothing to brute-force
/// offline. The secrets are released over the interactive handshake by
/// the P2P node, which this command arms with the offer + the Argon2id-
/// stretched handshake key.
#[tauri::command]
pub async fn generate_pair_qr(
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<GeneratePairQrResult, String> {
    crate::tauri_state::require_main_webview(&webview)?;
    #[cfg(feature = "p2p")]
    {
        let account_key = state
            .account_key()
            .await
            .ok_or_else(|| "pairing unavailable: account key not loaded".to_string())?;
        let cmd_tx = state.p2p_command_tx().await.ok_or_else(|| {
            "pairing requires the P2P node — enable sync in Settings first".to_string()
        })?;
        let source_peer_id = peer_id_from_state(&state).await;
        if source_peer_id.is_empty() {
            return Err("pairing unavailable: p2p identity not loaded".to_string());
        }
        let source_device_name = state.config.p2p.device_name.clone();
        // The MasterKey salt is released to the new device during the
        // handshake (it used to travel in the QR).
        let salt = std::fs::read(crate::setup::crypto_dir().join("salt"))
            .map_err(|e| format!("read salt: {e}"))?;
        // Concrete listen addrs collected by the event translator; the
        // new device falls back to mDNS discovery when empty.
        let addrs = state
            .p2p_listen_addrs
            .read()
            .map(|a| a.clone())
            .unwrap_or_default();

        let offer = sovereign_p2p::PairingOffer::new(
            source_peer_id,
            source_device_name.clone(),
            addrs,
            sovereign_p2p::pairing_offer::OFFER_TTL_SECONDS,
        );
        let code = pp::generate_pairing_code();

        // Argon2id stretch (~0.5 s) off the async runtime.
        let offer_for_kdf = offer.clone();
        let code_for_kdf = code.clone();
        let handshake_key = tauri::async_runtime::spawn_blocking(move || {
            sovereign_p2p::pairing_offer::derive_handshake_key(&code_for_kdf, &offer_for_kdf)
        })
        .await
        .map_err(|e| format!("kdf task: {e}"))?
        .str_err()?;

        cmd_tx
            .send(sovereign_p2p::P2pCommand::SetPairingOffer {
                offer: Box::new(sovereign_p2p::ActivePairingOffer::new(
                    offer.offer_id.clone(),
                    handshake_key,
                    offer.expires_at,
                    salt,
                    *account_key.as_bytes(),
                    source_device_name,
                )),
            })
            .await
            .map_err(|e| format!("arm pairing offer: {e}"))?;

        return Ok(GeneratePairQrResult {
            qr_payload_b64: offer.encode().str_err()?,
            pin: code,
            expires_at: offer.expires_at,
        });
    }
    #[allow(unreachable_code)]
    {
        let _ = &state;
        Err("pairing requires a build with the p2p feature".to_string())
    }
}

/// New device: decode-only preview of a pairing QR. Returns metadata
/// for a confirmation screen ("you are about to pair with 'Alice's
/// laptop'"). Doesn't persist anything; that's `complete_onboarding_paired`.
///
/// P3.1: the offer is plaintext, so no PIN is needed for the preview —
/// the typed code is only proven (online) during the handshake. The
/// `pin` input is accepted for API compatibility and ignored.
#[tauri::command]
pub async fn consume_pair_qr_preview(
    input: ConsumePairQrInput,
) -> Result<PairPayloadPreviewDto, String> {
    #[cfg(feature = "p2p")]
    {
        let _ = &input.pin;
        let offer = sovereign_p2p::PairingOffer::decode(&input.qr_payload_b64).str_err()?;
        return Ok(PairPayloadPreviewDto {
            source_peer_id: offer.source_peer_id,
            source_device_name: offer.source_device_name,
            issued_at: offer.issued_at,
            expires_at: offer.expires_at,
        });
    }
    #[allow(unreachable_code)]
    {
        let _ = &input;
        Err("pairing requires a build with the p2p feature".to_string())
    }
}

/// New device: full onboarding via the P3.1 interactive handshake.
/// Scans the offer, proves the typed pairing code online to the source
/// device, receives salt + AccountKey, derives + confirms this device's
/// final identity, then persists salt + device_id + auth.store under the
/// user's local passphrase and writes the onboarding_done marker. After
/// this returns Ok, the next `validate_password` unlocks the imported
/// AccountKey and the user lands in a fully synced workspace.
///
/// Idempotency: refuses to run if `onboarding_done` already exists, to
/// avoid clobbering a working install.
#[tauri::command]
pub async fn complete_onboarding_paired(
    state: State<'_, AppState>,
    input: CompletePairedOnboardingInput,
) -> Result<(), String> {
    #[cfg(feature = "p2p")]
    {
        use sovereign_crypto::device_key::DeviceKey;
        use sovereign_crypto::master_key::MasterKey;

        let profile_dir = &state.profile_dir;

        if profile_dir.join("onboarding_done").exists() {
            return Err("onboarding already complete on this device".to_string());
        }
        // IPC-001 (v0.0.7): also refuse if a credential store already exists —
        // mirror complete_onboarding's IPC-006 guard. `onboarding_done` is
        // written LAST during fresh onboarding (after auth.store), so guarding on
        // it alone leaves a window where auth.store exists but the marker does
        // not (a crash between the two writes, or a pre-login race). Without this
        // an unauthenticated IPC caller could OVERWRITE the victim's auth.store
        // below with an attacker-supplied QR / password / AccountKey — account
        // lockout or takeover.
        if profile_dir.join("crypto").join("auth.store").exists() {
            return Err("Already onboarded — log in instead.".to_string());
        }

        // 1. Decode the scanned offer (plaintext — the QR carries no
        //    secrets since P3.1) and mint this device's id up front.
        let offer = sovereign_p2p::PairingOffer::decode(&input.qr_payload_b64).str_err()?;
        let crypto_dir = profile_dir.join("crypto");
        std::fs::create_dir_all(&crypto_dir).str_err()?;
        let device_id = crate::setup::load_or_create_device_id().str_err()?;

        // 2. Interactive handshake: dial the source, prove the typed code
        //    (online, attempt-capped on the source side), receive salt +
        //    AccountKey sealed under the handshake key, then derive and
        //    confirm this device's FINAL identity — only possible once
        //    the salt is known, which is why it happens inside the
        //    callback. The DeviceKey is stashed for the steps below.
        let password = input.password.clone();
        let device_id_for_closure = device_id.clone();
        let device_name = state.config.p2p.device_name.clone();
        let mut device_key_stash: Option<DeviceKey> = None;
        let outcome = sovereign_p2p::pairing_client::pair_with_source(
            &offer,
            &input.pin,
            &device_name,
            |secrets| {
                let master = MasterKey::from_passphrase(password.as_bytes(), &secrets.salt)
                    .map_err(|e| format!("master key: {e}"))?;
                let dk = DeviceKey::derive(&master, &device_id_for_closure)
                    .map_err(|e| format!("device key: {e}"))?;
                let kp = sovereign_p2p::identity::derive_keypair(&dk)
                    .map_err(|e| format!("identity: {e}"))?;
                let peer_id = kp.public().to_peer_id().to_string();
                device_key_stash = Some(dk);
                Ok(peer_id)
            },
            std::time::Duration::from_secs(60),
        )
        .await
        .str_err()?;
        let device_key = device_key_stash
            .ok_or_else(|| "handshake finished without deriving an identity".to_string())?;
        let imported_account_key = AccountKey::from_bytes(outcome.secrets.account_key_bytes);

        // 3. Persist salt, then the AuthStore with the imported
        //    AccountKey wrapped under the new local passphrase's DeviceKey.
        std::fs::write(crypto_dir.join("salt"), &outcome.secrets.salt).str_err()?;
        let duress = input
            .duress_password
            .as_deref()
            .unwrap_or("duress-fallback-unused");
        let auth_store = AuthStore::create_with_imported_account_key(
            input.password.as_bytes(),
            duress.as_bytes(),
            &outcome.secrets.salt,
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

        // 7. Persist the source as a paired device WITH its per-pair key
        //    (P1.4) — the handshake gave us everything to compute it, so
        //    the store is encrypted from birth (no plaintext bootstrap).
        //    The source registered OUR final identity during PairComplete.
        let store_key = sovereign_p2p::pairing::derive_store_key(&device_key);
        let pair_key = imported_account_key
            .derive_pair_key(&outcome.final_peer_id, &offer.source_peer_id);
        let mut manager = sovereign_p2p::pairing::PairingManager::new(
            crypto_dir.join("paired_devices.json"),
        );
        manager.add_device(sovereign_p2p::pairing::PairedDevice::with_key(
            offer.source_peer_id.clone(),
            outcome.secrets.source_device_name.clone(),
            pair_key,
        ));
        if let Err(e) = manager.save(&store_key) {
            tracing::warn!("Failed to persist paired_devices.json: {e}");
        }

        // 8. Mark onboarding_done.
        std::fs::write(profile_dir.join("onboarding_done"), "1").str_err()?;
        tracing::info!(
            "Paired onboarding complete (source: {})",
            outcome.secrets.source_device_name
        );
        return Ok(());
    }
    #[allow(unreachable_code)]
    {
        let _ = (&state, &input);
        Err("pairing requires a build with the p2p feature".to_string())
    }
}

/// List devices this device has paired with. Reads through the
/// `PairingManager` installed by the post-login P2P startup; returns
/// an empty list if the manager isn't loaded yet (encryption-only
/// builds without the p2p feature, or pre-login).
#[tauri::command]
pub async fn list_paired_devices(
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<Vec<PairedDeviceDto>, String> {
    state.require_unlocked(&webview).await?;
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
    webview: tauri::Webview,
    state: State<'_, AppState>,
    peer_id: String,
) -> Result<(), String> {
    state.require_unlocked(&webview).await?;
    #[cfg(feature = "p2p")]
    {
        {
            // The store is encrypted under the DeviceKey-derived key
            // (P1.4) — it carries the per-pair sealing keys.
            let identity_key = state
                .p2p_identity_key()
                .await
                .ok_or_else(|| "p2p identity key not loaded".to_string())?;
            let store_key = sovereign_p2p::pairing::derive_store_key(&identity_key);
            let mut guard = state.pairing_manager.write().await;
            if let Some(manager) = guard.as_mut() {
                manager.remove_device(&peer_id);
                manager
                    .save(&store_key)
                    .map_err(|e| format!("save paired_devices.json: {e}"))?;
            }
        } // drop the write guard before refreshing (which read-locks it)
          // P2P-001 + P2P-005: push the shrunken allow-list and pair-key
          // map to the running node so the forgotten device can neither
          // sync nor be sealed for this session.
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
pub async fn trigger_sync_now(
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<u32, String> {
    state.require_unlocked(&webview).await?;
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

/// Disarm the active pairing offer (the user closed the pairing panel).
/// No-op when nothing is armed or P2P isn't running.
#[tauri::command]
pub async fn cancel_pairing(
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.require_unlocked(&webview).await?;
    #[cfg(feature = "p2p")]
    {
        if let Some(cmd_tx) = state.p2p_command_tx().await {
            let _ = cmd_tx
                .send(sovereign_p2p::P2pCommand::ClearPairingOffer)
                .await;
        }
        return Ok(());
    }
    #[allow(unreachable_code)]
    {
        let _ = &state;
        Ok(())
    }
}

#[derive(Serialize)]
pub struct P2pSettingsDto {
    /// Whether the build carries the p2p feature at all.
    pub available: bool,
    pub enabled: bool,
    pub device_name: String,
    pub enable_mdns: bool,
    pub wifi_only: bool,
    /// Whether the node is actually running this session.
    pub running: bool,
}

/// Read-only view of the P2P configuration for the Settings → Devices
/// tab. Editing still happens via config.toml — the config loader has a
/// fallback chain with no canonical write-back location yet (P3.2 gap).
#[tauri::command]
pub async fn get_p2p_settings(
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<P2pSettingsDto, String> {
    state.require_unlocked(&webview).await?;
    #[cfg(feature = "p2p")]
    {
        return Ok(P2pSettingsDto {
            available: true,
            enabled: state.config.p2p.enabled,
            device_name: state.config.p2p.device_name.clone(),
            enable_mdns: state.config.p2p.enable_mdns,
            wifi_only: state.config.p2p.wifi_only,
            running: state.p2p_command_tx().await.is_some(),
        });
    }
    #[allow(unreachable_code)]
    {
        let _ = &state;
        Ok(P2pSettingsDto {
            available: false,
            enabled: false,
            device_name: String::new(),
            enable_mdns: false,
            wifi_only: false,
            running: false,
        })
    }
}

/// Resolve a document sync conflict by keeping the local version: touch
/// the document (bumps `modified_at`, so this device wins the next
/// content-LWW round) and immediately trigger a sync so the peers
/// converge on it. "Keep theirs"/"keep both" need a fetch-document
/// protocol verb and are deferred (see the P2P completion plan).
#[tauri::command]
pub async fn resolve_sync_conflict_keep_mine(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    doc_id: String,
) -> Result<(), String> {
    state.require_unlocked(&webview).await?;
    // Touch: update with no field changes still bumps modified_at.
    state
        .db
        .update_document(&doc_id, None, None)
        .await
        .map_err(|e| format!("touch document: {e}"))?;
    #[cfg(feature = "p2p")]
    {
        crate::sync_startup::trigger_sync_for_all_paired(&state).await;
    }
    Ok(())
}

/// Return this device's libp2p PeerId for display in the Settings
/// panel. Empty string if the p2p identity isn't loaded yet OR the
/// build doesn't include the p2p feature.
#[tauri::command]
pub async fn get_local_peer_id(
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<String, String> {
    state.require_unlocked(&webview).await?;
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
