use async_trait::async_trait;
use chrono::Utc;
use surrealdb::engine::local::{Db, Mem, RocksDb};
use surrealdb::Surreal;

use crate::error::{DbError, DbResult};
use crate::schema::{
    ChannelType, Commit, Contact, Conversation, Document, DocumentSnapshot,
    Message, Milestone, ReadStatus, RelatedTo, RelationType, Thread,
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
            "DEFINE INDEX idx_doc_title ON document FIELDS title",
            "DEFINE INDEX idx_doc_created ON document FIELDS created_at",
            "DEFINE INDEX idx_commit_timestamp ON commit FIELDS timestamp",
            "DEFINE INDEX idx_commit_doc ON commit FIELDS document_id",
            // Contact indexes
            "DEFINE INDEX idx_contact_name ON contact FIELDS name",
            // Message indexes
            "DEFINE INDEX idx_message_conversation ON message FIELDS conversation_id",
            "DEFINE INDEX idx_message_sent_at ON message FIELDS sent_at",
            "DEFINE INDEX idx_message_from ON message FIELDS from_contact_id",
            "DEFINE INDEX idx_message_external ON message FIELDS external_id",
            // Conversation indexes
            "DEFINE INDEX idx_conversation_channel ON conversation FIELDS channel",
            "DEFINE INDEX idx_conversation_last_msg ON conversation FIELDS last_message_at",
        ];
        for q in queries {
            self.db
                .query(q)
                .await
                .map_err(|e| DbError::SchemaInit(e.to_string()))?;
        }
        Ok(())
    }

    // -- Documents ---

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
                    .query("SELECT * FROM document WHERE thread_id = $tid AND deleted_at IS NONE ORDER BY created_at DESC")
                    .bind(("tid", tid))
                    .await?;
                let docs: Vec<Document> = result.take(0)?;
                Ok(docs)
            }
            None => {
                let mut result = self
                    .db
                    .query("SELECT * FROM document WHERE deleted_at IS NONE")
                    .await?;
                let docs: Vec<Document> = result.take(0)?;
                Ok(docs)
            }
        }
    }

    async fn update_document(
        &self,
        id: &str,
        title: Option<&str>,
        content: Option<&str>,
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

    // -- Threads ---

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
        let mut result = self
            .db
            .query("SELECT * FROM thread WHERE deleted_at IS NONE")
            .await?;
        let threads: Vec<Thread> = result.take(0)?;
        Ok(threads)
    }

    async fn update_thread(
        &self,
        id: &str,
        name: Option<&str>,
        description: Option<&str>,
    ) -> DbResult<Thread> {
        let (table, key) = parse_thing(id)?;
        if table != "thread" {
            return Err(DbError::InvalidId(format!("Expected thread ID, got table: {table}")));
        }

        let current: Option<Thread> = self.db.select((table, key)).await?;
        let mut thread = current.ok_or_else(|| DbError::NotFound(id.to_string()))?;

        if let Some(n) = name {
            thread.name = n.to_string();
        }
        if let Some(d) = description {
            thread.description = d.to_string();
        }

        let updated: Option<Thread> = self.db.update((table, key)).content(thread).await?;
        updated.ok_or_else(|| DbError::Query("Failed to update thread".into()))
    }

    async fn delete_thread(&self, id: &str) -> DbResult<()> {
        let (table, key) = parse_thing(id)?;
        if table != "thread" {
            return Err(DbError::InvalidId(format!("Expected thread ID, got table: {table}")));
        }
        let _: Option<Thread> = self.db.delete((table, key)).await?;
        Ok(())
    }

    async fn move_document_to_thread(
        &self,
        doc_id: &str,
        new_thread_id: &str,
    ) -> DbResult<Document> {
        let (table, key) = parse_thing(doc_id)?;
        if table != "document" {
            return Err(DbError::InvalidId(format!("Expected document ID, got table: {table}")));
        }

        let current: Option<Document> = self.db.select((table, key)).await?;
        let mut doc = current.ok_or_else(|| DbError::NotFound(doc_id.to_string()))?;
        doc.thread_id = new_thread_id.to_string();
        doc.modified_at = Utc::now();

        let updated: Option<Document> = self.db.update((table, key)).content(doc).await?;
        updated.ok_or_else(|| DbError::Query("Failed to move document".into()))
    }

    // -- Adopt ---

    async fn adopt_document(&self, id: &str) -> DbResult<Document> {
        let (table, key) = parse_thing(id)?;
        if table != "document" {
            return Err(DbError::InvalidId(format!(
                "Expected document ID, got table: {table}"
            )));
        }
        let current: Option<Document> = self.db.select((table, key)).await?;
        let mut doc = current.ok_or_else(|| DbError::NotFound(id.to_string()))?;
        doc.is_owned = true;
        doc.modified_at = Utc::now();
        let updated: Option<Document> = self.db.update((table, key)).content(doc).await?;
        updated.ok_or_else(|| DbError::Query("Failed to adopt document".into()))
    }

    // -- Thread merge/split ---

    async fn merge_threads(&self, target_id: &str, source_id: &str) -> DbResult<()> {
        // Move all documents from source to target
        let source_id_str = source_id.to_string();
        let target_id_str = target_id.to_string();
        self.db
            .query("UPDATE document SET thread_id = $target WHERE thread_id = $source")
            .bind(("target", target_id_str))
            .bind(("source", source_id_str))
            .await?;

        // Soft-delete the source thread
        self.soft_delete_thread(source_id).await?;
        Ok(())
    }

    async fn split_thread(
        &self,
        _thread_id: &str,
        doc_ids: &[String],
        new_name: &str,
    ) -> DbResult<Thread> {
        // Create new thread
        let new_thread = Thread::new(new_name.to_string(), String::new());
        let created = self.create_thread(new_thread).await?;
        let new_tid = created.id_string().unwrap_or_default();

        // Move specified docs to the new thread
        for doc_id in doc_ids {
            self.move_document_to_thread(doc_id, &new_tid).await?;
        }

        Ok(created)
    }

    // -- Soft delete ---

    async fn soft_delete_document(&self, id: &str) -> DbResult<()> {
        let (table, key) = parse_thing(id)?;
        if table != "document" {
            return Err(DbError::InvalidId(format!(
                "Expected document ID, got table: {table}"
            )));
        }
        let current: Option<Document> = self.db.select((table, key)).await?;
        let mut doc = current.ok_or_else(|| DbError::NotFound(id.to_string()))?;
        doc.deleted_at = Some(Utc::now().to_rfc3339());
        let _: Option<Document> = self.db.update((table, key)).content(doc).await?;
        Ok(())
    }

    async fn restore_soft_deleted_document(&self, id: &str) -> DbResult<Document> {
        let (table, key) = parse_thing(id)?;
        if table != "document" {
            return Err(DbError::InvalidId(format!(
                "Expected document ID, got table: {table}"
            )));
        }
        let current: Option<Document> = self.db.select((table, key)).await?;
        let mut doc = current.ok_or_else(|| DbError::NotFound(id.to_string()))?;
        doc.deleted_at = None;
        let updated: Option<Document> = self.db.update((table, key)).content(doc).await?;
        updated.ok_or_else(|| DbError::Query("Failed to restore document".into()))
    }

    async fn soft_delete_thread(&self, id: &str) -> DbResult<()> {
        let (table, key) = parse_thing(id)?;
        if table != "thread" {
            return Err(DbError::InvalidId(format!(
                "Expected thread ID, got table: {table}"
            )));
        }
        let current: Option<Thread> = self.db.select((table, key)).await?;
        let mut thread = current.ok_or_else(|| DbError::NotFound(id.to_string()))?;
        thread.deleted_at = Some(Utc::now().to_rfc3339());
        let _: Option<Thread> = self.db.update((table, key)).content(thread).await?;
        Ok(())
    }

    async fn restore_soft_deleted_thread(&self, id: &str) -> DbResult<Thread> {
        let (table, key) = parse_thing(id)?;
        if table != "thread" {
            return Err(DbError::InvalidId(format!(
                "Expected thread ID, got table: {table}"
            )));
        }
        let current: Option<Thread> = self.db.select((table, key)).await?;
        let mut thread = current.ok_or_else(|| DbError::NotFound(id.to_string()))?;
        thread.deleted_at = None;
        let updated: Option<Thread> = self.db.update((table, key)).content(thread).await?;
        updated.ok_or_else(|| DbError::Query("Failed to restore thread".into()))
    }

    async fn purge_deleted(&self, max_age: std::time::Duration) -> DbResult<u64> {
        let cutoff =
            Utc::now() - chrono::Duration::seconds(max_age.as_secs() as i64);
        let cutoff_str = cutoff.to_rfc3339();

        // Delete documents older than cutoff
        let mut result = self
            .db
            .query("DELETE FROM document WHERE deleted_at IS NOT NONE AND deleted_at < $cutoff")
            .bind(("cutoff", cutoff_str.clone()))
            .await?;
        let _: Vec<Document> = result.take(0).unwrap_or_default();

        // Delete threads older than cutoff
        let mut result = self
            .db
            .query("DELETE FROM thread WHERE deleted_at IS NOT NONE AND deleted_at < $cutoff")
            .bind(("cutoff", cutoff_str))
            .await?;
        let _: Vec<Thread> = result.take(0).unwrap_or_default();

        // SurrealDB DELETE doesn't return count easily; return 0 as placeholder
        Ok(0)
    }

    // -- Milestones ---

    async fn create_milestone(&self, milestone: Milestone) -> DbResult<Milestone> {
        let created: Option<Milestone> = self.db.create("milestone").content(milestone).await?;
        created.ok_or_else(|| DbError::Query("Failed to create milestone".into()))
    }

    async fn list_milestones(&self, thread_id: &str) -> DbResult<Vec<Milestone>> {
        let tid = thread_id.to_string();
        let mut result = self
            .db
            .query("SELECT * FROM milestone WHERE thread_id = $tid ORDER BY timestamp DESC")
            .bind(("tid", tid))
            .await?;
        let milestones: Vec<Milestone> = result.take(0)?;
        Ok(milestones)
    }

    async fn delete_milestone(&self, id: &str) -> DbResult<()> {
        let (table, key) = parse_thing(id)?;
        if table != "milestone" {
            return Err(DbError::InvalidId(format!(
                "Expected milestone ID, got table: {table}"
            )));
        }
        let _: Option<Milestone> = self.db.delete((table, key)).await?;
        Ok(())
    }

    // -- Relationships ---

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
        let query = format!(
            "SELECT * FROM related_to WHERE in = {doc_id} OR out = {doc_id}"
        );
        let mut result = self.db.query(&query).await?;
        let rels: Vec<RelatedTo> = result.take(0)?;
        Ok(rels)
    }

    async fn traverse(&self, doc_id: &str, depth: u32, limit: u32) -> DbResult<Vec<Document>> {
        let arrow_path = "->related_to->document".repeat(depth as usize);
        let query = format!(
            "SELECT {arrow_path} FROM {doc_id} LIMIT {limit}"
        );
        let mut result = self.db.query(&query).await?;
        let docs: Vec<Document> = result.take(0)?;
        Ok(docs)
    }

    // -- Version control ---

    async fn commit_document(&self, doc_id: &str, message: &str) -> DbResult<Commit> {
        let doc = self.get_document(doc_id).await?;

        let snapshot = DocumentSnapshot {
            document_id: doc_id.to_string(),
            title: doc.title.clone(),
            content: doc.content.clone(),
        };

        let commit = Commit {
            id: None,
            document_id: doc_id.to_string(),
            parent_commit: doc.head_commit.clone(),
            message: message.to_string(),
            timestamp: Utc::now(),
            snapshot,
        };

        let created: Option<Commit> = self.db.create("commit").content(commit).await?;
        let created = created.ok_or_else(|| DbError::Query("Failed to create commit".into()))?;

        // Update document's head_commit pointer
        let commit_id = created.id_string().unwrap_or_default();
        let (table, key) = parse_thing(doc_id)?;
        let mut doc_updated = doc;
        doc_updated.head_commit = Some(commit_id);
        let _: Option<Document> = self.db.update((table, key)).content(doc_updated).await?;

        Ok(created)
    }

    async fn list_document_commits(&self, doc_id: &str) -> DbResult<Vec<Commit>> {
        let doc_id_owned = doc_id.to_string();
        let mut result = self
            .db
            .query("SELECT * FROM commit WHERE document_id = $doc_id ORDER BY timestamp DESC")
            .bind(("doc_id", doc_id_owned))
            .await?;
        let commits: Vec<Commit> = result.take(0)?;
        Ok(commits)
    }

    async fn get_commit(&self, commit_id: &str) -> DbResult<Commit> {
        let (table, key) = parse_thing(commit_id)?;
        if table != "commit" {
            return Err(DbError::InvalidId(format!("Expected commit ID, got table: {table}")));
        }
        let commit: Option<Commit> = self.db.select((table, key)).await?;
        commit.ok_or_else(|| DbError::NotFound(commit_id.to_string()))
    }

    async fn restore_document(&self, doc_id: &str, commit_id: &str) -> DbResult<Document> {
        let commit = self.get_commit(commit_id).await?;

        // Update document to the snapshot's state
        let (table, key) = parse_thing(doc_id)?;
        if table != "document" {
            return Err(DbError::InvalidId(format!("Expected document ID, got table: {table}")));
        }
        let current: Option<Document> = self.db.select((table, key)).await?;
        let mut doc = current.ok_or_else(|| DbError::NotFound(doc_id.to_string()))?;
        doc.title = commit.snapshot.title.clone();
        doc.content = commit.snapshot.content.clone();
        doc.modified_at = Utc::now();

        let updated: Option<Document> = self.db.update((table, key)).content(doc).await?;
        let restored = updated.ok_or_else(|| DbError::Query("Failed to restore document".into()))?;

        // Create a new commit recording the restore
        let restore_msg = format!("Restored from {}", commit_id);
        self.commit_document(doc_id, &restore_msg).await?;

        Ok(restored)
    }

    // -- Contacts ---

    async fn create_contact(&self, contact: Contact) -> DbResult<Contact> {
        let created: Option<Contact> = self.db.create("contact").content(contact).await?;
        created.ok_or_else(|| DbError::Query("Failed to create contact".into()))
    }

    async fn get_contact(&self, id: &str) -> DbResult<Contact> {
        let (table, key) = parse_thing(id)?;
        if table != "contact" {
            return Err(DbError::InvalidId(format!("Expected contact ID, got table: {table}")));
        }
        let contact: Option<Contact> = self.db.select((table, key)).await?;
        contact.ok_or_else(|| DbError::NotFound(id.to_string()))
    }

    async fn list_contacts(&self) -> DbResult<Vec<Contact>> {
        let mut result = self
            .db
            .query("SELECT * FROM contact WHERE deleted_at IS NONE ORDER BY name ASC")
            .await?;
        let contacts: Vec<Contact> = result.take(0)?;
        Ok(contacts)
    }

    async fn update_contact(
        &self,
        id: &str,
        name: Option<&str>,
        notes: Option<&str>,
        avatar: Option<&str>,
    ) -> DbResult<Contact> {
        let (table, key) = parse_thing(id)?;
        if table != "contact" {
            return Err(DbError::InvalidId(format!("Expected contact ID, got table: {table}")));
        }
        let current: Option<Contact> = self.db.select((table, key)).await?;
        let mut contact = current.ok_or_else(|| DbError::NotFound(id.to_string()))?;

        if let Some(n) = name {
            contact.name = n.to_string();
        }
        if let Some(n) = notes {
            contact.notes = n.to_string();
        }
        if let Some(a) = avatar {
            contact.avatar = Some(a.to_string());
        }
        contact.modified_at = Utc::now();

        let updated: Option<Contact> = self.db.update((table, key)).content(contact).await?;
        updated.ok_or_else(|| DbError::Query("Failed to update contact".into()))
    }

    async fn delete_contact(&self, id: &str) -> DbResult<()> {
        let (table, key) = parse_thing(id)?;
        if table != "contact" {
            return Err(DbError::InvalidId(format!("Expected contact ID, got table: {table}")));
        }
        let _: Option<Contact> = self.db.delete((table, key)).await?;
        Ok(())
    }

    async fn soft_delete_contact(&self, id: &str) -> DbResult<()> {
        let (table, key) = parse_thing(id)?;
        if table != "contact" {
            return Err(DbError::InvalidId(format!("Expected contact ID, got table: {table}")));
        }
        let current: Option<Contact> = self.db.select((table, key)).await?;
        let mut contact = current.ok_or_else(|| DbError::NotFound(id.to_string()))?;
        contact.deleted_at = Some(Utc::now().to_rfc3339());
        let _: Option<Contact> = self.db.update((table, key)).content(contact).await?;
        Ok(())
    }

    async fn find_contact_by_address(&self, address: &str) -> DbResult<Option<Contact>> {
        let addr = address.to_string();
        let mut result = self
            .db
            .query("SELECT * FROM contact WHERE deleted_at IS NONE AND addresses[*].address CONTAINS $addr")
            .bind(("addr", addr))
            .await?;
        let contacts: Vec<Contact> = result.take(0)?;
        Ok(contacts.into_iter().next())
    }

    async fn add_contact_address(
        &self,
        contact_id: &str,
        address: crate::schema::ChannelAddress,
    ) -> DbResult<Contact> {
        let (table, key) = parse_thing(contact_id)?;
        if table != "contact" {
            return Err(DbError::InvalidId(format!("Expected contact ID, got table: {table}")));
        }
        let current: Option<Contact> = self.db.select((table, key)).await?;
        let mut contact = current.ok_or_else(|| DbError::NotFound(contact_id.to_string()))?;
        contact.addresses.push(address);
        contact.modified_at = Utc::now();

        let updated: Option<Contact> = self.db.update((table, key)).content(contact).await?;
        updated.ok_or_else(|| DbError::Query("Failed to add contact address".into()))
    }

    // -- Messages ---

    async fn create_message(&self, message: Message) -> DbResult<Message> {
        let created: Option<Message> = self.db.create("message").content(message).await?;
        created.ok_or_else(|| DbError::Query("Failed to create message".into()))
    }

    async fn get_message(&self, id: &str) -> DbResult<Message> {
        let (table, key) = parse_thing(id)?;
        if table != "message" {
            return Err(DbError::InvalidId(format!("Expected message ID, got table: {table}")));
        }
        let message: Option<Message> = self.db.select((table, key)).await?;
        message.ok_or_else(|| DbError::NotFound(id.to_string()))
    }

    async fn list_messages(
        &self,
        conversation_id: &str,
        before: Option<chrono::DateTime<Utc>>,
        limit: u32,
    ) -> DbResult<Vec<Message>> {
        let cid = conversation_id.to_string();
        match before {
            Some(ts) => {
                let mut result = self
                    .db
                    .query("SELECT * FROM message WHERE conversation_id = $cid AND deleted_at IS NONE AND sent_at < $before ORDER BY sent_at DESC LIMIT $limit")
                    .bind(("cid", cid))
                    .bind(("before", ts))
                    .bind(("limit", limit))
                    .await?;
                let msgs: Vec<Message> = result.take(0)?;
                Ok(msgs)
            }
            None => {
                let mut result = self
                    .db
                    .query("SELECT * FROM message WHERE conversation_id = $cid AND deleted_at IS NONE ORDER BY sent_at DESC LIMIT $limit")
                    .bind(("cid", cid))
                    .bind(("limit", limit))
                    .await?;
                let msgs: Vec<Message> = result.take(0)?;
                Ok(msgs)
            }
        }
    }

    async fn update_message_read_status(
        &self,
        id: &str,
        status: ReadStatus,
    ) -> DbResult<Message> {
        let (table, key) = parse_thing(id)?;
        if table != "message" {
            return Err(DbError::InvalidId(format!("Expected message ID, got table: {table}")));
        }
        let current: Option<Message> = self.db.select((table, key)).await?;
        let mut message = current.ok_or_else(|| DbError::NotFound(id.to_string()))?;
        message.read_status = status;

        let updated: Option<Message> = self.db.update((table, key)).content(message).await?;
        updated.ok_or_else(|| DbError::Query("Failed to update message read status".into()))
    }

    async fn delete_message(&self, id: &str) -> DbResult<()> {
        let (table, key) = parse_thing(id)?;
        if table != "message" {
            return Err(DbError::InvalidId(format!("Expected message ID, got table: {table}")));
        }
        let _: Option<Message> = self.db.delete((table, key)).await?;
        Ok(())
    }

    async fn search_messages(&self, query: &str) -> DbResult<Vec<Message>> {
        let q = query.to_string();
        let mut result = self
            .db
            .query("SELECT * FROM message WHERE deleted_at IS NONE AND (body CONTAINS $q OR subject CONTAINS $q) ORDER BY sent_at DESC")
            .bind(("q", q))
            .await?;
        let msgs: Vec<Message> = result.take(0)?;
        Ok(msgs)
    }

    // -- Conversations ---

    async fn create_conversation(&self, conversation: Conversation) -> DbResult<Conversation> {
        let created: Option<Conversation> = self.db.create("conversation").content(conversation).await?;
        created.ok_or_else(|| DbError::Query("Failed to create conversation".into()))
    }

    async fn get_conversation(&self, id: &str) -> DbResult<Conversation> {
        let (table, key) = parse_thing(id)?;
        if table != "conversation" {
            return Err(DbError::InvalidId(format!("Expected conversation ID, got table: {table}")));
        }
        let conv: Option<Conversation> = self.db.select((table, key)).await?;
        conv.ok_or_else(|| DbError::NotFound(id.to_string()))
    }

    async fn list_conversations(
        &self,
        channel: Option<&ChannelType>,
    ) -> DbResult<Vec<Conversation>> {
        match channel {
            Some(ch) => {
                let ch_str = serde_json::to_string(ch)
                    .map_err(|e| DbError::Serialization(e.to_string()))?;
                // Remove quotes around the serialized string for SurrealDB comparison
                let ch_val = ch_str.trim_matches('"').to_string();
                let mut result = self
                    .db
                    .query("SELECT * FROM conversation WHERE deleted_at IS NONE AND channel = $ch ORDER BY last_message_at DESC")
                    .bind(("ch", ch_val))
                    .await?;
                let convs: Vec<Conversation> = result.take(0)?;
                Ok(convs)
            }
            None => {
                let mut result = self
                    .db
                    .query("SELECT * FROM conversation WHERE deleted_at IS NONE ORDER BY last_message_at DESC")
                    .await?;
                let convs: Vec<Conversation> = result.take(0)?;
                Ok(convs)
            }
        }
    }

    async fn update_conversation_unread(
        &self,
        id: &str,
        unread_count: u32,
    ) -> DbResult<Conversation> {
        let (table, key) = parse_thing(id)?;
        if table != "conversation" {
            return Err(DbError::InvalidId(format!("Expected conversation ID, got table: {table}")));
        }
        let current: Option<Conversation> = self.db.select((table, key)).await?;
        let mut conv = current.ok_or_else(|| DbError::NotFound(id.to_string()))?;
        conv.unread_count = unread_count;

        let updated: Option<Conversation> = self.db.update((table, key)).content(conv).await?;
        updated.ok_or_else(|| DbError::Query("Failed to update conversation unread count".into()))
    }

    async fn delete_conversation(&self, id: &str) -> DbResult<()> {
        let (table, key) = parse_thing(id)?;
        if table != "conversation" {
            return Err(DbError::InvalidId(format!("Expected conversation ID, got table: {table}")));
        }
        let _: Option<Conversation> = self.db.delete((table, key)).await?;
        Ok(())
    }

    async fn link_conversation_to_thread(
        &self,
        conversation_id: &str,
        thread_id: &str,
    ) -> DbResult<Conversation> {
        let (table, key) = parse_thing(conversation_id)?;
        if table != "conversation" {
            return Err(DbError::InvalidId(format!("Expected conversation ID, got table: {table}")));
        }
        let current: Option<Conversation> = self.db.select((table, key)).await?;
        let mut conv = current.ok_or_else(|| DbError::NotFound(conversation_id.to_string()))?;
        conv.linked_thread_id = Some(thread_id.to_string());

        let updated: Option<Conversation> = self.db.update((table, key)).content(conv).await?;
        updated.ok_or_else(|| DbError::Query("Failed to link conversation to thread".into()))
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
            "thread:test".into(),
            true,
        );
        let created = db.create_document(doc).await.unwrap();
        assert!(created.id.is_some());
        assert_eq!(created.title, "Test Doc");

        let id = created.id_string().unwrap();
        let fetched = db.get_document(&id).await.unwrap();
        assert_eq!(fetched.title, "Test Doc");
        assert!(fetched.is_owned);
    }

    #[tokio::test]
    async fn test_list_documents_all() {
        let db = setup_db().await;
        for i in 0..3 {
            let doc = Document::new(
                format!("Doc {i}"),
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
        let doc_a = Document::new("A".into(), "thread:alpha".into(), true);
        let doc_b = Document::new("B".into(), "thread:beta".into(), true);
        db.create_document(doc_a).await.unwrap();
        db.create_document(doc_b).await.unwrap();

        let alpha_docs = db.list_documents(Some("thread:alpha")).await.unwrap();
        assert_eq!(alpha_docs.len(), 1);
        assert_eq!(alpha_docs[0].title, "A");
    }

    #[tokio::test]
    async fn test_update_document() {
        let db = setup_db().await;
        let doc = Document::new("Original".into(), "thread:t".into(), true);
        let created = db.create_document(doc).await.unwrap();
        let id = created.id_string().unwrap();

        let updated = db
            .update_document(&id, Some("Updated Title"), Some("New content"))
            .await
            .unwrap();
        assert_eq!(updated.title, "Updated Title");
        assert_eq!(updated.content, "New content");
    }

    #[tokio::test]
    async fn test_delete_document() {
        let db = setup_db().await;
        let doc = Document::new("ToDelete".into(), "thread:t".into(), true);
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
        let doc1 = Document::new("Doc1".into(), "thread:t".into(), true);
        let doc2 = Document::new("Doc2".into(), "thread:t".into(), true);
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
    async fn test_commit_document() {
        let db = setup_db().await;
        let doc = Document::new("Snap".into(), "thread:t".into(), true);
        let created = db.create_document(doc).await.unwrap();
        let doc_id = created.id_string().unwrap();

        let commit = db.commit_document(&doc_id, "Initial commit").await.unwrap();
        assert!(commit.id.is_some());
        assert_eq!(commit.message, "Initial commit");
        assert_eq!(commit.document_id, doc_id);
        assert_eq!(commit.snapshot.title, "Snap");
        assert!(commit.parent_commit.is_none());

        // head_commit should be updated
        let fetched = db.get_document(&doc_id).await.unwrap();
        assert_eq!(fetched.head_commit, commit.id_string());
    }

    #[tokio::test]
    async fn test_commit_document_creates_chain() {
        let db = setup_db().await;
        let doc = Document::new("Chain".into(), "thread:t".into(), true);
        let created = db.create_document(doc).await.unwrap();
        let doc_id = created.id_string().unwrap();

        let c1 = db.commit_document(&doc_id, "First").await.unwrap();
        let c1_id = c1.id_string().unwrap();
        assert!(c1.parent_commit.is_none());

        let c2 = db.commit_document(&doc_id, "Second").await.unwrap();
        assert_eq!(c2.parent_commit, Some(c1_id));
    }

    #[tokio::test]
    async fn test_list_document_commits() {
        let db = setup_db().await;
        let doc = Document::new("D".into(), "thread:t".into(), true);
        let created = db.create_document(doc).await.unwrap();
        let doc_id = created.id_string().unwrap();

        db.commit_document(&doc_id, "First").await.unwrap();
        db.commit_document(&doc_id, "Second").await.unwrap();

        let commits = db.list_document_commits(&doc_id).await.unwrap();
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].message, "Second");
    }

    #[tokio::test]
    async fn test_get_commit() {
        let db = setup_db().await;
        let doc = Document::new("G".into(), "thread:t".into(), true);
        let created = db.create_document(doc).await.unwrap();
        let doc_id = created.id_string().unwrap();

        let commit = db.commit_document(&doc_id, "Snapshot").await.unwrap();
        let commit_id = commit.id_string().unwrap();

        let fetched = db.get_commit(&commit_id).await.unwrap();
        assert_eq!(fetched.message, "Snapshot");
        assert_eq!(fetched.snapshot.title, "G");
    }

    #[tokio::test]
    async fn test_restore_document() {
        let db = setup_db().await;
        let doc = Document::new("Original".into(), "thread:t".into(), true);
        let created = db.create_document(doc).await.unwrap();
        let doc_id = created.id_string().unwrap();

        // Commit v1
        let c1 = db.commit_document(&doc_id, "v1").await.unwrap();
        let c1_id = c1.id_string().unwrap();

        // Modify document
        db.update_document(&doc_id, Some("Modified"), None).await.unwrap();
        db.commit_document(&doc_id, "v2").await.unwrap();

        // Restore to v1
        let restored = db.restore_document(&doc_id, &c1_id).await.unwrap();
        assert_eq!(restored.title, "Original");

        // Should have created a restore commit
        let commits = db.list_document_commits(&doc_id).await.unwrap();
        assert!(commits[0].message.contains("Restored from"));
    }

    #[tokio::test]
    async fn test_update_thread() {
        let db = setup_db().await;
        let t = Thread::new("Original".into(), "desc".into());
        let created = db.create_thread(t).await.unwrap();
        let id = created.id_string().unwrap();

        let updated = db
            .update_thread(&id, Some("Renamed"), None)
            .await
            .unwrap();
        assert_eq!(updated.name, "Renamed");
        assert_eq!(updated.description, "desc");
    }

    #[tokio::test]
    async fn test_delete_thread() {
        let db = setup_db().await;
        let t = Thread::new("ToDelete".into(), "d".into());
        let created = db.create_thread(t).await.unwrap();
        let id = created.id_string().unwrap();

        db.delete_thread(&id).await.unwrap();

        let result = db.get_thread(&id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_move_document_to_thread() {
        let db = setup_db().await;
        let t1 = Thread::new("Thread A".into(), "".into());
        let t2 = Thread::new("Thread B".into(), "".into());
        let created_t1 = db.create_thread(t1).await.unwrap();
        let created_t2 = db.create_thread(t2).await.unwrap();
        let tid1 = created_t1.id_string().unwrap();
        let tid2 = created_t2.id_string().unwrap();

        let doc = Document::new("Movable".into(), tid1.clone(), true);
        let created = db.create_document(doc).await.unwrap();
        let doc_id = created.id_string().unwrap();
        assert_eq!(created.thread_id, tid1);

        let moved = db.move_document_to_thread(&doc_id, &tid2).await.unwrap();
        assert_eq!(moved.thread_id, tid2);

        let fetched = db.get_document(&doc_id).await.unwrap();
        assert_eq!(fetched.thread_id, tid2);
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

    // -- Soft delete tests ---

    #[tokio::test]
    async fn test_soft_delete_document() {
        let db = setup_db().await;
        let doc = Document::new("SoftDel".into(), "thread:t".into(), true);
        let created = db.create_document(doc).await.unwrap();
        let id = created.id_string().unwrap();

        db.soft_delete_document(&id).await.unwrap();

        // Document should not appear in list
        let docs = db.list_documents(None).await.unwrap();
        assert!(docs.iter().all(|d| d.id_string().as_deref() != Some(id.as_str())));

        // But get_document still works (it doesn't filter)
        let fetched = db.get_document(&id).await.unwrap();
        assert!(fetched.deleted_at.is_some());
    }

    #[tokio::test]
    async fn test_restore_soft_deleted_document() {
        let db = setup_db().await;
        let doc = Document::new("Restore".into(), "thread:t".into(), true);
        let created = db.create_document(doc).await.unwrap();
        let id = created.id_string().unwrap();

        db.soft_delete_document(&id).await.unwrap();
        let restored = db.restore_soft_deleted_document(&id).await.unwrap();
        assert!(restored.deleted_at.is_none());

        // Should appear in list again
        let docs = db.list_documents(None).await.unwrap();
        assert!(docs.iter().any(|d| d.id_string().as_deref() == Some(id.as_str())));
    }

    #[tokio::test]
    async fn test_soft_delete_thread() {
        let db = setup_db().await;
        let t = Thread::new("SoftDelThread".into(), "d".into());
        let created = db.create_thread(t).await.unwrap();
        let id = created.id_string().unwrap();

        db.soft_delete_thread(&id).await.unwrap();

        let threads = db.list_threads().await.unwrap();
        assert!(threads.iter().all(|t| t.id_string().as_deref() != Some(id.as_str())));
    }

    #[tokio::test]
    async fn test_restore_soft_deleted_thread() {
        let db = setup_db().await;
        let t = Thread::new("RestoreThread".into(), "d".into());
        let created = db.create_thread(t).await.unwrap();
        let id = created.id_string().unwrap();

        db.soft_delete_thread(&id).await.unwrap();
        let restored = db.restore_soft_deleted_thread(&id).await.unwrap();
        assert!(restored.deleted_at.is_none());

        let threads = db.list_threads().await.unwrap();
        assert!(threads.iter().any(|t| t.id_string().as_deref() == Some(id.as_str())));
    }

    #[tokio::test]
    async fn test_merge_threads() {
        let db = setup_db().await;
        let t1 = Thread::new("Target".into(), "".into());
        let t2 = Thread::new("Source".into(), "".into());
        let ct1 = db.create_thread(t1).await.unwrap();
        let ct2 = db.create_thread(t2).await.unwrap();
        let tid1 = ct1.id_string().unwrap();
        let tid2 = ct2.id_string().unwrap();

        let d1 = Document::new("DocA".into(), tid1.clone(), true);
        let d2 = Document::new("DocB".into(), tid2.clone(), true);
        db.create_document(d1).await.unwrap();
        db.create_document(d2).await.unwrap();

        db.merge_threads(&tid1, &tid2).await.unwrap();

        // All docs should be in target thread
        let docs = db.list_documents(Some(&tid1)).await.unwrap();
        assert_eq!(docs.len(), 2);

        // Source thread should be soft-deleted
        let threads = db.list_threads().await.unwrap();
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].name, "Target");
    }

    #[tokio::test]
    async fn test_split_thread() {
        let db = setup_db().await;
        let t = Thread::new("Original".into(), "".into());
        let ct = db.create_thread(t).await.unwrap();
        let tid = ct.id_string().unwrap();

        let d1 = Document::new("Keep".into(), tid.clone(), true);
        let d2 = Document::new("Split".into(), tid.clone(), true);
        let cd1 = db.create_document(d1).await.unwrap();
        let cd2 = db.create_document(d2).await.unwrap();
        let did2 = cd2.id_string().unwrap();
        let _did1 = cd1.id_string().unwrap();

        let new_thread = db
            .split_thread(&tid, &[did2.clone()], "New Thread")
            .await
            .unwrap();
        let new_tid = new_thread.id_string().unwrap();
        assert_eq!(new_thread.name, "New Thread");

        // Original thread should have 1 doc
        let orig_docs = db.list_documents(Some(&tid)).await.unwrap();
        assert_eq!(orig_docs.len(), 1);
        assert_eq!(orig_docs[0].title, "Keep");

        // New thread should have 1 doc
        let new_docs = db.list_documents(Some(&new_tid)).await.unwrap();
        assert_eq!(new_docs.len(), 1);
        assert_eq!(new_docs[0].title, "Split");
    }

    #[tokio::test]
    async fn test_list_documents_excludes_soft_deleted() {
        let db = setup_db().await;
        let doc1 = Document::new("Active".into(), "thread:t".into(), true);
        let doc2 = Document::new("Deleted".into(), "thread:t".into(), true);
        db.create_document(doc1).await.unwrap();
        let created2 = db.create_document(doc2).await.unwrap();
        let id2 = created2.id_string().unwrap();

        db.soft_delete_document(&id2).await.unwrap();

        let docs = db.list_documents(None).await.unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].title, "Active");
    }

    #[tokio::test]
    async fn test_list_documents_by_thread_excludes_soft_deleted() {
        let db = setup_db().await;
        let doc1 = Document::new("A".into(), "thread:alpha".into(), true);
        let doc2 = Document::new("B".into(), "thread:alpha".into(), true);
        db.create_document(doc1).await.unwrap();
        let created2 = db.create_document(doc2).await.unwrap();
        let id2 = created2.id_string().unwrap();

        db.soft_delete_document(&id2).await.unwrap();

        let docs = db.list_documents(Some("thread:alpha")).await.unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].title, "A");
    }

    #[tokio::test]
    async fn test_create_and_list_milestones() {
        let db = setup_db().await;
        let t = Thread::new("Research".into(), "".into());
        let ct = db.create_thread(t).await.unwrap();
        let tid = ct.id_string().unwrap();

        let m1 = Milestone::new("Alpha release".into(), tid.clone(), "First release".into());
        let m2 = Milestone::new("Beta release".into(), tid.clone(), "Second release".into());
        let cm1 = db.create_milestone(m1).await.unwrap();
        db.create_milestone(m2).await.unwrap();

        assert!(cm1.id.is_some());
        assert_eq!(cm1.title, "Alpha release");

        let milestones = db.list_milestones(&tid).await.unwrap();
        assert_eq!(milestones.len(), 2);
    }

    #[tokio::test]
    async fn test_delete_milestone() {
        let db = setup_db().await;
        let t = Thread::new("Dev".into(), "".into());
        let ct = db.create_thread(t).await.unwrap();
        let tid = ct.id_string().unwrap();

        let m = Milestone::new("v1.0".into(), tid.clone(), "".into());
        let created = db.create_milestone(m).await.unwrap();
        let mid = created.id_string().unwrap();

        db.delete_milestone(&mid).await.unwrap();

        let milestones = db.list_milestones(&tid).await.unwrap();
        assert!(milestones.is_empty());
    }

    #[tokio::test]
    async fn test_purge_deleted() {
        let db = setup_db().await;
        let doc = Document::new("PurgeMe".into(), "thread:t".into(), true);
        let created = db.create_document(doc).await.unwrap();
        let id = created.id_string().unwrap();

        // Soft delete with a past timestamp to simulate expiry
        let (table, key) = id.split_once(':').unwrap();
        let mut fetched: Document = db.get_document(&id).await.unwrap();
        let old_time = chrono::Utc::now() - chrono::Duration::days(31);
        fetched.deleted_at = Some(old_time.to_rfc3339());
        let _: Option<Document> = db.db.update((table, key)).content(fetched).await.unwrap();

        // Purge with 30-day max age
        let _ = db
            .purge_deleted(std::time::Duration::from_secs(30 * 24 * 3600))
            .await
            .unwrap();

        // Document should be gone
        let result = db.get_document(&id).await;
        assert!(result.is_err());
    }

    // -- Contact tests ---

    #[tokio::test]
    async fn test_create_and_get_contact() {
        let db = setup_db().await;
        let contact = Contact::new("Alice".into(), false);
        let created = db.create_contact(contact).await.unwrap();
        assert!(created.id.is_some());
        assert_eq!(created.name, "Alice");

        let id = created.id_string().unwrap();
        let fetched = db.get_contact(&id).await.unwrap();
        assert_eq!(fetched.name, "Alice");
        assert!(!fetched.is_owned);
    }

    #[tokio::test]
    async fn test_list_contacts() {
        let db = setup_db().await;
        db.create_contact(Contact::new("Alice".into(), false)).await.unwrap();
        db.create_contact(Contact::new("Bob".into(), false)).await.unwrap();

        let contacts = db.list_contacts().await.unwrap();
        assert_eq!(contacts.len(), 2);
    }

    #[tokio::test]
    async fn test_update_contact() {
        let db = setup_db().await;
        let created = db.create_contact(Contact::new("Alice".into(), false)).await.unwrap();
        let id = created.id_string().unwrap();

        let updated = db.update_contact(&id, Some("Alicia"), Some("A friend"), None).await.unwrap();
        assert_eq!(updated.name, "Alicia");
        assert_eq!(updated.notes, "A friend");
    }

    #[tokio::test]
    async fn test_soft_delete_contact() {
        let db = setup_db().await;
        let created = db.create_contact(Contact::new("ToDelete".into(), false)).await.unwrap();
        let id = created.id_string().unwrap();

        db.soft_delete_contact(&id).await.unwrap();

        let contacts = db.list_contacts().await.unwrap();
        assert!(contacts.is_empty());

        // Still fetchable directly
        let fetched = db.get_contact(&id).await.unwrap();
        assert!(fetched.deleted_at.is_some());
    }

    #[tokio::test]
    async fn test_add_contact_address_and_find() {
        use crate::schema::{ChannelAddress, ChannelType};
        let db = setup_db().await;
        let created = db.create_contact(Contact::new("Alice".into(), false)).await.unwrap();
        let id = created.id_string().unwrap();

        let addr = ChannelAddress {
            channel: ChannelType::Email,
            address: "alice@example.com".into(),
            display_name: Some("Alice".into()),
            is_primary: true,
        };
        let updated = db.add_contact_address(&id, addr).await.unwrap();
        assert_eq!(updated.addresses.len(), 1);
        assert_eq!(updated.addresses[0].address, "alice@example.com");

        let found = db.find_contact_by_address("alice@example.com").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Alice");

        let not_found = db.find_contact_by_address("nobody@example.com").await.unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_delete_contact() {
        let db = setup_db().await;
        let created = db.create_contact(Contact::new("Delete".into(), false)).await.unwrap();
        let id = created.id_string().unwrap();

        db.delete_contact(&id).await.unwrap();
        assert!(db.get_contact(&id).await.is_err());
    }

    // -- Message tests ---

    #[tokio::test]
    async fn test_create_and_get_message() {
        use crate::schema::{ChannelType, MessageDirection};
        let db = setup_db().await;
        let msg = Message::new(
            "conversation:1".into(),
            ChannelType::Email,
            MessageDirection::Inbound,
            "contact:alice".into(),
            vec!["contact:me".into()],
            "Hello!".into(),
        );
        let created = db.create_message(msg).await.unwrap();
        assert!(created.id.is_some());
        assert_eq!(created.body, "Hello!");

        let id = created.id_string().unwrap();
        let fetched = db.get_message(&id).await.unwrap();
        assert_eq!(fetched.body, "Hello!");
        assert_eq!(fetched.read_status, ReadStatus::Unread);
    }

    #[tokio::test]
    async fn test_list_messages_pagination() {
        use crate::schema::{ChannelType, MessageDirection};
        let db = setup_db().await;

        for i in 0..5 {
            let msg = Message::new(
                "conversation:1".into(),
                ChannelType::Email,
                MessageDirection::Inbound,
                "contact:alice".into(),
                vec!["contact:me".into()],
                format!("Message {i}"),
            );
            db.create_message(msg).await.unwrap();
        }

        // List with limit
        let msgs = db.list_messages("conversation:1", None, 3).await.unwrap();
        assert_eq!(msgs.len(), 3);

        // List all
        let all = db.list_messages("conversation:1", None, 100).await.unwrap();
        assert_eq!(all.len(), 5);
    }

    #[tokio::test]
    async fn test_update_message_read_status() {
        use crate::schema::{ChannelType, MessageDirection};
        let db = setup_db().await;
        let msg = Message::new(
            "conversation:1".into(),
            ChannelType::Email,
            MessageDirection::Inbound,
            "contact:alice".into(),
            vec!["contact:me".into()],
            "Read me".into(),
        );
        let created = db.create_message(msg).await.unwrap();
        let id = created.id_string().unwrap();
        assert_eq!(created.read_status, ReadStatus::Unread);

        let updated = db.update_message_read_status(&id, ReadStatus::Read).await.unwrap();
        assert_eq!(updated.read_status, ReadStatus::Read);
    }

    #[tokio::test]
    async fn test_search_messages() {
        use crate::schema::{ChannelType, MessageDirection};
        let db = setup_db().await;

        let msg1 = Message::new(
            "conversation:1".into(),
            ChannelType::Email,
            MessageDirection::Inbound,
            "contact:alice".into(),
            vec!["contact:me".into()],
            "Meeting tomorrow at 3pm".into(),
        );
        let msg2 = Message::new(
            "conversation:1".into(),
            ChannelType::Email,
            MessageDirection::Outbound,
            "contact:me".into(),
            vec!["contact:alice".into()],
            "Sounds good!".into(),
        );
        db.create_message(msg1).await.unwrap();
        db.create_message(msg2).await.unwrap();

        let found = db.search_messages("Meeting").await.unwrap();
        assert_eq!(found.len(), 1);
        assert!(found[0].body.contains("Meeting"));

        let none = db.search_messages("nonexistent").await.unwrap();
        assert!(none.is_empty());
    }

    #[tokio::test]
    async fn test_delete_message() {
        use crate::schema::{ChannelType, MessageDirection};
        let db = setup_db().await;
        let msg = Message::new(
            "conversation:1".into(),
            ChannelType::Email,
            MessageDirection::Inbound,
            "contact:alice".into(),
            vec!["contact:me".into()],
            "Delete me".into(),
        );
        let created = db.create_message(msg).await.unwrap();
        let id = created.id_string().unwrap();

        db.delete_message(&id).await.unwrap();
        assert!(db.get_message(&id).await.is_err());
    }

    // -- Conversation tests ---

    #[tokio::test]
    async fn test_create_and_get_conversation() {
        use crate::schema::ChannelType;
        let db = setup_db().await;
        let conv = Conversation::new(
            "Chat with Alice".into(),
            ChannelType::Email,
            vec!["contact:alice".into(), "contact:me".into()],
        );
        let created = db.create_conversation(conv).await.unwrap();
        assert!(created.id.is_some());
        assert_eq!(created.title, "Chat with Alice");

        let id = created.id_string().unwrap();
        let fetched = db.get_conversation(&id).await.unwrap();
        assert_eq!(fetched.title, "Chat with Alice");
        assert_eq!(fetched.participant_contact_ids.len(), 2);
    }

    #[tokio::test]
    async fn test_list_conversations() {
        use crate::schema::ChannelType;
        let db = setup_db().await;
        db.create_conversation(Conversation::new(
            "Email chat".into(),
            ChannelType::Email,
            vec!["contact:alice".into()],
        )).await.unwrap();
        db.create_conversation(Conversation::new(
            "SMS chat".into(),
            ChannelType::Sms,
            vec!["contact:bob".into()],
        )).await.unwrap();

        let all = db.list_conversations(None).await.unwrap();
        assert_eq!(all.len(), 2);

        let email_only = db.list_conversations(Some(&ChannelType::Email)).await.unwrap();
        assert_eq!(email_only.len(), 1);
        assert_eq!(email_only[0].title, "Email chat");
    }

    #[tokio::test]
    async fn test_update_conversation_unread() {
        use crate::schema::ChannelType;
        let db = setup_db().await;
        let conv = Conversation::new(
            "Test".into(),
            ChannelType::Email,
            vec!["contact:alice".into()],
        );
        let created = db.create_conversation(conv).await.unwrap();
        let id = created.id_string().unwrap();
        assert_eq!(created.unread_count, 0);

        let updated = db.update_conversation_unread(&id, 5).await.unwrap();
        assert_eq!(updated.unread_count, 5);
    }

    #[tokio::test]
    async fn test_link_conversation_to_thread() {
        use crate::schema::ChannelType;
        let db = setup_db().await;
        let thread = db.create_thread(Thread::new("Linked".into(), "".into())).await.unwrap();
        let tid = thread.id_string().unwrap();

        let conv = Conversation::new(
            "Linkable".into(),
            ChannelType::Email,
            vec!["contact:alice".into()],
        );
        let created = db.create_conversation(conv).await.unwrap();
        let cid = created.id_string().unwrap();
        assert!(created.linked_thread_id.is_none());

        let linked = db.link_conversation_to_thread(&cid, &tid).await.unwrap();
        assert_eq!(linked.linked_thread_id.as_deref(), Some(tid.as_str()));
    }

    #[tokio::test]
    async fn test_delete_conversation() {
        use crate::schema::ChannelType;
        let db = setup_db().await;
        let conv = Conversation::new(
            "Delete me".into(),
            ChannelType::Email,
            vec!["contact:alice".into()],
        );
        let created = db.create_conversation(conv).await.unwrap();
        let id = created.id_string().unwrap();

        db.delete_conversation(&id).await.unwrap();
        assert!(db.get_conversation(&id).await.is_err());
    }

    #[tokio::test]
    async fn test_relation_type_contactof_attachedto() {
        let rt1 = RelationType::ContactOf;
        assert_eq!(rt1.to_string(), "contactof");
        let parsed: RelationType = "contactof".parse().unwrap();
        assert_eq!(parsed, RelationType::ContactOf);

        let rt2 = RelationType::AttachedTo;
        assert_eq!(rt2.to_string(), "attachedto");
        let parsed2: RelationType = "attached_to".parse().unwrap();
        assert_eq!(parsed2, RelationType::AttachedTo);
    }
}
