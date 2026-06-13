//! `LayeredGraphDB`: an atomic-swap indirection over an `Arc<dyn GraphDB>`.
//!
//! Used to install `EncryptedGraphDB` at runtime after the user logs in. The
//! app boots with a raw [`crate::surreal::SurrealGraphDB`] wrapped here; once
//! login succeeds and the KEK is unlocked, the inner is replaced with an
//! [`crate::encrypted::EncryptedGraphDB`]. All consumers hold the same
//! `Arc<LayeredGraphDB>` so the swap is transparent — no Arc juggling at call
//! sites, no `Arc<dyn>` plumbing.
//!
//! The implementation uses [`arc_swap::ArcSwap`] for lock-free reads. Each
//! trait call loads the current inner (one atomic acquire), then forwards.
//! Swaps are rare (login is the only writer), so contention is irrelevant.
//!
//! Mixed-state rows are tolerated by design: rows written before the swap
//! land plaintext, rows written after the swap land encrypted. The decrypt
//! paths in `EncryptedGraphDB` already guard on `*_nonce` columns being
//! `Some` before attempting decryption.

use std::sync::Arc;

use arc_swap::ArcSwap;
use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::error::DbResult;
use crate::schema::{
    ChannelType, Commit, Contact, Conversation, Document, Entity, EntityKind, Message, Milestone,
    PiiRecord, ReadStatus, RelatedTo, RelationType, ReviewState, ShareRecord, SourceRef,
    SuggestedLink, SuggestionSource, SuggestionStatus, Thread,
};
use crate::traits::GraphDB;

/// Indirection layer over a swappable `Arc<dyn GraphDB>` inner.
///
/// See module docs for the boot → swap-on-login lifecycle.
pub struct LayeredGraphDB {
    /// The bootstrap inner — the raw storage backend wired up at app boot
    /// (typically `SurrealGraphDB`). Fixed for the lifetime of the layer
    /// so the login hook can always wrap *the raw inner* in an encryption
    /// decorator, even if a previous session already swapped one in.
    /// Without this, login would re-wrap the already-encrypted current,
    /// which would loop through the layer back to itself.
    raw: Arc<dyn GraphDB>,
    /// The currently-active inner. Starts pointing at `raw`; swapped to
    /// `EncryptedGraphDB` after login.
    current: ArcSwap<Box<dyn GraphDB>>,
}

impl LayeredGraphDB {
    pub fn new(initial: Arc<dyn GraphDB>) -> Self {
        // ArcSwap stores Arc<T>, so we box the trait object once and hand it
        // a Box-shaped pointer. Replacement is one atomic store.
        let boxed: Box<dyn GraphDB> = Box::new(ArcWrapper(initial.clone()));
        Self {
            raw: initial,
            current: ArcSwap::new(Arc::new(boxed)),
        }
    }

    /// Snapshot the original raw inner (bootstrap backend). Login uses this
    /// to wrap the raw storage in `EncryptedGraphDB`, then `swap()`s the
    /// wrapper in as the new current. Cheap (one Arc clone).
    pub fn raw_inner(&self) -> Arc<dyn GraphDB> {
        self.raw.clone()
    }

    /// Replace the active inner atomically. Idempotent.
    pub fn swap(&self, new_inner: Arc<dyn GraphDB>) {
        let boxed: Box<dyn GraphDB> = Box::new(ArcWrapper(new_inner));
        self.current.store(Arc::new(boxed));
    }

    fn current(&self) -> arc_swap::Guard<Arc<Box<dyn GraphDB>>> {
        self.current.load()
    }
}

/// Forward `&dyn GraphDB` calls through an `Arc<dyn GraphDB>`. Needed because
/// `ArcSwap` requires a sized payload; the `Arc` inside is the actual storage.
struct ArcWrapper(Arc<dyn GraphDB>);

#[async_trait]
impl GraphDB for ArcWrapper {
    async fn connect(&self) -> DbResult<()> { self.0.connect().await }
    async fn init_schema(&self) -> DbResult<()> { self.0.init_schema().await }

    async fn create_document(&self, doc: Document) -> DbResult<Document> { self.0.create_document(doc).await }
    async fn create_document_with_id(&self, doc: Document) -> DbResult<bool> { self.0.create_document_with_id(doc).await }
    async fn get_document(&self, id: &str) -> DbResult<Document> { self.0.get_document(id).await }
    async fn list_documents(&self, thread_id: Option<&str>) -> DbResult<Vec<Document>> { self.0.list_documents(thread_id).await }
    async fn update_document(&self, id: &str, title: Option<&str>, content: Option<&str>) -> DbResult<Document> { self.0.update_document(id, title, content).await }
    async fn delete_document(&self, id: &str) -> DbResult<()> { self.0.delete_document(id).await }
    async fn update_document_position(&self, id: &str, x: f32, y: f32) -> DbResult<()> { self.0.update_document_position(id, x, y).await }
    async fn search_documents_by_title(&self, query: &str) -> DbResult<Vec<Document>> { self.0.search_documents_by_title(query).await }
    async fn search_documents_by_title_token_hashes(&self, hashes: &[String]) -> DbResult<Vec<Document>> { self.0.search_documents_by_title_token_hashes(hashes).await }
    async fn set_document_title_encryption(&self, id: &str, title_ciphertext: &str, title_nonce: &str, title_token_hashes: &[String]) -> DbResult<()> {
        self.0.set_document_title_encryption(id, title_ciphertext, title_nonce, title_token_hashes).await
    }
    async fn set_document_content_encryption(&self, id: &str, content_ciphertext: &str, content_nonce: &str) -> DbResult<()> {
        self.0.set_document_content_encryption(id, content_ciphertext, content_nonce).await
    }
    async fn update_document_reliability(&self, id: &str, source_url: Option<&str>, classification: Option<&str>, score: Option<f32>, assessment_json: Option<&str>) -> DbResult<Document> {
        self.0.update_document_reliability(id, source_url, classification, score, assessment_json).await
    }

