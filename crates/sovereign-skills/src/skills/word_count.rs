use crate::manifest::Capability;
use crate::traits::{CoreSkill, SkillContext, SkillDocument, SkillOutput};

pub struct WordCountSkill;

impl CoreSkill for WordCountSkill {
    fn name(&self) -> &str {
        "word-count"
    }

    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::ReadDocument]
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
        _ctx: &SkillContext,
    ) -> anyhow::Result<SkillOutput> {
        match action {
            "count" => {
                let body = &doc.content.body;
                let words = body.split_whitespace().count();
                let characters = body.chars().count();
                let lines = if body.is_empty() { 0 } else { body.lines().count() };
                let reading_time_min = (words as f64 / 200.0).ceil() as u64;

                let json = serde_json::json!({
                    "words": words,
                    "characters": characters,
                    "lines": lines,
                    "reading_time_min": reading_time_min,
                });

                Ok(SkillOutput::StructuredData {
                    kind: "word_count".into(),
                    json: json.to_string(),
                })
            }
            _ => anyhow::bail!("Unknown action: {action}"),
        }
    }

    fn actions(&self) -> Vec<(String, String)> {
        vec![("count".into(), "Word Count".into())]
    }

    fn file_types(&self) -> Vec<String> {
        vec!["md".into(), "txt".into()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sovereign_core::content::ContentFields;

    fn dummy_ctx() -> SkillContext {
        SkillContext { granted: std::collections::HashSet::new(), db: None }
    }

    fn make_doc(body: &str) -> SkillDocument {
        SkillDocument {
            id: "document:test".into(),
            title: "Test".into(),
            content: ContentFields {
                body: body.into(),
                ..Default::default()
            },
        }
    }

    #[test]
    fn count_basic() {
        let skill = WordCountSkill;
        let doc = make_doc("hello world foo bar");
        let result = skill.execute("count", &doc, "", &dummy_ctx()).unwrap();
        match result {
            SkillOutput::StructuredData { kind, json } => {
                assert_eq!(kind, "word_count");
                let v: serde_json::Value = serde_json::from_str(&json).unwrap();
                assert_eq!(v["words"], 4);
                assert_eq!(v["characters"], 19);
                assert_eq!(v["lines"], 1);
                assert_eq!(v["reading_time_min"], 1);
            }
            _ => panic!("Expected StructuredData"),
        }
    }

    #[test]
    fn count_empty() {
        let skill = WordCountSkill;
        let doc = make_doc("");
        let result = skill.execute("count", &doc, "", &dummy_ctx()).unwrap();
        match result {
            SkillOutput::StructuredData { kind, json } => {
                assert_eq!(kind, "word_count");
                let v: serde_json::Value = serde_json::from_str(&json).unwrap();
                assert_eq!(v["words"], 0);
                assert_eq!(v["characters"], 0);
                assert_eq!(v["lines"], 0);
                assert_eq!(v["reading_time_min"], 0);
            }
            _ => panic!("Expected StructuredData"),
        }
    }

    #[test]
    fn count_multiline() {
        let skill = WordCountSkill;
        let doc = make_doc("line one\nline two\nline three");
        let result = skill.execute("count", &doc, "", &dummy_ctx()).unwrap();
        match result {
            SkillOutput::StructuredData { json, .. } => {
                let v: serde_json::Value = serde_json::from_str(&json).unwrap();
                assert_eq!(v["words"], 6);
                assert_eq!(v["lines"], 3);
            }
            _ => panic!("Expected StructuredData"),
        }
    }

    #[test]
    fn count_reading_time() {
        let skill = WordCountSkill;
        // 400 words -> 2 min reading time
        let body = "word ".repeat(400);
        let doc = make_doc(body.trim());
        let result = skill.execute("count", &doc, "", &dummy_ctx()).unwrap();
        match result {
            SkillOutput::StructuredData { json, .. } => {
                let v: serde_json::Value = serde_json::from_str(&json).unwrap();
                assert_eq!(v["words"], 400);
                assert_eq!(v["reading_time_min"], 2);
            }
            _ => panic!("Expected StructuredData"),
        }
    }

    #[test]
    fn actions_returns_count() {
        let skill = WordCountSkill;
        let actions = skill.actions();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].0, "count");
        assert_eq!(actions[0].1, "Word Count");
    }
}
