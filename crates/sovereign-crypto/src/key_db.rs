use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::aead::{self, NONCE_SIZE};
use crate::device_key::DeviceKey;
use crate::document_key::{DocumentKey, WrappedDocumentKey};
use crate::error::{CryptoError, CryptoResult};
use crate::kek::Kek;

/// Persistent key database storing wrapped document keys.
///
/// The entire database is encrypted at rest with the DeviceKey.
/// File format: `nonce (24 bytes) || ciphertext (AES of JSON)`.
#[derive(Debug)]
pub struct KeyDatabase {
    /// doc_id → list of wrapped keys (current + rotated old keys)
    entries: HashMap<String, Vec<WrappedDocumentKey>>,
    path: PathBuf,
}

/// Serialized form of the database contents (before encryption).
#[derive(serde::Serialize, serde::Deserialize)]
struct KeyDbContents {
    entries: HashMap<String, Vec<WrappedDocumentKey>>,
}

impl KeyDatabase {
    /// Create a new empty key database.
    pub fn new(path: PathBuf) -> Self {
        Self {
            entries: HashMap::new(),
            path,
        }
    }

    /// Load an existing key database from disk, decrypting with the DeviceKey.
    pub fn load(path: &Path, device_key: &DeviceKey) -> CryptoResult<Self> {
        let data = std::fs::read(path)
            .map_err(|e| CryptoError::KeyDbIo(e.to_string()))?;

        if data.len() < NONCE_SIZE {
            return Err(CryptoError::KeyDbIo("file too short".into()));
        }

        let mut nonce = [0u8; NONCE_SIZE];
        nonce.copy_from_slice(&data[..NONCE_SIZE]);
        let ciphertext = &data[NONCE_SIZE..];

        let plaintext = aead::decrypt(ciphertext, &nonce, device_key.as_bytes())?;
        let contents: KeyDbContents = serde_json::from_slice(&plaintext)
            .map_err(|e| CryptoError::Serialization(e.to_string()))?;

        Ok(Self {
            entries: contents.entries,
            path: path.to_path_buf(),
        })
    }

