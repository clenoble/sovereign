use std::collections::HashMap;
use std::sync::Arc;

use sovereign_db::schema::thing_to_raw;
use sovereign_db::GraphDB;

use crate::traits::SkillDbAccess;

/// Implements [`SkillDbAccess`] for any [`GraphDB`] impl, bridging the async
/// DB into the synchronous skill interface via `block_in_place`. Generic so
/// it works for both raw `SurrealGraphDB` and the `LayeredGraphDB` indirection
/// used at runtime to swap in `EncryptedGraphDB` post-login.
impl<T: GraphDB + Send + Sync> SkillDbAccess for T {
    fn search_documents(&self, query: &str) -> anyhow::Result<Vec<(String, String, String)>> {
        let query_lower = query.to_lowercase();
        let docs: Vec<sovereign_db::schema::Document> = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(GraphDB::list_documents(self, None))
        })?;

        // C-DOC adoption step 1: match and snippet on the document's text
        // read through the v2 model, not on its stored JSON (which matched
        // queries like "body" or "images" in every v1 envelope and showed
        // JSON as the snippet). Tokens stay `[pii:<id>]`.
        let results = docs
            .iter()
            .filter_map(|d| {
                let text = sovereign_docformat::plain_text_of(&d.content);
                let hit = d.title.to_lowercase().contains(&query_lower)
                    || text.to_lowercase().contains(&query_lower);
                hit.then(|| {
                    let id = d.id_string().unwrap_or_default();
                    let snippet: String = text.chars().take(100).collect();
                    (id, d.title.clone(), snippet)
                })
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

    /// Finds the thread; never creates one. Threads are written only through
    /// the document service's one thread-write path (THREAD-sync S1), which a
    /// bare store does not have: the host's service-backed wrapper
    /// (`sovereign-ai` `docs_host::ServiceSkillDb`) creates it.
    fn find_or_create_thread(
        &self,
        name: &str,
        _description: &str,
    ) -> anyhow::Result<String> {
        let id_opt = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(GraphDB::find_thread_by_name(self, name))
        })?;

        if let Some(thread) = id_opt {
            if let Some(id) = thread.id_string() {
                return Ok(id);
            }
        }
        anyhow::bail!("no thread named {name:?}, and threads are created only through the document service")
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

/// Helper to coerce any `Arc<T: GraphDB>` into `Arc<dyn SkillDbAccess>`.
/// Used by sovereign-app to hand the LayeredGraphDB to the skill registry.
pub fn wrap_db<T: GraphDB + Send + Sync + 'static>(db: Arc<T>) -> Arc<dyn SkillDbAccess> {
    db
}

#[cfg(test)]
mod tests {
    use super::*;
    use sovereign_db::schema::Document;
    use sovereign_db::surreal::{StorageMode, SurrealGraphDB};

    /// C-DOC adoption step 1: search matches and snippets the documents' text
    /// for a v2 document, a v1 envelope and raw text — never their stored
    /// JSON — with tokens as tokens.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn search_reads_v2_v1_and_raw_documents_as_text() {
        let db = SurrealGraphDB::new(StorageMode::Memory).await.unwrap();
        db.connect().await.unwrap();
        db.init_schema().await.unwrap();
        let v2 = r#"{"format":"sovereign.doc","v":2,"blocks":[{"type":"text","id":"00000000000000000000000000001e01","text":"needle in v2 [pii:pii_record:e1]"},{"type":"image","id":"00000000000000000000000000001e02","ref":{"path":"/i.png"},"alt":"a"}]}"#;
        let v1 = r#"{"body":"needle in v1 [pii:pii_record:e1]","images":[],"videos":[]}"#;
        for (title, content) in [("A", v2), ("B", v1), ("C", "needle in raw [pii:pii_record:e1]")] {
            let mut d = Document::new(title.into(), "thread:t".into(), true);
            d.content = content.into();
            GraphDB::create_document(&db, d).await.unwrap();
        }
        let db: Arc<dyn SkillDbAccess> = wrap_db(Arc::new(db));

        let mut hits = db.search_documents("needle").unwrap();
        hits.sort_by(|a, b| a.1.cmp(&b.1));
        let snippets: Vec<&str> = hits.iter().map(|h| h.2.as_str()).collect();
        assert_eq!(
            snippets,
            ["needle in v2 [pii:pii_record:e1]", "needle in v1 [pii:pii_record:e1]", "needle in raw [pii:pii_record:e1]"]
        );
        // Stored JSON keys are not text: they no longer match.
        for key in ["images", "sovereign.doc", "blocks"] {
            assert!(db.search_documents(key).unwrap().is_empty(), "{key} matched stored JSON");
        }
    }
}