    async fn create_thread(&self, thread: Thread) -> DbResult<Thread> { self.0.create_thread(thread).await }
    async fn get_thread(&self, id: &str) -> DbResult<Thread> { self.0.get_thread(id).await }
    async fn list_threads(&self) -> DbResult<Vec<Thread>> { self.0.list_threads().await }
    async fn update_thread(&self, id: &str, name: Option<&str>, description: Option<&str>) -> DbResult<Thread> { self.0.update_thread(id, name, description).await }
    async fn delete_thread(&self, id: &str) -> DbResult<()> { self.0.delete_thread(id).await }
    async fn find_thread_by_name(&self, name: &str) -> DbResult<Option<Thread>> { self.0.find_thread_by_name(name).await }
    async fn find_thread_by_name_token_hashes(&self, hashes: &[String]) -> DbResult<Option<Thread>> { self.0.find_thread_by_name_token_hashes(hashes).await }
    async fn set_thread_encryption(&self, id: &str, name_ciphertext: &str, name_nonce: &str, description_ciphertext: &str, description_nonce: &str, name_token_hashes: &[String]) -> DbResult<()> {
        self.0.set_thread_encryption(id, name_ciphertext, name_nonce, description_ciphertext, description_nonce, name_token_hashes).await
    }
    async fn move_document_to_thread(&self, doc_id: &str, new_thread_id: &str) -> DbResult<Document> { self.0.move_document_to_thread(doc_id, new_thread_id).await }

    async fn create_relationship(&self, from_id: &str, to_id: &str, relation_type: RelationType, strength: f32) -> DbResult<RelatedTo> { self.0.create_relationship(from_id, to_id, relation_type, strength).await }
    async fn list_outgoing_relationships(&self, doc_id: &str) -> DbResult<Vec<RelatedTo>> { self.0.list_outgoing_relationships(doc_id).await }
    async fn list_incoming_relationships(&self, doc_id: &str) -> DbResult<Vec<RelatedTo>> { self.0.list_incoming_relationships(doc_id).await }
    async fn list_all_relationships(&self) -> DbResult<Vec<RelatedTo>> { self.0.list_all_relationships().await }
    async fn traverse(&self, doc_id: &str, depth: u32, limit: u32) -> DbResult<Vec<Document>> { self.0.traverse(doc_id, depth, limit).await }

    async fn create_suggested_link(&self, from_id: &str, to_id: &str, relation_type: RelationType, strength: f32, rationale: &str, source: SuggestionSource) -> DbResult<SuggestedLink> {
        self.0.create_suggested_link(from_id, to_id, relation_type, strength, rationale, source).await
    }
    async fn list_pending_suggestions(&self) -> DbResult<Vec<SuggestedLink>> { self.0.list_pending_suggestions().await }
    async fn list_suggestions_for_document(&self, doc_id: &str) -> DbResult<Vec<SuggestedLink>> { self.0.list_suggestions_for_document(doc_id).await }
    async fn resolve_suggestion(&self, id: &str, status: SuggestionStatus) -> DbResult<SuggestedLink> { self.0.resolve_suggestion(id, status).await }
    async fn suggestion_exists(&self, from_id: &str, to_id: &str) -> DbResult<bool> { self.0.suggestion_exists(from_id, to_id).await }

    async fn adopt_document(&self, id: &str) -> DbResult<Document> { self.0.adopt_document(id).await }

    async fn merge_threads(&self, target_id: &str, source_id: &str) -> DbResult<()> { self.0.merge_threads(target_id, source_id).await }
    async fn split_thread(&self, thread_id: &str, doc_ids: &[String], new_name: &str) -> DbResult<Thread> { self.0.split_thread(thread_id, doc_ids, new_name).await }

    async fn soft_delete_document(&self, id: &str) -> DbResult<()> { self.0.soft_delete_document(id).await }
    async fn restore_soft_deleted_document(&self, id: &str) -> DbResult<Document> { self.0.restore_soft_deleted_document(id).await }
    async fn soft_delete_thread(&self, id: &str) -> DbResult<()> { self.0.soft_delete_thread(id).await }
    async fn restore_soft_deleted_thread(&self, id: &str) -> DbResult<Thread> { self.0.restore_soft_deleted_thread(id).await }
    async fn purge_deleted(&self, max_age: std::time::Duration) -> DbResult<u64> { self.0.purge_deleted(max_age).await }

    async fn commit_document(&self, doc_id: &str, message: &str) -> DbResult<Commit> { self.0.commit_document(doc_id, message).await }
    async fn list_document_commits(&self, doc_id: &str) -> DbResult<Vec<Commit>> { self.0.list_document_commits(doc_id).await }
    async fn get_commit(&self, commit_id: &str) -> DbResult<Commit> { self.0.get_commit(commit_id).await }
    async fn restore_document(&self, doc_id: &str, commit_id: &str) -> DbResult<Document> { self.0.restore_document(doc_id, commit_id).await }
    async fn set_commit_signature(&self, commit_id: &str, signature: &str) -> DbResult<()> { self.0.set_commit_signature(commit_id, signature).await }

