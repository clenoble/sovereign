//! Vault primitive — encrypted blobs keyed to the user's `DeviceKey`.
//!
//! The PII pipeline (see `doc/plans/pii-management-dashboard.md`) writes two
//! kinds of encrypted values, both keyed to the same per-device key:
//!
//! 1. `PiiRecord.value_encrypted` / `value_nonce` — vault entries
//!    (user-entered passwords, IBANs, etc.) and discovered findings.
//! 2. `Document.body_raw_encrypted` / `body_raw_nonce` and the matching
//!    `Message` fields — the original (pre-tokenization) body text,
//!    preserved for L3-gated reveal so LLM-NER false positives don't
//!    irreversibly mangle source content.
//!
//! No per-document key is wrapped here, unlike `key_db`/`document_key`:
//! these values are user-scoped and the cipher input is always the
//! `DeviceKey` directly. The blob format mirrors the schema's
//! `(ciphertext_b64, nonce_b64)` pair, so the type round-trips through
//! SurrealDB without bespoke conversion at every call site.

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};

use crate::aead::{self, NONCE_SIZE};
use crate::device_key::DeviceKey;
use crate::error::{CryptoError, CryptoResult};

/// XChaCha20-Poly1305 ciphertext plus its 24-byte nonce, both base64-encoded.
///
/// The two fields map 1:1 to the schema's storage shape:
///   - `PiiRecord { value_encrypted, value_nonce }`
///   - `Document { body_raw_encrypted, body_raw_nonce }` (each `Option`)
///   - `Message  { body_raw_encrypted, body_raw_nonce }` (each `Option`)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EncryptedBlob {
    /// Base64 (STANDARD) of the AEAD ciphertext (includes the 16-byte tag).
    pub ciphertext_b64: String,
    /// Base64 (STANDARD) of the 24-byte XChaCha20 nonce.
    pub nonce_b64: String,
}

impl EncryptedBlob {
    /// Encrypt arbitrary bytes under the device key.
    pub fn encrypt(plaintext: &[u8], device_key: &DeviceKey) -> CryptoResult<Self> {
        let (ct, nonce) = aead::encrypt(plaintext, device_key.as_bytes())?;
        Ok(Self {
            ciphertext_b64: B64.encode(&ct),
            nonce_b64: B64.encode(nonce),
        })
    }

    /// Encrypt a UTF-8 string under the device key.
    pub fn encrypt_str(plaintext: &str, device_key: &DeviceKey) -> CryptoResult<Self> {
        Self::encrypt(plaintext.as_bytes(), device_key)
    }

    /// Decrypt to raw bytes.
    pub fn decrypt(&self, device_key: &DeviceKey) -> CryptoResult<Vec<u8>> {
        let ct = B64
            .decode(&self.ciphertext_b64)
            .map_err(|e| CryptoError::Base64(e.to_string()))?;
        let nonce_vec = B64
            .decode(&self.nonce_b64)
            .map_err(|e| CryptoError::Base64(e.to_string()))?;
        if nonce_vec.len() != NONCE_SIZE {
            return Err(CryptoError::InvalidNonceLength {
                expected: NONCE_SIZE,
                got: nonce_vec.len(),
            });
        }
        let mut nonce = [0u8; NONCE_SIZE];
        nonce.copy_from_slice(&nonce_vec);
        aead::decrypt(&ct, &nonce, device_key.as_bytes())
    }

    /// Decrypt and interpret as UTF-8.
    pub fn decrypt_to_string(&self, device_key: &DeviceKey) -> CryptoResult<String> {
        let bytes = self.decrypt(device_key)?;
        String::from_utf8(bytes).map_err(|e| CryptoError::Serialization(e.to_string()))
    }

    /// Decompose into `(ciphertext_b64, nonce_b64)` for direct assignment to
    /// the matching schema fields.
    pub fn into_pair(self) -> (String, String) {
        (self.ciphertext_b64, self.nonce_b64)
    }

