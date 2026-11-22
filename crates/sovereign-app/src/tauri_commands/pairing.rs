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
    /// v0.0.12 lot 1, P3: the second entry of each value typed twice; see
    /// `OnboardingData`. Checked before the claim and the handshake.
    #[serde(default)]
    pub password_confirm: Option<String>,
    pub duress_password: Option<String>,
    #[serde(default)]
    pub duress_confirm: Option<String>,
    /// Standard onboarding inputs.
    pub nickname: Option<String>,
    pub bubble_style: Option<String>,
    pub canary_phrase: Option<String>,
    #[serde(default)]
    pub canary_confirm: Option<String>,
    pub seed_sample_data: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct PairedDeviceDto {
    pub peer_id: String,
    pub device_name: String,
    pub paired_at: String,
}

/// PR2 (spec ch. 05; v0.0.13, item C3): the source side's question when a
/// device joins under a name a paired device already has: « is it that
/// device, set up again? ». `candidates` are the paired devices of that name,
/// other than the one that just joined; she picks one (« Replace it ») or
/// keeps all (« Keep both »).
#[derive(Serialize, Clone, Debug)]
pub struct SameNameQuestionDto {
    pub new_peer_id: String,
    pub new_device_name: String,
    pub candidates: Vec<PairedDeviceDto>,
}

