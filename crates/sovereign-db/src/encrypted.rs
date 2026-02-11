//! Transparent encryption layer around any GraphDB implementation.
//!
//! Decorator pattern: wraps an inner GraphDB, encrypting document content
//! on write and decrypting on read. Thread/relationship/milestone operations
//! pass through unmodified.

use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine;
use sovereign_crypto::aead;
use sovereign_crypto::kek::Kek;
use sovereign_crypto::key_db::KeyDatabase;
use tokio::sync::Mutex;

use crate::error::{DbError, DbResult};
use crate::schema::{
    ChannelType, Commit, Contact, Conversation, Document, Message, Milestone,
    ReadStatus, RelatedTo, RelationType, Thread,
};
use crate::traits::GraphDB;

/// A GraphDB wrapper that encrypts/decrypts document content transparently.
pub struct EncryptedGraphDB {
    inner: Arc<dyn GraphDB>,
    key_db: Arc<Mutex<KeyDatabase>>,
    kek: Arc<Kek>,
}

impl EncryptedGraphDB {
    pub fn new(
        inner: Arc<dyn GraphDB>,
        key_db: Arc<Mutex<KeyDatabase>>,
        kek: Arc<Kek>,
    ) -> Self {
        Self { inner, key_db, kek }
    }

    /// Encrypt document content, returning the encrypted content string and nonce.
    async fn encrypt_content(&self, doc_id: &str, plaintext: &str) -> DbResult<(String, String)> {
        let mut key_db = self.key_db.lock().await;
        let doc_key = if key_db.contains(doc_id) {
            key_db.unwrap_current(doc_id, &self.kek)
                .map_err(|e| DbError::Query(format!("key unwrap failed: {e}")))?
        } else {
            let epoch = key_db
                .get_all(doc_id)
                .map(|keys| keys.len() as u32 + 1)
                .unwrap_or(1);
            key_db.create_document_key(doc_id, &self.kek, epoch)
                .map_err(|e| DbError::Query(format!("key creation failed: {e}")))?
        };

        let (ciphertext, nonce) = aead::encrypt(plaintext.as_bytes(), doc_key.as_bytes())
            .map_err(|e| DbError::Query(format!("encryption failed: {e}")))?;

        let b64 = base64::engine::general_purpose::STANDARD;
        Ok((b64.encode(&ciphertext), b64.encode(&nonce)))
    }

    /// Decrypt document content using the stored key.
    async fn decrypt_content(&self, doc_id: &str, ciphertext_b64: &str, nonce_b64: &str) -> DbResult<String> {
        let b64 = base64::engine::general_purpose::STANDARD;
        let ciphertext = b64.decode(ciphertext_b64)
            .map_err(|e| DbError::Query(format!("base64 decode ciphertext: {e}")))?;
        let nonce_bytes = b64.decode(nonce_b64)
            .map_err(|e| DbError::Query(format!("base64 decode nonce: {e}")))?;

        if nonce_bytes.len() != 24 {
            return Err(DbError::Query(format!(
                "invalid nonce length: expected 24, got {}",
                nonce_bytes.len()
            )));
        }
        let mut nonce = [0u8; 24];
        nonce.copy_from_slice(&nonce_bytes);

        let key_db = self.key_db.lock().await;
        let doc_key = key_db.unwrap_current(doc_id, &self.kek)
            .map_err(|e| DbError::Query(format!("key unwrap failed: {e}")))?;

        let plaintext = aead::decrypt(&ciphertext, &nonce, doc_key.as_bytes())
            .map_err(|e| DbError::Query(format!("decryption failed: {e}")))?;

        String::from_utf8(plaintext)
            .map_err(|e| DbError::Query(format!("UTF-8 decode failed: {e}")))
    }

