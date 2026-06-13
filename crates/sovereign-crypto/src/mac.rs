//! Device-keyed HMAC for *local* tamper-evidence (AUTOCOMMIT-001).
//!
//! Used to make local commit-history rows tamper-evident. The same device
//! writes and verifies, so a symmetric MAC under a key derived only from the
//! (unlocked) DeviceKey is exactly the right primitive: an attacker who edits a
//! stored commit on disk can't recompute the tag without the DeviceKey (which
//! is never on disk in the clear). This is integrity/non-tampering, not
//! cross-device non-repudiation — that's what the Ed25519 sync-envelope
//! signatures provide on the wire.

use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::device_key::DeviceKey;

type HmacSha256 = Hmac<Sha256>;

/// Domain-separation tag so this MAC can never collide with another use of the
/// DeviceKey (e.g. key-DB wrapping).
const COMMIT_MAC_DOMAIN: &[u8] = b"sovereign-commit-mac:v1";

/// HMAC-SHA256 of `data` under the device key (domain-separated), base64-encoded.
pub fn device_mac(device_key: &DeviceKey, data: &[u8]) -> String {
    keyed_mac(device_key.as_bytes(), COMMIT_MAC_DOMAIN, data)
}

/// HMAC-SHA256 of `domain || data` under an arbitrary 32-byte key, base64.
/// Generic form used where the caller holds raw key bytes (e.g. the model
/// integrity TOFU store keyed from the AccountKey). Always pass a distinct
/// `domain` per use so MACs from different subsystems can never collide.
pub fn keyed_mac(key: &[u8; 32], domain: &[u8], data: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(domain);
    mac.update(data);
    base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes())
}

/// Constant-time verification of a base64 MAC produced by [`keyed_mac`].
pub fn verify_keyed_mac(key: &[u8; 32], domain: &[u8], data: &[u8], mac_b64: &str) -> bool {
    let expected = keyed_mac(key, domain, data);
    let a = expected.as_bytes();
    let b = mac_b64.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Constant-time verification of a base64 device MAC over `data`.
pub fn verify_device_mac(device_key: &DeviceKey, data: &[u8], mac_b64: &str) -> bool {
    let expected = device_mac(device_key, data);
    // Compare the two base64 strings in constant time (no early exit).
    let a = expected.as_bytes();
    let b = mac_b64.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::master_key::MasterKey;

    fn dk() -> DeviceKey {
        let mk = MasterKey::from_passphrase(b"pw-for-mac-test", &[3u8; 32]).unwrap();
        DeviceKey::derive(&mk, "device-mac-test").unwrap()
    }

    #[test]
    fn mac_roundtrips_and_detects_tamper() {
        let key = dk();
        let mac = device_mac(&key, b"commit-canonical-bytes");
        assert!(verify_device_mac(&key, b"commit-canonical-bytes", &mac));
        // Any change to the data invalidates the MAC.
        assert!(!verify_device_mac(&key, b"commit-canonical-byteS", &mac));
        assert!(!verify_device_mac(&key, b"", &mac));
    }

    #[test]
    fn different_device_keys_produce_different_macs() {
        let mk = MasterKey::from_passphrase(b"pw-for-mac-test", &[3u8; 32]).unwrap();
        let a = DeviceKey::derive(&mk, "device-a").unwrap();
        let b = DeviceKey::derive(&mk, "device-b").unwrap();
        let data = b"same data";
        assert_ne!(device_mac(&a, data), device_mac(&b, data));
        assert!(!verify_device_mac(&b, data, &device_mac(&a, data)));
    }
}
