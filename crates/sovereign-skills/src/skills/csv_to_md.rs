use std::sync::OnceLock;

use regex::Regex;

use crate::content_util::replace_body;
use crate::manifest::Capability;
use crate::traits::{CoreSkill, SkillContext, SkillDocument, SkillOutput};

pub struct CsvToMdSkill;

impl CoreSkill for CsvToMdSkill {
    fn name(&self) -> &str {
        "csv-to-md"
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
            "convert" => {
                let body = &doc.content.body;
                let new_body = if has_csv_fences(body) {
                    convert_fenced_blocks(body)
                } else if looks_like_csv(body) {
                    csv_to_md_table(body).unwrap_or_else(|_| body.clone())
                } else {
                    body.clone()
                };
                Ok(SkillOutput::ContentUpdate(replace_body(doc, new_body)))
            }
            _ => anyhow::bail!("Unknown action: {action}"),
        }
    }

    fn actions(&self) -> Vec<(String, String)> {
        vec![("convert".into(), "Convert CSV to Markdown Table".into())]
    }

    fn file_types(&self) -> Vec<String> {
        vec!["md".into(), "csv".into()]
    }
}

fn fence_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?ms)^```csv\s*\n(.*?)\n```\s*$").unwrap())
}

fn has_csv_fences(body: &str) -> bool {
    fence_re().is_match(body)
}

/// Conservative whole-body CSV detector: at least two non-empty lines, each
/// containing at least one comma, and no obvious markdown syntax that would
/// indicate the body is prose rather than CSV.
fn looks_like_csv(body: &str) -> bool {
    let non_empty: Vec<&str> = body.lines().filter(|l| !l.trim().is_empty()).collect();
    if non_empty.len() < 2 {
        return false;
    }
    if !non_empty.iter().all(|l| l.contains(',')) {
        return false;
    }
    let has_md_syntax = body.contains("```")
        || body.lines().any(|l| {
            let t = l.trim_start();
            t.starts_with('#') || t.starts_with("* ") || t.starts_with("- ")
        });
    !has_md_syntax
}

fn convert_fenced_blocks(body: &str) -> String {
    fence_re()
        .replace_all(body, |caps: &regex::Captures| {
            let csv_text = &caps[1];
            csv_to_md_table(csv_text).unwrap_or_else(|_| {
                // Leave the original block intact if parsing fails
                format!("```csv\n{csv_text}\n```")
            })
        })
        .into_owned()
}

fn csv_to_md_table(csv_text: &str) -> anyhow::Result<String> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(csv_text.as_bytes());

    let mut rows: Vec<Vec<String>> = Vec::new();
    for record in reader.records() {
        let r = record?;
        rows.push(r.iter().map(|f| escape_cell(f)).collect());
    }

    if rows.is_empty() {
        anyhow::bail!("CSV is empty");
    }

    let cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if cols == 0 {
        anyhow::bail!("CSV has no columns");
    }

    // Pad short rows so the table is rectangular.
    for r in &mut rows {
        while r.len() < cols {
            r.push(String::new());
        }
    }

    let mut out = String::new();
    // Header row
    write_row(&mut out, &rows[0]);
    // Separator
    out.push('|');
    for _ in 0..cols {
        out.push_str(" --- |");
    }
    out.push('\n');
    // Data rows
    for r in rows.iter().skip(1) {
        write_row(&mut out, r);
    }
    Ok(out.trim_end().to_string())
}

fn write_row(out: &mut String, cells: &[String]) {
    out.push('|');
    for c in cells {
        out.push(' ');
        out.push_str(c);
        out.push_str(" |");
    }
    out.push('\n');
}

fn escape_cell(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('\n', "<br>")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{dummy_ctx, make_doc};

    fn run(body: &str) -> String {
        let skill = CsvToMdSkill;
        let doc = make_doc(body);
        match skill.execute("convert", &doc, "", &dummy_ctx()).unwrap() {
            SkillOutput::ContentUpdate(cf) => cf.body,
            _ => panic!("expected ContentUpdate"),
        }
    }

    #[test]
    fn converts_simple_csv_block_to_table() {
        let body = "before\n\n```csv\nname,age\nAlice,30\nBob,25\n```\n\nafter\n";
        let out = run(body);
        assert!(out.contains("before"));
        assert!(out.contains("after"));
        assert!(out.contains("| name | age |"));
        assert!(out.contains("| --- | --- |"));
        assert!(out.contains("| Alice | 30 |"));
        assert!(out.contains("| Bob | 25 |"));
        // Original CSV block should be replaced
        assert!(!out.contains("```csv"));
    }

    #[test]
    fn converts_whole_body_when_no_fences() {
        let body = "a,b,c\n1,2,3\n4,5,6\n";
        let out = run(body);
        assert!(out.starts_with("| a | b | c |"));
        assert!(out.contains("| 1 | 2 | 3 |"));
    }

    #[test]
    fn pads_short_rows() {
        let body = "a,b,c\n1,2\n";
        let out = run(body);
        // Second row got an empty cell appended
        assert!(out.contains("| 1 | 2 |  |"));
    }

    #[test]
    fn escapes_pipes_in_cells() {
        let body = "a,b\nfoo|bar,baz\n";
        let out = run(body);
        assert!(out.contains("foo\\|bar"));
    }

    #[test]
    fn handles_quoted_fields_with_commas() {
        let body = "name,quote\nAlice,\"hello, world\"\n";
        let out = run(body);
        assert!(out.contains("hello, world"));
    }

    #[test]
    fn leaves_invalid_csv_block_alone() {
        // An empty fenced block — csv parses it as zero records, we bail
        // and leave the block intact.
        let body = "```csv\n\n```\n";
        let out = run(body);
        assert!(out.contains("```csv"));
    }

    #[test]
    fn leaves_markdown_with_no_csv_fences_unchanged() {
        let body = "# Heading\n\nSome prose with, a comma in it.\n\n```rust\nfn main() {}\n```\n";
        let out = run(body);
        assert_eq!(out, body);
    }

    #[test]
    fn looks_like_csv_rejects_prose() {
        assert!(!looks_like_csv("hello, world\n"));
        assert!(!looks_like_csv("# Heading\nfoo,bar\nbaz,qux\n"));
        assert!(!looks_like_csv(""));
    }

    #[test]
    fn looks_like_csv_accepts_real_csv() {
        assert!(looks_like_csv("name,age\nAlice,30\nBob,25\n"));
        assert!(looks_like_csv("a,b,c\n1,2,3\n"));
    }
}