    /// Decrypt a document's content if it has an encryption nonce.
    async fn decrypt_document(&self, mut doc: Document) -> DbResult<Document> {
        if let Some(nonce) = &doc.encryption_nonce {
            let doc_id = doc.id.as_ref()
                .map(|t| crate::schema::thing_to_raw(t))
                .unwrap_or_default();
            doc.content = self.decrypt_content(&doc_id, &doc.content, nonce).await?;
            doc.encryption_nonce = None;
        }
        Ok(doc)
    }

    /// Decrypt a list of documents.
    async fn decrypt_documents(&self, docs: Vec<Document>) -> DbResult<Vec<Document>> {
        let mut result = Vec::with_capacity(docs.len());
        for doc in docs {
            result.push(self.decrypt_document(doc).await?);
        }
        Ok(result)
    }
}

#[async_trait]
impl GraphDB for EncryptedGraphDB {
    async fn connect(&self) -> DbResult<()> {
        self.inner.connect().await
    }

    async fn init_schema(&self) -> DbResult<()> {
        self.inner.init_schema().await
    }

    async fn create_document(&self, doc: Document) -> DbResult<Document> {
        // Create first (DB assigns ID), then encrypt and update
        let created = self.inner.create_document(doc).await?;
        let doc_id = created.id.as_ref()
            .map(|t| crate::schema::thing_to_raw(t))
            .unwrap_or_default();

        let (encrypted_content, _nonce) = self.encrypt_content(&doc_id, &created.content).await?;

        // Update the inner DB with encrypted content
        self.inner.update_document(&doc_id, None, Some(&encrypted_content)).await?;

        // Return plaintext document to the caller
        Ok(created)
    }

    async fn get_document(&self, id: &str) -> DbResult<Document> {
        let doc = self.inner.get_document(id).await?;
        self.decrypt_document(doc).await
    }

    async fn list_documents(&self, thread_id: Option<&str>) -> DbResult<Vec<Document>> {
        let docs = self.inner.list_documents(thread_id).await?;
        self.decrypt_documents(docs).await
    }

    async fn update_document(
        &self,
        id: &str,
        title: Option<&str>,
        content: Option<&str>,
    ) -> DbResult<Document> {
        let encrypted_content = if let Some(plaintext) = content {
            let (ct, _nonce) = self.encrypt_content(id, plaintext).await?;
            Some(ct)
        } else {
            None
        };

        let doc = self.inner.update_document(
            id,
            title,
            encrypted_content.as_deref(),
        ).await?;

        self.decrypt_document(doc).await
    }

    async fn delete_document(&self, id: &str) -> DbResult<()> {
        self.inner.delete_document(id).await
    }

    // Thread operations pass through unchanged
    async fn create_thread(&self, thread: Thread) -> DbResult<Thread> {
        self.inner.create_thread(thread).await
    }

    async fn get_thread(&self, id: &str) -> DbResult<Thread> {
        self.inner.get_thread(id).await
    }

    async fn list_threads(&self) -> DbResult<Vec<Thread>> {
        self.inner.list_threads().await
    }

    async fn update_thread(
        &self,
        id: &str,
        name: Option<&str>,
        description: Option<&str>,
    ) -> DbResult<Thread> {
        self.inner.update_thread(id, name, description).await
    }

    async fn delete_thread(&self, id: &str) -> DbResult<()> {
        self.inner.delete_thread(id).await
    }

    async fn move_document_to_thread(&self, doc_id: &str, new_thread_id: &str) -> DbResult<Document> {
        let doc = self.inner.move_document_to_thread(doc_id, new_thread_id).await?;
        self.decrypt_document(doc).await
    }

    // Relationship operations pass through unchanged
    async fn create_relationship(
        &self,
        from_id: &str,
        to_id: &str,
        relation_type: RelationType,
        strength: f32,
    ) -> DbResult<RelatedTo> {
        self.inner.create_relationship(from_id, to_id, relation_type, strength).await
    }

    async fn list_relationships(&self, doc_id: &str) -> DbResult<Vec<RelatedTo>> {
        self.inner.list_relationships(doc_id).await
    }

