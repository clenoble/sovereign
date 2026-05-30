use crate::content_util::replace_body;
use crate::manifest::Capability;
use crate::traits::{CoreSkill, SkillContext, SkillDocument, SkillOutput};

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
        params: &str,
        _ctx: &SkillContext,
    ) -> anyhow::Result<SkillOutput> {
        match action {
            "normalize" => {
                let normalized = normalize_markdown(&doc.content.body);
                Ok(SkillOutput::ContentUpdate(replace_body(doc, normalized)))
            }
            "sort_lists" => {
                let sorted = sort_list_blocks(&doc.content.body);
                Ok(SkillOutput::ContentUpdate(replace_body(doc, sorted)))
            }
            "convert_case" => {
                let mode = CaseMode::from_str(params)?;
                let converted = convert_case(&doc.content.body, mode);
                Ok(SkillOutput::ContentUpdate(replace_body(doc, converted)))
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
            ("sort_lists".into(), "Sort List Items A–Z".into()),
            ("convert_case".into(), "Convert Case".into()),
            ("preview".into(), "Preview".into()),
        ]
    }

    fn file_types(&self) -> Vec<String> {
        vec!["md".into()]
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum CaseMode {
    Title,
    Upper,
    Lower,
    Camel,
    Snake,
}

impl CaseMode {
    fn from_str(s: &str) -> anyhow::Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "title" => Ok(CaseMode::Title),
            "upper" | "uppercase" => Ok(CaseMode::Upper),
            "lower" | "lowercase" => Ok(CaseMode::Lower),
            "camel" | "camelcase" => Ok(CaseMode::Camel),
            "snake" | "snake_case" => Ok(CaseMode::Snake),
            other => anyhow::bail!(
                "Unknown case mode: {other:?}. Expected one of: title, upper, lower, camel, snake"
            ),
        }
    }
}

fn convert_case(body: &str, mode: CaseMode) -> String {
    match mode {
        CaseMode::Upper => body.to_uppercase(),
        CaseMode::Lower => body.to_lowercase(),
        CaseMode::Title => to_title_case(body),
        CaseMode::Camel => to_camel_case(body),
        CaseMode::Snake => to_snake_case(body),
    }
}

/// Capitalize the first letter of every whitespace-separated word; lowercase
/// the rest. Whitespace and punctuation preserved as-is.
fn to_title_case(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut at_word_start = true;
    for c in body.chars() {
        if c.is_whitespace() {
            at_word_start = true;
            out.push(c);
        } else if at_word_start {
            out.extend(c.to_uppercase());
            at_word_start = false;
        } else {
            out.extend(c.to_lowercase());
        }
    }
    out
}

/// Split on non-alphanumeric runs; first segment lowercase, subsequent
/// segments capitalized. Identifier-style transformation — drops original
/// whitespace and punctuation.
fn to_camel_case(body: &str) -> String {
    let segments: Vec<&str> = body
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect();
    let mut out = String::new();
    for (i, seg) in segments.iter().enumerate() {
        if i == 0 {
            out.push_str(&seg.to_lowercase());
        } else {
            let mut chars = seg.chars();
            if let Some(first) = chars.next() {
                out.extend(first.to_uppercase());
            }
            out.push_str(&chars.as_str().to_lowercase());
        }
    }
    out
}

/// Split on non-alphanumeric runs; lowercase all segments, join with `_`.
fn to_snake_case(body: &str) -> String {
    body.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
        .collect::<Vec<_>>()
        .join("_")
}

/// Detect contiguous list blocks (all lines starting with the same marker
/// type) and sort each block's items alphabetically. Non-list lines are
/// passed through unchanged.
fn sort_list_blocks(body: &str) -> String {
    let lines: Vec<&str> = body.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut block: Vec<String> = Vec::new();
    let mut block_kind: Option<ListKind> = None;

    for line in &lines {
        match list_kind(line) {
            Some(kind) if block_kind == Some(kind) || block_kind.is_none() => {
                block.push(line.to_string());
                block_kind = Some(kind);
            }
            _ => {
                flush_block(&mut out, &mut block, &mut block_kind);
                out.push(line.to_string());
            }
        }
    }
    flush_block(&mut out, &mut block, &mut block_kind);

    let mut joined = out.join("\n");
    if body.ends_with('\n') {
        joined.push('\n');
    }
    joined
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum ListKind {
    Unordered, // - or *
    Ordered,   // 1.
}

fn list_kind(line: &str) -> Option<ListKind> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
        Some(ListKind::Unordered)
    } else {
        // Ordered: "<digits>. "
        let mut chars = trimmed.chars();
        let mut digits = 0;
        while let Some(c) = chars.next() {
            if c.is_ascii_digit() {
                digits += 1;
            } else if digits > 0 && c == '.' && chars.next() == Some(' ') {
                return Some(ListKind::Ordered);
            } else {
                return None;
            }
        }
        None
    }
}

