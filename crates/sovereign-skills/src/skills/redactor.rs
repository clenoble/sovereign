use crate::content_util::replace_body;
use crate::manifest::Capability;
use crate::skills::pii_detector::detect;
use crate::traits::{CoreSkill, SkillContext, SkillDocument, SkillOutput};

pub struct RedactorSkill;

const REDACTED_TOKEN: &str = "[REDACTED]";

impl CoreSkill for RedactorSkill {
    fn name(&self) -> &str {
        "redactor"
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
            "redact" => {
                let new_body = redact(&doc.content.body);
                Ok(SkillOutput::ContentUpdate(replace_body(doc, new_body)))
            }
            _ => anyhow::bail!("Unknown action: {action}"),
        }
    }

    fn actions(&self) -> Vec<(String, String)> {
        vec![("redact".into(), "Redact PII".into())]
    }

    fn file_types(&self) -> Vec<String> {
        vec!["md".into(), "txt".into()]
    }
}

fn redact(text: &str) -> String {
    let mut findings = detect(text);
    if findings.is_empty() {
        return text.to_string();
    }
    // detect() already returns non-overlapping findings sorted by start.
    // Walk back-to-front so earlier offsets stay valid as we splice.
    findings.sort_by_key(|f| f.start);
    let mut out = text.to_string();
    for f in findings.iter().rev() {
        out.replace_range(f.start..f.end, REDACTED_TOKEN);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{dummy_ctx, make_doc};

    fn run(body: &str) -> String {
        let skill = RedactorSkill;
        let doc = make_doc(body);
        match skill.execute("redact", &doc, "", &dummy_ctx()).unwrap() {
            SkillOutput::ContentUpdate(cf) => cf.body,
            _ => panic!("expected ContentUpdate"),
        }
    }

    #[test]
    fn redacts_email() {
        let out = run("Contact alice@example.com please.");
        assert_eq!(out, "Contact [REDACTED] please.");
    }

    #[test]
    fn redacts_multiple_kinds_in_one_pass() {
        let out = run("Email: alice@example.com, phone: 555-123-4567, SSN: 123-45-6789");
        assert!(out.contains("Email: [REDACTED]"));
        assert!(out.contains("phone: [REDACTED]"));
        assert!(out.contains("SSN: [REDACTED]"));
        assert!(!out.contains("alice@example.com"));
        assert!(!out.contains("555-123-4567"));
        assert!(!out.contains("123-45-6789"));
    }

    #[test]
    fn leaves_clean_text_untouched() {
        let body = "No PII here, just plain prose.";
        let out = run(body);
        assert_eq!(out, body);
    }

    #[test]
    fn preserves_surrounding_whitespace() {
        let out = run("a@b.com\n\nnext paragraph");
        assert_eq!(out, "[REDACTED]\n\nnext paragraph");
    }

    #[test]
    fn handles_adjacent_findings() {
        // Two emails separated by a comma — both redacted, comma preserved
        let out = run("a@b.com,c@d.com");
        assert_eq!(out, "[REDACTED],[REDACTED]");
    }

    #[test]
    fn redaction_token_count_matches_finding_count() {
        let body = "x@y.com 192.168.1.1 555-123-4567";
        let out = run(body);
        assert_eq!(out.matches(REDACTED_TOKEN).count(), 3);
    }
}
