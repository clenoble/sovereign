use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::manifest::Capability;
use crate::traits::{CoreSkill, SkillContext, SkillDocument, SkillOutput};

pub struct PlaintextExportSkill;

impl CoreSkill for PlaintextExportSkill {
    fn name(&self) -> &str {
        "plaintext-export"
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
            "export" => {
                let plain = render_plaintext(&doc.content.body);
                let safe_name = sanitize_filename(&doc.title);
                Ok(SkillOutput::File {
                    name: format!("{safe_name}.txt"),
                    mime_type: "text/plain".into(),
                    data: plain.into_bytes(),
                })
            }
            _ => anyhow::bail!("Unknown action: {action}"),
        }
    }

    fn actions(&self) -> Vec<(String, String)> {
        vec![("export".into(), "Export as Plain Text".into())]
    }

    fn file_types(&self) -> Vec<String> {
        vec!["md".into(), "markdown".into()]
    }
}

fn render_plaintext(markdown: &str) -> String {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    let parser = Parser::new_ext(markdown, opts);

    let mut out = String::new();
    let mut list_depth = 0u8;
    let mut list_index_stack: Vec<Option<u64>> = Vec::new();

    for event in parser {
        match event {
            Event::Start(Tag::Heading { .. }) => {
                ensure_blank_line(&mut out);
            }
            Event::End(TagEnd::Heading(_)) => {
                out.push('\n');
                out.push('\n');
            }
            Event::Start(Tag::Paragraph) => {
                ensure_blank_line(&mut out);
            }
            Event::End(TagEnd::Paragraph) => {
                out.push('\n');
                out.push('\n');
            }
            Event::Start(Tag::List(start)) => {
                list_depth += 1;
                list_index_stack.push(start);
                ensure_blank_line(&mut out);
            }
            Event::End(TagEnd::List(_)) => {
                list_depth = list_depth.saturating_sub(1);
                list_index_stack.pop();
                if list_depth == 0 {
                    out.push('\n');
                }
            }
            Event::Start(Tag::Item) => {
                let indent = "  ".repeat(list_depth.saturating_sub(1) as usize);
                out.push_str(&indent);
                if let Some(Some(n)) = list_index_stack.last_mut() {
                    out.push_str(&format!("{n}. "));
                    *n += 1;
                } else {
                    out.push_str("- ");
                }
            }
            Event::End(TagEnd::Item) => {
                if !out.ends_with('\n') {
                    out.push('\n');
                }
            }
            Event::Start(Tag::BlockQuote(_)) => {
                ensure_blank_line(&mut out);
                out.push_str("> ");
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                out.push('\n');
            }
            Event::Start(Tag::CodeBlock(_)) => {
                ensure_blank_line(&mut out);
            }
            Event::End(TagEnd::CodeBlock) => {
                out.push('\n');
            }
            Event::Start(Tag::Link { .. })
            | Event::End(TagEnd::Link)
            | Event::Start(Tag::Emphasis)
            | Event::End(TagEnd::Emphasis)
            | Event::Start(Tag::Strong)
            | Event::End(TagEnd::Strong)
            | Event::Start(Tag::Strikethrough)
            | Event::End(TagEnd::Strikethrough) => {
                // emit only the text content, drop the formatting markers
            }
            Event::Text(t) | Event::Code(t) => {
                out.push_str(&t);
            }
            Event::SoftBreak => out.push(' '),
            Event::HardBreak => out.push('\n'),
            Event::Rule => {
                ensure_blank_line(&mut out);
                out.push_str("---\n\n");
            }
            Event::TaskListMarker(checked) => {
                out.push_str(if checked { "[x] " } else { "[ ] " });
            }
            _ => {}
        }
    }

    // Collapse runs of 3+ newlines down to 2 (one blank line max between blocks)
    collapse_blank_lines(out.trim_end()).to_string() + "\n"
}

fn ensure_blank_line(out: &mut String) {
    if out.is_empty() {
        return;
    }
    if !out.ends_with("\n\n") {
        if out.ends_with('\n') {
            out.push('\n');
        } else {
            out.push('\n');
            out.push('\n');
        }
    }
}

fn collapse_blank_lines(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut newlines = 0;
    for c in s.chars() {
        if c == '\n' {
            newlines += 1;
            if newlines <= 2 {
                out.push(c);
            }
        } else {
            newlines = 0;
            out.push(c);
        }
    }
    out
}

fn sanitize_filename(title: &str) -> String {
    let s: String = title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '.' | '_' | ' ') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = s.trim().trim_matches('.');
    if trimmed.is_empty() {
        "document".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sovereign_core::content::ContentFields;

    fn dummy_ctx() -> SkillContext {
        SkillContext { granted: std::collections::HashSet::new(), db: None, llm: None }
    }

    fn make_doc(title: &str, body: &str) -> SkillDocument {
        SkillDocument {
            id: "document:test".into(),
            title: title.into(),
            content: ContentFields { body: body.into(), ..Default::default() },
        }
    }

    fn run(body: &str) -> String {
        let skill = PlaintextExportSkill;
        let doc = make_doc("X", body);
        let result = skill.execute("export", &doc, "", &dummy_ctx()).unwrap();
        if let SkillOutput::File { data, .. } = result {
            String::from_utf8(data).unwrap()
        } else {
            panic!("expected File");
        }
    }

    #[test]
    fn strips_emphasis_and_links() {
        let out = run("This is **bold** and *italic* and a [link](https://x.com).");
        assert!(out.contains("bold"));
        assert!(out.contains("italic"));
        assert!(out.contains("link"));
        assert!(!out.contains("**"));
        assert!(!out.contains("https://x.com"));
        assert!(!out.contains('['));
    }

    #[test]
    fn keeps_code_block_content_drops_fences() {
        let out = run("Before\n\n```rust\nfn main() {}\n```\n\nAfter");
        assert!(out.contains("fn main()"));
        assert!(!out.contains("```"));
        assert!(out.contains("Before"));
        assert!(out.contains("After"));
    }

    #[test]
    fn renders_unordered_list() {
        let out = run("- one\n- two\n- three\n");
        assert!(out.contains("- one"));
        assert!(out.contains("- two"));
        assert!(out.contains("- three"));
    }

    #[test]
    fn renders_ordered_list_with_numbers() {
        let out = run("1. first\n2. second\n3. third\n");
        assert!(out.contains("1. first"));
        assert!(out.contains("2. second"));
        assert!(out.contains("3. third"));
    }

    #[test]
    fn output_filename_uses_txt_extension() {
        let skill = PlaintextExportSkill;
        let doc = make_doc("Title", "body");
        let result = skill.execute("export", &doc, "", &dummy_ctx()).unwrap();
        if let SkillOutput::File { name, mime_type, .. } = result {
            assert_eq!(name, "Title.txt");
            assert_eq!(mime_type, "text/plain");
        } else {
            panic!("expected File");
        }
    }

    #[test]
    fn collapses_excessive_blank_lines() {
        // Multiple block boundaries shouldn't produce 4+ consecutive newlines
        let out = run("# H1\n\n# H2\n\n# H3\n");
        assert!(!out.contains("\n\n\n\n"));
    }
}
