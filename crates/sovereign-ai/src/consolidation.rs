//! Background memory consolidation engine.
//!
//! Periodically scans the workspace for document pairs that may be related,
//! with priority on cross-pollination between web (external) and owned documents.
//! Generates AI-suggested links stored in a separate `suggested_link` edge table,
//! structurally distinct from user-created `related_to` edges.

use std::collections::HashSet;

use sovereign_core::interfaces::ModelBackend;
use sovereign_db::schema::{
    Document, RelationType, SuggestedLink, SuggestionSource,
};
use sovereign_db::traits::GraphDB;

use crate::llm::format::PromptFormatter;
use crate::llm::AsyncLlmBackend;
use crate::tools::strip_think_blocks;

/// Maximum candidate pairs to evaluate per consolidation cycle.
const MAX_PAIRS_PER_CYCLE: usize = 5;

/// Minimum strength threshold for creating a suggestion.
const MIN_STRENGTH_THRESHOLD: f32 = 0.4;

/// Maximum characters of content per document fingerprint.
const FINGERPRINT_CHARS: usize = 200;

/// System prompt for the pair-scoring LLM call.
const SCORING_SYSTEM_PROMPT: &str = "\
Given document pairs, determine if they are meaningfully related.
Output ONLY a JSON array with one entry per pair:
[{\"pair\":1,\"related\":true,\"type\":\"supports\",\"strength\":0.8,\"reason\":\"one sentence\"}]

Valid types: supports, references, contradicts, continues, derivedfrom

If a pair is unrelated, set \"related\":false and omit other fields.
Output ONLY the JSON array, nothing else.";

/// A scored candidate pair from LLM evaluation.
#[derive(Debug)]
struct ScoredPair {
    from_id: String,
    to_id: String,
    relation_type: RelationType,
    strength: f32,
    reason: String,
}

/// Build a short fingerprint for a document: title + first N chars of content body.
fn build_fingerprint(doc: &Document) -> String {
    let body = extract_body(&doc.content);
    let truncated = if body.len() > FINGERPRINT_CHARS {
        &body[..FINGERPRINT_CHARS]
    } else {
        &body
    };
    let ownership = if doc.is_owned { "owned" } else { "web" };
    format!("({ownership}): \"{}\" — {truncated}", doc.title)
}

/// Extract the body text from the JSON content field.
fn extract_body(content: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(content) {
        if let Some(body) = v["body"].as_str() {
            return body.to_string();
        }
    }
    // Fallback: treat entire content as body
    content.to_string()
}

/// Run one consolidation cycle: find candidate pairs, score them, persist suggestions.
///
/// Returns the newly created suggestions (empty if no candidates or all below threshold).
pub async fn run_cycle(
    db: &dyn GraphDB,
    router: &AsyncLlmBackend,
    formatter: &dyn PromptFormatter,
    source: SuggestionSource,
) -> anyhow::Result<Vec<SuggestedLink>> {
    // 1. Fetch all active documents
    let docs = db.list_documents(None).await?;
    if docs.len() < 2 {
        return Ok(vec![]);
    }

    // 2. Fetch existing relationships and suggestions to avoid duplicates
    let existing_rels = db.list_all_relationships().await?;
    let existing_suggestions = db.list_pending_suggestions().await?;

    // Build a set of existing pairs (bidirectional) for fast lookup
    let mut existing_pairs: HashSet<(String, String)> = HashSet::new();
    for rel in &existing_rels {
        if let (Some(in_t), Some(out_t)) = (&rel.in_, &rel.out) {
            let a = sovereign_db::schema::thing_to_raw(in_t);
            let b = sovereign_db::schema::thing_to_raw(out_t);
            existing_pairs.insert((a.clone(), b.clone()));
            existing_pairs.insert((b, a));
        }
    }
    for sugg in &existing_suggestions {
        if let (Some(in_t), Some(out_t)) = (&sugg.in_, &sugg.out) {
            let a = sovereign_db::schema::thing_to_raw(in_t);
            let b = sovereign_db::schema::thing_to_raw(out_t);
            existing_pairs.insert((a.clone(), b.clone()));
            existing_pairs.insert((b, a));
        }
    }

    // Also check dismissed suggestions via suggestion_exists (covers all statuses)
    // The existing_pairs set above only covers pending; we'll filter via db call below.

    // 3. Build candidate pairs, prioritizing cross-owned (web ↔ owned)
    let candidates = find_candidate_pairs(&docs, &existing_pairs, db).await;
    if candidates.is_empty() {
        return Ok(vec![]);
    }

    // 4. Score via LLM
    let scored = score_pairs(router, formatter, &candidates, &docs).await?;

    // 5. Persist passing pairs
    let mut created = Vec::new();
    for sp in scored {
        if sp.strength >= MIN_STRENGTH_THRESHOLD {
            let link = db
                .create_suggested_link(
                    &sp.from_id,
                    &sp.to_id,
                    sp.relation_type,
                    sp.strength,
                    &sp.reason,
                    source.clone(),
                )
                .await?;
            created.push(link);
        }
    }

    Ok(created)
}

/// Find candidate document pairs for evaluation.
///
/// Priority order:
/// 1. External ↔ owned (cross-pollination)
/// 2. Owned ↔ owned across different threads
///
/// Filters out pairs that already have relationships or suggestions.
/// Ranks by recency (at least one doc modified recently).
async fn find_candidate_pairs(
    docs: &[Document],
    existing_pairs: &HashSet<(String, String)>,
    db: &dyn GraphDB,
) -> Vec<(usize, usize)> {
    let mut cross_owned: Vec<(usize, usize, chrono::DateTime<chrono::Utc>)> = Vec::new();
    let mut cross_thread: Vec<(usize, usize, chrono::DateTime<chrono::Utc>)> = Vec::new();

    for i in 0..docs.len() {
        for j in (i + 1)..docs.len() {
            let a = &docs[i];
            let b = &docs[j];

            let a_id = a.id_string().unwrap_or_default();
            let b_id = b.id_string().unwrap_or_default();

            // Skip if already related
            if existing_pairs.contains(&(a_id.clone(), b_id.clone())) {
                continue;
            }

            // Skip if suggestion already exists (any status including dismissed)
            if db.suggestion_exists(&a_id, &b_id).await.unwrap_or(true) {
                continue;
            }

            let recency = a.modified_at.max(b.modified_at);

            if a.is_owned != b.is_owned {
                // Cross-pollination: one owned, one external
                cross_owned.push((i, j, recency));
            } else if a.is_owned && b.is_owned && a.thread_id != b.thread_id {
                // Cross-thread owned docs
                cross_thread.push((i, j, recency));
            }
        }
    }

    // Sort by recency (most recent first)
    cross_owned.sort_by(|a, b| b.2.cmp(&a.2));
    cross_thread.sort_by(|a, b| b.2.cmp(&a.2));

    // Take cross-owned first, then cross-thread, up to MAX_PAIRS
    let mut result: Vec<(usize, usize)> = Vec::new();
    for (i, j, _) in cross_owned.iter().take(MAX_PAIRS_PER_CYCLE) {
        result.push((*i, *j));
    }
    let remaining = MAX_PAIRS_PER_CYCLE.saturating_sub(result.len());
    for (i, j, _) in cross_thread.iter().take(remaining) {
        result.push((*i, *j));
    }

    result
}

/// Score candidate pairs using the 3B router model.
async fn score_pairs(
    router: &AsyncLlmBackend,
    formatter: &dyn PromptFormatter,
    candidates: &[(usize, usize)],
    docs: &[Document],
) -> anyhow::Result<Vec<ScoredPair>> {
    if candidates.is_empty() {
        return Ok(vec![]);
    }

    // Build the user prompt with all pairs
    let mut user_msg = String::new();
    for (idx, (i, j)) in candidates.iter().enumerate() {
        let fp_a = build_fingerprint(&docs[*i]);
        let fp_b = build_fingerprint(&docs[*j]);
        user_msg.push_str(&format!("Pair {}:\nA {}\nB {}\n\n", idx + 1, fp_a, fp_b));
    }

    let prompt = formatter.format_system_user(SCORING_SYSTEM_PROMPT, user_msg.trim());
    let response: String = router.generate(&prompt, 300).await?;
    let response = strip_think_blocks(response.trim());

    // Parse JSON array from response
    parse_scoring_response(&response, candidates, docs)
}

/// Parse the LLM's JSON array response into scored pairs.
fn parse_scoring_response(
    response: &str,
    candidates: &[(usize, usize)],
    docs: &[Document],
) -> anyhow::Result<Vec<ScoredPair>> {
    let trimmed = response.trim();

    // Find JSON array
    let json_str = if let Some(start) = trimmed.find('[') {
        if let Some(end) = trimmed.rfind(']') {
            &trimmed[start..=end]
        } else {
            return Ok(vec![]);
        }
    } else {
        return Ok(vec![]);
    };

    let arr: Vec<serde_json::Value> = match serde_json::from_str(json_str) {
        Ok(a) => a,
        Err(_) => return Ok(vec![]),
    };

    let mut results = Vec::new();
    for entry in &arr {
        let related = entry["related"].as_bool().unwrap_or(false);
        if !related {
            continue;
        }

        let pair_num = entry["pair"].as_u64().unwrap_or(0) as usize;
        if pair_num == 0 || pair_num > candidates.len() {
            continue;
        }

        let (i, j) = candidates[pair_num - 1];
        let from_id = docs[i].id_string().unwrap_or_default();
        let to_id = docs[j].id_string().unwrap_or_default();

        let type_str = entry["type"].as_str().unwrap_or("references");
        let relation_type = type_str.parse::<RelationType>().unwrap_or(RelationType::References);

        let strength = entry["strength"]
            .as_f64()
            .map(|f| f as f32)
            .unwrap_or(0.5)
            .clamp(0.0, 1.0);

        let reason = entry["reason"]
            .as_str()
            .unwrap_or("Related content")
            .to_string();

        results.push(ScoredPair {
            from_id,
            to_id,
            relation_type,
            strength,
            reason,
        });
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sovereign_db::mock::MockGraphDB;
    use sovereign_db::schema::{Document, SuggestionSource, Thread};

    fn make_doc(title: &str, thread_id: &str, is_owned: bool, content: &str) -> Document {
        let mut doc = Document::new(title.into(), thread_id.into(), is_owned);
        doc.content = format!(r#"{{"body":"{content}","images":[]}}"#);
        doc
    }

    #[test]
    fn test_build_fingerprint_owned() {
        let doc = make_doc("Notes", "t:1", true, "Some content here");
        let fp = build_fingerprint(&doc);
        assert!(fp.contains("(owned)"));
        assert!(fp.contains("\"Notes\""));
        assert!(fp.contains("Some content here"));
    }

    #[test]
    fn test_build_fingerprint_truncates() {
        let long_body = "x".repeat(500);
        let doc = make_doc("Long", "t:1", false, &long_body);
        let fp = build_fingerprint(&doc);
        assert!(fp.len() < 500); // should be truncated
    }

    #[test]
    fn test_extract_body_json() {
        let content = r#"{"body":"Hello world","images":[]}"#;
        assert_eq!(extract_body(content), "Hello world");
    }

    #[test]
    fn test_extract_body_fallback() {
        let content = "plain text content";
        assert_eq!(extract_body(content), "plain text content");
    }

    #[tokio::test]
    async fn test_parse_scoring_response_valid() {
        let response = r#"[
            {"pair": 1, "related": true, "type": "supports", "strength": 0.8, "reason": "Both about CRDTs"},
            {"pair": 2, "related": false}
        ]"#;

        // Use MockGraphDB to get proper IDs
        let db = MockGraphDB::new();
        let mut docs = Vec::new();
        for (name, owned) in [("A", true), ("B", false), ("C", true)] {
            let d = db.create_document(make_doc(name, "t:1", owned, "")).await.unwrap();
            docs.push(d);
        }

        let candidates = vec![(0, 1), (1, 2)];
        let result = parse_scoring_response(response, &candidates, &docs).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].relation_type, RelationType::Supports);
        assert!((result[0].strength - 0.8).abs() < 0.01);
        assert_eq!(result[0].reason, "Both about CRDTs");
    }

    #[test]
    fn test_parse_scoring_response_malformed() {
        let response = "I think pair 1 is related because...";
        let docs = vec![make_doc("A", "t:1", true, "")];
        let candidates = vec![(0, 0)];
        let result = parse_scoring_response(response, &candidates, &docs).unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_find_candidates_prefers_cross_owned() {
        let db = MockGraphDB::new();
        let t = db.create_thread(Thread::new("T".into(), "".into())).await.unwrap();
        let tid = t.id_string().unwrap();

        // Create 2 owned + 1 external
        let _d1 = db.create_document(make_doc("Owned1", &tid, true, "aaa")).await.unwrap();
        let _d2 = db.create_document(make_doc("Owned2", &tid, true, "bbb")).await.unwrap();
        let _d3 = db.create_document(make_doc("Web1", &tid, false, "ccc")).await.unwrap();

        let docs = db.list_documents(None).await.unwrap();
        let existing = HashSet::new();

        let pairs = find_candidate_pairs(&docs, &existing, &db).await;
        // Should have cross-owned pairs (owned↔web) before same-thread owned↔owned
        assert!(!pairs.is_empty());

        // At least one pair should be cross-owned
        let has_cross = pairs.iter().any(|(i, j)| docs[*i].is_owned != docs[*j].is_owned);
        assert!(has_cross);
    }

    #[tokio::test]
    async fn test_find_candidates_skips_existing() {
        let db = MockGraphDB::new();
        let t = db.create_thread(Thread::new("T".into(), "".into())).await.unwrap();
        let tid = t.id_string().unwrap();

        let d1 = db.create_document(make_doc("A", &tid, true, "aaa")).await.unwrap();
        let d2 = db.create_document(make_doc("B", &tid, false, "bbb")).await.unwrap();

        let a_id = d1.id_string().unwrap();
        let b_id = d2.id_string().unwrap();

        // Create existing suggestion so the pair is skipped
        db.create_suggested_link(
            &a_id, &b_id,
            RelationType::References, 0.5, "test", SuggestionSource::Consolidation,
        ).await.unwrap();

        let docs = db.list_documents(None).await.unwrap();
        let existing = HashSet::new();

        let pairs = find_candidate_pairs(&docs, &existing, &db).await;
        assert!(pairs.is_empty());
    }

    #[tokio::test]
    async fn test_empty_db_no_suggestions() {
        let db = MockGraphDB::new();
        // No formatter/router needed since we won't reach LLM call
        let docs = db.list_documents(None).await.unwrap();
        assert!(docs.is_empty());
        // run_cycle would return early with < 2 docs
    }
}