    async fn create_milestone(&self, milestone: Milestone) -> DbResult<Milestone> { self.0.create_milestone(milestone).await }
    async fn list_milestones(&self, thread_id: &str) -> DbResult<Vec<Milestone>> { self.0.list_milestones(thread_id).await }
    async fn list_all_milestones(&self) -> DbResult<Vec<Milestone>> { self.0.list_all_milestones().await }
    async fn delete_milestone(&self, id: &str) -> DbResult<()> { self.0.delete_milestone(id).await }

    async fn create_contact(&self, contact: Contact) -> DbResult<Contact> { self.0.create_contact(contact).await }
    async fn get_contact(&self, id: &str) -> DbResult<Contact> { self.0.get_contact(id).await }
    async fn list_contacts(&self) -> DbResult<Vec<Contact>> { self.0.list_contacts().await }
    async fn update_contact(&self, id: &str, name: Option<&str>, notes: Option<&str>, avatar: Option<&str>) -> DbResult<Contact> { self.0.update_contact(id, name, notes, avatar).await }
    async fn delete_contact(&self, id: &str) -> DbResult<()> { self.0.delete_contact(id).await }
    async fn set_contact_name_encryption(&self, id: &str, name_ciphertext: &str, name_nonce: &str) -> DbResult<()> { self.0.set_contact_name_encryption(id, name_ciphertext, name_nonce).await }
    async fn set_contact_notes_encryption(&self, id: &str, notes_ciphertext: &str, notes_nonce: &str) -> DbResult<()> { self.0.set_contact_notes_encryption(id, notes_ciphertext, notes_nonce).await }
    async fn set_contact_addresses_encryption(&self, id: &str, addresses_ciphertext: &str, addresses_nonce: &str) -> DbResult<()> { self.0.set_contact_addresses_encryption(id, addresses_ciphertext, addresses_nonce).await }
    async fn soft_delete_contact(&self, id: &str) -> DbResult<()> { self.0.soft_delete_contact(id).await }
    async fn find_contact_by_address(&self, address: &str) -> DbResult<Option<Contact>> { self.0.find_contact_by_address(address).await }
    async fn add_contact_address(&self, contact_id: &str, address: crate::schema::ChannelAddress) -> DbResult<Contact> { self.0.add_contact_address(contact_id, address).await }

    async fn create_message(&self, message: Message) -> DbResult<Message> { self.0.create_message(message).await }
    async fn get_message(&self, id: &str) -> DbResult<Message> { self.0.get_message(id).await }
    async fn list_messages(&self, conversation_id: &str, before: Option<DateTime<Utc>>, limit: u32) -> DbResult<Vec<Message>> { self.0.list_messages(conversation_id, before, limit).await }
    async fn update_message_read_status(&self, id: &str, status: ReadStatus) -> DbResult<Message> { self.0.update_message_read_status(id, status).await }
    async fn delete_message(&self, id: &str) -> DbResult<()> { self.0.delete_message(id).await }
    async fn list_all_messages(&self) -> DbResult<Vec<Message>> { self.0.list_all_messages().await }
    async fn list_messages_in_time_range(&self, after: DateTime<Utc>, before: DateTime<Utc>, limit: u32) -> DbResult<Vec<Message>> { self.0.list_messages_in_time_range(after, before, limit).await }
    async fn search_messages(&self, query: &str) -> DbResult<Vec<Message>> { self.0.search_messages(query).await }
    async fn search_messages_by_token_hashes(&self, hashes: &[String]) -> DbResult<Vec<Message>> { self.0.search_messages_by_token_hashes(hashes).await }
    async fn find_message_by_external_id(&self, external_id: &str) -> DbResult<Option<Message>> { self.0.find_message_by_external_id(external_id).await }
    async fn set_message_encryption(&self, id: &str, body_ciphertext: &str, body_nonce: &str, subject_ciphertext: Option<&str>, subject_nonce: Option<&str>, body_html_ciphertext: Option<&str>, body_html_nonce: Option<&str>, body_token_hashes: &[String]) -> DbResult<()> {
        self.0.set_message_encryption(id, body_ciphertext, body_nonce, subject_ciphertext, subject_nonce, body_html_ciphertext, body_html_nonce, body_token_hashes).await
    }

    async fn create_conversation(&self, conversation: Conversation) -> DbResult<Conversation> { self.0.create_conversation(conversation).await }
    async fn set_conversation_title_encryption(&self, id: &str, title_ciphertext: &str, title_nonce: &str) -> DbResult<()> { self.0.set_conversation_title_encryption(id, title_ciphertext, title_nonce).await }
    async fn get_conversation(&self, id: &str) -> DbResult<Conversation> { self.0.get_conversation(id).await }
    async fn list_conversations(&self, channel: Option<&ChannelType>) -> DbResult<Vec<Conversation>> { self.0.list_conversations(channel).await }
    async fn update_conversation_unread(&self, id: &str, unread_count: u32) -> DbResult<Conversation> { self.0.update_conversation_unread(id, unread_count).await }
    async fn update_conversation_last_message_at(&self, id: &str, at: DateTime<Utc>) -> DbResult<Conversation> { self.0.update_conversation_last_message_at(id, at).await }
    async fn delete_conversation(&self, id: &str) -> DbResult<()> { self.0.delete_conversation(id).await }
    async fn link_conversation_to_thread(&self, conversation_id: &str, thread_id: &str) -> DbResult<Conversation> { self.0.link_conversation_to_thread(conversation_id, thread_id).await }