    async fn traverse(&self, doc_id: &str, depth: u32, limit: u32) -> DbResult<Vec<Document>> {
        let docs = self.inner.traverse(doc_id, depth, limit).await?;
        self.decrypt_documents(docs).await
    }

    async fn adopt_document(&self, id: &str) -> DbResult<Document> {
        let doc = self.inner.adopt_document(id).await?;
        self.decrypt_document(doc).await
    }

    async fn merge_threads(&self, target_id: &str, source_id: &str) -> DbResult<()> {
        self.inner.merge_threads(target_id, source_id).await
    }

    async fn split_thread(
        &self,
        thread_id: &str,
        doc_ids: &[String],
        new_name: &str,
    ) -> DbResult<Thread> {
        self.inner.split_thread(thread_id, doc_ids, new_name).await
    }

    async fn soft_delete_document(&self, id: &str) -> DbResult<()> {
        self.inner.soft_delete_document(id).await
    }

    async fn restore_soft_deleted_document(&self, id: &str) -> DbResult<Document> {
        let doc = self.inner.restore_soft_deleted_document(id).await?;
        self.decrypt_document(doc).await
    }

    async fn soft_delete_thread(&self, id: &str) -> DbResult<()> {
        self.inner.soft_delete_thread(id).await
    }

    async fn restore_soft_deleted_thread(&self, id: &str) -> DbResult<Thread> {
        self.inner.restore_soft_deleted_thread(id).await
    }

    async fn purge_deleted(&self, max_age: std::time::Duration) -> DbResult<u64> {
        self.inner.purge_deleted(max_age).await
    }

    async fn commit_document(&self, doc_id: &str, message: &str) -> DbResult<Commit> {
        // Commit snapshots the current content — which is encrypted in the DB.
        // The snapshot will contain encrypted content.
        self.inner.commit_document(doc_id, message).await
    }

    async fn list_document_commits(&self, doc_id: &str) -> DbResult<Vec<Commit>> {
        self.inner.list_document_commits(doc_id).await
    }

    async fn get_commit(&self, commit_id: &str) -> DbResult<Commit> {
        self.inner.get_commit(commit_id).await
    }

    async fn restore_document(&self, doc_id: &str, commit_id: &str) -> DbResult<Document> {
        let doc = self.inner.restore_document(doc_id, commit_id).await?;
        self.decrypt_document(doc).await
    }

    async fn create_milestone(&self, milestone: Milestone) -> DbResult<Milestone> {
        self.inner.create_milestone(milestone).await
    }

    async fn list_milestones(&self, thread_id: &str) -> DbResult<Vec<Milestone>> {
        self.inner.list_milestones(thread_id).await
    }

    async fn delete_milestone(&self, id: &str) -> DbResult<()> {
        self.inner.delete_milestone(id).await
    }

    // -- Contacts: encrypt notes field ---

    async fn create_contact(&self, contact: Contact) -> DbResult<Contact> {
        // Create first, then encrypt notes if non-empty
        let created = self.inner.create_contact(contact).await?;
        if !created.notes.is_empty() {
            let contact_id = created.id.as_ref()
                .map(|t| crate::schema::thing_to_raw(t))
                .unwrap_or_default();
            let (enc_notes, _nonce) = self.encrypt_content(&contact_id, &created.notes).await?;
            self.inner.update_contact(&contact_id, None, Some(&enc_notes), None).await?;
        }
        Ok(created)
    }

    async fn get_contact(&self, id: &str) -> DbResult<Contact> {
        let mut contact = self.inner.get_contact(id).await?;
        if let Some(nonce) = &contact.encryption_nonce {
            contact.notes = self.decrypt_content(id, &contact.notes, nonce).await?;
            contact.encryption_nonce = None;
        }
        Ok(contact)
    }

