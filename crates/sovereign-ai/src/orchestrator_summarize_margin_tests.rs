//! v0.0.12 assistant lot, amendment to S2 (`doc/plans/V0012-lot-assistant.md`,
//! "Amendment after PANDA2's second position", claude-coord 0160 point 2):
//! **S2's measure takes a 15 % margin.** The estimate `ceil(chars /
//! chars_per_token)` undercounts accented, CJK or emoji-heavy text. A call now
//! fits if
//!
//! ```text
//! ceil(1.15 × chars(prompt) / chars_per_token) + answer <= n_ctx
//! ```
//!
//! with the formatter's chars-per-token ratio and `answer` the call's answer
//! budget (`max_tokens`). Every S1–S5 guarantee is otherwise unchanged.
//!
//! Tested end to end through the harness's fake model (it records each call's
//! prompt and budget): the prompt Summarize would send whole is placed exactly
//! at the new bound (one call), one character beyond it (no longer one call),
//! and between the new bound and the old one (fitted before, cut now). The
//! whole-document prompt's overhead (system text, chat template) is measured
//! from a first summarize of a short document, not copied from the code.
//!
//! The bound is computed in integers, 1.15 / 3.5 = 23 / 70, and the windows
//! are chosen so that it is never at an exact integer (a float implementation
//! cannot fall on the other side of it by rounding).
//!
//! Written before the code by another author; the implementer does not edit
//! this file. Compiles against today's API (plus the harness's fake model).

use super::assistant_harness::*;
use super::*;

/// ChatML's chars-per-token (the default `AiConfig` format), checked against
/// the orchestrator's formatter.
const CPT: f64 = 3.5;

async fn check_cpt(w: &World) {
    let c = w.orch.classifier.lock().await.formatter.chars_per_token();
    assert!((c - CPT).abs() < 1e-9, "these tests assume the default formatter (chars/token {CPT}), got {c}");
}

/// The amended estimate, exactly, for chars/token 3.5: ceil(23·chars / 70).
fn est_tokens_with_margin(chars: usize) -> usize {
    (23 * chars).div_ceil(70)
}

/// The old estimate, for chars/token 3.5: ceil(chars / 3.5) = ceil(2·chars / 7).
fn est_tokens_old(chars: usize) -> usize {
    (2 * chars).div_ceil(7)
}

/// The most characters a prompt can hold under the amended measure.
fn bound_with_margin(n_ctx: u32, answer: u32) -> usize {
    let budget = (n_ctx - answer) as usize;
    // Largest c with ceil(23c/70) <= budget, i.e. 23c <= 70·budget.
    let c = 70 * budget / 23;
    let exact = 70 * budget % 23 == 0;
    assert!(!exact, "the bound {c} falls on an exact integer: pick another window");
    // Neither side of the bound is a knife edge for a float computation.
    assert!((23 * c) % 70 != 0 && (23 * (c + 1)) % 70 != 0, "pick another window");
    c
}

/// The most characters a prompt could hold under the old measure.
fn bound_old(n_ctx: u32, answer: u32) -> usize {
    7 * (n_ctx - answer) as usize / 2
}

fn assert_every_call_fits_with_margin(calls: &[Call], n_ctx: u32) {
    for (i, c) in calls.iter().enumerate() {
        let chars = c.prompt.chars().count();
        let need = est_tokens_with_margin(chars) + c.max_tokens as usize;
        assert!(
            need <= n_ctx as usize,
            "call {i} does not fit the window with the 15 % margin: ceil(1.15 × {chars} / 3.5) = {} + {} answer = {need} > {n_ctx}",
            est_tokens_with_margin(chars),
            c.max_tokens
        );
    }
}

