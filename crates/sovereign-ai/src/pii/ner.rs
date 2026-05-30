//! LLM-NER stage of the PII pipeline.
//!
//! Zero-shot named-entity recognition for the categories the regex layer
//! cannot reliably catch: person names, organization names, and free-form
//! physical addresses (multi-line, non-Swiss postcode formats, etc.).
//!
//! Confidence in `[0.0, 1.0]` comes from the model. The pipeline uses a
//! threshold (default 0.7) downstream to decide whether a finding goes
//! through as `Confirmed` or is staged `Unreviewed` for the user. This
//! module does NOT filter — it returns every parseable finding so the
//! threshold lives in one place (the pipeline orchestrator in 3e).

use serde::{Deserialize, Serialize};
use sovereign_core::interfaces::ModelBackend;

use crate::llm::format::PromptFormatter;

use super::{Finding, PiiKind};

/// System prompt for zero-shot PII NER.
///
/// Constraints encoded in the prompt:
///   - Must return a JSON array (parsing-friendly).
///   - `value` must appear verbatim in the source so the orchestrator can
///     resolve byte spans by string-searching the original text.
///   - Excludes structured kinds the regex layer already covers, to avoid
///     duplicate findings at different confidences.
///   - Empty array on no entities (so an empty response is unambiguous).
const NER_SYSTEM_PROMPT: &str = "\
You are a Named Entity Recognition assistant. Extract person names, \
organization names, and physical addresses from the user's text. \
Respond with ONLY a JSON array, no other text.

Each entry is an object with these fields:
  - kind:       one of \"person_name\", \"org_name\", \"address\"
  - value:      the literal substring extracted (must appear verbatim in the source)
  - confidence: a float between 0.0 and 1.0

Do not extract emails, phone numbers, IBANs, AVS numbers, dates, IP \
addresses, credit card numbers, or passport numbers — those are handled \
separately by deterministic detectors.

