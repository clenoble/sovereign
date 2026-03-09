//! Content reliability assessment using local LLM.
//!
//! Ports the CRABE (Content Reliability Assessment Browser Extension) rubrics
//! to work with local Qwen models. Classifies web content as Factual/Opinion/Fiction
//! and scores reliability 0-5 across domain-specific rubric criteria.
//!
//! Two-step assessment:
//! 1. Classification (3B router): Factual, Opinion, or Fiction
//! 2. Rubric scoring (7B reasoning): score each indicator 0-5 with analysis

use serde::{Deserialize, Serialize};
use sovereign_core::interfaces::ModelBackend;

use crate::llm::format::PromptFormatter;
use crate::llm::AsyncLlmBackend;

/// Result of a reliability assessment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReliabilityResult {
    pub classification: String,
    pub final_score: f32,
    pub raw_assessment: Vec<RubricScore>,
}

/// A single rubric criterion score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RubricScore {
    pub indicator: String,
    pub analysis: String,
    pub score: f32,
}

/// System prompt for step 1: content classification.
const CLASSIFICATION_SYSTEM_PROMPT: &str = "\
You are a content classification assistant. Your task is to classify a given text \
into exactly one of three categories:

- Factual: Articles, reports, or documents that claim to present facts, data, or \
  objective information. Includes news articles, scientific papers, research reports, \
  technical documentation.
- Opinion: Editorials, commentary, advocacy, reviews, or persuasive writing. The \
  author is presenting their viewpoint, even if supported by evidence.
- Fiction: Creative writing, satire, parody, short stories, novels. Content that is \
  not meant to be taken as literal truth.

