use std::sync::Arc;

use sovereign_ai::Orchestrator;
use sovereign_skills::SkillLlmAccess;

/// Bridges the async `Orchestrator::generate` into the synchronous
/// `SkillLlmAccess` interface via `block_in_place`.
pub struct LlmBridge {
    orchestrator: Arc<Orchestrator>,
}

impl LlmBridge {
    pub fn new(orchestrator: Arc<Orchestrator>) -> Self {
        Self { orchestrator }
    }
}

impl SkillLlmAccess for LlmBridge {
    fn generate(&self, prompt: &str, max_tokens: u32) -> anyhow::Result<String> {
        let orchestrator = self.orchestrator.clone();
        let prompt = prompt.to_string();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(orchestrator.generate(&prompt, max_tokens))
        })
    }
}

/// Wrap an orchestrator as `Arc<dyn SkillLlmAccess>` for `SkillContext.llm`.
pub fn wrap_orchestrator(orch: Arc<Orchestrator>) -> Arc<dyn SkillLlmAccess> {
    Arc::new(LlmBridge::new(orch))
}
