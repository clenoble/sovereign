//! Pipeline orchestrator — runs regex + NER + entity disambiguation and
//! produces the per-source result that the ingest hooks (step 4) will
//! commit to the database.
//!
//! Order:
//!   1. [`regex_stage`](super::regex::regex_stage)     — confidence 1.0.
//!   2. [`ner_stage`](super::ner::ner_stage)           — confidence ≤ 1.0.
//!      NER findings whose span overlaps a regex finding are dropped:
//!      the regex layer is authoritative for the kinds it handles.
//!   3. [`disambiguate`](super::entity_disambig::disambiguate)
//!      — link to entities + propose new ones.
//!   4. Review-state threshold — findings below `review_threshold`
//!      become `ReviewState::Unreviewed` (queued for user confirmation);
//!      others become `Confirmed`.
//!
//! NER is best-effort. If the backend errors or the response is
//! unparseable, the pipeline falls back to regex-only — never a hard
//! dependency on the LLM being available.

use sovereign_core::interfaces::ModelBackend;
use sovereign_db::schema::{Contact, Entity, ReviewState};

use crate::llm::format::PromptFormatter;

use super::{
    entity_disambig::{disambiguate, DisambiguationResult},
    ner::ner_stage,
    regex::regex_stage,
    Finding, Locale,
};

/// Default review threshold from the plan: findings at or above this
/// confidence are auto-confirmed; below, they're queued for review.
pub const DEFAULT_REVIEW_THRESHOLD: f32 = 0.7;

/// Pipeline configuration.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub locale: Locale,
    /// Confidence at-or-above which findings are auto-confirmed.
    /// Findings below this are returned with `ReviewState::Unreviewed`.
    pub review_threshold: f32,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            locale: Locale::Generic,
            review_threshold: DEFAULT_REVIEW_THRESHOLD,
        }
    }
}

/// One finding ready to be committed to the database — already linked to
/// an entity (if any) and already labeled with the review state the
/// ingest hook should persist.
#[derive(Debug, Clone)]
pub struct ScannedFinding {
    pub finding: Finding,
    pub entity_id: Option<String>,
    pub review_state: ReviewState,
}

/// Result of one pipeline run over a single source body.
#[derive(Debug, Clone, Default)]
pub struct PipelineResult {
    pub findings: Vec<ScannedFinding>,
    /// Entities that don't yet exist but were inferred from unmatched
    /// findings. The ingest hook surfaces these as proposals in the
    /// dashboard review queue.
    pub proposed_entities: Vec<Entity>,
}

/// Run regex + NER + disambig over `text`.
///
/// `backend` and `formatter` are optional: when both are `None` (e.g.
/// during cold start, or for low-priority background scans) the
/// pipeline runs regex-only. NER errors are caught and logged via
/// `tracing::warn!`; they never propagate.
pub async fn run_pipeline(
    text: &str,
    config: &PipelineConfig,
    backend: Option<&dyn ModelBackend>,
    formatter: Option<&dyn PromptFormatter>,
    entities: &[Entity],
    contacts: &[Contact],
) -> PipelineResult {
    let regex_findings = regex_stage(text, config.locale);

    let ner_findings = match (backend, formatter) {
        (Some(b), Some(f)) => match ner_stage(b, f, text).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("NER stage errored, falling back to regex-only: {e}");
                Vec::new()
            }
        },
        _ => Vec::new(),
    };

    let combined = merge_regex_and_ner(regex_findings, ner_findings);
    let DisambiguationResult {
        linked,
        proposed_entities,
    } = disambiguate(combined, entities, contacts);

    let threshold = config.review_threshold;
    let findings = linked
        .into_iter()
        .map(|lf| {
            let review_state = if lf.finding.confidence >= threshold {
                ReviewState::Confirmed
            } else {
                ReviewState::Unreviewed
            };
            ScannedFinding {
                finding: lf.finding,
                entity_id: lf.entity_id,
                review_state,
            }
        })
        .collect();

    PipelineResult {
        findings,
        proposed_entities,
    }
}