/// Plain filler paragraphs (marker `Q<i>`, ~300 characters, no newline inside)
/// joined by blank lines, cut to exactly `len` characters, ending on a
/// non-space character. Several paragraphs, so the text can be cut in parts.
fn text_of_len(len: usize) -> String {
    const S: [&str; 5] = [
        "The orchard rows run east to west along the slope.",
        "Prune the apple trees in late winter, before the buds.",
        "Bees visit the clover between the rows in spring.",
        "The cider press is cleaned and oiled every autumn.",
        "Fallen fruit is gathered for the compost each week.",
    ];
    let mut paras = vec![];
    let mut total = 0;
    let mut i = 0;
    while total < len + 400 {
        let mut p = format!("Q{i:03} begins.");
        let mut k = i;
        while p.len() < 300 {
            p.push(' ');
            p.push_str(S[k % S.len()]);
            k += 1;
        }
        total += p.len() + 2;
        paras.push(p);
        i += 1;
    }
    let mut t: String = paras.join("\n\n").chars().take(len).collect();
    if t.ends_with(char::is_whitespace) {
        t.pop();
        t.push('.');
    }
    assert_eq!(t.chars().count(), len);
    t
}

async fn stored_doc(w: &World, title: &str, text: &str) {
    let thread = w.thread(&format!("{title} thread")).await;
    let mut d = sovereign_db::schema::Document::new(title.into(), thread, true);
    d.content = text.to_string();
    d.pii_scanned_at = Some(chrono::Utc::now());
    let id = w.store.create_document(d).await.unwrap().id_string().unwrap();
    w.store.update_document_pii_fields(&id, None, None, Some(chrono::Utc::now())).await.unwrap();
}

/// Summarizes a short document in `w` and returns (overhead, answer): the
/// characters the whole-document prompt adds to the text, and the call's
/// answer budget.
async fn probe(w: &World) -> (usize, u32) {
    let text = text_of_len(700);
    stored_doc(w, "Probe note", &text).await;
    let before = w.rec.calls().len();
    w.act("summarize", Some("Probe note")).await;
    let calls = w.rec.calls();
    assert_eq!(calls.len(), before + 1, "a short document is summarized in one call (S1)");
    let c = &calls[before];
    assert!(c.prompt.contains(&text), "the probe's whole text is in its call");
    (c.prompt.chars().count() - text.chars().count(), c.max_tokens)
}

/// The calls since index `from`.
fn calls_since(w: &World, from: usize) -> Vec<Call> {
    w.rec.calls()[from..].to_vec()
}

/// S2 amended, at the bound: a document whose whole prompt is exactly the
/// largest that fits `ceil(1.15 × chars / 3.5) + answer <= n_ctx` is
/// summarized in one call (S1), its whole text in it.
#[tokio::test]
async fn s2m_a_prompt_exactly_at_the_amended_bound_goes_in_one_call() {
    let w = world("s2m-at", 1024, answer("SUMMARY of the orchard")).await;
    check_cpt(&w).await;
    let (overhead, answer_budget) = probe(&w).await;
    let bound = bound_with_margin(w.n_ctx, answer_budget);
    let text = text_of_len(bound - overhead);
    stored_doc(&w, "Orchard notes", &text).await;
    let from = w.rec.calls().len();

    let evs = w.act("summarize", Some("Orchard notes")).await;
    let calls = calls_since(&w, from);
    assert_eq!(calls.len(), 1, "a prompt at the amended bound fits: one call ({} calls)", calls.len());
    assert!(calls[0].prompt.contains(&text), "the whole text in it");
    assert_eq!(
        calls[0].prompt.chars().count(),
        bound,
        "test premise: the whole-document prompt is the text plus a fixed overhead of {overhead}"
    );
    assert_every_call_fits_with_margin(&calls, w.n_ctx);
    assert_eq!(executed(&evs), vec![true], "{}", reply(&evs));
}