    async fn create_entity(&self, entity: Entity) -> DbResult<Entity> { self.0.create_entity(entity).await }
    async fn list_entities(&self) -> DbResult<Vec<Entity>> { self.0.list_entities().await }
    async fn create_pii_record(&self, record: PiiRecord) -> DbResult<PiiRecord> { self.0.create_pii_record(record).await }
    async fn get_pii_record(&self, id: &str) -> DbResult<PiiRecord> { self.0.get_pii_record(id).await }
    async fn list_pii_records(&self, entity_id: Option<&str>, review_state: Option<ReviewState>, stored_secret: Option<bool>) -> DbResult<Vec<PiiRecord>> { self.0.list_pii_records(entity_id, review_state, stored_secret).await }
    async fn update_pii_record_review_state(&self, id: &str, review_state: ReviewState) -> DbResult<()> { self.0.update_pii_record_review_state(id, review_state).await }
    async fn update_pii_record_value(&self, id: &str, value_encrypted: &str, value_nonce: &str) -> DbResult<()> { self.0.update_pii_record_value(id, value_encrypted, value_nonce).await }
    async fn soft_delete_pii_record(&self, id: &str) -> DbResult<()> { self.0.soft_delete_pii_record(id).await }
    async fn get_entity(&self, id: &str) -> DbResult<Entity> { self.0.get_entity(id).await }
    async fn update_entity(&self, id: &str, name: Option<&str>, kind: Option<EntityKind>, domains: Option<Vec<String>>, contact_ids: Option<Vec<String>>, notes: Option<&str>, is_owned: Option<bool>, deleted_at: Option<Option<String>>) -> DbResult<Entity> { self.0.update_entity(id, name, kind, domains, contact_ids, notes, is_owned, deleted_at).await }

    async fn create_share_record(&self, record: ShareRecord) -> DbResult<ShareRecord> { self.0.create_share_record(record).await }
    async fn set_share_record_via_url_encryption(&self, id: &str, via_url_ciphertext: &str, via_url_nonce: &str) -> DbResult<()> { self.0.set_share_record_via_url_encryption(id, via_url_ciphertext, via_url_nonce).await }
    async fn list_share_records_for_entity(&self, entity_id: &str) -> DbResult<Vec<ShareRecord>> { self.0.list_share_records_for_entity(entity_id).await }
    async fn list_all_share_records(&self) -> DbResult<Vec<ShareRecord>> { self.0.list_all_share_records().await }
    async fn get_share_record(&self, id: &str) -> DbResult<ShareRecord> { self.0.get_share_record(id).await }

    async fn update_pii_record_sources(&self, id: &str, sources: Vec<SourceRef>) -> DbResult<()> { self.0.update_pii_record_sources(id, sources).await }
    async fn update_pii_record_revealed_at(&self, id: &str, last_revealed_at: DateTime<Utc>) -> DbResult<()> { self.0.update_pii_record_revealed_at(id, last_revealed_at).await }
    async fn update_document_pii_fields(&self, id: &str, body_raw_encrypted: Option<&str>, body_raw_nonce: Option<&str>, pii_scanned_at: Option<DateTime<Utc>>) -> DbResult<()> { self.0.update_document_pii_fields(id, body_raw_encrypted, body_raw_nonce, pii_scanned_at).await }
    async fn update_message_body(&self, id: &str, body: &str, body_html: Option<&str>) -> DbResult<()> { self.0.update_message_body(id, body, body_html).await }
    async fn update_message_pii_fields(&self, id: &str, body_raw_encrypted: Option<&str>, body_raw_nonce: Option<&str>, pii_scanned_at: Option<DateTime<Utc>>) -> DbResult<()> { self.0.update_message_pii_fields(id, body_raw_encrypted, body_raw_nonce, pii_scanned_at).await }
    async fn update_contact_pii_fields(&self, id: &str, pii_scanned_at: Option<DateTime<Utc>>) -> DbResult<()> { self.0.update_contact_pii_fields(id, pii_scanned_at).await }

    async fn create_thread_with_id(&self, thread: Thread) -> DbResult<bool> { self.0.create_thread_with_id(thread).await }
    async fn create_entity_with_id(&self, entity: Entity) -> DbResult<bool> { self.0.create_entity_with_id(entity).await }
    async fn create_pii_record_with_id(&self, record: PiiRecord) -> DbResult<bool> { self.0.create_pii_record_with_id(record).await }
    async fn create_share_record_with_id(&self, record: ShareRecord) -> DbResult<bool> { self.0.create_share_record_with_id(record).await }
    async fn create_contact_with_id(&self, contact: Contact) -> DbResult<bool> { self.0.create_contact_with_id(contact).await }
    async fn create_message_with_id(&self, message: Message) -> DbResult<bool> { self.0.create_message_with_id(message).await }
    async fn create_conversation_with_id(&self, conversation: Conversation) -> DbResult<bool> { self.0.create_conversation_with_id(conversation).await }
    async fn create_milestone_with_id(&self, milestone: Milestone) -> DbResult<bool> { self.0.create_milestone_with_id(milestone).await }
    async fn create_relationship_with_id(&self, rel: RelatedTo) -> DbResult<bool> { self.0.create_relationship_with_id(rel).await }
    async fn create_suggested_link_with_id(&self, link: SuggestedLink) -> DbResult<bool> { self.0.create_suggested_link_with_id(link).await }
    async fn get_milestone(&self, id: &str) -> DbResult<Milestone> { self.0.get_milestone(id).await }
    async fn get_relationship(&self, id: &str) -> DbResult<RelatedTo> { self.0.get_relationship(id).await }
    async fn get_suggested_link(&self, id: &str) -> DbResult<SuggestedLink> { self.0.get_suggested_link(id).await }
    async fn list_all_suggested_links(&self) -> DbResult<Vec<SuggestedLink>> { self.0.list_all_suggested_links().await }
    async fn set_suggested_link_status(&self, id: &str, status: SuggestionStatus, resolved_at: Option<DateTime<Utc>>) -> DbResult<()> { self.0.set_suggested_link_status(id, status, resolved_at).await }
}