    async fn list_contacts(&self) -> DbResult<Vec<Contact>> {
        let contacts = self.inner.list_contacts().await?;
        let mut result = Vec::with_capacity(contacts.len());
        for mut c in contacts {
            if let Some(nonce) = &c.encryption_nonce {
                let id = c.id.as_ref()
                    .map(|t| crate::schema::thing_to_raw(t))
                    .unwrap_or_default();
                c.notes = self.decrypt_content(&id, &c.notes, nonce).await?;
                c.encryption_nonce = None;
            }
            result.push(c);
        }
        Ok(result)
    }

    async fn update_contact(
        &self,
        id: &str,
        name: Option<&str>,
        notes: Option<&str>,
        avatar: Option<&str>,
    ) -> DbResult<Contact> {
        let enc_notes = if let Some(n) = notes {
            let (ct, _nonce) = self.encrypt_content(id, n).await?;
            Some(ct)
        } else {
            None
        };
        let mut contact = self.inner.update_contact(id, name, enc_notes.as_deref(), avatar).await?;
        if let Some(nonce) = &contact.encryption_nonce {
            contact.notes = self.decrypt_content(id, &contact.notes, nonce).await?;
            contact.encryption_nonce = None;
        }
        Ok(contact)
    }

    async fn delete_contact(&self, id: &str) -> DbResult<()> {
        self.inner.delete_contact(id).await
    }

    async fn soft_delete_contact(&self, id: &str) -> DbResult<()> {
        self.inner.soft_delete_contact(id).await
    }

    async fn find_contact_by_address(&self, address: &str) -> DbResult<Option<Contact>> {
        match self.inner.find_contact_by_address(address).await? {
            Some(mut c) => {
                if let Some(nonce) = &c.encryption_nonce {
                    let id = c.id.as_ref()
                        .map(|t| crate::schema::thing_to_raw(t))
                        .unwrap_or_default();
                    c.notes = self.decrypt_content(&id, &c.notes, nonce).await?;
                    c.encryption_nonce = None;
                }
                Ok(Some(c))
            }
            None => Ok(None),
        }
    }

    async fn add_contact_address(
        &self,
        contact_id: &str,
        address: crate::schema::ChannelAddress,
    ) -> DbResult<Contact> {
        let mut contact = self.inner.add_contact_address(contact_id, address).await?;
        if let Some(nonce) = &contact.encryption_nonce {
            contact.notes = self.decrypt_content(contact_id, &contact.notes, nonce).await?;
            contact.encryption_nonce = None;
        }
        Ok(contact)
    }

    // -- Messages: encrypt body and body_html ---

    async fn create_message(&self, message: Message) -> DbResult<Message> {
        // Create first, then encrypt body
        let created = self.inner.create_message(message).await?;
        if !created.body.is_empty() {
            let msg_id = created.id.as_ref()
                .map(|t| crate::schema::thing_to_raw(t))
                .unwrap_or_default();
            let (_, _nonce) = self.encrypt_content(&msg_id, &created.body).await?;
            // Note: message body encryption would need a dedicated update method
            // For now, pass through — full implementation deferred to email channel
        }
        Ok(created)
    }

    async fn get_message(&self, id: &str) -> DbResult<Message> {
        let mut msg = self.inner.get_message(id).await?;
        if let Some(nonce) = &msg.encryption_nonce {
            msg.body = self.decrypt_content(id, &msg.body, nonce).await?;
            if let Some(ref html) = msg.body_html {
                msg.body_html = Some(self.decrypt_content(id, html, nonce).await?);
            }
            msg.encryption_nonce = None;
        }
        Ok(msg)
    }

