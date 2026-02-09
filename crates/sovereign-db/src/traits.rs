use async_trait::async_trait;

use crate::error::DbResult;
use crate::schema::{
    Commit, Document, RelatedTo, RelationType, Thread,
};

/// Core database abstraction for the Sovereign OS document graph.
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

    /// Traverse the graph from a document, returning connected documents up to `depth` hops.
    async fn traverse(&self, doc_id: &str, depth: u32, limit: u32) -> DbResult<Vec<Document>>;

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
}
