use std::collections::HashMap;
use std::sync::Arc;

use sovereign_db::schema::thing_to_raw;
use sovereign_db::surreal::SurrealGraphDB;
use sovereign_db::GraphDB;

use crate::traits::SkillDbAccess;

/// Implements [`SkillDbAccess`] for [`SurrealGraphDB`], bridging the async DB
/// into the synchronous skill interface via `block_in_place`.
impl SkillDbAccess for SurrealGraphDB {
    fn search_documents(&self, query: &str) -> anyhow::Result<Vec<(String, String, String)>> {
        let query_lower = query.to_lowercase();
        let docs: Vec<sovereign_db::schema::Document> = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(GraphDB::list_documents(self, None))
        })?;

        let results = docs
            .iter()
            .filter(|d| {
                d.title.to_lowercase().contains(&query_lower)
                    || d.content.to_lowercase().contains(&query_lower)
            })
            .map(|d| {
                let id = d.id_string().unwrap_or_default();
                let snippet: String = d.content.chars().take(100).collect();
                (id, d.title.clone(), snippet)
            })
            .collect();

        Ok(results)
    }

    fn get_document(&self, id: &str) -> anyhow::Result<(String, String, String)> {
        let doc = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(GraphDB::get_document(self, id))
        })?;
        Ok((doc.title, doc.thread_id, doc.content))
    }

    fn list_documents(&self, thread_id: Option<&str>) -> anyhow::Result<Vec<(String, String)>> {
        let docs = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(GraphDB::list_documents(self, thread_id))
        })?;
        let results = docs
            .iter()
            .map(|d| (d.id_string().unwrap_or_default(), d.title.clone()))
            .collect();
        Ok(results)
    }

    fn create_document(
        &self,
        title: &str,
        thread_id: &str,
        content: &str,
    ) -> anyhow::Result<String> {
        use sovereign_db::schema::Document;

        let mut doc = Document::new(title.to_string(), thread_id.to_string(), true);
        doc.content = content.to_string();

        let created = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(GraphDB::create_document(self, doc))
        })?;

        Ok(created.id_string().unwrap_or_default())
    }

    fn list_relationships(&self, doc_id: &str) -> anyhow::Result<Vec<(String, String)>> {
        let rels = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(GraphDB::list_outgoing_relationships(self, doc_id))
        })?;
        Ok(rels
            .iter()
            .filter_map(|r| {
                let target = r.out.as_ref().map(thing_to_raw)?;
                Some((r.relation_type.to_string(), target))
            })
            .collect())
    }

    fn list_backlinks(&self, doc_id: &str) -> anyhow::Result<Vec<(String, String)>> {
        let rels = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(GraphDB::list_incoming_relationships(self, doc_id))
        })?;
        Ok(rels
            .iter()
            .filter_map(|r| {
                let source = r.in_.as_ref().map(thing_to_raw)?;
                Some((source, r.relation_type.to_string()))
            })
            .collect())
    }

    fn find_or_create_thread(
        &self,
        name: &str,
        description: &str,
    ) -> anyhow::Result<String> {
        use sovereign_db::schema::Thread;

        let id_opt = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(GraphDB::find_thread_by_name(self, name))
        })?;

        if let Some(thread) = id_opt {
            if let Some(id) = thread.id_string() {
                return Ok(id);
            }
        }

        let thread = Thread::new(name.to_string(), description.to_string());
        let created = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(GraphDB::create_thread(self, thread))
        })?;
        created
            .id_string()
            .ok_or_else(|| anyhow::anyhow!("Created thread had no id"))
    }

    fn list_all_documents_with_link_counts(
        &self,
    ) -> anyhow::Result<Vec<(String, String, u32, u32)>> {
        let (docs, rels) = tokio::task::block_in_place(|| {
            let handle = tokio::runtime::Handle::current();
            let docs = handle.block_on(GraphDB::list_documents(self, None))?;
            let rels = handle.block_on(GraphDB::list_all_relationships(self))?;
            anyhow::Ok((docs, rels))
        })?;

        let mut out_count: HashMap<String, u32> = HashMap::new();
        let mut in_count: HashMap<String, u32> = HashMap::new();
        for r in &rels {
            if let Some(t) = &r.in_ {
                *out_count.entry(thing_to_raw(t)).or_insert(0) += 1;
            }
            if let Some(t) = &r.out {
                *in_count.entry(thing_to_raw(t)).or_insert(0) += 1;
            }
        }

        Ok(docs
            .iter()
            .map(|d| {
                let id = d.id_string().unwrap_or_default();
                let in_d = in_count.get(&id).copied().unwrap_or(0);
                let out_d = out_count.get(&id).copied().unwrap_or(0);
                (id, d.title.clone(), in_d, out_d)
            })
            .collect())
    }
}

/// Helper to wrap a `SurrealGraphDB` as `Arc<dyn SkillDbAccess>`.
pub fn wrap_db(db: Arc<SurrealGraphDB>) -> Arc<dyn SkillDbAccess> {
    db
}