    /// Save the key database to disk, encrypting with the DeviceKey.
    pub fn save(&self, device_key: &DeviceKey) -> CryptoResult<()> {
        let contents = KeyDbContents {
            entries: self.entries.clone(),
        };
        let json = serde_json::to_vec(&contents)
            .map_err(|e| CryptoError::Serialization(e.to_string()))?;

        let (ciphertext, nonce) = aead::encrypt(&json, device_key.as_bytes())?;

        let mut output = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
        output.extend_from_slice(&nonce);
        output.extend_from_slice(&ciphertext);

        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| CryptoError::KeyDbIo(e.to_string()))?;
        }
        std::fs::write(&self.path, &output)
            .map_err(|e| CryptoError::KeyDbIo(e.to_string()))?;

        Ok(())
    }

    /// Store a wrapped document key. Appends to the key list for rotation support.
    pub fn insert(&mut self, doc_id: &str, wrapped_key: WrappedDocumentKey) {
        self.entries
            .entry(doc_id.to_string())
            .or_default()
            .push(wrapped_key);
    }

    /// Get the current (latest epoch) wrapped document key for a document.
    pub fn get_current(&self, doc_id: &str) -> CryptoResult<&WrappedDocumentKey> {
        self.entries
            .get(doc_id)
            .and_then(|keys| keys.last())
            .ok_or_else(|| CryptoError::KeyNotFound(doc_id.to_string()))
    }

    /// Get a wrapped document key by epoch.
    pub fn get_by_epoch(&self, doc_id: &str, epoch: u32) -> CryptoResult<&WrappedDocumentKey> {
        self.entries
            .get(doc_id)
            .and_then(|keys| keys.iter().find(|k| k.epoch == epoch))
            .ok_or_else(|| CryptoError::KeyNotFound(format!("{}@epoch={}", doc_id, epoch)))
    }

    /// Get all wrapped keys for a document (current + old rotated keys).
    pub fn get_all(&self, doc_id: &str) -> Option<&[WrappedDocumentKey]> {
        self.entries.get(doc_id).map(|v| v.as_slice())
    }

    /// Check whether a key exists for a document.
    pub fn contains(&self, doc_id: &str) -> bool {
        self.entries.contains_key(doc_id)
    }

    /// Number of documents with stored keys.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the database is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Generate a new document key, wrap it with the KEK, and store it.
    /// Returns the unwrapped DocumentKey for immediate use.
    pub fn create_document_key(
        &mut self,
        doc_id: &str,
        kek: &Kek,
        epoch: u32,
    ) -> CryptoResult<DocumentKey> {
        let dk = DocumentKey::generate();
        let wrapped = dk.wrap(kek, epoch)?;
        self.insert(doc_id, wrapped);
        Ok(dk)
    }

    /// Unwrap the current document key for a document.
    pub fn unwrap_current(&self, doc_id: &str, kek: &Kek) -> CryptoResult<DocumentKey> {
        let wrapped = self.get_current(doc_id)?;
        DocumentKey::unwrap(wrapped, kek)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::master_key::MasterKey;

    fn test_keys() -> (DeviceKey, Kek) {
        let mk = MasterKey::from_passphrase(b"test", b"salt").unwrap();
        let dk = DeviceKey::derive(&mk, "dev-01").unwrap();
        let kek = Kek::generate();
        (dk, kek)
    }

    #[test]
    fn create_and_unwrap_document_key() {
        let (_, kek) = test_keys();
        let mut db = KeyDatabase::new(PathBuf::from("/tmp/test-keys.db"));
        let dk = db.create_document_key("document:abc", &kek, 1).unwrap();
        let recovered = db.unwrap_current("document:abc", &kek).unwrap();
        assert_eq!(dk.as_bytes(), recovered.as_bytes());
    }

    #[test]
    fn key_not_found() {
        let db = KeyDatabase::new(PathBuf::from("/tmp/test-keys.db"));
        assert!(db.get_current("nonexistent").is_err());
    }

    #[test]
    fn multiple_epochs() {
        let (_, kek) = test_keys();
        let mut db = KeyDatabase::new(PathBuf::from("/tmp/test-keys.db"));
        let _dk1 = db.create_document_key("doc:1", &kek, 1).unwrap();
        let dk2 = db.create_document_key("doc:1", &kek, 2).unwrap();

        // Current is epoch 2
        let current = db.unwrap_current("doc:1", &kek).unwrap();
        assert_eq!(current.as_bytes(), dk2.as_bytes());

        // Can still get epoch 1
        let old = db.get_by_epoch("doc:1", 1).unwrap();
        assert_eq!(old.epoch, 1);
    }

    #[test]
    fn save_load_roundtrip() {
        let (device_key, kek) = test_keys();
        let dir = std::env::temp_dir().join("sovereign-crypto-test-keydb");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("keys.db");

        let dk = {
            let mut db = KeyDatabase::new(path.clone());
            let dk = db.create_document_key("doc:test", &kek, 1).unwrap();
            db.save(&device_key).unwrap();
            dk
        };

        let db2 = KeyDatabase::load(&path, &device_key).unwrap();
        let recovered = db2.unwrap_current("doc:test", &kek).unwrap();
        assert_eq!(dk.as_bytes(), recovered.as_bytes());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wrong_device_key_cannot_load() {
        let (device_key, kek) = test_keys();
        let dir = std::env::temp_dir().join("sovereign-crypto-test-keydb-wrong");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("keys.db");

        let mut db = KeyDatabase::new(path.clone());
        let _ = db.create_document_key("doc:test", &kek, 1).unwrap();
        db.save(&device_key).unwrap();

        let wrong_mk = MasterKey::from_passphrase(b"wrong", b"salt").unwrap();
        let wrong_dk = DeviceKey::derive(&wrong_mk, "dev-01").unwrap();
        assert!(KeyDatabase::load(&path, &wrong_dk).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn len_and_contains() {
        let (_, kek) = test_keys();
        let mut db = KeyDatabase::new(PathBuf::from("/tmp/test-keys.db"));
        assert!(db.is_empty());
        assert_eq!(db.len(), 0);

        let _ = db.create_document_key("doc:1", &kek, 1).unwrap();
        assert_eq!(db.len(), 1);
        assert!(db.contains("doc:1"));
        assert!(!db.contains("doc:2"));
    }
}
