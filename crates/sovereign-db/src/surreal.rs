use async_trait::async_trait;
use chrono::Utc;
use surrealdb::engine::local::{Db, Mem, RocksDb};
use surrealdb::Surreal;

use crate::error::{DbError, DbResult};
use crate::schema::{
    Commit, Document, DocumentSnapshot, DocumentType, RelatedTo, RelationType, Thread,
};
use crate::traits::GraphDB;

/// Storage mode for SurrealDB
pub enum StorageMode {
    Memory,
    Persistent(String),
}

/// SurrealDB implementation of the GraphDB trait
pub struct SurrealGraphDB {
    db: Surreal<Db>,
}

impl SurrealGraphDB {
    /// Create a new SurrealGraphDB with the given storage mode.
    pub async fn new(mode: StorageMode) -> DbResult<Self> {
        let db = match mode {
            StorageMode::Memory => Surreal::new::<Mem>(()).await?,
            StorageMode::Persistent(ref path) => Surreal::new::<RocksDb>(path).await?,
        };
        Ok(Self { db })
    }
}

/// Parse a SurrealDB thing string like "document:abc123" into ("document", "abc123").
fn parse_thing(id: &str) -> DbResult<(&str, &str)> {
    id.split_once(':')
        .ok_or_else(|| DbError::InvalidId(format!("Expected 'table:id' format, got: {id}")))
}

#[async_trait]
impl GraphDB for SurrealGraphDB {
    async fn connect(&self) -> DbResult<()> {
        self.db
            .use_ns("sovereign")
            .use_db("main")
            .await
            .map_err(|e| DbError::Connection(e.to_string()))
    }

    async fn init_schema(&self) -> DbResult<()> {
        let queries = [
            "DEFINE INDEX idx_thread_id ON document FIELDS thread_id",
            "DEFINE INDEX idx_doc_type ON document FIELDS doc_type",
            "DEFINE INDEX idx_doc_title ON document FIELDS title",
            "DEFINE INDEX idx_doc_created ON document FIELDS created_at",
            "DEFINE INDEX idx_commit_timestamp ON commit FIELDS timestamp",
        ];
        for q in queries {
            self.db
                .query(q)
                .await
                .map_err(|e| DbError::SchemaInit(e.to_string()))?;
        }
        Ok(())
    }

    // ── Documents ───────────────────────────────────────────

    async fn create_document(&self, doc: Document) -> DbResult<Document> {
        let created: Option<Document> = self.db.create("document").content(doc).await?;
        created.ok_or_else(|| DbError::Query("Failed to create document".into()))
    }

    async fn get_document(&self, id: &str) -> DbResult<Document> {
        let (table, key) = parse_thing(id)?;
        if table != "document" {
            return Err(DbError::InvalidId(format!("Expected document ID, got table: {table}")));
        }
        let doc: Option<Document> = self.db.select((table, key)).await?;
        doc.ok_or_else(|| DbError::NotFound(id.to_string()))
    }

    async fn list_documents(&self, thread_id: Option<&str>) -> DbResult<Vec<Document>> {
        match thread_id {
            Some(tid) => {
                let tid = tid.to_string();
                let mut result = self
                    .db
                    .query("SELECT * FROM document WHERE thread_id = $tid ORDER BY created_at DESC")
                    .bind(("tid", tid))
                    .await?;
                let docs: Vec<Document> = result.take(0)?;
                Ok(docs)
            }
            None => {
                let docs: Vec<Document> = self.db.select("document").await?;
                Ok(docs)
            }
        }
    }

    async fn update_document(
        &self,
        id: &str,
        title: Option<&str>,
        content: Option<&str>,
        doc_type: Option<DocumentType>,
    ) -> DbResult<Document> {
        let (table, key) = parse_thing(id)?;
        if table != "document" {
            return Err(DbError::InvalidId(format!("Expected document ID, got table: {table}")));
        }

        // Fetch current document
        let current: Option<Document> = self.db.select((table, key)).await?;
        let mut doc = current.ok_or_else(|| DbError::NotFound(id.to_string()))?;

        if let Some(t) = title {
            doc.title = t.to_string();
        }
        if let Some(c) = content {
            doc.content = c.to_string();
        }
        if let Some(dt) = doc_type {
            doc.doc_type = dt;
        }
        doc.modified_at = Utc::now();

        let updated: Option<Document> = self.db.update((table, key)).content(doc).await?;
        updated.ok_or_else(|| DbError::Query("Failed to update document".into()))
    }

    async fn delete_document(&self, id: &str) -> DbResult<()> {
        let (table, key) = parse_thing(id)?;
        if table != "document" {
            return Err(DbError::InvalidId(format!("Expected document ID, got table: {table}")));
        }
        let _: Option<Document> = self.db.delete((table, key)).await?;
        Ok(())
    }

