use serde::Serialize;

use crate::manifest::Capability;
use crate::traits::{CoreSkill, SkillContext, SkillDocument, SkillOutput};

pub struct OutlineExtractorSkill;

#[derive(Debug, Serialize)]
struct OutlineNode {
    level: u8,
    text: String,
    children: Vec<OutlineNode>,
}

impl CoreSkill for OutlineExtractorSkill {
    fn name(&self) -> &str {
        "outline-extractor"
    }

    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::ReadDocument]
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
            "extract" => {
                let headings = extract_headings(&doc.content.body);
                let outline = build_tree(headings);
                let json = serde_json::to_string(&serde_json::json!({
                    "outline": outline,
                    "count": count_nodes(&outline),
                }))?;
                Ok(SkillOutput::StructuredData {
                    kind: "outline".into(),
                    json,
                })
            }
            _ => anyhow::bail!("Unknown action: {action}"),
        }
    }

    fn actions(&self) -> Vec<(String, String)> {
        vec![("extract".into(), "Extract Outline".into())]
    }

    fn file_types(&self) -> Vec<String> {
        vec!["md".into(), "markdown".into()]
    }
}

/// Parse ATX-style markdown headings (#, ##, ...) into (level, text) pairs.
/// Skips fenced code blocks so `# foo` inside ``` is not detected as a heading.
fn extract_headings(body: &str) -> Vec<(u8, String)> {
    let mut headings = Vec::new();
    let mut in_fence = false;
    let mut fence_marker: Option<&str> = None;

    for line in body.lines() {
        let trimmed = line.trim_start();
        if let Some(marker) = fence_marker {
            if trimmed.starts_with(marker) {
                in_fence = false;
                fence_marker = None;
            }
            continue;
        }
        if trimmed.starts_with("```") {
            in_fence = true;
            fence_marker = Some("```");
            continue;
        }
        if trimmed.starts_with("~~~") {
            in_fence = true;
            fence_marker = Some("~~~");
            continue;
        }
        if !in_fence {
            if let Some((level, text)) = parse_atx_heading(trimmed) {
                headings.push((level, text));
            }
        }
    }
    headings
}

fn parse_atx_heading(line: &str) -> Option<(u8, String)> {
    let mut level = 0u8;
    let bytes = line.as_bytes();
    while level < 6 && bytes.get(level as usize) == Some(&b'#') {
        level += 1;
    }
    if level == 0 {
        return None;
    }
    // Heading must be followed by space or end-of-line
    let rest = &line[level as usize..];
    if !rest.is_empty() && !rest.starts_with(' ') {
        return None;
    }
    let text = rest.trim().trim_end_matches('#').trim().to_string();
    Some((level, text))
}

/// Build a nested tree from a flat heading sequence.
/// Headings of higher level (deeper) become children of the most recent
/// heading of lower level.
fn build_tree(headings: Vec<(u8, String)>) -> Vec<OutlineNode> {
    let mut roots: Vec<OutlineNode> = Vec::new();
    for (level, text) in headings {
        let node = OutlineNode {
            level,
            text,
            children: Vec::new(),
        };
        insert_at_level(&mut roots, node);
    }
    roots
}

fn insert_at_level(siblings: &mut Vec<OutlineNode>, node: OutlineNode) {
    if let Some(last) = siblings.last_mut() {
        if node.level > last.level {
            insert_at_level(&mut last.children, node);
            return;
        }
    }
    siblings.push(node);
}

fn count_nodes(nodes: &[OutlineNode]) -> usize {
    nodes.iter().map(|n| 1 + count_nodes(&n.children)).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sovereign_core::content::ContentFields;

    fn dummy_ctx() -> SkillContext {
        SkillContext { granted: std::collections::HashSet::new(), db: None, llm: None }
    }

    fn make_doc(body: &str) -> SkillDocument {
        SkillDocument {
            id: "document:test".into(),
            title: "Test".into(),
            content: ContentFields { body: body.into(), ..Default::default() },
        }
    }

    #[test]
    fn extracts_flat_headings() {
        let skill = OutlineExtractorSkill;
        let doc = make_doc("# A\nbody\n## B\n## C\n");
        let result = skill.execute("extract", &doc, "", &dummy_ctx()).unwrap();
        match result {
            SkillOutput::StructuredData { json, .. } => {
                let v: serde_json::Value = serde_json::from_str(&json).unwrap();
                assert_eq!(v["count"], 3);
                assert_eq!(v["outline"][0]["text"], "A");
                assert_eq!(v["outline"][0]["children"][0]["text"], "B");
                assert_eq!(v["outline"][0]["children"][1]["text"], "C");
            }
            _ => panic!("expected StructuredData"),
        }
    }

    #[test]
    fn ignores_headings_in_code_fences() {
        let skill = OutlineExtractorSkill;
        let doc = make_doc("# Real\n```\n# Fake\n```\n## Also Real\n");
        let result = skill.execute("extract", &doc, "", &dummy_ctx()).unwrap();
        if let SkillOutput::StructuredData { json, .. } = result {
            let v: serde_json::Value = serde_json::from_str(&json).unwrap();
            assert_eq!(v["count"], 2);
            assert_eq!(v["outline"][0]["text"], "Real");
            assert_eq!(v["outline"][0]["children"][0]["text"], "Also Real");
        } else {
            panic!("expected StructuredData");
        }
    }

    #[test]
    fn strips_trailing_hashes() {
        let h = parse_atx_heading("## Section ##");
        assert_eq!(h, Some((2, "Section".into())));
    }

    #[test]
    fn rejects_atx_without_space() {
        // "#foo" is not a heading per CommonMark
        assert_eq!(parse_atx_heading("#foo"), None);
    }

    #[test]
    fn handles_jagged_levels() {
        // h1 -> h3 -> h2: h3 nests under h1, h2 starts new top-level child
        let skill = OutlineExtractorSkill;
        let doc = make_doc("# Top\n### Deep\n## Mid\n");
        let result = skill.execute("extract", &doc, "", &dummy_ctx()).unwrap();
        if let SkillOutput::StructuredData { json, .. } = result {
            let v: serde_json::Value = serde_json::from_str(&json).unwrap();
            assert_eq!(v["count"], 3);
            assert_eq!(v["outline"][0]["children"][0]["text"], "Deep");
            assert_eq!(v["outline"][0]["children"][1]["text"], "Mid");
        }
    }
}
