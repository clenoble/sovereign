use std::num::NonZeroU32;

use anyhow::{Context, Result};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;

/// Synchronous llama.cpp backend. Wraps model + backend with correct drop order.
pub struct LlamaCppBackend {
    // Field order matters: Rust drops fields in declaration order.
    // model must drop before backend (backend calls llama_backend_free).
    model: LlamaModel,
    backend: LlamaBackend,
    n_ctx: u32,
}

impl LlamaCppBackend {
    /// Load a GGUF model file. `n_gpu_layers` controls GPU offload (99 = all layers).
    pub fn load(path: &str, n_gpu_layers: i32, n_ctx: u32) -> Result<Self> {
        let backend = LlamaBackend::init().context("Failed to init llama backend")?;

        let model_params = LlamaModelParams::default().with_n_gpu_layers(n_gpu_layers as u32);

        let model = LlamaModel::load_from_file(&backend, path, &model_params)
            .map_err(|e| anyhow::anyhow!("Failed to load model: {:?}", e))?;

        Ok(Self {
            model,
            backend,
            n_ctx,
        })
    }

    /// Generate text from a prompt. Not suitable for direct async use — wrap with spawn_blocking.
    pub fn generate(&mut self, prompt: &str, max_tokens: u32) -> Result<String> {
        // Disable flash attention to avoid ggml symbol conflict with whisper-rs-sys
        // (both crates embed ggml; /FORCE:MULTIPLE merges symbols, and the flash-attn
        // code paths are incompatible between the two versions)
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(self.n_ctx))
            .with_n_batch(512)
            .with_flash_attention_policy(0);

        let mut ctx = self
            .model
            .new_context(&self.backend, ctx_params)
            .map_err(|e| anyhow::anyhow!("Failed to create context: {:?}", e))?;

        let tokens_list = self
            .model
            .str_to_token(prompt, AddBos::Always)
            .map_err(|e| anyhow::anyhow!("Tokenization failed: {:?}", e))?;

        let mut batch = LlamaBatch::new(512, 1);
        let last_index = tokens_list.len() as i32 - 1;
        for (i, token) in (0_i32..).zip(tokens_list.into_iter()) {
            batch
                .add(token, i, &[0], i == last_index)
                .map_err(|e| anyhow::anyhow!("Batch add failed: {:?}", e))?;
        }

        ctx.decode(&mut batch)
            .map_err(|e| anyhow::anyhow!("Initial decode failed: {:?}", e))?;

        let mut sampler = LlamaSampler::chain_simple([
            LlamaSampler::temp(0.7),
            LlamaSampler::top_p(0.9, 1),
            LlamaSampler::dist(42),
        ]);

        let mut decoder = encoding_rs::UTF_8.new_decoder();
        let mut output = String::new();
        let mut n_cur = batch.n_tokens();

        for _ in 0..max_tokens {
            let token = sampler.sample(&ctx, batch.n_tokens() - 1);
            sampler.accept(token);

            if self.model.is_eog_token(token) {
                break;
            }

            let piece = self
                .model
                .token_to_piece(token, &mut decoder, false, None)
                .map_err(|e| anyhow::anyhow!("Detokenize failed: {:?}", e))?;

            output.push_str(&piece);

            batch.clear();
            batch
                .add(token, n_cur, &[0], true)
                .map_err(|e| anyhow::anyhow!("Batch add failed: {:?}", e))?;
            n_cur += 1;

            ctx.decode(&mut batch)
                .map_err(|e| anyhow::anyhow!("Decode failed: {:?}", e))?;
        }

        Ok(output)
    }
}
