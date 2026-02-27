use crate::manifest::Capability;
use crate::traits::{CoreSkill, SkillContext, SkillDocument, SkillOutput};
use sovereign_core::content::ContentImage;

pub struct ImageSkill;

impl CoreSkill for ImageSkill {
    fn name(&self) -> &str {
        "image"
    }

    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::ReadDocument, Capability::WriteDocument, Capability::ReadFilesystem]
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
            "add" => {
                let mut updated = doc.content.clone();
                updated.images.push(ContentImage {
                    path: params.to_string(),
                    caption: String::new(),
                });
                Ok(SkillOutput::ContentUpdate(updated))
            }
            "remove" => {
                let idx: usize = params
                    .parse()
                    .map_err(|_| anyhow::anyhow!("Invalid image index: {params}"))?;
                if idx >= doc.content.images.len() {
                    anyhow::bail!("Image index {idx} out of range ({})", doc.content.images.len());
                }
                let mut updated = doc.content.clone();
                updated.images.remove(idx);
                Ok(SkillOutput::ContentUpdate(updated))
            }
            _ => anyhow::bail!("Unknown action: {action}"),
        }
    }

    fn actions(&self) -> Vec<(String, String)> {
        vec![
            ("add".into(), "Add Image".into()),
            ("remove".into(), "Remove Image".into()),
        ]
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

    fn make_doc() -> SkillDocument {
        SkillDocument {
            id: "document:test".into(),
            title: "Test Doc".into(),
            content: ContentFields {
                body: "body".into(),
                images: vec![ContentImage {
                    path: "/existing.png".into(),
                    caption: "existing".into(),
                }],
                ..Default::default()
            },
        }
    }

    #[test]
    fn add_appends_image() {
        let skill = ImageSkill;
        let doc = make_doc();
        let result = skill.execute("add", &doc, "/new.png", &dummy_ctx()).unwrap();
        match result {
            SkillOutput::ContentUpdate(cf) => {
                assert_eq!(cf.images.len(), 2);
                assert_eq!(cf.images[1].path, "/new.png");
            }
            _ => panic!("Expected ContentUpdate"),
        }
    }

    #[test]
    fn remove_deletes_image() {
        let skill = ImageSkill;
        let doc = make_doc();
        let result = skill.execute("remove", &doc, "0", &dummy_ctx()).unwrap();
        match result {
            SkillOutput::ContentUpdate(cf) => {
                assert!(cf.images.is_empty());
            }
            _ => panic!("Expected ContentUpdate"),
        }
    }

    #[test]
    fn actions_returns_add_and_remove() {
        let skill = ImageSkill;
        let actions = skill.actions();
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].0, "add");
        assert_eq!(actions[1].0, "remove");
    }
}
