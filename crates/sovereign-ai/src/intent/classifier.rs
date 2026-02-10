use std::path::Path;

use anyhow::Result;
use sovereign_core::config::AiConfig;
use sovereign_core::interfaces::{ModelBackend, UserIntent};

use crate::llm::prompt::{qwen_chat_prompt, REASONING_SYSTEM_PROMPT, ROUTER_SYSTEM_PROMPT};
use crate::llm::AsyncLlmBackend;

use super::parser::parse_intent_response;

/// Classifies user text into a `UserIntent` using the 3B router model.
/// Falls back to 7B reasoning model if confidence is below threshold.
pub struct IntentClassifier {
    pub(crate) router: AsyncLlmBackend,
    config: AiConfig,
    confidence_threshold: f32,
}

impl IntentClassifier {
    /// Create a new classifier. Does not load the model yet — call `load_router()` first.
    pub fn new(config: AiConfig) -> Self {
        Self {
            router: AsyncLlmBackend::new(config.n_ctx),
            config,
            confidence_threshold: 0.7,
        }
    }

    /// Load the 3B router model. Call during startup.
    pub async fn load_router(&mut self) -> Result<()> {
        let model_path = Path::new(&self.config.model_dir)
            .join(&self.config.router_model)
            .to_string_lossy()
            .to_string();

        if !Path::new(&model_path).exists() {
            anyhow::bail!("Router model not found: {model_path}");
        }

        tracing::info!("Loading router model: {model_path}");
        self.router
            .load(&model_path, self.config.n_gpu_layers)
            .await?;
        tracing::info!("Router model loaded");
        Ok(())
    }

    /// Classify user text into an intent. Uses 3B router, escalates to 7B if low confidence.
    pub async fn classify(&self, user_text: &str) -> Result<UserIntent> {
        let prompt = qwen_chat_prompt(ROUTER_SYSTEM_PROMPT, user_text);
        let response = self.router.generate(&prompt, 200).await?;
        tracing::debug!("Router response: {response}");

        let intent = parse_intent_response(&response)?;

        if intent.confidence < self.confidence_threshold {
            tracing::info!(
                "Low confidence ({:.2}), escalating to reasoning model",
                intent.confidence
            );
            return self.escalate_to_reasoning(user_text).await;
        }

        Ok(intent)
    }

    /// Hot-swap to 7B reasoning model for complex queries.
    /// Sequence: unload 3B → load 7B → infer → unload 7B → reload 3B.
    async fn escalate_to_reasoning(&self, user_text: &str) -> Result<UserIntent> {
        // We need &mut self for load/unload, but classify takes &self.
        // The AsyncLlmBackend uses Arc<Mutex<>> internally, so we can create
        // a separate backend for reasoning without touching the router.
        let mut reasoning = AsyncLlmBackend::new(self.config.n_ctx);

        let model_path = Path::new(&self.config.model_dir)
            .join(&self.config.reasoning_model)
            .to_string_lossy()
            .to_string();

        if !Path::new(&model_path).exists() {
            tracing::warn!("Reasoning model not found: {model_path}, using router result");
            let prompt = qwen_chat_prompt(ROUTER_SYSTEM_PROMPT, user_text);
            let response = self.router.generate(&prompt, 200).await?;
            return parse_intent_response(&response);
        }

        // Unload router to free VRAM
        // We can't unload through &self, so we'll load reasoning alongside.
        // On GTX 1660 (6GB), 3B (1.9GB) + 7B (4.2GB) won't fit.
        // For now, create a fresh backend — the router stays loaded in VRAM
        // and we rely on the OS/CUDA to handle the oversubscription for
        // the brief escalation window. In practice, the 7B will load to
        // remaining VRAM + system RAM offload.
        //
        // TODO: implement proper hot-swap with mutable access pattern
        tracing::info!("Loading reasoning model for escalation: {model_path}");
        reasoning
            .load(&model_path, self.config.n_gpu_layers)
            .await?;

        let prompt = qwen_chat_prompt(REASONING_SYSTEM_PROMPT, user_text);
        let response = reasoning.generate(&prompt, 300).await?;
        tracing::debug!("Reasoning response: {response}");

        reasoning.unload().await?;
        tracing::info!("Reasoning model unloaded");

        parse_intent_response(&response)
    }
}