/// Merge regex (1.0) and NER (variable) findings, dropping any NER finding
/// whose span overlaps a regex finding. The regex layer is authoritative
/// for the kinds it can detect — letting NER add a lower-confidence
/// duplicate would just litter the review queue.
fn merge_regex_and_ner(regex: Vec<Finding>, ner: Vec<Finding>) -> Vec<Finding> {
    let regex_spans: Vec<(usize, usize)> = regex.iter().map(|f| (f.start, f.end)).collect();
    let mut out = regex;
    for n in ner {
        let overlaps = regex_spans
            .iter()
            .any(|&(s, e)| n.start < e && s < n.end);
        if !overlaps {
            out.push(n);
        }
    }
    out.sort_by_key(|f| (f.start, f.end));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::context::ChatTurn;
    use crate::pii::ner::NerEntity;
    use crate::pii::PiiKind;
    use anyhow::Result;
    use async_trait::async_trait;
    use sovereign_db::schema::EntityKind;

    /// Mock backend returning a canned NER response.
    struct CannedBackend(String);

    #[async_trait]
    impl ModelBackend for CannedBackend {
        async fn load(&mut self, _path: &str, _layers: i32) -> Result<()> {
            Ok(())
        }
        async fn generate(&self, _prompt: &str, _max_tokens: u32) -> Result<String> {
            Ok(self.0.clone())
        }
        async fn unload(&mut self) -> Result<()> {
            Ok(())
        }
    }

    /// Backend that always errors — used to confirm the pipeline degrades
    /// gracefully to regex-only.
    struct FailingBackend;

    #[async_trait]
    impl ModelBackend for FailingBackend {
        async fn load(&mut self, _path: &str, _layers: i32) -> Result<()> {
            Ok(())
        }
        async fn generate(&self, _prompt: &str, _max_tokens: u32) -> Result<String> {
            anyhow::bail!("model not loaded")
        }
        async fn unload(&mut self) -> Result<()> {
            Ok(())
        }
    }

    struct PlainFormatter;

    impl PromptFormatter for PlainFormatter {
        fn format_system_user(&self, system: &str, user: &str) -> String {
            format!("{system}\n\n{user}")
        }
        fn format_conversation(&self, _system: &str, _turns: &[ChatTurn]) -> String {
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

    fn ner_response(entities: &[NerEntity]) -> String {
        serde_json::to_string(entities).unwrap()
    }

    // --- regex-only path ---

    #[tokio::test]
    async fn regex_only_no_backend() {
        let text = "Email me at alice@example.ch.";
        let result = run_pipeline(
            text,
            &PipelineConfig::default(),
            None,
            None,
            &[],
            &[],
        )
        .await;
        assert_eq!(result.findings.len(), 1);
        let f = &result.findings[0];
        assert_eq!(f.finding.kind, PiiKind::Email);
        // Confidence 1.0 ≥ default threshold 0.7 → Confirmed.
        assert_eq!(f.review_state, ReviewState::Confirmed);
        // Email's domain is unknown → propose Service entity.
        assert_eq!(result.proposed_entities.len(), 1);
        assert_eq!(result.proposed_entities[0].kind, EntityKind::Service);
    }

    // --- NER fallback ---

    #[tokio::test]
    async fn ner_failure_does_not_break_pipeline() {
        let text = "Email me at alice@example.ch.";
        let formatter = PlainFormatter;
        let backend = FailingBackend;
        let result = run_pipeline(
            text,
            &PipelineConfig::default(),
            Some(&backend),
            Some(&formatter),
            &[],
            &[],
        )
        .await;
        // Regex findings still present despite NER error.
        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.findings[0].finding.kind, PiiKind::Email);
    }

    // --- regex + NER end-to-end ---

    #[tokio::test]
    async fn regex_and_ner_combine() {
        let text = "Met with Alice Smith. Email: alice@acme.com.";
        let response = ner_response(&[NerEntity {
            kind: "person_name".into(),
            value: "Alice Smith".into(),
            confidence: 0.92,
        }]);
        let backend = CannedBackend(response);
        let formatter = PlainFormatter;

        let mut acme = Entity::new("Acme Corp".into(), EntityKind::Org);
        acme.domains = vec!["acme.com".into()];
        acme.id = Some(sovereign_db::schema::raw_to_thing("entity:acme").unwrap());
        let entities = vec![acme];

        let result = run_pipeline(
            text,
            &PipelineConfig::default(),
            Some(&backend),
            Some(&formatter),
            &entities,
            &[],
        )
        .await;

        // Two findings: PersonName (NER) and Email (regex).
        assert_eq!(result.findings.len(), 2);
        let kinds: Vec<&PiiKind> = result.findings.iter().map(|f| &f.finding.kind).collect();
        assert!(kinds.contains(&&PiiKind::PersonName));
        assert!(kinds.contains(&&PiiKind::Email));

        // Email links to acme, person doesn't link (no Alice entity).
        let email = result
            .findings
            .iter()
            .find(|f| f.finding.kind == PiiKind::Email)
            .unwrap();
        assert_eq!(email.entity_id.as_deref(), Some("entity:acme"));
        let person = result
            .findings
            .iter()
            .find(|f| f.finding.kind == PiiKind::PersonName)
            .unwrap();
        assert!(person.entity_id.is_none());

        // Person finding above threshold → Confirmed.
        assert_eq!(person.review_state, ReviewState::Confirmed);

        // One Person entity proposed for the unlinked Alice; Acme exists.
        assert_eq!(result.proposed_entities.len(), 1);
        assert_eq!(result.proposed_entities[0].kind, EntityKind::Person);
    }

    // --- review threshold ---

    #[tokio::test]
    async fn low_confidence_finding_is_unreviewed() {
        let text = "Met with Alice Smith yesterday.";
        let response = ner_response(&[NerEntity {
            kind: "person_name".into(),
            value: "Alice Smith".into(),
            confidence: 0.5, // below default threshold 0.7
        }]);
        let backend = CannedBackend(response);
        let formatter = PlainFormatter;
        let result = run_pipeline(
            text,
            &PipelineConfig::default(),
            Some(&backend),
            Some(&formatter),
            &[],
            &[],
        )
        .await;
        let person = result
            .findings
            .iter()
            .find(|f| f.finding.kind == PiiKind::PersonName)
            .unwrap();
        assert_eq!(person.review_state, ReviewState::Unreviewed);
    }

    #[tokio::test]
    async fn custom_threshold_respected() {
        let text = "Met with Alice Smith yesterday.";
        let response = ner_response(&[NerEntity {
            kind: "person_name".into(),
            value: "Alice Smith".into(),
            confidence: 0.5,
        }]);
        let backend = CannedBackend(response);
        let formatter = PlainFormatter;
        let config = PipelineConfig {
            review_threshold: 0.4,
            ..Default::default()
        };
        let result = run_pipeline(
            text,
            &config,
            Some(&backend),
            Some(&formatter),
            &[],
            &[],
        )
        .await;
        let person = result
            .findings
            .iter()
            .find(|f| f.finding.kind == PiiKind::PersonName)
            .unwrap();
        // 0.5 ≥ 0.4 → Confirmed.
        assert_eq!(person.review_state, ReviewState::Confirmed);
    }

    // --- regex / NER overlap policy ---

    #[tokio::test]
    async fn ner_overlap_with_regex_is_dropped() {
        // Regex catches the email "alice@acme.com" with confidence 1.0.
        // NER (incorrectly) returns "alice@acme.com" as a person_name —
        // the merge step must drop it.
        let text = "alice@acme.com is the email";
        let response = ner_response(&[NerEntity {
            kind: "person_name".into(),
            value: "alice@acme.com".into(),
            confidence: 0.6,
        }]);
        let backend = CannedBackend(response);
        let formatter = PlainFormatter;
        let result = run_pipeline(
            text,
            &PipelineConfig::default(),
            Some(&backend),
            Some(&formatter),
            &[],
            &[],
        )
        .await;
        // Only the email survives.
        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.findings[0].finding.kind, PiiKind::Email);
    }

    // --- Locale plumbing ---

    #[tokio::test]
    async fn swiss_locale_enables_avs() {
        let text = "AVS 756.1234.5678.97";
        let config = PipelineConfig {
            locale: Locale::Swiss,
            ..Default::default()
        };
        let result = run_pipeline(text, &config, None, None, &[], &[]).await;
        assert!(result
            .findings
            .iter()
            .any(|f| f.finding.kind == PiiKind::Avs));
    }

    #[tokio::test]
    async fn generic_locale_skips_avs() {
        let text = "AVS 756.1234.5678.97";
        let result = run_pipeline(
            text,
            &PipelineConfig::default(),
            None,
            None,
            &[],
            &[],
        )
        .await;
        assert!(!result
            .findings
            .iter()
            .any(|f| f.finding.kind == PiiKind::Avs));
    }

    // --- merge_regex_and_ner unit ---

    #[test]
    fn merge_drops_overlap_keeps_disjoint() {
        let regex_f = vec![Finding {
            kind: PiiKind::Email,
            start: 5,
            end: 20,
            sample: "x".into(),
            confidence: 1.0,
        }];
        let ner_f = vec![
            // Overlap with regex span — must be dropped.
            Finding {
                kind: PiiKind::PersonName,
                start: 10,
                end: 25,
                sample: "y".into(),
                confidence: 0.8,
            },
            // Disjoint — must survive.
            Finding {
                kind: PiiKind::OrgName,
                start: 30,
                end: 40,
                sample: "z".into(),
                confidence: 0.8,
            },
        ];
        let merged = merge_regex_and_ner(regex_f, ner_f);
        assert_eq!(merged.len(), 2);
        let kinds: Vec<&PiiKind> = merged.iter().map(|f| &f.kind).collect();
        assert!(kinds.contains(&&PiiKind::Email));
        assert!(kinds.contains(&&PiiKind::OrgName));
        assert!(!kinds.contains(&&PiiKind::PersonName));
    }
}
