use std::path::Path;

use crate::manifest::SkillManifest;
use crate::traits::CoreSkill;

pub struct SkillRegistry {
    manifests: Vec<SkillManifest>,
    skills: Vec<Box<dyn CoreSkill>>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self {
            manifests: Vec::new(),
            skills: Vec::new(),
        }
    }

    /// Walk a directory looking for `skill.json` files and parse them.
    pub fn scan_directory(&mut self, path: &Path) -> anyhow::Result<()> {
        if !path.is_dir() {
            return Ok(());
        }
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let skill_path = entry.path().join("skill.json");
            if skill_path.exists() {
                match SkillManifest::load(&skill_path) {
                    Ok(manifest) => {
                        tracing::info!("Loaded skill: {} v{}", manifest.name, manifest.version);
                        self.manifests.push(manifest);
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to load skill from {}: {e}",
                            skill_path.display()
                        );
                    }
                }
            }
        }
        Ok(())
    }

    pub fn manifests(&self) -> &[SkillManifest] {
        &self.manifests
    }

    /// Register a core skill instance.
    pub fn register(&mut self, skill: Box<dyn CoreSkill>) {
        self.skills.push(skill);
    }

    /// Find a registered skill by name.
    pub fn find_skill(&self, name: &str) -> Option<&dyn CoreSkill> {
        self.skills.iter().find(|s| s.name() == name).map(|s| &**s)
    }

    /// Get all registered skills.
    pub fn all_skills(&self) -> &[Box<dyn CoreSkill>] {
        &self.skills
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::{SkillDocument, SkillOutput};
    use std::path::PathBuf;

    struct DummySkill(&'static str);

    impl CoreSkill for DummySkill {
        fn name(&self) -> &str {
            self.0
        }
        fn activate(&mut self) -> anyhow::Result<()> {
            Ok(())
        }
        fn deactivate(&mut self) -> anyhow::Result<()> {
            Ok(())
        }
        fn execute(
            &self,
            _action: &str,
            _doc: &SkillDocument,
            _params: &str,
        ) -> anyhow::Result<SkillOutput> {
            Ok(SkillOutput::None)
        }
        fn actions(&self) -> Vec<(String, String)> {
            vec![]
        }
    }

    fn project_skills_dir() -> PathBuf {
        // Navigate from crate root to workspace root
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        PathBuf::from(manifest_dir)
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("skills")
    }

    #[test]
    fn test_scan_finds_example_manifests() {
        let skills_dir = project_skills_dir();
        let mut registry = SkillRegistry::new();
        registry.scan_directory(&skills_dir).unwrap();
        assert_eq!(
            registry.manifests().len(),
            3,
            "Expected 3 skill manifests in {:?}, found {}",
            skills_dir,
            registry.manifests().len()
        );
    }

    #[test]
    fn test_scan_nonexistent_dir() {
        let mut registry = SkillRegistry::new();
        registry
            .scan_directory(std::path::Path::new("/nonexistent"))
            .unwrap();
        assert_eq!(registry.manifests().len(), 0);
    }

    #[test]
    fn test_register_and_find_skill() {
        let mut registry = SkillRegistry::new();
        registry.register(Box::new(DummySkill("text-editor")));
        registry.register(Box::new(DummySkill("image")));

        assert!(registry.find_skill("text-editor").is_some());
        assert!(registry.find_skill("image").is_some());
        assert!(registry.find_skill("nonexistent").is_none());
    }

    #[test]
    fn test_all_skills_returns_registered() {
        let mut registry = SkillRegistry::new();
        assert!(registry.all_skills().is_empty());

        registry.register(Box::new(DummySkill("a")));
        registry.register(Box::new(DummySkill("b")));
        assert_eq!(registry.all_skills().len(), 2);
    }
}
