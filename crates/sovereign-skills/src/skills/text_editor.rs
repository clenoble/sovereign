use crate::content_util::replace_body;
use crate::manifest::Capability;
use crate::traits::{CoreSkill, SkillContext, SkillDocument, SkillOutput};

pub struct TextEditorSkill;

impl CoreSkill for TextEditorSkill {
    fn name(&self) -> &str {
        "text-editor"
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
            "save" => {
                Ok(SkillOutput::ContentUpdate(replace_body(doc, params.to_string())))
            }
            _ => anyhow::bail!("Unknown action: {action}"),
        }
    }

    fn actions(&self) -> Vec<(String, String)> {
        vec![("save".into(), "Save".into())]
    }

    fn file_types(&self) -> Vec<String> {
        vec!["md".into(), "txt".into()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{dummy_ctx, make_doc_with_title};

    #[test]
    fn save_returns_content_update_with_new_body() {
        let skill = TextEditorSkill;
        let doc = make_doc_with_title("Test Doc", "original body");
        let result = skill.execute("save", &doc, "new body text", &dummy_ctx()).unwrap();
        match result {
            SkillOutput::ContentUpdate(cf) => {
                assert_eq!(cf.body, "new body text");
            }
            _ => panic!("Expected ContentUpdate"),
        }
    }

    #[test]
    fn actions_returns_save() {
        let skill = TextEditorSkill;
        let actions = skill.actions();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].0, "save");
    }
}
