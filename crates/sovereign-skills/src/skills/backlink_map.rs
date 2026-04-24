use serde::Serialize;

use crate::manifest::Capability;
use crate::traits::{CoreSkill, SkillContext, SkillDocument, SkillOutput};

pub struct BacklinkMapSkill;

#[derive(Debug, Serialize)]
struct Backlink {
    source_id: String,
    source_title: String,
    relation_type: String,
}

impl CoreSkill for BacklinkMapSkill {
    fn name(&self) -> &str {
        "backlink-map"
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
        doc: &SkillDocument,
        _params: &str,
        ctx: &SkillContext,
    ) -> anyhow::Result<SkillOutput> {
        match action {
            "find_backlinks" => {
                let db = ctx
                    .db
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Backlink Map requires database access"))?;

                let raw = db.list_backlinks(&doc.id)?;
                let mut backlinks: Vec<Backlink> = Vec::with_capacity(raw.len());
                for (source_id, relation_type) in raw {
                    let title = match db.get_document(&source_id) {
                        Ok((t, _, _)) => t,
                        Err(_) => "(unknown)".to_string(),
                    };
                    backlinks.push(Backlink {
                        source_id,
                        source_title: title,
                        relation_type,
                    });
                }

                let json = serde_json::to_string(&serde_json::json!({
                    "doc_id": doc.id,
                    "doc_title": doc.title,
                    "backlinks": backlinks,
                    "count": backlinks.len(),
                }))?;
                Ok(SkillOutput::StructuredData {
                    kind: "backlinks".into(),
                    json,
                })
            }
            _ => anyhow::bail!("Unknown action: {action}"),
        }
    }

    fn actions(&self) -> Vec<(String, String)> {
        vec![("find_backlinks".into(), "Find Backlinks".into())]
    }

    fn file_types(&self) -> Vec<String> {
        // Universal — works on any document.
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Arc;

    use super::*;
    use sovereign_core::content::ContentFields;

    use crate::traits::SkillDbAccess;

    struct StubDb;

    impl SkillDbAccess for StubDb {
        fn search_documents(&self, _: &str) -> anyhow::Result<Vec<(String, String, String)>> {
            Ok(vec![])
        }
        fn get_document(&self, id: &str) -> anyhow::Result<(String, String, String)> {
            Ok((format!("Title for {id}"), "thread:1".into(), "body".into()))
        }
        fn list_documents(&self, _: Option<&str>) -> anyhow::Result<Vec<(String, String)>> {
            Ok(vec![])
        }
        fn create_document(&self, _: &str, _: &str, _: &str) -> anyhow::Result<String> {
            Ok("document:new".into())
        }
        fn list_relationships(&self, _: &str) -> anyhow::Result<Vec<(String, String)>> {
            Ok(vec![])
        }
        fn list_backlinks(&self, _: &str) -> anyhow::Result<Vec<(String, String)>> {
            Ok(vec![
                ("document:src1".into(), "references".into()),
                ("document:src2".into(), "supports".into()),
            ])
        }
        fn list_all_documents_with_link_counts(
            &self,
        ) -> anyhow::Result<Vec<(String, String, u32, u32)>> {
            Ok(vec![])
        }
        fn find_or_create_thread(&self, _: &str, _: &str) -> anyhow::Result<String> {
            Ok("thread:1".into())
        }
    }

    fn make_doc() -> SkillDocument {
        SkillDocument {
            id: "document:target".into(),
            title: "Target".into(),
            content: ContentFields::default(),
        }
    }

    fn ctx_with_db() -> SkillContext {
        SkillContext {
            granted: [Capability::ReadAllDocuments].into_iter().collect(),
            db: Some(Arc::new(StubDb) as Arc<dyn SkillDbAccess>),
            llm: None,
        }
    }

    #[test]
    fn returns_backlinks_with_source_titles() {
        let skill = BacklinkMapSkill;
        let result = skill.execute("find_backlinks", &make_doc(), "", &ctx_with_db()).unwrap();
        if let SkillOutput::StructuredData { json, kind } = result {
            assert_eq!(kind, "backlinks");
            let v: serde_json::Value = serde_json::from_str(&json).unwrap();
            assert_eq!(v["doc_id"], "document:target");
            assert_eq!(v["count"], 2);
            assert_eq!(v["backlinks"][0]["source_id"], "document:src1");
            assert_eq!(v["backlinks"][0]["source_title"], "Title for document:src1");
            assert_eq!(v["backlinks"][0]["relation_type"], "references");
            assert_eq!(v["backlinks"][1]["relation_type"], "supports");
        } else {
            panic!("expected StructuredData");
        }
    }

    #[test]
    fn errors_when_no_db_in_context() {
        let skill = BacklinkMapSkill;
        let ctx = SkillContext { granted: HashSet::new(), db: None, llm: None };
        let result = skill.execute("find_backlinks", &make_doc(), "", &ctx);
        assert!(result.is_err());
    }
}
