use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::error::DbResult;
use crate::schema::{
    ChannelType, Commit, Contact, Conversation, Document, Entity, Message, Milestone,
    PiiRecord, ReadStatus, RelatedTo, RelationType, ReviewState, ShareRecord, SourceRef,
    SuggestedLink, SuggestionSource, SuggestionStatus, Thread,
};

/// Core database abstraction for the Sovereign GE document graph.
///
/// Uses `async-trait` for object safety (`dyn GraphDB`).
#[async_trait]
pub trait GraphDB: Send + Sync {
    /// Connect to the database backend.
    async fn connect(&self) -> DbResult<()>;

    /// Initialize schema (tables, indexes).
    async fn init_schema(&self) -> DbResult<()>;

    // -- Documents ---

    async fn create_document(&self, doc: Document) -> DbResult<Document>;
    async fn get_document(&self, id: &str) -> DbResult<Document>;
    async fn list_documents(&self, thread_id: Option<&str>) -> DbResult<Vec<Document>>;
    async fn update_document(
        &self,
        id: &str,
        title: Option<&str>,
        content: Option<&str>,
    ) -> DbResult<Document>;
    async fn delete_document(&self, id: &str) -> DbResult<()>;

    /// Update a document's spatial canvas position.
    async fn update_document_position(&self, id: &str, x: f32, y: f32) -> DbResult<()>;

    /// Search documents by title (case-insensitive substring match).
    /// On `EncryptedGraphDB`, tokenizes + hashes the query and delegates to
    /// `search_documents_by_title_token_hashes`. On raw `SurrealGraphDB`,
    /// does a SurrealQL `string::lowercase(title) CONTAINS …` query.
    async fn search_documents_by_title(&self, query: &str) -> DbResult<Vec<Document>>;

    /// Blind-index lookup over `document.title_token_hashes` (CONTAINSALL).
    async fn search_documents_by_title_token_hashes(
        &self,
        hashes: &[String],
    ) -> DbResult<Vec<Document>>;

    /// Internal setter used by `EncryptedGraphDB::create_document` /
    /// `update_document` to write back encrypted title fields. The id-passed-in
    /// must already exist in the DB.
    async fn set_document_title_encryption(
        &self,
        id: &str,
        title_ciphertext: &str,
        title_nonce: &str,
        title_token_hashes: &[String],
    ) -> DbResult<()>;

    /// Update a document's reliability assessment fields.
    async fn update_document_reliability(
        &self,
        id: &str,
        source_url: Option<&str>,
        classification: Option<&str>,
        score: Option<f32>,
        assessment_json: Option<&str>,
    ) -> DbResult<Document>;

    // -- Threads ---

    async fn create_thread(&self, thread: Thread) -> DbResult<Thread>;
    async fn get_thread(&self, id: &str) -> DbResult<Thread>;
    async fn list_threads(&self) -> DbResult<Vec<Thread>>;
    async fn update_thread(
        &self,
        id: &str,
        name: Option<&str>,
        description: Option<&str>,
    ) -> DbResult<Thread>;
    async fn delete_thread(&self, id: &str) -> DbResult<()>;

    /// Find a thread by name (case-insensitive substring match). Returns first match.
    /// On `EncryptedGraphDB`, tokenizes + hashes the name and delegates to
    /// `find_thread_by_name_token_hashes`.
    async fn find_thread_by_name(&self, name: &str) -> DbResult<Option<Thread>>;

    /// Blind-index lookup over `thread.name_token_hashes` (CONTAINSALL).
    /// Returns first matching active thread, or None.
    async fn find_thread_by_name_token_hashes(
        &self,
        hashes: &[String],
    ) -> DbResult<Option<Thread>>;

    /// Internal setter used by `EncryptedGraphDB` after `create_thread` /
    /// `update_thread`. Writes encrypted name + description and name token hashes.
    async fn set_thread_encryption(
        &self,
        id: &str,
        name_ciphertext: &str,
        name_nonce: &str,
        description_ciphertext: &str,
        description_nonce: &str,
        name_token_hashes: &[String],
    ) -> DbResult<()>;

    async fn move_document_to_thread(
        &self,
        doc_id: &str,
        new_thread_id: &str,
    ) -> DbResult<Document>;

    // -- Relationships ---

