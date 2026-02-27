use crate::manifest::Capability;
use crate::traits::{CoreSkill, SkillContext, SkillDocument, SkillOutput};
use sovereign_core::content::ContentFields;

/// Skill for document-level markdown operations.
///
/// Cursor-dependent formatting (bold, italic, etc.) is handled directly
/// in the UI layer. This skill handles whole-document operations.
pub struct MarkdownEditorSkill;

impl CoreSkill for MarkdownEditorSkill {
    fn name(&self) -> &str {
        "markdown-editor"
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
        match action {
            "normalize" => {
                let normalized = normalize_markdown(&doc.content.body);
                let updated = ContentFields {
                    body: normalized,
                    images: doc.content.images.clone(),
                    videos: doc.content.videos.clone(),
                };
                Ok(SkillOutput::ContentUpdate(updated))
            }
            "preview" => Ok(SkillOutput::StructuredData {
                kind: "preview_hint".into(),
                json: r#"{"action":"toggle_preview"}"#.into(),
            }),
            _ => anyhow::bail!("Unknown action: {action}"),
        }
    }

    fn actions(&self) -> Vec<(String, String)> {
        vec![
            ("normalize".into(), "Normalize".into()),
            ("preview".into(), "Preview".into()),
        ]
    }

    fn file_types(&self) -> Vec<String> {
        vec!["md".into()]
    }
}

/// Normalize markdown: ensure blank lines around headings, trim trailing whitespace,
/// collapse multiple blank lines into one.
fn normalize_markdown(body: &str) -> String {
    let mut result = Vec::new();
    let lines: Vec<&str> = body.lines().collect();
    let mut prev_blank = false;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_end();

        // Collapse multiple blank lines
        if trimmed.is_empty() {
            if !prev_blank {
                result.push(String::new());
            }
            prev_blank = true;
            continue;
        }
        prev_blank = false;

        let is_heading = trimmed.starts_with('#');

        // Ensure blank line before heading (unless at start)
        if is_heading && i > 0 && !result.last().map_or(true, |l: &String| l.is_empty()) {
            result.push(String::new());
        }

        result.push(trimmed.to_string());

        // Ensure blank line after heading
        if is_heading {
            let next_non_empty = lines.get(i + 1).map(|l| !l.trim().is_empty()).unwrap_or(false);
            if next_non_empty {
                result.push(String::new());
            }
        }
    }

    // Trim trailing blank lines
    while result.last().map_or(false, |l| l.is_empty()) {
        result.pop();
    }

    result.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use sovereign_core::content::ContentFields;

    fn make_doc(body: &str) -> SkillDocument {
        SkillDocument {
            id: "document:test".into(),
            title: "Test".into(),
            content: ContentFields {
                body: body.into(),
                images: vec![],
                videos: vec![],
            },
        }
    }

    #[test]
    fn normalize_adds_blank_lines_around_headings() {
        let doc = make_doc("text\n# Heading\nmore text");
        let skill = MarkdownEditorSkill;
        let result = skill.execute("normalize", &doc, "", &SkillContext { granted: std::collections::HashSet::new(), db: None }).unwrap();
        match result {
            SkillOutput::ContentUpdate(cf) => {
                assert!(cf.body.contains("text\n\n# Heading\n\nmore text"));
            }
            _ => panic!("Expected ContentUpdate"),
        }
    }

    #[test]
    fn normalize_collapses_multiple_blank_lines() {
        let normalized = normalize_markdown("a\n\n\n\nb");
        assert_eq!(normalized, "a\n\nb");
    }

    #[test]
    fn normalize_trims_trailing_whitespace() {
        let normalized = normalize_markdown("hello   \nworld  ");
        assert_eq!(normalized, "hello\nworld");
    }

    #[test]
    fn preview_returns_structured_data() {
        let doc = make_doc("# Test");
        let skill = MarkdownEditorSkill;
        let result = skill.execute("preview", &doc, "", &SkillContext { granted: std::collections::HashSet::new(), db: None }).unwrap();
        assert!(matches!(result, SkillOutput::StructuredData { .. }));
    }

    #[test]
    fn unknown_action_fails() {
        let doc = make_doc("");
        let skill = MarkdownEditorSkill;
        assert!(skill.execute("unknown", &doc, "", &SkillContext { granted: std::collections::HashSet::new(), db: None }).is_err());
    }

    #[test]
    fn actions_list() {
        let skill = MarkdownEditorSkill;
        let actions = skill.actions();
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].0, "normalize");
        assert_eq!(actions[1].0, "preview");
    }
}
