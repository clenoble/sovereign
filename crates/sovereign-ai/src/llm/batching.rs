//! How a prompt is fed to the model (v0.0.12 assistant lot, amendment S6,
//! claude-coord 0160): pure functions, available without the `llama`
//! feature, so the plan is tested without a model.
//!
//! A prompt longer than the context's batch is decoded in successive chunks
//! of at most `n_batch` tokens, positions continuing across chunks; only the
//! prompt's last token asks for logits. A prompt that leaves no room for the
//! answer in the window is refused before anything is decoded.

/// One chunk of a prompt's decode plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PromptChunk {
    /// Index of the chunk's first token in the prompt (= its first position).
    pub start: usize,
    /// Number of tokens in the chunk.
    pub len: usize,
    /// Whether the chunk's last token asks for logits.
    pub logits_on_last: bool,
}

/// The plan for decoding a prompt of `n_tokens` tokens in batches of at most
/// `n_batch`: chunks in order, contiguous from 0, each `1..=n_batch` long,
/// every chunk but the last exactly `n_batch` long; only the last asks for
/// logits, on the prompt's last token. Empty prompt: no chunk.
/// `n_batch == 0` is treated as 1.
pub fn prompt_chunks(n_tokens: usize, n_batch: usize) -> Vec<PromptChunk> {
    let n_batch = n_batch.max(1);
    let mut plan = Vec::with_capacity(n_tokens.div_ceil(n_batch));
    let mut start = 0;
    while start < n_tokens {
        let len = n_batch.min(n_tokens - start);
        plan.push(PromptChunk { start, len, logits_on_last: start + len == n_tokens });
        start += len;
    }
    plan
}

/// Whether a prompt of `n_prompt_tokens` leaves room for `max_tokens` of
/// answer in a window of `n_ctx`: `Ok` exactly when
/// `n_prompt_tokens + max_tokens <= n_ctx`, computed without overflow. The
/// error is a plain sentence naming the prompt's token count and the window.
pub fn prompt_fits_context(n_prompt_tokens: usize, max_tokens: u32, n_ctx: u32) -> Result<(), String> {
    let room = (n_ctx as u64).checked_sub(max_tokens as u64);
    match room {
        Some(room) if (n_prompt_tokens as u64) <= room => Ok(()),
        _ => Err(format!(
            "The prompt is too long for the model: {n_prompt_tokens} tokens, plus {max_tokens} \
             reserved for the answer, exceed the {n_ctx}-token window."
        )),
    }
}