/// S2 amended, one character beyond the bound: the whole prompt no longer
/// fits, so the document is not sent in one call; it is summarized in parts,
/// each fitting with the margin.
#[tokio::test]
async fn s2m_one_character_beyond_the_amended_bound_is_no_longer_one_call() {
    let w = world("s2m-beyond", 1024, answer("SUMMARY of the orchard")).await;
    check_cpt(&w).await;
    let (overhead, answer_budget) = probe(&w).await;
    let bound = bound_with_margin(w.n_ctx, answer_budget);
    let text = text_of_len(bound + 1 - overhead);
    stored_doc(&w, "Orchard notes", &text).await;
    let from = w.rec.calls().len();

    let evs = w.act("summarize", Some("Orchard notes")).await;
    let calls = calls_since(&w, from);
    assert!(
        calls.iter().all(|c| !c.prompt.contains(&text)),
        "a prompt one character beyond the amended bound is not sent whole"
    );
    assert!(calls.len() >= 2, "summarized in parts ({} calls)", calls.len());
    assert_every_call_fits_with_margin(&calls, w.n_ctx);
    assert_eq!(executed(&evs), vec![true], "{}", reply(&evs));
}

/// S2 amended, between the bounds: a document whose whole prompt fitted the
/// old measure (`ceil(chars / 3.5) + answer <= n_ctx`) but not the amended
/// one is now cut: not one call, every call fits with the margin, and every
/// paragraph's marker still reaches the model.
#[tokio::test]
async fn s2m_a_prompt_that_fitted_the_old_measure_but_not_the_new_is_now_cut() {
    let w = world("s2m-between", 1024, answer("SUMMARY of the orchard")).await;
    check_cpt(&w).await;
    let (overhead, answer_budget) = probe(&w).await;
    let new_bound = bound_with_margin(w.n_ctx, answer_budget);
    let old_bound = bound_old(w.n_ctx, answer_budget);
    let prompt_len = (new_bound + old_bound) / 2;
    assert!(est_tokens_old(prompt_len) + answer_budget as usize <= w.n_ctx as usize, "fitted before");
    assert!(est_tokens_with_margin(prompt_len) + answer_budget as usize > w.n_ctx as usize, "does not fit now");
    let text = text_of_len(prompt_len - overhead);
    stored_doc(&w, "Orchard notes", &text).await;
    let from = w.rec.calls().len();

    let evs = w.act("summarize", Some("Orchard notes")).await;
    let calls = calls_since(&w, from);
    assert!(calls.iter().all(|c| !c.prompt.contains(&text)), "no longer sent whole");
    assert!(calls.len() >= 2, "summarized in parts ({} calls)", calls.len());
    assert_every_call_fits_with_margin(&calls, w.n_ctx);
    let markers: Vec<String> =
        text.split("\n\n").filter(|p| p.len() > 6).map(|p| p[..4].to_string()).collect();
    for m in &markers {
        assert!(calls.iter().any(|c| c.prompt.contains(&format!("{m} "))), "paragraph {m} reaches the model");
    }
    assert_eq!(executed(&evs), vec![true], "{}", reply(&evs));
}

/// S2 amended, a long document under the default window (4,096): every part
/// and every summary call fits with the margin (the measure the cut uses is
/// the amended one, not only the one-call test).
#[tokio::test]
async fn s2m_every_part_and_summary_fits_with_the_margin_under_the_default_window() {
    let n_ctx = AiConfig::default().n_ctx;
    let w = world("s2m-default", n_ctx, answer("SUMMARY of a part of the orchard notes")).await;
    check_cpt(&w).await;
    let text = text_of_len(3 * bound_old(n_ctx, 200));
    stored_doc(&w, "Orchard archive", &text).await;

    let evs = w.act("summarize", Some("Orchard archive")).await;
    let calls = w.rec.calls();
    assert!(calls.len() >= 3, "a document of ~3 windows is summarized in parts ({} calls)", calls.len());
    assert_every_call_fits_with_margin(&calls, w.n_ctx);
    assert_eq!(executed(&evs), vec![true], "{}", reply(&evs));
}