#[async_trait]
impl GraphDB for LayeredGraphDB {
    async fn connect(&self) -> DbResult<()> { self.current().connect().await }
    async fn init_schema(&self) -> DbResult<()> { self.current().init_schema().await }

    async fn create_document(&self, doc: Document) -> DbResult<Document> { self.current().create_document(doc).await }
    async fn create_document_with_id(&self, doc: Document) -> DbResult<bool> { self.current().create_document_with_id(doc).await }
    async fn get_document(&self, id: &str) -> DbResult<Document> { self.current().get_document(id).await }
    async fn list_documents(&self, thread_id: Option<&str>) -> DbResult<Vec<Document>> { self.current().list_documents(thread_id).await }
    async fn update_document(&self, id: &str, title: Option<&str>, content: Option<&str>) -> DbResult<Document> { self.current().update_document(id, title, content).await }
    async fn delete_document(&self, id: &str) -> DbResult<()> { self.current().delete_document(id).await }
    async fn update_document_position(&self, id: &str, x: f32, y: f32) -> DbResult<()> { self.current().update_document_position(id, x, y).await }
    async fn search_documents_by_title(&self, query: &str) -> DbResult<Vec<Document>> { self.current().search_documents_by_title(query).await }
    async fn search_documents_by_title_token_hashes(&self, hashes: &[String]) -> DbResult<Vec<Document>> { self.current().search_documents_by_title_token_hashes(hashes).await }
    async fn set_document_title_encryption(&self, id: &str, title_ciphertext: &str, title_nonce: &str, title_token_hashes: &[String]) -> DbResult<()> {
        self.current().set_document_title_encryption(id, title_ciphertext, title_nonce, title_token_hashes).await
    }
    async fn set_document_content_encryption(&self, id: &str, content_ciphertext: &str, content_nonce: &str) -> DbResult<()> {
        self.current().set_document_content_encryption(id, content_ciphertext, content_nonce).await
    }
    async fn update_document_reliability(&self, id: &str, source_url: Option<&str>, classification: Option<&str>, score: Option<f32>, assessment_json: Option<&str>) -> DbResult<Document> {
        self.current().update_document_reliability(id, source_url, classification, score, assessment_json).await
    }

    async fn create_thread(&self, thread: Thread) -> DbResult<Thread> { self.current().create_thread(thread).await }
    async fn get_thread(&self, id: &str) -> DbResult<Thread> { self.current().get_thread(id).await }
    async fn list_threads(&self) -> DbResult<Vec<Thread>> { self.current().list_threads().await }
    async fn update_thread(&self, id: &str, name: Option<&str>, description: Option<&str>) -> DbResult<Thread> { self.current().update_thread(id, name, description).await }
    async fn delete_thread(&self, id: &str) -> DbResult<()> { self.current().delete_thread(id).await }
    async fn find_thread_by_name(&self, name: &str) -> DbResult<Option<Thread>> { self.current().find_thread_by_name(name).await }
    async fn find_thread_by_name_token_hashes(&self, hashes: &[String]) -> DbResult<Option<Thread>> { self.current().find_thread_by_name_token_hashes(hashes).await }
    async fn set_thread_encryption(&self, id: &str, name_ciphertext: &str, name_nonce: &str, description_ciphertext: &str, description_nonce: &str, name_token_hashes: &[String]) -> DbResult<()> {
        self.current().set_thread_encryption(id, name_ciphertext, name_nonce, description_ciphertext, description_nonce, name_token_hashes).await
    }
    async fn move_document_to_thread(&self, doc_id: &str, new_thread_id: &str) -> DbResult<Document> { self.current().move_document_to_thread(doc_id, new_thread_id).await }

    async fn create_relationship(&self, from_id: &str, to_id: &str, relation_type: RelationType, strength: f32) -> DbResult<RelatedTo> { self.current().create_relationship(from_id, to_id, relation_type, strength).await }
    async fn list_outgoing_relationships(&self, doc_id: &str) -> DbResult<Vec<RelatedTo>> { self.current().list_outgoing_relationships(doc_id).await }
    async fn list_incoming_relationships(&self, doc_id: &str) -> DbResult<Vec<RelatedTo>> { self.current().list_incoming_relationships(doc_id).await }
    async fn list_all_relationships(&self) -> DbResult<Vec<RelatedTo>> { self.current().list_all_relationships().await }
    async fn traverse(&self, doc_id: &str, depth: u32, limit: u32) -> DbResult<Vec<Document>> { self.current().traverse(doc_id, depth, limit).await }

    async fn create_suggested_link(&self, from_id: &str, to_id: &str, relation_type: RelationType, strength: f32, rationale: &str, source: SuggestionSource) -> DbResult<SuggestedLink> {
        self.current().create_suggested_link(from_id, to_id, relation_type, strength, rationale, source).await
    }
    async fn list_pending_suggestions(&self) -> DbResult<Vec<SuggestedLink>> { self.current().list_pending_suggestions().await }
    async fn list_suggestions_for_document(&self, doc_id: &str) -> DbResult<Vec<SuggestedLink>> { self.current().list_suggestions_for_document(doc_id).await }
    async fn resolve_suggestion(&self, id: &str, status: SuggestionStatus) -> DbResult<SuggestedLink> { self.current().resolve_suggestion(id, status).await }
    async fn suggestion_exists(&self, from_id: &str, to_id: &str) -> DbResult<bool> { self.current().suggestion_exists(from_id, to_id).await }

