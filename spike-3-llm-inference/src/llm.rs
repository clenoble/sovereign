use std::io::Write;
use std::num::NonZeroU32;

use anyhow::{Context, Result};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;

use crate::backend::ModelBackend;

pub struct LlamaCppBackend {
    // Field order matters: Rust drops fields in declaration order.
    // model must drop before backend (backend calls llama_backend_free).
    model: LlamaModel,
    backend: LlamaBackend,
    n_ctx: u32,
}

impl ModelBackend for LlamaCppBackend {
    fn load(path: &str, n_gpu_layers: u32, n_ctx: u32) -> Result<Self> {
        let backend = LlamaBackend::init().context("Failed to init llama backend")?;

        let model_params = LlamaModelParams::default().with_n_gpu_layers(n_gpu_layers);

        let model = LlamaModel::load_from_file(&backend, path, &model_params)
            .map_err(|e| anyhow::anyhow!("Failed to load model: {:?}", e))?;

        Ok(Self {
            backend,
            model,
            n_ctx,
        })
    }

    fn generate(&mut self, prompt: &str, max_tokens: u32) -> Result<String> {
        self.run_inference(prompt, max_tokens, true)
    }

    fn unload(self) {
        // Drop order: model first (references backend), then backend.
        // Rust drops fields in declaration order, which is correct here.
        drop(self);
    }
}

impl LlamaCppBackend {
    /// Generate without printing tokens to stdout (for benchmarking)
    pub fn generate_silent(&mut self, prompt: &str, max_tokens: u32) -> Result<String> {
        self.run_inference(prompt, max_tokens, false)
    }

    fn run_inference(&mut self, prompt: &str, max_tokens: u32, print: bool) -> Result<String> {
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(self.n_ctx))
            .with_n_batch(512);

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
            if print {
                print!("{piece}");
                std::io::stdout().flush().ok();
            }

            batch.clear();
            batch
                .add(token, n_cur, &[0], true)
                .map_err(|e| anyhow::anyhow!("Batch add failed: {:?}", e))?;
            n_cur += 1;

            ctx.decode(&mut batch)
                .map_err(|e| anyhow::anyhow!("Decode failed: {:?}", e))?;
        }
        if print {
            println!();
        }

        Ok(output)
    }
}
