#[cfg(feature = "llama")]
use std::num::NonZeroU32;
#[cfg(feature = "llama")]
use std::sync::OnceLock;

use anyhow::Result;
#[cfg(feature = "llama")]
use llama_cpp_2::context::LlamaContext;
#[cfg(feature = "llama")]
use llama_cpp_2::context::params::LlamaContextParams;
#[cfg(feature = "llama")]
use llama_cpp_2::llama_backend::LlamaBackend;
#[cfg(feature = "llama")]
use llama_cpp_2::llama_batch::LlamaBatch;
#[cfg(feature = "llama")]
use llama_cpp_2::model::params::LlamaModelParams;
#[cfg(feature = "llama")]
use llama_cpp_2::model::LlamaModel;
#[cfg(feature = "llama")]
use llama_cpp_2::sampling::LlamaSampler;
#[cfg(feature = "llama")]
use crate::llm::batching::{prompt_chunks, prompt_fits_context};

/// The context's batch size (`with_n_batch`) and the size of the chunks a
/// prompt is decoded in (S6): one name, so the two cannot drift apart.
#[cfg(feature = "llama")]
const N_BATCH: usize = 2048;

/// Per-model sampling parameters. Different model families perform best with
/// different settings (e.g. Qwen 3.5 wants higher temperature and top_k).
#[derive(Debug, Clone)]
pub struct SamplingConfig {
    pub temperature: f32,
    pub top_p: f32,
    /// 0 = disabled (no top-k filtering).
    pub top_k: i32,
    pub presence_penalty: f32,
    pub seed: u32,
}

impl Default for SamplingConfig {
    fn default() -> Self {
        Self {
            temperature: 0.7,
            top_p: 0.9,
            top_k: 0,
            presence_penalty: 0.0,
            seed: 42,
        }
    }
}

/// Global llama.cpp backend — initialized once, never freed until process exit.
/// llama_backend_init() is a global operation; calling it twice or freeing it
/// while models are live causes crashes.
#[cfg(feature = "llama")]
static LLAMA_BACKEND: OnceLock<LlamaBackend> = OnceLock::new();

#[cfg(feature = "llama")]
fn get_or_init_backend() -> Result<&'static LlamaBackend> {
    Ok(LLAMA_BACKEND.get_or_init(|| {
        LlamaBackend::init().expect("Failed to init llama backend")
    }))
}

/// Synchronous llama.cpp backend. Wraps model + cached context.
///
/// Field order matters: Rust drops fields in declaration order.
/// `ctx` must drop before `model`.
#[cfg(feature = "llama")]
pub struct LlamaCppBackend {
    // SAFETY: `ctx` borrows `model` via a transmuted `'static` lifetime.
    // This is sound because `ctx` is declared first, so it drops before `model`.
    ctx: Option<LlamaContext<'static>>,
    model: LlamaModel,
}

// SAFETY: LlamaCppBackend is only accessed through Mutex in AsyncLlmBackend,
// ensuring exclusive access. The underlying llama_context raw pointer is safe
// to move between threads when not accessed concurrently.
#[cfg(feature = "llama")]
unsafe impl Send for LlamaCppBackend {}

