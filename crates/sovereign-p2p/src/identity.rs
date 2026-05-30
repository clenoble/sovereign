use hkdf::Hkdf;
use sha2::Sha256;

use crate::error::{P2pError, P2pResult};

/// The per-device key used to derive this device's libp2p PeerId.
///
/// Type-aliased to the existing `sovereign_crypto::device_key::DeviceKey`
/// since v0.0.5 — they're the same key bytes, but the alias narrows the
/// intent to "P2P identity seed only". User-scoped at-rest encryption
/// uses `sovereign_crypto::account_key::AccountKey` instead.
pub type P2pIdentityKey = sovereign_crypto::device_key::DeviceKey;

/// Derive a deterministic Ed25519 libp2p Keypair from a P2pIdentityKey via HKDF.
///
/// This ensures the same device always gets the same PeerId.
pub fn derive_keypair(identity_key: &P2pIdentityKey) -> P2pResult<libp2p::identity::Keypair> {
    let hk = Hkdf::<Sha256>::new(None, identity_key.as_bytes());
    let mut seed = [0u8; 32];
    hk.expand(b"sovereign-p2p-identity", &mut seed)
        .map_err(|e| P2pError::Identity(e.to_string()))?;

    let keypair = libp2p::identity::Keypair::ed25519_from_bytes(seed)
        .map_err(|e: libp2p::identity::DecodingError| P2pError::Identity(e.to_string()))?;

    Ok(keypair)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sovereign_crypto::device_key::DeviceKey;
    use sovereign_crypto::master_key::MasterKey;

    fn test_device_key() -> DeviceKey {
        let mk = MasterKey::from_passphrase(b"test", b"salt").unwrap();
        DeviceKey::derive(&mk, "dev-01").unwrap()
    }

    #[test]
    fn deterministic_keypair() {
        let dk = test_device_key();
        let kp1 = derive_keypair(&dk).unwrap();
        let kp2 = derive_keypair(&dk).unwrap();
        assert_eq!(kp1.public().to_peer_id(), kp2.public().to_peer_id());
    }

    #[test]
    fn different_device_keys_differ() {
        let mk = MasterKey::from_passphrase(b"test", b"salt").unwrap();
        let dk1 = DeviceKey::derive(&mk, "dev-01").unwrap();
        let dk2 = DeviceKey::derive(&mk, "dev-02").unwrap();
        let kp1 = derive_keypair(&dk1).unwrap();
        let kp2 = derive_keypair(&dk2).unwrap();
        assert_ne!(kp1.public().to_peer_id(), kp2.public().to_peer_id());
    }
}
