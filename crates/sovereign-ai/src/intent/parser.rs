use anyhow::Result;
use sovereign_core::interfaces::UserIntent;
use sovereign_core::security::Plane;

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
        origin: Plane::Control,
    })
}

fn extract_intent_heuristic(response: &str) -> UserIntent {
    let lower = response.to_lowercase();

    // Thread-specific intents (check before generic "create"/"new")
    let action = if lower.contains("create thread") || lower.contains("new thread") || lower.contains("new project") {
        "create_thread"
    } else if lower.contains("rename thread") || lower.contains("rename project") {
        "rename_thread"
    } else if lower.contains("delete thread") || lower.contains("remove thread") || lower.contains("delete project") {
        "delete_thread"
    } else if lower.contains("move") || lower.contains("assign") || lower.contains("reassign") {
        "move_document"
    } else if lower.contains("history") || lower.contains("versions") || lower.contains("changelog") {
        "history"
    } else if lower.contains("restore") || lower.contains("revert") || lower.contains("rollback") {
        "restore"
    } else if lower.contains("word count") || lower.contains("statistics") || lower.contains("how many words") {
        "word_count"
    } else if lower.contains("find and replace") || lower.contains("find & replace") || lower.contains("replace all") {
        "find_replace"
    } else if lower.contains("duplicate") || lower.contains("copy document") || lower.contains("make a copy") {
        "duplicate"
    } else if lower.contains("import file") || lower.contains("import from") || lower.contains("import a file") {
        "import_file"
    } else if lower.contains("search") || lower.contains("find") || lower.contains("look") {
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
        confidence: 0.3,
        entities: vec![],
        origin: Plane::Control,
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

    #[test]
    fn heuristic_create_thread() {
        let intent = parse_intent_response("create thread called Alpha").unwrap();
        assert_eq!(intent.action, "create_thread");
    }

    #[test]
    fn heuristic_new_project() {
        let intent = parse_intent_response("I want a new project for research").unwrap();
        assert_eq!(intent.action, "create_thread");
    }

    #[test]
    fn heuristic_rename_thread() {
        let intent = parse_intent_response("rename thread Research to Science").unwrap();
        assert_eq!(intent.action, "rename_thread");
    }

    #[test]
    fn heuristic_delete_thread() {
        let intent = parse_intent_response("delete thread Old Stuff").unwrap();
        assert_eq!(intent.action, "delete_thread");
    }

    #[test]
    fn heuristic_move_document() {
        let intent = parse_intent_response("move Research Notes to Development").unwrap();
        assert_eq!(intent.action, "move_document");
    }

    #[test]
    fn heuristic_assign_document() {
        let intent = parse_intent_response("assign this document to the Research thread").unwrap();
        assert_eq!(intent.action, "move_document");
    }

    #[test]
    fn heuristic_history() {
        let intent = parse_intent_response("show me the history of this document").unwrap();
        assert_eq!(intent.action, "history");
    }

    #[test]
    fn heuristic_restore() {
        let intent = parse_intent_response("restore the previous version").unwrap();
        assert_eq!(intent.action, "restore");
    }

    #[test]
    fn heuristic_word_count() {
        let intent = parse_intent_response("show me the word count").unwrap();
        assert_eq!(intent.action, "word_count");
    }

    #[test]
    fn heuristic_statistics() {
        let intent = parse_intent_response("show document statistics").unwrap();
        assert_eq!(intent.action, "word_count");
    }

    #[test]
    fn heuristic_find_replace() {
        let intent = parse_intent_response("find and replace all occurrences").unwrap();
        assert_eq!(intent.action, "find_replace");
    }

    #[test]
    fn heuristic_duplicate() {
        let intent = parse_intent_response("duplicate this document").unwrap();
        assert_eq!(intent.action, "duplicate");
    }

    #[test]
    fn heuristic_copy_document() {
        let intent = parse_intent_response("copy document to a new one").unwrap();
        assert_eq!(intent.action, "duplicate");
    }

    #[test]
    fn heuristic_import_file() {
        let intent = parse_intent_response("import file from disk").unwrap();
        assert_eq!(intent.action, "import_file");
    }
}
