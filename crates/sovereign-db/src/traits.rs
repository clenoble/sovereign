use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::error::DbResult;
use crate::schema::{
    ChannelType, Commit, Contact, Conversation, Document, Message, Milestone,
    ReadStatus, RelatedTo, RelationType, Thread,
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

    /// Search documents by title (case-insensitive substring match).
    async fn search_documents_by_title(&self, query: &str) -> DbResult<Vec<Document>>;

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
    async fn find_thread_by_name(&self, name: &str) -> DbResult<Option<Thread>>;

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

    async fn list_relationships(&self, doc_id: &str) -> DbResult<Vec<RelatedTo>>;

    /// List all relationships in the database.
    async fn list_all_relationships(&self) -> DbResult<Vec<RelatedTo>>;

    /// Traverse the graph from a document, returning connected documents up to `depth` hops.
    async fn traverse(&self, doc_id: &str, depth: u32, limit: u32) -> DbResult<Vec<Document>>;

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

    /// Search messages by body or subject text.
    async fn search_messages(&self, query: &str) -> DbResult<Vec<Message>>;

    // -- Conversations ---

    /// Create a new conversation.
    async fn create_conversation(&self, conversation: Conversation) -> DbResult<Conversation>;

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
}
