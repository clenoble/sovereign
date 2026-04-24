use serde::Serialize;

use crate::manifest::Capability;
use crate::markdown_util::{scan_headings, Heading};
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
                let headings = scan_headings(&doc.content.body);
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

/// Build a nested tree from a flat heading sequence. Headings of higher
/// level (deeper) become children of the most recent heading of lower level.
fn build_tree(headings: Vec<Heading>) -> Vec<OutlineNode> {
    let mut roots: Vec<OutlineNode> = Vec::new();
    for (level, text) in headings {
        let node = OutlineNode { level, text, children: Vec::new() };
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
    use crate::test_util::{dummy_ctx, make_doc};

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
    fn handles_jagged_levels() {
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
