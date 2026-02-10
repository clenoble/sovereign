use std::sync::Arc;

use sovereign_db::schema::Document;
use sovereign_db::surreal::SurrealGraphDB;
use sovereign_db::GraphDB;

use crate::traits::{CoreSkill, SkillDocument, SkillOutput};

pub struct DuplicateDocumentSkill {
    db: Arc<SurrealGraphDB>,
}

impl DuplicateDocumentSkill {
    pub fn new(db: Arc<SurrealGraphDB>) -> Self {
        Self { db }
    }
}

impl CoreSkill for DuplicateDocumentSkill {
    fn name(&self) -> &str {
        "duplicate-document"
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
        doc: &SkillDocument,
        _params: &str,
    ) -> anyhow::Result<SkillOutput> {
        match action {
            "duplicate" => {
                let db = self.db.clone();
                let doc_id = doc.id.clone();

                let original = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(db.get_document(&doc_id))
                })?;

                let new_title = format!("Copy of {}", original.title);
                let mut new_doc = Document::new(
                    new_title.clone(),
                    original.thread_id.clone(),
                    original.is_owned,
                );
                new_doc.content = original.content.clone();

                let created = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(db.create_document(new_doc))
                })?;

                let new_id = created.id_string().unwrap_or_default();
                let json = serde_json::json!({
                    "doc_id": new_id,
                    "title": new_title,
                    "original_id": doc_id,
                });

                Ok(SkillOutput::StructuredData {
                    kind: "duplicate_result".into(),
                    json: json.to_string(),
                })
            }
            _ => anyhow::bail!("Unknown action: {action}"),
        }
    }

    fn actions(&self) -> Vec<(String, String)> {
        vec![("duplicate".into(), "Duplicate".into())]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sovereign_core::content::ContentFields;
    use sovereign_db::surreal::StorageMode;

    async fn setup_db() -> Arc<SurrealGraphDB> {
        let db = SurrealGraphDB::new(StorageMode::Memory).await.unwrap();
        db.connect().await.unwrap();
        db.init_schema().await.unwrap();
        Arc::new(db)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn duplicate_creates_copy() {
        let db = setup_db().await;

        let mut orig = Document::new("Original".into(), "thread:t".into(), true);
        let cf = ContentFields { body: "some body text".into(), images: vec![] };
        orig.content = cf.serialize();
        let created = db.create_document(orig).await.unwrap();
        let orig_id = created.id_string().unwrap();

        let skill = DuplicateDocumentSkill::new(db.clone());
        let skill_doc = SkillDocument {
            id: orig_id.clone(),
            title: "Original".into(),
            content: cf.clone(),
        };

        let result = skill.execute("duplicate", &skill_doc, "").unwrap();
        match result {
            SkillOutput::StructuredData { kind, json } => {
                assert_eq!(kind, "duplicate_result");
                let v: serde_json::Value = serde_json::from_str(&json).unwrap();
                assert_eq!(v["title"], "Copy of Original");
                assert_eq!(v["original_id"], orig_id);
                assert!(!v["doc_id"].as_str().unwrap().is_empty());
            }
            _ => panic!("Expected StructuredData"),
        }

        // Should now have 2 documents
        let docs = db.list_documents(None).await.unwrap();
        assert_eq!(docs.len(), 2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn duplicate_nonexistent_errors() {
        let db = setup_db().await;
        let skill = DuplicateDocumentSkill::new(db);
        let skill_doc = SkillDocument {
            id: "document:nonexistent".into(),
            title: "X".into(),
            content: ContentFields::default(),
        };
        let result = skill.execute("duplicate", &skill_doc, "");
        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn duplicate_preserves_content() {
        let db = setup_db().await;
        let mut orig = Document::new("WithContent".into(), "thread:t".into(), true);
        let cf = ContentFields { body: "important data".into(), images: vec![] };
        orig.content = cf.serialize();
        let created = db.create_document(orig).await.unwrap();
        let orig_id = created.id_string().unwrap();

        let skill = DuplicateDocumentSkill::new(db.clone());
        let skill_doc = SkillDocument {
            id: orig_id,
            title: "WithContent".into(),
            content: cf,
        };
        skill.execute("duplicate", &skill_doc, "").unwrap();

        let docs = db.list_documents(None).await.unwrap();
        let copy = docs.iter().find(|d| d.title == "Copy of WithContent").unwrap();
        assert_eq!(copy.content, created.content);
    }
}
