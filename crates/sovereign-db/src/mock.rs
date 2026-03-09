//! In-memory mock implementation of GraphDB for testing.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use surrealdb::sql::Thing;

use crate::error::{DbError, DbResult};
use crate::schema::*;
use crate::traits::GraphDB;

/// In-memory GraphDB implementation for unit testing.
pub struct MockGraphDB {
    documents: RwLock<HashMap<String, Document>>,
    threads: RwLock<HashMap<String, Thread>>,
    contacts: RwLock<HashMap<String, Contact>>,
    messages: RwLock<HashMap<String, Message>>,
    conversations: RwLock<HashMap<String, Conversation>>,
    commits: RwLock<HashMap<String, Vec<Commit>>>,
    relationships: RwLock<Vec<RelatedTo>>,
    suggested_links: RwLock<Vec<SuggestedLink>>,
    next_id: AtomicU64,
}

impl MockGraphDB {
    pub fn new() -> Self {
        Self {
            documents: RwLock::new(HashMap::new()),
            threads: RwLock::new(HashMap::new()),
            contacts: RwLock::new(HashMap::new()),
            messages: RwLock::new(HashMap::new()),
            conversations: RwLock::new(HashMap::new()),
            commits: RwLock::new(HashMap::new()),
            relationships: RwLock::new(Vec::new()),
            suggested_links: RwLock::new(Vec::new()),
            next_id: AtomicU64::new(1),
        }
    }

    fn next_key(&self) -> String {
        self.next_id.fetch_add(1, Ordering::Relaxed).to_string()
    }

    fn make_thing(table: &str, key: &str) -> Thing {
        Thing::from((table.to_string(), key.to_string()))
    }
}

#[async_trait]
impl GraphDB for MockGraphDB {
    async fn connect(&self) -> DbResult<()> { Ok(()) }
    async fn init_schema(&self) -> DbResult<()> { Ok(()) }

    async fn create_document(&self, mut doc: Document) -> DbResult<Document> {
        let key = self.next_key();
        let thing = Self::make_thing("document", &key);
        let id_str = thing_to_raw(&thing);
        doc.id = Some(thing);
        self.documents.write().unwrap().insert(id_str, doc.clone());
        Ok(doc)
    }

