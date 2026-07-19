//! Sealing for in-progress Guardian Access Recovery shares (RECOVERY-001).
//!
//! During the guardian-approval window the recovering device accumulates raw
//! Recovery-Key shares on disk (`access_recovery.json`). Left plaintext, once
//! ≥threshold shares are gathered that one file reconstructs the Recovery Key →
//! KEK + AccountKey → the whole account, **with no passphrase** — the
//! distributed-guardian trust model collapses into one cleartext file
//! (RECOVERY-001).
//!
//! Fix (Céline's design): the user sets the NEW passphrase up front, before
//! guardians are contacted, and the shares are sealed under a key derived from
//! it. The on-disk artifact is then worthless without that passphrase, so the
//! vulnerable window goes from the full 72h to zero. The seal key is **never
//! persisted** — it is re-derived from the passphrase (which the user re-enters
//! on resume) plus a stored, non-secret per-recovery salt. Same Argon2id
//! hardness as the account passphrase, so offline brute-force is no easier than
//! against `auth.store`.

use crate::aead::{self, NONCE_SIZE};
use crate::error::CryptoResult;
use crate::master_key::{Kdf, MasterKey};

/// Fixed marker sealed under the seal key at recovery start. Re-checking it on
/// resume tells a wrong passphrase from a right one **even before any share has
/// arrived**, so a mistyped resume-passphrase re-prompts rather than silently
/// failing to unseal later.
const PROBE_MARKER: &[u8] = b"sovereign/access-recovery/probe/v1";

/// Derive the share-sealing key from the recovery passphrase + a per-recovery
/// salt, with the same Argon2id parameters as the account passphrase.
pub fn derive_seal_key(passphrase: &[u8], salt: &[u8]) -> CryptoResult<[u8; 32]> {
    let mk = MasterKey::derive(passphrase, salt, &Kdf::current())?;
    Ok(*mk.as_bytes())
}

/// Seal the fixed probe under `seal_key`. Returns `(nonce, ciphertext)` to store
/// alongside the sealed shares.
pub fn seal_probe(seal_key: &[u8; 32]) -> CryptoResult<(Vec<u8>, Vec<u8>)> {
    let (ct, nonce) = aead::encrypt(PROBE_MARKER, seal_key)?;
    Ok((nonce.to_vec(), ct))
}

/// True iff `seal_key` opens the stored probe — i.e. the resume passphrase
/// matches the one recovery was started with. Never panics on malformed input.
pub fn probe_ok(seal_key: &[u8; 32], nonce: &[u8], ct: &[u8]) -> bool {
    if nonce.len() != NONCE_SIZE {
        return false;
    }
    let mut n = [0u8; NONCE_SIZE];
    n.copy_from_slice(nonce);
    matches!(aead::decrypt(ct, &n, seal_key), Ok(p) if p == PROBE_MARKER)
}

/// Seal the serialized shares blob under `seal_key`. Returns `(nonce, ciphertext)`.
pub fn seal_shares(seal_key: &[u8; 32], shares_plaintext: &[u8]) -> CryptoResult<(Vec<u8>, Vec<u8>)> {
    let (ct, nonce) = aead::encrypt(shares_plaintext, seal_key)?;
    Ok((nonce.to_vec(), ct))
}

/// Unseal the shares blob. `Err` if `seal_key` is wrong (AEAD auth fails) or the
/// nonce is malformed — the caller treats that as "wrong passphrase".
pub fn unseal_shares(seal_key: &[u8; 32], nonce: &[u8], ct: &[u8]) -> CryptoResult<Vec<u8>> {
    if nonce.len() != NONCE_SIZE {
        return Err(crate::error::CryptoError::InvalidInput("bad nonce length".into()));
    }
    let mut n = [0u8; NONCE_SIZE];
    n.copy_from_slice(nonce);
    aead::decrypt(ct, &n, seal_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_key_is_deterministic_and_salt_bound() {
        let a = derive_seal_key(b"correct horse", b"salt-one").unwrap();
        let a2 = derive_seal_key(b"correct horse", b"salt-one").unwrap();
        let diff_salt = derive_seal_key(b"correct horse", b"salt-two").unwrap();
        let diff_pass = derive_seal_key(b"wrong horse", b"salt-one").unwrap();
        assert_eq!(a, a2, "same passphrase+salt → same key");
        assert_ne!(a, diff_salt, "salt-bound");
        assert_ne!(a, diff_pass, "passphrase-bound");
    }

    #[test]
    fn probe_distinguishes_right_from_wrong_passphrase() {
        let key = derive_seal_key(b"right", b"sixteen-byte-slt").unwrap();
        let (nonce, ct) = seal_probe(&key).unwrap();
        assert!(probe_ok(&key, &nonce, &ct), "right passphrase opens the probe");
        let wrong = derive_seal_key(b"wrong", b"sixteen-byte-slt").unwrap();
        assert!(!probe_ok(&wrong, &nonce, &ct), "wrong passphrase rejected");
        assert!(!probe_ok(&key, b"short", &ct), "malformed nonce rejected, no panic");
    }

    #[test]
    fn shares_seal_roundtrips_and_wrong_key_fails() {
        let key = derive_seal_key(b"pw", b"sixteen-byte-slt").unwrap();
        let plaintext = br#"{"g1":"c2hhcmU="}"#;
        let (nonce, ct) = seal_shares(&key, plaintext).unwrap();
        assert_ne!(&ct[..], &plaintext[..], "not stored in cleartext");
        assert_eq!(unseal_shares(&key, &nonce, &ct).unwrap(), plaintext, "roundtrips");
        let wrong = derive_seal_key(b"nope", b"sixteen-byte-slt").unwrap();
        assert!(unseal_shares(&wrong, &nonce, &ct).is_err(), "wrong key cannot unseal");
    }
}
