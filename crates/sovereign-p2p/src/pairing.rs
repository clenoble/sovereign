//! Paired-device records + the per-pair key material (P1.4 / P2P-005).
//!
//! `paired_devices.json` carries each paired peer's **pair key** (the
//! AEAD key that seals row/commit envelopes between this device and that
//! peer), so the file itself is encrypted at rest under a key derived
//! from the device identity key ([`derive_store_key`]). Loading falls
//! back transparently to the legacy plaintext shape (which never carried
//! key material) so existing installs migrate on their first save.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use base64::Engine;
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use sovereign_crypto::account_key::AccountKey;

use crate::error::{P2pError, P2pResult};
use crate::identity::P2pIdentityKey;

/// A paired device record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairedDevice {
    /// The peer's libp2p PeerId as a string.
    pub peer_id: String,
    /// Human-readable device name.
    pub device_name: String,
    /// The per-pair sealing key (base64). Populated lazily by
    /// [`PairingManager::ensure_pair_keys`]; empty only in the pre-login
    /// bootstrap record written during paired onboarding.
    pub pair_key_b64: String,
    /// ISO-8601 timestamp of pairing.
    pub paired_at: String,
}

/// On-disk wrapper for the encrypted paired-device store (P1.4). The
/// legacy plaintext shape was a bare JSON array, so the two formats are
/// unambiguous to a parser.
#[derive(Serialize, Deserialize)]
struct EncryptedPairedStore {
    v: u8,
    nonce: String,
    ciphertext: String,
}

/// Derive the at-rest encryption key for `paired_devices.json` from the
/// per-device identity key. Domain-separated from the libp2p identity
/// seed derived from the same key in `identity::derive_keypair`.
pub fn derive_store_key(identity_key: &P2pIdentityKey) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(None, identity_key.as_bytes());
    let mut out = [0u8; 32];
    hk.expand(b"sovereign-paired-store-key:v1", &mut out)
        .expect("32 bytes is within HKDF output limit");
    out
}

impl PairedDevice {
    /// Build a record with the per-pair key encoded (P1.4), stamped now.
    pub fn with_key(peer_id: String, device_name: String, pair_key: [u8; 32]) -> Self {
        Self {
            peer_id,
            device_name,
            pair_key_b64: base64::engine::general_purpose::STANDARD.encode(pair_key),
            paired_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

/// Manages paired device state.
pub struct PairingManager {
    devices: HashMap<String, PairedDevice>,
    path: PathBuf,
}

impl PairingManager {
    pub fn new(path: PathBuf) -> Self {
        Self {
            devices: HashMap::new(),
            path,
        }
    }

    /// Register a paired device.
    pub fn add_device(&mut self, device: PairedDevice) {
        self.devices.insert(device.peer_id.clone(), device);
    }

    /// Remove a paired device.
    pub fn remove_device(&mut self, peer_id: &str) -> Option<PairedDevice> {
        self.devices.remove(peer_id)
    }

    /// Get a paired device.
    pub fn get_device(&self, peer_id: &str) -> Option<&PairedDevice> {
        self.devices.get(peer_id)
    }

    /// List all paired devices.
    pub fn list_devices(&self) -> Vec<&PairedDevice> {
        self.devices.values().collect()
    }

    /// Check if a peer is paired.
    pub fn is_paired(&self, peer_id: &str) -> bool {
        self.devices.contains_key(peer_id)
    }

    /// Populate any missing per-pair keys by deterministic derivation
    /// from the shared AccountKey (both ends derive the same key for the
    /// sorted peer-id pair, so no handshake is needed — see
    /// [`AccountKey::derive_pair_key`]). Returns true if anything was
    /// populated (caller should save).
    pub fn ensure_pair_keys(&mut self, account_key: &AccountKey, local_peer_id: &str) -> bool {
        let mut changed = false;
        for d in self.devices.values_mut() {
            if d.pair_key_b64.is_empty() {
                let key = account_key.derive_pair_key(local_peer_id, &d.peer_id);
                d.pair_key_b64 = base64::engine::general_purpose::STANDARD.encode(key);
                changed = true;
            }
        }
        changed
    }

    /// Decode the pair keys into the `peer_id → key` map the SyncService
    /// seals/unseals envelopes with. Devices with a missing or malformed
    /// key are omitted (they fail closed at seal time).
    pub fn pair_key_map(&self) -> HashMap<String, [u8; 32]> {
        let mut out = HashMap::new();
        for d in self.devices.values() {
            if d.pair_key_b64.is_empty() {
                continue;
            }
            let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(&d.pair_key_b64)
            else {
                tracing::warn!("paired device {} has malformed pair key; skipping", d.peer_id);
                continue;
            };
            if bytes.len() != 32 {
                tracing::warn!(
                    "paired device {} has wrong-length pair key; skipping",
                    d.peer_id
                );
                continue;
            }
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            out.insert(d.peer_id.clone(), key);
        }
        out
    }

    /// Save the paired devices to disk, AEAD-encrypted under the
    /// device-derived store key (P1.4 — the file carries pair-key
    /// material). Atomic + owner-only via `fs_private`.
    pub fn save(&self, store_key: &[u8; 32]) -> P2pResult<()> {
        let devices: Vec<&PairedDevice> = self.devices.values().collect();
        let json = serde_json::to_vec(&devices)
            .map_err(|e| P2pError::PairingError(e.to_string()))?;
        let (ciphertext, nonce) = sovereign_crypto::aead::encrypt(&json, store_key)
            .map_err(|e| P2pError::PairingError(format!("paired store encrypt: {e}")))?;
        let wrapper = EncryptedPairedStore {
            v: 1,
            nonce: base64::engine::general_purpose::STANDARD.encode(nonce),
            ciphertext: base64::engine::general_purpose::STANDARD.encode(&ciphertext),
        };
        let out = serde_json::to_string_pretty(&wrapper)
            .map_err(|e| P2pError::PairingError(e.to_string()))?;
        self.write_file(out)
    }

    /// Plaintext save for the ONE pre-login path: paired onboarding writes
    /// the source-device record before any local keys exist. Refuses to
    /// write if any record carries pair-key material — that must go
    /// through the encrypted [`Self::save`].
    pub fn save_plaintext_bootstrap(&self) -> P2pResult<()> {
        if self.devices.values().any(|d| !d.pair_key_b64.is_empty()) {
            return Err(P2pError::PairingError(
                "refusing plaintext save: store carries pair-key material".into(),
            ));
        }
        let devices: Vec<&PairedDevice> = self.devices.values().collect();
        let json = serde_json::to_string_pretty(&devices)
            .map_err(|e| P2pError::PairingError(e.to_string()))?;
        self.write_file(json)
    }

    fn write_file(&self, content: String) -> P2pResult<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| P2pError::PairingError(e.to_string()))?;
        }
        sovereign_crypto::fs_private::write_private(&self.path, content)
            .map_err(|e| P2pError::PairingError(e.to_string()))?;
        Ok(())
    }

