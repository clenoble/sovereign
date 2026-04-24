use serde::Serialize;

use crate::manifest::Capability;
use crate::traits::{CoreSkill, SkillContext, SkillDocument, SkillOutput};

pub struct OrphanFinderSkill;

#[derive(Debug, Serialize)]
struct Orphan {
    id: String,
    title: String,
    out_degree: u32,
}

impl CoreSkill for OrphanFinderSkill {
    fn name(&self) -> &str {
        "orphan-finder"
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
        _params: &str,
        ctx: &SkillContext,
    ) -> anyhow::Result<SkillOutput> {
        match action {
            "find_orphans" => {
                let db = ctx
                    .db
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Orphan Finder requires database access"))?;

                let all = db.list_all_documents_with_link_counts()?;
                let orphans: Vec<Orphan> = all
                    .into_iter()
                    .filter(|(_, _, in_d, _)| *in_d == 0)
                    .map(|(id, title, _, out_d)| Orphan { id, title, out_degree: out_d })
                    .collect();

                let json = serde_json::to_string(&serde_json::json!({
                    "orphans": orphans,
                    "count": orphans.len(),
                }))?;
                Ok(SkillOutput::StructuredData {
                    kind: "orphans".into(),
                    json,
                })
            }
            _ => anyhow::bail!("Unknown action: {action}"),
        }
    }

    fn actions(&self) -> Vec<(String, String)> {
        vec![("find_orphans".into(), "Find Orphan Documents".into())]
    }

    fn file_types(&self) -> Vec<String> {
        // Universal — works on any document; the action ignores the doc.
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use sovereign_core::content::ContentFields;

    use crate::traits::SkillDbAccess;

    struct StubDb {
        rows: Vec<(String, String, u32, u32)>,
    }

    impl SkillDbAccess for StubDb {
        fn search_documents(&self, _: &str) -> anyhow::Result<Vec<(String, String, String)>> {
            Ok(vec![])
        }
        fn get_document(&self, _: &str) -> anyhow::Result<(String, String, String)> {
            Ok(("".into(), "".into(), "".into()))
        }
        fn list_documents(&self, _: Option<&str>) -> anyhow::Result<Vec<(String, String)>> {
            Ok(vec![])
        }
        fn create_document(&self, _: &str, _: &str, _: &str) -> anyhow::Result<String> {
            Ok("".into())
        }
        fn list_relationships(&self, _: &str) -> anyhow::Result<Vec<(String, String)>> {
            Ok(vec![])
        }
        fn list_backlinks(&self, _: &str) -> anyhow::Result<Vec<(String, String)>> {
            Ok(vec![])
        }
        fn list_all_documents_with_link_counts(
            &self,
        ) -> anyhow::Result<Vec<(String, String, u32, u32)>> {
            Ok(self.rows.clone())
        }
        fn find_or_create_thread(&self, _: &str, _: &str) -> anyhow::Result<String> {
            Ok("thread:1".into())
        }
    }

    fn dummy_doc() -> SkillDocument {
        SkillDocument {
            id: "document:any".into(),
            title: "Any".into(),
            content: ContentFields::default(),
        }
    }

    fn ctx_with(rows: Vec<(String, String, u32, u32)>) -> SkillContext {
        SkillContext {
            granted: [Capability::ReadAllDocuments].into_iter().collect(),
            db: Some(Arc::new(StubDb { rows }) as Arc<dyn SkillDbAccess>),
            llm: None,
        }
    }

    fn run(rows: Vec<(String, String, u32, u32)>) -> serde_json::Value {
        let skill = OrphanFinderSkill;
        let result = skill
            .execute("find_orphans", &dummy_doc(), "", &ctx_with(rows))
            .unwrap();
        if let SkillOutput::StructuredData { json, .. } = result {
            serde_json::from_str(&json).unwrap()
        } else {
            panic!("expected StructuredData");
        }
    }

    #[test]
    fn returns_only_documents_with_zero_in_degree() {
        let v = run(vec![
            ("document:a".into(), "Linked".into(), 2, 1),
            ("document:b".into(), "Lonely".into(), 0, 3),
            ("document:c".into(), "Also lonely".into(), 0, 0),
            ("document:d".into(), "Popular".into(), 5, 0),
        ]);
        assert_eq!(v["count"], 2);
        let titles: Vec<&str> = v["orphans"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| o["title"].as_str().unwrap())
            .collect();
        assert!(titles.contains(&"Lonely"));
        assert!(titles.contains(&"Also lonely"));
    }

    #[test]
    fn empty_workspace_returns_zero() {
        let v = run(vec![]);
        assert_eq!(v["count"], 0);
    }

    #[test]
    fn includes_out_degree_in_output() {
        let v = run(vec![("document:x".into(), "X".into(), 0, 7)]);
        assert_eq!(v["orphans"][0]["out_degree"], 7);
    }
}