    async fn adopt_document(&self, id: &str) -> DbResult<Document> { self.current().adopt_document(id).await }

    async fn merge_threads(&self, target_id: &str, source_id: &str) -> DbResult<()> { self.current().merge_threads(target_id, source_id).await }
    async fn split_thread(&self, thread_id: &str, doc_ids: &[String], new_name: &str) -> DbResult<Thread> { self.current().split_thread(thread_id, doc_ids, new_name).await }

    async fn soft_delete_document(&self, id: &str) -> DbResult<()> { self.current().soft_delete_document(id).await }
    async fn restore_soft_deleted_document(&self, id: &str) -> DbResult<Document> { self.current().restore_soft_deleted_document(id).await }
    async fn soft_delete_thread(&self, id: &str) -> DbResult<()> { self.current().soft_delete_thread(id).await }
    async fn restore_soft_deleted_thread(&self, id: &str) -> DbResult<Thread> { self.current().restore_soft_deleted_thread(id).await }
    async fn purge_deleted(&self, max_age: std::time::Duration) -> DbResult<u64> { self.current().purge_deleted(max_age).await }

    async fn commit_document(&self, doc_id: &str, message: &str) -> DbResult<Commit> { self.current().commit_document(doc_id, message).await }
    async fn list_document_commits(&self, doc_id: &str) -> DbResult<Vec<Commit>> { self.current().list_document_commits(doc_id).await }
    async fn get_commit(&self, commit_id: &str) -> DbResult<Commit> { self.current().get_commit(commit_id).await }
    async fn restore_document(&self, doc_id: &str, commit_id: &str) -> DbResult<Document> { self.current().restore_document(doc_id, commit_id).await }
    async fn set_commit_signature(&self, commit_id: &str, signature: &str) -> DbResult<()> { self.current().set_commit_signature(commit_id, signature).await }

    async fn create_milestone(&self, milestone: Milestone) -> DbResult<Milestone> { self.current().create_milestone(milestone).await }
    async fn list_milestones(&self, thread_id: &str) -> DbResult<Vec<Milestone>> { self.current().list_milestones(thread_id).await }
    async fn list_all_milestones(&self) -> DbResult<Vec<Milestone>> { self.current().list_all_milestones().await }
    async fn delete_milestone(&self, id: &str) -> DbResult<()> { self.current().delete_milestone(id).await }

    async fn create_contact(&self, contact: Contact) -> DbResult<Contact> { self.current().create_contact(contact).await }
    async fn get_contact(&self, id: &str) -> DbResult<Contact> { self.current().get_contact(id).await }
    async fn list_contacts(&self) -> DbResult<Vec<Contact>> { self.current().list_contacts().await }
    async fn update_contact(&self, id: &str, name: Option<&str>, notes: Option<&str>, avatar: Option<&str>) -> DbResult<Contact> { self.current().update_contact(id, name, notes, avatar).await }
    async fn delete_contact(&self, id: &str) -> DbResult<()> { self.current().delete_contact(id).await }
    async fn set_contact_name_encryption(&self, id: &str, name_ciphertext: &str, name_nonce: &str) -> DbResult<()> { self.current().set_contact_name_encryption(id, name_ciphertext, name_nonce).await }
    async fn set_contact_notes_encryption(&self, id: &str, notes_ciphertext: &str, notes_nonce: &str) -> DbResult<()> { self.current().set_contact_notes_encryption(id, notes_ciphertext, notes_nonce).await }
    async fn set_contact_addresses_encryption(&self, id: &str, addresses_ciphertext: &str, addresses_nonce: &str) -> DbResult<()> { self.current().set_contact_addresses_encryption(id, addresses_ciphertext, addresses_nonce).await }
    async fn soft_delete_contact(&self, id: &str) -> DbResult<()> { self.current().soft_delete_contact(id).await }
    async fn find_contact_by_address(&self, address: &str) -> DbResult<Option<Contact>> { self.current().find_contact_by_address(address).await }
    async fn add_contact_address(&self, contact_id: &str, address: crate::schema::ChannelAddress) -> DbResult<Contact> { self.current().add_contact_address(contact_id, address).await }

    async fn create_message(&self, message: Message) -> DbResult<Message> { self.current().create_message(message).await }
    async fn get_message(&self, id: &str) -> DbResult<Message> { self.current().get_message(id).await }
    async fn list_messages(&self, conversation_id: &str, before: Option<DateTime<Utc>>, limit: u32) -> DbResult<Vec<Message>> { self.current().list_messages(conversation_id, before, limit).await }
    async fn update_message_read_status(&self, id: &str, status: ReadStatus) -> DbResult<Message> { self.current().update_message_read_status(id, status).await }
    async fn delete_message(&self, id: &str) -> DbResult<()> { self.current().delete_message(id).await }
    async fn list_all_messages(&self) -> DbResult<Vec<Message>> { self.current().list_all_messages().await }
    async fn list_messages_in_time_range(&self, after: DateTime<Utc>, before: DateTime<Utc>, limit: u32) -> DbResult<Vec<Message>> { self.current().list_messages_in_time_range(after, before, limit).await }
    async fn search_messages(&self, query: &str) -> DbResult<Vec<Message>> { self.current().search_messages(query).await }
    async fn search_messages_by_token_hashes(&self, hashes: &[String]) -> DbResult<Vec<Message>> { self.current().search_messages_by_token_hashes(hashes).await }
    async fn find_message_by_external_id(&self, external_id: &str) -> DbResult<Option<Message>> { self.current().find_message_by_external_id(external_id).await }
    async fn set_message_encryption(&self, id: &str, body_ciphertext: &str, body_nonce: &str, subject_ciphertext: Option<&str>, subject_nonce: Option<&str>, body_html_ciphertext: Option<&str>, body_html_nonce: Option<&str>, body_token_hashes: &[String]) -> DbResult<()> {
        self.current().set_message_encryption(id, body_ciphertext, body_nonce, subject_ciphertext, subject_nonce, body_html_ciphertext, body_html_nonce, body_token_hashes).await
    }

