//! Feature 1 — Guardian Access Recovery: the dedicated **Recovery Key** and the
//! **Recovery Bundle** it seals.
//!
//! Canonical design: `doc/spec/sovereign_os_specification.md` §Guardian Social
//! Recovery. The Recovery Key is a random 256-bit key, Shamir-split (3-of-5)
//! across the owner's guardians. It seals the **account secrets** (the random
//! [`Kek`] + [`AccountKey`] that actually decrypt content) into a
//! [`RecoveryBundle`], which the owner stores on their (synced) devices —
//! **outside** the passphrase wrapping. A device whose user forgot the
//! passphrase still holds the bundle but cannot open it; reconstructing the
//! Recovery Key from ≥3 guardian shares opens it, yielding the account secrets,
//! which are then re-wrapped under a **new** passphrase (fresh `auth.store`).
//!
//! Guardians hold only Shamir shares of the Recovery Key — never documents,
//! never the account secrets, never a per-backup data key.

use blahaj::Share;
use rand::Rng;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::account_key::AccountKey;
use crate::aead::{self, KEY_SIZE, NONCE_SIZE};
use crate::error::{CryptoError, CryptoResult};
use crate::guardian::shamir;
use crate::kek::Kek;

/// Plaintext sealed in a bundle: `KEK(32) || AccountKey(32)`.
const BUNDLE_PLAINTEXT_LEN: usize = KEY_SIZE * 2;

/// A dedicated random 256-bit key, Shamir-split across guardians (Feature 1).
/// Distinct from the passphrase-derived Master Key: reconstructing it restores
/// decryption capability **without** the passphrase.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct RecoveryKey {
    bytes: [u8; KEY_SIZE],
}

/// The account secrets (KEK + AccountKey) AEAD-sealed under a [`RecoveryKey`].
/// Stored on the owner's synced devices, outside the passphrase wrapping.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RecoveryBundle {
    pub ciphertext: Vec<u8>,
    pub nonce: [u8; NONCE_SIZE],
}

impl RecoveryKey {
    /// Fresh random Recovery Key.
    pub fn generate() -> Self {
        let mut bytes = [0u8; KEY_SIZE];
        rand::rng().fill_bytes(&mut bytes);
        Self { bytes }
    }

    /// Reconstruct from raw bytes (e.g. after Shamir recovery).
    pub fn from_bytes(bytes: [u8; KEY_SIZE]) -> Self {
        Self { bytes }
    }

    /// Raw key bytes.
    pub fn as_bytes(&self) -> &[u8; KEY_SIZE] {
        &self.bytes
    }

    /// Seal the account secrets (KEK + AccountKey) under this Recovery Key.
    pub fn seal_bundle(&self, kek: &Kek, account_key: &AccountKey) -> CryptoResult<RecoveryBundle> {
        let mut plaintext = [0u8; BUNDLE_PLAINTEXT_LEN];
        plaintext[..KEY_SIZE].copy_from_slice(kek.as_bytes());
        plaintext[KEY_SIZE..].copy_from_slice(account_key.as_bytes());
        let sealed = aead::encrypt(&plaintext, &self.bytes);
        plaintext.zeroize();
        let (ciphertext, nonce) = sealed?;
        Ok(RecoveryBundle { ciphertext, nonce })
    }

    /// Open a bundle, recovering the account secrets (KEK, AccountKey).
    pub fn open_bundle(&self, bundle: &RecoveryBundle) -> CryptoResult<(Kek, AccountKey)> {
        let mut plaintext = aead::decrypt(&bundle.ciphertext, &bundle.nonce, &self.bytes)?;
        if plaintext.len() != BUNDLE_PLAINTEXT_LEN {
            plaintext.zeroize();
            return Err(CryptoError::RecoveryError(format!(
                "recovery bundle wrong length: {} (expected {BUNDLE_PLAINTEXT_LEN})",
                plaintext.len()
            )));
        }
        let mut kek_b = [0u8; KEY_SIZE];
        let mut ak_b = [0u8; KEY_SIZE];
        kek_b.copy_from_slice(&plaintext[..KEY_SIZE]);
        ak_b.copy_from_slice(&plaintext[KEY_SIZE..]);
        plaintext.zeroize();
        // Kek / AccountKey own their bytes and zeroize on drop.
        Ok((Kek::from_bytes(kek_b), AccountKey::from_bytes(ak_b)))
    }

