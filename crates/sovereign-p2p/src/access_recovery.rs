//! Feature 1 — Guardian **Access** Recovery driver (Workstream B1a).
//!
//! Canonical design: `doc/spec/sovereign_os_specification.md` §Guardian Social
//! Recovery. Distinct from `recovery.rs` (the Feature-2 *data* path): this
//! gathers Recovery-Key **shares** from guardians, reconstructs the Recovery
//! Key, and opens the [`RecoveryBundle`] to recover the account secrets (KEK +
//! AccountKey). The caller then re-wraps those under a NEW passphrase
//! (`AuthStore::create_with_secrets`) — no fragments, no old passphrase; the
//! data is already present on the (synced) device.
//!
//! Runs **pre-login**: persisted to `access_recovery.json` so the multi-day
//! guardian-approval wait (72h per guardian) survives restarts.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sovereign_crypto::account_key::AccountKey;
use sovereign_crypto::guardian::shamir;
use sovereign_crypto::kek::Kek;
use sovereign_crypto::recovery_key::{RecoveryBundle, RecoveryKey};

use crate::backup_client::request_recovery_share;
use crate::error::{P2pError, P2pResult};

/// Coarse phase for the wizard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessRecoveryPhase {
    /// Waiting for ≥threshold guardians to approve + release (72h each).
    AwaitingShares,
    /// Enough shares collected — ready for `finalize`.
    Ready,
    /// Account re-installed under the new passphrase.
    Installed,
    /// Hard failure (bad shares / bundle); see `error`.
    Failed,
}

/// One guardian's request bookkeeping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianRequest {
    /// Guardian id (its peer id).
    pub guardian_id: String,
    /// Dialable address (e.g. a relay-circuit multiaddr).
    pub addr: String,
    /// True once this guardian released its share.
    pub released: bool,
}

/// Persisted access-recovery progress.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessRecovery {
    pub recovery_id: String,
    pub phase: AccessRecoveryPhase,
    /// The owner's guardian tag `T_g` — what guardians match shard requests on.
    pub owner_tag: String,
    pub epoch: u32,
    pub threshold: u8,
    pub guardians: Vec<GuardianRequest>,
    /// The account secrets sealed under the Recovery Key (from the recovery
    /// card / `recovery.bundle`). Inert without ≥threshold shares.
    pub bundle: RecoveryBundle,
    /// Collected raw shares (guardian id → base64), **in memory only**. Never
    /// serialized (`serde(skip)`): at rest they live in `sealed_shares`, AEAD-
    /// sealed under the passphrase-derived key so `access_recovery.json` is not
    /// a passphrase-free path to the account (RECOVERY-001).
    #[serde(skip)]
    pub shares: BTreeMap<String, String>,
    /// Per-recovery salt for the seal-key KDF (not secret). Set at start.
    #[serde(default)]
    seal_salt: Vec<u8>,
    /// `shares` sealed under the seal key as `(nonce, ciphertext)`. THIS is what
    /// persists; the raw shares never touch disk (RECOVERY-001).
    #[serde(default)]
    sealed_shares: Option<(Vec<u8>, Vec<u8>)>,
    /// A fixed marker sealed under the seal key `(nonce, ciphertext)`, so a wrong
    /// resume-passphrase is caught before any share is even present.
    #[serde(default)]
    probe: Option<(Vec<u8>, Vec<u8>)>,
    pub error: Option<String>,
}

impl AccessRecovery {
    pub fn new(
        recovery_id: String,
        owner_tag: String,
        epoch: u32,
        threshold: u8,
        guardians: Vec<(String, String)>,
        bundle: RecoveryBundle,
    ) -> Self {
        Self {
            recovery_id,
            phase: AccessRecoveryPhase::AwaitingShares,
            owner_tag,
            epoch,
            threshold,
            guardians: guardians
                .into_iter()
                .map(|(guardian_id, addr)| GuardianRequest { guardian_id, addr, released: false })
                .collect(),
            bundle,
            shares: BTreeMap::new(),
            seal_salt: Vec::new(),
            sealed_shares: None,
            probe: None,
            error: None,
        }
    }

    /// Count from the plaintext `released` flags, not `shares` — so status/phase
    /// stay readable without the seal key (the shares may be sealed on disk and
    /// not yet unsealed). `released` ⟺ a valid share was ingested for it.
    pub fn shares_collected(&self) -> usize {
        self.guardians.iter().filter(|g| g.released).count()
    }
    pub fn have_enough(&self) -> bool {
        self.threshold > 0 && self.shares_collected() >= self.threshold as usize
    }