fn flush_block(
    out: &mut Vec<String>,
    block: &mut Vec<String>,
    block_kind: &mut Option<ListKind>,
) {
    if block.is_empty() {
        return;
    }
    // Sort by item text, ignoring the leading marker for comparison.
    block.sort_by(|a, b| sort_key(a).cmp(&sort_key(b)));
    // For ordered lists, renumber from 1.
    if *block_kind == Some(ListKind::Ordered) {
        for (i, line) in block.iter_mut().enumerate() {
            if let Some(after_marker) = strip_ordered_marker(line) {
                let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
                *line = format!("{indent}{}. {after_marker}", i + 1);
            }
        }
    }
    out.extend(block.drain(..));
    *block_kind = None;
}

fn sort_key(line: &str) -> String {
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
        rest.to_lowercase()
    } else if let Some(rest) = strip_ordered_marker(trimmed) {
        rest.to_lowercase()
    } else {
        trimmed.to_lowercase()
    }
}

fn strip_ordered_marker(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let mut chars = trimmed.chars().peekable();
    let mut digit_count = 0;
    while chars.peek().is_some_and(|c| c.is_ascii_digit()) {
        chars.next();
        digit_count += 1;
    }
    if digit_count == 0 {
        return None;
    }
    if chars.next() != Some('.') {
        return None;
    }
    if chars.next() != Some(' ') {
        return None;
    }
    Some(chars.collect())
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
    use crate::test_util::{dummy_ctx, make_doc};

    #[test]
    fn normalize_adds_blank_lines_around_headings() {
        let doc = make_doc("text\n# Heading\nmore text");
        let skill = MarkdownEditorSkill;
        let result = skill.execute("normalize", &doc, "", &dummy_ctx()).unwrap();
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
        let result = skill.execute("preview", &doc, "", &dummy_ctx()).unwrap();
        assert!(matches!(result, SkillOutput::StructuredData { .. }));
    }

    #[test]
    fn unknown_action_fails() {
        let doc = make_doc("");
        let skill = MarkdownEditorSkill;
        assert!(skill.execute("unknown", &doc, "", &dummy_ctx()).is_err());
    }

    #[test]
    fn actions_list() {
        let skill = MarkdownEditorSkill;
        let actions = skill.actions();
        assert_eq!(actions.len(), 4);
        assert_eq!(actions[0].0, "normalize");
        assert_eq!(actions[1].0, "sort_lists");
        assert_eq!(actions[2].0, "convert_case");
        assert_eq!(actions[3].0, "preview");
    }

    fn run(action: &str, body: &str, params: &str) -> String {
        let skill = MarkdownEditorSkill;
        let doc = make_doc(body);
        let result = skill.execute(action, &doc, params, &dummy_ctx()).unwrap();
        match result {
            SkillOutput::ContentUpdate(cf) => cf.body,
            _ => panic!("Expected ContentUpdate"),
        }
    }

    #[test]
    fn sort_lists_orders_unordered_items_alphabetically() {
        let body = "- cherry\n- apple\n- banana\n";
        let out = run("sort_lists", body, "");
        assert_eq!(out, "- apple\n- banana\n- cherry\n");
    }

    #[test]
    fn sort_lists_renumbers_ordered_lists() {
        let body = "1. cherry\n2. apple\n3. banana\n";
        let out = run("sort_lists", body, "");
        assert_eq!(out, "1. apple\n2. banana\n3. cherry\n");
    }

    #[test]
    fn sort_lists_leaves_non_list_lines_alone() {
        let body = "Heading\n\n- cherry\n- apple\n\nfooter\n";
        let out = run("sort_lists", body, "");
        assert!(out.contains("Heading\n\n- apple\n- cherry\n\nfooter"));
    }

    #[test]
    fn sort_lists_handles_multiple_separated_blocks() {
        let body = "- z\n- a\n\n# Heading\n\n- y\n- b\n";
        let out = run("sort_lists", body, "");
        assert!(out.contains("- a\n- z\n"));
        assert!(out.contains("- b\n- y\n"));
    }

    #[test]
    fn case_title_capitalizes_each_word() {
        let out = run("convert_case", "hello world FOO", "title");
        assert_eq!(out, "Hello World Foo");
    }

    #[test]
    fn case_upper_uppercases_everything() {
        let out = run("convert_case", "Hello, world!", "upper");
        assert_eq!(out, "HELLO, WORLD!");
    }

    #[test]
    fn case_lower_lowercases_everything() {
        let out = run("convert_case", "Hello, World!", "lower");
        assert_eq!(out, "hello, world!");
    }

    #[test]
    fn case_camel_strips_separators_and_lowers_first() {
        let out = run("convert_case", "Hello World Foo", "camel");
        assert_eq!(out, "helloWorldFoo");
    }

    #[test]
    fn case_snake_joins_with_underscores() {
        let out = run("convert_case", "Hello World Foo", "snake");
        assert_eq!(out, "hello_world_foo");
    }

    #[test]
    fn case_unknown_mode_errors() {
        let skill = MarkdownEditorSkill;
        let doc = make_doc("anything");
        let result = skill.execute("convert_case", &doc, "klingon", &dummy_ctx());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown case mode"));
    }

    #[test]
    fn case_mode_aliases_accepted() {
        // "uppercase" -> Upper, "snake_case" -> Snake
        let out = run("convert_case", "hello", "uppercase");
        assert_eq!(out, "HELLO");
        let out = run("convert_case", "hello world", "snake_case");
        assert_eq!(out, "hello_world");
    }
}
