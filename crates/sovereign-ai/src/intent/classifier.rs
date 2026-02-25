use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use sovereign_core::config::AiConfig;
use sovereign_core::interfaces::{ModelBackend, UserIntent};

use crate::llm::format::{self, PromptFormatter};
use crate::llm::prompt::{build_reasoning_system_prompt, build_router_system_prompt, format_single_turn};
use crate::llm::AsyncLlmBackend;

use super::parser::parse_intent_response;

/// Idle timeout before unloading the reasoning model to free VRAM.
const REASONING_IDLE_SECS: u64 = 300; // 5 minutes

/// Classifies user text into a `UserIntent` using the 3B router model.
/// Falls back to 7B reasoning model if confidence is below threshold.
pub struct IntentClassifier {
    pub(crate) router: AsyncLlmBackend,
    config: AiConfig,
    confidence_threshold: f32,
    /// Cached 7B reasoning backend — loaded on first escalation, reused thereafter.
    reasoning: Option<AsyncLlmBackend>,
    /// Timestamp of the last reasoning model use, for idle-timeout unloading.
    last_escalation: Option<Instant>,
    /// Model-family prompt formatter, created from config.
    pub(crate) formatter: Arc<dyn PromptFormatter>,
}

impl IntentClassifier {
    /// Create a new classifier. Does not load the model yet — call `load_router()` first.
    pub fn new(config: AiConfig) -> Self {
        let fmt = format::PromptFormat::from_str(&config.prompt_format);
        let formatter: Arc<dyn PromptFormatter> = Arc::from(format::create_formatter(fmt));
        Self {
            router: AsyncLlmBackend::new(config.n_ctx),
            config,
            confidence_threshold: 0.7,
            reasoning: None,
            last_escalation: None,
            formatter,
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

    /// Hot-swap the router model to a different .gguf file.
    pub(crate) async fn swap_router(&self, model_path: &str, n_gpu_layers: i32) -> Result<()> {
        tracing::info!("Swapping router model to: {model_path}");
        self.router.swap(model_path, n_gpu_layers).await?;
        tracing::info!("Router model swapped successfully");
        Ok(())
    }

    /// Replace the prompt formatter at runtime (e.g. after a model hot-swap).
    pub(crate) fn swap_formatter(&mut self, new_format: format::PromptFormat) {
        let old = self.config.prompt_format.clone();
        let new_name = match new_format {
            format::PromptFormat::ChatML => "chatml",
            format::PromptFormat::Mistral => "mistral",
            format::PromptFormat::Llama3 => "llama3",
        };
        tracing::info!("Swapping prompt formatter: {old} → {new_name}");
        self.formatter = Arc::from(format::create_formatter(new_format));
        self.config.prompt_format = new_name.to_string();
    }

    /// Classify user text into an intent. Uses 3B router, escalates to 7B if low confidence.
    pub async fn classify(&mut self, user_text: &str) -> Result<UserIntent> {
        // Lazy cleanup: if reasoning model has been idle too long, unload to free VRAM.
        if let Some(last) = self.last_escalation {
            if last.elapsed().as_secs() > REASONING_IDLE_SECS {
                if let Some(mut r) = self.reasoning.take() {
                    tracing::info!("Unloading idle reasoning model ({}s)", last.elapsed().as_secs());
                    let _ = r.unload().await;
                }
                self.last_escalation = None;
            }
        }

        let system = build_router_system_prompt();
        let prompt = format_single_turn(&*self.formatter, &system, user_text);
        let response = self.router.generate(&prompt, 200).await?;
        tracing::debug!("Router response: {response}");

        let intent = parse_intent_response(&response)?;

        if intent.confidence < self.confidence_threshold {
            tracing::info!(
                "Low confidence ({:.2}), escalating to reasoning model",
                intent.confidence
            );
            return self.escalate_to_reasoning(user_text, intent).await;
        }

        Ok(intent)
    }

    /// Escalate to 7B reasoning model for complex queries.
    /// Reuses cached reasoning backend on subsequent calls.
    async fn escalate_to_reasoning(
        &mut self,
        user_text: &str,
        router_result: UserIntent,
    ) -> Result<UserIntent> {
        let model_path = Path::new(&self.config.model_dir)
            .join(&self.config.reasoning_model)
            .to_string_lossy()
            .to_string();

        if !Path::new(&model_path).exists() {
            tracing::warn!("Reasoning model not found: {model_path}, using router result");
            return Ok(router_result);
        }

        // Load reasoning model on first use, reuse on subsequent calls.
        if self.reasoning.is_none() {
            tracing::info!("Loading reasoning model for escalation: {model_path}");
            let mut reasoning = AsyncLlmBackend::new(self.config.n_ctx);
            reasoning
                .load(&model_path, self.config.n_gpu_layers)
                .await?;
            self.reasoning = Some(reasoning);
        }

        let reasoning = self.reasoning.as_ref().unwrap();
        self.last_escalation = Some(Instant::now());

        let system = build_reasoning_system_prompt();
        let prompt = format_single_turn(&*self.formatter, &system, user_text);
        let response = reasoning.generate(&prompt, 300).await?;
        tracing::debug!("Reasoning response: {response}");

        parse_intent_response(&response)
    }
}
