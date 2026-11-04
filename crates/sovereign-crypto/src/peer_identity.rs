//! A persona's device identity: the libp2p peer id derived from its
//! [`DeviceKey`] (v0.0.12 item 22, `doc/plans/V0012-item22-device-identity.md`).
//!
//! **The one derivation.** It lives here, beside the device key, because every
//! host depends on this crate whatever its cargo features: the native shell,
//! the phone, and the app's default desktop/CLI build, which does not compile
//! `sovereign-p2p` yet builds a document service. `sovereign_p2p::identity::
//! derive_keypair` calls [`derive_keypair`], so the id a document service
//! stamps with is the id the P2P node announces (D1).
//!
//! - [`derive_keypair`]: the Ed25519 libp2p keypair (the P2P node's identity).
//! - [`peer_id`]: its peer id as a string (`12D3KooW…`) — what a document
//!   service writes as its device in stamps, authors and trust decisions.
//! - [`is_peer_id`]: whether a device string is a peer id. A stamp written
//!   before item 22 carries a display name instead; it is shown as that name
//!   (D3, D5).
//!
//! Unique per workspace and persona: the device key is derived from the
//! persona's passphrase and this device's id, so the duress persona has its
//! own (D6), and renaming the device changes nothing (D4).

use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroize;

use crate::device_key::DeviceKey;
use crate::error::{CryptoError, CryptoResult};

pub use libp2p_identity;

/// The HKDF info string of the derivation. Changing it changes every
/// device's peer id (and so its pairings and its stamps' identity).
const INFO: &[u8] = b"sovereign-p2p-identity";

/// The deterministic Ed25519 libp2p keypair of a device key: HKDF-SHA256 over
/// the key's bytes, expanded with [`INFO`] into the Ed25519 secret.
pub fn derive_keypair(device_key: &DeviceKey) -> CryptoResult<libp2p_identity::Keypair> {
    let hk = Hkdf::<Sha256>::new(None, device_key.as_bytes());
    let mut seed = [0u8; 32];
    hk.expand(INFO, &mut seed)
        .map_err(|e| CryptoError::DerivationFailed(e.to_string()))?;
    let keypair = libp2p_identity::Keypair::ed25519_from_bytes(&mut seed)
        .map_err(|e| CryptoError::DerivationFailed(e.to_string()));
    seed.zeroize();
    keypair
}

/// This device key's libp2p peer id, as a string (`PeerId::to_string`).
pub fn peer_id(device_key: &DeviceKey) -> CryptoResult<String> {
    Ok(derive_keypair(device_key)?.public().to_peer_id().to_string())
}

/// Whether `device` is a libp2p peer id (as [`peer_id`] writes one). A device
/// string that is not one is a display name from before item 22.
pub fn is_peer_id(device: &str) -> bool {
    device.parse::<libp2p_identity::PeerId>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::master_key::MasterKey;

    fn key(passphrase: &str, device: &str) -> DeviceKey {
        let mk = MasterKey::from_passphrase(passphrase.as_bytes(), b"salt").unwrap();
        DeviceKey::derive(&mk, device).unwrap()
    }

    #[test]
    fn the_same_key_gives_the_same_peer_id() {
        let dk = key("test", "dev-01");
        assert_eq!(peer_id(&dk).unwrap(), peer_id(&dk).unwrap());
        assert!(peer_id(&dk).unwrap().starts_with("12D3KooW"));
    }

    #[test]
    fn two_keys_two_peer_ids() {
        assert_ne!(peer_id(&key("test", "dev-01")).unwrap(), peer_id(&key("test", "dev-02")).unwrap());
        assert_ne!(peer_id(&key("primary", "dev-01")).unwrap(), peer_id(&key("duress", "dev-01")).unwrap());
    }

    #[test]
    fn a_display_name_is_not_a_peer_id() {
        assert!(is_peer_id(&peer_id(&key("test", "dev-01")).unwrap()));
        for name in ["Sovereign Device", "", "desktop", "12D3KooW", "Atelier Nord"] {
            assert!(!is_peer_id(name), "{name:?}");
        }
    }
}