    async fn create_relationship(
        &self,
        from_id: &str,
        to_id: &str,
        relation_type: RelationType,
        strength: f32,
    ) -> DbResult<RelatedTo>;

    /// List edges where this document is the source (outgoing).
    async fn list_outgoing_relationships(&self, doc_id: &str) -> DbResult<Vec<RelatedTo>>;

    /// List edges where this document is the target (incoming / backlinks).
    async fn list_incoming_relationships(&self, doc_id: &str) -> DbResult<Vec<RelatedTo>>;

    /// List all relationships in the database.
    async fn list_all_relationships(&self) -> DbResult<Vec<RelatedTo>>;

    /// Traverse the graph from a document, returning connected documents up to `depth` hops.
    async fn traverse(&self, doc_id: &str, depth: u32, limit: u32) -> DbResult<Vec<Document>>;

    // -- Suggested Links (AI-created, separate from user relationships) ---

    /// Create an AI-suggested link between two documents.
    async fn create_suggested_link(
        &self,
        from_id: &str,
        to_id: &str,
        relation_type: RelationType,
        strength: f32,
        rationale: &str,
        source: SuggestionSource,
    ) -> DbResult<SuggestedLink>;

    /// List all pending (unresolved) suggested links.
    async fn list_pending_suggestions(&self) -> DbResult<Vec<SuggestedLink>>;

    /// List all suggested links for a specific document (any status).
    async fn list_suggestions_for_document(&self, doc_id: &str) -> DbResult<Vec<SuggestedLink>>;

    /// Resolve a suggestion: Accepted promotes to a real `related_to` edge,
    /// Dismissed marks it as rejected. Both set `resolved_at`.
    async fn resolve_suggestion(
        &self,
        id: &str,
        status: SuggestionStatus,
    ) -> DbResult<SuggestedLink>;

    /// Check if a suggestion already exists for this document pair (any status, bidirectional).
    async fn suggestion_exists(&self, from_id: &str, to_id: &str) -> DbResult<bool>;

    // -- Adopt ---

    /// Mark a document as owned (adopt external content).
    async fn adopt_document(&self, id: &str) -> DbResult<Document>;

    // -- Thread merge/split ---

    /// Merge source thread into target: move all docs from source to target, soft-delete source.
    async fn merge_threads(&self, target_id: &str, source_id: &str) -> DbResult<()>;

    /// Split specified docs out of a thread into a new thread with the given name.
    async fn split_thread(
        &self,
        thread_id: &str,
        doc_ids: &[String],
        new_name: &str,
    ) -> DbResult<Thread>;

    // -- Soft delete ---

    /// Mark a document as deleted (soft delete). Sets deleted_at timestamp.
    async fn soft_delete_document(&self, id: &str) -> DbResult<()>;

    /// Restore a soft-deleted document (clear deleted_at).
    async fn restore_soft_deleted_document(&self, id: &str) -> DbResult<Document>;

    /// Mark a thread as deleted (soft delete). Sets deleted_at timestamp.
    async fn soft_delete_thread(&self, id: &str) -> DbResult<()>;

    /// Restore a soft-deleted thread (clear deleted_at).
    async fn restore_soft_deleted_thread(&self, id: &str) -> DbResult<Thread>;

    /// Permanently remove records whose deleted_at is older than `max_age`.
    async fn purge_deleted(&self, max_age: std::time::Duration) -> DbResult<u64>;

    // -- Version control ---

    /// Snapshot a single document into a commit, linked to its parent commit.
    async fn commit_document(&self, doc_id: &str, message: &str) -> DbResult<Commit>;

    /// List commits for a specific document, most recent first.
    async fn list_document_commits(&self, doc_id: &str) -> DbResult<Vec<Commit>>;

    /// Get a single commit by ID.
    async fn get_commit(&self, commit_id: &str) -> DbResult<Commit>;

    /// Restore a document to a previous commit's snapshot.
    async fn restore_document(&self, doc_id: &str, commit_id: &str) -> DbResult<Document>;

    // -- Milestones ---

    /// Create a milestone on a thread's timeline.
    async fn create_milestone(&self, milestone: Milestone) -> DbResult<Milestone>;

    /// List milestones for a thread, most recent first.
    async fn list_milestones(&self, thread_id: &str) -> DbResult<Vec<Milestone>>;

    /// List all milestones across all threads, most recent first.
    async fn list_all_milestones(&self) -> DbResult<Vec<Milestone>>;

