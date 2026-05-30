//! Vault primitive — encrypted blobs keyed to the user's `AccountKey`.
//!
//! The PII pipeline (see `doc/plans/pii-management-dashboard.md`) writes two
//! kinds of encrypted values, both keyed to the same user-scoped key:
//!
//! 1. `PiiRecord.value_encrypted` / `value_nonce` — vault entries
//!    (user-entered passwords, IBANs, etc.) and discovered findings.
//! 2. `Document.body_raw_encrypted` / `body_raw_nonce` and the matching
//!    `Message` fields — the original (pre-tokenization) body text,
//!    preserved for L3-gated reveal so LLM-NER false positives don't
//!    irreversibly mangle source content.
//!
//! As of v0.0.5 the cipher input is the `AccountKey`, not the per-device
//! `DeviceKey`. This makes encrypted-at-rest data portable across paired
//! devices that share a MasterKey. No per-document key is wrapped here,
//! unlike `key_db`/`document_key`. The blob format mirrors the schema's
//! `(ciphertext_b64, nonce_b64)` pair, so the type round-trips through
//! SurrealDB without bespoke conversion at every call site.

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};

use crate::account_key::AccountKey;
use crate::aead::{self, NONCE_SIZE};
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
    /// Encrypt arbitrary bytes under the account key.
    pub fn encrypt(plaintext: &[u8], account_key: &AccountKey) -> CryptoResult<Self> {
        let (ct, nonce) = aead::encrypt(plaintext, account_key.as_bytes())?;
        Ok(Self {
            ciphertext_b64: B64.encode(&ct),
            nonce_b64: B64.encode(nonce),
        })
    }

    /// Encrypt a UTF-8 string under the account key.
    pub fn encrypt_str(plaintext: &str, account_key: &AccountKey) -> CryptoResult<Self> {
        Self::encrypt(plaintext.as_bytes(), account_key)
    }

    /// Decrypt to raw bytes.
    pub fn decrypt(&self, account_key: &AccountKey) -> CryptoResult<Vec<u8>> {
        self.decrypt_with_key_bytes(account_key.as_bytes())
    }

    /// Decrypt and interpret as UTF-8.
    pub fn decrypt_to_string(&self, account_key: &AccountKey) -> CryptoResult<String> {
        let bytes = self.decrypt(account_key)?;
        String::from_utf8(bytes).map_err(|e| CryptoError::Serialization(e.to_string()))
    }

    /// Encrypt with raw key bytes — symmetric with `decrypt_with_key_bytes`,
    /// used by the v0.0.4→v0.0.5 migration tests to fabricate blobs that
    /// look like they were encrypted under the legacy DeviceKey. Production
    /// code should prefer `encrypt(&AccountKey)`.
    pub fn encrypt_with_key_bytes(plaintext: &[u8], key_bytes: &[u8; 32]) -> CryptoResult<Self> {
        let (ct, nonce) = aead::encrypt(plaintext, key_bytes)?;
        Ok(Self {
            ciphertext_b64: B64.encode(&ct),
            nonce_b64: B64.encode(nonce),
        })
    }

    /// Decrypt with raw key bytes — used by the v0.0.4→v0.0.5 migration
    /// to read blobs that were encrypted under the old per-device
    /// `DeviceKey` before re-encrypting them under the new `AccountKey`.
    /// Production code should prefer `decrypt(&AccountKey)`.
    pub fn decrypt_with_key_bytes(&self, key_bytes: &[u8; 32]) -> CryptoResult<Vec<u8>> {
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
        aead::decrypt(&ct, &nonce, key_bytes)
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

    fn test_account_key() -> AccountKey {
        let mk = MasterKey::from_passphrase(b"test", b"salt").unwrap();
        AccountKey::derive(&mk).unwrap()
    }

    #[test]
    fn round_trip_bytes() {
        let ak = test_account_key();
        let plaintext = b"my-iban-CH9300762011623852957";
        let blob = EncryptedBlob::encrypt(plaintext, &ak).unwrap();
        let recovered = blob.decrypt(&ak).unwrap();
        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn round_trip_string_utf8() {
        // Non-ASCII to confirm no byte-level mangling.
        let ak = test_account_key();
        let plaintext = "céline@example.ch — 北京";
        let blob = EncryptedBlob::encrypt_str(plaintext, &ak).unwrap();
        let recovered = blob.decrypt_to_string(&ak).unwrap();
        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn empty_plaintext_round_trip() {
        let ak = test_account_key();
        let blob = EncryptedBlob::encrypt(b"", &ak).unwrap();
        assert!(blob.decrypt(&ak).unwrap().is_empty());
    }

    #[test]
    fn large_payload_round_trip() {
        let ak = test_account_key();
        let plaintext = vec![0xA5u8; 256 * 1024];
        let blob = EncryptedBlob::encrypt(&plaintext, &ak).unwrap();
        assert_eq!(blob.decrypt(&ak).unwrap(), plaintext);
    }

    #[test]
    fn wrong_account_key_fails() {
        let mk_a = MasterKey::from_passphrase(b"good", b"salt").unwrap();
        let ak_a = AccountKey::derive(&mk_a).unwrap();
        let mk_b = MasterKey::from_passphrase(b"bad", b"salt").unwrap();
        let ak_b = AccountKey::derive(&mk_b).unwrap();
        let blob = EncryptedBlob::encrypt(b"secret", &ak_a).unwrap();
        assert!(matches!(
            blob.decrypt(&ak_b).unwrap_err(),
            CryptoError::DecryptionFailed
        ));
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let ak = test_account_key();
        let mut blob = EncryptedBlob::encrypt(b"secret", &ak).unwrap();
        let mut ct = B64.decode(&blob.ciphertext_b64).unwrap();
        ct[0] ^= 0xff;
        blob.ciphertext_b64 = B64.encode(&ct);
        assert!(matches!(
            blob.decrypt(&ak).unwrap_err(),
            CryptoError::DecryptionFailed
        ));
    }

    #[test]
    fn invalid_nonce_length_rejected() {
        let ak = test_account_key();
        let blob = EncryptedBlob {
            ciphertext_b64: B64.encode(b"abc"),
            nonce_b64: B64.encode(b"too short"),
        };
        match blob.decrypt(&ak).unwrap_err() {
            CryptoError::InvalidNonceLength { expected, got } => {
                assert_eq!(expected, NONCE_SIZE);
                assert_eq!(got, 9);
            }
            other => panic!("expected InvalidNonceLength, got {other:?}"),
        }
    }

    #[test]
    fn invalid_base64_rejected() {
        let ak = test_account_key();
        let blob = EncryptedBlob {
            ciphertext_b64: "!!! not base64 !!!".into(),
            nonce_b64: "also bad".into(),
        };
        assert!(matches!(
            blob.decrypt(&ak).unwrap_err(),
            CryptoError::Base64(_)
        ));
    }

    #[test]
    fn nonces_unique_across_encrypts() {
        let ak = test_account_key();
        let plaintext = b"same plaintext";
        let a = EncryptedBlob::encrypt(plaintext, &ak).unwrap();
        let b = EncryptedBlob::encrypt(plaintext, &ak).unwrap();
        assert_ne!(a.nonce_b64, b.nonce_b64);
        assert_ne!(a.ciphertext_b64, b.ciphertext_b64);
    }

    #[test]
    fn invalid_utf8_decrypt_to_string_errors() {
        let ak = test_account_key();
        let blob = EncryptedBlob::encrypt(&[0xFFu8, 0xFE, 0xFD], &ak).unwrap();
        assert!(matches!(
            blob.decrypt_to_string(&ak).unwrap_err(),
            CryptoError::Serialization(_)
        ));
        assert_eq!(blob.decrypt(&ak).unwrap(), vec![0xFFu8, 0xFE, 0xFD]);
    }

    #[test]
    fn into_pair_and_back() {
        let ak = test_account_key();
        let blob = EncryptedBlob::encrypt(b"vault content", &ak).unwrap();
        let (ct_b64, nonce_b64) = blob.clone().into_pair();
        let restored = EncryptedBlob::from_pair(ct_b64, nonce_b64);
        assert_eq!(blob, restored);
        assert_eq!(restored.decrypt(&ak).unwrap(), b"vault content");
    }

    #[test]
    fn serde_round_trip() {
        let ak = test_account_key();
        let blob = EncryptedBlob::encrypt(b"json me", &ak).unwrap();
        let json = serde_json::to_string(&blob).unwrap();
        let back: EncryptedBlob = serde_json::from_str(&json).unwrap();
        assert_eq!(back.decrypt(&ak).unwrap(), b"json me");
    }

    #[test]
    fn account_key_portable_across_devices() {
        // Two paired devices share the same MasterKey (same passphrase
        // and salt) but have different device_ids. AccountKey doesn't
        // include device_id, so blobs encrypted on device A decrypt on
        // device B. This is the v0.0.5 sync invariant.
        let mk = MasterKey::from_passphrase(b"shared", b"shared-salt").unwrap();
        let ak_a = AccountKey::derive(&mk).unwrap();
        let ak_b = AccountKey::derive(&mk).unwrap();
        let blob = EncryptedBlob::encrypt(b"sync me", &ak_a).unwrap();
        assert_eq!(blob.decrypt(&ak_b).unwrap(), b"sync me");
    }

    #[test]
    fn decrypt_with_key_bytes_matches_decrypt() {
        // The migration path needs to decrypt under raw bytes from the
        // old DeviceKey. Verify both APIs agree.
        let ak = test_account_key();
        let blob = EncryptedBlob::encrypt(b"migrate me", &ak).unwrap();
        assert_eq!(
            blob.decrypt_with_key_bytes(ak.as_bytes()).unwrap(),
            blob.decrypt(&ak).unwrap()
        );
    }
}
