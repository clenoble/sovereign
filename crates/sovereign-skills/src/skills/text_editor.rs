use crate::traits::{CoreSkill, SkillDocument, SkillOutput};
use sovereign_core::content::ContentFields;

pub struct TextEditorSkill;

impl CoreSkill for TextEditorSkill {
    fn name(&self) -> &str {
        "text-editor"
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
    ) -> anyhow::Result<SkillOutput> {
        match action {
            "save" => {
                let updated = ContentFields {
                    body: params.to_string(),
                    images: doc.content.images.clone(),
                    videos: doc.content.videos.clone(),
                };
                Ok(SkillOutput::ContentUpdate(updated))
            }
            _ => anyhow::bail!("Unknown action: {action}"),
        }
    }

    fn actions(&self) -> Vec<(String, String)> {
        vec![("save".into(), "Save".into())]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_doc() -> SkillDocument {
        SkillDocument {
            id: "document:test".into(),
            title: "Test Doc".into(),
            content: ContentFields {
                body: "original body".into(),
                ..Default::default()
            },
        }
    }

    #[test]
    fn save_returns_content_update_with_new_body() {
        let skill = TextEditorSkill;
        let doc = make_doc();
        let result = skill.execute("save", &doc, "new body text").unwrap();
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
