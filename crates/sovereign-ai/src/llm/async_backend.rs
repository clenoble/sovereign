use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use sovereign_core::interfaces::ModelBackend;

use super::backend::{LlamaCppBackend, SamplingConfig};

/// Async wrapper around `LlamaCppBackend` using `spawn_blocking`.
///
/// `LlamaModel` is `Send` and `LlamaBackend` is `Send+Sync`,
/// so `Arc<Mutex<Option<LlamaCppBackend>>>` is safe to share with `spawn_blocking`.
pub struct AsyncLlmBackend {
    inner: Arc<Mutex<Option<LlamaCppBackend>>>,
    n_ctx: u32,
    sampling: Arc<Mutex<SamplingConfig>>,
}

impl AsyncLlmBackend {
    pub fn new(n_ctx: u32) -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
            n_ctx,
            sampling: Arc::new(Mutex::new(SamplingConfig::default())),
        }
    }

    /// Update the sampling config (e.g. after a model hot-swap).
    pub fn set_sampling(&self, config: SamplingConfig) {
        *self.sampling.lock().unwrap() = config;
    }

    /// Hot-swap the loaded model. Works through `&self` via `Arc<Mutex<>>`.
    /// Drops the old model (freeing VRAM) before loading the new one.
    pub async fn swap(&self, model_path: &str, n_gpu_layers: i32) -> Result<()> {
        let path = model_path.to_string();
        let n_ctx = self.n_ctx;
        let inner = self.inner.clone();

        tokio::task::spawn_blocking(move || {
            let mut guard = inner.lock().unwrap();
            *guard = None; // drop old model, free VRAM
            let backend = LlamaCppBackend::load(&path, n_gpu_layers, n_ctx)?;
            *guard = Some(backend);
            Ok(())
        })
        .await?
    }
}

#[async_trait]
impl ModelBackend for AsyncLlmBackend {
    async fn load(&mut self, model_path: &str, n_gpu_layers: i32) -> Result<()> {
        let path = model_path.to_string();
        let n_ctx = self.n_ctx;
        let inner = self.inner.clone();

        tokio::task::spawn_blocking(move || {
            let backend = LlamaCppBackend::load(&path, n_gpu_layers, n_ctx)?;
            let mut guard = inner.lock().unwrap();
            *guard = Some(backend);
            Ok(())
        })
        .await?
    }

    async fn generate(&self, prompt: &str, max_tokens: u32) -> Result<String> {
        let inner = self.inner.clone();
        let sampling = self.sampling.clone();
        let prompt = prompt.to_string();

        tokio::task::spawn_blocking(move || {
            let mut guard = inner.lock().unwrap();
            let sampling = sampling.lock().unwrap().clone();
            let backend = guard
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("Model not loaded"))?;
            backend.generate(&prompt, max_tokens, &sampling)
        })
        .await?
    }

    async fn unload(&mut self) -> Result<()> {
        let inner = self.inner.clone();

        tokio::task::spawn_blocking(move || {
            let mut guard = inner.lock().unwrap();
            *guard = None; // drops LlamaCppBackend
            Ok(())
        })
        .await?
    }
}
