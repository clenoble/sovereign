use anyhow::Result;
use sovereign_core::interfaces::UserIntent;

/// Parse the LLM's JSON response into a UserIntent.
/// Falls back to keyword extraction if JSON is malformed.
pub fn parse_intent_response(response: &str) -> Result<UserIntent> {
    // Try strict JSON parse first
    if let Ok(intent) = try_parse_json(response) {
        return Ok(intent);
    }

    // Fallback: extract action keyword heuristically
    Ok(extract_intent_heuristic(response))
}

#[derive(serde::Deserialize)]
struct IntentJson {
    action: String,
    #[serde(default)]
    target: Option<String>,
    #[serde(default = "default_confidence")]
    confidence: f32,
    #[serde(default)]
    entities: Vec<(String, String)>,
}

fn default_confidence() -> f32 {
    0.5
}

fn try_parse_json(response: &str) -> Result<UserIntent> {
    // Find JSON object in response (model may include surrounding text)
    let start = response
        .find('{')
        .ok_or_else(|| anyhow::anyhow!("No JSON object found"))?;
    let end = response
        .rfind('}')
        .ok_or_else(|| anyhow::anyhow!("No closing brace found"))?;
    let json_str = &response[start..=end];

    let parsed: IntentJson = serde_json::from_str(json_str)?;
    Ok(UserIntent {
        action: parsed.action,
        target: parsed.target,
        confidence: parsed.confidence,
        entities: parsed.entities,
    })
}

fn extract_intent_heuristic(response: &str) -> UserIntent {
    let lower = response.to_lowercase();

    let action = if lower.contains("search") || lower.contains("find") || lower.contains("look") {
        "search"
    } else if lower.contains("open") || lower.contains("show") {
        "open"
    } else if lower.contains("create") || lower.contains("new") {
        "create"
    } else if lower.contains("navigate") || lower.contains("go to") {
        "navigate"
    } else if lower.contains("summarize") || lower.contains("summary") {
        "summarize"
    } else {
        "unknown"
    };

    UserIntent {
        action: action.to_string(),
        target: None,
        confidence: 0.3, // low confidence for heuristic
        entities: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_json() {
        let response = r#"{"action": "search", "target": "meeting notes", "confidence": 0.95, "entities": [["topic", "meetings"]]}"#;
        let intent = parse_intent_response(response).unwrap();
        assert_eq!(intent.action, "search");
        assert_eq!(intent.target.as_deref(), Some("meeting notes"));
        assert!((intent.confidence - 0.95).abs() < f32::EPSILON);
        assert_eq!(intent.entities.len(), 1);
    }

    #[test]
    fn parse_json_with_surrounding_text() {
        let response = "Sure! Here's the classification:\n{\"action\": \"open\", \"target\": \"budget.xlsx\", \"confidence\": 0.88, \"entities\": []}\nHope that helps!";
        let intent = parse_intent_response(response).unwrap();
        assert_eq!(intent.action, "open");
        assert_eq!(intent.target.as_deref(), Some("budget.xlsx"));
    }

    #[test]
    fn parse_json_missing_optional_fields() {
        let response = r#"{"action": "search"}"#;
        let intent = parse_intent_response(response).unwrap();
        assert_eq!(intent.action, "search");
        assert!(intent.target.is_none());
        assert!((intent.confidence - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn fallback_heuristic_search() {
        let response = "I think the user wants to search for documents about rust";
        let intent = parse_intent_response(response).unwrap();
        assert_eq!(intent.action, "search");
        assert!((intent.confidence - 0.3).abs() < f32::EPSILON);
    }

    #[test]
    fn fallback_heuristic_open() {
        let response = "The user wants to open the file";
        let intent = parse_intent_response(response).unwrap();
        assert_eq!(intent.action, "open");
    }

    #[test]
    fn fallback_heuristic_unknown() {
        let response = "I cannot determine the intent";
        let intent = parse_intent_response(response).unwrap();
        assert_eq!(intent.action, "unknown");
    }

    #[test]
    fn parse_malformed_json_falls_back() {
        let response = r#"{"action": search, confidence: high}"#;
        let intent = parse_intent_response(response).unwrap();
        // Malformed JSON → falls back to heuristic which finds "search"
        assert_eq!(intent.action, "search");
    }
}
