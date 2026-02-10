//! Data migration: encrypt existing plaintext documents.
//!
//! `migrate_to_encrypted()` iterates all documents, generates a DocumentKey
//! for each unencrypted one, encrypts content, and updates the document.
//! Idempotent — skips documents that already have an encryption_nonce.

use crate::aead;
use crate::error::CryptoResult;
use crate::kek::Kek;
use crate::key_db::KeyDatabase;

/// Progress callback type: (encrypted_count, total_count).
pub type ProgressCallback = Box<dyn Fn(u32, u32) + Send>;

/// Encryption plan for a single document.
pub struct DocumentEncryptionPlan {
    pub doc_id: String,
    pub plaintext_content: String,
}

/// Result of encrypting a single document.
pub struct EncryptedDocumentResult {
    pub doc_id: String,
    pub encrypted_content: String,
    pub nonce_b64: String,
}

/// Encrypt a batch of documents' content.
///
/// This function handles the crypto side only — the caller is responsible
/// for updating the database with the encrypted content and nonce.
pub fn encrypt_documents(
    plans: &[DocumentEncryptionPlan],
    key_db: &mut KeyDatabase,
    kek: &Kek,
    progress: Option<&ProgressCallback>,
) -> CryptoResult<Vec<EncryptedDocumentResult>> {
    let total = plans.len() as u32;
    let mut results = Vec::with_capacity(plans.len());

    for (i, plan) in plans.iter().enumerate() {
        let epoch = key_db
            .get_all(&plan.doc_id)
            .map(|keys| keys.len() as u32 + 1)
            .unwrap_or(1);

        let doc_key = key_db.create_document_key(&plan.doc_id, kek, epoch)?;

        let (ciphertext, nonce) = aead::encrypt(
            plan.plaintext_content.as_bytes(),
            doc_key.as_bytes(),
        )?;

        let b64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &ciphertext,
        );
        let nonce_b64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &nonce,
        );

        results.push(EncryptedDocumentResult {
            doc_id: plan.doc_id.clone(),
            encrypted_content: b64,
            nonce_b64,
        });

        if let Some(cb) = progress {
            cb((i + 1) as u32, total);
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn encrypt_documents_batch() {
        let kek = Kek::generate();
        let mut key_db = KeyDatabase::new(PathBuf::from("/tmp/migration-test.db"));

        let plans = vec![
            DocumentEncryptionPlan {
                doc_id: "document:1".into(),
                plaintext_content: r#"{"body":"Hello","images":[]}"#.into(),
            },
            DocumentEncryptionPlan {
                doc_id: "document:2".into(),
                plaintext_content: r#"{"body":"World","images":[]}"#.into(),
            },
        ];

        let progress_cb: ProgressCallback = Box::new(|_done, _total| {});
        let results = encrypt_documents(
            &plans,
            &mut key_db,
            &kek,
            Some(&progress_cb),
        ).unwrap();

        assert_eq!(results.len(), 2);

        // Verify each result can be decrypted
        for (plan, result) in plans.iter().zip(results.iter()) {
            use base64::Engine;
            let ct = base64::engine::general_purpose::STANDARD
                .decode(&result.encrypted_content).unwrap();
            let nonce_bytes = base64::engine::general_purpose::STANDARD
                .decode(&result.nonce_b64).unwrap();
            let mut nonce = [0u8; 24];
            nonce.copy_from_slice(&nonce_bytes);

            let doc_key = key_db.unwrap_current(&plan.doc_id, &kek).unwrap();
            let plaintext = aead::decrypt(&ct, &nonce, doc_key.as_bytes()).unwrap();
            assert_eq!(String::from_utf8(plaintext).unwrap(), plan.plaintext_content);
        }
    }

    #[test]
    fn encrypt_empty_batch() {
        let kek = Kek::generate();
        let mut key_db = KeyDatabase::new(PathBuf::from("/tmp/migration-test-empty.db"));
        let results = encrypt_documents(&[], &mut key_db, &kek, None).unwrap();
        assert!(results.is_empty());
    }
}