    // ── Threads ─────────────────────────────────────────────

    async fn create_thread(&self, thread: Thread) -> DbResult<Thread> {
        let created: Option<Thread> = self.db.create("thread").content(thread).await?;
        created.ok_or_else(|| DbError::Query("Failed to create thread".into()))
    }

    async fn get_thread(&self, id: &str) -> DbResult<Thread> {
        let (table, key) = parse_thing(id)?;
        if table != "thread" {
            return Err(DbError::InvalidId(format!("Expected thread ID, got table: {table}")));
        }
        let thread: Option<Thread> = self.db.select((table, key)).await?;
        thread.ok_or_else(|| DbError::NotFound(id.to_string()))
    }

    async fn list_threads(&self) -> DbResult<Vec<Thread>> {
        let threads: Vec<Thread> = self.db.select("thread").await?;
        Ok(threads)
    }

    // ── Relationships ───────────────────────────────────────

    async fn create_relationship(
        &self,
        from_id: &str,
        to_id: &str,
        relation_type: RelationType,
        strength: f32,
    ) -> DbResult<RelatedTo> {
        let now = Utc::now();
        let relation_type_str = relation_type.to_string();

        let query = format!(
            "RELATE {from_id}->related_to->{to_id} SET \
             relation_type = $rtype, \
             strength = $strength, \
             created_at = $created_at \
             RETURN AFTER"
        );

        let mut result = self
            .db
            .query(&query)
            .bind(("rtype", relation_type_str))
            .bind(("strength", strength))
            .bind(("created_at", now))
            .await?;

        let rels: Vec<RelatedTo> = result.take(0)?;
        rels.into_iter()
            .next()
            .ok_or_else(|| DbError::Query("Failed to create relationship".into()))
    }

    async fn list_relationships(&self, doc_id: &str) -> DbResult<Vec<RelatedTo>> {
        // Use record reference directly in query since SurrealDB stores in/out as Thing
        let query = format!(
            "SELECT * FROM related_to WHERE in = {doc_id} OR out = {doc_id}"
        );
        let mut result = self.db.query(&query).await?;
        let rels: Vec<RelatedTo> = result.take(0)?;
        Ok(rels)
    }

    async fn traverse(&self, doc_id: &str, depth: u32, limit: u32) -> DbResult<Vec<Document>> {
        // Build traversal path based on depth
        let arrow_path = "->related_to->document".repeat(depth as usize);
        let query = format!(
            "SELECT {arrow_path} FROM {doc_id} LIMIT {limit}"
        );
        let mut result = self.db.query(&query).await?;
        let docs: Vec<Document> = result.take(0)?;
        Ok(docs)
    }

    // ── Version control ─────────────────────────────────────

    async fn commit(&self, message: &str) -> DbResult<Commit> {
        // Snapshot all documents
        let all_docs: Vec<Document> = self.db.select("document").await?;
        let snapshots: Vec<DocumentSnapshot> = all_docs
            .into_iter()
            .map(|doc| DocumentSnapshot {
                document_id: doc.id_string().unwrap_or_default(),
                title: doc.title,
                content: doc.content,
                doc_type: doc.doc_type,
            })
            .collect();

        let commit = Commit {
            id: None,
            message: message.to_string(),
            timestamp: Utc::now(),
            snapshots,
        };

        let created: Option<Commit> = self.db.create("commit").content(commit).await?;
        created.ok_or_else(|| DbError::Query("Failed to create commit".into()))
    }

