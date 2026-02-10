use hkdf::Hkdf;
use rand::Rng;
use sha2::Sha256;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::aead::KEY_SIZE;
use crate::error::{CryptoError, CryptoResult};

/// The root of the key hierarchy. 256-bit master secret.
///
/// In production, this would be backed by a TPM. For WSL2 development,
/// it is derived from a passphrase via HKDF.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct MasterKey {
    bytes: [u8; KEY_SIZE],
}

impl MasterKey {
    /// Generate a random master key using the system CSPRNG.
    pub fn generate() -> Self {
        let mut bytes = [0u8; KEY_SIZE];
        rand::rng().fill_bytes(&mut bytes);
        Self { bytes }
    }

    /// Derive a master key from a passphrase and salt via HKDF-SHA256.
    /// This is the WSL2/development fallback; production would use TPM.
    pub fn from_passphrase(passphrase: &[u8], salt: &[u8]) -> CryptoResult<Self> {
        let hk = Hkdf::<Sha256>::new(Some(salt), passphrase);
        let mut bytes = [0u8; KEY_SIZE];
        hk.expand(b"sovereign-master-key", &mut bytes)
            .map_err(|e| CryptoError::DerivationFailed(e.to_string()))?;
        Ok(Self { bytes })
    }

    /// Access the raw key bytes.
    pub fn as_bytes(&self) -> &[u8; KEY_SIZE] {
        &self.bytes
    }

    /// Reconstruct a MasterKey from raw bytes (used during Guardian recovery).
    pub fn from_bytes(bytes: [u8; KEY_SIZE]) -> Self {
        Self { bytes }
    }
}

impl std::fmt::Debug for MasterKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MasterKey")
            .field("bytes", &"[REDACTED]")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_produces_nonzero() {
        let mk = MasterKey::generate();
        assert_ne!(mk.as_bytes(), &[0u8; KEY_SIZE]);
    }

    #[test]
    fn passphrase_derivation_deterministic() {
        let salt = b"test-salt-12345678";
        let mk1 = MasterKey::from_passphrase(b"my passphrase", salt).unwrap();
        let mk2 = MasterKey::from_passphrase(b"my passphrase", salt).unwrap();
        assert_eq!(mk1.as_bytes(), mk2.as_bytes());
    }

    #[test]
    fn different_passphrases_differ() {
        let salt = b"test-salt-12345678";
        let mk1 = MasterKey::from_passphrase(b"passphrase A", salt).unwrap();
        let mk2 = MasterKey::from_passphrase(b"passphrase B", salt).unwrap();
        assert_ne!(mk1.as_bytes(), mk2.as_bytes());
    }

    #[test]
    fn different_salts_differ() {
        let mk1 = MasterKey::from_passphrase(b"same", b"salt-one").unwrap();
        let mk2 = MasterKey::from_passphrase(b"same", b"salt-two").unwrap();
        assert_ne!(mk1.as_bytes(), mk2.as_bytes());
    }

    #[test]
    fn from_bytes_roundtrip() {
        let mk = MasterKey::generate();
        let bytes = *mk.as_bytes();
        let mk2 = MasterKey::from_bytes(bytes);
        assert_eq!(mk.as_bytes(), mk2.as_bytes());
    }

    #[test]
    fn debug_redacts_key() {
        let mk = MasterKey::generate();
        let dbg = format!("{:?}", mk);
        assert!(dbg.contains("REDACTED"));
        assert!(!dbg.contains(&format!("{:?}", mk.as_bytes())));
    }
}