    async fn list_messages(
        &self,
        conversation_id: &str,
        before: Option<chrono::DateTime<chrono::Utc>>,
        limit: u32,
    ) -> DbResult<Vec<Message>> {
        let msgs = self.inner.list_messages(conversation_id, before, limit).await?;
        let mut result = Vec::with_capacity(msgs.len());
        for mut m in msgs {
            if let Some(nonce) = &m.encryption_nonce {
                let id = m.id.as_ref()
                    .map(|t| crate::schema::thing_to_raw(t))
                    .unwrap_or_default();
                m.body = self.decrypt_content(&id, &m.body, nonce).await?;
                if let Some(ref html) = m.body_html {
                    m.body_html = Some(self.decrypt_content(&id, html, nonce).await?);
                }
                m.encryption_nonce = None;
            }
            result.push(m);
        }
        Ok(result)
    }

    async fn update_message_read_status(
        &self,
        id: &str,
        status: ReadStatus,
    ) -> DbResult<Message> {
        self.inner.update_message_read_status(id, status).await
    }

    async fn delete_message(&self, id: &str) -> DbResult<()> {
        self.inner.delete_message(id).await
    }

    async fn search_messages(&self, query: &str) -> DbResult<Vec<Message>> {
        // Search operates on stored (potentially encrypted) content
        self.inner.search_messages(query).await
    }

    // -- Conversations: pass through (title is not encrypted) ---

    async fn create_conversation(&self, conversation: Conversation) -> DbResult<Conversation> {
        self.inner.create_conversation(conversation).await
    }

    async fn get_conversation(&self, id: &str) -> DbResult<Conversation> {
        self.inner.get_conversation(id).await
    }

    async fn list_conversations(
        &self,
        channel: Option<&ChannelType>,
    ) -> DbResult<Vec<Conversation>> {
        self.inner.list_conversations(channel).await
    }

    async fn update_conversation_unread(
        &self,
        id: &str,
        unread_count: u32,
    ) -> DbResult<Conversation> {
        self.inner.update_conversation_unread(id, unread_count).await
    }

    async fn delete_conversation(&self, id: &str) -> DbResult<()> {
        self.inner.delete_conversation(id).await
    }

