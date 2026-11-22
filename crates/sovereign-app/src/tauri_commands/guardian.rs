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

/// The behaviour under `begin_guardian_enrollment`, on the state alone, as
/// `pairing::generate_pair_qr_in` is under `generate_pair_qr` (the command
/// keeps its main-webview check and calls this):
/// - in a primary session, the real offer, armed on the node as today;
/// - **in a duress session, the decoy** (ch. 07, *After login*, Céline
///   2026-10-17: guardian enrollment « appears to work, waits for the guardian,
///   and never completes »; AR4: « Guardian enrollment keeps its decoy waiting
///   screen »): an offer of the real form, through the same key stretch as
///   the real one ([`stretch_enroll_key`]), armed nowhere, with nothing read
///   from or written to the recovery store.
///
/// Signature by the C-AREA test author (`l1c-tests-v10`; claude-coord
/// from-fable-2nde-position/0062, from-windows/0302, from-laptop/0285); its
/// body is the implementer's.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) async fn begin_guardian_enrollment_in(state: &AppState) -> Result<BeginEnrollmentResult, String> {
    #[cfg(all(feature = "p2p", feature = "encryption"))]
    {
        use sovereign_crypto::recovery_roster::{GUARDIAN_THRESHOLD, GUARDIAN_TOTAL};

        state.require_session_unlocked().await?;
        // C-AREA AR4: a duress session gets the decoy, before anything of the
        // real enrollment (its KEK, the node, the recovery store) is asked for.
        if state.is_duress_session() {
            return decoy_guardian_offer(state).await;
        }
        let kek = state
            .kek()
            .await
            .ok_or_else(|| "recovery setup unavailable: KEK not loaded".to_string())?;
        let account_key = state
            .account_key()
            .await
            .ok_or_else(|| "recovery setup unavailable: account key not loaded".to_string())?;
        // C-SUP, DEL-0019: while sync is stopped, the stop's text.
        let cmd_tx = state.p2p_command_tx().await.ok_or_else(|| {
            state.node_refusal("guardian enrollment requires the P2P node — enable sync in Settings first")
        })?;
        let owner_peer_id = crate::tauri_commands::pairing::peer_id_from_state(state).await;
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

        // Argon2id stretch (~0.5 s), the one the decoy runs too.
        let handshake_key = stretch_enroll_key(&offer, &code).await?;

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
        let _ = state;
        Err("guardian enrollment requires the p2p + encryption features".into())
    }
}

/// The duress session's guardian-enrollment screen (see
/// [`begin_guardian_enrollment_in`]): built as the real offer is, from this
/// session's own peer id and the device's name, a code from the real
/// generator, through the same key stretch (its key thrown away), and the
/// first slot of a persona with no roster yet. Nothing is armed (a duress
/// session runs no node) and the recovery store is neither read nor written,
/// so the screen waits for a guardian that never comes.
#[cfg(all(feature = "p2p", feature = "encryption"))]
async fn decoy_guardian_offer(state: &AppState) -> Result<BeginEnrollmentResult, String> {
    use sovereign_crypto::recovery_roster::GUARDIAN_TOTAL;
    let owner_peer_id = crate::tauri_commands::pairing::peer_id_from_state(state).await;
    if owner_peer_id.is_empty() {
        return Err("guardian enrollment unavailable: p2p identity not loaded".into());
    }
    let offer = sovereign_p2p::guardian_enroll::GuardianEnrollOffer::new(
        owner_peer_id,
        state.config.p2p.device_name.clone(),
        Vec::new(),
        sovereign_p2p::guardian_enroll::GUARDIAN_OFFER_TTL_SECONDS,
    );
    let code = sovereign_crypto::pair_payload::generate_pairing_code();
    let _ = stretch_enroll_key(&offer, &code).await?;
    Ok(BeginEnrollmentResult {
        qr_payload_b64: offer.encode().str_err()?,
        code,
        // The first slot of the first epoch, as a fresh roster names it.
        shard_id: "rk-e1-0".to_string(),
        enrolled_count: 0,
        total: GUARDIAN_TOTAL as u8,
    })
}