    /// Fold in a released share (dedup by guardian, mark released).
    pub fn ingest_share(&mut self, guardian_id: &str, share_bytes: &[u8]) {
        use base64::Engine;
        // Validate it parses as a Shamir share before accepting.
        if shamir::share_from_bytes(share_bytes).is_err() {
            tracing::warn!(target: "access_recovery", "guardian {guardian_id} returned an unparseable share — ignored");
            return;
        }
        self.shares.insert(
            guardian_id.to_string(),
            base64::engine::general_purpose::STANDARD.encode(share_bytes),
        );
        if let Some(g) = self.guardians.iter_mut().find(|g| g.guardian_id == guardian_id) {
            g.released = true;
        }
    }

    pub fn recompute_phase(&mut self) {
        if matches!(self.phase, AccessRecoveryPhase::Installed | AccessRecoveryPhase::Failed) {
            return;
        }
        self.phase = if self.have_enough() {
            AccessRecoveryPhase::Ready
        } else {
            AccessRecoveryPhase::AwaitingShares
        };
    }

    /// One network round: poll every not-yet-released guardian for its share
    /// (fixed request-id per guardian → same 72h window). Errors on individual
    /// guardians are logged and skipped, never fatal.
    pub async fn poll_round(&mut self, timeout: Duration) {
        let pending: Vec<(String, String)> = self
            .guardians
            .iter()
            .filter(|g| !g.released)
            .map(|g| (g.guardian_id.clone(), g.addr.clone()))
            .collect();
        for (gid, addr) in pending {
            let req_id = format!("{}-{}", self.recovery_id, gid);
            match request_recovery_share(&addr, &req_id, &self.owner_tag, self.epoch, timeout).await {
                Ok(Some(share)) => self.ingest_share(&gid, &share),
                Ok(None) => {} // still pending approval / 72h
                Err(e) => tracing::warn!(target: "access_recovery", "request share from {addr} failed (skipped): {e}"),
            }
        }
        self.recompute_phase();
    }

    /// Reconstruct the Recovery Key from collected shares and open the bundle,
    /// yielding the account secrets. Requires [`Self::have_enough`].
    pub fn open(&self) -> P2pResult<(Kek, AccountKey)> {
        use base64::Engine;
        if !self.have_enough() {
            return Err(P2pError::SyncError(format!(
                "not enough shares: {}/{}",
                self.shares_collected(),
                self.threshold
            )));
        }
        let shares: Vec<_> = self
            .shares
            .values()
            .filter_map(|b64| {
                base64::engine::general_purpose::STANDARD
                    .decode(b64)
                    .ok()
                    .and_then(|raw| shamir::share_from_bytes(&raw).ok())
            })
            .collect();
        let key = RecoveryKey::reconstruct(&shares, self.threshold)
            .map_err(|e| P2pError::SyncError(format!("recovery key reconstruct: {e}")))?;
        key.open_bundle(&self.bundle)
            .map_err(|e| P2pError::SyncError(format!("open recovery bundle: {e}")))
    }

    // --- persistence (mirror recovery.rs) ---

    /// Persist progress, **sealing the collected shares** under `seal_key`
    /// (RECOVERY-001): the raw shares never touch disk, only their AEAD-sealed
    /// form + a probe. Written owner-only (`write_private`) as defense in depth
    /// on top of the seal.
    pub fn save(&self, dir: &std::path::Path, seal_key: &[u8; 32]) -> P2pResult<()> {
        std::fs::create_dir_all(dir)
            .map_err(|e| P2pError::SyncError(format!("mkdir access-recovery: {e}")))?;
        let shares_json = serde_json::to_vec(&self.shares)
            .map_err(|e| P2pError::SyncError(format!("serialize shares: {e}")))?;
        let sealed = sovereign_crypto::recovery_seal::seal_shares(seal_key, &shares_json)
            .map_err(|e| P2pError::SyncError(format!("seal shares: {e}")))?;
        let probe = sovereign_crypto::recovery_seal::seal_probe(seal_key)
            .map_err(|e| P2pError::SyncError(format!("seal probe: {e}")))?;
        let mut snapshot = self.clone();
        snapshot.sealed_shares = Some(sealed);
        snapshot.probe = Some(probe);
        let json = serde_json::to_vec_pretty(&snapshot)
            .map_err(|e| P2pError::SyncError(format!("serialize access-recovery: {e}")))?;
        sovereign_crypto::fs_private::write_private(&dir.join("access_recovery.json"), &json)
            .map_err(|e| P2pError::SyncError(format!("write access-recovery: {e}")))
    }

