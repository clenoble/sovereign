use serde::{Deserialize, Serialize};

/// A sync manifest entry for a single document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentManifestEntry {
    /// Document ID (e.g. "document:abc123").
    pub doc_id: String,
    /// Latest commit ID on this device.
    pub head_commit: Option<String>,
    /// Commit count (for fast divergence detection).
    pub commit_count: u32,
    /// SHA-256 hash of the content for quick equality check.
    pub content_hash: String,
    /// ISO-8601 of last modification.
    pub modified_at: String,
}

/// A sync manifest listing all documents on a device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncManifest {
    /// Device identifier.
    pub device_id: String,
    /// ISO-8601 timestamp when this manifest was generated.
    pub generated_at: String,
    /// Per-document entries.
    pub entries: Vec<DocumentManifestEntry>,
}

/// An encrypted sync manifest for wire transport.
/// Encrypted with the device-pair key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedManifest {
    /// Base64-encoded ciphertext.
    pub ciphertext: String,
    /// Base64-encoded nonce.
    pub nonce: String,
}

impl SyncManifest {
    pub fn new(device_id: String) -> Self {
        Self {
            device_id,
            generated_at: chrono::Utc::now().to_rfc3339(),
            entries: Vec::new(),
        }
    }

    /// Encrypt this manifest for transport using a shared pair key.
    pub fn encrypt(&self, pair_key: &[u8; 32]) -> Result<EncryptedManifest, sovereign_crypto::CryptoError> {
        let json = serde_json::to_vec(self)
            .map_err(|e| sovereign_crypto::CryptoError::Serialization(e.to_string()))?;
        let (ciphertext, nonce) = sovereign_crypto::aead::encrypt(&json, pair_key)?;
        Ok(EncryptedManifest {
            ciphertext: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &ciphertext),
            nonce: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &nonce),
        })
    }

    /// Decrypt a manifest from transport.
    pub fn decrypt(encrypted: &EncryptedManifest, pair_key: &[u8; 32]) -> Result<Self, sovereign_crypto::CryptoError> {
        use base64::Engine;
        let ciphertext = base64::engine::general_purpose::STANDARD
            .decode(&encrypted.ciphertext)
            .map_err(|e| sovereign_crypto::CryptoError::Base64(e.to_string()))?;
        let nonce_bytes = base64::engine::general_purpose::STANDARD
            .decode(&encrypted.nonce)
            .map_err(|e| sovereign_crypto::CryptoError::Base64(e.to_string()))?;

        let mut nonce = [0u8; 24];
        if nonce_bytes.len() != 24 {
            return Err(sovereign_crypto::CryptoError::InvalidNonceLength {
                expected: 24,
                got: nonce_bytes.len(),
            });
        }
        nonce.copy_from_slice(&nonce_bytes);

        let plaintext = sovereign_crypto::aead::decrypt(&ciphertext, &nonce, pair_key)?;
        serde_json::from_slice(&plaintext)
            .map_err(|e| sovereign_crypto::CryptoError::Serialization(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_encrypt_decrypt_roundtrip() {
        let mut manifest = SyncManifest::new("device-001".into());
        manifest.entries.push(DocumentManifestEntry {
            doc_id: "document:abc".into(),
            head_commit: Some("commit:123".into()),
            commit_count: 5,
            content_hash: "deadbeef".into(),
            modified_at: "2026-01-01T00:00:00Z".into(),
        });

        let pair_key = [42u8; 32];
        let encrypted = manifest.encrypt(&pair_key).unwrap();
        let decrypted = SyncManifest::decrypt(&encrypted, &pair_key).unwrap();

        assert_eq!(decrypted.device_id, "device-001");
        assert_eq!(decrypted.entries.len(), 1);
        assert_eq!(decrypted.entries[0].doc_id, "document:abc");
    }

    #[test]
    fn wrong_key_fails_decrypt() {
        let manifest = SyncManifest::new("dev-1".into());
        let pair_key = [42u8; 32];
        let wrong_key = [99u8; 32];
        let encrypted = manifest.encrypt(&pair_key).unwrap();
        assert!(SyncManifest::decrypt(&encrypted, &wrong_key).is_err());
    }

    #[test]
    fn empty_manifest_roundtrip() {
        let manifest = SyncManifest::new("dev-1".into());
        let pair_key = [7u8; 32];
        let encrypted = manifest.encrypt(&pair_key).unwrap();
        let decrypted = SyncManifest::decrypt(&encrypted, &pair_key).unwrap();
        assert_eq!(decrypted.device_id, "dev-1");
        assert!(decrypted.entries.is_empty());
    }
}
