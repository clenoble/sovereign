use sovereign_core::content::ContentFields;

use crate::manifest::Capability;
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
        Ok(SkillOutput::ContentUpdate(ContentFields {
            body: new_body,
            images: doc.content.images.clone(),
            videos: doc.content.videos.clone(),
        }))
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

    // No marker present — insert after the first H1 if any, else at the top.
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
            // include the trailing newline
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
    // Skip the document's H1 (usually the title) — TOC starts at H2.
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

/// ATX-only heading scanner, fence-aware. Matches `#`..`######` followed by a
/// space. Returns (level, text) pairs in document order.
fn scan_headings(body: &str) -> Vec<(u8, String)> {
    let mut out = Vec::new();
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
            if let Some((l, t)) = parse_atx(trimmed) {
                out.push((l, t));
            }
        }
    }
    out
}

fn parse_atx(line: &str) -> Option<(u8, String)> {
    let bytes = line.as_bytes();
    let mut level = 0u8;
    while level < 6 && bytes.get(level as usize) == Some(&b'#') {
        level += 1;
    }
    if level == 0 {
        return None;
    }
    let rest = &line[level as usize..];
    if !rest.is_empty() && !rest.starts_with(' ') {
        return None;
    }
    Some((level, rest.trim().trim_end_matches('#').trim().to_string()))
}

/// GitHub-style slug: lowercase, spaces → hyphens, drop everything else
/// except alphanumerics and hyphens.
fn slugify(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if c == ' ' || c == '-' || c == '_' {
            out.push('-');
        }
    }
    // Collapse consecutive hyphens
    let mut collapsed = String::with_capacity(out.len());
    let mut prev_hyphen = false;
    for c in out.chars() {
        if c == '-' {
            if !prev_hyphen {
                collapsed.push(c);
            }
            prev_hyphen = true;
        } else {
            collapsed.push(c);
            prev_hyphen = false;
        }
    }
    collapsed.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_ctx() -> SkillContext {
        SkillContext { granted: std::collections::HashSet::new(), db: None, llm: None }
    }

    fn make_doc(body: &str) -> SkillDocument {
        SkillDocument {
            id: "document:test".into(),
            title: "T".into(),
            content: ContentFields { body: body.into(), ..Default::default() },
        }
    }

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
        // TOC appears after H1, before "Intro"
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
        // Only one TOC block remains
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

    #[test]
    fn skips_headings_inside_code_fences() {
        let body = "# T\n\n## Real\n\n```\n## Fake\n```\n\n## Also Real\n";
        let out = run("insert", body).unwrap();
        assert!(out.contains("- [Real](#real)"));
        assert!(out.contains("- [Also Real](#also-real)"));
        assert!(!out.contains("Fake"));
    }

    #[test]
    fn slugify_handles_punctuation_and_case() {
        assert_eq!(slugify("Hello, World!"), "hello-world");
        assert_eq!(slugify("Section 1.2"), "section-12");
        assert_eq!(slugify("snake_case_thing"), "snake-case-thing");
    }
}
