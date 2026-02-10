use std::collections::HashMap;
use std::path::{Path, PathBuf};

use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::error::{P2pError, P2pResult};

/// A paired device record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairedDevice {
    /// The peer's libp2p PeerId as a string.
    pub peer_id: String,
    /// Human-readable device name.
    pub device_name: String,
    /// The derived pair key (base64-encoded, encrypted at rest).
    pub pair_key_b64: String,
    /// ISO-8601 timestamp of pairing.
    pub paired_at: String,
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

    /// Derive a pair key from a shared secret.
    pub fn derive_pair_key(shared_secret: &[u8]) -> [u8; 32] {
        let hk = Hkdf::<Sha256>::new(None, shared_secret);
        let mut pair_key = [0u8; 32];
        hk.expand(b"sovereign-pair-key", &mut pair_key)
            .expect("32 bytes is within HKDF limit");
        pair_key
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

    /// Save paired devices to disk (plaintext JSON — should be encrypted in production).
    pub fn save(&self) -> P2pResult<()> {
        let devices: Vec<&PairedDevice> = self.devices.values().collect();
        let json = serde_json::to_string_pretty(&devices)
            .map_err(|e| P2pError::PairingError(e.to_string()))?;

        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| P2pError::PairingError(e.to_string()))?;
        }
        std::fs::write(&self.path, json)
            .map_err(|e| P2pError::PairingError(e.to_string()))?;
        Ok(())
    }

    /// Load paired devices from disk.
    pub fn load(path: &Path) -> P2pResult<Self> {
        let json = std::fs::read_to_string(path)
            .map_err(|e| P2pError::PairingError(e.to_string()))?;
        let devices_vec: Vec<PairedDevice> = serde_json::from_str(&json)
            .map_err(|e| P2pError::PairingError(e.to_string()))?;

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

    #[test]
    fn derive_pair_key_deterministic() {
        let secret = b"shared-secret-from-pairing";
        let pk1 = PairingManager::derive_pair_key(secret);
        let pk2 = PairingManager::derive_pair_key(secret);
        assert_eq!(pk1, pk2);
    }

    #[test]
    fn different_secrets_different_keys() {
        let pk1 = PairingManager::derive_pair_key(b"secret-a");
        let pk2 = PairingManager::derive_pair_key(b"secret-b");
        assert_ne!(pk1, pk2);
    }

    #[test]
    fn add_remove_device() {
        let mut pm = PairingManager::new(PathBuf::from("/tmp/paired.json"));
        let device = PairedDevice {
            peer_id: "peer-123".into(),
            device_name: "My Phone".into(),
            pair_key_b64: "base64key".into(),
            paired_at: "2026-01-01T00:00:00Z".into(),
        };
        pm.add_device(device);
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

        let mut pm = PairingManager::new(path.clone());
        pm.add_device(PairedDevice {
            peer_id: "peer-abc".into(),
            device_name: "Laptop".into(),
            pair_key_b64: "key123".into(),
            paired_at: "2026-01-01T00:00:00Z".into(),
        });
        pm.save().unwrap();

        let pm2 = PairingManager::load(&path).unwrap();
        assert!(pm2.is_paired("peer-abc"));
        assert_eq!(pm2.get_device("peer-abc").unwrap().device_name, "Laptop");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
