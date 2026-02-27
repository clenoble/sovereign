use crate::manifest::Capability;
use crate::traits::{CoreSkill, SkillContext, SkillDocument, SkillOutput};
use sovereign_core::content::ContentFields;

pub struct FindReplaceSkill;

#[derive(serde::Deserialize)]
struct FindReplaceParams {
    find: String,
    replace: String,
}

impl CoreSkill for FindReplaceSkill {
    fn name(&self) -> &str {
        "find-replace"
    }

    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::ReadDocument, Capability::WriteDocument]
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
        params: &str,
        _ctx: &SkillContext,
    ) -> anyhow::Result<SkillOutput> {
        match action {
            "find_replace" => {
                let p: FindReplaceParams = serde_json::from_str(params)
                    .map_err(|e| anyhow::anyhow!("Bad params (expected JSON with 'find' and 'replace'): {e}"))?;

                if p.find.is_empty() {
                    anyhow::bail!("Find string cannot be empty");
                }

                // Single-pass replace; compare to original to detect no-match
                let new_body = doc.content.body.replace(&p.find, &p.replace);
                if new_body == doc.content.body {
                    let json = serde_json::json!({ "replacements": 0 });
                    return Ok(SkillOutput::StructuredData {
                        kind: "find_replace".into(),
                        json: json.to_string(),
                    });
                }

                Ok(SkillOutput::ContentUpdate(ContentFields {
                    body: new_body,
                    images: doc.content.images.clone(),
                    videos: doc.content.videos.clone(),
                }))
            }
            _ => anyhow::bail!("Unknown action: {action}"),
        }
    }

    fn actions(&self) -> Vec<(String, String)> {
        vec![("find_replace".into(), "Find & Replace".into())]
    }

    fn file_types(&self) -> Vec<String> {
        vec!["md".into(), "txt".into()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn basic_replace() {
        let skill = FindReplaceSkill;
        let doc = make_doc("hello world");
        let params = r#"{"find": "world", "replace": "rust"}"#;
        let result = skill.execute("find_replace", &doc, params, &dummy_ctx()).unwrap();
        match result {
            SkillOutput::ContentUpdate(cf) => {
                assert_eq!(cf.body, "hello rust");
            }
            _ => panic!("Expected ContentUpdate"),
        }
    }

    #[test]
    fn multiple_replacements() {
        let skill = FindReplaceSkill;
        let doc = make_doc("aaa bbb aaa ccc aaa");
        let params = r#"{"find": "aaa", "replace": "xxx"}"#;
        let result = skill.execute("find_replace", &doc, params, &dummy_ctx()).unwrap();
        match result {
            SkillOutput::ContentUpdate(cf) => {
                assert_eq!(cf.body, "xxx bbb xxx ccc xxx");
            }
            _ => panic!("Expected ContentUpdate"),
        }
    }

    #[test]
    fn no_match_returns_structured_data() {
        let skill = FindReplaceSkill;
        let doc = make_doc("hello world");
        let params = r#"{"find": "notfound", "replace": "x"}"#;
        let result = skill.execute("find_replace", &doc, params, &dummy_ctx()).unwrap();
        match result {
            SkillOutput::StructuredData { kind, json } => {
                assert_eq!(kind, "find_replace");
                let v: serde_json::Value = serde_json::from_str(&json).unwrap();
                assert_eq!(v["replacements"], 0);
            }
            _ => panic!("Expected StructuredData"),
        }
    }

    #[test]
    fn empty_find_errors() {
        let skill = FindReplaceSkill;
        let doc = make_doc("hello");
        let params = r#"{"find": "", "replace": "x"}"#;
        let result = skill.execute("find_replace", &doc, params, &dummy_ctx());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn bad_params_errors() {
        let skill = FindReplaceSkill;
        let doc = make_doc("hello");
        let result = skill.execute("find_replace", &doc, "not json", &dummy_ctx());
        assert!(result.is_err());
    }

    #[test]
    fn actions_returns_find_replace() {
        let skill = FindReplaceSkill;
        let actions = skill.actions();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].0, "find_replace");
        assert_eq!(actions[0].1, "Find & Replace");
    }
}
