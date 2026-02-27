use crate::manifest::Capability;
use crate::traits::{CoreSkill, SkillContext, SkillDocument, SkillOutput};

pub struct SearchSkill;

impl CoreSkill for SearchSkill {
    fn name(&self) -> &str {
        "search"
    }

    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::ReadAllDocuments]
    }

    fn activate(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn deactivate(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn execute(
        &self,
        action: &str,
        _doc: &SkillDocument,
        params: &str,
        ctx: &SkillContext,
    ) -> anyhow::Result<SkillOutput> {
        match action {
            "search" => {
                let query = params.to_lowercase();
                if query.is_empty() {
                    anyhow::bail!("Search query cannot be empty");
                }

                let db = ctx
                    .db
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Search skill requires DB access"))?;

                let results = db.search_documents(&query)?;

                let matches: Vec<serde_json::Value> = results
                    .into_iter()
                    .map(|(id, title, snippet)| {
                        serde_json::json!({
                            "id": id,
                            "title": title,
                            "snippet": snippet,
                        })
                    })
                    .collect();

                let json = serde_json::to_string(&matches)?;
                Ok(SkillOutput::StructuredData {
                    kind: "search_results".into(),
                    json,
                })
            }
            _ => anyhow::bail!("Unknown action: {action}"),
        }
    }

    fn actions(&self) -> Vec<(String, String)> {
        vec![("search".into(), "Search Documents".into())]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sovereign_core::content::ContentFields;
    use sovereign_db::schema::Document;
    use sovereign_db::surreal::{StorageMode, SurrealGraphDB};
    use sovereign_db::GraphDB;
    use std::sync::Arc;

    async fn setup_db() -> Arc<SurrealGraphDB> {
        let db = SurrealGraphDB::new(StorageMode::Memory).await.unwrap();
        db.connect().await.unwrap();
        db.init_schema().await.unwrap();
        Arc::new(db)
    }

    fn make_ctx(db: Arc<SurrealGraphDB>) -> SkillContext {
        SkillContext {
            granted: [Capability::ReadAllDocuments].into_iter().collect(),
            db: Some(crate::db_bridge::wrap_db(db)),
        }
    }

    fn make_doc() -> SkillDocument {
        SkillDocument {
            id: "document:dummy".into(),
            title: "Dummy".into(),
            content: ContentFields::default(),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn search_finds_matching_title() {
        let db = setup_db().await;
        let mut doc = Document::new("Rust Programming".into(), "thread:t".into(), true);
        let cf = ContentFields { body: "learning rust".into(), ..Default::default() };
        doc.content = cf.serialize();
        db.create_document(doc).await.unwrap();

        let ctx = make_ctx(db);
        let skill = SearchSkill;
        let result = skill.execute("search", &make_doc(), "rust", &ctx).unwrap();
        match result {
            SkillOutput::StructuredData { kind, json } => {
                assert_eq!(kind, "search_results");
                let v: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
                assert_eq!(v.len(), 1);
                assert_eq!(v[0]["title"], "Rust Programming");
            }
            _ => panic!("Expected StructuredData"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn search_no_results() {
        let db = setup_db().await;
        let ctx = make_ctx(db);
        let skill = SearchSkill;
        let result = skill.execute("search", &make_doc(), "nonexistent", &ctx).unwrap();
        match result {
            SkillOutput::StructuredData { json, .. } => {
                let v: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
                assert!(v.is_empty());
            }
            _ => panic!("Expected StructuredData"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn search_empty_query_errors() {
        let db = setup_db().await;
        let ctx = make_ctx(db);
        let skill = SearchSkill;
        let result = skill.execute("search", &make_doc(), "", &ctx);
        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn search_finds_by_content() {
        let db = setup_db().await;
        let mut doc = Document::new("My Notes".into(), "thread:t".into(), true);
        let cf = ContentFields { body: "important meeting notes about budget".into(), ..Default::default() };
        doc.content = cf.serialize();
        db.create_document(doc).await.unwrap();

        let ctx = make_ctx(db);
        let skill = SearchSkill;
        let result = skill.execute("search", &make_doc(), "budget", &ctx).unwrap();
        match result {
            SkillOutput::StructuredData { json, .. } => {
                let v: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
                assert_eq!(v.len(), 1);
            }
            _ => panic!("Expected StructuredData"),
        }
    }
}
