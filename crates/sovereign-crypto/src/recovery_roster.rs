//! Feature 1 — owner-side recovery setup + guardian roster.
//!
//! Canonical design: `doc/spec/sovereign_os_specification.md` §Guardian Social
//! Recovery. When the owner sets up recovery, this:
//!   1. generates a [`RecoveryKey`], seals the account secrets into a
//!      [`RecoveryBundle`] (kept by the owner, synced across devices),
//!   2. pre-splits the key 3-of-5 into shares, one per roster slot,
//!   3. hands each share to a guardian as they enroll **in person**, clearing
//!      the owner's local copy of that share once it is distributed.
//!
//! **Recovery is ARMED only when all 5 guardians are enrolled** (spec: full
//! 5-guardian roster before recovery is offered — decided 2026-07-10). Until
//! then the roster reports `enrolled / 5` and holds the not-yet-distributed
//! shares. The [`RecoveryKey`] itself is never retained — only the bundle and
//! the pending shares are; after full enrollment the owner holds the bundle
//! alone, and the 5 shares live one-per-guardian.

use serde::{Deserialize, Serialize};

use crate::account_key::AccountKey;
use crate::error::{CryptoError, CryptoResult};
use crate::guardian::shamir::{self, DEFAULT_THRESHOLD, DEFAULT_TOTAL_SHARES};
use crate::kek::Kek;
use crate::recovery_key::{RecoveryBundle, RecoveryKey};

/// Total guardians in the roster (spec: 5).
pub const GUARDIAN_TOTAL: usize = DEFAULT_TOTAL_SHARES;
/// Shares required to reconstruct (spec: 3-of-5).
pub const GUARDIAN_THRESHOLD: u8 = DEFAULT_THRESHOLD;

/// One roster slot: a pre-minted share, and — once a guardian enrolls in
/// person — that guardian's identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianSlot {
    /// Stable id naming this share/shard (quoted in the guardian handshake).
    pub shard_id: String,
    /// The serialized Shamir share (base64). `Some` while pending on the
    /// owner's device; set to `None` once handed off to the guardian, so the
    /// owner does not retain it.
    pub pending_share_b64: Option<String>,
    /// The enrolled guardian's peer id — `None` until enrolled.
    pub guardian_peer_id: Option<String>,
    /// Human label for the roster UI ("Mum", "Alex").
    pub label: Option<String>,
    /// RFC3339 enrollment time — `None` until enrolled.
    pub enrolled_at: Option<String>,
}

impl GuardianSlot {
    /// True once a guardian has been enrolled into this slot.
    pub fn is_enrolled(&self) -> bool {
        self.guardian_peer_id.is_some()
    }
}

/// Owner-side recovery state: the sealed bundle + the 5-slot guardian roster.
/// Serialize to persist (`recovery_roster.json`); the bundle travels with it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoverySetup {
    /// Key-rotation epoch these shares belong to (bumped on rotation).
    pub epoch: u32,
    /// Account secrets sealed under the (now-discarded) Recovery Key.
    pub bundle: RecoveryBundle,
    /// Exactly [`GUARDIAN_TOTAL`] slots.
    pub slots: Vec<GuardianSlot>,
}

impl RecoverySetup {
    /// Begin recovery setup: generate the Recovery Key, seal the account
    /// secrets, split 3-of-5, and lay out 5 pending slots. The Recovery Key is
    /// dropped here — it survives only as the 5 shares.
    pub fn new(kek: &Kek, account_key: &AccountKey, epoch: u32) -> CryptoResult<Self> {
        let recovery_key = RecoveryKey::generate();
        let bundle = recovery_key.seal_bundle(kek, account_key)?;
        let shares = recovery_key.split(GUARDIAN_THRESHOLD, GUARDIAN_TOTAL)?;
        // recovery_key drops at end of scope (ZeroizeOnDrop).

        let slots = shares
            .iter()
            .enumerate()
            .map(|(i, share)| {
                use base64::Engine;
                GuardianSlot {
                    shard_id: format!("rk-e{epoch}-{i}"),
                    pending_share_b64: Some(
                        base64::engine::general_purpose::STANDARD
                            .encode(shamir::share_to_bytes(share)),
                    ),
                    guardian_peer_id: None,
                    label: None,
                    enrolled_at: None,
                }
            })
            .collect();

        Ok(Self { epoch, bundle, slots })
    }

    /// Number of guardians enrolled so far.
    pub fn enrolled_count(&self) -> usize {
        self.slots.iter().filter(|s| s.is_enrolled()).count()
    }

    /// Recovery is usable only with the full 5-guardian roster (spec).
    pub fn is_armed(&self) -> bool {
        self.enrolled_count() == GUARDIAN_TOTAL
    }

