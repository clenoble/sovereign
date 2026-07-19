//! Guardian-side enrollment: scan the owner's QR, type the spoken code,
//! come away with a duty. Thin glue over
//! `sovereign_p2p::guardian_enroll::enroll_with_owner` — the shard is
//! persisted into custody *inside* the handshake (before the custody
//! receipt is sent), so "the owner saw me enroll" and "the shard is on
//! my disk" can never disagree.

use std::time::Duration;

use sovereign_p2p::guardian_enroll::{enroll_with_owner, GuardianEnrollOffer};

use crate::custody::{CustodyStore, GuardianDuty};
use crate::error::{GuardianError, GuardianResult};
use crate::state::GuardianIdentity;

/// Default handshake timeout — in-person flow, generous for slow typing.
pub const ENROLL_TIMEOUT: Duration = Duration::from_secs(120);

/// Run the enrollment against a scanned offer. On success the duty is
/// already persisted in `custody`.
pub async fn enroll(
    offer_b64: &str,
    code: &str,
    guardian_label: &str,
    identity: &GuardianIdentity,
    custody: &mut CustodyStore,
    timeout: Duration,
) -> GuardianResult<GuardianDuty> {
    let offer = GuardianEnrollOffer::decode(offer_b64)?;
    let keypair = identity.keypair()?;
    let owner_peer_id = offer.owner_peer_id.clone();

    // The duty produced inside the persist callback, handed back out.
    let mut persisted: Option<GuardianDuty> = None;
    let outcome = enroll_with_owner(
        &offer,
        code,
        keypair,
        guardian_label,
        |grant| {
            let duty = custody
                .add_duty(grant, &owner_peer_id)
                .map_err(|e| e.to_string())?;
            persisted = Some(duty);
            Ok(())
        },
        timeout,
    )
    .await?;

    persisted.ok_or_else(|| {
        // Unreachable by construction (outcome implies the callback ran),
        // but never panic in an app friends install.
        GuardianError::Custody(format!(
            "enrollment for {} finished without persisting a duty",
            outcome.owner_peer_id
        ))
    })
}
