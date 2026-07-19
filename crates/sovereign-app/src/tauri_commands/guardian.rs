//! Feature 1 — owner-side guardian enrollment commands.
//!
//! Canonical design: `doc/spec/sovereign_os_specification.md` §Guardian Social
//! Recovery. `begin_guardian_enrollment` arms an in-person QR that hands the
//! **next Recovery-Key share** to a guardian (the first call generates the
//! Recovery Key + bundle + 5-slot roster). `list_guardians` reports progress —
//! recovery is armed only at 5 enrolled.

use crate::tauri_state::AppState;
use serde::Serialize;
use tauri::State;

#[cfg(all(feature = "p2p", feature = "encryption"))]
use crate::err::ToStringErr;

/// Say what reconciliation could not apply.
///
/// `sovereign-crypto` has no logging surface by design (see its crate docs), so
/// it returns what it could not fold in rather than logging it. This is the
/// app-side face of that contract. A retained entry means a queued enrollment
/// was unreadable and has been **kept** for review — a guardian may believe
/// they are enrolled while the roster does not list them, which otherwise
/// surfaces only at recovery time. That is a warning, not a debug line.
#[cfg(all(feature = "p2p", feature = "encryption"))]
fn report_reconciled(r: sovereign_crypto::recovery_store::Reconciled) {
    if r.needs_attention() {
        tracing::warn!(
            "guardian roster: {} unreadable pending enrollment(s) kept for review \
             — a guardian may hold a shard the roster does not list: {}",
            r.retained,
            r.skipped.join("; ")
        );
    } else if !r.skipped.is_empty() {
        tracing::debug!("guardian roster: {}", r.skipped.join("; "));
    }
}

#[derive(Serialize)]
pub struct BeginEnrollmentResult {
    /// QR the guardian scans (base64url of the offer).
    pub qr_payload_b64: String,
    /// Short code the owner reads aloud in person.
    pub code: String,
    /// The share/slot this offer hands out.
    pub shard_id: String,
    pub enrolled_count: u8,
    pub total: u8,
}

#[derive(Serialize)]
pub struct GuardianSlotDto {
    pub label: Option<String>,
    pub enrolled: bool,
    pub enrolled_at: Option<String>,
}

#[derive(Serialize)]
pub struct GuardianRosterDto {
    pub enrolled_count: u8,
    pub total: u8,
    /// True only when all 5 are enrolled (recovery becomes usable).
    pub armed: bool,
    pub guardians: Vec<GuardianSlotDto>,
}

