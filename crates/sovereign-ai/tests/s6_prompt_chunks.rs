//! v0.0.12 assistant lot, amendment S6 (`doc/plans/V0012-lot-assistant.md`,
//! "Amendment after PANDA2's second position", claude-coord 0160): **a prompt
//! longer than the batch is decoded in chunks.**
//!
//! Found on the real backend: `LlamaCppBackend::generate` put the whole prompt
//! into one `LlamaBatch::new(2048, 1)` while the context had `n_batch` 2048, so
//! any prompt of 2049 tokens or more failed with "Batch add failed", although
//! it fitted `n_ctx`. `AsyncLlmBackend::fake` has no batch, so no test of the
//! orchestrator could see it. These tests cannot load a model either; they pin
//! the guarantee through two pure functions and a scan of `generate`.
//!
//! ## API named by these tests (to be written; available WITHOUT the `llama`
//! feature, re-exported from `sovereign_ai::llm`)
//!
//! ```ignore
//! #[derive(Debug, Clone, Copy, PartialEq, Eq)]
//! pub struct PromptChunk {
//!     /// Index of the chunk's first token in the prompt (= its first position).
//!     pub start: usize,
//!     /// Number of tokens in the chunk.
//!     pub len: usize,
//!     /// Whether the chunk's last token asks for logits.
//!     pub logits_on_last: bool,
//! }
//!
//! /// The plan for decoding a prompt of `n_tokens` tokens in batches of at most
//! /// `n_batch`: chunks in order, contiguous from 0, each 1..=n_batch long,
//! /// every chunk but the last exactly `n_batch` long; only the last asks for
//! /// logits, on the prompt's last token. Empty prompt: no chunk.
//! /// `n_batch == 0` is treated as 1 (never panics, never loops).
//! pub fn prompt_chunks(n_tokens: usize, n_batch: usize) -> Vec<PromptChunk>;
//!
//! /// Whether a prompt of `n_prompt_tokens` leaves room for `max_tokens` of
//! /// answer in a window of `n_ctx`: Ok iff `n_prompt_tokens + max_tokens <=
//! /// n_ctx` (computed without overflow). The error is a plain sentence that
//! /// names the prompt's token count and the window.
//! pub fn prompt_fits_context(n_prompt_tokens: usize, max_tokens: u32, n_ctx: u32) -> Result<(), String>;
//! ```
//!
//! And in `crates/sovereign-ai/src/llm/backend.rs` (production part, read as
//! text): the llama `generate` calls `prompt_fits_context` before any batch
//! add or decode, decodes the prompt through `prompt_chunks`, sized with the
//! same constant or parameter as the context's `with_n_batch`; no
//! `LlamaBatch::new(2048` remains; and the generation's first position is not
//! taken from the last chunk's batch.
//!
//! Written before the code by another author; the implementer does not edit
//! this file.

use sovereign_ai::llm::{prompt_chunks, prompt_fits_context, PromptChunk};

// ---------------------------------------------------------- the chunk plan --

/// The properties every plan must have, checked for one (n_tokens, n_batch).
fn check_plan(n_tokens: usize, n_batch: usize) {
    let plan: Vec<PromptChunk> = prompt_chunks(n_tokens, n_batch);
    let ctx = format!("prompt_chunks({n_tokens}, {n_batch}) = {plan:?}");
    if n_tokens == 0 {
        assert!(plan.is_empty(), "an empty prompt gives no chunk: {ctx}");
        return;
    }
    assert!(!plan.is_empty(), "a non-empty prompt gives at least one chunk: {ctx}");
    // Every token exactly once, in order, contiguous from 0.
    let mut next = 0usize;
    for (i, c) in plan.iter().enumerate() {
        assert_eq!(c.start, next, "chunk {i} starts where the previous one ended: {ctx}");
        assert!(c.len >= 1, "chunk {i} is not empty: {ctx}");
        assert!(c.len <= n_batch, "chunk {i} holds at most n_batch = {n_batch} tokens: {ctx}");
        next = c.start + c.len;
    }
    assert_eq!(next, n_tokens, "the chunks end at the prompt's last token: {ctx}");
    // Exactly one token asks for logits: the last chunk's last, i.e. the prompt's last.
    let with_logits: Vec<usize> = (0..plan.len()).filter(|&i| plan[i].logits_on_last).collect();
    assert_eq!(with_logits, vec![plan.len() - 1], "only the last chunk asks for logits: {ctx}");
}

