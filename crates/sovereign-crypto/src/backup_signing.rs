//! A1 (backup plan): Ed25519 signing/verification for backup manifests.
//!
//! The signing key is account-derived ([`AccountKey::derive_backup_signing_key`]),
//! so hosts and guardians — both untrusted — can hand back a manifest whose
//! integrity and origin the recovering device verifies before trusting a
//! single byte of it. Base64 keeps signatures and pubkeys JSON-friendly.

use base64::Engine;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

use crate::error::{CryptoError, CryptoResult};

/// Base64 of the verifying (public) key for embedding in a manifest.
pub fn verifying_key_b64(key: &SigningKey) -> String {
    base64::engine::general_purpose::STANDARD.encode(key.verifying_key().as_bytes())
}

/// Sign `msg`, returning a base64 signature.
pub fn sign_b64(key: &SigningKey, msg: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(key.sign(msg).to_bytes())
}

/// Verify a base64 signature against a base64 verifying key.
/// Errors distinguish malformed inputs from a genuine verification failure.
pub fn verify_b64(pubkey_b64: &str, msg: &[u8], sig_b64: &str) -> CryptoResult<()> {
    let pk_bytes = base64::engine::general_purpose::STANDARD
        .decode(pubkey_b64)
        .map_err(|e| CryptoError::InvalidInput(format!("verifying key base64: {e}")))?;
    let pk_arr: [u8; 32] = pk_bytes
        .try_into()
        .map_err(|_| CryptoError::InvalidInput("verifying key must be 32 bytes".into()))?;
    let key = VerifyingKey::from_bytes(&pk_arr)
        .map_err(|e| CryptoError::InvalidInput(format!("verifying key decode: {e}")))?;

    let sig_bytes = base64::engine::general_purpose::STANDARD
        .decode(sig_b64)
        .map_err(|e| CryptoError::InvalidInput(format!("signature base64: {e}")))?;
    let sig_arr: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| CryptoError::InvalidInput("signature must be 64 bytes".into()))?;
    let sig = Signature::from_bytes(&sig_arr);

    key.verify(msg, &sig)
        .map_err(|_| CryptoError::VerificationFailed("backup manifest signature".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account_key::AccountKey;
    use crate::master_key::MasterKey;

    fn key() -> SigningKey {
        let mk = MasterKey::from_passphrase(b"test", b"salt").unwrap();
        AccountKey::derive(&mk).unwrap().derive_backup_signing_key()
    }

    #[test]
    fn sign_verify_roundtrip() {
        let sk = key();
        let sig = sign_b64(&sk, b"manifest bytes");
        verify_b64(&verifying_key_b64(&sk), b"manifest bytes", &sig).unwrap();
    }

    #[test]
    fn deterministic_across_devices() {
        // Same passphrase+salt on two devices → same signing identity.
        assert_eq!(verifying_key_b64(&key()), verifying_key_b64(&key()));
    }

    #[test]
    fn tampered_message_rejected() {
        let sk = key();
        let sig = sign_b64(&sk, b"manifest bytes");
        assert!(verify_b64(&verifying_key_b64(&sk), b"tampered", &sig).is_err());
    }

    #[test]
    fn wrong_key_rejected() {
        let sk = key();
        let other = MasterKey::from_passphrase(b"other", b"salt").unwrap();
        let other_pk = verifying_key_b64(
            &AccountKey::derive(&other).unwrap().derive_backup_signing_key(),
        );
        let sig = sign_b64(&sk, b"manifest bytes");
        assert!(verify_b64(&other_pk, b"manifest bytes", &sig).is_err());
    }

    #[test]
    fn malformed_inputs_rejected_cleanly() {
        let sk = key();
        let sig = sign_b64(&sk, b"m");
        assert!(verify_b64("not-base64!", b"m", &sig).is_err());
        assert!(verify_b64(&verifying_key_b64(&sk), b"m", "AAAA").is_err());
    }
}
