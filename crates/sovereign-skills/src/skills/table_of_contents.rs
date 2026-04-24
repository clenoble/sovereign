use crate::content_util::replace_body;
use crate::manifest::Capability;
use crate::markdown_util::{scan_headings, slugify};
use crate::traits::{CoreSkill, SkillContext, SkillDocument, SkillOutput};

pub struct TableOfContentsSkill;

const TOC_OPEN: &str = "<!-- toc -->";
const TOC_CLOSE: &str = "<!-- /toc -->";

impl CoreSkill for TableOfContentsSkill {
    fn name(&self) -> &str {
        "table-of-contents"
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
        let body = &doc.content.body;
        let new_body = match action {
            "insert" => insert_or_replace(body),
            "update" => {
                if !body.contains(TOC_OPEN) {
                    anyhow::bail!("No TOC marker found. Use the `insert` action first.")
                }
                insert_or_replace(body)
            }
            _ => anyhow::bail!("Unknown action: {action}"),
        };
        Ok(SkillOutput::ContentUpdate(replace_body(doc, new_body)))
    }

    fn actions(&self) -> Vec<(String, String)> {
        vec![
            ("insert".into(), "Insert TOC".into()),
            ("update".into(), "Update TOC".into()),
        ]
    }

    fn file_types(&self) -> Vec<String> {
        vec!["md".into(), "markdown".into()]
    }
}

fn insert_or_replace(body: &str) -> String {
    let toc = build_toc(body);
    let block = format!("{TOC_OPEN}\n{toc}\n{TOC_CLOSE}");

    if let Some(start) = body.find(TOC_OPEN) {
        if let Some(close_rel) = body[start..].find(TOC_CLOSE) {
            let end = start + close_rel + TOC_CLOSE.len();
            let mut out = String::with_capacity(body.len());
            out.push_str(&body[..start]);
            out.push_str(&block);
            out.push_str(&body[end..]);
            return out;
        }
    }

    if let Some(first_h1_end) = first_h1_line_end(body) {
        let mut out = String::with_capacity(body.len() + block.len() + 4);
        out.push_str(&body[..first_h1_end]);
        out.push_str("\n\n");
        out.push_str(&block);
        out.push_str(&body[first_h1_end..]);
        out
    } else {
        format!("{block}\n\n{body}")
    }
}

/// Byte offset of the newline ending the first `# Heading` line, or None.
fn first_h1_line_end(body: &str) -> Option<usize> {
    let mut offset = 0usize;
    let mut in_fence = false;
    for line in body.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
        } else if !in_fence
            && trimmed.starts_with("# ")
            && !trimmed.starts_with("## ")
        {
            return Some(offset + line.len() - usize::from(line.ends_with('\n')));
        }
        offset += line.len();
    }
    None
}

fn build_toc(body: &str) -> String {
    let headings = scan_headings(body);
    if headings.is_empty() {
        return String::from("_(no headings found)_");
    }
    let min_level = headings
        .iter()
        .map(|(l, _)| *l)
        .filter(|l| *l > 1)
        .min()
        .unwrap_or(2);

    let mut out = String::new();
    for (level, text) in &headings {
        if *level < min_level {
            continue;
        }
        let indent = "  ".repeat((*level as usize) - (min_level as usize));
        let slug = slugify(text);
        out.push_str(&format!("{indent}- [{text}](#{slug})\n"));
    }
    out.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{dummy_ctx, make_doc};

    fn run(action: &str, body: &str) -> anyhow::Result<String> {
        let skill = TableOfContentsSkill;
        let doc = make_doc(body);
        match skill.execute(action, &doc, "", &dummy_ctx())? {
            SkillOutput::ContentUpdate(cf) => Ok(cf.body),
            _ => panic!("expected ContentUpdate"),
        }
    }

    #[test]
    fn insert_adds_toc_after_h1() {
        let body = "# Title\n\nIntro\n\n## Section A\n\n## Section B\n";
        let out = run("insert", body).unwrap();
        assert!(out.contains(TOC_OPEN));
        assert!(out.contains(TOC_CLOSE));
        assert!(out.contains("- [Section A](#section-a)"));
        assert!(out.contains("- [Section B](#section-b)"));
        let toc_pos = out.find(TOC_OPEN).unwrap();
        let intro_pos = out.find("Intro").unwrap();
        assert!(toc_pos < intro_pos);
    }

    #[test]
    fn insert_at_top_when_no_h1() {
        let body = "## A\n## B\n";
        let out = run("insert", body).unwrap();
        assert!(out.starts_with(TOC_OPEN));
    }

    #[test]
    fn insert_replaces_existing_block() {
        let body = format!(
            "# T\n\n{TOC_OPEN}\n- [Old](#old)\n{TOC_CLOSE}\n\n## New Section\n",
        );
        let out = run("insert", &body).unwrap();
        assert!(!out.contains("Old"));
        assert!(out.contains("- [New Section](#new-section)"));
        assert_eq!(out.matches(TOC_OPEN).count(), 1);
    }

    #[test]
    fn update_errors_when_no_marker() {
        let body = "# T\n\n## A\n";
        let result = run("update", body);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No TOC marker"));
    }

    #[test]
    fn update_refreshes_existing() {
        let body = format!(
            "# T\n\n{TOC_OPEN}\n- [Old](#old)\n{TOC_CLOSE}\n\n## A\n",
        );
        let out = run("update", &body).unwrap();
        assert!(out.contains("- [A](#a)"));
        assert!(!out.contains("Old"));
    }

    #[test]
    fn nested_indent_for_h3() {
        let body = "# T\n\n## A\n### A1\n### A2\n## B\n";
        let out = run("insert", body).unwrap();
        assert!(out.contains("- [A](#a)"));
        assert!(out.contains("  - [A1](#a1)"));
        assert!(out.contains("  - [A2](#a2)"));
    }
}