    /// Load paired devices from disk. Accepts the encrypted wrapper
    /// (normal case) or the legacy plaintext array (pre-P1.4 installs and
    /// the onboarding bootstrap record) — callers re-save through the
    /// encrypted path, which completes the migration.
    pub fn load(path: &Path, store_key: &[u8; 32]) -> P2pResult<Self> {
        let json = std::fs::read_to_string(path)
            .map_err(|e| P2pError::PairingError(e.to_string()))?;

        let devices_vec: Vec<PairedDevice> =
            if let Ok(wrapper) = serde_json::from_str::<EncryptedPairedStore>(&json) {
                let ciphertext = base64::engine::general_purpose::STANDARD
                    .decode(&wrapper.ciphertext)
                    .map_err(|e| P2pError::PairingError(format!("paired store b64: {e}")))?;
                let nonce_bytes = base64::engine::general_purpose::STANDARD
                    .decode(&wrapper.nonce)
                    .map_err(|e| P2pError::PairingError(format!("paired store b64: {e}")))?;
                if nonce_bytes.len() != 24 {
                    return Err(P2pError::PairingError("paired store nonce length".into()));
                }
                let mut nonce = [0u8; 24];
                nonce.copy_from_slice(&nonce_bytes);
                let plaintext = sovereign_crypto::aead::decrypt(&ciphertext, &nonce, store_key)
                    .map_err(|e| {
                        P2pError::PairingError(format!("paired store decrypt: {e}"))
                    })?;
                serde_json::from_slice(&plaintext)
                    .map_err(|e| P2pError::PairingError(e.to_string()))?
            } else {
                // Legacy plaintext array (no pair-key material by design).
                serde_json::from_str(&json).map_err(|e| P2pError::PairingError(e.to_string()))?
            };

        let devices = devices_vec
            .into_iter()
            .map(|d| (d.peer_id.clone(), d))
            .collect();

        Ok(Self {
            devices,
            path: path.to_path_buf(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sovereign_crypto::master_key::MasterKey;

    fn test_account_key() -> AccountKey {
        let mk = MasterKey::from_passphrase(b"test", b"salt").unwrap();
        AccountKey::derive(&mk).unwrap()
    }

    fn device(peer_id: &str, name: &str, pair_key_b64: &str) -> PairedDevice {
        PairedDevice {
            peer_id: peer_id.into(),
            device_name: name.into(),
            pair_key_b64: pair_key_b64.into(),
            paired_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn add_remove_device() {
        let mut pm = PairingManager::new(std::env::temp_dir().join("paired.json"));
        pm.add_device(device("peer-123", "My Phone", "base64key"));
        assert!(pm.is_paired("peer-123"));
        assert_eq!(pm.list_devices().len(), 1);

        let removed = pm.remove_device("peer-123").unwrap();
        assert_eq!(removed.device_name, "My Phone");
        assert!(!pm.is_paired("peer-123"));
    }

    #[test]
    fn save_load_roundtrip() {
        let dir = std::env::temp_dir().join("sovereign-p2p-test-pairing");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("paired.json");
        let store_key = [0x33u8; 32];

        let mut pm = PairingManager::new(path.clone());
        pm.add_device(device("peer-abc", "Laptop", "key123"));
        pm.save(&store_key).unwrap();

        // The file on disk must NOT contain the pair key or device name.
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("key123"), "pair key must not be on disk in the clear");
        assert!(!raw.contains("Laptop"), "device name must be sealed too");

        let pm2 = PairingManager::load(&path, &store_key).unwrap();
        assert!(pm2.is_paired("peer-abc"));
        assert_eq!(pm2.get_device("peer-abc").unwrap().device_name, "Laptop");
        assert_eq!(pm2.get_device("peer-abc").unwrap().pair_key_b64, "key123");

        // A wrong store key must fail, not silently return empty.
        assert!(PairingManager::load(&path, &[0x44u8; 32]).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_plaintext_store_still_loads() {
        let dir = std::env::temp_dir().join("sovereign-p2p-test-pairing-legacy");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("paired.json");
        std::fs::write(
            &path,
            r#"[{"peer_id":"peer-old","device_name":"Old Phone","pair_key_b64":"","paired_at":"2026-01-01T00:00:00Z"}]"#,
        )
        .unwrap();

        let pm = PairingManager::load(&path, &[0x33u8; 32]).unwrap();
        assert!(pm.is_paired("peer-old"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn plaintext_bootstrap_refuses_key_material() {
        let dir = std::env::temp_dir().join("sovereign-p2p-test-pairing-bootstrap");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut pm = PairingManager::new(dir.join("paired.json"));
        pm.add_device(device("peer-1", "Phone", ""));
        pm.save_plaintext_bootstrap().unwrap();

        pm.add_device(device("peer-2", "Tablet", "c2VjcmV0"));
        assert!(
            pm.save_plaintext_bootstrap().is_err(),
            "bootstrap save must refuse stores carrying pair keys"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ensure_pair_keys_populates_and_matches_both_ends() {
        let ak = test_account_key();

        // Device A's view: it has B paired.
        let mut pm_a = PairingManager::new(std::env::temp_dir().join("a.json"));
        pm_a.add_device(device("peer-B", "Device B", ""));
        assert!(pm_a.ensure_pair_keys(&ak, "peer-A"));
        // Second call is a no-op.
        assert!(!pm_a.ensure_pair_keys(&ak, "peer-A"));

        // Device B's view: it has A paired.
        let mut pm_b = PairingManager::new(std::env::temp_dir().join("b.json"));
        pm_b.add_device(device("peer-A", "Device A", ""));
        assert!(pm_b.ensure_pair_keys(&ak, "peer-B"));

        let key_at_a = pm_a.pair_key_map().remove("peer-B").unwrap();
        let key_at_b = pm_b.pair_key_map().remove("peer-A").unwrap();
        assert_eq!(
            key_at_a, key_at_b,
            "both ends must derive the same pair key without a handshake"
        );
    }

    #[test]
    fn pair_key_map_skips_malformed_entries() {
        let mut pm = PairingManager::new(std::env::temp_dir().join("m.json"));
        pm.add_device(device("peer-empty", "No key", ""));
        pm.add_device(device("peer-bad", "Bad b64", "!!!not-base64!!!"));
        pm.add_device(device(
            "peer-short",
            "Short key",
            &base64::engine::general_purpose::STANDARD.encode([1u8; 7]),
        ));
        pm.add_device(device(
            "peer-good",
            "Good",
            &base64::engine::general_purpose::STANDARD.encode([9u8; 32]),
        ));

        let map = pm.pair_key_map();
        assert_eq!(map.len(), 1);
        assert_eq!(map.get("peer-good"), Some(&[9u8; 32]));
    }

    #[test]
    fn derive_store_key_deterministic_and_distinct_from_identity_seed() {
        let mk = MasterKey::from_passphrase(b"test", b"salt").unwrap();
        let dk = sovereign_crypto::device_key::DeviceKey::derive(&mk, "dev-01").unwrap();
        let k1 = derive_store_key(&dk);
        let k2 = derive_store_key(&dk);
        assert_eq!(k1, k2);
        assert_ne!(&k1, dk.as_bytes(), "store key must not be the raw device key");

        let dk2 = sovereign_crypto::device_key::DeviceKey::derive(&mk, "dev-02").unwrap();
        assert_ne!(k1, derive_store_key(&dk2));
    }
}