    async fn get_document(&self, id: &str) -> DbResult<Document> {
        self.documents.read().unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| DbError::NotFound(id.to_string()))
    }

    async fn list_documents(&self, thread_id: Option<&str>) -> DbResult<Vec<Document>> {
        let docs = self.documents.read().unwrap();
        let mut result: Vec<Document> = docs.values()
            .filter(|d| d.deleted_at.is_none())
            .filter(|d| thread_id.map_or(true, |tid| d.thread_id == tid))
            .cloned()
            .collect();
        result.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(result)
    }

    async fn update_document(&self, id: &str, title: Option<&str>, content: Option<&str>) -> DbResult<Document> {
        let mut docs = self.documents.write().unwrap();
        let doc = docs.get_mut(id).ok_or_else(|| DbError::NotFound(id.to_string()))?;
        if let Some(t) = title { doc.title = t.to_string(); }
        if let Some(c) = content { doc.content = c.to_string(); }
        doc.modified_at = Utc::now();
        Ok(doc.clone())
    }

    async fn update_document_reliability(
        &self,
        id: &str,
        source_url: Option<&str>,
        classification: Option<&str>,
        score: Option<f32>,
        assessment_json: Option<&str>,
    ) -> DbResult<Document> {
        let mut docs = self.documents.write().unwrap();
        let doc = docs.get_mut(id).ok_or_else(|| DbError::NotFound(id.to_string()))?;
        if let Some(u) = source_url { doc.source_url = Some(u.to_string()); }
        if let Some(c) = classification { doc.reliability_classification = Some(c.to_string()); }
        if let Some(s) = score { doc.reliability_score = Some(s); }
        if let Some(a) = assessment_json { doc.reliability_assessment = Some(a.to_string()); }
        if classification.is_some() || score.is_some() {
            doc.assessed_at = Some(Utc::now());
        }
        Ok(doc.clone())
    }

    async fn update_document_position(&self, id: &str, x: f32, y: f32) -> DbResult<()> {
        let mut docs = self.documents.write().unwrap();
        let doc = docs.get_mut(id).ok_or_else(|| DbError::NotFound(id.to_string()))?;
        doc.spatial_x = x;
        doc.spatial_y = y;
        Ok(())
    }

    async fn delete_document(&self, id: &str) -> DbResult<()> {
        self.documents.write().unwrap().remove(id);
        Ok(())
    }

    async fn search_documents_by_title(&self, query: &str) -> DbResult<Vec<Document>> {
        let q = query.to_lowercase();
        let docs = self.documents.read().unwrap();
        let mut result: Vec<Document> = docs.values()
            .filter(|d| d.deleted_at.is_none())
            .filter(|d| d.title.to_lowercase().contains(&q))
            .cloned()
            .collect();
        result.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        if result.len() > 20 { result.truncate(20); }
        Ok(result)
    }

    async fn create_thread(&self, mut thread: Thread) -> DbResult<Thread> {
        let key = self.next_key();
        let thing = Self::make_thing("thread", &key);
        let id_str = thing_to_raw(&thing);
        thread.id = Some(thing);
        self.threads.write().unwrap().insert(id_str, thread.clone());
        Ok(thread)
    }

    async fn get_thread(&self, id: &str) -> DbResult<Thread> {
        self.threads.read().unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| DbError::NotFound(id.to_string()))
    }

    async fn list_threads(&self) -> DbResult<Vec<Thread>> {
        let threads = self.threads.read().unwrap();
        Ok(threads.values()
            .filter(|t| t.deleted_at.is_none())
            .cloned()
            .collect())
    }

    async fn update_thread(&self, id: &str, name: Option<&str>, description: Option<&str>) -> DbResult<Thread> {
        let mut threads = self.threads.write().unwrap();
        let thread = threads.get_mut(id).ok_or_else(|| DbError::NotFound(id.to_string()))?;
        if let Some(n) = name { thread.name = n.to_string(); }
        if let Some(d) = description { thread.description = d.to_string(); }
        Ok(thread.clone())
    }

    async fn delete_thread(&self, id: &str) -> DbResult<()> {
        self.threads.write().unwrap().remove(id);
        Ok(())
    }

    async fn find_thread_by_name(&self, name: &str) -> DbResult<Option<Thread>> {
        let n = name.to_lowercase();
        let threads = self.threads.read().unwrap();
        Ok(threads.values()
            .find(|t| t.deleted_at.is_none() && t.name.to_lowercase().contains(&n))
            .cloned())
    }

    async fn move_document_to_thread(&self, doc_id: &str, new_thread_id: &str) -> DbResult<Document> {
        let mut docs = self.documents.write().unwrap();
        let doc = docs.get_mut(doc_id).ok_or_else(|| DbError::NotFound(doc_id.to_string()))?;
        doc.thread_id = new_thread_id.to_string();
        doc.modified_at = Utc::now();
        Ok(doc.clone())
    }

    async fn create_relationship(&self, _from_id: &str, _to_id: &str, _relation_type: RelationType, _strength: f32) -> DbResult<RelatedTo> {
        let key = self.next_key();
        let rel = RelatedTo {
            id: Some(Self::make_thing("related_to", &key)),
            in_: None,
            out: None,
            relation_type: _relation_type,
            strength: _strength,
            created_at: Utc::now(),
        };
        self.relationships.write().unwrap().push(rel.clone());
        Ok(rel)
    }

    async fn list_relationships(&self, _doc_id: &str) -> DbResult<Vec<RelatedTo>> { Ok(vec![]) }
    async fn list_all_relationships(&self) -> DbResult<Vec<RelatedTo>> {
        Ok(self.relationships.read().unwrap().clone())
    }
    async fn traverse(&self, _doc_id: &str, _depth: u32, _limit: u32) -> DbResult<Vec<Document>> { Ok(vec![]) }

    // -- Suggested Links ---

    async fn create_suggested_link(
        &self,
        from_id: &str,
        to_id: &str,
        relation_type: RelationType,
        strength: f32,
        rationale: &str,
        source: SuggestionSource,
    ) -> DbResult<SuggestedLink> {
        let key = self.next_key();
        let link = SuggestedLink {
            id: Some(Self::make_thing("suggested_link", &key)),
            in_: Some(Self::make_thing("document", to_id.split(':').last().unwrap_or(to_id))),
            out: Some(Self::make_thing("document", from_id.split(':').last().unwrap_or(from_id))),
            relation_type,
            strength,
            rationale: rationale.to_string(),
            source,
            status: SuggestionStatus::Pending,
            created_at: Utc::now(),
            resolved_at: None,
        };
        self.suggested_links.write().unwrap().push(link.clone());
        Ok(link)
    }

    async fn list_pending_suggestions(&self) -> DbResult<Vec<SuggestedLink>> {
        let links = self.suggested_links.read().unwrap();
        Ok(links.iter().filter(|l| l.status == SuggestionStatus::Pending).cloned().collect())
    }

    async fn list_suggestions_for_document(&self, doc_id: &str) -> DbResult<Vec<SuggestedLink>> {
        let links = self.suggested_links.read().unwrap();
        let id_part = doc_id.split(':').last().unwrap_or(doc_id);
        Ok(links
            .iter()
            .filter(|l| {
                let in_match = l.in_.as_ref().map(|t| t.id.to_raw() == id_part).unwrap_or(false);
                let out_match = l.out.as_ref().map(|t| t.id.to_raw() == id_part).unwrap_or(false);
                in_match || out_match
            })
            .cloned()
            .collect())
    }

    async fn resolve_suggestion(
        &self,
        id: &str,
        status: SuggestionStatus,
    ) -> DbResult<SuggestedLink> {
        let result = {
            let mut links = self.suggested_links.write().unwrap();
            let link = links
                .iter_mut()
                .find(|l| l.id.as_ref().map(|t| t.id.to_raw() == id).unwrap_or(false))
                .ok_or_else(|| DbError::NotFound(id.to_string()))?;

            link.status = status.clone();
            link.resolved_at = Some(Utc::now());
            link.clone()
        }; // write lock dropped here

        // If accepted, create a real relationship
        if status == SuggestionStatus::Accepted {
            if let (Some(in_thing), Some(out_thing)) = (&result.in_, &result.out) {
                let from_str = crate::schema::thing_to_raw(out_thing);
                let to_str = crate::schema::thing_to_raw(in_thing);
                self.create_relationship(&from_str, &to_str, result.relation_type.clone(), result.strength).await?;
            }
        }
        Ok(result)
    }

    async fn suggestion_exists(&self, from_id: &str, to_id: &str) -> DbResult<bool> {
        let links = self.suggested_links.read().unwrap();
        let from_part = from_id.split(':').last().unwrap_or(from_id);
        let to_part = to_id.split(':').last().unwrap_or(to_id);
        let exists = links.iter().any(|l| {
            let in_id = l.in_.as_ref().map(|t| t.id.to_raw()).unwrap_or_default();
            let out_id = l.out.as_ref().map(|t| t.id.to_raw()).unwrap_or_default();
            (in_id == to_part && out_id == from_part) || (in_id == from_part && out_id == to_part)
        });
        Ok(exists)
    }

    async fn adopt_document(&self, id: &str) -> DbResult<Document> {
        let mut docs = self.documents.write().unwrap();
        let doc = docs.get_mut(id).ok_or_else(|| DbError::NotFound(id.to_string()))?;
        doc.is_owned = true;
        Ok(doc.clone())
    }

    async fn merge_threads(&self, target_id: &str, source_id: &str) -> DbResult<()> {
        {
            let mut docs = self.documents.write().unwrap();
            for doc in docs.values_mut() {
                if doc.thread_id == source_id {
                    doc.thread_id = target_id.to_string();
                }
            }
        } // guard dropped here before .await
        self.soft_delete_thread(source_id).await
    }

    async fn split_thread(&self, _thread_id: &str, doc_ids: &[String], new_name: &str) -> DbResult<Thread> {
        let new_thread = self.create_thread(Thread::new(new_name.to_string(), String::new())).await?;
        let new_tid = new_thread.id.as_ref().map(thing_to_raw).unwrap_or_default();
        let mut docs = self.documents.write().unwrap();
        for doc in docs.values_mut() {
            let doc_id = doc.id.as_ref().map(thing_to_raw).unwrap_or_default();
            if doc_ids.contains(&doc_id) {
                doc.thread_id = new_tid.clone();
            }
        }
        Ok(new_thread)
    }

    async fn soft_delete_document(&self, id: &str) -> DbResult<()> {
        let mut docs = self.documents.write().unwrap();
        if let Some(doc) = docs.get_mut(id) {
            doc.deleted_at = Some(Utc::now().to_rfc3339());
        }
        Ok(())
    }

    async fn restore_soft_deleted_document(&self, id: &str) -> DbResult<Document> {
        let mut docs = self.documents.write().unwrap();
        let doc = docs.get_mut(id).ok_or_else(|| DbError::NotFound(id.to_string()))?;
        doc.deleted_at = None;
        Ok(doc.clone())
    }

    async fn soft_delete_thread(&self, id: &str) -> DbResult<()> {
        let mut threads = self.threads.write().unwrap();
        if let Some(thread) = threads.get_mut(id) {
            thread.deleted_at = Some(Utc::now().to_rfc3339());
        }
        Ok(())
    }

    async fn restore_soft_deleted_thread(&self, id: &str) -> DbResult<Thread> {
        let mut threads = self.threads.write().unwrap();
        let thread = threads.get_mut(id).ok_or_else(|| DbError::NotFound(id.to_string()))?;
        thread.deleted_at = None;
        Ok(thread.clone())
    }

    async fn purge_deleted(&self, _max_age: std::time::Duration) -> DbResult<u64> { Ok(0) }

    async fn commit_document(&self, doc_id: &str, message: &str) -> DbResult<Commit> {
        let docs = self.documents.read().unwrap();
        let doc = docs.get(doc_id).ok_or_else(|| DbError::NotFound(doc_id.to_string()))?;

        let key = self.next_key();
        let doc_title = doc.title.clone();
        let doc_content = doc.content.clone();

        let existing = self.commits.read().unwrap();
        let parent = existing.get(doc_id)
            .and_then(|v| v.last())
            .and_then(|c| c.id.as_ref().map(thing_to_raw));

        drop(existing);
        drop(docs);

        let commit = Commit {
            id: Some(Self::make_thing("commit", &key)),
            document_id: doc_id.to_string(),
            parent_commit: parent,
            message: message.to_string(),
            snapshot: DocumentSnapshot {
                document_id: doc_id.to_string(),
                title: doc_title,
                content: doc_content,
            },
            timestamp: Utc::now(),
        };

        let mut commits = self.commits.write().unwrap();
        commits.entry(doc_id.to_string()).or_default().push(commit.clone());

        // Update head_commit on the document
        let commit_id_str = commit.id.as_ref().map(thing_to_raw).unwrap_or_default();
        let mut docs = self.documents.write().unwrap();
        if let Some(doc) = docs.get_mut(doc_id) {
            doc.head_commit = Some(commit_id_str);
        }

        Ok(commit)
    }

    async fn list_document_commits(&self, doc_id: &str) -> DbResult<Vec<Commit>> {
        let commits = self.commits.read().unwrap();
        let mut result = commits.get(doc_id).cloned().unwrap_or_default();
        result.reverse();
        Ok(result)
    }

    async fn get_commit(&self, commit_id: &str) -> DbResult<Commit> {
        let commits = self.commits.read().unwrap();
        for doc_commits in commits.values() {
            for c in doc_commits {
                if c.id.as_ref().map(thing_to_raw).as_deref() == Some(commit_id) {
                    return Ok(c.clone());
                }
            }
        }
        Err(DbError::NotFound(commit_id.to_string()))
    }

    async fn restore_document(&self, doc_id: &str, commit_id: &str) -> DbResult<Document> {
        let commit = self.get_commit(commit_id).await?;
        let mut docs = self.documents.write().unwrap();
        let doc = docs.get_mut(doc_id).ok_or_else(|| DbError::NotFound(doc_id.to_string()))?;
        doc.title = commit.snapshot.title;
        doc.content = commit.snapshot.content;
        Ok(doc.clone())
    }

    async fn create_milestone(&self, _milestone: Milestone) -> DbResult<Milestone> {
        Ok(_milestone)
    }
    async fn list_milestones(&self, _thread_id: &str) -> DbResult<Vec<Milestone>> { Ok(vec![]) }
    async fn list_all_milestones(&self) -> DbResult<Vec<Milestone>> { Ok(vec![]) }
    async fn delete_milestone(&self, _id: &str) -> DbResult<()> { Ok(()) }

    async fn create_contact(&self, mut contact: Contact) -> DbResult<Contact> {
        let key = self.next_key();
        let thing = Self::make_thing("contact", &key);
        let id_str = thing_to_raw(&thing);
        contact.id = Some(thing);
        self.contacts.write().unwrap().insert(id_str, contact.clone());
        Ok(contact)
    }

    async fn get_contact(&self, id: &str) -> DbResult<Contact> {
        self.contacts.read().unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| DbError::NotFound(id.to_string()))
    }

    async fn list_contacts(&self) -> DbResult<Vec<Contact>> {
        let contacts = self.contacts.read().unwrap();
        Ok(contacts.values()
            .filter(|c| c.deleted_at.is_none())
            .cloned()
            .collect())
    }

    async fn update_contact(&self, id: &str, name: Option<&str>, notes: Option<&str>, _avatar: Option<&str>) -> DbResult<Contact> {
        let mut contacts = self.contacts.write().unwrap();
        let contact = contacts.get_mut(id).ok_or_else(|| DbError::NotFound(id.to_string()))?;
        if let Some(n) = name { contact.name = n.to_string(); }
        if let Some(n) = notes { contact.notes = n.to_string(); }
        Ok(contact.clone())
    }

    async fn delete_contact(&self, id: &str) -> DbResult<()> {
        self.contacts.write().unwrap().remove(id);
        Ok(())
    }

    async fn soft_delete_contact(&self, id: &str) -> DbResult<()> {
        let mut contacts = self.contacts.write().unwrap();
        if let Some(c) = contacts.get_mut(id) {
            c.deleted_at = Some(Utc::now().to_rfc3339());
        }
        Ok(())
    }

    async fn find_contact_by_address(&self, address: &str) -> DbResult<Option<Contact>> {
        let contacts = self.contacts.read().unwrap();
        Ok(contacts.values()
            .find(|c| c.addresses.iter().any(|a| a.address == address))
            .cloned())
    }

    async fn add_contact_address(&self, contact_id: &str, address: ChannelAddress) -> DbResult<Contact> {
        let mut contacts = self.contacts.write().unwrap();
        let contact = contacts.get_mut(contact_id).ok_or_else(|| DbError::NotFound(contact_id.to_string()))?;
        contact.addresses.push(address);
        Ok(contact.clone())
    }

    async fn create_message(&self, mut message: Message) -> DbResult<Message> {
        let key = self.next_key();
        let thing = Self::make_thing("message", &key);
        let id_str = thing_to_raw(&thing);
        message.id = Some(thing);
        self.messages.write().unwrap().insert(id_str, message.clone());
        Ok(message)
    }

    async fn get_message(&self, id: &str) -> DbResult<Message> {
        self.messages.read().unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| DbError::NotFound(id.to_string()))
    }

    async fn list_messages(&self, conversation_id: &str, _before: Option<DateTime<Utc>>, limit: u32) -> DbResult<Vec<Message>> {
        let msgs = self.messages.read().unwrap();
        let mut result: Vec<Message> = msgs.values()
            .filter(|m| m.conversation_id == conversation_id)
            .cloned()
            .collect();
        result.sort_by(|a, b| b.sent_at.cmp(&a.sent_at));
        result.truncate(limit as usize);
        Ok(result)
    }

    async fn update_message_read_status(&self, id: &str, status: ReadStatus) -> DbResult<Message> {
        let mut msgs = self.messages.write().unwrap();
        let msg = msgs.get_mut(id).ok_or_else(|| DbError::NotFound(id.to_string()))?;
        msg.read_status = status;
        Ok(msg.clone())
    }

    async fn delete_message(&self, id: &str) -> DbResult<()> {
        self.messages.write().unwrap().remove(id);
        Ok(())
    }

    async fn list_all_messages(&self) -> DbResult<Vec<Message>> {
        let msgs = self.messages.read().unwrap();
        let mut all: Vec<Message> = msgs.values().cloned().collect();
        all.sort_by(|a, b| b.sent_at.cmp(&a.sent_at));
        Ok(all)
    }

    async fn list_messages_in_time_range(
        &self,
        after: DateTime<Utc>,
        before: DateTime<Utc>,
        limit: u32,
    ) -> DbResult<Vec<Message>> {
        let msgs = self.messages.read().unwrap();
        let mut result: Vec<Message> = msgs.values()
            .filter(|m| m.deleted_at.is_none() && m.sent_at >= after && m.sent_at <= before)
            .cloned()
            .collect();
        result.sort_by(|a, b| b.sent_at.cmp(&a.sent_at));
        result.truncate(limit as usize);
        Ok(result)
    }

    async fn search_messages(&self, query: &str) -> DbResult<Vec<Message>> {
        let q = query.to_lowercase();
        let msgs = self.messages.read().unwrap();
        Ok(msgs.values()
            .filter(|m| m.body.to_lowercase().contains(&q))
            .cloned()
            .collect())
    }

    async fn create_conversation(&self, mut conv: Conversation) -> DbResult<Conversation> {
        let key = self.next_key();
        let thing = Self::make_thing("conversation", &key);
        let id_str = thing_to_raw(&thing);
        conv.id = Some(thing);
        self.conversations.write().unwrap().insert(id_str, conv.clone());
        Ok(conv)
    }

    async fn get_conversation(&self, id: &str) -> DbResult<Conversation> {
        self.conversations.read().unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| DbError::NotFound(id.to_string()))
    }

    async fn list_conversations(&self, channel: Option<&ChannelType>) -> DbResult<Vec<Conversation>> {
        let convs = self.conversations.read().unwrap();
        Ok(convs.values()
            .filter(|c| c.deleted_at.is_none())
            .filter(|c| channel.map_or(true, |ch| &c.channel == ch))
            .cloned()
            .collect())
    }

    async fn update_conversation_unread(&self, id: &str, unread_count: u32) -> DbResult<Conversation> {
        let mut convs = self.conversations.write().unwrap();
        let conv = convs.get_mut(id).ok_or_else(|| DbError::NotFound(id.to_string()))?;
        conv.unread_count = unread_count;
        Ok(conv.clone())
    }

    async fn update_conversation_last_message_at(&self, id: &str, at: DateTime<Utc>) -> DbResult<Conversation> {
        let mut convs = self.conversations.write().unwrap();
        let conv = convs.get_mut(id).ok_or_else(|| DbError::NotFound(id.to_string()))?;
        conv.last_message_at = Some(at);
        Ok(conv.clone())
    }

    async fn delete_conversation(&self, id: &str) -> DbResult<()> {
        self.conversations.write().unwrap().remove(id);
        Ok(())
    }

    async fn link_conversation_to_thread(&self, conversation_id: &str, thread_id: &str) -> DbResult<Conversation> {
        let mut convs = self.conversations.write().unwrap();
        let conv = convs.get_mut(conversation_id).ok_or_else(|| DbError::NotFound(conversation_id.to_string()))?;
        conv.linked_thread_id = Some(thread_id.to_string());
        Ok(conv.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_create_and_list_documents() {
        let db = MockGraphDB::new();
        let t = db.create_thread(Thread::new("T".into(), "".into())).await.unwrap();
        let tid = t.id.as_ref().map(thing_to_raw).unwrap();

        let d1 = Document::new("Alpha".into(), tid.clone(), true);
        db.create_document(d1).await.unwrap();
        let d2 = Document::new("Beta".into(), tid.clone(), true);
        db.create_document(d2).await.unwrap();

        let all = db.list_documents(None).await.unwrap();
        assert_eq!(all.len(), 2);

        let by_thread = db.list_documents(Some(&tid)).await.unwrap();
        assert_eq!(by_thread.len(), 2);
    }

    #[tokio::test]
    async fn mock_search_documents_by_title() {
        let db = MockGraphDB::new();
        let d1 = Document::new("Meeting Notes".into(), "thread:1".into(), true);
        db.create_document(d1).await.unwrap();
        let d2 = Document::new("Grocery List".into(), "thread:1".into(), true);
        db.create_document(d2).await.unwrap();

        let results = db.search_documents_by_title("meeting").await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Meeting Notes");

        let empty = db.search_documents_by_title("nonexistent").await.unwrap();
        assert!(empty.is_empty());
    }

    #[tokio::test]
    async fn mock_find_thread_by_name() {
        let db = MockGraphDB::new();
        db.create_thread(Thread::new("Work".into(), "".into())).await.unwrap();
        db.create_thread(Thread::new("Personal".into(), "".into())).await.unwrap();

        let found = db.find_thread_by_name("work").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Work");

        let not_found = db.find_thread_by_name("missing").await.unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn mock_create_and_list_threads() {
        let db = MockGraphDB::new();
        db.create_thread(Thread::new("A".into(), "".into())).await.unwrap();
        db.create_thread(Thread::new("B".into(), "".into())).await.unwrap();

        let all = db.list_threads().await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn mock_create_and_list_suggested_links() {
        let db = MockGraphDB::new();
        let link = db
            .create_suggested_link(
                "document:a",
                "document:b",
                RelationType::References,
                0.8,
                "Both discuss CRDTs",
                SuggestionSource::Consolidation,
            )
            .await
            .unwrap();
        assert_eq!(link.status, SuggestionStatus::Pending);
        assert!(link.resolved_at.is_none());

        let pending = db.list_pending_suggestions().await.unwrap();
        assert_eq!(pending.len(), 1);
    }

    #[tokio::test]
    async fn mock_resolve_suggestion_accepted_creates_relationship() {
        let db = MockGraphDB::new();
        let link = db
            .create_suggested_link(
                "document:a",
                "document:b",
                RelationType::Supports,
                0.7,
                "Related topics",
                SuggestionSource::Consolidation,
            )
            .await
            .unwrap();
        let link_id = link.id.as_ref().unwrap().id.to_raw();

        let resolved = db.resolve_suggestion(&link_id, SuggestionStatus::Accepted).await.unwrap();
        assert_eq!(resolved.status, SuggestionStatus::Accepted);
        assert!(resolved.resolved_at.is_some());

        // Should have created a real relationship
        let rels = db.list_all_relationships().await.unwrap();
        assert_eq!(rels.len(), 1);

        // Pending list should be empty
        let pending = db.list_pending_suggestions().await.unwrap();
        assert!(pending.is_empty());
    }

    #[tokio::test]
    async fn mock_resolve_suggestion_dismissed_no_relationship() {
        let db = MockGraphDB::new();
        let link = db
            .create_suggested_link(
                "document:x",
                "document:y",
                RelationType::Contradicts,
                0.5,
                "Opposing views",
                SuggestionSource::Chat,
            )
            .await
            .unwrap();
        let link_id = link.id.as_ref().unwrap().id.to_raw();

        db.resolve_suggestion(&link_id, SuggestionStatus::Dismissed).await.unwrap();

        let rels = db.list_all_relationships().await.unwrap();
        assert!(rels.is_empty());

        let pending = db.list_pending_suggestions().await.unwrap();
        assert!(pending.is_empty());
    }

    #[tokio::test]
    async fn mock_suggestion_exists_bidirectional() {
        let db = MockGraphDB::new();
        db.create_suggested_link(
            "document:a",
            "document:b",
            RelationType::References,
            0.6,
            "test",
            SuggestionSource::Consolidation,
        )
        .await
        .unwrap();

        // Forward
        assert!(db.suggestion_exists("document:a", "document:b").await.unwrap());
        // Reverse
        assert!(db.suggestion_exists("document:b", "document:a").await.unwrap());
        // Unrelated
        assert!(!db.suggestion_exists("document:a", "document:c").await.unwrap());
    }

    #[tokio::test]
    async fn mock_list_pending_excludes_resolved() {
        let db = MockGraphDB::new();
        let l1 = db
            .create_suggested_link("document:a", "document:b", RelationType::Supports, 0.8, "r1", SuggestionSource::Consolidation)
            .await
            .unwrap();
        db.create_suggested_link("document:c", "document:d", RelationType::References, 0.6, "r2", SuggestionSource::Consolidation)
            .await
            .unwrap();

        let l1_id = l1.id.as_ref().unwrap().id.to_raw();
        db.resolve_suggestion(&l1_id, SuggestionStatus::Accepted).await.unwrap();

        let pending = db.list_pending_suggestions().await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].rationale, "r2");
    }
}