#[cfg(feature = "llama")]
impl LlamaCppBackend {
    /// Load a GGUF model file. `n_gpu_layers` controls GPU offload (99 = all layers).
    /// Creates and caches a `LlamaContext` so the KV cache is allocated once.
    pub fn load(path: &str, n_gpu_layers: i32, n_ctx: u32) -> Result<Self> {
        // MODELTRUST-002: refuse to load a model whose bytes fail integrity
        // verification (pinned-manifest mismatch, or a TOFU model that changed
        // since first use). This is the single choke point for every GGUF load.
        // M3: the guard is held across the load — llama re-opens the file by
        // path, so without it a writer could swap the bytes between the hash
        // and the load.
        let (_pinned_format, load_guard) = crate::model_integrity::verify_path_guarded(path)?;
        let backend = get_or_init_backend()?;

        let model_params = LlamaModelParams::default().with_n_gpu_layers(n_gpu_layers as u32);

        let model = LlamaModel::load_from_file(backend, path, &model_params)
            .map_err(|e| anyhow::anyhow!("Failed to load model: {:?}", e))?;

        // M3: the bytes llama just consumed must be the verified bytes.
        load_guard.confirm()?;

        // Create context once during load — avoids MB-scale KV cache re-allocation per generate().
        // Disable flash attention to avoid ggml symbol conflict with whisper-rs-sys.
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(n_ctx))
            .with_n_batch(N_BATCH as u32)
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
    pub fn generate(&mut self, prompt: &str, max_tokens: u32, sampling: &SamplingConfig) -> Result<String> {
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

        // S6: refuse, in plain words, a prompt that leaves no room for the
        // answer, before anything is added or decoded.
        prompt_fits_context(tokens_list.len(), max_tokens, ctx.n_ctx()).map_err(|e| anyhow::anyhow!(e))?;
        if tokens_list.is_empty() {
            // No chunk would be decoded, so there would be no logits to sample from.
            anyhow::bail!("The prompt is empty: there is nothing to generate from.");
        }

        // S6: decode the prompt in chunks of at most the batch size, in order,
        // each token at its absolute position; only the prompt's last token asks
        // for logits. llama.cpp may clamp the context's batch below N_BATCH (to
        // n_ctx), so a chunk never exceeds what the context accepts.
        let mut batch = LlamaBatch::new(N_BATCH, 1);
        for chunk in prompt_chunks(tokens_list.len(), N_BATCH.min(ctx.n_batch() as usize)) {
            batch.clear();
            let tokens = &tokens_list[chunk.start..chunk.start + chunk.len];
            for (k, token) in tokens.iter().enumerate() {
                let pos = i32::try_from(chunk.start + k)
                    .map_err(|_| anyhow::anyhow!("The prompt is too long for the model."))?;
                let logits = chunk.logits_on_last && k + 1 == chunk.len;
                batch
                    .add(*token, pos, &[0], logits)
                    .map_err(|e| anyhow::anyhow!("Batch add failed: {:?}", e))?;
            }
            ctx.decode(&mut batch)
                .map_err(|e| anyhow::anyhow!("Prompt decode failed: {:?}", e))?;
        }

        let mut samplers: Vec<LlamaSampler> = vec![LlamaSampler::temp(sampling.temperature)];
        if sampling.top_k > 0 {
            samplers.push(LlamaSampler::top_k(sampling.top_k));
        }
        samplers.push(LlamaSampler::top_p(sampling.top_p, 1));
        if sampling.presence_penalty > 0.0 {
            samplers.push(LlamaSampler::penalties(
                256,   // penalty_last_n
                1.0,   // penalty_repeat (disabled)
                0.0,   // penalty_freq (disabled)
                sampling.presence_penalty,
            ));
        }
        samplers.push(LlamaSampler::dist(sampling.seed));
        let mut sampler = LlamaSampler::chain_simple(samplers);

        let mut decoder = encoding_rs::UTF_8.new_decoder();
        let mut output = String::new();
        // Generation continues after the whole prompt, not after the last chunk.
        let mut n_cur = i32::try_from(tokens_list.len())
            .map_err(|_| anyhow::anyhow!("The prompt is too long for the model."))?;

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

/// Built without the `llama` feature (the Android build, K12: no AI on mobile).
/// No llama.cpp is linked; loading a model fails with a clear error, so the
/// backend can never hold a model.
#[cfg(not(feature = "llama"))]
pub struct LlamaCppBackend {
    never: std::convert::Infallible,
}

#[cfg(not(feature = "llama"))]
impl LlamaCppBackend {
    pub fn load(_path: &str, _n_gpu_layers: i32, _n_ctx: u32) -> Result<Self> {
        anyhow::bail!("local AI is not available in this build (built without the `llama` feature)")
    }

    pub fn generate(&mut self, _prompt: &str, _max_tokens: u32, _sampling: &SamplingConfig) -> Result<String> {
        match self.never {}
    }
}