    async fn list_commits(&self) -> DbResult<Vec<Commit>> {
        let mut result = self
            .db
            .query("SELECT * FROM commit ORDER BY timestamp DESC")
            .await?;
        let commits: Vec<Commit> = result.take(0)?;
        Ok(commits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_db() -> SurrealGraphDB {
        let db = SurrealGraphDB::new(StorageMode::Memory).await.unwrap();
        db.connect().await.unwrap();
        db.init_schema().await.unwrap();
        db
    }

    #[tokio::test]
    async fn test_create_and_get_document() {
        let db = setup_db().await;
        let doc = Document::new(
            "Test Doc".into(),
            DocumentType::Markdown,
            "thread:test".into(),
            true,
        );
        let created = db.create_document(doc).await.unwrap();
        assert!(created.id.is_some());
        assert_eq!(created.title, "Test Doc");

        let id = created.id_string().unwrap();
        let fetched = db.get_document(&id).await.unwrap();
        assert_eq!(fetched.title, "Test Doc");
        assert_eq!(fetched.doc_type, DocumentType::Markdown);
        assert!(fetched.is_owned);
    }

    #[tokio::test]
    async fn test_list_documents_all() {
        let db = setup_db().await;
        for i in 0..3 {
            let doc = Document::new(
                format!("Doc {i}"),
                DocumentType::Markdown,
                "thread:test".into(),
                true,
            );
            db.create_document(doc).await.unwrap();
        }
        let docs = db.list_documents(None).await.unwrap();
        assert_eq!(docs.len(), 3);
    }

    #[tokio::test]
    async fn test_list_documents_by_thread() {
        let db = setup_db().await;
        let doc_a = Document::new("A".into(), DocumentType::Markdown, "thread:alpha".into(), true);
        let doc_b = Document::new("B".into(), DocumentType::Image, "thread:beta".into(), true);
        db.create_document(doc_a).await.unwrap();
        db.create_document(doc_b).await.unwrap();

        let alpha_docs = db.list_documents(Some("thread:alpha")).await.unwrap();
        assert_eq!(alpha_docs.len(), 1);
        assert_eq!(alpha_docs[0].title, "A");
    }

    #[tokio::test]
    async fn test_update_document() {
        let db = setup_db().await;
        let doc = Document::new("Original".into(), DocumentType::Markdown, "thread:t".into(), true);
        let created = db.create_document(doc).await.unwrap();
        let id = created.id_string().unwrap();

        let updated = db
            .update_document(&id, Some("Updated Title"), Some("New content"), None)
            .await
            .unwrap();
        assert_eq!(updated.title, "Updated Title");
        assert_eq!(updated.content, "New content");
    }

    #[tokio::test]
    async fn test_delete_document() {
        let db = setup_db().await;
        let doc = Document::new("ToDelete".into(), DocumentType::Pdf, "thread:t".into(), true);
        let created = db.create_document(doc).await.unwrap();
        let id = created.id_string().unwrap();

        db.delete_document(&id).await.unwrap();

        let result = db.get_document(&id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_create_and_list_threads() {
        let db = setup_db().await;
        let t1 = Thread::new("Research".into(), "Research notes".into());
        let t2 = Thread::new("Development".into(), "Dev stuff".into());
        db.create_thread(t1).await.unwrap();
        db.create_thread(t2).await.unwrap();

        let threads = db.list_threads().await.unwrap();
        assert_eq!(threads.len(), 2);
    }

    #[tokio::test]
    async fn test_get_thread() {
        let db = setup_db().await;
        let t = Thread::new("MyThread".into(), "desc".into());
        let created = db.create_thread(t).await.unwrap();
        let id = created.id_string().unwrap();

        let fetched = db.get_thread(&id).await.unwrap();
        assert_eq!(fetched.name, "MyThread");
    }

    #[tokio::test]
    async fn test_create_and_list_relationships() {
        let db = setup_db().await;
        let doc1 = Document::new("Doc1".into(), DocumentType::Markdown, "thread:t".into(), true);
        let doc2 = Document::new("Doc2".into(), DocumentType::Markdown, "thread:t".into(), true);
        let d1 = db.create_document(doc1).await.unwrap();
        let d2 = db.create_document(doc2).await.unwrap();
        let id1 = d1.id_string().unwrap();
        let id2 = d2.id_string().unwrap();

        let rel = db
            .create_relationship(&id1, &id2, RelationType::References, 0.8)
            .await
            .unwrap();
        assert!(rel.id.is_some());

        let rels = db.list_relationships(&id1).await.unwrap();
        assert_eq!(rels.len(), 1);
    }

    #[tokio::test]
    async fn test_commit_snapshots_documents() {
        let db = setup_db().await;
        let doc = Document::new("Snap".into(), DocumentType::Markdown, "thread:t".into(), true);
        db.create_document(doc).await.unwrap();

        let commit = db.commit("Test commit").await.unwrap();
        assert!(commit.id.is_some());
        assert_eq!(commit.message, "Test commit");
        assert_eq!(commit.snapshots.len(), 1);
        assert_eq!(commit.snapshots[0].title, "Snap");
    }

    #[tokio::test]
    async fn test_list_commits() {
        let db = setup_db().await;
        let doc = Document::new("D".into(), DocumentType::Data, "thread:t".into(), true);
        db.create_document(doc).await.unwrap();

        db.commit("First").await.unwrap();
        db.commit("Second").await.unwrap();

        let commits = db.list_commits().await.unwrap();
        assert_eq!(commits.len(), 2);
        // Most recent first
        assert_eq!(commits[0].message, "Second");
    }

    #[tokio::test]
    async fn test_get_nonexistent_document() {
        let db = setup_db().await;
        let result = db.get_document("document:nonexistent").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            DbError::NotFound(_) => {}
            other => panic!("Expected NotFound, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_invalid_id_format() {
        let db = setup_db().await;
        let result = db.get_document("nocolon").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            DbError::InvalidId(_) => {}
            other => panic!("Expected InvalidId, got: {other:?}"),
        }
    }
}
