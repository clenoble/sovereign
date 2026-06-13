//! Blind-index key: a 32-byte HMAC-SHA256 secret used to derive deterministic
//! token hashes for encrypted-field search.
//!
//! The same plaintext token always hashes to the same value under one key, so
//! ciphertext rows can be queried by hash without exposing the plaintext. This
//! leaks token-frequency to anyone who reads the DB file but is strictly better
//! than plaintext bodies for our local-disk threat model.
//!
//! Key hierarchy: Master → DeviceKey → KEK → **IndexKey**
//! Stored wrapped under the KEK using the same `WrappedDocumentKey` wire format.

use std::path::{Path, PathBuf};

use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::aead::{KEY_SIZE, NONCE_SIZE};
use crate::device_key::DeviceKey;
use crate::document_key::{DocumentKey, WrappedDocumentKey};
use crate::error::{CryptoError, CryptoResult};
use crate::kek::Kek;
use crate::{aead};

type HmacSha256 = Hmac<Sha256>;

/// Per-DB HMAC key for deterministic token derivation.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct IndexKey {
    bytes: [u8; KEY_SIZE],
}

impl IndexKey {
    /// Generate a fresh random index key.
    pub fn generate() -> Self {
        // Reuse DocumentKey::generate to avoid a second RNG path.
        let dk = DocumentKey::generate();
        let mut bytes = [0u8; KEY_SIZE];
        bytes.copy_from_slice(dk.as_bytes());
        Self { bytes }
    }

    pub fn as_bytes(&self) -> &[u8; KEY_SIZE] {
        &self.bytes
    }

    /// HMAC-SHA256 of `data` under this key, base64-encoded.
    ///
    /// CRYPTO-004 (accepted tradeoff): this is a DETERMINISTIC blind index — the
    /// same plaintext token always hashes to the same value, which is exactly
    /// what makes equality search over encrypted-at-rest data possible. The cost
    /// is that anyone who can read the stored hashes can do frequency analysis
    /// and confirm the presence of a *guessed* token (chosen-plaintext) across
    /// rows. It never reveals un-guessed plaintext, and the single-DB scope
    /// bounds it (no cross-DB correlation). If this needs hardening later,
    /// salt/bucket the hashes or index bigrams instead of whole tokens.
    pub fn hash_token(&self, data: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(&self.bytes)
            .expect("HMAC accepts any key length");
        mac.update(data);
        let result = mac.finalize().into_bytes();
        base64::engine::general_purpose::STANDARD.encode(result)
    }

    /// Load + decrypt an IndexKey file. File format mirrors KeyDatabase:
    /// `nonce (24 bytes) || ciphertext(JSON{WrappedDocumentKey})`.
    /// The outer layer is encrypted by DeviceKey; the inner WrappedDocumentKey
    /// is unwrapped with the KEK to yield the 32-byte HMAC secret.
    pub fn load(path: &Path, device_key: &DeviceKey, kek: &Kek) -> CryptoResult<Self> {
        let data = std::fs::read(path)
            .map_err(|e| CryptoError::KeyDbIo(e.to_string()))?;
        if data.len() < NONCE_SIZE {
            return Err(CryptoError::KeyDbIo("index key file too short".into()));
        }

        let mut nonce = [0u8; NONCE_SIZE];
        nonce.copy_from_slice(&data[..NONCE_SIZE]);
        let ciphertext = &data[NONCE_SIZE..];

        let plaintext = aead::decrypt(ciphertext, &nonce, device_key.as_bytes())?;
        let wrapped: WrappedDocumentKey = serde_json::from_slice(&plaintext)
            .map_err(|e| CryptoError::Serialization(e.to_string()))?;

        let dk = DocumentKey::unwrap(&wrapped, kek)?;
        let mut bytes = [0u8; KEY_SIZE];
        bytes.copy_from_slice(dk.as_bytes());
        Ok(Self { bytes })
    }