    /// Reconstitute from a `(ciphertext_b64, nonce_b64)` pair previously
    /// stored in a SurrealDB record.
    pub fn from_pair(ciphertext_b64: String, nonce_b64: String) -> Self {
        Self {
            ciphertext_b64,
            nonce_b64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::master_key::MasterKey;

    fn test_device_key() -> DeviceKey {
        let mk = MasterKey::from_passphrase(b"test", b"salt").unwrap();
        DeviceKey::derive(&mk, "dev-vault").unwrap()
    }

    #[test]
    fn round_trip_bytes() {
        let dk = test_device_key();
        let plaintext = b"my-iban-CH9300762011623852957";
        let blob = EncryptedBlob::encrypt(plaintext, &dk).unwrap();
        let recovered = blob.decrypt(&dk).unwrap();
        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn round_trip_string_utf8() {
        // Non-ASCII to confirm no byte-level mangling.
        let dk = test_device_key();
        let plaintext = "céline@example.ch — 北京";
        let blob = EncryptedBlob::encrypt_str(plaintext, &dk).unwrap();
        let recovered = blob.decrypt_to_string(&dk).unwrap();
        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn empty_plaintext_round_trip() {
        let dk = test_device_key();
        let blob = EncryptedBlob::encrypt(b"", &dk).unwrap();
        assert!(blob.decrypt(&dk).unwrap().is_empty());
    }

    #[test]
    fn large_payload_round_trip() {
        // Document bodies can be arbitrarily long; ensure no surprise ceilings.
        let dk = test_device_key();
        let plaintext = vec![0xA5u8; 256 * 1024];
        let blob = EncryptedBlob::encrypt(&plaintext, &dk).unwrap();
        assert_eq!(blob.decrypt(&dk).unwrap(), plaintext);
    }

    #[test]
    fn wrong_device_key_fails() {
        let mk_a = MasterKey::from_passphrase(b"good", b"salt").unwrap();
        let dk_a = DeviceKey::derive(&mk_a, "dev-A").unwrap();
        let mk_b = MasterKey::from_passphrase(b"bad", b"salt").unwrap();
        let dk_b = DeviceKey::derive(&mk_b, "dev-A").unwrap();
        let blob = EncryptedBlob::encrypt(b"secret", &dk_a).unwrap();
        assert!(matches!(
            blob.decrypt(&dk_b).unwrap_err(),
            CryptoError::DecryptionFailed
        ));
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let dk = test_device_key();
        let mut blob = EncryptedBlob::encrypt(b"secret", &dk).unwrap();
        let mut ct = B64.decode(&blob.ciphertext_b64).unwrap();
        ct[0] ^= 0xff;
        blob.ciphertext_b64 = B64.encode(&ct);
        assert!(matches!(
            blob.decrypt(&dk).unwrap_err(),
            CryptoError::DecryptionFailed
        ));
    }

    #[test]
    fn invalid_nonce_length_rejected() {
        let dk = test_device_key();
        let blob = EncryptedBlob {
            ciphertext_b64: B64.encode(b"abc"),
            nonce_b64: B64.encode(b"too short"),
        };
        match blob.decrypt(&dk).unwrap_err() {
            CryptoError::InvalidNonceLength { expected, got } => {
                assert_eq!(expected, NONCE_SIZE);
                assert_eq!(got, 9);
            }
            other => panic!("expected InvalidNonceLength, got {other:?}"),
        }
    }

    #[test]
    fn invalid_base64_rejected() {
        let dk = test_device_key();
        let blob = EncryptedBlob {
            ciphertext_b64: "!!! not base64 !!!".into(),
            nonce_b64: "also bad".into(),
        };
        assert!(matches!(
            blob.decrypt(&dk).unwrap_err(),
            CryptoError::Base64(_)
        ));
    }

    #[test]
    fn nonces_unique_across_encrypts() {
        // XChaCha20-Poly1305 confidentiality requires never reusing
        // (key, nonce). The nonce comes from OsRng inside aead::encrypt; this
        // guards against any future regression that hardcodes it.
        let dk = test_device_key();
        let plaintext = b"same plaintext";
        let a = EncryptedBlob::encrypt(plaintext, &dk).unwrap();
        let b = EncryptedBlob::encrypt(plaintext, &dk).unwrap();
        assert_ne!(a.nonce_b64, b.nonce_b64);
        assert_ne!(a.ciphertext_b64, b.ciphertext_b64);
    }

    #[test]
    fn invalid_utf8_decrypt_to_string_errors() {
        // Non-UTF-8 bytes must surface as Serialization, not silently lossy.
        let dk = test_device_key();
        let blob = EncryptedBlob::encrypt(&[0xFFu8, 0xFE, 0xFD], &dk).unwrap();
        assert!(matches!(
            blob.decrypt_to_string(&dk).unwrap_err(),
            CryptoError::Serialization(_)
        ));
        // But `decrypt` to bytes still works.
        assert_eq!(blob.decrypt(&dk).unwrap(), vec![0xFFu8, 0xFE, 0xFD]);
    }

    #[test]
    fn into_pair_and_back() {
        let dk = test_device_key();
        let blob = EncryptedBlob::encrypt(b"vault content", &dk).unwrap();
        let (ct_b64, nonce_b64) = blob.clone().into_pair();
        let restored = EncryptedBlob::from_pair(ct_b64, nonce_b64);
        assert_eq!(blob, restored);
        assert_eq!(restored.decrypt(&dk).unwrap(), b"vault content");
    }

    #[test]
    fn serde_round_trip() {
        // Confirms the type is wire-compatible with the schema fields, which
        // are also strings persisted by SurrealDB.
        let dk = test_device_key();
        let blob = EncryptedBlob::encrypt(b"json me", &dk).unwrap();
        let json = serde_json::to_string(&blob).unwrap();
        let back: EncryptedBlob = serde_json::from_str(&json).unwrap();
        assert_eq!(back.decrypt(&dk).unwrap(), b"json me");
    }
}
