use crate::manifest::Capability;
use crate::traits::{CoreSkill, SkillContext, SkillDocument, SkillOutput};

pub struct DuplicateDocumentSkill;

impl CoreSkill for DuplicateDocumentSkill {
    fn name(&self) -> &str {
        "duplicate-document"
    }

    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::ReadAllDocuments, Capability::WriteAllDocuments]
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
        ctx: &SkillContext,
    ) -> anyhow::Result<SkillOutput> {
        match action {
            "duplicate" => {
                let db = ctx
                    .db
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Duplicate skill requires DB access"))?;

                let (title, thread_id, content) = db.get_document(&doc.id)?;
                let new_title = format!("Copy of {}", title);
                let new_id = db.create_document(&new_title, &thread_id, &content)?;

                let json = serde_json::json!({
                    "doc_id": new_id,
                    "title": new_title,
                    "original_id": doc.id,
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
            granted: [Capability::ReadAllDocuments, Capability::WriteAllDocuments]
                .into_iter()
                .collect(),
            db: Some(crate::db_bridge::wrap_db(db)),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn duplicate_creates_copy() {
        let db = setup_db().await;

        let mut orig = Document::new("Original".into(), "thread:t".into(), true);
        let cf = ContentFields { body: "some body text".into(), ..Default::default() };
        orig.content = cf.serialize();
        let created = db.create_document(orig).await.unwrap();
        let orig_id = created.id_string().unwrap();

        let ctx = make_ctx(db.clone());
        let skill = DuplicateDocumentSkill;
        let skill_doc = SkillDocument {
            id: orig_id.clone(),
            title: "Original".into(),
            content: cf.clone(),
        };

        let result = skill.execute("duplicate", &skill_doc, "", &ctx).unwrap();
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

        let docs = db.list_documents(None).await.unwrap();
        assert_eq!(docs.len(), 2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn duplicate_nonexistent_errors() {
        let db = setup_db().await;
        let ctx = make_ctx(db);
        let skill = DuplicateDocumentSkill;
        let skill_doc = SkillDocument {
            id: "document:nonexistent".into(),
            title: "X".into(),
            content: ContentFields::default(),
        };
        let result = skill.execute("duplicate", &skill_doc, "", &ctx);
        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn duplicate_preserves_content() {
        let db = setup_db().await;
        let mut orig = Document::new("WithContent".into(), "thread:t".into(), true);
        let cf = ContentFields { body: "important data".into(), ..Default::default() };
        orig.content = cf.serialize();
        let created = db.create_document(orig).await.unwrap();
        let orig_id = created.id_string().unwrap();

        let ctx = make_ctx(db.clone());
        let skill = DuplicateDocumentSkill;
        let skill_doc = SkillDocument {
            id: orig_id,
            title: "WithContent".into(),
            content: cf,
        };
        skill.execute("duplicate", &skill_doc, "", &ctx).unwrap();

        let docs = db.list_documents(None).await.unwrap();
        let copy = docs.iter().find(|d| d.title == "Copy of WithContent").unwrap();
        assert_eq!(copy.content, created.content);
    }
}