/// The guardian-enrollment key of `offer` under `code`: the Argon2id stretch
/// (~0.5 s), as `sovereign_p2p::guardian_enroll::derive_enroll_key` computes
/// it, off the async runtime. **The real offer and the duress session's decoy
/// both go through this one function** (C-AREA AR4; SIDECHANNEL-001's rule;
/// the phone's form of the shell's `p2p::stretch_enroll_key`). Each call
/// counts in `enroll_key_stretches` (tests).
///
/// Signature by the C-AREA test author (`l1c-tests-v10`); its body is the
/// implementer's.
#[cfg(feature = "p2p")]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) async fn stretch_enroll_key(
    offer: &sovereign_p2p::guardian_enroll::GuardianEnrollOffer,
    code: &str,
) -> Result<[u8; 32], String> {
    #[cfg(test)]
    ENROLL_KEY_STRETCHES.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let offer = offer.clone();
    let code = code.to_string();
    tauri::async_runtime::spawn_blocking(move || sovereign_p2p::guardian_enroll::derive_enroll_key(&code, &offer))
        .await
        .map_err(|e| format!("kdf task: {e}"))?
        .map_err(|e| e.to_string())
}

/// The runs of [`stretch_enroll_key`] in this process (tests only).
#[cfg(all(test, feature = "p2p"))]
static ENROLL_KEY_STRETCHES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// How many times this process has run [`stretch_enroll_key`] (tests only):
/// the C-AREA walk reads it around the real guardian offer and around the
/// decoy.
///
/// Signature by the C-AREA test author (`l1c-tests-v10`); its body is the
/// implementer's.
#[cfg(all(test, feature = "p2p"))]
pub(crate) fn enroll_key_stretches() -> usize {
    ENROLL_KEY_STRETCHES.load(std::sync::atomic::Ordering::SeqCst)
}

/// Arm an in-person guardian-enrollment offer handing out the next
/// Recovery-Key share. First call generates the Recovery Key + bundle + roster.
#[tauri::command]
pub async fn begin_guardian_enrollment(
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<BeginEnrollmentResult, String> {
    crate::tauri_state::require_main_webview(&webview)?;
    begin_guardian_enrollment_in(&state).await
}

/// The guardian list as `list_guardians` answers it, on the state alone, for the session the
/// state holds (v0.0.13 rc2, Céline 2026-11-20: in a duress session it answers as for a persona
/// with no roster, and no error). The command checks its webview, then answers this.
///
/// Signature by the test author of the rc2's lot on personas in logs and exports; its body is the
/// implementer's.
#[cfg(feature = "p2p")]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) async fn list_guardians_in(state: &AppState) -> Result<GuardianRosterDto, String> {
    #[cfg(feature = "encryption")]
    {
        use sovereign_crypto::recovery_roster::GUARDIAN_TOTAL;

        let none_enrolled = || GuardianRosterDto {
            enrolled_count: 0,
            total: GUARDIAN_TOTAL as u8,
            armed: false,
            guardians: vec![],
        };
        state.require_session_unlocked().await?;
        // A duress session answers as a persona with no roster, before the
        // KEK or any file of the recovery store is asked for, as its decoy
        // enrolment does.
        if state.is_duress_session() {
            return Ok(none_enrolled());
        }
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
            None => return Ok(none_enrolled()),
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
    #[cfg(not(feature = "encryption"))]
    {
        let _ = state;
        Err("guardian roster requires the p2p + encryption features".into())
    }
}

/// The owner's guardian roster (X/5, armed only at 5). Empty before setup.
#[tauri::command]
pub async fn list_guardians(
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<GuardianRosterDto, String> {
    state.require_unlocked(&webview).await?;
    #[cfg(feature = "p2p")]
    {
        list_guardians_in(&state).await
    }
    #[cfg(not(feature = "p2p"))]
    {
        let _ = (webview, state);
        Err("guardian roster requires the p2p + encryption features".into())
    }
}
