use anyhow::Result;
use sovereign_core::interfaces::UserIntent;
use sovereign_core::security::Plane;

/// Post-classification override: if the *user text* (not the LLM response)
/// clearly signals a model swap or model list, correct a misclassified intent.
/// This guards against the 3B router treating model names as search targets.
pub fn override_model_intent(user_text: &str, intent: &mut UserIntent) {
    let lower = user_text.to_lowercase();

    // Check for swap_model signals in the original user text
    if intent.action != "swap_model" && intent.action != "list_models" {
        let explicit_swap = lower.contains("swap model")
            || lower.contains("switch model")
            || lower.contains("change model")
            || lower.contains("use model");
        if explicit_swap || contains_model_swap_intent(&lower) {
            tracing::info!(
                "Overriding action '{}' → 'swap_model' (user text matches model swap pattern)",
                intent.action
            );
            intent.action = "swap_model".to_string();
            intent.confidence = 0.9;
            return;
        }
    }

    // Check for list_models signals
    if intent.action != "list_models" {
        if lower.contains("list models")
            || lower.contains("available models")
            || lower.contains("what models")
            || lower.contains("show models")
        {
            tracing::info!(
                "Overriding action '{}' → 'list_models' (user text matches list models pattern)",
                intent.action
            );
            intent.action = "list_models".to_string();
            intent.confidence = 0.9;
        }
    }
}

/// Post-classification override: if the *user text* matches a panel-toggle
/// phrase (e.g. "open the PII dashboard"), correct a misclassified intent.
/// The LLM tends to map these to action="open" with target="PII dashboard"
/// because the panel-toggle vocabulary isn't in the prompt yet.
pub fn override_panel_intent(user_text: &str, intent: &mut UserIntent) {
    // Skip if already a panel action.
    if matches!(
        intent.action.as_str(),
        "open_pii_dashboard" | "open_models" | "open_inbox" | "browse" | "open_settings"
    ) {
        return;
    }

    let lower = user_text.to_lowercase();
    let new_action = if lower.contains("pii dashboard")
        || lower.contains("pii vault")
        || lower.contains("pii panel")
        || lower.contains("open pii")
        || lower.contains("show pii")
    {
        Some("open_pii_dashboard")
    } else if lower.contains("model panel")
        || lower.contains("model switcher")
        || lower.contains("model switch")
        || lower.contains("open models")
        || lower.contains("open the model")
    {
        Some("open_models")
    } else if lower.contains("open inbox")
        || lower.contains("show inbox")
        || lower.contains("open my inbox")
        || lower.contains("inbox panel")
    {
        Some("open_inbox")
    } else if lower.contains("open browser")
        || lower.contains("open the browser")
        || lower.contains("open web browser")
        || lower.contains("web browser")
        || lower == "browse"
        || lower.starts_with("browse ")
    {
        Some("browse")
    } else if lower.contains("open settings")
        || lower.contains("show settings")
        || lower.contains("open preferences")
        || lower == "settings"
    {
        Some("open_settings")
    } else {
        None
    };

    if let Some(action) = new_action {
        tracing::info!(
            "Overriding action '{}' → '{}' (user text matches panel-toggle pattern)",
            intent.action,
            action
        );
        intent.action = action.to_string();
        intent.confidence = 0.9;
    }
}

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

/// Known model family names used to detect swap intents without the word "model".
pub(crate) const MODEL_FAMILIES: &[&str] = &[
    "ministral", "mistral", "llama", "qwen", "phi", "gemma", "hermes",
];

/// Check if text contains a model-swap intent by combining action verbs with model names.
/// E.g. "switch to Ministral", "use llama", "load qwen 7b".
fn contains_model_swap_intent(lower: &str) -> bool {
    let has_model_name = MODEL_FAMILIES.iter().any(|name| lower.contains(name));
    if !has_model_name {
        return false;
    }
    lower.contains("switch")
        || lower.contains("swap")
        || lower.contains("change to")
        || lower.contains("load ")
        || lower.starts_with("use ")
        || lower.contains(" use ")
}