    /// Load progress **without** the seal key: metadata + phase are readable
    /// (`shares_collected` uses the plaintext `released` flags), but `shares`
    /// stays empty until [`Self::unseal`].
    pub fn load(dir: &std::path::Path) -> Option<Self> {
        let bytes = std::fs::read(dir.join("access_recovery.json")).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Store the per-recovery salt (at start), so resume can re-derive the seal
    /// key from the re-entered passphrase. Not secret.
    pub fn set_seal_salt(&mut self, salt: Vec<u8>) {
        self.seal_salt = salt;
    }
    pub fn seal_salt(&self) -> &[u8] {
        &self.seal_salt
    }

    /// True iff `seal_key` opens the stored probe — the resume passphrase
    /// matches the one recovery started with. `true` when no probe exists yet.
    pub fn verify_probe(&self, seal_key: &[u8; 32]) -> bool {
        match &self.probe {
            Some((nonce, ct)) => sovereign_crypto::recovery_seal::probe_ok(seal_key, nonce, ct),
            None => true,
        }
    }

    /// Decrypt the sealed shares into memory. Call after [`Self::load`] with the
    /// seal key before `poll_round` (which appends) or [`Self::open`]. No-op if
    /// nothing is sealed yet. `Err` on a wrong key = wrong passphrase.
    pub fn unseal(&mut self, seal_key: &[u8; 32]) -> P2pResult<()> {
        if let Some((nonce, ct)) = self.sealed_shares.clone() {
            let plaintext = sovereign_crypto::recovery_seal::unseal_shares(seal_key, &nonce, &ct)
                .map_err(|e| P2pError::SyncError(format!("unseal shares: {e}")))?;
            self.shares = serde_json::from_slice(&plaintext)
                .map_err(|e| P2pError::SyncError(format!("parse shares: {e}")))?;
        }
        Ok(())
    }

    pub fn cancel(dir: &std::path::Path) {
        let _ = std::fs::remove_file(dir.join("access_recovery.json"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sovereign_crypto::recovery_roster::RecoverySetup;

    // Build a real setup, then simulate collecting 3 of its shares as the
    // guardians would release them, and prove reconstruct+open recovers the
    // exact account secrets.
    #[test]
    fn collected_shares_reconstruct_and_open_the_bundle() {
        use base64::Engine;
        let kek = Kek::from_bytes([0x51; 32]);
        let ak = AccountKey::from_bytes([0x52; 32]);
        let setup = RecoverySetup::new(&kek, &ak, 1).unwrap();

        // Raw share bytes as a guardian would hold + release them.
        let raw_shares: Vec<Vec<u8>> = setup
            .slots
            .iter()
            .map(|s| {
                base64::engine::general_purpose::STANDARD
                    .decode(s.pending_share_b64.as_ref().unwrap())
                    .unwrap()
            })
            .collect();

        let mut rec = AccessRecovery::new(
            "rec-acc-1".into(),
            "T_g-abc".into(),
            1,
            3,
            (0..5).map(|i| (format!("g{i}"), format!("/addr/{i}"))).collect(),
            setup.bundle.clone(),
        );
        assert!(!rec.have_enough());
        // Three guardians release.
        rec.ingest_share("g0", &raw_shares[0]);
        rec.ingest_share("g2", &raw_shares[2]);
        rec.ingest_share("g4", &raw_shares[4]);
        rec.recompute_phase();
        assert!(rec.have_enough());
        assert_eq!(rec.phase, AccessRecoveryPhase::Ready);

        let (kek2, ak2) = rec.open().unwrap();
        assert_eq!(kek2.as_bytes(), &[0x51; 32]);
        assert_eq!(ak2.as_bytes(), &[0x52; 32]);
    }

    #[test]
    fn below_threshold_cannot_open() {
        let kek = Kek::from_bytes([1; 32]);
        let ak = AccountKey::from_bytes([2; 32]);
        let setup = RecoverySetup::new(&kek, &ak, 1).unwrap();
        let mut rec = AccessRecovery::new(
            "r".into(),
            "t".into(),
            1,
            3,
            vec![("g0".into(), "/a".into())],
            setup.bundle.clone(),
        );
        use base64::Engine;
        let s0 = base64::engine::general_purpose::STANDARD
            .decode(setup.slots[0].pending_share_b64.as_ref().unwrap())
            .unwrap();
        rec.ingest_share("g0", &s0);
        assert!(!rec.have_enough());
        assert!(rec.open().is_err());
    }

    #[test]
    fn dedup_by_guardian_and_gross_garbage_rejected() {
        let kek = Kek::from_bytes([3; 32]);
        let ak = AccountKey::from_bytes([4; 32]);
        let setup = RecoverySetup::new(&kek, &ak, 1).unwrap();
        let mut rec = AccessRecovery::new(
            "r".into(),
            "t".into(),
            1,
            3,
            (0..5).map(|i| (format!("g{i}"), "/a".into())).collect(),
            setup.bundle.clone(),
        );
        use base64::Engine;
        let s0 = base64::engine::general_purpose::STANDARD
            .decode(setup.slots[0].pending_share_b64.as_ref().unwrap())
            .unwrap();
        rec.ingest_share("g0", &s0);
        rec.ingest_share("g0", &s0); // same guardian → no double count
        rec.ingest_share("g1", b""); // grossly malformed → rejected at parse
        assert_eq!(rec.shares_collected(), 1);
    }

    #[test]
    fn wrong_shares_fail_at_open_not_silently() {
        // Shamir shares are not self-authenticating: three well-formed shares
        // from the WRONG key reconstruct to a wrong Recovery Key, and the
        // bundle's AEAD is what rejects it. Recovery fails loudly at open(),
        // never returns wrong secrets. (This is why a malicious guardian's
        // garbage share can't silently poison the result.)
        let kek = Kek::from_bytes([7; 32]);
        let ak = AccountKey::from_bytes([8; 32]);
        let setup = RecoverySetup::new(&kek, &ak, 1).unwrap(); // bundle from A
        let other = RecoverySetup::new(&kek, &ak, 1).unwrap(); // shares from B

        use base64::Engine;
        let mut rec = AccessRecovery::new(
            "r".into(),
            "t".into(),
            1,
            3,
            (0..5).map(|i| (format!("g{i}"), "/a".into())).collect(),
            setup.bundle.clone(),
        );
        for i in [0usize, 1, 2] {
            let raw = base64::engine::general_purpose::STANDARD
                .decode(other.slots[i].pending_share_b64.as_ref().unwrap())
                .unwrap();
            rec.ingest_share(&format!("g{i}"), &raw);
        }
        assert!(rec.have_enough());
        assert!(rec.open().is_err(), "wrong-key shares must fail at the bundle AEAD");
    }

    #[test]
    fn shares_are_sealed_at_rest_and_only_the_right_passphrase_unseals() {
        // RECOVERY-001: the persisted file must not be a passphrase-free path to
        // the account. Shares are sealed under the passphrase-derived key.
        use base64::Engine;
        use sovereign_crypto::recovery_seal::derive_seal_key;
        let dir = std::env::temp_dir().join("sovereign-p2p-access-recovery-seal-test");
        let _ = std::fs::remove_dir_all(&dir);
        let salt = b"argon2id-salt-16";
        let key = derive_seal_key(b"recover-into-this", salt).unwrap();

        let kek = Kek::from_bytes([0x71; 32]);
        let ak = AccountKey::from_bytes([0x72; 32]);
        let setup = RecoverySetup::new(&kek, &ak, 1).unwrap();
        let raw: Vec<Vec<u8>> = setup
            .slots
            .iter()
            .map(|s| {
                base64::engine::general_purpose::STANDARD
                    .decode(s.pending_share_b64.as_ref().unwrap())
                    .unwrap()
            })
            .collect();

        let mut rec = AccessRecovery::new(
            "rec-seal-1".into(),
            "T_g".into(),
            1,
            3,
            (0..5).map(|i| (format!("g{i}"), format!("/addr/{i}"))).collect(),
            setup.bundle.clone(),
        );
        rec.set_seal_salt(salt.to_vec());
        rec.ingest_share("g0", &raw[0]);
        rec.ingest_share("g1", &raw[1]);
        rec.ingest_share("g2", &raw[2]);
        rec.save(&dir, &key).unwrap();

        // The raw share must NOT appear in cleartext on disk.
        let on_disk = std::fs::read_to_string(dir.join("access_recovery.json")).unwrap();
        let share_b64 = base64::engine::general_purpose::STANDARD.encode(&raw[0]);
        assert!(
            !on_disk.contains(&share_b64),
            "raw share leaked to disk in cleartext (RECOVERY-001)"
        );

        // Keyless load: count survives (released flags), shares stay sealed.
        let mut loaded = AccessRecovery::load(&dir).unwrap();
        assert_eq!(loaded.shares_collected(), 3, "count is keyless");
        assert!(loaded.shares.is_empty(), "shares stay sealed until unseal");

        // Wrong passphrase: probe rejects, unseal fails — the "wrong-passphrase" gate.
        let wrong = derive_seal_key(b"WRONG-passphrase", salt).unwrap();
        assert!(!loaded.verify_probe(&wrong));
        let mut w = loaded.clone();
        assert!(w.unseal(&wrong).is_err());

        // Right passphrase: probe passes, unseal → open recovers the exact secrets.
        assert!(loaded.verify_probe(&key));
        loaded.unseal(&key).unwrap();
        let (k2, a2) = loaded.open().unwrap();
        assert_eq!(k2.as_bytes(), &[0x71; 32]);
        assert_eq!(a2.as_bytes(), &[0x72; 32]);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
