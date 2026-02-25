use sovereign_core::content::ContentFields;

/// A document passed to a skill for execution.
#[derive(Debug, Clone)]
pub struct SkillDocument {
    pub id: String,
    pub title: String,
    pub content: ContentFields,
}

/// Output from a skill execution.
#[derive(Debug, Clone)]
pub enum SkillOutput {
    /// Updated content to save back
    ContentUpdate(ContentFields),
    /// Binary file to save (e.g., PDF)
    File {
        name: String,
        mime_type: String,
        data: Vec<u8>,
    },
    /// Nothing (action had side effects only)
    None,
    /// Structured data result (e.g., search results, word count stats).
    /// `kind` discriminates the payload; `json` is the serialized data.
    StructuredData { kind: String, json: String },
}

/// Trait for core skills that are compiled into the Sovereign GE binary.
///
/// Core skills use direct Rust trait calls (no IPC).
/// Community/sideloaded skills will use IPC instead.
pub trait CoreSkill: Send + Sync {
    fn name(&self) -> &str;
    fn activate(&mut self) -> anyhow::Result<()>;
    fn deactivate(&mut self) -> anyhow::Result<()>;

    /// Execute a skill action on a document.
    /// `params` is action-specific (e.g., image path for "add image").
    fn execute(
        &self,
        action: &str,
        doc: &SkillDocument,
        params: &str,
    ) -> anyhow::Result<SkillOutput>;

    /// List available actions this skill provides.
    /// Returns vec of (action_id, display_label).
    fn actions(&self) -> Vec<(String, String)>;

    /// File extensions this skill applies to (e.g. `["md", "txt"]`).
    /// Empty means universal — the skill works on any document type.
    fn file_types(&self) -> Vec<String> {
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockSkill {
        active: bool,
    }

    impl CoreSkill for MockSkill {
        fn name(&self) -> &str {
            "mock-skill"
        }
        fn activate(&mut self) -> anyhow::Result<()> {
            self.active = true;
            Ok(())
        }
        fn deactivate(&mut self) -> anyhow::Result<()> {
            self.active = false;
            Ok(())
        }
        fn execute(
            &self,
            action: &str,
            doc: &SkillDocument,
            _params: &str,
        ) -> anyhow::Result<SkillOutput> {
            match action {
                "save" => {
                    let mut updated = doc.content.clone();
                    updated.body = format!("saved: {}", updated.body);
                    Ok(SkillOutput::ContentUpdate(updated))
                }
                _ => Ok(SkillOutput::None),
            }
        }
        fn actions(&self) -> Vec<(String, String)> {
            vec![("save".into(), "Save".into())]
        }
    }

    #[test]
    fn test_core_skill_trait_implementable() {
        let mut skill = MockSkill { active: false };
        assert_eq!(skill.name(), "mock-skill");
        assert!(!skill.active);

        skill.activate().unwrap();
        assert!(skill.active);

        let doc = SkillDocument {
            id: "document:abc".into(),
            title: "Test".into(),
            content: ContentFields {
                body: "hello".into(),
                ..Default::default()
            },
        };
        let result = skill.execute("save", &doc, "").unwrap();
        match result {
            SkillOutput::ContentUpdate(cf) => assert_eq!(cf.body, "saved: hello"),
            _ => panic!("Expected ContentUpdate"),
        }

        skill.deactivate().unwrap();
        assert!(!skill.active);
    }

    #[test]
    fn test_core_skill_is_object_safe() {
        let skill: Box<dyn CoreSkill> = Box::new(MockSkill { active: false });
        assert_eq!(skill.name(), "mock-skill");
        assert_eq!(skill.actions().len(), 1);
    }

    #[test]
    fn test_skill_document_construction() {
        let doc = SkillDocument {
            id: "document:xyz".into(),
            title: "My Doc".into(),
            content: ContentFields::default(),
        };
        assert_eq!(doc.id, "document:xyz");
        assert_eq!(doc.title, "My Doc");
        assert_eq!(doc.content.body, "");
        assert!(doc.content.images.is_empty());
    }

    #[test]
    fn test_skill_output_variants() {
        let update = SkillOutput::ContentUpdate(ContentFields::default());
        assert!(matches!(update, SkillOutput::ContentUpdate(_)));

        let file = SkillOutput::File {
            name: "test.pdf".into(),
            mime_type: "application/pdf".into(),
            data: vec![1, 2, 3],
        };
        assert!(matches!(file, SkillOutput::File { .. }));

        let none = SkillOutput::None;
        assert!(matches!(none, SkillOutput::None));

        let data = SkillOutput::StructuredData {
            kind: "test".into(),
            json: r#"{"value":42}"#.into(),
        };
        assert!(matches!(data, SkillOutput::StructuredData { .. }));
        if let SkillOutput::StructuredData { kind, json } = data {
            assert_eq!(kind, "test");
            assert!(json.contains("42"));
        }
    }
}
