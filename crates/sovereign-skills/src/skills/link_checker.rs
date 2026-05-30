use std::sync::OnceLock;

use regex::Regex;
use serde::Serialize;

use crate::manifest::Capability;
use crate::traits::{CoreSkill, SkillContext, SkillDocument, SkillOutput};

pub struct LinkCheckerSkill;

#[derive(Debug, Serialize, PartialEq)]
struct Link {
    url: String,
    /// Display text. `None` for autolinks and bare URLs.
    text: Option<String>,
    /// "markdown" (`[text](url)`), "autolink" (`<url>`), or "bare" (raw URL).
    kind: &'static str,
}

impl CoreSkill for LinkCheckerSkill {
    fn name(&self) -> &str {
        "link-checker"
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
                let links = extract_links(&doc.content.body);
                let json = serde_json::to_string(&serde_json::json!({
                    "links": links,
                    "count": links.len(),
                }))?;
                Ok(SkillOutput::StructuredData {
                    kind: "links".into(),
                    json,
                })
            }
            _ => anyhow::bail!("Unknown action: {action}"),
        }
    }

    fn actions(&self) -> Vec<(String, String)> {
        vec![("extract".into(), "Extract Links".into())]
    }

    fn file_types(&self) -> Vec<String> {
        vec!["md".into(), "markdown".into()]
    }
}

fn md_link_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[([^\]]+)\]\(([^)\s]+)(?:\s+[^)]*)?\)").unwrap())
}

fn autolink_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"<((?:https?|ftp)://[^>\s]+)>").unwrap())
}

fn bare_url_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // URL not preceded by `(`, `[`, or `<` so we don't double-count markdown links / autolinks
    RE.get_or_init(|| {
        Regex::new(r"(?:^|[^(\[<])((?:https?|ftp)://[^\s)>\]]+)").unwrap()
    })
}

fn extract_links(body: &str) -> Vec<Link> {
    let mut links = Vec::new();
    let mut consumed_spans: Vec<(usize, usize)> = Vec::new();

    for cap in md_link_re().captures_iter(body) {
        let m = cap.get(0).unwrap();
        consumed_spans.push((m.start(), m.end()));
        links.push(Link {
            url: cap[2].to_string(),
            text: Some(cap[1].to_string()),
            kind: "markdown",
        });
    }

    for cap in autolink_re().captures_iter(body) {
        let m = cap.get(0).unwrap();
        consumed_spans.push((m.start(), m.end()));
        links.push(Link {
            url: cap[1].to_string(),
            text: None,
            kind: "autolink",
        });
    }

    for cap in bare_url_re().captures_iter(body) {
        let url_match = cap.get(1).unwrap();
        if consumed_spans
            .iter()
            .any(|(s, e)| url_match.start() >= *s && url_match.end() <= *e)
        {
            continue;
        }
        // Trim trailing punctuation that often follows URLs in prose
        let trimmed = url_match
            .as_str()
            .trim_end_matches(|c: char| matches!(c, '.' | ',' | ';' | ':' | '!' | '?'));
        links.push(Link {
            url: trimmed.to_string(),
            text: None,
            kind: "bare",
        });
    }

    links
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{dummy_ctx, make_doc};

    fn run(body: &str) -> serde_json::Value {
        let skill = LinkCheckerSkill;
        let doc = make_doc(body);
        let result = skill.execute("extract", &doc, "", &dummy_ctx()).unwrap();
        if let SkillOutput::StructuredData { json, .. } = result {
            serde_json::from_str(&json).unwrap()
        } else {
            panic!("expected StructuredData");
        }
    }

    #[test]
    fn extracts_markdown_links() {
        let v = run("See [Example](https://example.com) for more.");
        assert_eq!(v["count"], 1);
        assert_eq!(v["links"][0]["url"], "https://example.com");
        assert_eq!(v["links"][0]["text"], "Example");
        assert_eq!(v["links"][0]["kind"], "markdown");
    }

    #[test]
    fn extracts_autolinks() {
        let v = run("Visit <https://rust-lang.org> today.");
        assert_eq!(v["count"], 1);
        assert_eq!(v["links"][0]["url"], "https://rust-lang.org");
        assert_eq!(v["links"][0]["kind"], "autolink");
    }

    #[test]
    fn extracts_bare_urls() {
        let v = run("Plain link: https://example.org/path?q=1");
        assert_eq!(v["count"], 1);
        assert_eq!(v["links"][0]["url"], "https://example.org/path?q=1");
        assert_eq!(v["links"][0]["kind"], "bare");
    }

    #[test]
    fn does_not_double_count_markdown_url_as_bare() {
        let v = run("[Example](https://example.com)");
        assert_eq!(v["count"], 1);
        assert_eq!(v["links"][0]["kind"], "markdown");
    }

    #[test]
    fn trims_trailing_punctuation_on_bare_urls() {
        let v = run("See https://example.com.");
        assert_eq!(v["links"][0]["url"], "https://example.com");
    }

    #[test]
    fn extracts_mixed() {
        let v = run("[A](https://a.com) and <https://b.com> and https://c.com");
        assert_eq!(v["count"], 3);
    }
}