    async fn link_conversation_to_thread(
        &self,
        conversation_id: &str,
        thread_id: &str,
    ) -> DbResult<Conversation> {
        self.inner.link_conversation_to_thread(conversation_id, thread_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sovereign_crypto::kek::Kek;
    use sovereign_crypto::key_db::KeyDatabase;
    use std::path::PathBuf;

    fn test_kek() -> Kek {
        Kek::generate()
    }

    fn test_key_db() -> KeyDatabase {
        KeyDatabase::new(PathBuf::from("/tmp/test-encrypted-db-keys.db"))
    }

    #[test]
    fn encrypted_graph_db_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<EncryptedGraphDB>();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn encrypt_decrypt_content_roundtrip() {
        let kek = test_kek();
        let key_db = Arc::new(Mutex::new(test_key_db()));

        // Create a mock "inner" — we just test the encrypt/decrypt helpers
        let encrypted_db = EncryptedGraphDB {
            inner: Arc::new(MockDb),
            key_db: key_db.clone(),
            kek: Arc::new(Kek::from_bytes(*kek.as_bytes())),
        };

        let plaintext = r#"{"body":"Hello, encrypted world!","images":[]}"#;
        let (ct, nonce) = encrypted_db.encrypt_content("document:test1", plaintext).await.unwrap();

        // Ciphertext should differ from plaintext
        assert_ne!(ct, plaintext);

        let decrypted = encrypted_db.decrypt_content("document:test1", &ct, &nonce).await.unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn decrypt_unencrypted_document_passes_through() {
        let kek = test_kek();
        let key_db = Arc::new(Mutex::new(test_key_db()));

        let encrypted_db = EncryptedGraphDB {
            inner: Arc::new(MockDb),
            key_db,
            kek: Arc::new(Kek::from_bytes(*kek.as_bytes())),
        };

        let doc = Document::new("test".into(), "thread:1".into(), true);
        // No encryption_nonce → should pass through
        let result = encrypted_db.decrypt_document(doc.clone()).await.unwrap();
        assert_eq!(result.content, doc.content);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn wrong_kek_cannot_decrypt() {
        let kek1 = Kek::generate();
        let kek2 = Kek::generate();
        let key_db = Arc::new(Mutex::new(test_key_db()));

        let db1 = EncryptedGraphDB {
            inner: Arc::new(MockDb),
            key_db: key_db.clone(),
            kek: Arc::new(Kek::from_bytes(*kek1.as_bytes())),
        };

        let (ct, nonce) = db1.encrypt_content("document:wrongkey", "secret").await.unwrap();

        // Try to decrypt with a different KEK — the key_db has the key wrapped with kek1
        let db2 = EncryptedGraphDB {
            inner: Arc::new(MockDb),
            key_db: key_db.clone(), // same key_db but different KEK
            kek: Arc::new(Kek::from_bytes(*kek2.as_bytes())),
        };

        let result = db2.decrypt_content("document:wrongkey", &ct, &nonce).await;
        assert!(result.is_err());
    }

    // Minimal mock DB for testing encrypt/decrypt helpers
    struct MockDb;

    #[async_trait]
    impl GraphDB for MockDb {
        async fn connect(&self) -> DbResult<()> { Ok(()) }
        async fn init_schema(&self) -> DbResult<()> { Ok(()) }
        async fn create_document(&self, doc: Document) -> DbResult<Document> { Ok(doc) }
        async fn get_document(&self, _id: &str) -> DbResult<Document> { Err(DbError::NotFound("mock".into())) }
        async fn list_documents(&self, _thread_id: Option<&str>) -> DbResult<Vec<Document>> { Ok(vec![]) }
        async fn update_document(&self, _id: &str, _title: Option<&str>, _content: Option<&str>) -> DbResult<Document> { Err(DbError::NotFound("mock".into())) }
        async fn delete_document(&self, _id: &str) -> DbResult<()> { Ok(()) }
        async fn create_thread(&self, thread: Thread) -> DbResult<Thread> { Ok(thread) }
        async fn get_thread(&self, _id: &str) -> DbResult<Thread> { Err(DbError::NotFound("mock".into())) }
        async fn list_threads(&self) -> DbResult<Vec<Thread>> { Ok(vec![]) }
        async fn update_thread(&self, _id: &str, _name: Option<&str>, _description: Option<&str>) -> DbResult<Thread> { Err(DbError::NotFound("mock".into())) }
        async fn delete_thread(&self, _id: &str) -> DbResult<()> { Ok(()) }
        async fn move_document_to_thread(&self, _doc_id: &str, _new_thread_id: &str) -> DbResult<Document> { Err(DbError::NotFound("mock".into())) }
        async fn create_relationship(&self, _from_id: &str, _to_id: &str, _relation_type: RelationType, _strength: f32) -> DbResult<RelatedTo> { Err(DbError::NotFound("mock".into())) }
        async fn list_relationships(&self, _doc_id: &str) -> DbResult<Vec<RelatedTo>> { Ok(vec![]) }
        async fn traverse(&self, _doc_id: &str, _depth: u32, _limit: u32) -> DbResult<Vec<Document>> { Ok(vec![]) }
        async fn adopt_document(&self, _id: &str) -> DbResult<Document> { Err(DbError::NotFound("mock".into())) }
        async fn merge_threads(&self, _target_id: &str, _source_id: &str) -> DbResult<()> { Ok(()) }
        async fn split_thread(&self, _thread_id: &str, _doc_ids: &[String], _new_name: &str) -> DbResult<Thread> { Err(DbError::NotFound("mock".into())) }
        async fn soft_delete_document(&self, _id: &str) -> DbResult<()> { Ok(()) }
        async fn restore_soft_deleted_document(&self, _id: &str) -> DbResult<Document> { Err(DbError::NotFound("mock".into())) }
        async fn soft_delete_thread(&self, _id: &str) -> DbResult<()> { Ok(()) }
        async fn restore_soft_deleted_thread(&self, _id: &str) -> DbResult<Thread> { Err(DbError::NotFound("mock".into())) }
        async fn purge_deleted(&self, _max_age: std::time::Duration) -> DbResult<u64> { Ok(0) }
        async fn commit_document(&self, _doc_id: &str, _message: &str) -> DbResult<Commit> { Err(DbError::NotFound("mock".into())) }
        async fn list_document_commits(&self, _doc_id: &str) -> DbResult<Vec<Commit>> { Ok(vec![]) }
        async fn get_commit(&self, _commit_id: &str) -> DbResult<Commit> { Err(DbError::NotFound("mock".into())) }
        async fn restore_document(&self, _doc_id: &str, _commit_id: &str) -> DbResult<Document> { Err(DbError::NotFound("mock".into())) }
        async fn create_milestone(&self, milestone: Milestone) -> DbResult<Milestone> { Ok(milestone) }
        async fn list_milestones(&self, _thread_id: &str) -> DbResult<Vec<Milestone>> { Ok(vec![]) }
        async fn delete_milestone(&self, _id: &str) -> DbResult<()> { Ok(()) }
        // Contacts
        async fn create_contact(&self, contact: Contact) -> DbResult<Contact> { Ok(contact) }
        async fn get_contact(&self, _id: &str) -> DbResult<Contact> { Err(DbError::NotFound("mock".into())) }
        async fn list_contacts(&self) -> DbResult<Vec<Contact>> { Ok(vec![]) }
        async fn update_contact(&self, _id: &str, _name: Option<&str>, _notes: Option<&str>, _avatar: Option<&str>) -> DbResult<Contact> { Err(DbError::NotFound("mock".into())) }
        async fn delete_contact(&self, _id: &str) -> DbResult<()> { Ok(()) }
        async fn soft_delete_contact(&self, _id: &str) -> DbResult<()> { Ok(()) }
        async fn find_contact_by_address(&self, _address: &str) -> DbResult<Option<Contact>> { Ok(None) }
        async fn add_contact_address(&self, _contact_id: &str, _address: crate::schema::ChannelAddress) -> DbResult<Contact> { Err(DbError::NotFound("mock".into())) }
        // Messages
        async fn create_message(&self, message: Message) -> DbResult<Message> { Ok(message) }
        async fn get_message(&self, _id: &str) -> DbResult<Message> { Err(DbError::NotFound("mock".into())) }
        async fn list_messages(&self, _conversation_id: &str, _before: Option<chrono::DateTime<chrono::Utc>>, _limit: u32) -> DbResult<Vec<Message>> { Ok(vec![]) }
        async fn update_message_read_status(&self, _id: &str, _status: ReadStatus) -> DbResult<Message> { Err(DbError::NotFound("mock".into())) }
        async fn delete_message(&self, _id: &str) -> DbResult<()> { Ok(()) }
        async fn search_messages(&self, _query: &str) -> DbResult<Vec<Message>> { Ok(vec![]) }
        // Conversations
        async fn create_conversation(&self, conversation: Conversation) -> DbResult<Conversation> { Ok(conversation) }
        async fn get_conversation(&self, _id: &str) -> DbResult<Conversation> { Err(DbError::NotFound("mock".into())) }
        async fn list_conversations(&self, _channel: Option<&ChannelType>) -> DbResult<Vec<Conversation>> { Ok(vec![]) }
        async fn update_conversation_unread(&self, _id: &str, _unread_count: u32) -> DbResult<Conversation> { Err(DbError::NotFound("mock".into())) }
        async fn delete_conversation(&self, _id: &str) -> DbResult<()> { Ok(()) }
        async fn link_conversation_to_thread(&self, _conversation_id: &str, _thread_id: &str) -> DbResult<Conversation> { Err(DbError::NotFound("mock".into())) }
    }
}