fn extract_intent_heuristic(response: &str) -> UserIntent {
    let lower = response.to_lowercase();

    // Model management intents (check early — before generic "show"/"list" catch-alls)
    let action = if lower.contains("swap model")
        || lower.contains("switch model")
        || lower.contains("change model")
        || lower.contains("use model")
        || contains_model_swap_intent(&lower)
    {
        "swap_model"
    } else if lower.contains("list models")
        || lower.contains("available models")
        || lower.contains("what models")
        || lower.contains("show models")
    {
        "list_models"
    // ── UI panel toggles ─────────────────────────────────────────────────
    // Must come before the generic "open"/"show"/"inbox" catch-alls below
    // so phrases like "open inbox" route to the panel toggle, not to a
    // document open or message-view.
    } else if lower.contains("pii dashboard")
        || lower.contains("pii vault")
        || lower.contains("pii panel")
        || lower.contains("open pii")
        || lower.contains("show pii")
    {
        "open_pii_dashboard"
    } else if lower.contains("model panel")
        || lower.contains("model switcher")
        || lower.contains("model switch")
        || lower.contains("open models")
        || lower.contains("open the model")
    {
        "open_models"
    } else if lower.contains("open inbox")
        || lower.contains("show inbox")
        || lower.contains("open my inbox")
        || lower.contains("inbox panel")
    {
        "open_inbox"
    } else if lower.contains("open browser")
        || lower.contains("open the browser")
        || lower.contains("open web browser")
        || lower.contains("web browser")
        || lower == "browse"
        || lower.starts_with("browse ")
    {
        "browse"
    } else if lower.contains("open settings")
        || lower.contains("show settings")
        || lower.contains("open preferences")
        || lower == "settings"
    {
        "open_settings"
    // Thread merge/split (check before generic thread ops)
    } else if lower.contains("merge thread") || lower.contains("combine thread") || lower.contains("merge project") {
        "merge_threads"
    } else if lower.contains("split thread") || lower.contains("separate thread") || lower.contains("split project") {
        "split_thread"
    // Thread-specific intents (check before generic "create"/"new")
    } else if lower.contains("create thread") || lower.contains("new thread") || lower.contains("new project") {
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
    } else if lower.contains("adopt") || lower.contains("claim") || lower.contains("take ownership") {
        "adopt"
    } else if lower.contains("create milestone") || lower.contains("add milestone") || lower.contains("set milestone") {
        "create_milestone"
    } else if lower.contains("list milestone") || lower.contains("show milestone") {
        "list_milestones"
    // P2P / Guardian / Encryption intents
    } else if lower.contains("sync") && (lower.contains("device") || lower.contains("peer")) {
        "sync_device"
    } else if lower.contains("pair device") || lower.contains("pair my") || lower.contains("connect device") {
        "pair_device"
    } else if lower.contains("list device") || lower.contains("show device") || lower.contains("paired device") {
        "list_devices"
    } else if lower.contains("enroll guardian") || lower.contains("add guardian") || lower.contains("new guardian") {
        "enroll_guardian"
    } else if lower.contains("list guardian") || lower.contains("show guardian") || lower.contains("my guardian") {
        "list_guardians"
    } else if lower.contains("revoke guardian") || lower.contains("remove guardian") {
        "revoke_guardian"
    } else if lower.contains("rotate shard") || lower.contains("rotate key") {
        "rotate_shards"
    } else if lower.contains("initiate recovery") || lower.contains("recover key") || lower.contains("start recovery") {
        "initiate_recovery"
    } else if lower.contains("sync status") || lower.contains("sync state") {
        "sync_status"
    } else if lower.contains("encrypt") && (lower.contains("data") || lower.contains("enable") || lower.contains("turn on")) {
        "encrypt_data"
    // Communications intents
    } else if lower.contains("list contact") || lower.contains("show contact") || lower.contains("my contact") {
        "list_contacts"
    } else if lower.contains("message") || lower.contains("conversation") || lower.contains("inbox") {
        "view_messages"
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
    } else if lower.contains("hello") || lower.contains("hi ") || lower.contains("hey ")
        || lower.contains("what is") || lower.contains("tell me") || lower.contains("explain")
        || lower.contains("how do") || lower.contains("can you") || lower.contains("help me")
    {
        "chat"
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

    #[test]
    fn heuristic_merge_threads() {
        let intent = parse_intent_response("merge thread Alpha into Beta").unwrap();
        assert_eq!(intent.action, "merge_threads");
    }

    #[test]
    fn heuristic_combine_threads() {
        let intent = parse_intent_response("combine threads Research and Dev").unwrap();
        assert_eq!(intent.action, "merge_threads");
    }

    #[test]
    fn heuristic_split_thread() {
        let intent = parse_intent_response("split thread Research into two").unwrap();
        assert_eq!(intent.action, "split_thread");
    }

    #[test]
    fn heuristic_separate_thread() {
        let intent = parse_intent_response("separate thread notes from main").unwrap();
        assert_eq!(intent.action, "split_thread");
    }

    #[test]
    fn heuristic_create_milestone() {
        let intent = parse_intent_response("create milestone Alpha release").unwrap();
        assert_eq!(intent.action, "create_milestone");
    }

    #[test]
    fn heuristic_add_milestone() {
        let intent = parse_intent_response("add milestone for v2.0").unwrap();
        assert_eq!(intent.action, "create_milestone");
    }

    #[test]
    fn heuristic_list_milestones() {
        let intent = parse_intent_response("list milestones for this project").unwrap();
        assert_eq!(intent.action, "list_milestones");
    }

    #[test]
    fn heuristic_show_milestones() {
        let intent = parse_intent_response("show milestones").unwrap();
        assert_eq!(intent.action, "list_milestones");
    }

    #[test]
    fn heuristic_swap_model() {
        let intent = parse_intent_response("swap model to Qwen2.5-7B").unwrap();
        assert_eq!(intent.action, "swap_model");
    }

    #[test]
    fn heuristic_switch_model() {
        let intent = parse_intent_response("switch model to something bigger").unwrap();
        assert_eq!(intent.action, "swap_model");
    }

    #[test]
    fn heuristic_change_model() {
        let intent = parse_intent_response("change model to the 7B variant").unwrap();
        assert_eq!(intent.action, "swap_model");
    }

    #[test]
    fn heuristic_use_model() {
        let intent = parse_intent_response("use model Qwen2.5-3B").unwrap();
        assert_eq!(intent.action, "swap_model");
    }

    #[test]
    fn heuristic_list_models() {
        let intent = parse_intent_response("list models").unwrap();
        assert_eq!(intent.action, "list_models");
    }

    #[test]
    fn heuristic_available_models() {
        let intent = parse_intent_response("what models are available").unwrap();
        assert_eq!(intent.action, "list_models");
    }

    #[test]
    fn heuristic_show_models() {
        let intent = parse_intent_response("show models please").unwrap();
        assert_eq!(intent.action, "list_models");
    }

    #[test]
    fn heuristic_list_contacts() {
        let intent = parse_intent_response("list my contacts").unwrap();
        assert_eq!(intent.action, "list_contacts");
    }

    #[test]
    fn heuristic_show_contacts() {
        let intent = parse_intent_response("show contacts").unwrap();
        assert_eq!(intent.action, "list_contacts");
    }

    #[test]
    fn heuristic_view_messages() {
        let intent = parse_intent_response("show my messages").unwrap();
        assert_eq!(intent.action, "view_messages");
    }

    #[test]
    fn heuristic_view_conversations() {
        let intent = parse_intent_response("open conversation with Alice").unwrap();
        assert_eq!(intent.action, "view_messages");
    }

    // --- Model-name-based swap detection (no "model" keyword) ---

    #[test]
    fn heuristic_switch_to_ministral() {
        let intent = parse_intent_response("switch to Ministral").unwrap();
        assert_eq!(intent.action, "swap_model");
    }

    #[test]
    fn heuristic_switch_to_llama() {
        let intent = parse_intent_response("switch to llama").unwrap();
        assert_eq!(intent.action, "swap_model");
    }

    #[test]
    fn heuristic_use_qwen() {
        let intent = parse_intent_response("use qwen for the router").unwrap();
        assert_eq!(intent.action, "swap_model");
    }

    #[test]
    fn heuristic_swap_to_mistral() {
        let intent = parse_intent_response("swap to the mistral 7b").unwrap();
        assert_eq!(intent.action, "swap_model");
    }

    #[test]
    fn heuristic_load_phi() {
        let intent = parse_intent_response("load phi instead").unwrap();
        assert_eq!(intent.action, "swap_model");
    }

    #[test]
    fn heuristic_change_to_hermes() {
        let intent = parse_intent_response("change to hermes please").unwrap();
        assert_eq!(intent.action, "swap_model");
    }

    // --- UI panel toggle actions (open_pii_dashboard, open_models, etc.) ---

    #[test]
    fn heuristic_open_pii_dashboard() {
        for phrase in [
            "open the PII dashboard please",
            "show pii",
            "open pii vault",
            "pii panel",
        ] {
            let intent = parse_intent_response(phrase).unwrap();
            assert_eq!(intent.action, "open_pii_dashboard", "phrase: {phrase}");
        }
    }

    #[test]
    fn heuristic_open_models_panel_does_not_collide_with_list_models() {
        // "show models" → list_models (existing behaviour preserved).
        let listing = parse_intent_response("show models").unwrap();
        assert_eq!(listing.action, "list_models");

        // But "open models" / "model panel" / "model switcher" → open_models.
        for phrase in [
            "open models",
            "open the model panel",
            "open the model switcher",
            "model switch",
        ] {
            let intent = parse_intent_response(phrase).unwrap();
            assert_eq!(intent.action, "open_models", "phrase: {phrase}");
        }
    }

    #[test]
    fn heuristic_open_inbox_specific_phrases() {
        // Specific "open inbox" / "show inbox" / "inbox panel" must hit open_inbox.
        for phrase in ["open inbox", "show inbox", "open my inbox", "inbox panel"] {
            let intent = parse_intent_response(phrase).unwrap();
            assert_eq!(intent.action, "open_inbox", "phrase: {phrase}");
        }
        // Generic mention of "inbox" without an open verb still routes to
        // view_messages (existing catch-all behaviour preserved).
        let generic = parse_intent_response("any new mail in the inbox today").unwrap();
        assert_eq!(generic.action, "view_messages");
    }

    #[test]
    fn heuristic_browse() {
        for phrase in ["open browser", "open the browser", "open web browser", "browse"] {
            let intent = parse_intent_response(phrase).unwrap();
            assert_eq!(intent.action, "browse", "phrase: {phrase}");
        }
        // "browse to acme.com" should also be browse (starts_with check).
        let intent = parse_intent_response("browse to acme.com").unwrap();
        assert_eq!(intent.action, "browse");
    }

    #[test]
    fn heuristic_open_settings() {
        for phrase in ["open settings", "show settings", "open preferences"] {
            let intent = parse_intent_response(phrase).unwrap();
            assert_eq!(intent.action, "open_settings", "phrase: {phrase}");
        }
    }

    // --- override_panel_intent: corrects LLM misclassification ---

    fn make_intent(action: &str) -> UserIntent {
        UserIntent {
            action: action.to_string(),
            target: None,
            confidence: 0.7,
            entities: vec![],
            origin: Plane::Control,
        }
    }

    #[test]
    fn override_panel_corrects_open_to_pii_dashboard() {
        // The router LLM commonly returns action="open" target="PII dashboard".
        let mut intent = make_intent("open");
        override_panel_intent("open the PII dashboard please", &mut intent);
        assert_eq!(intent.action, "open_pii_dashboard");
        assert!(intent.confidence >= 0.9);
    }

    #[test]
    fn override_panel_corrects_search_to_open_settings() {
        let mut intent = make_intent("search");
        override_panel_intent("open settings", &mut intent);
        assert_eq!(intent.action, "open_settings");
    }

    #[test]
    fn override_panel_corrects_open_to_browse() {
        let mut intent = make_intent("open");
        override_panel_intent("open the browser", &mut intent);
        assert_eq!(intent.action, "browse");
    }

    #[test]
    fn override_panel_corrects_view_messages_to_open_inbox() {
        let mut intent = make_intent("view_messages");
        override_panel_intent("show inbox", &mut intent);
        assert_eq!(intent.action, "open_inbox");
    }

    #[test]
    fn override_panel_corrects_open_to_models_panel() {
        let mut intent = make_intent("open");
        override_panel_intent("open the model panel", &mut intent);
        assert_eq!(intent.action, "open_models");
    }

    #[test]
    fn override_panel_does_not_touch_already_correct_action() {
        let mut intent = make_intent("open_pii_dashboard");
        intent.confidence = 0.5;
        override_panel_intent("open the PII dashboard", &mut intent);
        // Action unchanged AND confidence not bumped (override skipped).
        assert_eq!(intent.action, "open_pii_dashboard");
        assert_eq!(intent.confidence, 0.5);
    }

    #[test]
    fn override_panel_does_not_trigger_for_unrelated_text() {
        let mut intent = make_intent("search");
        override_panel_intent("find documents about Rust", &mut intent);
        assert_eq!(intent.action, "search", "unrelated text must not trigger override");
    }

    // --- Post-classification override tests ---

    #[test]
    fn override_corrects_search_to_swap_model() {
        let mut intent = UserIntent {
            action: "search".to_string(),
            target: Some("Ministral".to_string()),
            confidence: 0.85,
            entities: vec![],
            origin: Plane::Control,
        };
        override_model_intent("switch to Ministral", &mut intent);
        assert_eq!(intent.action, "swap_model");
        assert!((intent.confidence - 0.9).abs() < f32::EPSILON);
    }

    #[test]
    fn override_corrects_open_to_swap_model() {
        let mut intent = UserIntent {
            action: "open".to_string(),
            target: Some("llama".to_string()),
            confidence: 0.80,
            entities: vec![],
            origin: Plane::Control,
        };
        override_model_intent("use llama", &mut intent);
        assert_eq!(intent.action, "swap_model");
    }

    #[test]
    fn override_does_not_touch_correct_swap() {
        let mut intent = UserIntent {
            action: "swap_model".to_string(),
            target: Some("Ministral".to_string()),
            confidence: 0.95,
            entities: vec![],
            origin: Plane::Control,
        };
        override_model_intent("switch to Ministral", &mut intent);
        assert_eq!(intent.action, "swap_model");
        // Confidence stays at original value, not overridden
        assert!((intent.confidence - 0.95).abs() < f32::EPSILON);
    }

    #[test]
    fn override_does_not_trigger_for_unrelated() {
        let mut intent = UserIntent {
            action: "search".to_string(),
            target: Some("meeting notes".to_string()),
            confidence: 0.90,
            entities: vec![],
            origin: Plane::Control,
        };
        override_model_intent("find my meeting notes", &mut intent);
        assert_eq!(intent.action, "search");
    }

    #[test]
    fn override_corrects_to_list_models() {
        let mut intent = UserIntent {
            action: "chat".to_string(),
            target: None,
            confidence: 0.70,
            entities: vec![],
            origin: Plane::Control,
        };
        override_model_intent("what models are available?", &mut intent);
        assert_eq!(intent.action, "list_models");
    }
}