/// S6: for every prompt length 0..=3·n and batch sizes 1, 2, 7, the chunks
/// cover every token exactly once, in order, each at most `n_batch` long, and
/// exactly one token (the last) asks for logits; an empty prompt gives none.
#[test]
fn s6_the_chunk_plan_covers_every_token_once_in_order_small_batches() {
    for n_batch in [1usize, 2, 7] {
        for n_tokens in 0..=3 * n_batch {
            check_plan(n_tokens, n_batch);
        }
    }
}

/// S6, the production batch size (2048): lengths around the edges where the
/// bug lived (2048 fitted, 2049 failed) and well past them, up to 3·n + 1.
#[test]
fn s6_the_chunk_plan_holds_around_the_2048_batch() {
    for n_tokens in [0usize, 1, 2, 1000, 2047, 2048, 2049, 3896, 4095, 4096, 4097, 6143, 6144, 6145] {
        check_plan(n_tokens, 2048);
    }
}

/// S6: a prompt that fits one batch is one chunk, the whole prompt, with its
/// logits (decoding is unchanged for every prompt that worked before).
#[test]
fn s6_a_prompt_within_the_batch_is_one_chunk_as_before() {
    for (n_tokens, n_batch) in [(1usize, 1usize), (5, 7), (7, 7), (2047, 2048), (2048, 2048)] {
        assert_eq!(
            prompt_chunks(n_tokens, n_batch),
            vec![PromptChunk { start: 0, len: n_tokens, logits_on_last: true }],
            "prompt_chunks({n_tokens}, {n_batch})"
        );
    }
}

/// S6 (reading, see the report): chunks are full batches but the last, so a
/// long prompt costs ceil(n / n_batch) decodes, not one per token.
#[test]
fn s6_every_chunk_but_the_last_is_a_full_batch() {
    for (n_tokens, n_batch) in [(2049usize, 2048usize), (3896, 2048), (6145, 2048), (20, 7), (21, 7), (5, 2)] {
        let plan = prompt_chunks(n_tokens, n_batch);
        assert_eq!(plan.len(), n_tokens.div_ceil(n_batch), "prompt_chunks({n_tokens}, {n_batch}) = {plan:?}");
        for c in &plan[..plan.len() - 1] {
            assert_eq!(c.len, n_batch, "prompt_chunks({n_tokens}, {n_batch}) = {plan:?}");
        }
    }
}

/// S6: `n_batch == 0` (never passed by the backend) neither panics nor loops;
/// it is treated as 1.
#[test]
fn s6_a_zero_batch_is_treated_as_one() {
    for n_tokens in [0usize, 1, 2, 5] {
        assert_eq!(prompt_chunks(n_tokens, 0), prompt_chunks(n_tokens, 1), "prompt_chunks({n_tokens}, 0)");
    }
}

// ------------------------------------------------- refusal of an oversize --

/// S6: a prompt is decoded when it leaves room for the answer in the window,
/// whatever its length relative to the batch: 3896 + 200 = 4096 fits.
#[test]
fn s6_a_prompt_that_fits_the_window_is_accepted_even_beyond_the_batch() {
    assert_eq!(prompt_fits_context(3896, 200, 4096), Ok(()));
    assert_eq!(prompt_fits_context(2049, 200, 4096), Ok(()), "longer than the 2048 batch, within the window");
    assert_eq!(prompt_fits_context(1, 200, 4096), Ok(()));
    assert_eq!(prompt_fits_context(824, 200, 1024), Ok(()));
}

