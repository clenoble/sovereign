use pulldown_cmark::{html, Event, LinkType, Options, Parser, Tag, TagEnd};

use crate::manifest::Capability;
use crate::markdown_util::sanitize_filename;
use crate::traits::{CoreSkill, SkillContext, SkillDocument, SkillOutput};

pub struct HtmlExportSkill;

impl CoreSkill for HtmlExportSkill {
    fn name(&self) -> &str {
        "html-export"
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
                let body_html = render_html(&doc.content.body);
                let document = wrap_standalone(&doc.title, &body_html);
                let safe_name = sanitize_filename(&doc.title);
                Ok(SkillOutput::File {
                    name: format!("{safe_name}.html"),
                    mime_type: "text/html".into(),
                    data: document.into_bytes(),
                })
            }
            _ => anyhow::bail!("Unknown action: {action}"),
        }
    }

    fn actions(&self) -> Vec<(String, String)> {
        vec![("export".into(), "Export as HTML".into())]
    }

    fn file_types(&self) -> Vec<String> {
        vec!["md".into(), "markdown".into()]
    }
}

/// The document's Markdown as HTML (spec ch. 11; v0.0.13, the review's item
/// 23). Markdown's formatting is rendered; what the document holds is never
/// run: raw HTML, as a block or inline, is text shown as written, and a link
/// whose address is not `http`, `https`, `mailto`, relative or a fragment
/// keeps its text and shows its address as text after it.
fn render_html(markdown: &str) -> String {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_FOOTNOTES);
    // For each open link: `None` when it stays a link, else what is written
    // after its text in its place (empty for an autolink, whose text is its
    // address already).
    let mut links: Vec<Option<String>> = Vec::new();
    let events = Parser::new_ext(markdown, opts).filter_map(|event| match event {
        Event::Html(raw) | Event::InlineHtml(raw) => Some(Event::Text(raw)),
        Event::Start(Tag::HtmlBlock) => Some(Event::Start(Tag::Paragraph)),
        Event::End(TagEnd::HtmlBlock) => Some(Event::End(TagEnd::Paragraph)),
        Event::Start(Tag::Link { link_type, dest_url, title, id }) => {
            if link_address_is_safe(&dest_url) {
                links.push(None);
                Some(Event::Start(Tag::Link { link_type, dest_url, title, id }))
            } else {
                let after = match link_type {
                    LinkType::Autolink | LinkType::Email => String::new(),
                    _ => format!(" ({dest_url})"),
                };
                links.push(Some(after));
                None
            }
        }
        Event::End(TagEnd::Link) => match links.pop().flatten() {
            None => Some(Event::End(TagEnd::Link)),
            Some(after) if after.is_empty() => None,
            Some(after) => Some(Event::Text(after.into())),
        },
        other => Some(other),
    });
    let mut out = String::new();
    html::push_html(&mut out, events);
    out
}

/// Whether a link's address may stay a link: `http`, `https` and `mailto`, or
/// an address with no scheme (relative, or a fragment). Read as a browser
/// reads it: tabs and line breaks anywhere, and leading spaces and control
/// characters, do not count; a scheme's case does not matter.
fn link_address_is_safe(address: &str) -> bool {
    let read: String = address
        .trim_start_matches(|c: char| c == ' ' || c.is_ascii_control())
        .chars()
        .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
        .collect();
    let end = read.find(['/', '?', '#']).unwrap_or(read.len());
    match read[..end].find(':') {
        None => true,
        Some(colon) => {
            let scheme = read[..colon].to_ascii_lowercase();
            matches!(scheme.as_str(), "http" | "https" | "mailto")
        }
    }
}

fn wrap_standalone(title: &str, body_html: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>{title}</title>
<style>
body {{
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif;
    max-width: 720px;
    margin: 2rem auto;
    padding: 0 1rem;
    line-height: 1.6;
    color: #222;
}}
h1, h2, h3, h4, h5, h6 {{ line-height: 1.2; margin-top: 1.6em; }}
pre {{
    background: #f5f5f5;
    padding: 0.8em 1em;
    border-radius: 4px;
    overflow-x: auto;
}}
code {{ background: #f5f5f5; padding: 0.1em 0.3em; border-radius: 3px; }}
pre code {{ background: none; padding: 0; }}
blockquote {{
    border-left: 3px solid #ccc;
    margin: 0;
    padding: 0.2em 1em;
    color: #555;
}}
table {{ border-collapse: collapse; }}
th, td {{ border: 1px solid #ddd; padding: 0.4em 0.8em; }}
img {{ max-width: 100%; height: auto; }}
a {{ color: #0066cc; }}
</style>
</head>
<body>
<h1>{title}</h1>
{body_html}
</body>
</html>
"#,
        title = html_escape(title),
        body_html = body_html,
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// v0.0.13, the review's item 23: tests written before the code, by another
// author; the implementer does not edit them. At src/skills/html_export/.
#[cfg(test)]
mod html_export_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{dummy_ctx, make_doc_with_title};

    #[test]
    fn renders_basic_markdown_to_html() {
        let skill = HtmlExportSkill;
        let doc = make_doc_with_title("My Doc", "# Hello\n\nWorld");
        let result = skill.execute("export", &doc, "", &dummy_ctx()).unwrap();
        if let SkillOutput::File { name, mime_type, data } = result {
            assert_eq!(name, "My Doc.html");
            assert_eq!(mime_type, "text/html");
            let html = String::from_utf8(data).unwrap();
            assert!(html.contains("<!DOCTYPE html>"));
            assert!(html.contains("<title>My Doc</title>"));
            assert!(html.contains("<h1>My Doc</h1>"));
            assert!(html.contains("<h1>Hello</h1>"));
            assert!(html.contains("<p>World</p>"));
        } else {
            panic!("expected File");
        }
    }

    #[test]
    fn renders_tables_and_strikethrough_via_extensions() {
        let html = render_html("|a|b|\n|-|-|\n|1|2|\n\n~~struck~~");
        assert!(html.contains("<table>"));
        assert!(html.contains("<del>struck</del>"));
    }

    #[test]
    fn html_escapes_title() {
        let escaped = html_escape("Hello <world> & \"friends\"");
        assert_eq!(escaped, "Hello &lt;world&gt; &amp; &quot;friends&quot;");
    }
}
