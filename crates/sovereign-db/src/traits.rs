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

    // -- Version control ---

    /// Snapshot all documents into a commit record.
    async fn commit(&self, message: &str) -> DbResult<Commit>;

    /// List all commits, most recent first.
    async fn list_commits(&self) -> DbResult<Vec<Commit>>;
}
