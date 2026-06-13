//! User-scoped at-rest encryption key.
//!
//! AccountKey is derived from MasterKey *without* a device_id, so two
//! devices belonging to the same user (same passphrase + same salt)
//! derive the same AccountKey. This is the key used to encrypt:
//!   - PII vault entries (`PiiRecord.value_encrypted`)
//!   - Document `body_raw_encrypted`
//!   - Message `body_raw_encrypted`
//!   - Session log entries (via HKDF expansion)
//!
//! Per-device responsibilities (libp2p identity, KEK wrapping) stay on
//! the existing `DeviceKey`. See the v0.0.5 sync plan for the full
//! key hierarchy.

use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::aead::{self, KEY_SIZE, NONCE_SIZE};
use crate::device_key::DeviceKey;
use crate::error::{CryptoError, CryptoResult};
use crate::master_key::MasterKey;

/// User-scoped key for at-rest encryption of vault, body_raw, and session log.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct AccountKey {
    bytes: [u8; KEY_SIZE],
}

impl AccountKey {
    /// Derive an AccountKey from the user's MasterKey.
    ///
    /// HKDF info string is versioned (`:v1`) so future hierarchy
    /// changes can coexist with existing on-disk data.
    pub fn derive(master: &MasterKey) -> CryptoResult<Self> {
        let hk = Hkdf::<Sha256>::new(None, master.as_bytes());
        let mut bytes = [0u8; KEY_SIZE];
        hk.expand(b"sovereign-account-key:v1", &mut bytes)
            .map_err(|e| CryptoError::DerivationFailed(e.to_string()))?;
        Ok(Self { bytes })
    }

    /// Construct from raw bytes — used by the pairing flow when the
    /// AccountKey is imported from another device rather than derived
    /// from a local passphrase.
    pub fn from_bytes(bytes: [u8; KEY_SIZE]) -> Self {
        Self { bytes }
    }

    /// Derive the P2P sync **transport key** from this AccountKey.
    ///
    /// All of a user's paired devices share the same AccountKey (imported
    /// out-of-band during pairing), so they all derive the *same* transport
    /// key and can encrypt/decrypt each other's sync envelopes without an
    /// interactive key exchange. Domain-separated (`:v1` info string) from
    /// the at-rest material so the transport key is independent of the
    /// vault/body/session keys. See P2P-002.
    pub fn derive_transport_key(&self) -> [u8; KEY_SIZE] {
        let hk = Hkdf::<Sha256>::new(None, &self.bytes);
        let mut out = [0u8; KEY_SIZE];
        hk.expand(b"sovereign-p2p-transport-key:v1", &mut out)
            .expect("32 bytes is within HKDF output limit");
        out
    }

    /// Derive the sealing key for one device **pair** (P1.4 / P2P-005).
    ///
    /// Replaces the single account-wide transport key for row/commit
    /// envelopes: each pair of devices gets a distinct key, so captured
    /// traffic between one pair can't be opened with another pair's key,
    /// and unpairing a device retires exactly its pair keys. The peer ids
    /// are sorted before derivation, so both ends derive the same key
    /// without an interactive exchange.
    ///
    /// Caveat (until the P3.1 handshake lands): the key is derivable by
    /// any holder of the AccountKey, so this is isolation between pairs,
    /// not full per-device crypto revocation — that needs the fresh-ECDH
    /// pair exchange.
    pub fn derive_pair_key(&self, peer_a: &str, peer_b: &str) -> [u8; KEY_SIZE] {
        let (lo, hi) = if peer_a <= peer_b {
            (peer_a, peer_b)
        } else {
            (peer_b, peer_a)
        };
        let hk = Hkdf::<Sha256>::new(None, &self.bytes);
        let mut out = [0u8; KEY_SIZE];
        let info = format!("sovereign-p2p-pair-key:v1:{lo}:{hi}");
        hk.expand(info.as_bytes(), &mut out)
            .expect("32 bytes is within HKDF output limit");
        out
    }

    /// Derive the public **backup owner tag** (P4): the identifier backup
    /// hosts file this account's snapshot fragments under, and the lookup
    /// key a recovering device presents to fetch them. Derivable only
    /// from the AccountKey (passphrase + salt), but treated as PUBLIC —
    /// fragments are opaque ciphertext whose key is Shamir-split across
    /// guardians, so knowing the tag yields nothing decryptable.
    pub fn derive_backup_tag(&self) -> String {
        let hk = Hkdf::<Sha256>::new(None, &self.bytes);
        let mut out = [0u8; 16];
        hk.expand(b"sovereign-backup-owner-tag:v1", &mut out)
            .expect("16 bytes is within HKDF output limit");
        out.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Access the raw key bytes.
    pub fn as_bytes(&self) -> &[u8; KEY_SIZE] {
        &self.bytes
    }

    /// Wrap this AccountKey with a per-device DeviceKey for storage in
    /// the auth.store. Used by the pairing flow: a paired device imports
    /// the AccountKey out-of-band (QR + PIN), then wraps it under its
    /// own freshly-derived DeviceKey so subsequent logins reproduce it
    /// without needing the master passphrase that derived it elsewhere.
    pub fn wrap(&self, device_key: &DeviceKey) -> CryptoResult<WrappedAccountKey> {
        let (ciphertext, nonce) = aead::encrypt(&self.bytes, device_key.as_bytes())?;
        Ok(WrappedAccountKey { ciphertext, nonce })
    }

    /// Unwrap a stored AccountKey using a DeviceKey.
    pub fn unwrap_with(
        wrapped: &WrappedAccountKey,
        device_key: &DeviceKey,
    ) -> CryptoResult<Self> {
        let bytes_vec =
            aead::decrypt(&wrapped.ciphertext, &wrapped.nonce, device_key.as_bytes())?;
        let mut bytes = [0u8; KEY_SIZE];
        bytes.copy_from_slice(&bytes_vec);
        Ok(Self { bytes })
    }
}

/// An AccountKey encrypted (wrapped) by a per-device DeviceKey, suitable
/// for storage in the auth.store. Symmetric with [`WrappedKek`](crate::kek::WrappedKek).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WrappedAccountKey {
    pub ciphertext: Vec<u8>,
    pub nonce: [u8; NONCE_SIZE],
}