    /// Shamir-split into `total` shares; any `threshold` reconstruct the key.
    /// Serialize each share with [`shamir::share_to_bytes`] for handoff to a
    /// guardian; guardians store the opaque bytes.
    pub fn split(&self, threshold: u8, total: usize) -> CryptoResult<Vec<Share>> {
        shamir::split_secret(&self.bytes, threshold, total)
    }

    /// Reconstruct from ≥`threshold` collected shares.
    pub fn reconstruct(shares: &[Share], threshold: u8) -> CryptoResult<Self> {
        Ok(Self::from_bytes(shamir::reconstruct_secret(shares, threshold)?))
    }
}

impl std::fmt::Debug for RecoveryKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecoveryKey").field("bytes", &"[REDACTED]").finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secrets() -> (Kek, AccountKey) {
        (Kek::from_bytes([0x11; KEY_SIZE]), AccountKey::from_bytes([0x22; KEY_SIZE]))
    }

    #[test]
    fn seal_then_open_roundtrips_the_account_secrets() {
        let rk = RecoveryKey::generate();
        let (kek, ak) = secrets();
        let bundle = rk.seal_bundle(&kek, &ak).unwrap();
        let (kek2, ak2) = rk.open_bundle(&bundle).unwrap();
        assert_eq!(kek2.as_bytes(), &[0x11; KEY_SIZE]);
        assert_eq!(ak2.as_bytes(), &[0x22; KEY_SIZE]);
    }

    #[test]
    fn split_3_of_5_reconstructs_and_opens() {
        let rk = RecoveryKey::generate();
        let (kek, ak) = secrets();
        let bundle = rk.seal_bundle(&kek, &ak).unwrap();

        let shares = rk.split(3, 5).unwrap();
        assert_eq!(shares.len(), 5);

        // Any 3 shares reconstruct the Recovery Key and open the bundle.
        let subset = vec![shares[0].clone(), shares[2].clone(), shares[4].clone()];
        let recovered = RecoveryKey::reconstruct(&subset, 3).unwrap();
        assert_eq!(recovered.as_bytes(), rk.as_bytes());
        let (kek2, ak2) = recovered.open_bundle(&bundle).unwrap();
        assert_eq!(kek2.as_bytes(), &[0x11; KEY_SIZE]);
        assert_eq!(ak2.as_bytes(), &[0x22; KEY_SIZE]);
    }

    #[test]
    fn fewer_than_threshold_cannot_reconstruct() {
        let rk = RecoveryKey::generate();
        let shares = rk.split(3, 5).unwrap();
        // 2 shares: below threshold → error (never the right key).
        let two = vec![shares[0].clone(), shares[1].clone()];
        assert!(RecoveryKey::reconstruct(&two, 3).is_err());
    }

    #[test]
    fn shares_survive_byte_serialization() {
        let rk = RecoveryKey::generate();
        let shares = rk.split(3, 5).unwrap();
        // Round-trip through the transport form guardians actually store.
        let wire: Vec<Vec<u8>> = shares.iter().map(shamir::share_to_bytes).collect();
        let back: Vec<Share> = wire
            .iter()
            .take(3)
            .map(|b| shamir::share_from_bytes(b).unwrap())
            .collect();
        let recovered = RecoveryKey::reconstruct(&back, 3).unwrap();
        assert_eq!(recovered.as_bytes(), rk.as_bytes());
    }

    #[test]
    fn wrong_recovery_key_cannot_open_bundle() {
        let rk = RecoveryKey::generate();
        let (kek, ak) = secrets();
        let bundle = rk.seal_bundle(&kek, &ak).unwrap();
        let other = RecoveryKey::generate();
        assert!(other.open_bundle(&bundle).is_err());
    }
}