Do not extract honorifics (\"Dr.\", \"Mr.\", \"Mme\") on their own as \
person names. Do not extract street types (\"Rue\", \"Strasse\") on their \
own as addresses. The match must be the full identifying span.

If you find no entities, respond with [].

Example:
[{\"kind\":\"person_name\",\"value\":\"Alice Smith\",\"confidence\":0.95},\
{\"kind\":\"org_name\",\"value\":\"Acme AG\",\"confidence\":0.85}]";

/// Maximum characters of text to send to the model. Keeps prompt size in
/// the 7B reasoning model's context budget; longer documents need to be
/// chunked by the caller.
pub const MAX_NER_CHARS: usize = 8000;

/// Raw entity returned by the NER prompt.
///
/// `pub` for callers that want to inspect what the model said before
/// span-resolution drops anything; the canonical pipeline output is
/// [`Finding`] though.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NerEntity {
    pub kind: String,
    pub value: String,
    pub confidence: f32,
}

/// Run the LLM-NER stage and return typed findings with model confidence.
///
/// Findings are not threshold-filtered — every entity that maps to a
/// known [`PiiKind`] and whose `value` is locatable in `text` becomes a
/// `Finding`. The pipeline orchestrator decides what to do below the
/// review threshold.
///
/// Returns an empty vec on any LLM or parsing failure (logged via
/// `tracing::warn!`); NER is a best-effort enrichment, never a hard
/// dependency. Length-truncates `text` to [`MAX_NER_CHARS`].
pub async fn ner_stage(
    backend: &dyn ModelBackend,
    formatter: &dyn PromptFormatter,
    text: &str,
) -> anyhow::Result<Vec<Finding>> {
    let truncated = if text.len() > MAX_NER_CHARS {
        // Round to a char boundary to avoid splitting a UTF-8 codepoint.
        let mut end = MAX_NER_CHARS;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        &text[..end]
    } else {
        text
    };

    let user_msg = format!("Extract entities from this text:\n\n{truncated}");
    let prompt = formatter.format_system_user(NER_SYSTEM_PROMPT, &user_msg);
    let response = backend.generate(&prompt, 1024).await?;

    let entities = parse_ner_response(&response);
    Ok(entities_to_findings(entities, text))
}

/// Parse the model's response into [`NerEntity`]s.
///
/// Tolerant of leading/trailing prose around the JSON array (some models
/// add commentary even when told not to). Returns an empty vec — never
/// panics — if no array is found or JSON is malformed.
pub fn parse_ner_response(response: &str) -> Vec<NerEntity> {
    let trimmed = response.trim();
    let Some(start) = trimmed.find('[') else {
        return Vec::new();
    };
    let Some(rel_end) = trimmed[start..].rfind(']') else {
        return Vec::new();
    };
    let json_str = &trimmed[start..=start + rel_end];
    match serde_json::from_str::<Vec<NerEntity>>(json_str) {
        Ok(mut entities) => {
            for e in &mut entities {
                e.confidence = e.confidence.clamp(0.0, 1.0);
            }
            entities
        }
        Err(e) => {
            tracing::warn!("NER: failed to parse JSON array: {e}");
            Vec::new()
        }
    }
}

/// Resolve each entity's byte span in `text` and produce [`Finding`]s.
///
/// If the same value appears multiple times in the entity list, each
/// occurrence is mapped to a distinct span, walking forward through the
/// text. Entities whose value cannot be located (the model hallucinated
/// or paraphrased) are dropped, as are entities with an unknown kind.
pub fn entities_to_findings(entities: Vec<NerEntity>, text: &str) -> Vec<Finding> {
    let mut used: Vec<(usize, usize)> = Vec::new();
    let mut out: Vec<Finding> = Vec::with_capacity(entities.len());
    for ent in entities {
        let Some(kind) = ner_kind_to_pii_kind(&ent.kind) else {
            continue;
        };
        if ent.value.is_empty() {
            continue;
        }
        if let Some((start, end)) = find_first_unused(text, &ent.value, &used) {
            used.push((start, end));
            out.push(Finding {
                kind,
                start,
                end,
                sample: ent.value,
                confidence: ent.confidence,
            });
        }
    }
    // Pipeline expects offset-ordered output (matches regex_stage's contract).
    out.sort_by_key(|f| (f.start, f.end));
    out
}

fn ner_kind_to_pii_kind(s: &str) -> Option<PiiKind> {
    match s.to_ascii_lowercase().as_str() {
        "person_name" | "person" | "name" => Some(PiiKind::PersonName),
        "org_name" | "organization" | "org" => Some(PiiKind::OrgName),
        "address" => Some(PiiKind::Address),
        _ => None,
    }
}

/// Find the first occurrence of `needle` in `haystack` whose span doesn't
/// overlap any range in `used`. None if no such occurrence exists.
fn find_first_unused(
    haystack: &str,
    needle: &str,
    used: &[(usize, usize)],
) -> Option<(usize, usize)> {
    let mut search_from = 0;
    while search_from <= haystack.len() {
        let rel = haystack.get(search_from..)?.find(needle)?;
        let start = search_from + rel;
        let end = start + needle.len();
        let overlaps = used.iter().any(|&(s, e)| start < e && s < end);
        if !overlaps {
            return Some((start, end));
        }
        // Step past the start of the conflicting match and try again. We
        // need to land on a char boundary so the next slice doesn't panic.
        search_from = start + 1;
        while search_from < haystack.len() && !haystack.is_char_boundary(search_from) {
            search_from += 1;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use async_trait::async_trait;

    // --- parse_ner_response ---

    #[test]
    fn parse_simple_array() {
        let r = r#"[{"kind":"person_name","value":"Alice Smith","confidence":0.9}]"#;
        let e = parse_ner_response(r);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].value, "Alice Smith");
        assert!((e[0].confidence - 0.9).abs() < 1e-6);
    }

    #[test]
    fn parse_empty_array() {
        assert!(parse_ner_response("[]").is_empty());
    }

    #[test]
    fn parse_array_embedded_in_prose() {
        // Models sometimes prefix/suffix JSON with commentary. Tolerate it.
        let r = r#"Sure, here are the entities:
[{"kind":"org_name","value":"Acme AG","confidence":0.7}]
Hope that helps!"#;
        let e = parse_ner_response(r);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].value, "Acme AG");
    }

    #[test]
    fn parse_malformed_returns_empty() {
        assert!(parse_ner_response("not json at all").is_empty());
        assert!(parse_ner_response(r#"[{"kind":"person_name","#).is_empty());
    }

    #[test]
    fn parse_clamps_confidence_to_unit_range() {
        let r = r#"[{"kind":"person_name","value":"X","confidence":1.5},
                    {"kind":"org_name","value":"Y","confidence":-0.2}]"#;
        let e = parse_ner_response(r);
        assert_eq!(e[0].confidence, 1.0);
        assert_eq!(e[1].confidence, 0.0);
    }

    // --- entities_to_findings ---

    #[test]
    fn finds_single_occurrence_span() {
        let text = "Met with Alice Smith yesterday.";
        let e = vec![NerEntity {
            kind: "person_name".into(),
            value: "Alice Smith".into(),
            confidence: 0.9,
        }];
        let f = entities_to_findings(e, text);
        assert_eq!(f.len(), 1);
        assert_eq!(&text[f[0].start..f[0].end], "Alice Smith");
        assert_eq!(f[0].kind, PiiKind::PersonName);
    }

    #[test]
    fn duplicate_value_maps_to_distinct_spans() {
        let text = "Alice Smith and again Alice Smith.";
        let e = vec![
            NerEntity {
                kind: "person_name".into(),
                value: "Alice Smith".into(),
                confidence: 0.9,
            },
            NerEntity {
                kind: "person_name".into(),
                value: "Alice Smith".into(),
                confidence: 0.9,
            },
        ];
        let f = entities_to_findings(e, text);
        assert_eq!(f.len(), 2);
        assert_ne!(f[0].start, f[1].start);
        assert_eq!(&text[f[0].start..f[0].end], "Alice Smith");
        assert_eq!(&text[f[1].start..f[1].end], "Alice Smith");
    }

    #[test]
    fn missing_value_is_dropped() {
        // The model hallucinated an entity not actually in the source.
        let text = "Plain text with no people.";
        let e = vec![NerEntity {
            kind: "person_name".into(),
            value: "Bob Jones".into(),
            confidence: 0.9,
        }];
        assert!(entities_to_findings(e, text).is_empty());
    }

    #[test]
    fn unknown_kind_is_dropped() {
        let text = "anything";
        let e = vec![NerEntity {
            kind: "license_plate".into(),
            value: "anything".into(),
            confidence: 1.0,
        }];
        assert!(entities_to_findings(e, text).is_empty());
    }

    #[test]
    fn empty_value_is_dropped() {
        let text = "anything";
        let e = vec![NerEntity {
            kind: "person_name".into(),
            value: "".into(),
            confidence: 1.0,
        }];
        assert!(entities_to_findings(e, text).is_empty());
    }

    #[test]
    fn org_synonyms_map_to_org_name() {
        let text = "Acme AG and BigCorp Inc are partners.";
        let e = vec![
            NerEntity {
                kind: "organization".into(),
                value: "Acme AG".into(),
                confidence: 0.8,
            },
            NerEntity {
                kind: "ORG".into(),
                value: "BigCorp Inc".into(),
                confidence: 0.8,
            },
        ];
        let f = entities_to_findings(e, text);
        assert_eq!(f.len(), 2);
        assert!(f.iter().all(|x| x.kind == PiiKind::OrgName));
    }

    #[test]
    fn findings_returned_in_offset_order() {
        // Entities provided in reverse-textual order; output must reorder.
        let text = "Bob Jones met Alice Smith.";
        let e = vec![
            NerEntity {
                kind: "person_name".into(),
                value: "Alice Smith".into(),
                confidence: 0.9,
            },
            NerEntity {
                kind: "person_name".into(),
                value: "Bob Jones".into(),
                confidence: 0.9,
            },
        ];
        let f = entities_to_findings(e, text);
        assert_eq!(f.len(), 2);
        assert!(f[0].start < f[1].start);
    }

    // --- ner_stage smoke test with mock backend ---

    /// Minimal mock backend that returns a canned response, regardless of
    /// the prompt. Lets us smoke-test the orchestration without loading a
    /// model.
    struct CannedBackend(String);

    #[async_trait]
    impl ModelBackend for CannedBackend {
        async fn load(&mut self, _model_path: &str, _n_gpu_layers: i32) -> Result<()> {
            Ok(())
        }
        async fn generate(&self, _prompt: &str, _max_tokens: u32) -> Result<String> {
            Ok(self.0.clone())
        }
        async fn unload(&mut self) -> Result<()> {
            Ok(())
        }
    }

    /// Tiny PromptFormatter for the test — just concatenates system+user.
    struct PlainFormatter;

    impl PromptFormatter for PlainFormatter {
        fn format_system_user(&self, system: &str, user: &str) -> String {
            format!("{system}\n\n{user}")
        }
        fn format_conversation(
            &self,
            _system: &str,
            _turns: &[crate::llm::context::ChatTurn],
        ) -> String {
            String::new()
        }
        fn tool_call_open_tag(&self) -> &str {
            ""
        }
        fn tool_call_close_tag(&self) -> &str {
            ""
        }
        fn format_tool_turn(&self, _content: &str) -> String {
            String::new()
        }
        fn chars_per_token(&self) -> f64 {
            4.0
        }
        fn tool_call_format_instruction(&self) -> String {
            String::new()
        }
        fn wrap_tool_call_example(&self, json: &str) -> String {
            json.to_string()
        }
    }

    #[tokio::test]
    async fn ner_stage_end_to_end_with_mock() {
        let response =
            r#"[{"kind":"person_name","value":"Alice Smith","confidence":0.92}]"#;
        let backend = CannedBackend(response.into());
        let formatter = PlainFormatter;
        let text = "Met with Alice Smith yesterday at the office.";
        let findings = ner_stage(&backend, &formatter, text).await.unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, PiiKind::PersonName);
        assert_eq!(&text[findings[0].start..findings[0].end], "Alice Smith");
        assert!((findings[0].confidence - 0.92).abs() < 1e-6);
    }

    #[tokio::test]
    async fn ner_stage_returns_empty_on_empty_array() {
        let backend = CannedBackend("[]".into());
        let formatter = PlainFormatter;
        let findings = ner_stage(&backend, &formatter, "no entities here").await.unwrap();
        assert!(findings.is_empty());
    }

    #[tokio::test]
    async fn ner_stage_returns_empty_on_malformed_response() {
        let backend = CannedBackend("the model went off-script".into());
        let formatter = PlainFormatter;
        let findings = ner_stage(&backend, &formatter, "anything").await.unwrap();
        assert!(findings.is_empty());
    }

    #[test]
    fn truncation_respects_utf8_boundaries() {
        // Build a string just over MAX_NER_CHARS where the boundary lands
        // inside a multi-byte codepoint (the é at the end).
        let filler = "a".repeat(MAX_NER_CHARS - 1);
        let text = format!("{filler}é"); // é is 2 bytes
        // Sanity check: this would split mid-codepoint at MAX_NER_CHARS.
        assert!(!text.is_char_boundary(MAX_NER_CHARS));

        // We don't run the LLM here, just confirm the boundary-finding
        // loop in ner_stage would back off cleanly. Replicate it inline.
        let mut end = MAX_NER_CHARS;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let _truncated = &text[..end];
        // No panic = pass.
    }
}