impl std::fmt::Debug for AccountKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccountKey")
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
        let ak1 = AccountKey::derive(&mk).unwrap();
        let ak2 = AccountKey::derive(&mk).unwrap();
        assert_eq!(ak1.as_bytes(), ak2.as_bytes());
    }

    #[test]
    fn different_masters_differ() {
        let mk1 = MasterKey::from_passphrase(b"pass-a", b"salt").unwrap();
        let mk2 = MasterKey::from_passphrase(b"pass-b", b"salt").unwrap();
        let ak1 = AccountKey::derive(&mk1).unwrap();
        let ak2 = AccountKey::derive(&mk2).unwrap();
        assert_ne!(ak1.as_bytes(), ak2.as_bytes());
    }

    #[test]
    fn user_scoped_not_device_scoped() {
        // Two devices, same passphrase, same salt → same AccountKey.
        // (Compare against DeviceKey which differs per device_id.)
        use crate::device_key::DeviceKey;
        let mk = MasterKey::from_passphrase(b"test", b"shared-salt").unwrap();

        let ak_device_a = AccountKey::derive(&mk).unwrap();
        let ak_device_b = AccountKey::derive(&mk).unwrap();
        assert_eq!(
            ak_device_a.as_bytes(),
            ak_device_b.as_bytes(),
            "AccountKey must be identical across devices that share a MasterKey"
        );

        let dk_device_a = DeviceKey::derive(&mk, "device-001").unwrap();
        let dk_device_b = DeviceKey::derive(&mk, "device-002").unwrap();
        assert_ne!(
            dk_device_a.as_bytes(),
            dk_device_b.as_bytes(),
            "DeviceKey must differ across devices with different device_ids"
        );
    }

    #[test]
    fn from_bytes_roundtrip() {
        let raw = [0xAB; KEY_SIZE];
        let ak = AccountKey::from_bytes(raw);
        assert_eq!(ak.as_bytes(), &raw);
    }

    #[test]
    fn transport_key_deterministic_shared_distinct_from_account_key() {
        // P2P-002: two paired devices share the AccountKey → same transport
        // key (so they can decrypt each other's sync envelopes), and the
        // transport key is domain-separated from the AccountKey itself.
        let mk = MasterKey::from_passphrase(b"test", b"shared-salt").unwrap();
        let ak_a = AccountKey::derive(&mk).unwrap();
        let ak_b = AccountKey::derive(&mk).unwrap();
        assert_eq!(
            ak_a.derive_transport_key(),
            ak_b.derive_transport_key(),
            "paired devices must derive the same transport key"
        );
        assert_ne!(
            &ak_a.derive_transport_key(),
            ak_a.as_bytes(),
            "transport key must be domain-separated from the AccountKey"
        );

        // A different account → different transport key.
        let mk2 = MasterKey::from_passphrase(b"other", b"shared-salt").unwrap();
        let ak2 = AccountKey::derive(&mk2).unwrap();
        assert_ne!(ak_a.derive_transport_key(), ak2.derive_transport_key());
    }

    #[test]
    fn backup_tag_deterministic_and_account_scoped() {
        let mk = MasterKey::from_passphrase(b"test", b"shared-salt").unwrap();
        let ak = AccountKey::derive(&mk).unwrap();
        let tag = ak.derive_backup_tag();
        assert_eq!(tag.len(), 32, "16 bytes hex-encoded");
        assert!(tag.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(tag, ak.derive_backup_tag(), "deterministic");

        let mk2 = MasterKey::from_passphrase(b"other", b"shared-salt").unwrap();
        let ak2 = AccountKey::derive(&mk2).unwrap();
        assert_ne!(tag, ak2.derive_backup_tag(), "account-scoped");
    }

    #[test]
    fn pair_key_order_independent_and_distinct_per_pair() {
        // P1.4 / P2P-005: both ends of a pair derive the same key
        // regardless of argument order; different pairs get different
        // keys; and pair keys are domain-separated from the transport key.
        let mk = MasterKey::from_passphrase(b"test", b"shared-salt").unwrap();
        let ak = AccountKey::derive(&mk).unwrap();

        let ab = ak.derive_pair_key("12D3KooWPeerA", "12D3KooWPeerB");
        let ba = ak.derive_pair_key("12D3KooWPeerB", "12D3KooWPeerA");
        assert_eq!(ab, ba, "pair key must be order-independent");

        let ac = ak.derive_pair_key("12D3KooWPeerA", "12D3KooWPeerC");
        assert_ne!(ab, ac, "each device pair must get a distinct key");

        assert_ne!(ab, ak.derive_transport_key());
        assert_ne!(&ab, ak.as_bytes());

        // A different account derives different pair keys for the same pair.
        let mk2 = MasterKey::from_passphrase(b"other", b"shared-salt").unwrap();
        let ak2 = AccountKey::derive(&mk2).unwrap();
        assert_ne!(ab, ak2.derive_pair_key("12D3KooWPeerA", "12D3KooWPeerB"));
    }
}
