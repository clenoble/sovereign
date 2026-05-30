use std::path::Path;
use wasmtime::component::Component;
use wasmtime::{Config, Engine};

use crate::wasm::wasm_skill::WasmSkill;

/// Default resource limits for WASM plugin execution.
const DEFAULT_MEMORY_LIMIT: usize = 16 * 1024 * 1024; // 16 MB
const DEFAULT_FUEL: u64 = 1_000_000_000; // ~1 billion instructions
const DEFAULT_INSTANCE_LIMIT: usize = 10;

/// Resource limits for WASM plugin execution.
#[derive(Debug, Clone)]
pub struct WasmLimits {
    pub memory_bytes: usize,
    pub fuel: u64,
    pub max_instances: usize,
}

impl Default for WasmLimits {
    fn default() -> Self {
        Self {
            memory_bytes: DEFAULT_MEMORY_LIMIT,
            fuel: DEFAULT_FUEL,
            max_instances: DEFAULT_INSTANCE_LIMIT,
        }
    }
}

/// Manages the wasmtime Engine, loads WASM components, and produces
/// `WasmSkill` instances that implement `CoreSkill`.
pub struct WasmSkillRunner {
    engine: Engine,
    limits: WasmLimits,
}

impl WasmSkillRunner {
    /// Create a new runner with default resource limits.
    pub fn new() -> anyhow::Result<Self> {
        Self::with_limits(WasmLimits::default())
    }

    /// Create a new runner with custom resource limits.
    pub fn with_limits(limits: WasmLimits) -> anyhow::Result<Self> {
        let mut config = Config::new();
        config.wasm_component_model(true);
        config.consume_fuel(true);
        let engine = Engine::new(&config)?;

        Ok(Self { engine, limits })
    }

    /// Load a WASM component from a file and return a `WasmSkill`.
    pub fn load_skill(&self, wasm_path: &Path) -> anyhow::Result<WasmSkill> {
        let component = Component::from_file(&self.engine, wasm_path)?;
        WasmSkill::new(self.engine.clone(), component, self.limits.clone())
    }

    /// Load a WASM component from bytes.
    pub fn load_skill_from_bytes(&self, bytes: &[u8]) -> anyhow::Result<WasmSkill> {
        let component = Component::from_binary(&self.engine, bytes)?;
        WasmSkill::new(self.engine.clone(), component, self.limits.clone())
    }

    /// Scan a directory for WASM skill plugins.
    /// Looks for subdirectories containing both a `skill.json` and a `.wasm` file.
    pub fn discover_skills(&self, dir: &Path) -> Vec<(String, anyhow::Result<WasmSkill>)> {
        let mut results = Vec::new();
        if !dir.is_dir() {
            return results;
        }

        let Ok(entries) = std::fs::read_dir(dir) else {
            return results;
        };

        for entry in entries.flatten() {
            let skill_dir = entry.path();
            if !skill_dir.is_dir() {
                continue;
            }

            let manifest_path = skill_dir.join("skill.json");
            if !manifest_path.exists() {
                continue;
            }

            // Find the first .wasm file in the directory
            let wasm_file = std::fs::read_dir(&skill_dir)
                .ok()
                .and_then(|entries| {
                    entries
                        .flatten()
                        .find(|e| {
                            e.path()
                                .extension()
                                .map(|ext| ext == "wasm")
                                .unwrap_or(false)
                        })
                        .map(|e| e.path())
                });

            if let Some(wasm_path) = wasm_file {
                let dir_name = skill_dir
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                results.push((dir_name, self.load_skill(&wasm_path)));
            }
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runner_creation() {
        let runner = WasmSkillRunner::new();
        assert!(runner.is_ok());
    }

    #[test]
    fn test_runner_custom_limits() {
        let limits = WasmLimits {
            memory_bytes: 1024 * 1024,
            fuel: 100_000,
            max_instances: 2,
        };
        let runner = WasmSkillRunner::with_limits(limits);
        assert!(runner.is_ok());
    }

    #[test]
    fn test_load_nonexistent_file() {
        let runner = WasmSkillRunner::new().unwrap();
        let result = runner.load_skill(Path::new("/nonexistent/skill.wasm"));
        assert!(result.is_err());
    }

    #[test]
    fn test_discover_empty_dir() {
        let runner = WasmSkillRunner::new().unwrap();
        let results = runner.discover_skills(Path::new("/nonexistent"));
        assert!(results.is_empty());
    }
}
