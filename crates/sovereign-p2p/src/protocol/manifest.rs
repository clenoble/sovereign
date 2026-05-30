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
    /// Soft-delete marker (ISO-8601 string), `None` for active rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<String>,
}

/// A sync manifest entry for a single thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadManifestEntry {
    pub thread_id: String,
    pub modified_at: String,
    pub content_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<String>,
}

/// A sync manifest entry for a single entity (PII aggregator).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityManifestEntry {
    pub entity_id: String,
    pub modified_at: String,
    pub content_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<String>,
}

/// A sync manifest entry for a single PII record. Uses `discovered_at`
/// as the LWW timestamp since `PiiRecord` has no `modified_at` today.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiiRecordManifestEntry {
    pub record_id: String,
    pub discovered_at: String,
    pub content_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<String>,
}

/// A sync manifest entry for a single share record. Append-only —
/// no LWW needed; `shared_at` is the natural ordering field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareRecordManifestEntry {
    pub record_id: String,
    pub shared_at: String,
    pub content_hash: String,
}

/// A sync manifest covering every syncable table on a device. Documents
/// keep their existing commit-chain track via `documents`; everything
/// else moves through the row-level `EncryptedRow` protocol added in
/// Phase 3 of the v0.0.5 plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncManifest {
    /// Device identifier.
    pub device_id: String,
    /// ISO-8601 timestamp when this manifest was generated.
    pub generated_at: String,
    /// Per-document entries (commit-chain tracked).
    #[serde(default)]
    pub documents: Vec<DocumentManifestEntry>,
    /// Per-thread entries (LWW on `modified_at`).
    #[serde(default)]
    pub threads: Vec<ThreadManifestEntry>,
    /// Per-entity entries (LWW on `modified_at`).
    #[serde(default)]
    pub entities: Vec<EntityManifestEntry>,
    /// Per-PII-record entries (LWW on `discovered_at`).
    #[serde(default)]
    pub pii_records: Vec<PiiRecordManifestEntry>,
    /// Per-share-record entries (append-only).
    #[serde(default)]
    pub share_records: Vec<ShareRecordManifestEntry>,
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
            documents: Vec::new(),
            threads: Vec::new(),
            entities: Vec::new(),
            pii_records: Vec::new(),
            share_records: Vec::new(),
        }
    }

    /// Wrap as plaintext EncryptedManifest (empty nonce = plaintext marker).
    /// Used for Phase 1 LAN sync before pair-key encryption is available.
    pub fn to_plaintext(&self) -> EncryptedManifest {
        let json = serde_json::to_vec(self).unwrap_or_default();
        EncryptedManifest {
            ciphertext: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &json),
            nonce: String::new(),
        }
    }

    /// Decode a plaintext EncryptedManifest (empty nonce = plaintext marker).
    pub fn from_plaintext(encrypted: &EncryptedManifest) -> Option<Self> {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&encrypted.ciphertext)
            .ok()?;
        serde_json::from_slice(&bytes).ok()
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
        manifest.documents.push(DocumentManifestEntry {
            doc_id: "document:abc".into(),
            head_commit: Some("commit:123".into()),
            commit_count: 5,
            content_hash: "deadbeef".into(),
            modified_at: "2026-01-01T00:00:00Z".into(),
            deleted_at: None,
        });

        let pair_key = [42u8; 32];
        let encrypted = manifest.encrypt(&pair_key).unwrap();
        let decrypted = SyncManifest::decrypt(&encrypted, &pair_key).unwrap();

        assert_eq!(decrypted.device_id, "device-001");
        assert_eq!(decrypted.documents.len(), 1);
        assert_eq!(decrypted.documents[0].doc_id, "document:abc");
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
        assert!(decrypted.documents.is_empty());
        assert!(decrypted.threads.is_empty());
        assert!(decrypted.entities.is_empty());
        assert!(decrypted.pii_records.is_empty());
        assert!(decrypted.share_records.is_empty());
    }

    #[test]
    fn multi_table_manifest_roundtrip() {
        let mut manifest = SyncManifest::new("dev-2".into());
        manifest.threads.push(ThreadManifestEntry {
            thread_id: "thread:t1".into(),
            modified_at: "2026-02-01T00:00:00Z".into(),
            content_hash: "abc".into(),
            deleted_at: None,
        });
        manifest.entities.push(EntityManifestEntry {
            entity_id: "entity:e1".into(),
            modified_at: "2026-02-02T00:00:00Z".into(),
            content_hash: "def".into(),
            deleted_at: None,
        });
        manifest.pii_records.push(PiiRecordManifestEntry {
            record_id: "pii_record:p1".into(),
            discovered_at: "2026-02-03T00:00:00Z".into(),
            content_hash: "ghi".into(),
            deleted_at: None,
        });
        manifest.share_records.push(ShareRecordManifestEntry {
            record_id: "share_record:s1".into(),
            shared_at: "2026-02-04T00:00:00Z".into(),
            content_hash: "jkl".into(),
        });

        let pair_key = [11u8; 32];
        let encrypted = manifest.encrypt(&pair_key).unwrap();
        let decrypted = SyncManifest::decrypt(&encrypted, &pair_key).unwrap();
        assert_eq!(decrypted.threads.len(), 1);
        assert_eq!(decrypted.entities.len(), 1);
        assert_eq!(decrypted.pii_records.len(), 1);
        assert_eq!(decrypted.share_records.len(), 1);
    }
}
