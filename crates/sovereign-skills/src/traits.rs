use std::collections::HashSet;
use std::sync::Arc;

use sovereign_core::content::ContentFields;

use crate::manifest::Capability;

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

/// Narrow DB interface exposed to skills.
/// Skills never see the full database — only this subset.
pub trait SkillDbAccess: Send + Sync {
    /// Search documents matching query. Returns (id, title, snippet).
    fn search_documents(&self, query: &str) -> anyhow::Result<Vec<(String, String, String)>>;
    /// Get a single document by ID. Returns (title, thread_id, content).
    fn get_document(&self, id: &str) -> anyhow::Result<(String, String, String)>;
    /// List documents, optionally filtered by thread. Returns (id, title).
    fn list_documents(&self, thread_id: Option<&str>) -> anyhow::Result<Vec<(String, String)>>;
    /// Create a new document, returns the document ID.
    fn create_document(&self, title: &str, thread_id: &str, content: &str) -> anyhow::Result<String>;
}

/// Resources available to a skill during execution.
/// The registry checks that required_capabilities() is a subset of granted.
pub struct SkillContext {
    pub granted: HashSet<Capability>,
    pub db: Option<Arc<dyn SkillDbAccess>>,
}

/// Trait for core skills that are compiled into the Sovereign GE binary.
///
/// Core skills use direct Rust trait calls (no IPC).
/// Community/sideloaded skills will use IPC instead.
pub trait CoreSkill: Send + Sync {
    fn name(&self) -> &str;

    /// Capabilities this skill requires to function.
    fn required_capabilities(&self) -> Vec<Capability>;

    fn activate(&mut self) -> anyhow::Result<()>;
    fn deactivate(&mut self) -> anyhow::Result<()>;

    /// Execute a skill action on a document.
    /// `params` is action-specific (e.g., image path for "add image").
    fn execute(
        &self,
        action: &str,
        doc: &SkillDocument,
        params: &str,
        ctx: &SkillContext,
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
        fn required_capabilities(&self) -> Vec<Capability> {
            vec![Capability::ReadDocument, Capability::WriteDocument]
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
            _ctx: &SkillContext,
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
        let ctx = SkillContext {
            granted: skill.required_capabilities().into_iter().collect(),
            db: None,
        };
        let result = skill.execute("save", &doc, "", &ctx).unwrap();
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
