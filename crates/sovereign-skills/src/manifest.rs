use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SkillType {
    Core,
    Community,
}

/// Capabilities a skill may request. Enforced by the registry at execution time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    ReadDocument,
    WriteDocument,
    ReadAllDocuments,
    WriteAllDocuments,
    ReadFilesystem,
    WriteFilesystem,
    Network,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub skill_type: SkillType,
    pub capabilities: Vec<Capability>,
    pub file_types: Vec<String>,
}

impl SkillManifest {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let manifest: SkillManifest = serde_json::from_str(&content)?;
        Ok(manifest)
    }

    pub fn from_json(json: &str) -> anyhow::Result<Self> {
        let manifest: SkillManifest = serde_json::from_str(json)?;
        Ok(manifest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_manifest() {
        let json = r#"{
            "name": "Test Skill",
            "version": "1.0.0",
            "description": "A test skill",
            "author": "Test Author",
            "skill_type": "core",
            "capabilities": ["read_document", "write_document"],
            "file_types": ["txt"]
        }"#;
        let manifest = SkillManifest::from_json(json).unwrap();
        assert_eq!(manifest.name, "Test Skill");
        assert_eq!(manifest.skill_type, SkillType::Core);
        assert_eq!(
            manifest.capabilities,
            vec![Capability::ReadDocument, Capability::WriteDocument]
        );
    }

    #[test]
    fn test_parse_community_type() {
        let json = r#"{
            "name": "Community Skill",
            "version": "0.5.0",
            "description": "A community skill",
            "author": "Community",
            "skill_type": "community",
            "capabilities": ["write_filesystem"],
            "file_types": ["pdf"]
        }"#;
        let manifest = SkillManifest::from_json(json).unwrap();
        assert_eq!(manifest.skill_type, SkillType::Community);
        assert_eq!(manifest.capabilities, vec![Capability::WriteFilesystem]);
    }

    #[test]
    fn test_parse_missing_fields() {
        let json = r#"{"name": "Incomplete"}"#;
        let result = SkillManifest::from_json(json);
        assert!(result.is_err());
    }
}
