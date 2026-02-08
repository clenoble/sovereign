use std::path::Path;

use crate::manifest::SkillManifest;

pub struct SkillRegistry {
    manifests: Vec<SkillManifest>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self {
            manifests: Vec::new(),
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
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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
}
