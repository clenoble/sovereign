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
    /// Test-only seam: a scripted model answering `generate` in place of
    /// llama.cpp (prompt, max_tokens) → text. Never compiled outside tests
    /// (this crate's, or a host's through the `test-support` feature).
    #[cfg(any(test, feature = "test-support"))]
    fake: Option<FakeGenerate>,
}

/// Test-only seam: the function a fake backend answers `generate` with.
#[cfg(any(test, feature = "test-support"))]
pub type FakeGenerate = Arc<dyn Fn(&str, u32) -> Result<String> + Send + Sync>;

impl AsyncLlmBackend {
    pub fn new(n_ctx: u32) -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
            n_ctx,
            sampling: Arc::new(Mutex::new(SamplingConfig::default())),
            #[cfg(any(test, feature = "test-support"))]
            fake: None,
        }
    }

    /// Test-only seam: a backend with window `n_ctx` whose `generate` is
    /// answered by `fake` (no model is loaded or needed).
    #[cfg(any(test, feature = "test-support"))]
    pub fn fake(n_ctx: u32, fake: FakeGenerate) -> Self {
        let mut b = Self::new(n_ctx);
        b.fake = Some(fake);
        b
    }

    /// The model's context window, in tokens (`AiConfig::n_ctx`): what one
    /// call's prompt and answer budget must fit in (v0.0.12 item 13).
    pub fn n_ctx(&self) -> u32 {
        self.n_ctx
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
        #[cfg(any(test, feature = "test-support"))]
        if let Some(fake) = &self.fake {
            return fake(prompt, max_tokens);
        }
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
