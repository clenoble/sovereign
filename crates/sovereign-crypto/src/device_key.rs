use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::aead::KEY_SIZE;
use crate::error::{CryptoError, CryptoResult};
use crate::master_key::MasterKey;

/// Per-device key derived from the master key via HKDF.
///
/// Key hierarchy: Master → DeviceKey (per device_id) → KEK → DocumentKey
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct DeviceKey {
    bytes: [u8; KEY_SIZE],
}

impl DeviceKey {
    /// Derive a device key from a master key and device identifier.
    pub fn derive(master: &MasterKey, device_id: &str) -> CryptoResult<Self> {
        let hk = Hkdf::<Sha256>::new(None, master.as_bytes());
        let mut bytes = [0u8; KEY_SIZE];
        let info = format!("sovereign-device-key:{}", device_id);
        hk.expand(info.as_bytes(), &mut bytes)
            .map_err(|e| CryptoError::DerivationFailed(e.to_string()))?;
        Ok(Self { bytes })
    }

    /// Access the raw key bytes.
    pub fn as_bytes(&self) -> &[u8; KEY_SIZE] {
        &self.bytes
    }
}

impl std::fmt::Debug for DeviceKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceKey")
            .field("bytes", &"[REDACTED]")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_deterministic() {
        let mk = MasterKey::from_passphrase(b"test", b"salt").unwrap();
        let dk1 = DeviceKey::derive(&mk, "device-001").unwrap();
        let dk2 = DeviceKey::derive(&mk, "device-001").unwrap();
        assert_eq!(dk1.as_bytes(), dk2.as_bytes());
    }

    #[test]
    fn different_devices_differ() {
        let mk = MasterKey::from_passphrase(b"test", b"salt").unwrap();
        let dk1 = DeviceKey::derive(&mk, "device-001").unwrap();
        let dk2 = DeviceKey::derive(&mk, "device-002").unwrap();
        assert_ne!(dk1.as_bytes(), dk2.as_bytes());
    }

    #[test]
    fn different_masters_differ() {
        let mk1 = MasterKey::from_passphrase(b"pass-a", b"salt").unwrap();
        let mk2 = MasterKey::from_passphrase(b"pass-b", b"salt").unwrap();
        let dk1 = DeviceKey::derive(&mk1, "device-001").unwrap();
        let dk2 = DeviceKey::derive(&mk2, "device-001").unwrap();
        assert_ne!(dk1.as_bytes(), dk2.as_bytes());
    }
}
