use rand::Rng;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::aead::{self, KEY_SIZE, NONCE_SIZE};
use crate::device_key::DeviceKey;
use crate::error::CryptoResult;

/// Key-Encryption Key: wraps/unwraps per-document keys.
///
/// Key hierarchy: Master → DeviceKey → **KEK** → DocumentKey
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct Kek {
    bytes: [u8; KEY_SIZE],
}

/// A KEK encrypted (wrapped) by a DeviceKey.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WrappedKek {
    pub ciphertext: Vec<u8>,
    pub nonce: [u8; NONCE_SIZE],
}

impl Kek {
    /// Generate a fresh random KEK.
    pub fn generate() -> Self {
        let mut bytes = [0u8; KEY_SIZE];
        rand::rng().fill_bytes(&mut bytes);
        Self { bytes }
    }

    /// Access the raw key bytes.
    pub fn as_bytes(&self) -> &[u8; KEY_SIZE] {
        &self.bytes
    }

    /// Wrap this KEK with a DeviceKey for storage.
    pub fn wrap(&self, device_key: &DeviceKey) -> CryptoResult<WrappedKek> {
        let (ciphertext, nonce) = aead::encrypt(&self.bytes, device_key.as_bytes())?;
        Ok(WrappedKek { ciphertext, nonce })
    }

    /// Unwrap a stored KEK using a DeviceKey.
    pub fn unwrap(wrapped: &WrappedKek, device_key: &DeviceKey) -> CryptoResult<Self> {
        let bytes_vec = aead::decrypt(&wrapped.ciphertext, &wrapped.nonce, device_key.as_bytes())?;
        let mut bytes = [0u8; KEY_SIZE];
        bytes.copy_from_slice(&bytes_vec);
        Ok(Self { bytes })
    }

    /// Reconstruct from raw bytes.
    pub fn from_bytes(bytes: [u8; KEY_SIZE]) -> Self {
        Self { bytes }
    }
}

impl std::fmt::Debug for Kek {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Kek")
            .field("bytes", &"[REDACTED]")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::master_key::MasterKey;

    fn test_device_key() -> DeviceKey {
        let mk = MasterKey::from_passphrase(b"test", b"salt").unwrap();
        DeviceKey::derive(&mk, "dev-01").unwrap()
    }

    #[test]
    fn wrap_unwrap_roundtrip() {
        let dk = test_device_key();
        let kek = Kek::generate();
        let wrapped = kek.wrap(&dk).unwrap();
        let recovered = Kek::unwrap(&wrapped, &dk).unwrap();
        assert_eq!(kek.as_bytes(), recovered.as_bytes());
    }

    #[test]
    fn wrong_device_key_fails() {
        let mk1 = MasterKey::from_passphrase(b"pass-a", b"salt").unwrap();
        let mk2 = MasterKey::from_passphrase(b"pass-b", b"salt").unwrap();
        let dk1 = DeviceKey::derive(&mk1, "dev-01").unwrap();
        let dk2 = DeviceKey::derive(&mk2, "dev-01").unwrap();

        let kek = Kek::generate();
        let wrapped = kek.wrap(&dk1).unwrap();
        assert!(Kek::unwrap(&wrapped, &dk2).is_err());
    }

    #[test]
    fn wrapped_kek_serializable() {
        let dk = test_device_key();
        let kek = Kek::generate();
        let wrapped = kek.wrap(&dk).unwrap();
        let json = serde_json::to_string(&wrapped).unwrap();
        let back: WrappedKek = serde_json::from_str(&json).unwrap();
        let recovered = Kek::unwrap(&back, &dk).unwrap();
        assert_eq!(kek.as_bytes(), recovered.as_bytes());
    }
}
