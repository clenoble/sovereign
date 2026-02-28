wit_bindgen::generate!({
    world: "skill-plugin",
    path: "wit",
});

struct WordCountWasm;

impl Guest for WordCountWasm {
    fn name() -> String {
        "word-count-wasm".to_string()
    }

    fn required_capabilities() -> Vec<sovereign::skill::types::Capability> {
        use sovereign::skill::types::Capability;
        vec![Capability::ReadDocument]
    }

    fn actions() -> Vec<(String, String)> {
        vec![("count".to_string(), "Word Count".to_string())]
    }

    fn file_types() -> Vec<String> {
        vec!["md".to_string(), "txt".to_string()]
    }

    fn execute(
        action: String,
        doc: sovereign::skill::types::SkillDocument,
        _params: String,
        _granted_capabilities: Vec<sovereign::skill::types::Capability>,
    ) -> Result<sovereign::skill::types::SkillOutput, String> {
        use sovereign::skill::types::{SkillOutput, StructuredOutput};

        match action.as_str() {
            "count" => {
                let body = &doc.body;
                let words = body.split_whitespace().count();
                let characters = body.chars().count();
                let lines = if body.is_empty() {
                    0
                } else {
                    body.lines().count()
                };
                let reading_time_min = ((words as f64) / 200.0).ceil() as u64;

                let json = format!(
                    r#"{{"words":{},"characters":{},"lines":{},"reading_time_min":{}}}"#,
                    words, characters, lines, reading_time_min
                );

                Ok(SkillOutput::StructuredData(StructuredOutput {
                    kind: "word_count".to_string(),
                    json,
                }))
            }
            _ => Err(format!("Unknown action: {action}")),
        }
    }
}

export!(WordCountWasm);