    async fn create_conversation(&self, conversation: Conversation) -> DbResult<Conversation> { self.current().create_conversation(conversation).await }
    async fn set_conversation_title_encryption(&self, id: &str, title_ciphertext: &str, title_nonce: &str) -> DbResult<()> { self.current().set_conversation_title_encryption(id, title_ciphertext, title_nonce).await }
    async fn get_conversation(&self, id: &str) -> DbResult<Conversation> { self.current().get_conversation(id).await }
    async fn list_conversations(&self, channel: Option<&ChannelType>) -> DbResult<Vec<Conversation>> { self.current().list_conversations(channel).await }
    async fn update_conversation_unread(&self, id: &str, unread_count: u32) -> DbResult<Conversation> { self.current().update_conversation_unread(id, unread_count).await }
    async fn update_conversation_last_message_at(&self, id: &str, at: DateTime<Utc>) -> DbResult<Conversation> { self.current().update_conversation_last_message_at(id, at).await }
    async fn delete_conversation(&self, id: &str) -> DbResult<()> { self.current().delete_conversation(id).await }
    async fn link_conversation_to_thread(&self, conversation_id: &str, thread_id: &str) -> DbResult<Conversation> { self.current().link_conversation_to_thread(conversation_id, thread_id).await }

    async fn create_entity(&self, entity: Entity) -> DbResult<Entity> { self.current().create_entity(entity).await }
    async fn list_entities(&self) -> DbResult<Vec<Entity>> { self.current().list_entities().await }
    async fn create_pii_record(&self, record: PiiRecord) -> DbResult<PiiRecord> { self.current().create_pii_record(record).await }
    async fn get_pii_record(&self, id: &str) -> DbResult<PiiRecord> { self.current().get_pii_record(id).await }
    async fn list_pii_records(&self, entity_id: Option<&str>, review_state: Option<ReviewState>, stored_secret: Option<bool>) -> DbResult<Vec<PiiRecord>> { self.current().list_pii_records(entity_id, review_state, stored_secret).await }
    async fn update_pii_record_review_state(&self, id: &str, review_state: ReviewState) -> DbResult<()> { self.current().update_pii_record_review_state(id, review_state).await }
    async fn update_pii_record_value(&self, id: &str, value_encrypted: &str, value_nonce: &str) -> DbResult<()> { self.current().update_pii_record_value(id, value_encrypted, value_nonce).await }
    async fn soft_delete_pii_record(&self, id: &str) -> DbResult<()> { self.current().soft_delete_pii_record(id).await }
    async fn get_entity(&self, id: &str) -> DbResult<Entity> { self.current().get_entity(id).await }
    async fn update_entity(&self, id: &str, name: Option<&str>, kind: Option<EntityKind>, domains: Option<Vec<String>>, contact_ids: Option<Vec<String>>, notes: Option<&str>, is_owned: Option<bool>, deleted_at: Option<Option<String>>) -> DbResult<Entity> { self.current().update_entity(id, name, kind, domains, contact_ids, notes, is_owned, deleted_at).await }

    async fn create_share_record(&self, record: ShareRecord) -> DbResult<ShareRecord> { self.current().create_share_record(record).await }
    async fn set_share_record_via_url_encryption(&self, id: &str, via_url_ciphertext: &str, via_url_nonce: &str) -> DbResult<()> { self.current().set_share_record_via_url_encryption(id, via_url_ciphertext, via_url_nonce).await }
    async fn list_share_records_for_entity(&self, entity_id: &str) -> DbResult<Vec<ShareRecord>> { self.current().list_share_records_for_entity(entity_id).await }
    async fn list_all_share_records(&self) -> DbResult<Vec<ShareRecord>> { self.current().list_all_share_records().await }
    async fn get_share_record(&self, id: &str) -> DbResult<ShareRecord> { self.current().get_share_record(id).await }

    async fn update_pii_record_sources(&self, id: &str, sources: Vec<SourceRef>) -> DbResult<()> { self.current().update_pii_record_sources(id, sources).await }
    async fn update_pii_record_revealed_at(&self, id: &str, last_revealed_at: DateTime<Utc>) -> DbResult<()> { self.current().update_pii_record_revealed_at(id, last_revealed_at).await }
    async fn update_document_pii_fields(&self, id: &str, body_raw_encrypted: Option<&str>, body_raw_nonce: Option<&str>, pii_scanned_at: Option<DateTime<Utc>>) -> DbResult<()> { self.current().update_document_pii_fields(id, body_raw_encrypted, body_raw_nonce, pii_scanned_at).await }
    async fn update_message_body(&self, id: &str, body: &str, body_html: Option<&str>) -> DbResult<()> { self.current().update_message_body(id, body, body_html).await }
    async fn update_message_pii_fields(&self, id: &str, body_raw_encrypted: Option<&str>, body_raw_nonce: Option<&str>, pii_scanned_at: Option<DateTime<Utc>>) -> DbResult<()> { self.current().update_message_pii_fields(id, body_raw_encrypted, body_raw_nonce, pii_scanned_at).await }
    async fn update_contact_pii_fields(&self, id: &str, pii_scanned_at: Option<DateTime<Utc>>) -> DbResult<()> { self.current().update_contact_pii_fields(id, pii_scanned_at).await }

