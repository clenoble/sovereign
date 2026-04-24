use pulldown_cmark::{html, Options, Parser};

use crate::manifest::Capability;
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

fn render_html(markdown: &str) -> String {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_FOOTNOTES);
    let parser = Parser::new_ext(markdown, opts);
    let mut out = String::new();
    html::push_html(&mut out, parser);
    out
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

/// Strip filesystem-unfriendly characters; keep alphanumerics, dashes, dots,
/// underscores, and spaces. Falls back to "document" if everything was stripped.
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

    #[test]
    fn renders_basic_markdown_to_html() {
        let skill = HtmlExportSkill;
        let doc = make_doc("My Doc", "# Hello\n\nWorld");
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

    #[test]
    fn sanitizes_unsafe_filename_chars() {
        assert_eq!(sanitize_filename("foo/bar.txt"), "foo_bar.txt");
        assert_eq!(sanitize_filename("a:b?c*"), "a_b_c_");
        assert_eq!(sanitize_filename(""), "document");
        assert_eq!(sanitize_filename("..."), "document");
    }
}
