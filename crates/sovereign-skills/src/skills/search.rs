use std::sync::Arc;

use sovereign_db::surreal::SurrealGraphDB;
use sovereign_db::GraphDB;

use crate::traits::{CoreSkill, SkillDocument, SkillOutput};

pub struct SearchSkill {
    db: Arc<SurrealGraphDB>,
}

impl SearchSkill {
    pub fn new(db: Arc<SurrealGraphDB>) -> Self {
        Self { db }
    }
}

impl CoreSkill for SearchSkill {
    fn name(&self) -> &str {
        "search"
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
    ) -> anyhow::Result<SkillOutput> {
        match action {
            "search" => {
                let query = params.to_lowercase();
                if query.is_empty() {
                    anyhow::bail!("Search query cannot be empty");
                }

                let db = self.db.clone();
                let docs = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(db.list_documents(None))
                })?;

                let matches: Vec<serde_json::Value> = docs
                    .iter()
                    .filter(|d| {
                        d.title.to_lowercase().contains(&query)
                            || d.content.to_lowercase().contains(&query)
                    })
                    .map(|d| {
                        let id = d.id_string().unwrap_or_default();
                        let snippet = d.content.chars().take(100).collect::<String>();
                        serde_json::json!({
                            "id": id,
                            "title": d.title,
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
    use sovereign_db::surreal::StorageMode;

    async fn setup_db() -> Arc<SurrealGraphDB> {
        let db = SurrealGraphDB::new(StorageMode::Memory).await.unwrap();
        db.connect().await.unwrap();
        db.init_schema().await.unwrap();
        Arc::new(db)
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
        let cf = ContentFields { body: "learning rust".into(), images: vec![] };
        doc.content = cf.serialize();
        db.create_document(doc).await.unwrap();

        let skill = SearchSkill::new(db);
        let result = skill.execute("search", &make_doc(), "rust").unwrap();
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
        let skill = SearchSkill::new(db);
        let result = skill.execute("search", &make_doc(), "nonexistent").unwrap();
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
        let skill = SearchSkill::new(db);
        let result = skill.execute("search", &make_doc(), "");
        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn search_finds_by_content() {
        let db = setup_db().await;
        let mut doc = Document::new("My Notes".into(), "thread:t".into(), true);
        let cf = ContentFields { body: "important meeting notes about budget".into(), images: vec![] };
        doc.content = cf.serialize();
        db.create_document(doc).await.unwrap();

        let skill = SearchSkill::new(db);
        let result = skill.execute("search", &make_doc(), "budget").unwrap();
        match result {
            SkillOutput::StructuredData { json, .. } => {
                let v: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
                assert_eq!(v.len(), 1);
            }
            _ => panic!("Expected StructuredData"),
        }
    }
}
