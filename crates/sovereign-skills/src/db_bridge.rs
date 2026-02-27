use std::sync::Arc;

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
}

/// Helper to wrap a `SurrealGraphDB` as `Arc<dyn SkillDbAccess>`.
pub fn wrap_db(db: Arc<SurrealGraphDB>) -> Arc<dyn SkillDbAccess> {
    db
}