    async fn create_thread_with_id(&self, thread: Thread) -> DbResult<bool> { self.current().create_thread_with_id(thread).await }
    async fn create_entity_with_id(&self, entity: Entity) -> DbResult<bool> { self.current().create_entity_with_id(entity).await }
    async fn create_pii_record_with_id(&self, record: PiiRecord) -> DbResult<bool> { self.current().create_pii_record_with_id(record).await }
    async fn create_share_record_with_id(&self, record: ShareRecord) -> DbResult<bool> { self.current().create_share_record_with_id(record).await }
    async fn create_contact_with_id(&self, contact: Contact) -> DbResult<bool> { self.current().create_contact_with_id(contact).await }
    async fn create_message_with_id(&self, message: Message) -> DbResult<bool> { self.current().create_message_with_id(message).await }
    async fn create_conversation_with_id(&self, conversation: Conversation) -> DbResult<bool> { self.current().create_conversation_with_id(conversation).await }
    async fn create_milestone_with_id(&self, milestone: Milestone) -> DbResult<bool> { self.current().create_milestone_with_id(milestone).await }
    async fn create_relationship_with_id(&self, rel: RelatedTo) -> DbResult<bool> { self.current().create_relationship_with_id(rel).await }
    async fn create_suggested_link_with_id(&self, link: SuggestedLink) -> DbResult<bool> { self.current().create_suggested_link_with_id(link).await }
    async fn get_milestone(&self, id: &str) -> DbResult<Milestone> { self.current().get_milestone(id).await }
    async fn get_relationship(&self, id: &str) -> DbResult<RelatedTo> { self.current().get_relationship(id).await }
    async fn get_suggested_link(&self, id: &str) -> DbResult<SuggestedLink> { self.current().get_suggested_link(id).await }
    async fn list_all_suggested_links(&self) -> DbResult<Vec<SuggestedLink>> { self.current().list_all_suggested_links().await }
    async fn set_suggested_link_status(&self, id: &str, status: SuggestionStatus, resolved_at: Option<DateTime<Utc>>) -> DbResult<()> { self.current().set_suggested_link_status(id, status, resolved_at).await }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockGraphDB;

    /// Building block: a fresh layer wrapping a fresh MockGraphDB.
    fn build() -> (Arc<MockGraphDB>, Arc<LayeredGraphDB>) {
        let inner = Arc::new(MockGraphDB::new());
        let layered = Arc::new(LayeredGraphDB::new(inner.clone()));
        (inner, layered)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn forwards_to_initial_inner() {
        let (_, layered) = build();
        let t = layered.create_thread(Thread::new("work".into(), "stuff".into())).await.unwrap();
        let listed = layered.list_threads().await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, t.id);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn swap_replaces_inner_atomically() {
        let (first_inner, layered) = build();
        // Write through the first inner via the layer.
        layered.create_thread(Thread::new("first".into(), "".into())).await.unwrap();
        assert_eq!(first_inner.list_threads().await.unwrap().len(), 1);

        // Hot-swap to a brand new empty inner.
        let second_inner = Arc::new(MockGraphDB::new());
        layered.swap(second_inner.clone());

        // Reads through the layer see the new inner (empty).
        assert!(layered.list_threads().await.unwrap().is_empty());
        // The original inner still has its one thread (we didn't touch it).
        assert_eq!(first_inner.list_threads().await.unwrap().len(), 1);

        // Writes through the layer now land on the second inner.
        layered.create_thread(Thread::new("second".into(), "".into())).await.unwrap();
        assert_eq!(second_inner.list_threads().await.unwrap().len(), 1);
        assert_eq!(first_inner.list_threads().await.unwrap().len(), 1, "first inner untouched after swap");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn raw_inner_is_stable_across_swaps() {
        // Regression guard: the bootstrap inner returned by raw_inner() must
        // stay pointing at the original storage, even after a swap replaces
        // the current active inner. If raw_inner() ever started returning the
        // current (or the layer itself), the login wiring would build an
        // encryption decorator around a wrapper-around-itself and any DB call
        // would stack-overflow.
        let (first_inner, layered) = build();
        let bootstrap_ptr = Arc::as_ptr(&layered.raw_inner()) as *const ();

        // Add a row through the layer, then swap to a new inner.
        layered.create_thread(Thread::new("a".into(), "".into())).await.unwrap();
        let second_inner = Arc::new(MockGraphDB::new());
        layered.swap(second_inner.clone());

        // raw_inner() still points at the bootstrap.
        let after_swap_ptr = Arc::as_ptr(&layered.raw_inner()) as *const ();
        assert_eq!(bootstrap_ptr, after_swap_ptr, "raw_inner must not change");

        // And the bootstrap still has the original row.
        assert_eq!(first_inner.list_threads().await.unwrap().len(), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn swap_is_idempotent_on_same_inner() {
        let (inner, layered) = build();
        layered.swap(inner.clone());
        layered.swap(inner.clone());
        layered.create_thread(Thread::new("x".into(), "".into())).await.unwrap();
        assert_eq!(inner.list_threads().await.unwrap().len(), 1);
    }
}