    /// Delete a milestone by ID.
    async fn delete_milestone(&self, id: &str) -> DbResult<()>;

    // -- Contacts ---

    /// Create a new contact.
    async fn create_contact(&self, contact: Contact) -> DbResult<Contact>;

    /// Get a contact by ID.
    async fn get_contact(&self, id: &str) -> DbResult<Contact>;

    /// List all contacts (excludes soft-deleted).
    async fn list_contacts(&self) -> DbResult<Vec<Contact>>;

    /// Update a contact's name, notes, or avatar.
    async fn update_contact(
        &self,
        id: &str,
        name: Option<&str>,
        notes: Option<&str>,
        avatar: Option<&str>,
    ) -> DbResult<Contact>;

    /// Hard-delete a contact.
    async fn delete_contact(&self, id: &str) -> DbResult<()>;

    /// Internal setter for the encrypted `name` field on Contact. Writes
    /// ciphertext and the paired nonce; leaves `notes` and its nonce untouched.
    async fn set_contact_name_encryption(
        &self,
        id: &str,
        name_ciphertext: &str,
        name_nonce: &str,
    ) -> DbResult<()>;

    /// Internal setter for the encrypted `notes` field on Contact. Writes
    /// ciphertext and the paired `encryption_nonce`. Fixes a pre-Phase-2b path
    /// where notes ciphertext landed in the row but the nonce never did
    /// (`update_contact` doesn't write `encryption_nonce`), so subsequent
    /// reads returned ciphertext as plaintext.
    async fn set_contact_notes_encryption(
        &self,
        id: &str,
        notes_ciphertext: &str,
        notes_nonce: &str,
    ) -> DbResult<()>;

    /// Soft-delete a contact.
    async fn soft_delete_contact(&self, id: &str) -> DbResult<()>;

    /// Find a contact by channel address.
    async fn find_contact_by_address(&self, address: &str) -> DbResult<Option<Contact>>;

    /// Add an address to an existing contact.
    async fn add_contact_address(
        &self,
        contact_id: &str,
        address: crate::schema::ChannelAddress,
    ) -> DbResult<Contact>;

    // -- Messages ---

    /// Create a new message.
    async fn create_message(&self, message: Message) -> DbResult<Message>;

    /// Get a message by ID.
    async fn get_message(&self, id: &str) -> DbResult<Message>;

    /// List messages in a conversation, ordered by sent_at descending.
    /// `before` enables cursor-based pagination (messages sent before this time).
    /// `limit` caps the result count.
    async fn list_messages(
        &self,
        conversation_id: &str,
        before: Option<DateTime<Utc>>,
        limit: u32,
    ) -> DbResult<Vec<Message>>;

    /// Update a message's read status.
    async fn update_message_read_status(
        &self,
        id: &str,
        status: ReadStatus,
    ) -> DbResult<Message>;

    /// Hard-delete a message.
    async fn delete_message(&self, id: &str) -> DbResult<()>;

    /// List all messages across all conversations, ordered by sent_at descending.
    async fn list_all_messages(&self) -> DbResult<Vec<Message>>;

    /// List messages within a time range, ordered by sent_at descending.
    async fn list_messages_in_time_range(
        &self,
        after: DateTime<Utc>,
        before: DateTime<Utc>,
        limit: u32,
    ) -> DbResult<Vec<Message>>;

    /// Search messages by body or subject text.
    ///
    /// On an `EncryptedGraphDB`, this tokenizes the query, hashes the tokens
    /// against the per-DB index key, and delegates to
    /// `search_messages_by_token_hashes` on the inner DB. On a raw `SurrealGraphDB`
    /// it does a plaintext CONTAINS query against body/subject (used for tests
    /// against unencrypted DBs and for any inner DB whose data is still plaintext).
    async fn search_messages(&self, query: &str) -> DbResult<Vec<Message>>;

    /// Search messages by precomputed blind-index token hashes (CONTAINSALL semantics).
    ///
    /// All supplied hashes must be present in a row's `body_token_hashes` for it
    /// to match. An empty `hashes` slice matches nothing (callers should short-circuit).
    async fn search_messages_by_token_hashes(
        &self,
        hashes: &[String],
    ) -> DbResult<Vec<Message>>;