    /// The next slot with a share still to hand out — used to arm a guardian
    /// enrollment offer. `None` when every share has been distributed.
    pub fn next_pending_slot(&self) -> Option<&GuardianSlot> {
        self.slots
            .iter()
            .find(|s| s.pending_share_b64.is_some() && !s.is_enrolled())
    }

    /// Record a guardian as enrolled into the slot named by `shard_id`, and
    /// clear the owner's local copy of that share (it now lives with the
    /// guardian). Errors if the slot is unknown or already enrolled.
    pub fn mark_enrolled(
        &mut self,
        shard_id: &str,
        guardian_peer_id: &str,
        label: &str,
        enrolled_at: &str,
    ) -> CryptoResult<()> {
        let slot = self
            .slots
            .iter_mut()
            .find(|s| s.shard_id == shard_id)
            .ok_or_else(|| CryptoError::RecoveryError(format!("unknown roster slot {shard_id}")))?;
        if slot.is_enrolled() {
            return Err(CryptoError::RecoveryError(format!(
                "slot {shard_id} already enrolled to {:?}",
                slot.guardian_peer_id
            )));
        }
        slot.guardian_peer_id = Some(guardian_peer_id.to_string());
        slot.label = Some(label.to_string());
        slot.enrolled_at = Some(enrolled_at.to_string());
        slot.pending_share_b64 = None; // owner no longer retains the share
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> RecoverySetup {
        let kek = Kek::from_bytes([0x33; 32]);
        let ak = AccountKey::from_bytes([0x44; 32]);
        RecoverySetup::new(&kek, &ak, 1).unwrap()
    }

    #[test]
    fn new_lays_out_five_pending_slots_not_armed() {
        let s = setup();
        assert_eq!(s.slots.len(), GUARDIAN_TOTAL);
        assert_eq!(s.enrolled_count(), 0);
        assert!(!s.is_armed());
        assert!(s.slots.iter().all(|sl| sl.pending_share_b64.is_some()));
    }

    #[test]
    fn armed_only_at_five_enrolled() {
        let mut s = setup();
        for i in 0..GUARDIAN_TOTAL {
            let shard_id = s.next_pending_slot().unwrap().shard_id.clone();
            s.mark_enrolled(&shard_id, &format!("peer{i}"), &format!("G{i}"), "t").unwrap();
            let armed = i == GUARDIAN_TOTAL - 1;
            assert_eq!(s.is_armed(), armed, "armed after {} enrolled", i + 1);
        }
        assert_eq!(s.enrolled_count(), 5);
        // Every share handed off; owner retains none.
        assert!(s.slots.iter().all(|sl| sl.pending_share_b64.is_none()));
        assert!(s.next_pending_slot().is_none());
    }

    #[test]
    fn enrolled_guardians_shares_still_reconstruct_the_key_and_open_bundle() {
        // Simulate: capture the 5 shares as they'd be handed out, then prove
        // any 3 reconstruct the key that opens this roster's bundle.
        let mut s = setup();
        use base64::Engine;
        let mut handed = Vec::new();
        while let Some(slot) = s.next_pending_slot() {
            let b = slot.pending_share_b64.clone().unwrap();
            let shard_id = slot.shard_id.clone();
            handed.push(b);
            s.mark_enrolled(&shard_id, "p", "L", "t").unwrap();
        }
        assert_eq!(handed.len(), 5);
        let shares: Vec<_> = handed
            .iter()
            .take(3)
            .map(|b64| {
                let raw = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
                shamir::share_from_bytes(&raw).unwrap()
            })
            .collect();
        let rk = RecoveryKey::reconstruct(&shares, GUARDIAN_THRESHOLD).unwrap();
        let (kek, ak) = rk.open_bundle(&s.bundle).unwrap();
        assert_eq!(kek.as_bytes(), &[0x33; 32]);
        assert_eq!(ak.as_bytes(), &[0x44; 32]);
    }

    #[test]
    fn double_enroll_and_unknown_slot_error() {
        let mut s = setup();
        let shard_id = s.slots[0].shard_id.clone();
        s.mark_enrolled(&shard_id, "p", "L", "t").unwrap();
        assert!(s.mark_enrolled(&shard_id, "p2", "L2", "t").is_err());
        assert!(s.mark_enrolled("nope", "p", "L", "t").is_err());
    }

    #[test]
    fn roster_survives_json_roundtrip() {
        let s = setup();
        let json = serde_json::to_string(&s).unwrap();
        let back: RecoverySetup = serde_json::from_str(&json).unwrap();
        assert_eq!(back.slots.len(), 5);
        assert_eq!(back.epoch, 1);
    }
}