/// Arm an in-person guardian-enrollment offer handing out the next
/// Recovery-Key share. First call generates the Recovery Key + bundle + roster.
#[tauri::command]
pub async fn begin_guardian_enrollment(
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<BeginEnrollmentResult, String> {
    state.require_unlocked(&webview).await?;
    #[cfg(all(feature = "p2p", feature = "encryption"))]
    {
        use sovereign_crypto::recovery_roster::{GUARDIAN_THRESHOLD, GUARDIAN_TOTAL};

        let kek = state
            .kek()
            .await
            .ok_or_else(|| "recovery setup unavailable: KEK not loaded".to_string())?;
        let account_key = state
            .account_key()
            .await
            .ok_or_else(|| "recovery setup unavailable: account key not loaded".to_string())?;
        let cmd_tx = state.p2p_command_tx().await.ok_or_else(|| {
            "guardian enrollment requires the P2P node — enable sync in Settings first".to_string()
        })?;
        let owner_peer_id = crate::tauri_commands::pairing::peer_id_from_state(&state).await;
        if owner_peer_id.is_empty() {
            return Err("guardian enrollment unavailable: p2p identity not loaded".into());
        }

        // Load-or-create the roster, folding in any queued enrollments first.
        let store = crate::setup::recovery_store();
        let mut setup = store.load_or_create(&kek, &account_key)?;
        report_reconciled(store.reconcile_pending(&mut setup, &kek)?);
        // Refresh the pre-login recovery card from the current roster.
        if let Err(e) = store.write_recovery_card(
            &setup,
            &account_key.derive_backup_tag(),
            &state.config.p2p.seed_relays,
        ) {
            tracing::warn!("recovery card refresh failed (continuing): {e}");
        }

        if setup.is_armed() {
            return Err("Recovery is already set up — all 5 guardians are enrolled.".into());
        }
        let (shard_id, share_b64) = {
            let slot = setup
                .next_pending_slot()
                .ok_or_else(|| "no share left to hand out".to_string())?;
            (
                slot.shard_id.clone(),
                slot.pending_share_b64
                    .clone()
                    .ok_or_else(|| "slot has no pending share".to_string())?,
            )
        };
        let enrolled_count = setup.enrolled_count() as u8;

        // T_g: guardians are enrolled under the guardian tag (spec).
        let owner_tag = account_key.derive_backup_tag();
        let owner_label = state.config.p2p.device_name.clone();
        let addrs = state
            .p2p_listen_addrs
            .read()
            .map(|a| a.clone())
            .unwrap_or_default()
            .into_iter()
            .filter(|a| sovereign_p2p::is_routable_listen_addr(a))
            .collect::<Vec<_>>();

        let offer = sovereign_p2p::guardian_enroll::GuardianEnrollOffer::new(
            owner_peer_id,
            owner_label.clone(),
            addrs,
            sovereign_p2p::guardian_enroll::GUARDIAN_OFFER_TTL_SECONDS,
        );
        let code = sovereign_crypto::pair_payload::generate_pairing_code();

        // Argon2id stretch (~0.5 s) off the async runtime.
        let offer_for_kdf = offer.clone();
        let code_for_kdf = code.clone();
        let handshake_key = tauri::async_runtime::spawn_blocking(move || {
            sovereign_p2p::guardian_enroll::derive_enroll_key(&code_for_kdf, &offer_for_kdf)
        })
        .await
        .map_err(|e| format!("kdf task: {e}"))?
        .str_err()?;

        cmd_tx
            .send(sovereign_p2p::P2pCommand::SetGuardianOffer {
                offer: Box::new(sovereign_p2p::ActiveGuardianOffer::new(
                    offer.offer_id.clone(),
                    handshake_key,
                    offer.expires_at,
                    share_b64,
                    shard_id.clone(),
                    owner_tag,
                    owner_label,
                    setup.epoch,
                    GUARDIAN_THRESHOLD,
                    GUARDIAN_TOTAL as u8,
                )),
            })
            .await
            .map_err(|e| format!("arm guardian offer: {e}"))?;

        Ok(BeginEnrollmentResult {
            qr_payload_b64: offer.encode().str_err()?,
            code,
            shard_id,
            enrolled_count,
            total: GUARDIAN_TOTAL as u8,
        })
    }
    #[cfg(not(all(feature = "p2p", feature = "encryption")))]
    {
        let _ = (webview, state);
        Err("guardian enrollment requires the p2p + encryption features".into())
    }
}

/// The owner's guardian roster (X/5, armed only at 5). Empty before setup.
#[tauri::command]
pub async fn list_guardians(
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<GuardianRosterDto, String> {
    state.require_unlocked(&webview).await?;
    #[cfg(all(feature = "p2p", feature = "encryption"))]
    {
        use sovereign_crypto::recovery_roster::GUARDIAN_TOTAL;

        let kek = state
            .kek()
            .await
            .ok_or_else(|| "guardian roster unavailable: KEK not loaded".to_string())?;
        let store = crate::setup::recovery_store();
        let setup = match store.load(&kek)? {
            Some(mut s) => {
                report_reconciled(store.reconcile_pending(&mut s, &kek)?);
                if let Some(ak) = state.account_key().await {
                    let _ = store.write_recovery_card(
                        &s,
                        &ak.derive_backup_tag(),
                        &state.config.p2p.seed_relays,
                    );
                }
                s
            }
            None => {
                return Ok(GuardianRosterDto {
                    enrolled_count: 0,
                    total: GUARDIAN_TOTAL as u8,
                    armed: false,
                    guardians: vec![],
                });
            }
        };
        let guardians = setup
            .slots
            .iter()
            .map(|sl| GuardianSlotDto {
                label: sl.label.clone(),
                enrolled: sl.is_enrolled(),
                enrolled_at: sl.enrolled_at.clone(),
            })
            .collect();
        Ok(GuardianRosterDto {
            enrolled_count: setup.enrolled_count() as u8,
            total: GUARDIAN_TOTAL as u8,
            armed: setup.is_armed(),
            guardians,
        })
    }
    #[cfg(not(all(feature = "p2p", feature = "encryption")))]
    {
        let _ = (webview, state);
        Err("guardian roster requires the p2p + encryption features".into())
    }
}