Respond with ONLY a JSON object, no other text:
{\"classification\": \"Factual\" or \"Opinion\" or \"Fiction\"}";

/// System prompt for step 2: factual content rubric scoring.
const FACTUAL_RUBRIC_PROMPT: &str = "\
You are a content reliability analyst. Score the following FACTUAL text on three criteria, \
each from 0 (worst) to 5 (best). Respond with ONLY a JSON array, no other text.

Criteria:

1. Evidentiary Integrity (0-5): How well does the text support its claims with evidence?
   5 = Dense quantitative data, specific citations, named studies and sources
   3 = Some evidence but vague or unverifiable
   0 = No evidence, pure assertion

2. Logical Coherence (0-5): How logically sound is the reasoning?
   5 = Clear logical structure, no fallacies
   3 = Generally coherent but with some logical gaps
   0 = Frequent fallacy markers (\"everyone knows\", ad hominem, false equivalence)

3. Rhetorical Style (0-5): How neutral and measured is the language?
   5 = Objective, measured, academic tone
   3 = Mostly neutral with occasional emotional language
   0 = Highly charged (\"outrageous\", \"miracle\", \"shocking\", \"devastating\")

Format:
[{\"indicator\":\"Evidentiary Integrity\",\"analysis\":\"...\",\"score\":N},\
{\"indicator\":\"Logical Coherence\",\"analysis\":\"...\",\"score\":N},\
{\"indicator\":\"Rhetorical Style\",\"analysis\":\"...\",\"score\":N}]";

/// System prompt for step 2: opinion content rubric scoring.
const OPINION_RUBRIC_PROMPT: &str = "\
You are a content reliability analyst. Score the following OPINION text on three criteria, \
each from 0 (worst) to 5 (best). Respond with ONLY a JSON array, no other text.

Criteria:

1. Transparency of Position (0-5): Does the author clearly identify this as opinion?
   5 = Explicit first-person framing (\"I believe\", \"In my view\"), clearly subjective
   3 = Implicitly opinionated but not clearly marked
   0 = Disguised as factual, no subjective markers

2. Support for Opinion (0-5): Is the opinion backed by evidence?
   5 = Specific data, examples, and citations supporting the viewpoint
   3 = Some anecdotal evidence
   0 = Pure assertion with no support

3. Intellectual Honesty (0-5): Does the author acknowledge counterarguments?
   5 = Engages seriously with opposing views, acknowledges limitations
   3 = Mentions opposing views briefly
   0 = Dismisses or ignores all counterarguments

Format:
[{\"indicator\":\"Transparency of Position\",\"analysis\":\"...\",\"score\":N},\
{\"indicator\":\"Support for Opinion\",\"analysis\":\"...\",\"score\":N},\
{\"indicator\":\"Intellectual Honesty\",\"analysis\":\"...\",\"score\":N}]";

/// System prompt for step 2: fiction content rubric scoring.
const FICTION_RUBRIC_PROMPT: &str = "\
You are a content reliability analyst. Score the following FICTION text on two criteria, \
each from 0 (worst) to 5 (best). Respond with ONLY a JSON array, no other text.

Criteria:

1. Explicit Labeling (0-5): Is the content clearly labeled as fiction/satire/parody?
   5 = Clearly labeled (\"Satire\", \"Short Story\", \"Parody\", \"A Novel\")
   3 = Genre conventions suggest fiction but no explicit label
   0 = No labeling — could be mistaken for factual reporting

2. Content & Stylistic Cues (0-5): How clearly does the style signal fiction?
   5 = Obvious literary devices (dialogue, narrative description, character development)
   3 = Mixed signals, some fictional elements but could be mistaken for reportage
   0 = Written in news/report style with no fictional markers

Format:
[{\"indicator\":\"Explicit Labeling\",\"analysis\":\"...\",\"score\":N},\
{\"indicator\":\"Content & Stylistic Cues\",\"analysis\":\"...\",\"score\":N}]";

/// Maximum characters of content to send to the LLM for assessment.
/// Keeps prompt size reasonable for local models.
const MAX_ASSESSMENT_CHARS: usize = 8000;

/// Run a two-step reliability assessment on the given text.
///
/// Step 1: Classify content using the router (3B) model.
/// Step 2: Score rubric criteria using the router (or reasoning if available).
pub async fn assess_reliability(
    router: &AsyncLlmBackend,
    formatter: &dyn PromptFormatter,
    text: &str,
) -> anyhow::Result<ReliabilityResult> {
    // Truncate text to reasonable length for local model context
    let text = if text.len() > MAX_ASSESSMENT_CHARS {
        &text[..MAX_ASSESSMENT_CHARS]
    } else {
        text
    };

    // Step 1: Classify
    let classification = classify_content(router, formatter, text).await?;

    // Step 2: Score rubric
    let rubric_prompt = match classification.as_str() {
        "Opinion" => OPINION_RUBRIC_PROMPT,
        "Fiction" => FICTION_RUBRIC_PROMPT,
        _ => FACTUAL_RUBRIC_PROMPT, // default to Factual
    };

    let scores = score_rubric(router, formatter, rubric_prompt, text).await?;

    let final_score = if scores.is_empty() {
        0.0
    } else {
        scores.iter().map(|s| s.score).sum::<f32>() / scores.len() as f32
    };

    Ok(ReliabilityResult {
        classification,
        final_score,
        raw_assessment: scores,
    })
}

/// Step 1: Classify content as Factual, Opinion, or Fiction.
async fn classify_content(
    backend: &AsyncLlmBackend,
    formatter: &dyn PromptFormatter,
    text: &str,
) -> anyhow::Result<String> {
    let user_msg = format!("Classify this text:\n\n{text}");
    let prompt = formatter.format_system_user(CLASSIFICATION_SYSTEM_PROMPT, &user_msg);
    let response: String = backend.generate(&prompt, 50).await?;

    // Parse JSON response
    let trimmed = response.trim();
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed[start..].rfind('}') {
            let json_str = &trimmed[start..=start + end];
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(json_str) {
                if let Some(c) = v["classification"].as_str() {
                    let normalized = match c.to_lowercase().as_str() {
                        "factual" => "Factual",
                        "opinion" => "Opinion",
                        "fiction" => "Fiction",
                        _ => "Factual",
                    };
                    return Ok(normalized.to_string());
                }
            }
        }
    }

    // Fallback: try to find classification keyword in raw response
    let lower = trimmed.to_lowercase();
    if lower.contains("opinion") {
        Ok("Opinion".to_string())
    } else if lower.contains("fiction") {
        Ok("Fiction".to_string())
    } else {
        Ok("Factual".to_string())
    }
}

/// Step 2: Score rubric criteria.
async fn score_rubric(
    backend: &AsyncLlmBackend,
    formatter: &dyn PromptFormatter,
    rubric_prompt: &str,
    text: &str,
) -> anyhow::Result<Vec<RubricScore>> {
    let user_msg = format!("Score this text:\n\n{text}");
    let prompt = formatter.format_system_user(rubric_prompt, &user_msg);
    let response: String = backend.generate(&prompt, 800).await?;

    // Parse JSON array from response
    let trimmed = response.trim();
    if let Some(start) = trimmed.find('[') {
        if let Some(end) = trimmed[start..].rfind(']') {
            let json_str = &trimmed[start..=start + end];
            if let Ok(scores) = serde_json::from_str::<Vec<RubricScore>>(json_str) {
                // Clamp scores to 0-5 range
                let clamped: Vec<RubricScore> = scores
                    .into_iter()
                    .map(|mut s| {
                        s.score = s.score.clamp(0.0, 5.0);
                        s
                    })
                    .collect();
                return Ok(clamped);
            }
        }
    }

    // If parsing fails, return empty — caller will see final_score = 0
    tracing::warn!("Failed to parse rubric scores from LLM response: {trimmed}");
    Ok(vec![])
}

/// Serialize a ReliabilityResult's raw_assessment to JSON string for DB storage.
pub fn assessment_to_json(result: &ReliabilityResult) -> String {
    serde_json::to_string(&result.raw_assessment).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reliability_result_serialization() {
        let result = ReliabilityResult {
            classification: "Factual".into(),
            final_score: 3.5,
            raw_assessment: vec![
                RubricScore {
                    indicator: "Evidentiary Integrity".into(),
                    analysis: "Good citations".into(),
                    score: 4.0,
                },
                RubricScore {
                    indicator: "Logical Coherence".into(),
                    analysis: "Sound reasoning".into(),
                    score: 3.5,
                },
                RubricScore {
                    indicator: "Rhetorical Style".into(),
                    analysis: "Mostly neutral".into(),
                    score: 3.0,
                },
            ],
        };

        let json = serde_json::to_string(&result).unwrap();
        let back: ReliabilityResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.classification, "Factual");
        assert!((back.final_score - 3.5).abs() < 0.01);
        assert_eq!(back.raw_assessment.len(), 3);
    }

    #[test]
    fn test_assessment_to_json() {
        let result = ReliabilityResult {
            classification: "Opinion".into(),
            final_score: 2.0,
            raw_assessment: vec![RubricScore {
                indicator: "Test".into(),
                analysis: "Analysis".into(),
                score: 2.0,
            }],
        };
        let json = assessment_to_json(&result);
        assert!(json.contains("Test"));
        assert!(json.contains("2.0") || json.contains("2"));
    }

    #[test]
    fn test_empty_assessment() {
        let result = ReliabilityResult {
            classification: "Fiction".into(),
            final_score: 0.0,
            raw_assessment: vec![],
        };
        let json = assessment_to_json(&result);
        assert_eq!(json, "[]");
    }
}
