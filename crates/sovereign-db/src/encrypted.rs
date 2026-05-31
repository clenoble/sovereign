//! Transparent encryption layer around any GraphDB implementation.
//!
//! Decorator pattern: wraps an inner GraphDB, encrypting document content
//! on write and decrypting on read. Thread/relationship/milestone operations
//! pass through unmodified.

use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine;
use sovereign_crypto::aead;
use sovereign_crypto::device_key::DeviceKey;
use sovereign_crypto::index_key::{self, IndexKey};
use sovereign_crypto::kek::Kek;
use sovereign_crypto::key_db::KeyDatabase;
use tokio::sync::RwLock;

use crate::error::{DbError, DbResult};
use crate::schema::{
    ChannelType, Commit, Contact, Conversation, Document, Entity, Message, Milestone,
    PiiRecord, ReadStatus, RelatedTo, RelationType, ReviewState, ShareRecord, SourceRef,
    SuggestedLink, SuggestionSource, SuggestionStatus, Thread,
};
use crate::traits::GraphDB;

/// Cap on tokens emitted per message into the blind-index. Bounds storage cost
/// on pathological inputs; chosen wide enough not to truncate normal chat.
const MESSAGE_TOKEN_CAP: usize = 256;

/// A GraphDB wrapper that encrypts/decrypts content transparently.
///
/// Per-entity-type key databases (separate `KeyDatabase` files, separate
/// `RwLock`s) keep message and document encryption paths from contending for
/// the same lock. After Phase 2b, threads/conversations/contacts/share_records
/// each have their own KeyDatabase as well.
pub struct EncryptedGraphDB {
    inner: Arc<dyn GraphDB>,
    key_db: Arc<RwLock<KeyDatabase>>,
    messages_key_db: Arc<RwLock<KeyDatabase>>,
    threads_key_db: Arc<RwLock<KeyDatabase>>,
    conversations_key_db: Arc<RwLock<KeyDatabase>>,
    contacts_key_db: Arc<RwLock<KeyDatabase>>,
    share_records_key_db: Arc<RwLock<KeyDatabase>>,
    kek: Arc<Kek>,
    index_key: Arc<IndexKey>,
    /// Needed to persist `KeyDatabase` files: each key DB is encrypted at rest
    /// under the DeviceKey on every save. Without this, keys minted at runtime
    /// would only live in memory and any encrypted row would become unreadable
    /// after app restart.
    device_key: Arc<DeviceKey>,
}

