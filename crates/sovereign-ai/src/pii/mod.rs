//! PII detection pipeline.
//!
//! Three stages, each producing a uniform [`Finding`]:
//!   1. [`regex`]            — deterministic, synchronous, confidence 1.0.
//!   2. [`ner`]              — LLM-NER for free-form names, orgs, addresses.
//!                             Confidence 0.0–1.0 from the model.
//!   3. [`entity_disambig`]  — auto-link a finding to an `Entity` by domain,
//!                             contact lookup, or fuzzy name match.
//!
//! The [`pipeline`] module wires the three stages together and produces the
//! per-source result that the ingest hooks (step 4) commit to the database.
//!
//! See `doc/plans/pii-management-dashboard.md` for the design.

pub mod entity_disambig;
pub mod ner;
pub mod regex;

pub use sovereign_db::schema::PiiKind;
pub use sovereign_skills::skills::pii_detector::Locale;

/// A single PII finding from any stage of the pipeline.
///
/// Spans are UTF-8 byte offsets into the source text; samples are the
/// matched substring for traceability and review-queue display. The kind
/// is the typed [`PiiKind`] used by the schema, not the string-tagged
/// representation that lives inside the regex layer in `sovereign-skills`.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub kind: PiiKind,
    pub start: usize,
    pub end: usize,
    pub sample: String,
    /// 1.0 for regex, 0.0–1.0 for LLM-NER. Below the pipeline's review
    /// threshold (default 0.7) the finding is staged as `Unreviewed`
    /// rather than committed eagerly.
    pub confidence: f32,
}