/// S6: a prompt longer than `n_ctx − max_tokens` is refused with a plain
/// error naming its length and the window — not a llama decode failure.
#[test]
fn s6_a_prompt_beyond_the_window_is_refused_with_a_plain_error() {
    for (n, max, ctx) in [(3897usize, 200u32, 4096u32), (4096, 200, 4096), (10_000, 200, 4096), (825, 200, 1024)] {
        let err = prompt_fits_context(n, max, ctx).expect_err(&format!("{n} + {max} > {ctx} is refused"));
        assert!(err.contains(&n.to_string()), "the error names the prompt's {n} tokens: {err}");
        assert!(err.contains(&ctx.to_string()), "the error names the {ctx}-token window: {err}");
        let lower = err.to_lowercase();
        assert!(!lower.contains("decode") && !lower.contains("batch add"), "a plain error, not a llama one: {err}");
    }
}

/// S6: the check cannot overflow: an answer budget larger than the window, or
/// absurd lengths, are refused without panicking.
#[test]
fn s6_the_fit_check_does_not_overflow() {
    assert!(prompt_fits_context(1, 4096, 4096).is_err(), "no room left for any prompt token");
    assert!(prompt_fits_context(1, 5000, 4096).is_err(), "answer budget beyond the window");
    assert!(prompt_fits_context(1, u32::MAX, 4096).is_err());
    assert!(prompt_fits_context(usize::MAX, 200, 4096).is_err());
    assert!(prompt_fits_context(usize::MAX, u32::MAX, u32::MAX).is_err());
}

// ------------------------------------------------------- scan of generate --

const BACKEND_RS: &str = include_str!("../src/llm/backend.rs");

/// `src` without its inline `#[cfg(test)] mod x { … }` blocks (ended at the
/// first later line that is the `mod` line's indentation followed by `}`, as
/// rustfmt writes it) and without `//` comments (doc comments included). The
/// house style of `sovereign-shell/src/no_gl_windows_tests.rs`.
fn production_code(src: &str) -> String {
    let lines: Vec<&str> = src.lines().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim() == "#[cfg(test)]" {
            let mut j = i + 1;
            while j < lines.len() && lines[j].trim().starts_with("#[") {
                j += 1;
            }
            if j < lines.len() {
                let t = lines[j].trim();
                if t.starts_with("mod ") && t.ends_with('{') {
                    let indent = &lines[j][..lines[j].len() - lines[j].trim_start().len()];
                    let close = format!("{indent}}}");
                    let mut k = j + 1;
                    while k < lines.len() && lines[k].trim_end() != close {
                        k += 1;
                    }
                    i = k + 1;
                    continue;
                }
            }
        }
        out.push_str(lines[i].split("//").next().unwrap());
        out.push('\n');
        i += 1;
    }
    out
}

/// The body (between its braces) of the first `fn generate(` in the llama
/// part of `backend.rs` — the `#[cfg(feature = "llama")] impl LlamaCppBackend`
/// that precedes the no-llama stub.
fn llama_generate_body() -> String {
    let prod = production_code(BACKEND_RS);
    let llama_part = prod.split("#[cfg(not(feature = \"llama\"))]\npub struct LlamaCppBackend").next().unwrap();
    let at = llama_part.find("fn generate(").expect("backend.rs (llama part) has a `fn generate(`");
    let rest = &llama_part[at..];
    let open = rest.find('{').expect("generate has a body");
    let mut depth = 0i32;
    for (k, ch) in rest[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return rest[open + 1..open + k].to_string();
                }
            }
            _ => {}
        }
    }
    panic!("generate's body is not closed");
}

/// The text between the parentheses of the first call `name(` in `code`.
fn call_args<'a>(code: &'a str, name: &str) -> Option<&'a str> {
    let at = code.find(name)? + name.len();
    let rest = &code[at..];
    let mut depth = 1i32;
    for (k, ch) in rest.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&rest[..k]);
                }
            }
            _ => {}
        }
    }
    None
}

