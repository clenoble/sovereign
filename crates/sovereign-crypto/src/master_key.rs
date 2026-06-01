use hkdf::Hkdf;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::aead::KEY_SIZE;
use crate::error::{CryptoError, CryptoResult};

/// Key-derivation function used to stretch the user passphrase into the
/// MasterKey. Recorded in the AuthStore so an existing store always
/// unlocks with the KDF it was created under (forward+backward compat).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kdf {
    /// Legacy single-pass HKDF-SHA256 (v<=0.0.6). **Not** a password KDF —
    /// no memory/time hardness, so it is offline-brute-forceable. Retained
    /// ONLY to unlock pre-existing stores + the v0.0.4 data migration;
    /// never selected for a new store.
    LegacyHkdf,
    /// Argon2id password stretching. `m_cost_kib` = memory in KiB,
    /// `t_cost` = iterations, `p_cost` = lanes.
    Argon2id { m_cost_kib: u32, t_cost: u32, p_cost: u32 },
}

impl Kdf {
    /// The KDF + params used for every NEW auth store. ~64 MiB / 3 passes
    /// makes offline brute-force of the passphrase prohibitively expensive
    /// (hundreds of ms per guess vs microseconds for HKDF).
    pub fn current() -> Self {
        Kdf::Argon2id {
            m_cost_kib: 64 * 1024,
            t_cost: 3,
            p_cost: 1,
        }
    }

    /// serde `default` for auth stores written before the `kdf` field
    /// existed — those were all HKDF.
    pub fn legacy() -> Self {
        Kdf::LegacyHkdf
    }
}

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
    ///
    /// LEGACY: single-pass HKDF has no brute-force hardness. Retained only
    /// to unlock pre-`kdf`-field stores and to re-derive the OLD key during
    /// the v0.0.4 data migration. New stores use [`MasterKey::derive`] with
    /// [`Kdf::current`] (Argon2id). See CRYPTO-001.
    pub fn from_passphrase(passphrase: &[u8], salt: &[u8]) -> CryptoResult<Self> {
        let hk = Hkdf::<Sha256>::new(Some(salt), passphrase);
        let mut bytes = [0u8; KEY_SIZE];
        hk.expand(b"sovereign-master-key", &mut bytes)
            .map_err(|e| CryptoError::DerivationFailed(e.to_string()))?;
        Ok(Self { bytes })
    }

    /// Derive a master key using the given [`Kdf`]. This is the
    /// version-aware entry point: a store records the `Kdf` it was created
    /// under and re-derives with the same one. Argon2id is the current
    /// default for new stores; `LegacyHkdf` is dispatched to
    /// [`MasterKey::from_passphrase`] for backward compatibility.
    pub fn derive(passphrase: &[u8], salt: &[u8], kdf: &Kdf) -> CryptoResult<Self> {
        match kdf {
            Kdf::LegacyHkdf => Self::from_passphrase(passphrase, salt),
            Kdf::Argon2id {
                m_cost_kib,
                t_cost,
                p_cost,
            } => {
                use argon2::{Algorithm, Argon2, Params, Version};
                let params = Params::new(*m_cost_kib, *t_cost, *p_cost, Some(KEY_SIZE))
                    .map_err(|e| CryptoError::DerivationFailed(format!("argon2 params: {e}")))?;
                let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
                let mut bytes = [0u8; KEY_SIZE];
                argon
                    .hash_password_into(passphrase, salt, &mut bytes)
                    .map_err(|e| CryptoError::DerivationFailed(format!("argon2: {e}")))?;
                Ok(Self { bytes })
            }
        }
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

    #[test]
    fn argon2id_deterministic_and_differs_from_hkdf() {
        // CRYPTO-001: new stores stretch the passphrase with Argon2id.
        let salt = b"test-salt-32-bytes-long-padding!";
        let cur = Kdf::current();
        assert!(matches!(cur, Kdf::Argon2id { .. }));
        let a = MasterKey::derive(b"pw", salt, &cur).unwrap();
        let b = MasterKey::derive(b"pw", salt, &cur).unwrap();
        assert_eq!(a.as_bytes(), b.as_bytes(), "Argon2id must be deterministic");

        // The whole point: Argon2id output != single-pass HKDF output, so a
        // captured store derived under the new KDF is not the cheap oracle.
        let legacy = MasterKey::derive(b"pw", salt, &Kdf::LegacyHkdf).unwrap();
        assert_ne!(a.as_bytes(), legacy.as_bytes());

        // LegacyHkdf dispatch is exactly from_passphrase (backward compat).
        let direct = MasterKey::from_passphrase(b"pw", salt).unwrap();
        assert_eq!(legacy.as_bytes(), direct.as_bytes());
    }
}
