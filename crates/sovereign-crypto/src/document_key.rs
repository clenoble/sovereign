use rand::Rng;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::aead::{self, KEY_SIZE, NONCE_SIZE};
use crate::error::CryptoResult;
use crate::kek::Kek;

/// Per-document encryption key. Wrapped by the KEK for storage.
///
/// Key hierarchy: Master → DeviceKey → KEK → **DocumentKey**
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct DocumentKey {
    bytes: [u8; KEY_SIZE],
}

/// A DocumentKey encrypted (wrapped) by a KEK, along with its rotation epoch.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WrappedDocumentKey {
    pub ciphertext: Vec<u8>,
    pub nonce: [u8; NONCE_SIZE],
    pub epoch: u32,
}

impl DocumentKey {
    /// Generate a fresh random document key.
    pub fn generate() -> Self {
        let mut bytes = [0u8; KEY_SIZE];
        rand::rng().fill_bytes(&mut bytes);
        Self { bytes }
    }

    /// Access the raw key bytes.
    pub fn as_bytes(&self) -> &[u8; KEY_SIZE] {
        &self.bytes
    }

    /// Wrap this document key with a KEK for storage. Tags it with a rotation epoch.
    pub fn wrap(&self, kek: &Kek, epoch: u32) -> CryptoResult<WrappedDocumentKey> {
        let (ciphertext, nonce) = aead::encrypt(&self.bytes, kek.as_bytes())?;
        Ok(WrappedDocumentKey {
            ciphertext,
            nonce,
            epoch,
        })
    }

    /// Unwrap a stored document key using a KEK.
    pub fn unwrap(wrapped: &WrappedDocumentKey, kek: &Kek) -> CryptoResult<Self> {
        let bytes_vec = aead::decrypt(&wrapped.ciphertext, &wrapped.nonce, kek.as_bytes())?;
        let mut bytes = [0u8; KEY_SIZE];
        bytes.copy_from_slice(&bytes_vec);
        Ok(Self { bytes })
    }

    /// Reconstruct from raw bytes.
    pub fn from_bytes(bytes: [u8; KEY_SIZE]) -> Self {
        Self { bytes }
    }
}

impl std::fmt::Debug for DocumentKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DocumentKey")
            .field("bytes", &"[REDACTED]")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_unwrap_roundtrip() {
        let kek = Kek::generate();
        let dk = DocumentKey::generate();
        let wrapped = dk.wrap(&kek, 1).unwrap();
        assert_eq!(wrapped.epoch, 1);
        let recovered = DocumentKey::unwrap(&wrapped, &kek).unwrap();
        assert_eq!(dk.as_bytes(), recovered.as_bytes());
    }

    #[test]
    fn wrong_kek_fails() {
        let kek1 = Kek::generate();
        let kek2 = Kek::generate();
        let dk = DocumentKey::generate();
        let wrapped = dk.wrap(&kek1, 1).unwrap();
        assert!(DocumentKey::unwrap(&wrapped, &kek2).is_err());
    }

    #[test]
    fn epoch_preserved() {
        let kek = Kek::generate();
        let dk = DocumentKey::generate();
        let wrapped = dk.wrap(&kek, 42).unwrap();
        assert_eq!(wrapped.epoch, 42);
    }

    #[test]
    fn wrapped_document_key_serializable() {
        let kek = Kek::generate();
        let dk = DocumentKey::generate();
        let wrapped = dk.wrap(&kek, 5).unwrap();
        let json = serde_json::to_string(&wrapped).unwrap();
        let back: WrappedDocumentKey = serde_json::from_str(&json).unwrap();
        let recovered = DocumentKey::unwrap(&back, &kek).unwrap();
        assert_eq!(dk.as_bytes(), recovered.as_bytes());
    }
}