    /// Save this key wrapped under KEK and outer-encrypted by DeviceKey.
    pub fn save(&self, path: &Path, device_key: &DeviceKey, kek: &Kek) -> CryptoResult<()> {
        let dk = DocumentKey::from_bytes(self.bytes);
        let wrapped = dk.wrap(kek, 1)?;
        let json = serde_json::to_vec(&wrapped)
            .map_err(|e| CryptoError::Serialization(e.to_string()))?;

        let (ciphertext, nonce) = aead::encrypt(&json, device_key.as_bytes())?;
        let mut output = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
        output.extend_from_slice(&nonce);
        output.extend_from_slice(&ciphertext);

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| CryptoError::KeyDbIo(e.to_string()))?;
        }
        crate::fs_private::write_private(path, &output)
            .map_err(|e| CryptoError::KeyDbIo(e.to_string()))?;
        Ok(())
    }

    /// Load if the file exists, otherwise generate-and-save a fresh key.
    pub fn load_or_create(
        path: PathBuf,
        device_key: &DeviceKey,
        kek: &Kek,
    ) -> CryptoResult<Self> {
        if path.exists() {
            Self::load(&path, device_key, kek)
        } else {
            let key = Self::generate();
            key.save(&path, device_key, kek)?;
            Ok(key)
        }
    }
}

impl std::fmt::Debug for IndexKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IndexKey").field("bytes", &"[REDACTED]").finish()
    }
}

/// Tokenize a plaintext string for blind-index storage.
///
/// Lowercases, splits on non-alphanumeric, drops tokens shorter than 3 chars,
/// dedupes, caps at `max_tokens`. Unicode-naive — sufficient for English-ish
/// chat search; trigram support can be layered on later.
pub fn tokenize(text: &str, max_tokens: usize) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for raw in text.split(|c: char| !c.is_alphanumeric()) {
        if raw.len() < 3 {
            continue;
        }
        let lower = raw.to_lowercase();
        if seen.insert(lower.clone()) {
            out.push(lower);
            if out.len() >= max_tokens {
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::master_key::MasterKey;

    fn test_keys() -> (DeviceKey, Kek) {
        let mk = MasterKey::from_passphrase(b"test", b"salt").unwrap();
        let dk = DeviceKey::derive(&mk, "dev-01").unwrap();
        let kek = Kek::generate();
        (dk, kek)
    }

    #[test]
    fn hash_is_deterministic() {
        let k = IndexKey::generate();
        assert_eq!(k.hash_token(b"hello"), k.hash_token(b"hello"));
        assert_ne!(k.hash_token(b"hello"), k.hash_token(b"world"));
    }

    #[test]
    fn different_keys_produce_different_hashes() {
        let a = IndexKey::generate();
        let b = IndexKey::generate();
        assert_ne!(a.hash_token(b"hello"), b.hash_token(b"hello"));
    }

    #[test]
    fn save_load_roundtrip() {
        let (dk, kek) = test_keys();
        let dir = std::env::temp_dir().join("sovereign-crypto-test-indexkey");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("index.key");

        let k1 = IndexKey::generate();
        let h_before = k1.hash_token(b"sample");
        k1.save(&path, &dk, &kek).unwrap();

        let k2 = IndexKey::load(&path, &dk, &kek).unwrap();
        assert_eq!(h_before, k2.hash_token(b"sample"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wrong_kek_cannot_load() {
        let (dk, kek) = test_keys();
        let dir = std::env::temp_dir().join("sovereign-crypto-test-indexkey-wrong");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("index.key");

        IndexKey::generate().save(&path, &dk, &kek).unwrap();
        let wrong_kek = Kek::generate();
        assert!(IndexKey::load(&path, &dk, &wrong_kek).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_or_create_idempotent() {
        let (dk, kek) = test_keys();
        let dir = std::env::temp_dir().join("sovereign-crypto-test-indexkey-loc");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("index.key");

        let k1 = IndexKey::load_or_create(path.clone(), &dk, &kek).unwrap();
        let h1 = k1.hash_token(b"x");
        let k2 = IndexKey::load_or_create(path.clone(), &dk, &kek).unwrap();
        let h2 = k2.hash_token(b"x");
        assert_eq!(h1, h2);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tokenize_basics() {
        let toks = tokenize("Hello, world! This is a TEST of the system.", 100);
        assert!(toks.contains(&"hello".to_string()));
        assert!(toks.contains(&"world".to_string()));
        assert!(toks.contains(&"test".to_string()));
        assert!(toks.contains(&"the".to_string()));
        assert!(toks.contains(&"system".to_string()));
        // Length < 3 dropped
        assert!(!toks.contains(&"is".to_string()));
        assert!(!toks.contains(&"a".to_string()));
        // Dedupe — only one "test"
        assert_eq!(toks.iter().filter(|t| *t == "test").count(), 1);
    }

    #[test]
    fn tokenize_cap_enforced() {
        let text: String = (0..50).map(|i| format!("word{} ", i)).collect();
        let toks = tokenize(&text, 10);
        assert_eq!(toks.len(), 10);
    }
}