    /// Internal setter used by `EncryptedGraphDB` to write back the ciphertext
    /// fields after a message is created. Updates body, subject, body_html and
    /// their nonces, plus the body_token_hashes index array. The id-passed-in
    /// must already exist in the DB. Not intended to be called by application
    /// code — use `create_message` / `EncryptedGraphDB::create_message`.
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
    ) -> DbResult<()>;

    // -- Conversations ---

    /// Create a new conversation.
    async fn create_conversation(&self, conversation: Conversation) -> DbResult<Conversation>;

    /// Internal setter for the encrypted `title` field on Conversation.
    async fn set_conversation_title_encryption(
        &self,
        id: &str,
        title_ciphertext: &str,
        title_nonce: &str,
    ) -> DbResult<()>;

    /// Get a conversation by ID.
    async fn get_conversation(&self, id: &str) -> DbResult<Conversation>;

    /// List conversations, optionally filtered by channel type.
    async fn list_conversations(
        &self,
        channel: Option<&ChannelType>,
    ) -> DbResult<Vec<Conversation>>;

    /// Update a conversation's unread count.
    async fn update_conversation_unread(
        &self,
        id: &str,
        unread_count: u32,
    ) -> DbResult<Conversation>;

    /// Update a conversation's last_message_at timestamp.
    async fn update_conversation_last_message_at(
        &self,
        id: &str,
        at: DateTime<Utc>,
    ) -> DbResult<Conversation>;

    /// Hard-delete a conversation.
    async fn delete_conversation(&self, id: &str) -> DbResult<()>;

    /// Link a conversation to a thread.
    async fn link_conversation_to_thread(
        &self,
        conversation_id: &str,
        thread_id: &str,
    ) -> DbResult<Conversation>;

    // -- Entities (PII management) ---

    /// Create a new business / personal entity. Used by the PII pipeline
    /// to write disambiguator-proposed entities (`is_owned == false`)
    /// and by the dashboard's "new entity" flow (`is_owned == true`).
    async fn create_entity(&self, entity: Entity) -> DbResult<Entity>;

    /// List all entities (excludes soft-deleted), ordered by name.
    async fn list_entities(&self) -> DbResult<Vec<Entity>>;

    // -- PII Records ---

    /// Insert a new PiiRecord. Discovered findings (`stored_secret == false`)
    /// arrive here from the ingest pipeline; vault entries
    /// (`stored_secret == true`) arrive from the dashboard "new secret" flow.
    async fn create_pii_record(&self, record: PiiRecord) -> DbResult<PiiRecord>;

    /// Fetch a `PiiRecord` by ID. The returned record's `value_encrypted`
    /// is still ciphertext — callers decrypt via `EncryptedBlob` and the
    /// `DeviceKey`. Used by the resolution API (step 5) when expanding
    /// `[pii:<record_id>]` tokens.
    async fn get_pii_record(&self, id: &str) -> DbResult<PiiRecord>;

    /// List PiiRecords with optional filters. Excludes soft-deleted
    /// records. Order: most-recently-discovered first.
    ///
    /// All filter args are AND-combined: passing `entity_id = Some(...)`,
    /// `review_state = Some(Confirmed)`, `stored_secret = Some(false)`
    /// returns confirmed discovered findings for that entity.
    async fn list_pii_records(
        &self,
        entity_id: Option<&str>,
        review_state: Option<ReviewState>,
        stored_secret: Option<bool>,
    ) -> DbResult<Vec<PiiRecord>>;

    /// Set a record's `review_state`. Used by the dashboard's review
    /// queue when the user confirms or dismisses an Unreviewed finding.
    async fn update_pii_record_review_state(
        &self,
        id: &str,
        review_state: ReviewState,
    ) -> DbResult<()>;

    /// Replace a PiiRecord's encrypted value + nonce in place. Used by
    /// the v0.0.4 → v0.0.5 migration to re-encrypt vault entries from
    /// the per-device DeviceKey to the user-scoped AccountKey, and by
    /// any future re-key flow. Other fields are untouched.
    async fn update_pii_record_value(
        &self,
        id: &str,
        value_encrypted: &str,
        value_nonce: &str,
    ) -> DbResult<()>;

    /// Soft-delete a PiiRecord. Sets `deleted_at` so the record falls
    /// out of `list_pii_records` but remains in the DB for audit / undo.
    /// Used by the dashboard's redact (L5) action.
    async fn soft_delete_pii_record(&self, id: &str) -> DbResult<()>;

    // -- Entity reads ---

    /// Fetch an `Entity` by ID.
    async fn get_entity(&self, id: &str) -> DbResult<Entity>;

    // -- Share Records (PII sharing ledger) ---

    /// Insert a new `ShareRecord` documenting that a `PiiRecord` was
    /// disclosed to an `Entity` at a moment in time. Always outbound;
    /// receiving PII isn't tracked here.
    async fn create_share_record(&self, record: ShareRecord) -> DbResult<ShareRecord>;

    /// Internal setter for the encrypted `via_url` field on ShareRecord.
    /// No-op when `via_url` was None at create time (nothing to encrypt).
    async fn set_share_record_via_url_encryption(
        &self,
        id: &str,
        via_url_ciphertext: &str,
        via_url_nonce: &str,
    ) -> DbResult<()>;

    /// List share records where `to_entity_id == entity_id`. Used by
    /// the dashboard's Shared tab on the entity-detail panel. Order:
    /// most-recently-shared first.
    async fn list_share_records_for_entity(
        &self,
        entity_id: &str,
    ) -> DbResult<Vec<ShareRecord>>;

    /// Return every share record (across all entities). Used by the
    /// cross-device sync engine to build the share-ledger manifest;
    /// share records are append-only so the full list isn't pruned.
    async fn list_all_share_records(&self) -> DbResult<Vec<ShareRecord>>;

    /// Fetch a single share record by ID. Used by the sync engine when
    /// a remote peer requests specific share records via `GetRows`.
    async fn get_share_record(&self, id: &str) -> DbResult<ShareRecord>;

    /// Replace a record's `sources` list. Used by the ingest hook after
    /// canonical-body substitution to update spans from indexed
    /// placeholders to the post-substitution placeholder spans.
    async fn update_pii_record_sources(
        &self,
        id: &str,
        sources: Vec<SourceRef>,
    ) -> DbResult<()>;

    /// Set `last_revealed_at` on a PiiRecord. Called by the resolution
    /// API every time the user reveals a value (L3 Modify), so the
    /// dashboard can show "this PII was last viewed N hours ago".
    async fn update_pii_record_revealed_at(
        &self,
        id: &str,
        last_revealed_at: chrono::DateTime<Utc>,
    ) -> DbResult<()>;

    /// Set the PII-pipeline-managed fields on a Document: encrypted raw
    /// body + nonce, plus the scan timestamp. Caller is responsible for
    /// updating `content` separately (via `update_document`) since the
    /// ingest hook returns the canonical body for the same write path.
    async fn update_document_pii_fields(
        &self,
        id: &str,
        body_raw_encrypted: Option<&str>,
        body_raw_nonce: Option<&str>,
        pii_scanned_at: Option<chrono::DateTime<Utc>>,
    ) -> DbResult<()>;

    /// Replace a Message's `body` (and optionally `body_html`) — used
    /// after PII ingest to rewrite the body in canonical form with
    /// `[pii:<record_id>]` tokens. There is no general-purpose
    /// `update_message` method because the body is the only field we
    /// rewrite post-ingest; everything else is set at create time.
    async fn update_message_body(
        &self,
        id: &str,
        body: &str,
        body_html: Option<&str>,
    ) -> DbResult<()>;

    /// Set the PII-pipeline-managed fields on a Message: encrypted raw
    /// body + nonce, plus the scan timestamp. Mirrors
    /// `update_document_pii_fields` for the Message table.
    async fn update_message_pii_fields(
        &self,
        id: &str,
        body_raw_encrypted: Option<&str>,
        body_raw_nonce: Option<&str>,
        pii_scanned_at: Option<chrono::DateTime<Utc>>,
    ) -> DbResult<()>;

    /// Set the PII-pipeline-managed fields on a Contact. Contact has no
    /// `body_raw_encrypted` (notes are already encrypted via the per-doc
    /// key by `EncryptedGraphDB`); only `pii_scanned_at` is set here.
    /// Caller uses the existing `update_contact` method to rewrite
    /// `notes` with canonical-form tokens.
    async fn update_contact_pii_fields(
        &self,
        id: &str,
        pii_scanned_at: Option<chrono::DateTime<Utc>>,
    ) -> DbResult<()>;
}