impl EncryptedGraphDB {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        inner: Arc<dyn GraphDB>,
        key_db: Arc<RwLock<KeyDatabase>>,
        messages_key_db: Arc<RwLock<KeyDatabase>>,
        threads_key_db: Arc<RwLock<KeyDatabase>>,
        conversations_key_db: Arc<RwLock<KeyDatabase>>,
        contacts_key_db: Arc<RwLock<KeyDatabase>>,
        share_records_key_db: Arc<RwLock<KeyDatabase>>,
        kek: Arc<Kek>,
        index_key: Arc<IndexKey>,
        device_key: Arc<DeviceKey>,
    ) -> Self {
        Self {
            inner,
            key_db,
            messages_key_db,
            threads_key_db,
            conversations_key_db,
            contacts_key_db,
            share_records_key_db,
            kek,
            index_key,
            device_key,
        }
    }

    /// Encrypt content under a per-entity key from the given key DB. Generates
    /// a fresh nonce per call (safe with random nonces under XChaCha20-Poly1305).
    ///
    /// When a new entity key is minted, the key DB is persisted to disk before
    /// returning. Otherwise restart loses the key and the encrypted row
    /// becomes permanently unreadable. Save failure is logged and falls
    /// through — the row is still encryptable in this session; recovery is
    /// the next time the same entity gets touched (which will re-mint).
    async fn encrypt_with(
        &self,
        key_db: &Arc<RwLock<KeyDatabase>>,
        entity_id: &str,
        plaintext: &[u8],
    ) -> DbResult<(String, String)> {
        let mut kdb = key_db.write().await;
        let (entity_key, minted) = if kdb.contains(entity_id) {
            (
                kdb.unwrap_current(entity_id, &self.kek)
                    .map_err(|e| DbError::Query(format!("key unwrap failed: {e}")))?,
                false,
            )
        } else {
            let epoch = kdb
                .get_all(entity_id)
                .map(|keys| keys.len() as u32 + 1)
                .unwrap_or(1);
            (
                kdb.create_document_key(entity_id, &self.kek, epoch)
                    .map_err(|e| DbError::Query(format!("key creation failed: {e}")))?,
                true,
            )
        };

        if minted {
            if let Err(e) = kdb.save(&self.device_key) {
                tracing::warn!(
                    "key DB save failed after minting key for {entity_id}: {e}. \
                     Row will encrypt in this session but is at risk on restart."
                );
            }
        }

        let (ciphertext, nonce) = aead::encrypt(plaintext, entity_key.as_bytes())
            .map_err(|e| DbError::Query(format!("encryption failed: {e}")))?;

        let b64 = base64::engine::general_purpose::STANDARD;
        Ok((b64.encode(&ciphertext), b64.encode(&nonce)))
    }

    /// Decrypt content with the given key DB. `nonce_b64` must be a 24-byte
    /// XChaCha20 nonce. Returns the plaintext as a UTF-8 String.
    async fn decrypt_with(
        &self,
        key_db: &Arc<RwLock<KeyDatabase>>,
        entity_id: &str,
        ciphertext_b64: &str,
        nonce_b64: &str,
    ) -> DbResult<String> {
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

        let kdb = key_db.read().await;
        let entity_key = kdb.unwrap_current(entity_id, &self.kek)
            .map_err(|e| DbError::Query(format!("key unwrap failed: {e}")))?;

        let plaintext = aead::decrypt(&ciphertext, &nonce, entity_key.as_bytes())
            .map_err(|e| DbError::Query(format!("decryption failed: {e}")))?;

        String::from_utf8(plaintext)
            .map_err(|e| DbError::Query(format!("UTF-8 decode failed: {e}")))
    }

    // -- Document key-db shims (preserve existing call sites) --

    async fn encrypt_content(&self, doc_id: &str, plaintext: &str) -> DbResult<(String, String)> {
        self.encrypt_with(&self.key_db, doc_id, plaintext.as_bytes()).await
    }

    async fn decrypt_content(&self, doc_id: &str, ciphertext_b64: &str, nonce_b64: &str) -> DbResult<String> {
        self.decrypt_with(&self.key_db, doc_id, ciphertext_b64, nonce_b64).await
    }

    // -- Message helpers --

    /// Decrypt a message's encrypted fields in-place. Idempotent on rows with
    /// no body_nonce (treated as plaintext / unencrypted legacy rows).
    async fn decrypt_message(&self, mut msg: Message) -> DbResult<Message> {
        let msg_id = msg.id.as_ref()
            .map(|t| crate::schema::thing_to_raw(t))
            .unwrap_or_default();
        if msg_id.is_empty() {
            return Ok(msg);
        }
        if let Some(nonce) = msg.body_nonce.take() {
            msg.body = self.decrypt_with(&self.messages_key_db, &msg_id, &msg.body, &nonce).await?;
        }
        if let (Some(subject_ct), Some(nonce)) = (msg.subject.take(), msg.subject_nonce.take()) {
            msg.subject = Some(self.decrypt_with(&self.messages_key_db, &msg_id, &subject_ct, &nonce).await?);
        }
        if let (Some(html_ct), Some(nonce)) = (msg.body_html.take(), msg.body_html_nonce.take()) {
            msg.body_html = Some(self.decrypt_with(&self.messages_key_db, &msg_id, &html_ct, &nonce).await?);
        }
        // body_token_hashes are not user-visible — clear so callers can't accidentally re-emit them.
        msg.body_token_hashes.clear();
        Ok(msg)
    }

    async fn decrypt_messages(&self, msgs: Vec<Message>) -> DbResult<Vec<Message>> {
        let mut out = Vec::with_capacity(msgs.len());
        for m in msgs {
            out.push(self.decrypt_message(m).await?);
        }
        Ok(out)
    }

    /// Hash the (lowercased, deduped) tokens in `text` using the per-DB index key.
    fn token_hashes(&self, text: &str) -> Vec<String> {
        let tokens = index_key::tokenize(text, MESSAGE_TOKEN_CAP);
        tokens.iter().map(|t| self.index_key.hash_token(t.as_bytes())).collect()
    }

    /// Decrypt a document's content and title (if encrypted). Idempotent on
    /// rows with no nonces set (treated as plaintext / legacy).
    async fn decrypt_document(&self, mut doc: Document) -> DbResult<Document> {
        let doc_id = doc.id.as_ref()
            .map(|t| crate::schema::thing_to_raw(t))
            .unwrap_or_default();
        if doc_id.is_empty() {
            return Ok(doc);
        }
        if let Some(nonce) = doc.encryption_nonce.take() {
            doc.content = self.decrypt_content(&doc_id, &doc.content, &nonce).await?;
        }
        if let Some(nonce) = doc.title_nonce.take() {
            doc.title = self.decrypt_content(&doc_id, &doc.title, &nonce).await?;
        }
        doc.title_token_hashes.clear();
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

    // -- Thread helpers --

    async fn decrypt_thread(&self, mut thread: Thread) -> DbResult<Thread> {
        let id = thread.id.as_ref()
            .map(|t| crate::schema::thing_to_raw(t))
            .unwrap_or_default();
        if id.is_empty() {
            return Ok(thread);
        }
        if let Some(nonce) = thread.name_nonce.take() {
            thread.name = self.decrypt_with(&self.threads_key_db, &id, &thread.name, &nonce).await?;
        }
        if let Some(nonce) = thread.description_nonce.take() {
            thread.description = self.decrypt_with(&self.threads_key_db, &id, &thread.description, &nonce).await?;
        }
        thread.name_token_hashes.clear();
        Ok(thread)
    }

    async fn decrypt_threads(&self, threads: Vec<Thread>) -> DbResult<Vec<Thread>> {
        let mut out = Vec::with_capacity(threads.len());
        for t in threads {
            out.push(self.decrypt_thread(t).await?);
        }
        Ok(out)
    }

    // -- Conversation helpers --

    async fn decrypt_conversation(&self, mut conv: Conversation) -> DbResult<Conversation> {
        let id = conv.id.as_ref()
            .map(|t| crate::schema::thing_to_raw(t))
            .unwrap_or_default();
        if id.is_empty() {
            return Ok(conv);
        }
        if let Some(nonce) = conv.title_nonce.take() {
            conv.title = self.decrypt_with(&self.conversations_key_db, &id, &conv.title, &nonce).await?;
        }
        Ok(conv)
    }

    async fn decrypt_conversations(&self, convs: Vec<Conversation>) -> DbResult<Vec<Conversation>> {
        let mut out = Vec::with_capacity(convs.len());
        for c in convs {
            out.push(self.decrypt_conversation(c).await?);
        }
        Ok(out)
    }

    // -- Contact helpers --

    async fn decrypt_contact(&self, mut contact: Contact) -> DbResult<Contact> {
        let id = contact.id.as_ref()
            .map(|t| crate::schema::thing_to_raw(t))
            .unwrap_or_default();
        if id.is_empty() {
            return Ok(contact);
        }
        if let Some(nonce) = contact.name_nonce.take() {
            contact.name = self.decrypt_with(&self.contacts_key_db, &id, &contact.name, &nonce).await?;
        }
        // Notes encryption pre-dated Phase 2b under the documents key DB; from
        // 2b onward notes are encrypted under the contacts key DB. Until the
        // wiring slice migrates desktop rows, neither path produces ciphertext
        // in practice (EncryptedGraphDB isn't instantiated at runtime). Tests
        // exercise the contacts-key-DB path.
        if let Some(nonce) = contact.encryption_nonce.take() {
            contact.notes = self.decrypt_with(&self.contacts_key_db, &id, &contact.notes, &nonce).await?;
        }
        Ok(contact)
    }

    async fn decrypt_contacts(&self, contacts: Vec<Contact>) -> DbResult<Vec<Contact>> {
        let mut out = Vec::with_capacity(contacts.len());
        for c in contacts {
            out.push(self.decrypt_contact(c).await?);
        }
        Ok(out)
    }

    // -- ShareRecord helpers --

    async fn decrypt_share_record(&self, mut rec: ShareRecord) -> DbResult<ShareRecord> {
        let id = rec.id.as_ref()
            .map(|t| crate::schema::thing_to_raw(t))
            .unwrap_or_default();
        if id.is_empty() {
            return Ok(rec);
        }
        if let (Some(url_ct), Some(nonce)) = (rec.via_url.take(), rec.via_url_nonce.take()) {
            rec.via_url = Some(self.decrypt_with(&self.share_records_key_db, &id, &url_ct, &nonce).await?);
        }
        Ok(rec)
    }

    async fn decrypt_share_records(&self, recs: Vec<ShareRecord>) -> DbResult<Vec<ShareRecord>> {
        let mut out = Vec::with_capacity(recs.len());
        for r in recs {
            out.push(self.decrypt_share_record(r).await?);
        }
        Ok(out)
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
        // Compute title hashes from plaintext before we lose them.
        let title_hashes = self.token_hashes(&doc.title);

        let created = self.inner.create_document(doc).await?;
        let doc_id = created.id.as_ref()
            .map(|t| crate::schema::thing_to_raw(t))
            .unwrap_or_default();

        // Encrypt content and update via update_document (same as v0.0.5 path).
        let (encrypted_content, _content_nonce) = self.encrypt_content(&doc_id, &created.content).await?;
        self.inner.update_document(&doc_id, None, Some(&encrypted_content)).await?;

        // Encrypt title separately and write through the dedicated setter, which
        // also stores the blind-index token hashes for search.
        let (title_ct, title_nonce) = self.encrypt_with(
            &self.key_db, &doc_id, created.title.as_bytes(),
        ).await?;
        self.inner.set_document_title_encryption(
            &doc_id, &title_ct, &title_nonce, &title_hashes,
        ).await?;

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

        // Update content first (does not touch title fields if `title` is None).
        let mut doc = self.inner.update_document(
            id,
            None,
            encrypted_content.as_deref(),
        ).await?;

        // If the caller passed a new title, encrypt + update token hashes via the dedicated setter.
        if let Some(plaintext_title) = title {
            let hashes = self.token_hashes(plaintext_title);
            let (title_ct, title_nonce) = self.encrypt_with(
                &self.key_db, id, plaintext_title.as_bytes(),
            ).await?;
            self.inner.set_document_title_encryption(id, &title_ct, &title_nonce, &hashes).await?;
            // Refresh — the doc we hold above pre-dates the title setter.
            doc = self.inner.get_document(id).await?;
        }

        self.decrypt_document(doc).await
    }

    async fn delete_document(&self, id: &str) -> DbResult<()> {
        self.inner.delete_document(id).await
    }

    async fn update_document_position(&self, id: &str, x: f32, y: f32) -> DbResult<()> {
        self.inner.update_document_position(id, x, y).await
    }

    async fn search_documents_by_title(&self, query: &str) -> DbResult<Vec<Document>> {
        // Phase 2b: titles are encrypted, so the plaintext CONTAINS path can no
        // longer hit anything. Tokenize the query and route through the
        // blind-index lookup on `title_token_hashes`.
        let hashes = self.token_hashes(query);
        if hashes.is_empty() {
            return Ok(Vec::new());
        }
        let docs = self.inner.search_documents_by_title_token_hashes(&hashes).await?;
        self.decrypt_documents(docs).await
    }

    async fn search_documents_by_title_token_hashes(
        &self,
        hashes: &[String],
    ) -> DbResult<Vec<Document>> {
        let docs = self.inner.search_documents_by_title_token_hashes(hashes).await?;
        self.decrypt_documents(docs).await
    }

    async fn set_document_title_encryption(
        &self,
        id: &str,
        title_ciphertext: &str,
        title_nonce: &str,
        title_token_hashes: &[String],
    ) -> DbResult<()> {
        self.inner.set_document_title_encryption(
            id, title_ciphertext, title_nonce, title_token_hashes,
        ).await
    }

    async fn update_document_reliability(
        &self,
        id: &str,
        source_url: Option<&str>,
        classification: Option<&str>,
        score: Option<f32>,
        assessment_json: Option<&str>,
    ) -> DbResult<Document> {
        let doc = self.inner.update_document_reliability(
            id, source_url, classification, score, assessment_json,
        ).await?;
        self.decrypt_document(doc).await
    }

    // -- Threads: encrypt name + description, blind-index on name --

    async fn create_thread(&self, thread: Thread) -> DbResult<Thread> {
        let name_hashes = self.token_hashes(&thread.name);

        let created = self.inner.create_thread(thread).await?;
        let id = created.id.as_ref()
            .map(|t| crate::schema::thing_to_raw(t))
            .unwrap_or_default();

        let (name_ct, name_nonce) = self.encrypt_with(
            &self.threads_key_db, &id, created.name.as_bytes(),
        ).await?;
        let (desc_ct, desc_nonce) = self.encrypt_with(
            &self.threads_key_db, &id, created.description.as_bytes(),
        ).await?;

        self.inner.set_thread_encryption(
            &id, &name_ct, &name_nonce, &desc_ct, &desc_nonce, &name_hashes,
        ).await?;

        Ok(created)
    }

    async fn get_thread(&self, id: &str) -> DbResult<Thread> {
        let thread = self.inner.get_thread(id).await?;
        self.decrypt_thread(thread).await
    }

    async fn list_threads(&self) -> DbResult<Vec<Thread>> {
        let threads = self.inner.list_threads().await?;
        self.decrypt_threads(threads).await
    }

    async fn find_thread_by_name(&self, name: &str) -> DbResult<Option<Thread>> {
        let hashes = self.token_hashes(name);
        if hashes.is_empty() {
            return Ok(None);
        }
        let thread = self.inner.find_thread_by_name_token_hashes(&hashes).await?;
        match thread {
            Some(t) => Ok(Some(self.decrypt_thread(t).await?)),
            None => Ok(None),
        }
    }

    async fn find_thread_by_name_token_hashes(
        &self,
        hashes: &[String],
    ) -> DbResult<Option<Thread>> {
        let thread = self.inner.find_thread_by_name_token_hashes(hashes).await?;
        match thread {
            Some(t) => Ok(Some(self.decrypt_thread(t).await?)),
            None => Ok(None),
        }
    }

    async fn update_thread(
        &self,
        id: &str,
        name: Option<&str>,
        description: Option<&str>,
    ) -> DbResult<Thread> {
        // Fetch the existing row in raw form so we can preserve whichever
        // ciphertext field the caller isn't replacing.
        let raw = self.inner.get_thread(id).await?;

        // Determine the plaintext name (either new or existing-decrypted) for hash recomputation.
        let plaintext_name: String = match name {
            Some(n) => n.to_string(),
            None => {
                if let Some(nonce) = &raw.name_nonce {
                    self.decrypt_with(&self.threads_key_db, id, &raw.name, nonce).await?
                } else {
                    raw.name.clone()
                }
            }
        };

        let (name_ct, name_nonce) = self.encrypt_with(
            &self.threads_key_db, id, plaintext_name.as_bytes(),
        ).await?;

        let (desc_ct, desc_nonce) = match description {
            Some(d) => self.encrypt_with(&self.threads_key_db, id, d.as_bytes()).await?,
            None => {
                // Re-encrypt the existing description under a fresh nonce. (Cheap;
                // keeps the row's nonce-policy uniform across UPDATEs.)
                let plain = if let Some(nonce) = &raw.description_nonce {
                    self.decrypt_with(&self.threads_key_db, id, &raw.description, nonce).await?
                } else {
                    raw.description.clone()
                };
                self.encrypt_with(&self.threads_key_db, id, plain.as_bytes()).await?
            }
        };

        let hashes = self.token_hashes(&plaintext_name);

        self.inner.set_thread_encryption(
            id, &name_ct, &name_nonce, &desc_ct, &desc_nonce, &hashes,
        ).await?;

        // The inner update_thread is the place that bumps modified_at — call it
        // with no field changes (None, None) so it just touches the timestamp.
        let updated = self.inner.update_thread(id, None, None).await?;
        self.decrypt_thread(updated).await
    }

    async fn delete_thread(&self, id: &str) -> DbResult<()> {
        self.inner.delete_thread(id).await
    }

    async fn set_thread_encryption(
        &self,
        id: &str,
        name_ciphertext: &str,
        name_nonce: &str,
        description_ciphertext: &str,
        description_nonce: &str,
        name_token_hashes: &[String],
    ) -> DbResult<()> {
        self.inner.set_thread_encryption(
            id, name_ciphertext, name_nonce,
            description_ciphertext, description_nonce, name_token_hashes,
        ).await
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

    async fn list_outgoing_relationships(&self, doc_id: &str) -> DbResult<Vec<RelatedTo>> {
        self.inner.list_outgoing_relationships(doc_id).await
    }

    async fn list_incoming_relationships(&self, doc_id: &str) -> DbResult<Vec<RelatedTo>> {
        self.inner.list_incoming_relationships(doc_id).await
    }

    async fn list_all_relationships(&self) -> DbResult<Vec<RelatedTo>> {
        self.inner.list_all_relationships().await
    }

    async fn traverse(&self, doc_id: &str, depth: u32, limit: u32) -> DbResult<Vec<Document>> {
        let docs = self.inner.traverse(doc_id, depth, limit).await?;
        self.decrypt_documents(docs).await
    }

    // Suggested links: not encrypted (rationale text is AI-generated, not user content)
    async fn create_suggested_link(
        &self,
        from_id: &str,
        to_id: &str,
        relation_type: RelationType,
        strength: f32,
        rationale: &str,
        source: SuggestionSource,
    ) -> DbResult<SuggestedLink> {
        self.inner.create_suggested_link(from_id, to_id, relation_type, strength, rationale, source).await
    }

    async fn list_pending_suggestions(&self) -> DbResult<Vec<SuggestedLink>> {
        self.inner.list_pending_suggestions().await
    }

    async fn list_suggestions_for_document(&self, doc_id: &str) -> DbResult<Vec<SuggestedLink>> {
        self.inner.list_suggestions_for_document(doc_id).await
    }

    async fn resolve_suggestion(
        &self,
        id: &str,
        status: SuggestionStatus,
    ) -> DbResult<SuggestedLink> {
        self.inner.resolve_suggestion(id, status).await
    }

    async fn suggestion_exists(&self, from_id: &str, to_id: &str) -> DbResult<bool> {
        self.inner.suggestion_exists(from_id, to_id).await
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

    async fn list_all_milestones(&self) -> DbResult<Vec<Milestone>> {
        self.inner.list_all_milestones().await
    }

    async fn delete_milestone(&self, id: &str) -> DbResult<()> {
        self.inner.delete_milestone(id).await
    }

    // -- Contacts: encrypt name (new in 2b) + notes (existed pre-2b, now under contacts key DB) ---

    async fn create_contact(&self, contact: Contact) -> DbResult<Contact> {
        let created = self.inner.create_contact(contact).await?;
        let id = created.id.as_ref()
            .map(|t| crate::schema::thing_to_raw(t))
            .unwrap_or_default();

        // Always encrypt the name (no plaintext fallback in the 2b model).
        let (name_ct, name_nonce) = self.encrypt_with(
            &self.contacts_key_db, &id, created.name.as_bytes(),
        ).await?;
        self.inner.set_contact_name_encryption(&id, &name_ct, &name_nonce).await?;

        // Notes: only encrypt if non-empty, mirroring the pre-2b behaviour.
        // Going forward this is done under the contacts key DB (not the documents key DB
        // — pre-2b had a latent bug here: notes ciphertext landed in the row but the
        // nonce companion was never written, so subsequent reads returned the ciphertext
        // as plaintext. set_contact_notes_encryption writes both atomically.
        if !created.notes.is_empty() {
            let (notes_ct, notes_nonce) = self.encrypt_with(
                &self.contacts_key_db, &id, created.notes.as_bytes(),
            ).await?;
            self.inner.set_contact_notes_encryption(&id, &notes_ct, &notes_nonce).await?;
        }

        Ok(created)
    }

    async fn get_contact(&self, id: &str) -> DbResult<Contact> {
        let contact = self.inner.get_contact(id).await?;
        self.decrypt_contact(contact).await
    }

    async fn list_contacts(&self) -> DbResult<Vec<Contact>> {
        let contacts = self.inner.list_contacts().await?;
        self.decrypt_contacts(contacts).await
    }

    async fn update_contact(
        &self,
        id: &str,
        name: Option<&str>,
        notes: Option<&str>,
        avatar: Option<&str>,
    ) -> DbResult<Contact> {
        // Update avatar (and only avatar) through the inner update path. Name and
        // notes are encrypted and routed through dedicated setters below so the
        // ciphertext/nonce pairs land atomically.
        if avatar.is_some() {
            self.inner.update_contact(id, None, None, avatar).await?;
        }

        if let Some(n) = name {
            let (name_ct, name_nonce) = self.encrypt_with(
                &self.contacts_key_db, id, n.as_bytes(),
            ).await?;
            self.inner.set_contact_name_encryption(id, &name_ct, &name_nonce).await?;
        }

        if let Some(n) = notes {
            let (notes_ct, notes_nonce) = self.encrypt_with(
                &self.contacts_key_db, id, n.as_bytes(),
            ).await?;
            self.inner.set_contact_notes_encryption(id, &notes_ct, &notes_nonce).await?;
        }

        let fresh = self.inner.get_contact(id).await?;
        self.decrypt_contact(fresh).await
    }

    async fn delete_contact(&self, id: &str) -> DbResult<()> {
        self.inner.delete_contact(id).await
    }

    async fn soft_delete_contact(&self, id: &str) -> DbResult<()> {
        self.inner.soft_delete_contact(id).await
    }

    async fn find_contact_by_address(&self, address: &str) -> DbResult<Option<Contact>> {
        // Address lookup goes through plaintext addresses field, which is not
        // encrypted in 2b (Vec<ChannelAddress> per-element encryption is deferred).
        match self.inner.find_contact_by_address(address).await? {
            Some(c) => Ok(Some(self.decrypt_contact(c).await?)),
            None => Ok(None),
        }
    }

    async fn add_contact_address(
        &self,
        contact_id: &str,
        address: crate::schema::ChannelAddress,
    ) -> DbResult<Contact> {
        let contact = self.inner.add_contact_address(contact_id, address).await?;
        self.decrypt_contact(contact).await
    }

    async fn set_contact_name_encryption(
        &self,
        id: &str,
        name_ciphertext: &str,
        name_nonce: &str,
    ) -> DbResult<()> {
        self.inner.set_contact_name_encryption(id, name_ciphertext, name_nonce).await
    }

    async fn set_contact_notes_encryption(
        &self,
        id: &str,
        notes_ciphertext: &str,
        notes_nonce: &str,
    ) -> DbResult<()> {
        self.inner.set_contact_notes_encryption(id, notes_ciphertext, notes_nonce).await
    }

    // -- Messages: encrypt body, subject, body_html with per-field nonces ---

    async fn create_message(&self, message: Message) -> DbResult<Message> {
        // Compute blind-index hashes from plaintext before we lose them.
        let mut combined = String::with_capacity(
            message.body.len() + message.subject.as_deref().map(|s| s.len() + 1).unwrap_or(0),
        );
        if let Some(s) = &message.subject {
            combined.push_str(s);
            combined.push(' ');
        }
        combined.push_str(&message.body);
        let token_hashes = self.token_hashes(&combined);

        // Create first so the DB assigns an ID; that ID is the key-DB entry name.
        let created = self.inner.create_message(message).await?;
        let msg_id = created.id.as_ref()
            .map(|t| crate::schema::thing_to_raw(t))
            .unwrap_or_default();

        // Encrypt each field with a fresh nonce under the per-message key.
        let (body_ct, body_nonce) = self.encrypt_with(
            &self.messages_key_db, &msg_id, created.body.as_bytes(),
        ).await?;
        let subject_enc = if let Some(s) = &created.subject {
            let (ct, n) = self.encrypt_with(
                &self.messages_key_db, &msg_id, s.as_bytes(),
            ).await?;
            Some((ct, n))
        } else {
            None
        };
        let body_html_enc = if let Some(h) = &created.body_html {
            let (ct, n) = self.encrypt_with(
                &self.messages_key_db, &msg_id, h.as_bytes(),
            ).await?;
            Some((ct, n))
        } else {
            None
        };

        self.inner.set_message_encryption(
            &msg_id,
            &body_ct, &body_nonce,
            subject_enc.as_ref().map(|(ct, _)| ct.as_str()),
            subject_enc.as_ref().map(|(_, n)| n.as_str()),
            body_html_enc.as_ref().map(|(ct, _)| ct.as_str()),
            body_html_enc.as_ref().map(|(_, n)| n.as_str()),
            &token_hashes,
        ).await?;

        // Return plaintext to the caller (created already has plaintext fields).
        Ok(created)
    }

    async fn get_message(&self, id: &str) -> DbResult<Message> {
        let msg = self.inner.get_message(id).await?;
        self.decrypt_message(msg).await
    }

    async fn list_messages(
        &self,
        conversation_id: &str,
        before: Option<chrono::DateTime<chrono::Utc>>,
        limit: u32,
    ) -> DbResult<Vec<Message>> {
        let msgs = self.inner.list_messages(conversation_id, before, limit).await?;
        self.decrypt_messages(msgs).await
    }

    async fn update_message_read_status(
        &self,
        id: &str,
        status: ReadStatus,
    ) -> DbResult<Message> {
        let msg = self.inner.update_message_read_status(id, status).await?;
        self.decrypt_message(msg).await
    }

    async fn delete_message(&self, id: &str) -> DbResult<()> {
        self.inner.delete_message(id).await
    }

    async fn list_all_messages(&self) -> DbResult<Vec<Message>> {
        let msgs = self.inner.list_all_messages().await?;
        self.decrypt_messages(msgs).await
    }

    async fn list_messages_in_time_range(
        &self,
        after: chrono::DateTime<chrono::Utc>,
        before: chrono::DateTime<chrono::Utc>,
        limit: u32,
    ) -> DbResult<Vec<Message>> {
        let msgs = self.inner.list_messages_in_time_range(after, before, limit).await?;
        self.decrypt_messages(msgs).await
    }

    async fn search_messages(&self, query: &str) -> DbResult<Vec<Message>> {
        // Tokenize the query, HMAC the tokens, and delegate to the blind-index lookup.
        // Empty hash list short-circuits (no tokens => no match).
        let hashes = self.token_hashes(query);
        if hashes.is_empty() {
            return Ok(Vec::new());
        }
        let msgs = self.inner.search_messages_by_token_hashes(&hashes).await?;
        self.decrypt_messages(msgs).await
    }

    async fn search_messages_by_token_hashes(
        &self,
        hashes: &[String],
    ) -> DbResult<Vec<Message>> {
        // Direct pass-through: caller already hashed under our index key.
        let msgs = self.inner.search_messages_by_token_hashes(hashes).await?;
        self.decrypt_messages(msgs).await
    }

    async fn set_message_encryption(
        &self,
        id: &str,
        body_ciphertext: &str,
        body_nonce: &str,
        subject_ciphertext: Option<&str>,
        subject_nonce: Option<&str>,
        body_html_ciphertext: Option<&str>,
        body_html_nonce: Option<&str>,
        body_token_hashes: &[String],
    ) -> DbResult<()> {
        // Encrypted DB is the only legitimate caller; forward unchanged.
        self.inner.set_message_encryption(
            id,
            body_ciphertext, body_nonce,
            subject_ciphertext, subject_nonce,
            body_html_ciphertext, body_html_nonce,
            body_token_hashes,
        ).await
    }

    // -- Conversations: encrypt title (no search trait method exists) --

    async fn create_conversation(&self, conversation: Conversation) -> DbResult<Conversation> {
        let created = self.inner.create_conversation(conversation).await?;
        let id = created.id.as_ref()
            .map(|t| crate::schema::thing_to_raw(t))
            .unwrap_or_default();

        let (title_ct, title_nonce) = self.encrypt_with(
            &self.conversations_key_db, &id, created.title.as_bytes(),
        ).await?;
        self.inner.set_conversation_title_encryption(&id, &title_ct, &title_nonce).await?;

        Ok(created)
    }

    async fn get_conversation(&self, id: &str) -> DbResult<Conversation> {
        let conv = self.inner.get_conversation(id).await?;
        self.decrypt_conversation(conv).await
    }

    async fn list_conversations(
        &self,
        channel: Option<&ChannelType>,
    ) -> DbResult<Vec<Conversation>> {
        let convs = self.inner.list_conversations(channel).await?;
        self.decrypt_conversations(convs).await
    }

    async fn update_conversation_unread(
        &self,
        id: &str,
        unread_count: u32,
    ) -> DbResult<Conversation> {
        let conv = self.inner.update_conversation_unread(id, unread_count).await?;
        self.decrypt_conversation(conv).await
    }

    async fn update_conversation_last_message_at(
        &self,
        id: &str,
        at: chrono::DateTime<chrono::Utc>,
    ) -> DbResult<Conversation> {
        let conv = self.inner.update_conversation_last_message_at(id, at).await?;
        self.decrypt_conversation(conv).await
    }

    async fn delete_conversation(&self, id: &str) -> DbResult<()> {
        self.inner.delete_conversation(id).await
    }

    async fn link_conversation_to_thread(
        &self,
        conversation_id: &str,
        thread_id: &str,
    ) -> DbResult<Conversation> {
        let conv = self.inner.link_conversation_to_thread(conversation_id, thread_id).await?;
        self.decrypt_conversation(conv).await
    }

    async fn set_conversation_title_encryption(
        &self,
        id: &str,
        title_ciphertext: &str,
        title_nonce: &str,
    ) -> DbResult<()> {
        self.inner.set_conversation_title_encryption(id, title_ciphertext, title_nonce).await
    }

    // -- Entities and PII records pass through unencrypted by this
    //    decorator: PiiRecord values are already ciphertext (encrypted
    //    under DeviceKey by the AI layer's vault primitive), and Entity
    //    has no fields that need additional encryption.

    async fn create_entity(&self, entity: Entity) -> DbResult<Entity> {
        self.inner.create_entity(entity).await
    }

    async fn list_entities(&self) -> DbResult<Vec<Entity>> {
        self.inner.list_entities().await
    }

    async fn create_pii_record(&self, record: PiiRecord) -> DbResult<PiiRecord> {
        self.inner.create_pii_record(record).await
    }

    async fn get_pii_record(&self, id: &str) -> DbResult<PiiRecord> {
        // PiiRecord values are encrypted at the AI layer (vault primitive)
        // before reaching the DB — this decorator passes through.
        self.inner.get_pii_record(id).await
    }

    async fn list_pii_records(
        &self,
        entity_id: Option<&str>,
        review_state: Option<ReviewState>,
        stored_secret: Option<bool>,
    ) -> DbResult<Vec<PiiRecord>> {
        self.inner
            .list_pii_records(entity_id, review_state, stored_secret)
            .await
    }

    async fn update_pii_record_review_state(
        &self,
        id: &str,
        review_state: ReviewState,
    ) -> DbResult<()> {
        self.inner.update_pii_record_review_state(id, review_state).await
    }

    async fn update_pii_record_value(
        &self,
        id: &str,
        value_encrypted: &str,
        value_nonce: &str,
    ) -> DbResult<()> {
        self.inner
            .update_pii_record_value(id, value_encrypted, value_nonce)
            .await
    }

    async fn soft_delete_pii_record(&self, id: &str) -> DbResult<()> {
        self.inner.soft_delete_pii_record(id).await
    }

    async fn get_entity(&self, id: &str) -> DbResult<Entity> {
        self.inner.get_entity(id).await
    }

    async fn create_share_record(&self, record: ShareRecord) -> DbResult<ShareRecord> {
        // Capture the plaintext via_url before the create, so we can re-encrypt
        // afterward (the DB-assigned ID is needed to mint the per-record key).
        let plain_url = record.via_url.clone();
        let created = self.inner.create_share_record(record).await?;

        if let Some(url) = plain_url {
            let id = created.id.as_ref()
                .map(|t| crate::schema::thing_to_raw(t))
                .unwrap_or_default();
            let (url_ct, url_nonce) = self.encrypt_with(
                &self.share_records_key_db, &id, url.as_bytes(),
            ).await?;
            self.inner.set_share_record_via_url_encryption(&id, &url_ct, &url_nonce).await?;
        }

        Ok(created)
    }

    async fn list_share_records_for_entity(
        &self,
        entity_id: &str,
    ) -> DbResult<Vec<ShareRecord>> {
        let recs = self.inner.list_share_records_for_entity(entity_id).await?;
        self.decrypt_share_records(recs).await
    }

    async fn list_all_share_records(&self) -> DbResult<Vec<ShareRecord>> {
        let recs = self.inner.list_all_share_records().await?;
        self.decrypt_share_records(recs).await
    }

    async fn get_share_record(&self, id: &str) -> DbResult<ShareRecord> {
        let rec = self.inner.get_share_record(id).await?;
        self.decrypt_share_record(rec).await
    }

    async fn set_share_record_via_url_encryption(
        &self,
        id: &str,
        via_url_ciphertext: &str,
        via_url_nonce: &str,
    ) -> DbResult<()> {
        self.inner.set_share_record_via_url_encryption(id, via_url_ciphertext, via_url_nonce).await
    }

    async fn update_pii_record_sources(
        &self,
        id: &str,
        sources: Vec<SourceRef>,
    ) -> DbResult<()> {
        self.inner.update_pii_record_sources(id, sources).await
    }

    async fn update_pii_record_revealed_at(
        &self,
        id: &str,
        last_revealed_at: chrono::DateTime<chrono::Utc>,
    ) -> DbResult<()> {
        self.inner.update_pii_record_revealed_at(id, last_revealed_at).await
    }

    async fn update_document_pii_fields(
        &self,
        id: &str,
        body_raw_encrypted: Option<&str>,
        body_raw_nonce: Option<&str>,
        pii_scanned_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> DbResult<()> {
        self.inner
            .update_document_pii_fields(id, body_raw_encrypted, body_raw_nonce, pii_scanned_at)
            .await
    }

    async fn update_message_body(
        &self,
        id: &str,
        body: &str,
        body_html: Option<&str>,
    ) -> DbResult<()> {
        // PII pipeline rewrites the canonical body in place. To preserve the
        // Phase-2a invariants we must: (a) re-encrypt the new plaintext body
        // and body_html under the existing per-message key with fresh nonces,
        // (b) recompute the blind-index hashes from the new plaintext body
        // plus the unchanged plaintext subject, and (c) leave the subject
        // ciphertext+nonce on disk unchanged.
        let raw = self.inner.get_message(id).await?;
        let plaintext_subject = match (&raw.subject, &raw.subject_nonce) {
            (Some(ct), Some(nonce)) => Some(
                self.decrypt_with(&self.messages_key_db, id, ct, nonce).await?,
            ),
            (Some(plain), None) => Some(plain.clone()), // legacy plaintext row
            _ => None,
        };

        let (body_ct, body_nonce) = self.encrypt_with(
            &self.messages_key_db, id, body.as_bytes(),
        ).await?;
        let html_enc = if let Some(h) = body_html {
            let (ct, n) = self.encrypt_with(
                &self.messages_key_db, id, h.as_bytes(),
            ).await?;
            Some((ct, n))
        } else {
            None
        };

        let mut combined = String::new();
        if let Some(s) = &plaintext_subject {
            combined.push_str(s);
            combined.push(' ');
        }
        combined.push_str(body);
        let token_hashes = self.token_hashes(&combined);

        self.inner.set_message_encryption(
            id,
            &body_ct, &body_nonce,
            raw.subject.as_deref(),
            raw.subject_nonce.as_deref(),
            html_enc.as_ref().map(|(ct, _)| ct.as_str()),
            html_enc.as_ref().map(|(_, n)| n.as_str()),
            &token_hashes,
        ).await
    }

    async fn update_message_pii_fields(
        &self,
        id: &str,
        body_raw_encrypted: Option<&str>,
        body_raw_nonce: Option<&str>,
        pii_scanned_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> DbResult<()> {
        // body_raw_* is already ciphertext from the AI layer's vault
        // primitive — pass through.
        self.inner
            .update_message_pii_fields(id, body_raw_encrypted, body_raw_nonce, pii_scanned_at)
            .await
    }

    async fn update_contact_pii_fields(
        &self,
        id: &str,
        pii_scanned_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> DbResult<()> {
        self.inner.update_contact_pii_fields(id, pii_scanned_at).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sovereign_crypto::kek::Kek;
    use sovereign_crypto::key_db::KeyDatabase;

    fn test_kek() -> Kek {
        Kek::generate()
    }

    fn test_key_db() -> KeyDatabase {
        KeyDatabase::new(std::env::temp_dir().join("test-encrypted-db-keys.db"))
    }

    fn test_device_key() -> sovereign_crypto::device_key::DeviceKey {
        use sovereign_crypto::master_key::MasterKey;
        let mk = MasterKey::from_passphrase(b"test", b"salt").unwrap();
        sovereign_crypto::device_key::DeviceKey::derive(&mk, "test-device").unwrap()
    }

    #[test]
    fn encrypted_graph_db_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<EncryptedGraphDB>();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn encrypt_decrypt_content_roundtrip() {
        let kek = test_kek();
        let key_db = Arc::new(RwLock::new(test_key_db()));

        // Create a mock "inner" — we just test the encrypt/decrypt helpers
        let encrypted_db = EncryptedGraphDB {
            inner: Arc::new(MockDb),
            key_db: key_db.clone(),
            messages_key_db: Arc::new(RwLock::new(KeyDatabase::new(std::env::temp_dir().join("test-encrypted-db-msgkeys.db")))),
            threads_key_db: Arc::new(RwLock::new(KeyDatabase::new(std::env::temp_dir().join("test-encrypted-db-thrkeys-a.db")))),
            conversations_key_db: Arc::new(RwLock::new(KeyDatabase::new(std::env::temp_dir().join("test-encrypted-db-convkeys-a.db")))),
            contacts_key_db: Arc::new(RwLock::new(KeyDatabase::new(std::env::temp_dir().join("test-encrypted-db-conkeys-a.db")))),
            share_records_key_db: Arc::new(RwLock::new(KeyDatabase::new(std::env::temp_dir().join("test-encrypted-db-shrkeys-a.db")))),
            kek: Arc::new(Kek::from_bytes(*kek.as_bytes())),
            index_key: Arc::new(IndexKey::generate()),
            device_key: Arc::new(test_device_key()),
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
        let key_db = Arc::new(RwLock::new(test_key_db()));

        let encrypted_db = EncryptedGraphDB {
            inner: Arc::new(MockDb),
            key_db,
            messages_key_db: Arc::new(RwLock::new(KeyDatabase::new(std::env::temp_dir().join("test-encrypted-db-msgkeys2.db")))),
            threads_key_db: Arc::new(RwLock::new(KeyDatabase::new(std::env::temp_dir().join("test-encrypted-db-thrkeys-b.db")))),
            conversations_key_db: Arc::new(RwLock::new(KeyDatabase::new(std::env::temp_dir().join("test-encrypted-db-convkeys-b.db")))),
            contacts_key_db: Arc::new(RwLock::new(KeyDatabase::new(std::env::temp_dir().join("test-encrypted-db-conkeys-b.db")))),
            share_records_key_db: Arc::new(RwLock::new(KeyDatabase::new(std::env::temp_dir().join("test-encrypted-db-shrkeys-b.db")))),
            kek: Arc::new(Kek::from_bytes(*kek.as_bytes())),
            index_key: Arc::new(IndexKey::generate()),
            device_key: Arc::new(test_device_key()),
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
        let key_db = Arc::new(RwLock::new(test_key_db()));

        let msg_db1 = Arc::new(RwLock::new(KeyDatabase::new(std::env::temp_dir().join("test-encrypted-db-msgkeys3.db"))));
        let thr_db1 = Arc::new(RwLock::new(KeyDatabase::new(std::env::temp_dir().join("test-encrypted-db-thrkeys-c.db"))));
        let conv_db1 = Arc::new(RwLock::new(KeyDatabase::new(std::env::temp_dir().join("test-encrypted-db-convkeys-c.db"))));
        let con_db1 = Arc::new(RwLock::new(KeyDatabase::new(std::env::temp_dir().join("test-encrypted-db-conkeys-c.db"))));
        let shr_db1 = Arc::new(RwLock::new(KeyDatabase::new(std::env::temp_dir().join("test-encrypted-db-shrkeys-c.db"))));
        let db1 = EncryptedGraphDB {
            inner: Arc::new(MockDb),
            key_db: key_db.clone(),
            messages_key_db: msg_db1.clone(),
            threads_key_db: thr_db1.clone(),
            conversations_key_db: conv_db1.clone(),
            contacts_key_db: con_db1.clone(),
            share_records_key_db: shr_db1.clone(),
            kek: Arc::new(Kek::from_bytes(*kek1.as_bytes())),
            index_key: Arc::new(IndexKey::generate()),
            device_key: Arc::new(test_device_key()),
        };

        let (ct, nonce) = db1.encrypt_content("document:wrongkey", "secret").await.unwrap();

        // Try to decrypt with a different KEK — the key_db has the key wrapped with kek1
        let db2 = EncryptedGraphDB {
            inner: Arc::new(MockDb),
            key_db: key_db.clone(), // same key_db but different KEK
            messages_key_db: msg_db1.clone(),
            threads_key_db: thr_db1.clone(),
            conversations_key_db: conv_db1.clone(),
            contacts_key_db: con_db1.clone(),
            share_records_key_db: shr_db1.clone(),
            kek: Arc::new(Kek::from_bytes(*kek2.as_bytes())),
            index_key: Arc::new(IndexKey::generate()),
            device_key: Arc::new(test_device_key()),
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
        async fn update_document_position(&self, _id: &str, _x: f32, _y: f32) -> DbResult<()> { Ok(()) }
        async fn search_documents_by_title(&self, _query: &str) -> DbResult<Vec<Document>> { Ok(vec![]) }
        async fn search_documents_by_title_token_hashes(&self, _hashes: &[String]) -> DbResult<Vec<Document>> { Ok(vec![]) }
        async fn set_document_title_encryption(&self, _id: &str, _title_ciphertext: &str, _title_nonce: &str, _title_token_hashes: &[String]) -> DbResult<()> { Ok(()) }
        async fn update_document_reliability(&self, _id: &str, _source_url: Option<&str>, _classification: Option<&str>, _score: Option<f32>, _assessment_json: Option<&str>) -> DbResult<Document> { Err(DbError::NotFound("mock".into())) }
        async fn create_suggested_link(&self, _from_id: &str, _to_id: &str, _relation_type: RelationType, _strength: f32, _rationale: &str, _source: SuggestionSource) -> DbResult<SuggestedLink> { Err(DbError::NotFound("mock".into())) }
        async fn list_pending_suggestions(&self) -> DbResult<Vec<SuggestedLink>> { Ok(vec![]) }
        async fn list_suggestions_for_document(&self, _doc_id: &str) -> DbResult<Vec<SuggestedLink>> { Ok(vec![]) }
        async fn resolve_suggestion(&self, _id: &str, _status: SuggestionStatus) -> DbResult<SuggestedLink> { Err(DbError::NotFound("mock".into())) }
        async fn suggestion_exists(&self, _from_id: &str, _to_id: &str) -> DbResult<bool> { Ok(false) }
        async fn create_thread(&self, thread: Thread) -> DbResult<Thread> { Ok(thread) }
        async fn get_thread(&self, _id: &str) -> DbResult<Thread> { Err(DbError::NotFound("mock".into())) }
        async fn list_threads(&self) -> DbResult<Vec<Thread>> { Ok(vec![]) }
        async fn update_thread(&self, _id: &str, _name: Option<&str>, _description: Option<&str>) -> DbResult<Thread> { Err(DbError::NotFound("mock".into())) }
        async fn delete_thread(&self, _id: &str) -> DbResult<()> { Ok(()) }
        async fn find_thread_by_name(&self, _name: &str) -> DbResult<Option<Thread>> { Ok(None) }
        async fn find_thread_by_name_token_hashes(&self, _hashes: &[String]) -> DbResult<Option<Thread>> { Ok(None) }
        async fn set_thread_encryption(&self, _id: &str, _name_ciphertext: &str, _name_nonce: &str, _description_ciphertext: &str, _description_nonce: &str, _name_token_hashes: &[String]) -> DbResult<()> { Ok(()) }
        async fn move_document_to_thread(&self, _doc_id: &str, _new_thread_id: &str) -> DbResult<Document> { Err(DbError::NotFound("mock".into())) }
        async fn create_relationship(&self, _from_id: &str, _to_id: &str, _relation_type: RelationType, _strength: f32) -> DbResult<RelatedTo> { Err(DbError::NotFound("mock".into())) }
        async fn list_outgoing_relationships(&self, _doc_id: &str) -> DbResult<Vec<RelatedTo>> { Ok(vec![]) }
        async fn list_incoming_relationships(&self, _doc_id: &str) -> DbResult<Vec<RelatedTo>> { Ok(vec![]) }
        async fn list_all_relationships(&self) -> DbResult<Vec<RelatedTo>> { Ok(vec![]) }
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
        async fn list_all_milestones(&self) -> DbResult<Vec<Milestone>> { Ok(vec![]) }
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
        async fn set_contact_name_encryption(&self, _id: &str, _name_ciphertext: &str, _name_nonce: &str) -> DbResult<()> { Ok(()) }
        async fn set_contact_notes_encryption(&self, _id: &str, _notes_ciphertext: &str, _notes_nonce: &str) -> DbResult<()> { Ok(()) }
        // Messages
        async fn create_message(&self, message: Message) -> DbResult<Message> { Ok(message) }
        async fn get_message(&self, _id: &str) -> DbResult<Message> { Err(DbError::NotFound("mock".into())) }
        async fn list_messages(&self, _conversation_id: &str, _before: Option<chrono::DateTime<chrono::Utc>>, _limit: u32) -> DbResult<Vec<Message>> { Ok(vec![]) }
        async fn update_message_read_status(&self, _id: &str, _status: ReadStatus) -> DbResult<Message> { Err(DbError::NotFound("mock".into())) }
        async fn delete_message(&self, _id: &str) -> DbResult<()> { Ok(()) }
        async fn list_all_messages(&self) -> DbResult<Vec<Message>> { Ok(vec![]) }
        async fn list_messages_in_time_range(&self, _after: chrono::DateTime<chrono::Utc>, _before: chrono::DateTime<chrono::Utc>, _limit: u32) -> DbResult<Vec<Message>> { Ok(vec![]) }
        async fn search_messages(&self, _query: &str) -> DbResult<Vec<Message>> { Ok(vec![]) }
        async fn search_messages_by_token_hashes(&self, _hashes: &[String]) -> DbResult<Vec<Message>> { Ok(vec![]) }
        async fn set_message_encryption(
            &self,
            _id: &str,
            _body_ciphertext: &str,
            _body_nonce: &str,
            _subject_ciphertext: Option<&str>,
            _subject_nonce: Option<&str>,
            _body_html_ciphertext: Option<&str>,
            _body_html_nonce: Option<&str>,
            _body_token_hashes: &[String],
        ) -> DbResult<()> { Ok(()) }
        // Conversations
        async fn create_conversation(&self, conversation: Conversation) -> DbResult<Conversation> { Ok(conversation) }
        async fn get_conversation(&self, _id: &str) -> DbResult<Conversation> { Err(DbError::NotFound("mock".into())) }
        async fn list_conversations(&self, _channel: Option<&ChannelType>) -> DbResult<Vec<Conversation>> { Ok(vec![]) }
        async fn update_conversation_unread(&self, _id: &str, _unread_count: u32) -> DbResult<Conversation> { Err(DbError::NotFound("mock".into())) }
        async fn update_conversation_last_message_at(&self, _id: &str, _at: chrono::DateTime<chrono::Utc>) -> DbResult<Conversation> { Err(DbError::NotFound("mock".into())) }
        async fn delete_conversation(&self, _id: &str) -> DbResult<()> { Ok(()) }
        async fn link_conversation_to_thread(&self, _conversation_id: &str, _thread_id: &str) -> DbResult<Conversation> { Err(DbError::NotFound("mock".into())) }
        async fn set_conversation_title_encryption(&self, _id: &str, _title_ciphertext: &str, _title_nonce: &str) -> DbResult<()> { Ok(()) }
        // Entities + PII records
        async fn create_entity(&self, entity: Entity) -> DbResult<Entity> { Ok(entity) }
        async fn list_entities(&self) -> DbResult<Vec<Entity>> { Ok(vec![]) }
        async fn create_pii_record(&self, record: PiiRecord) -> DbResult<PiiRecord> { Ok(record) }
        async fn get_pii_record(&self, _id: &str) -> DbResult<PiiRecord> { Err(DbError::NotFound("mock".into())) }
        async fn list_pii_records(&self, _entity_id: Option<&str>, _review_state: Option<ReviewState>, _stored_secret: Option<bool>) -> DbResult<Vec<PiiRecord>> { Ok(vec![]) }
        async fn update_pii_record_review_state(&self, _id: &str, _review_state: ReviewState) -> DbResult<()> { Ok(()) }
        async fn update_pii_record_value(&self, _id: &str, _value_encrypted: &str, _value_nonce: &str) -> DbResult<()> { Ok(()) }
        async fn soft_delete_pii_record(&self, _id: &str) -> DbResult<()> { Ok(()) }
        async fn get_entity(&self, _id: &str) -> DbResult<Entity> { Err(DbError::NotFound("mock".into())) }
        async fn create_share_record(&self, record: ShareRecord) -> DbResult<ShareRecord> { Ok(record) }
        async fn set_share_record_via_url_encryption(&self, _id: &str, _via_url_ciphertext: &str, _via_url_nonce: &str) -> DbResult<()> { Ok(()) }
        async fn list_share_records_for_entity(&self, _entity_id: &str) -> DbResult<Vec<ShareRecord>> { Ok(vec![]) }
        async fn list_all_share_records(&self) -> DbResult<Vec<ShareRecord>> { Ok(vec![]) }
        async fn get_share_record(&self, _id: &str) -> DbResult<ShareRecord> { Err(DbError::NotFound("mock".into())) }
        async fn update_pii_record_sources(&self, _id: &str, _sources: Vec<SourceRef>) -> DbResult<()> { Ok(()) }
        async fn update_pii_record_revealed_at(&self, _id: &str, _last_revealed_at: chrono::DateTime<chrono::Utc>) -> DbResult<()> { Ok(()) }
        async fn update_document_pii_fields(&self, _id: &str, _body_raw_encrypted: Option<&str>, _body_raw_nonce: Option<&str>, _pii_scanned_at: Option<chrono::DateTime<chrono::Utc>>) -> DbResult<()> { Ok(()) }
        async fn update_message_body(&self, _id: &str, _body: &str, _body_html: Option<&str>) -> DbResult<()> { Ok(()) }
        async fn update_message_pii_fields(&self, _id: &str, _body_raw_encrypted: Option<&str>, _body_raw_nonce: Option<&str>, _pii_scanned_at: Option<chrono::DateTime<chrono::Utc>>) -> DbResult<()> { Ok(()) }
        async fn update_contact_pii_fields(&self, _id: &str, _pii_scanned_at: Option<chrono::DateTime<chrono::Utc>>) -> DbResult<()> { Ok(()) }
    }

    // ── Phase 2a behavioural tests: Message.body + subject encryption + blind-index search ──

    use crate::mock::MockGraphDB;
    use crate::schema::{ChannelType, Message, MessageDirection};

    /// Builder: an EncryptedGraphDB wrapping a fresh MockGraphDB with unique
    /// scratch paths for the per-entity key DBs. Per-test isolation.
    fn build_encrypted_db(tag: &str) -> (Arc<MockGraphDB>, EncryptedGraphDB) {
        let inner = Arc::new(MockGraphDB::new());
        let kek = Kek::generate();
        let mk_kdb = |suffix: &str| {
            Arc::new(RwLock::new(KeyDatabase::new(
                std::env::temp_dir().join(format!("test-{tag}-{suffix}-keys.db")),
            )))
        };
        let edb = EncryptedGraphDB {
            inner: inner.clone(),
            key_db: mk_kdb("doc"),
            messages_key_db: mk_kdb("msg"),
            threads_key_db: mk_kdb("thr"),
            conversations_key_db: mk_kdb("conv"),
            contacts_key_db: mk_kdb("con"),
            share_records_key_db: mk_kdb("shr"),
            kek: Arc::new(Kek::from_bytes(*kek.as_bytes())),
            index_key: Arc::new(IndexKey::generate()),
            device_key: Arc::new(test_device_key()),
        };
        (inner, edb)
    }

    fn sample_message(conv: &str, body: &str, subject: Option<&str>) -> Message {
        let mut m = Message::new(
            conv.to_string(),
            ChannelType::Email,
            MessageDirection::Inbound,
            "contact:from".to_string(),
            vec!["contact:to".to_string()],
            body.to_string(),
        );
        m.subject = subject.map(|s| s.to_string());
        m
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn message_body_and_subject_roundtrip() {
        let (inner, edb) = build_encrypted_db("roundtrip");
        let msg = sample_message(
            "conv:1",
            "The quarterly numbers landed and they look strong.",
            Some("Q4 results"),
        );
        let created = edb.create_message(msg.clone()).await.unwrap();
        let id = created.id_string().unwrap();

        // Caller sees plaintext
        let got = edb.get_message(&id).await.unwrap();
        assert_eq!(got.body, msg.body);
        assert_eq!(got.subject.as_deref(), Some("Q4 results"));
        assert!(got.body_nonce.is_none(), "decrypted view exposes no body_nonce");
        assert!(got.subject_nonce.is_none());
        assert!(got.body_token_hashes.is_empty(), "decrypted view clears index hashes");

        // Inner row is ciphertext
        let raw = inner.get_message(&id).await.unwrap();
        assert_ne!(raw.body, msg.body, "body at rest must be ciphertext");
        assert_ne!(raw.subject.as_deref(), Some("Q4 results"), "subject at rest must be ciphertext");
        assert!(raw.body_nonce.is_some(), "body_nonce must be set after encryption");
        assert!(raw.subject_nonce.is_some(), "subject_nonce must be set after encryption");
        assert!(!raw.body_token_hashes.is_empty(), "token hashes must populate for search");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn search_returns_messages_with_matching_tokens() {
        let (_, edb) = build_encrypted_db("search-hit");
        edb.create_message(sample_message("conv:1", "let us discuss the budget tomorrow", None)).await.unwrap();
        edb.create_message(sample_message("conv:1", "weather is nice today", None)).await.unwrap();
        edb.create_message(sample_message("conv:1", "budget approval needed", Some("urgent"))).await.unwrap();

        let hits = edb.search_messages("budget").await.unwrap();
        assert_eq!(hits.len(), 2);
        for hit in &hits {
            assert!(hit.body.contains("budget"), "got plaintext body containing match");
            assert!(hit.body_nonce.is_none(), "search results are decrypted");
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn search_subject_only_match() {
        let (_, edb) = build_encrypted_db("search-subj");
        edb.create_message(sample_message(
            "conv:1",
            "see attached",
            Some("invoice from supplier"),
        )).await.unwrap();

        let hits = edb.search_messages("invoice").await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].subject.as_deref(), Some("invoice from supplier"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn search_miss_returns_empty() {
        let (_, edb) = build_encrypted_db("search-miss");
        edb.create_message(sample_message("conv:1", "lunch tomorrow", None)).await.unwrap();
        let hits = edb.search_messages("zebrafish").await.unwrap();
        assert!(hits.is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn search_empty_query_returns_empty() {
        let (_, edb) = build_encrypted_db("search-empty");
        edb.create_message(sample_message("conv:1", "anything at all", None)).await.unwrap();
        let hits = edb.search_messages("").await.unwrap();
        assert!(hits.is_empty(), "empty query has no tokens, must not return the world");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn search_requires_all_tokens() {
        // CONTAINSALL semantics: every query token must appear.
        let (_, edb) = build_encrypted_db("search-all");
        edb.create_message(sample_message("conv:1", "alpha beta gamma", None)).await.unwrap();
        edb.create_message(sample_message("conv:1", "alpha delta", None)).await.unwrap();

        let both = edb.search_messages("alpha beta").await.unwrap();
        assert_eq!(both.len(), 1, "only the alpha+beta row should match");
        assert!(both[0].body.contains("beta"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn search_skips_soft_deleted_messages() {
        let (inner, edb) = build_encrypted_db("search-deleted");
        let created = edb.create_message(sample_message("conv:1", "secret plan", None)).await.unwrap();
        let id = created.id_string().unwrap();

        // soft-delete via the inner DB (sets deleted_at on the row)
        inner.delete_message(&id).await.unwrap();

        let hits = edb.search_messages("secret").await.unwrap();
        assert!(hits.is_empty(), "deleted_at IS NOT NONE rows must be excluded");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn different_index_keys_produce_different_hashes_on_disk() {
        let (inner_a, edb_a) = build_encrypted_db("idx-a");
        let (inner_b, edb_b) = build_encrypted_db("idx-b");
        edb_a.create_message(sample_message("conv:1", "shared text here", None)).await.unwrap();
        edb_b.create_message(sample_message("conv:1", "shared text here", None)).await.unwrap();

        let a_rows = inner_a.list_all_messages().await.unwrap();
        let b_rows = inner_b.list_all_messages().await.unwrap();
        assert_eq!(a_rows.len(), 1);
        assert_eq!(b_rows.len(), 1);
        assert_ne!(
            a_rows[0].body_token_hashes,
            b_rows[0].body_token_hashes,
            "same plaintext under different index keys must hash differently",
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_messages_decrypts_all() {
        let (_, edb) = build_encrypted_db("list");
        for body in ["one", "two three four", "five six seven eight"] {
            edb.create_message(sample_message("conv:1", body, None)).await.unwrap();
        }
        let listed = edb.list_messages("conv:1", None, 100).await.unwrap();
        assert_eq!(listed.len(), 3);
        for m in &listed {
            assert!(m.body_nonce.is_none(), "list_messages returns decrypted views");
            // Plaintext bodies are short ASCII words — never base64.
            assert!(!m.body.contains('='), "no base64 padding leaked through");
        }
    }

    // ── Phase 2b: Thread / Conversation / Contact / ShareRecord / Document.title ──

    use crate::schema::{Conversation, Document, ShareChannel, ShareRecord, Thread};

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn document_title_roundtrip_and_search() {
        let (inner, edb) = build_encrypted_db("doc-title");
        let d1 = edb.create_document(Document::new("Quarterly Budget Report".into(), "thread:1".into(), true)).await.unwrap();
        let _ = edb.create_document(Document::new("Holiday Plans".into(), "thread:1".into(), true)).await.unwrap();

        // Read returns plaintext title; ciphertext lives in the inner row.
        let got = edb.get_document(&d1.id_string().unwrap()).await.unwrap();
        assert_eq!(got.title, "Quarterly Budget Report");
        let raw = inner.get_document(&d1.id_string().unwrap()).await.unwrap();
        assert_ne!(raw.title, "Quarterly Budget Report", "title at rest must be ciphertext");
        assert!(raw.title_nonce.is_some());
        assert!(!raw.title_token_hashes.is_empty());

        // Search hits via token blind-index.
        let hits = edb.search_documents_by_title("budget").await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Quarterly Budget Report");

        let miss = edb.search_documents_by_title("submarine").await.unwrap();
        assert!(miss.is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn document_update_rewrites_title_hashes() {
        let (_, edb) = build_encrypted_db("doc-update");
        let d = edb.create_document(Document::new("alpha beta".into(), "thread:1".into(), true)).await.unwrap();
        let id = d.id_string().unwrap();

        edb.update_document(&id, Some("gamma delta"), None).await.unwrap();

        // Old tokens no longer match; new ones do.
        assert!(edb.search_documents_by_title("alpha").await.unwrap().is_empty());
        let hits = edb.search_documents_by_title("gamma").await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "gamma delta");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn thread_name_and_description_roundtrip_and_lookup() {
        let (inner, edb) = build_encrypted_db("thread");
        let t = edb.create_thread(Thread::new("Operations 2026".into(), "logistics + ops".into())).await.unwrap();
        let id = t.id_string().unwrap();

        let got = edb.get_thread(&id).await.unwrap();
        assert_eq!(got.name, "Operations 2026");
        assert_eq!(got.description, "logistics + ops");

        // Ciphertext at rest, nonces populated, blind-index emitted.
        let raw = inner.get_thread(&id).await.unwrap();
        assert_ne!(raw.name, "Operations 2026");
        assert_ne!(raw.description, "logistics + ops");
        assert!(raw.name_nonce.is_some());
        assert!(raw.description_nonce.is_some());
        assert!(!raw.name_token_hashes.is_empty());

        // find_thread_by_name traverses the blind-index.
        let found = edb.find_thread_by_name("operations").await.unwrap().unwrap();
        assert_eq!(found.name, "Operations 2026");

        let missing = edb.find_thread_by_name("submarine").await.unwrap();
        assert!(missing.is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn thread_update_rewrites_name_hashes() {
        let (_, edb) = build_encrypted_db("thread-update");
        let t = edb.create_thread(Thread::new("first name".into(), "desc".into())).await.unwrap();
        let id = t.id_string().unwrap();

        edb.update_thread(&id, Some("renamed thing"), None).await.unwrap();

        assert!(edb.find_thread_by_name("first").await.unwrap().is_none());
        let found = edb.find_thread_by_name("renamed").await.unwrap().unwrap();
        assert_eq!(found.name, "renamed thing");
        // description should still decrypt correctly even though we didn't change it.
        assert_eq!(found.description, "desc");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn conversation_title_roundtrip() {
        let (inner, edb) = build_encrypted_db("conv");
        let conv = edb.create_conversation(Conversation::new(
            "Tax planning with accountant".into(),
            ChannelType::Email,
            vec!["contact:a".into(), "contact:b".into()],
        )).await.unwrap();
        let id = conv.id_string().unwrap();

        let got = edb.get_conversation(&id).await.unwrap();
        assert_eq!(got.title, "Tax planning with accountant");
        let raw = inner.get_conversation(&id).await.unwrap();
        assert_ne!(raw.title, "Tax planning with accountant");
        assert!(raw.title_nonce.is_some());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn contact_name_and_notes_roundtrip() {
        let (inner, edb) = build_encrypted_db("contact");
        let mut c = Contact::new("Alice Example".into(), false);
        c.notes = "private debrief notes".into();
        let created = edb.create_contact(c).await.unwrap();
        let id = created.id_string().unwrap();

        let got = edb.get_contact(&id).await.unwrap();
        assert_eq!(got.name, "Alice Example");
        assert_eq!(got.notes, "private debrief notes");

        // Both fields ciphertext at rest with paired nonces.
        let raw = inner.get_contact(&id).await.unwrap();
        assert_ne!(raw.name, "Alice Example", "name at rest must be ciphertext");
        assert_ne!(raw.notes, "private debrief notes", "notes at rest must be ciphertext");
        assert!(raw.name_nonce.is_some());
        assert!(raw.encryption_nonce.is_some(), "notes nonce companion must be set (pre-2b bug fix)");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn contact_update_re_encrypts_fields() {
        let (_, edb) = build_encrypted_db("contact-update");
        let c = edb.create_contact(Contact::new("Original Name".into(), false)).await.unwrap();
        let id = c.id_string().unwrap();

        let after = edb.update_contact(&id, Some("New Name"), Some("now with notes"), None).await.unwrap();
        assert_eq!(after.name, "New Name");
        assert_eq!(after.notes, "now with notes");

        // Notes field can be empty if cleared (passing Some("") at the trait layer).
        let cleared = edb.update_contact(&id, None, Some(""), None).await.unwrap();
        assert_eq!(cleared.notes, "");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn contact_create_with_empty_notes_no_notes_nonce() {
        // Notes empty at create time: we skip the encryption write, so the
        // encryption_nonce stays None and reads see the empty plaintext.
        let (inner, edb) = build_encrypted_db("contact-empty-notes");
        let c = edb.create_contact(Contact::new("No Notes".into(), false)).await.unwrap();
        let id = c.id_string().unwrap();

        let raw = inner.get_contact(&id).await.unwrap();
        assert_eq!(raw.notes, "", "no ciphertext written when notes was empty");
        assert!(raw.encryption_nonce.is_none());
        // But name IS encrypted unconditionally.
        assert!(raw.name_nonce.is_some());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn share_record_via_url_roundtrip() {
        let (inner, edb) = build_encrypted_db("share");
        let rec = ShareRecord {
            id: None,
            pii_record_id: "pii_record:abc".into(),
            to_entity_id: "entity:acme".into(),
            via_message_id: None,
            via_url: Some("https://acme.com/signup".into()),
            shared_at: chrono::Utc::now(),
            channel: ShareChannel::Web,
            via_url_nonce: None,
        };
        let created = edb.create_share_record(rec).await.unwrap();
        let id = created.id_string().unwrap();

        let got = edb.get_share_record(&id).await.unwrap();
        assert_eq!(got.via_url.as_deref(), Some("https://acme.com/signup"));

        let raw = inner.get_share_record(&id).await.unwrap();
        assert!(raw.via_url.is_some());
        assert_ne!(raw.via_url.as_deref(), Some("https://acme.com/signup"), "url at rest must be ciphertext");
        assert!(raw.via_url_nonce.is_some());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn share_record_without_via_url_passes_through() {
        // Common case for IM/email channels: pii_record_id + via_message_id only;
        // no via_url to encrypt.
        let (inner, edb) = build_encrypted_db("share-nourl");
        let rec = ShareRecord {
            id: None,
            pii_record_id: "pii_record:abc".into(),
            to_entity_id: "entity:acme".into(),
            via_message_id: Some("message:1".into()),
            via_url: None,
            shared_at: chrono::Utc::now(),
            channel: ShareChannel::Email,
            via_url_nonce: None,
        };
        let created = edb.create_share_record(rec).await.unwrap();
        let id = created.id_string().unwrap();

        let raw = inner.get_share_record(&id).await.unwrap();
        assert!(raw.via_url.is_none());
        assert!(raw.via_url_nonce.is_none(), "no nonce written when via_url was None");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn search_documents_skips_soft_deleted() {
        let (inner, edb) = build_encrypted_db("doc-deleted");
        let d = edb.create_document(Document::new("secret document".into(), "thread:1".into(), true)).await.unwrap();
        let id = d.id_string().unwrap();
        inner.soft_delete_document(&id).await.unwrap();
        let hits = edb.search_documents_by_title("secret").await.unwrap();
        assert!(hits.is_empty(), "soft-deleted rows must be excluded from blind-index search");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn find_thread_skips_soft_deleted() {
        let (inner, edb) = build_encrypted_db("thread-deleted");
        let t = edb.create_thread(Thread::new("Confidential project".into(), "d".into())).await.unwrap();
        let id = t.id_string().unwrap();
        inner.soft_delete_thread(&id).await.unwrap();
        let found = edb.find_thread_by_name("confidential").await.unwrap();
        assert!(found.is_none());
    }

    // ── Key-DB persistence guard ───────────────────────────────────────
    //
    // Regression for the Phase 2c on-device finding: encrypt_with mints a
    // per-entity key into the in-memory KeyDatabase, but used to never call
    // KeyDatabase::save(). Without persistence, the key vanished on app
    // restart and any encrypted row became permanently unreadable. These
    // tests pin the post-fix behaviour: minting a key persists the
    // KeyDatabase file to disk, and a fresh layer reading that file can
    // decrypt the row.

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn mint_persists_key_db_to_disk() {
        // Use a unique dir so we own the keys.threads.db file.
        let tag = "persist-keydb";
        let dir = std::env::temp_dir().join(format!("test-{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let threads_kdb_path = dir.join("keys.threads.db");

        let inner = Arc::new(crate::mock::MockGraphDB::new());
        let kek = Kek::generate();
        let dk = test_device_key();
        let dk_arc = Arc::new(test_device_key()); // same secret (deterministic from "test"/"salt"/"test-device")
        assert_eq!(dk.as_bytes(), dk_arc.as_bytes());

        let edb = EncryptedGraphDB {
            inner: inner.clone(),
            key_db: Arc::new(RwLock::new(KeyDatabase::new(dir.join("keys.db")))),
            messages_key_db: Arc::new(RwLock::new(KeyDatabase::new(dir.join("keys.messages.db")))),
            threads_key_db: Arc::new(RwLock::new(KeyDatabase::new(threads_kdb_path.clone()))),
            conversations_key_db: Arc::new(RwLock::new(KeyDatabase::new(dir.join("keys.conv.db")))),
            contacts_key_db: Arc::new(RwLock::new(KeyDatabase::new(dir.join("keys.contacts.db")))),
            share_records_key_db: Arc::new(RwLock::new(KeyDatabase::new(dir.join("keys.shares.db")))),
            kek: Arc::new(Kek::from_bytes(*kek.as_bytes())),
            index_key: Arc::new(IndexKey::generate()),
            device_key: dk_arc,
        };

        // Sanity: file does not exist before any write.
        assert!(!threads_kdb_path.exists());

        // Create a thread — this mints a per-thread key and should persist.
        edb.create_thread(Thread::new("any thread".into(), "x".into())).await.unwrap();

        // The threads key DB file now lives on disk under the device key.
        assert!(threads_kdb_path.exists(), "keys.threads.db must be persisted after a key is minted");
        let bytes_on_disk = std::fs::metadata(&threads_kdb_path).unwrap().len();
        assert!(bytes_on_disk > 0, "persisted key DB must be non-empty");

        // And the wire format is recoverable by another reader with the same DeviceKey.
        let recovered = KeyDatabase::load(&threads_kdb_path, &dk).unwrap();
        assert!(!recovered.is_empty(), "loaded key DB must contain the minted thread key");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn restart_simulation_recovers_encrypted_thread() {
        // Full "create on session A → reload on session B with the same keys"
        // simulation. The MockGraphDB is shared across both sessions to
        // simulate the underlying SurrealDB persisting rows; the KeyDatabase
        // file IS the only encryption-state bridge.
        let tag = "persist-restart";
        let dir = std::env::temp_dir().join(format!("test-{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let inner = Arc::new(crate::mock::MockGraphDB::new());
        let kek = Kek::generate();
        let kek_bytes = *kek.as_bytes();
        let dk_bytes = *test_device_key().as_bytes();
        let _ = dk_bytes; // (DeviceKey::derive is deterministic from passphrase + device_id, so we just rebuild)

        let mk_layer = || EncryptedGraphDB {
            inner: inner.clone(),
            key_db: Arc::new(RwLock::new(
                if dir.join("keys.db").exists() {
                    KeyDatabase::load(&dir.join("keys.db"), &test_device_key()).unwrap()
                } else { KeyDatabase::new(dir.join("keys.db")) }
            )),
            messages_key_db: Arc::new(RwLock::new(KeyDatabase::new(dir.join("keys.messages.db")))),
            threads_key_db: Arc::new(RwLock::new(
                if dir.join("keys.threads.db").exists() {
                    KeyDatabase::load(&dir.join("keys.threads.db"), &test_device_key()).unwrap()
                } else { KeyDatabase::new(dir.join("keys.threads.db")) }
            )),
            conversations_key_db: Arc::new(RwLock::new(KeyDatabase::new(dir.join("keys.conv.db")))),
            contacts_key_db: Arc::new(RwLock::new(KeyDatabase::new(dir.join("keys.contacts.db")))),
            share_records_key_db: Arc::new(RwLock::new(KeyDatabase::new(dir.join("keys.shares.db")))),
            kek: Arc::new(Kek::from_bytes(kek_bytes)),
            index_key: Arc::new(IndexKey::generate()), // index key is irrelevant for raw decrypt
            device_key: Arc::new(test_device_key()),
        };

        // Session A: create the thread, verify it round-trips.
        let id = {
            let edb_a = mk_layer();
            let created = edb_a.create_thread(Thread::new("Secret Plan".into(), "private".into())).await.unwrap();
            let id = created.id_string().unwrap();
            let got = edb_a.get_thread(&id).await.unwrap();
            assert_eq!(got.name, "Secret Plan");
            assert_eq!(got.description, "private");
            id
            // edb_a goes out of scope: simulates the app being killed.
        };

        // Session B: brand new layer, reloading the keys from disk. The
        // underlying MockGraphDB row still holds ciphertext + nonces.
        let edb_b = mk_layer();
        let recovered = edb_b.get_thread(&id).await.unwrap();
        assert_eq!(recovered.name, "Secret Plan", "session B must decrypt name with the persisted key");
        assert_eq!(recovered.description, "private");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
