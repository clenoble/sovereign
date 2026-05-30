use std::sync::OnceLock;

use regex::Regex;

use crate::content_util::replace_body;
use crate::manifest::Capability;
use crate::traits::{CoreSkill, SkillContext, SkillDocument, SkillOutput};

pub struct JsonYamlFormatterSkill;

#[derive(Copy, Clone)]
enum Mode {
    Format,
    Minify,
}

impl CoreSkill for JsonYamlFormatterSkill {
    fn name(&self) -> &str {
        "json-yaml-formatter"
    }

    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::ReadDocument, Capability::WriteDocument]
    }

    fn activate(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn deactivate(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn execute(
        &self,
        action: &str,
        doc: &SkillDocument,
        _params: &str,
        _ctx: &SkillContext,
    ) -> anyhow::Result<SkillOutput> {
        let mode = match action {
            "format" => Mode::Format,
            "minify" => Mode::Minify,
            _ => anyhow::bail!("Unknown action: {action}"),
        };
        let new_body = transform_blocks(&doc.content.body, mode);
        Ok(SkillOutput::ContentUpdate(replace_body(doc, new_body)))
    }

    fn actions(&self) -> Vec<(String, String)> {
        vec![
            ("format".into(), "Format JSON/YAML blocks".into()),
            ("minify".into(), "Minify JSON/YAML blocks".into()),
        ]
    }

    fn file_types(&self) -> Vec<String> {
        vec!["md".into(), "json".into(), "yaml".into(), "yml".into()]
    }
}

fn fence_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // Capture: lang in group 1, content in group 2.
        Regex::new(r"(?ms)^```(json|yaml|yml)\s*\n(.*?)\n```\s*$").unwrap()
    })
}

fn transform_blocks(body: &str, mode: Mode) -> String {
    fence_re()
        .replace_all(body, |caps: &regex::Captures| {
            let lang = &caps[1];
            let content = &caps[2];
            let transformed = match lang {
                "json" => transform_json(content, mode),
                _ => transform_yaml(content, mode),
            };
            // On parse error, leave the block untouched.
            let inner = transformed.unwrap_or_else(|_| content.to_string());
            format!("```{lang}\n{inner}\n```")
        })
        .into_owned()
}

fn transform_json(text: &str, mode: Mode) -> anyhow::Result<String> {
    let value: serde_json::Value = serde_json::from_str(text)?;
    Ok(match mode {
        Mode::Format => serde_json::to_string_pretty(&value)?,
        Mode::Minify => serde_json::to_string(&value)?,
    })
}

fn transform_yaml(text: &str, mode: Mode) -> anyhow::Result<String> {
    let value: serde_yml::Value = serde_yml::from_str(text)?;
    let formatted = serde_yml::to_string(&value)?;
    Ok(match mode {
        // serde_yml::to_string already produces multi-line block style; that
        // is the "formatted" shape.
        Mode::Format => formatted.trim_end().to_string(),
        Mode::Minify => yaml_to_flow(&formatted),
    })
}

/// Naive flow-style minify: parse the formatted YAML back and re-emit
/// using compact JSON-compatible flow syntax. YAML supports flow style for
/// mappings and sequences via `{}` and `[]`. We use serde_json as the
/// emitter since flow YAML is a superset of JSON for this shape.
fn yaml_to_flow(formatted_yaml: &str) -> String {
    let parsed: Result<serde_yml::Value, _> = serde_yml::from_str(formatted_yaml);
    match parsed {
        Ok(v) => {
            // Convert serde_yml::Value -> serde_json::Value for compact flow
            // emission. Lossy on YAML-only types (anchors, tags) — acceptable
            // for a minify operation on simple docs.
            let json: serde_json::Value = serde_json::to_value(v).unwrap_or(serde_json::Value::Null);
            serde_json::to_string(&json).unwrap_or_else(|_| formatted_yaml.to_string())
        }
        Err(_) => formatted_yaml.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{dummy_ctx, make_doc};

    fn run(action: &str, body: &str) -> String {
        let skill = JsonYamlFormatterSkill;
        let doc = make_doc(body);
        match skill.execute(action, &doc, "", &dummy_ctx()).unwrap() {
            SkillOutput::ContentUpdate(cf) => cf.body,
            _ => panic!("expected ContentUpdate"),
        }
    }

    #[test]
    fn formats_json_block() {
        let body = "before\n\n```json\n{\"a\":1,\"b\":[2,3]}\n```\n\nafter\n";
        let out = run("format", body);
        assert!(out.contains("before"));
        assert!(out.contains("after"));
        assert!(out.contains("\"a\": 1"));
        assert!(out.contains("\"b\": ["));
    }

    #[test]
    fn minifies_json_block() {
        let body = "```json\n{\n  \"a\": 1,\n  \"b\": 2\n}\n```\n";
        let out = run("minify", body);
        assert!(out.contains("{\"a\":1,\"b\":2}"));
    }

    #[test]
    fn formats_yaml_block() {
        let body = "```yaml\na: 1\nb:\n  - 2\n  - 3\n```\n";
        let out = run("format", body);
        // serde_yml round-trip output should still contain the keys
        assert!(out.contains("a:"));
        assert!(out.contains("b:"));
    }

    #[test]
    fn minify_yaml_produces_flow_style() {
        let body = "```yaml\na: 1\nb: 2\n```\n";
        let out = run("minify", body);
        // Compact JSON-style flow mapping
        assert!(out.contains(r#"{"a":1,"b":2}"#));
    }

    #[test]
    fn leaves_invalid_json_untouched() {
        let body = "```json\nthis is not json\n```\n";
        let out = run("format", body);
        assert!(out.contains("this is not json"));
    }

    #[test]
    fn ignores_non_json_yaml_fences() {
        let body = "```rust\nfn main() {}\n```\n";
        let out = run("format", body);
        assert_eq!(out, body);
    }

    #[test]
    fn handles_multiple_blocks() {
        let body = "```json\n{\"a\":1}\n```\n\ntext\n\n```yaml\nb: 2\n```\n";
        let out = run("format", body);
        assert!(out.contains("\"a\": 1"));
        assert!(out.contains("b: 2"));
    }
}
