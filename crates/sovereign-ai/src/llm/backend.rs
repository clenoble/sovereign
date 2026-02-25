use std::num::NonZeroU32;
use std::sync::OnceLock;

use anyhow::Result;
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::sampling::LlamaSampler;

/// Global llama.cpp backend — initialized once, never freed until process exit.
/// llama_backend_init() is a global operation; calling it twice or freeing it
/// while models are live causes crashes.
static LLAMA_BACKEND: OnceLock<LlamaBackend> = OnceLock::new();

fn get_or_init_backend() -> Result<&'static LlamaBackend> {
    Ok(LLAMA_BACKEND.get_or_init(|| {
        LlamaBackend::init().expect("Failed to init llama backend")
    }))
}

/// Synchronous llama.cpp backend. Wraps model + cached context.
///
/// Field order matters: Rust drops fields in declaration order.
/// `ctx` must drop before `model`.
pub struct LlamaCppBackend {
    // SAFETY: `ctx` borrows `model` via a transmuted `'static` lifetime.
    // This is sound because `ctx` is declared first, so it drops before `model`.
    ctx: Option<LlamaContext<'static>>,
    model: LlamaModel,
}

// SAFETY: LlamaCppBackend is only accessed through Mutex in AsyncLlmBackend,
// ensuring exclusive access. The underlying llama_context raw pointer is safe
// to move between threads when not accessed concurrently.
unsafe impl Send for LlamaCppBackend {}

impl LlamaCppBackend {
    /// Load a GGUF model file. `n_gpu_layers` controls GPU offload (99 = all layers).
    /// Creates and caches a `LlamaContext` so the KV cache is allocated once.
    pub fn load(path: &str, n_gpu_layers: i32, n_ctx: u32) -> Result<Self> {
        let backend = get_or_init_backend()?;

        let model_params = LlamaModelParams::default().with_n_gpu_layers(n_gpu_layers as u32);

        let model = LlamaModel::load_from_file(backend, path, &model_params)
            .map_err(|e| anyhow::anyhow!("Failed to load model: {:?}", e))?;

        // Create context once during load — avoids MB-scale KV cache re-allocation per generate().
        // Disable flash attention to avoid ggml symbol conflict with whisper-rs-sys.
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(n_ctx))
            .with_n_batch(2048)
            .with_flash_attention_policy(0);

        let ctx = model
            .new_context(backend, ctx_params)
            .map_err(|e| anyhow::anyhow!("Failed to create context: {:?}", e))?;

        // SAFETY: `ctx` borrows `model`, but both live in this struct.
        // `ctx` is declared before `model`, so Rust drops it first — the borrow is always valid.
        let ctx: LlamaContext<'static> = unsafe { std::mem::transmute(ctx) };

        Ok(Self {
            ctx: Some(ctx),
            model,
        })
    }

    /// Generate text from a prompt. Reuses the cached context (clears KV cache between calls).
    /// Not suitable for direct async use — wrap with spawn_blocking.
    pub fn generate(&mut self, prompt: &str, max_tokens: u32) -> Result<String> {
        let ctx = self
            .ctx
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Context not initialized"))?;

        // Clear KV cache from previous generation — cheap compared to re-creating the context.
        ctx.clear_kv_cache();

        let tokens_list = self
            .model
            .str_to_token(prompt, llama_cpp_2::model::AddBos::Never)
            .map_err(|e| anyhow::anyhow!("Tokenization failed: {:?}", e))?;

        let mut batch = LlamaBatch::new(2048, 1);
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
            let token = sampler.sample(ctx, batch.n_tokens() - 1);
            sampler.accept(token);

            if self.model.is_eog_token(token) {
                break;
            }

            // Try with special=true so control/special tokens are rendered rather
            // than returning UnknownTokenType. If decoding still fails, skip the
            // token instead of aborting the entire generation.
            match self.model.token_to_piece(token, &mut decoder, true, None) {
                Ok(piece) => output.push_str(&piece),
                Err(e) => {
                    tracing::debug!("Skipping undecodable token {}: {:?}", token.0, e);
                    continue;
                }
            }

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