/// PR2: the question the source side holds, if any (see [`SameNameQuestionDto`]).
#[cfg(feature = "p2p")]
pub(crate) fn same_name_question_in(state: &AppState) -> Option<SameNameQuestionDto> {
    state.same_name.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// PR2: the question to hold after `peer` joined under `name`: the paired
/// devices other than it that hold that name (the shared lookup,
/// `PairingManager::same_name`), oldest pairing first; `None` when there are
/// none.
#[cfg(feature = "p2p")]
pub(crate) fn same_name_question_for(
    manager: &sovereign_p2p::pairing::PairingManager,
    peer: &str,
    name: &str,
) -> Option<SameNameQuestionDto> {
    let candidates: Vec<PairedDeviceDto> = manager
        .same_name(name, peer)
        .into_iter()
        .map(|d| PairedDeviceDto {
            peer_id: d.peer_id.clone(),
            device_name: d.device_name.clone(),
            paired_at: d.paired_at.clone(),
        })
        .collect();
    if candidates.is_empty() {
        return None;
    }
    Some(SameNameQuestionDto {
        new_peer_id: peer.to_string(),
        new_device_name: name.to_string(),
        candidates,
    })
}

/// PR1 and PR2: her answer to the held question. `Some(peer_id)`, « Replace
/// it »: that paired device's record goes, as at unpairing (the store, the
/// saved file, the allow-list and the sealing map pushed to the node).
/// `None`, « Keep both »: every record stays. Either way the question is
/// answered. Nothing is removed that the question did not ask about.
///
/// A « Replace it » naming a device the question does not list is refused and
/// the question stays held, as on the desktop; with no question held it is
/// refused too (the screen asked about a question that is gone, for instance
/// after a session switch). A « Keep both » with no question held answers
/// nothing and is not an error: closing the panel may answer twice.
#[cfg(feature = "p2p")]
pub(crate) async fn answer_same_name_in(state: &AppState, replace: Option<String>) -> Result<(), String> {
    let question = {
        let mut held = state.same_name.lock().unwrap_or_else(|e| e.into_inner());
        match (held.as_ref(), replace.as_deref()) {
            (None, None) => return Ok(()),
            (None, Some(_)) => {
                return Err("No question about a device paired again is waiting; nothing was removed.".into())
            }
            (Some(q), Some(peer)) if !q.candidates.iter().any(|c| c.peer_id == peer) => {
                return Err(format!(
                    "{peer} is not one of the devices the question asks about; nothing was removed."
                ))
            }
            _ => held.take(),
        }
    };
    let Some(peer) = replace else { return Ok(()) };
    if let Err(e) = forget_paired_device_in(state, &peer).await {
        // The record may still be there: the question stays, so she can
        // answer again, unless a newer pairing has asked its own meanwhile.
        let mut held = state.same_name.lock().unwrap_or_else(|e| e.into_inner());
        if held.is_none() {
            *held = question;
        }
        return Err(e);
    }
    Ok(())
}

/// PR2: the held question ([`same_name_question_in`]), asked by the screen
/// once a pairing is reported (`device-paired`).
#[cfg(feature = "p2p")]
#[tauri::command]
pub async fn same_name_question(
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<Option<SameNameQuestionDto>, String> {
    state.require_unlocked(&webview).await?;
    Ok(same_name_question_in(&state))
}

/// PR1 and PR2: her answer ([`answer_same_name_in`]).
#[cfg(feature = "p2p")]
#[tauri::command]
pub async fn answer_same_name(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    replace: Option<String>,
) -> Result<(), String> {
    state.require_unlocked(&webview).await?;
    answer_same_name_in(&state, replace).await
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
    generate_pair_qr_in(&state).await
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
/// final identity, records the source device, then persists auth.store +
/// salt under the user's local passphrase and writes the onboarding_done
/// marker. After this returns Ok, the next `validate_password` unlocks the
/// imported AccountKey and the user lands in a fully synced workspace.
///
/// v0.0.12 lot 1: the confirmations and duress ≠ password are checked before
/// anything (P3, P4); the source's record is written before auth.store and
/// its failure fails the setup (P2c); once auth.store is written a failed
/// step reports the setup `incomplete` instead of an error (P2b).
///
/// Idempotency: the claim refuses once `auth.store` exists ("Already
/// onboarded — log in instead"), so a working install is never clobbered.
#[tauri::command]
pub async fn complete_onboarding_paired(
    state: State<'_, AppState>,
    input: CompletePairedOnboardingInput,
) -> Result<crate::tauri_commands::OnboardingOutcome, String> {
    #[cfg(feature = "p2p")]
    {
        use sovereign_crypto::device_key::DeviceKey;
        use sovereign_crypto::master_key::MasterKey;

        let profile_dir = &state.profile_dir;

        // v0.0.12 lot 1, P3/P4: before the claim, the handshake, and any
        // derivation or write.
        crate::tauri_commands::auth::check_typed_twice(
            Some(input.password.as_str()),
            input.password_confirm.as_deref(),
            input.duress_password.as_deref(),
            input.duress_confirm.as_deref(),
            input.canary_phrase.as_deref(),
            input.canary_confirm.as_deref(),
        )?;

        // rc2 (spec ch. 07, *Auth store*, "Created once"): the single-flight
        // claim, taken first — before the pairing handshake (which derives the
        // master key in its callback) and before any file write — and held to
        // the end of this command, past the write of auth.store. It also
        // carries IPC-001 (v0.0.7): an existing credential store refuses, even
        // without the `onboarding_done` marker (written last, so a crash or a
        // pre-login race can leave auth.store without it); without that an
        // unauthenticated IPC caller could OVERWRITE the victim's auth.store
        // with an attacker-supplied QR / password / AccountKey.
        //
        // v0.0.12 lot 1, P1: the claim is the only gate. The former refusal
        // on an existing `onboarding_done` marker is gone: without
        // `auth.store` the app now shows the wizard whatever the marker says,
        // and a stale marker must not block the pairing it offers.
        let _claim = crate::onboarding_claim::claim(profile_dir).map_err(|e| e.to_string())?;

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
                // Derive THIS device's identity with the SAME KDF login will use —
                // Argon2id (Kdf::current), matching create_with_imported_account_key
                // + AuthStore::authenticate below. The legacy HKDF `from_passphrase`
                // makes the handshake PeerId + paired-store key diverge from the
                // login-time ones: paired_devices.json then fails to decrypt after
                // login (AEAD error → empty allow-list → no sync) AND the source
                // registers the wrong PeerId for us. (The native shell already does
                // this correctly — see sovereign-shell/src/p2p.rs.)
                let master = MasterKey::derive(
                    password.as_bytes(),
                    &secrets.salt,
                    &sovereign_crypto::master_key::Kdf::current(),
                )
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

        // 3. Persist the source as a paired device WITH its per-pair key
        //    (P1.4) — the handshake gave us everything to compute it, so
        //    the store is encrypted from birth (no plaintext bootstrap).
        //    The source registered OUR final identity during PairComplete.
        //    v0.0.12 lot 1, P2c: written BEFORE auth.store, and its failure
        //    fails the setup (auth.store not written yet), so an account that
        //    opens is one that knows its source. A fresh manager replaces any
        //    record a previous attempt left (`add_device` keys by peer id; the
        //    file is rewritten whole), so pairing again after a failure leaves
        //    only the new record.
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
        manager
            .save(&store_key)
            .map_err(|e| format!("Could not record the source device: {e}"))?;

        // 4. Persist the AuthStore with the imported AccountKey wrapped
        //    under the new local passphrase's DeviceKey, then the salt (in
        //    that order, so a refused exclusive write changes no account
        //    file). Duress optional → RANDOM unreachable decoy, never the
        //    shared source-visible literal (H-shell1 theme; matches the
        //    wizard); an empty duress password counts as none.
        let random_duress = sovereign_crypto::random_hex_32();
        let duress = input
            .duress_password
            .as_deref()
            .filter(|d| !d.is_empty())
            .unwrap_or(&random_duress);
        let auth_store = AuthStore::create_with_imported_account_key(
            input.password.as_bytes(),
            duress.as_bytes(),
            &outcome.secrets.salt,
            &device_id,
            &imported_account_key,
        )
        .str_err()?;
        // Exclusive create: never replaces an existing auth.store. The mode
        // (hard link or in-place) is logged, to observe it on the device.
        let mode = auth_store
            .save_new(&crypto_dir.join("auth.store"))
            .map_err(crate::onboarding_claim::save_new_error)?;
        tracing::info!("auth.store published (paired onboarding): {mode:?}");

        // P2b: the account exists from here on; a failed step makes the
        // setup incomplete (logged), and the marker is then not written.
        fn step(failed: &mut Vec<&'static str>, name: &'static str, result: Result<(), String>) {
            if let Err(e) = result {
                tracing::warn!("paired setup step '{name}' failed after auth.store was written (setup incomplete): {e}");
                failed.push(name);
            }
        }
        let mut failed: Vec<&'static str> = Vec::new();

        step(
            &mut failed,
            "salt",
            sovereign_crypto::fs_private::write_private(&crypto_dir.join("salt"), &outcome.secrets.salt)
                .str_err(),
        );

        // 5. Save user profile (nickname, bubble_style, theme — same as
        //    fresh onboarding).
        let mut profile = sovereign_core::profile::UserProfile::load(profile_dir)
            .unwrap_or_else(|_| sovereign_core::profile::UserProfile::default_new());
        if let Some(ref nick) = input.nickname {
            profile.nickname = Some(nick.clone());
        }
        step(&mut failed, "profile", profile.save(profile_dir).str_err());
        // C-AREA AR2: the theme and bubble style in the device preferences;
        // the duress persona's own identity profile, drawn at random.
        step(
            &mut failed,
            "device preferences",
            crate::tauri_commands::auth::save_wizard_preferences(&state, input.bubble_style.as_deref()).str_err(),
        );
        step(&mut failed, "duress profile", crate::tauri_commands::auth::create_duress_identity(profile_dir));

        // 6. Optional canary phrase — same shape as fresh onboarding. A failed
        //    authentication is a failed step (before, it was skipped silently).
        if let Some(ref phrase) = input.canary_phrase {
            // The authentication's failure says what failed, in the login's
            // words (spec ch. 07, login errors, LE2, LE3).
            let saved = auth_store
                .authenticate(input.password.as_bytes())
                .map_err(|e| {
                    sovereign_crypto::login_error::LoginFailure::from_authenticate(
                        &e,
                        &[sovereign_core::sovereign_dir().as_path(), profile_dir.as_path()],
                    )
                    .to_string()
                })
                .and_then(|auth_result| {
                    sovereign_crypto::canary::CanaryStore::encrypt(phrase, auth_result.kek.as_bytes())
                        .and_then(|canary| canary.save(&crypto_dir.join("canary.store")))
                        .str_err()
                });
            step(&mut failed, "safety phrase", saved);
        }

        // 7. Don't seed sample data on a paired device — its real data
        //    arrives through sync. The flag is accepted for API symmetry
        //    but ignored.
        let _ = input.seed_sample_data;

        // 8. Mark onboarding_done — only when every step succeeded (P2: the
        //    marker's absence beside auth.store is what the first login's
        //    notice reads, and that login writes it).
        if failed.is_empty() {
            let marker = std::fs::write(profile_dir.join("onboarding_done"), "1").str_err();
            step(&mut failed, "marker", marker);
        }
        let incomplete = !failed.is_empty();
        tracing::info!(
            "Paired onboarding {} (source: {})",
            if incomplete { "incomplete" } else { "complete" },
            outcome.secrets.source_device_name
        );
        return Ok(crate::tauri_commands::OnboardingOutcome { incomplete });
    }
    #[allow(unreachable_code)]
    {
        let _ = (&state, &input);
        Err("pairing requires a build with the p2p feature".to_string())
    }
}

/// The behaviour under `generate_pair_qr`, on the state alone (C-AREA AR3;
/// claude-coord from-windows/0241 (a), 0242): arms a pairing offer for the
/// unlocked session and returns it.
///
/// **In a duress session, the decoy** (C-AREA AR3, AR4; Céline 2026-11-13:
/// « Le même leurre que les gardiens »): an offer of the real form, from the
/// session's own peer id, and a code from the real generator, after the same
/// key stretch, but nothing is armed and no node runs: a device that tries it
/// gets no answer, and the screen waits as for a device that never comes.
///
/// **Over a paired list that could not be read**, no offer: a pairing would
/// have to replace the list, which is kept as it is (C-AREA AR3).
pub(crate) async fn generate_pair_qr_in(state: &AppState) -> Result<GeneratePairQrResult, String> {
    #[cfg(feature = "p2p")]
    {
        let account_key = state
            .account_key()
            .await
            .ok_or_else(|| "pairing unavailable: account key not loaded".to_string())?;
        if state.is_duress_session() {
            return decoy_pair_qr(state).await;
        }
        // C-SUP, DEL-0019: while sync is stopped, the stop's text.
        let cmd_tx = state.p2p_command_tx().await.ok_or_else(|| {
            state.node_refusal("pairing requires the P2P node — enable sync in Settings first")
        })?;
        if let Some(why) = state.pairing_manager.read().await.as_ref().and_then(|m| m.unreadable().map(str::to_string)) {
            return Err(format!(
                "Your paired devices could not be read ({why}). The list is kept as it is, and a new pairing would replace it."
            ));
        }
        let source_peer_id = peer_id_from_state(state).await;
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
        //
        // Filter out loopback + unspecified addresses before advertising
        // them: they are never reachable by a *remote* joiner, and a
        // loopback hint (/ip4/127.0.0.1/…) is actively harmful — a joiner
        // that "tries every address" would dial ITSELF on that port instead
        // of this device. Routable LAN addresses (192.168.x, 10.x, etc.) are
        // kept; the joiner races them and uses whichever connects first.
        let addrs = state
            .p2p_listen_addrs
            .read()
            .map(|a| a.clone())
            .unwrap_or_default()
            .into_iter()
            .filter(|a| sovereign_p2p::is_routable_listen_addr(a))
            .collect::<Vec<_>>();

        let offer = sovereign_p2p::PairingOffer::new(
            source_peer_id,
            source_device_name.clone(),
            addrs,
            sovereign_p2p::pairing_offer::OFFER_TTL_SECONDS,
        );
        let code = pp::generate_pairing_code();
        let handshake_key = stretch(&offer, &code).await?;

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
        let _ = state;
        Err("pairing requires a build with the p2p feature".to_string())
    }
}

/// The handshake key of `offer` under `code`: the Argon2id stretch (~0.5 s),
/// off the async runtime.
#[cfg(feature = "p2p")]
async fn stretch(offer: &sovereign_p2p::PairingOffer, code: &str) -> Result<[u8; 32], String> {
    let offer_for_kdf = offer.clone();
    let code_for_kdf = code.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        sovereign_p2p::pairing_offer::derive_handshake_key(&code_for_kdf, &offer_for_kdf)
    })
    .await
    .map_err(|e| format!("kdf task: {e}"))?
    .str_err()
}

/// The duress session's pairing screen (see [`generate_pair_qr_in`]): built
/// as a real offer is, from this session's own peer id and the device's name,
/// with the same key stretch so it takes as long, but armed nowhere. No node
/// runs in a duress session, so it carries no address.
#[cfg(feature = "p2p")]
async fn decoy_pair_qr(state: &AppState) -> Result<GeneratePairQrResult, String> {
    let source_peer_id = peer_id_from_state(state).await;
    if source_peer_id.is_empty() {
        return Err("pairing unavailable: p2p identity not loaded".to_string());
    }
    let offer = sovereign_p2p::PairingOffer::new(
        source_peer_id,
        state.config.p2p.device_name.clone(),
        Vec::new(),
        sovereign_p2p::pairing_offer::OFFER_TTL_SECONDS,
    );
    let code = pp::generate_pairing_code();
    let _ = stretch(&offer, &code).await?;
    Ok(GeneratePairQrResult {
        qr_payload_b64: offer.encode().str_err()?,
        pin: code,
        expires_at: offer.expires_at,
    })
}

/// The behaviour under `list_paired_devices`, on the state alone (C-AREA
/// AR3): the session's paired devices; over a list that cannot be read, an
/// `Err` naming the failure, never an empty list. Reads through the
/// `PairingManager` installed by the post-login P2P startup; an empty list
/// when none is loaded (a build without the p2p feature, before login, or a
/// duress session, which runs no node and has paired nothing).
pub(crate) async fn list_paired_devices_in(state: &AppState) -> Result<Vec<PairedDeviceDto>, String> {
    #[cfg(feature = "p2p")]
    {
        if let Some(ref manager) = *state.pairing_manager.read().await {
            if let Some(why) = manager.unreadable() {
                return Err(format!(
                    "Your paired devices could not be read ({why}). The list is kept as it is."
                ));
            }
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
    let _ = state;
    Ok(Vec::new())
}

/// List devices this device has paired with ([`list_paired_devices_in`]).
#[tauri::command]
pub async fn list_paired_devices(
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<Vec<PairedDeviceDto>, String> {
    state.require_unlocked(&webview).await?;
    list_paired_devices_in(&state).await
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
        return forget_paired_device_in(&state, &peer_id).await;
    }
    #[allow(unreachable_code)]
    {
        let _ = (&state, &peer_id);
        Ok(())
    }
}

/// Unpairing, on the state alone: the device's record leaves the store and
/// the saved file, and the trimmed allow-list and sealing map are pushed to
/// the running node. Shared by « Forget » and by « Replace it » (PR1), so a
/// replaced pairing goes exactly as a forgotten one does.
#[cfg(feature = "p2p")]
pub(crate) async fn forget_paired_device_in(state: &AppState, peer_id: &str) -> Result<(), String> {
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
            manager.remove_device(peer_id);
            manager
                .save(&store_key)
                .map_err(|e| format!("save paired_devices.json: {e}"))?;
        }
    } // drop the write guard before refreshing (which read-locks it)
      // P2P-001 + P2P-005: push the shrunken allow-list and pair-key
      // map to the running node so the forgotten device can neither
      // sync nor be sealed for this session.
    crate::sync_startup::refresh_paired_peers(state).await;
    Ok(())
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

/// C-SUP SU3: the gesture « Restart sync »: a fresh node with the open
/// session's keys, by the login's own start. Never in a duress session (SU4,
/// C-AREA AR4): there it starts nothing. A start that fails returns its
/// reason (« Sync could not be restarted: {reason} » on the screens) and
/// leaves sync stopped (DEL-0019).
#[tauri::command]
pub async fn restart_sync(webview: tauri::Webview, state: State<'_, AppState>) -> Result<(), String> {
    state.require_unlocked(&webview).await?;
    if state.is_duress_session() {
        return Ok(());
    }
    #[cfg(feature = "p2p")]
    {
        crate::sync_startup::stop_p2p_node(&state).await;
        let started = crate::sync_startup::start_p2p_node(&state).await;
        if started.is_err() {
            crate::sync_startup::keep_sync_stopped(&state.profile_dir);
        }
        return started;
    }
    #[allow(unreachable_code)]
    {
        Err("sync is not built into this app".to_string())
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
    /// True while the node runs without the DNS transport (no system
    /// resolver config, as on Android) — v0.0.12 item 12, P4: Settings →
    /// Devices says the phone cannot look up names: it syncs on the local
    /// network and with addresses given by IP.
    pub without_dns: bool,
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
            without_dns: state.p2p_without_dns().await,
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
            without_dns: false,
        })
    }
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
pub(crate) async fn peer_id_from_state(state: &AppState) -> String {
    match state.p2p_identity_key().await {
        Some(p2p_key) => match sovereign_p2p::identity::derive_keypair(&p2p_key) {
            Ok(kp) => kp.public().to_peer_id().to_string(),
            Err(_) => String::new(),
        },
        None => String::new(),
    }
}

#[cfg(not(feature = "p2p"))]
pub(crate) async fn peer_id_from_state(_state: &AppState) -> String {
    String::new()
}