/// The identifiers in `expr`, without casts, `self` and integer types.
fn identifiers(expr: &str) -> Vec<String> {
    expr.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty() && !w.chars().next().unwrap().is_ascii_digit())
        .filter(|w| !matches!(*w, "as" | "self" | "Self" | "u32" | "usize" | "i32" | "u64" | "NonZeroU32" | "new" | "Some"))
        .map(str::to_string)
        .collect()
}

/// S6: `generate` decodes the prompt through the chunk plan — the call is in
/// `generate`'s own body, not merely defined in the file.
#[test]
fn s6_scan_generate_decodes_the_prompt_through_the_chunk_plan() {
    let body = llama_generate_body();
    assert!(body.contains("prompt_chunks("), "generate does not call prompt_chunks:\n{body}");
}

/// S6: no `LlamaBatch` is sized 2048 (or any literal above 1) independently of
/// the plan; the context's `with_n_batch` takes a named constant or parameter,
/// and that same name sizes the chunk plan in `generate`.
#[test]
fn s6_scan_one_batch_size_for_the_context_and_the_chunks() {
    let prod = production_code(BACKEND_RS);
    assert!(!prod.contains("LlamaBatch::new(2048"), "a `LlamaBatch::new(2048` literal remains in backend.rs");
    // Every LlamaBatch sized by a literal is sized 1 (one generated token at a time).
    let mut rest = prod.as_str();
    while let Some(at) = rest.find("LlamaBatch::new(") {
        let args = call_args(&rest[at..], "LlamaBatch::new(").expect("LlamaBatch::new( is closed");
        let first = args.split(',').next().unwrap().trim();
        if let Ok(n) = first.parse::<u64>() {
            assert!(n <= 1, "LlamaBatch::new({args}) is sized by a literal, independently of the chunk plan");
        }
        rest = &rest[at + "LlamaBatch::new(".len()..];
    }
    let n_batch = call_args(&prod, "with_n_batch(").expect("the context sets with_n_batch");
    assert!(
        n_batch.trim().parse::<u64>().is_err(),
        "with_n_batch({n_batch}) is a literal: the batch size must be one named constant or parameter"
    );
    let names = identifiers(n_batch);
    assert!(!names.is_empty(), "with_n_batch({n_batch}) names no constant or parameter");
    let body = llama_generate_body();
    let plan_args = call_args(&body, "prompt_chunks(").expect("generate calls prompt_chunks");
    let plan_names = identifiers(plan_args);
    assert!(
        names.iter().any(|n| plan_names.contains(n)),
        "the chunk plan prompt_chunks({plan_args}) is not sized by what the context's with_n_batch({n_batch}) is"
    );
}

/// S6: an oversize prompt is refused before decoding: `generate` calls
/// `prompt_fits_context` before any batch add or decode.
#[test]
fn s6_scan_generate_checks_the_window_before_decoding() {
    let body = llama_generate_body();
    let check = body.find("prompt_fits_context(").unwrap_or_else(|| panic!("generate does not call prompt_fits_context:\n{body}"));
    for op in [".decode(", ".add("] {
        if let Some(at) = body.find(op) {
            assert!(check < at, "generate calls prompt_fits_context after its first `{op}`");
        }
    }
}

/// S6, "positions continuing across chunks; generation then continues as
/// today" (reading, see the report): the first generated token's position is
/// the prompt's length, not the size of the last chunk's batch. Today
/// `let mut n_cur = batch.n_tokens();` is right only because there is one
/// batch; with chunks it would put generation at the last chunk's length and
/// overwrite the prompt's positions. No `let` in `generate` takes its value
/// from a batch's `n_tokens()`.
#[test]
fn s6_scan_generation_does_not_restart_at_the_last_chunks_size() {
    let body = llama_generate_body();
    for line in body.lines() {
        let t = line.trim();
        assert!(
            !(t.starts_with("let ") && t.ends_with(".n_tokens();")),
            "generate takes a position from a batch's size: `{t}` (with chunks, that is the last chunk's size)"
        );
    }
}
