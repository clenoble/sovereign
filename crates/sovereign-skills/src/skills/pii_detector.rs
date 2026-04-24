use std::collections::BTreeMap;
use std::sync::OnceLock;

use regex::Regex;
use serde::Serialize;

use crate::manifest::Capability;
use crate::traits::{CoreSkill, SkillContext, SkillDocument, SkillOutput};

pub struct PiiDetectorSkill;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PiiFinding {
    /// Category: "email", "phone", "ssn", "credit_card", "ipv4".
    pub kind: &'static str,
    /// UTF-8 byte offsets into the source text.
    pub start: usize,
    pub end: usize,
    /// The matched substring.
    pub sample: String,
}

impl CoreSkill for PiiDetectorSkill {
    fn name(&self) -> &str {
        "pii-detector"
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
            "scan" => {
                let findings = detect(&doc.content.body);
                let mut by_kind: BTreeMap<&str, usize> = BTreeMap::new();
                for f in &findings {
                    *by_kind.entry(f.kind).or_insert(0) += 1;
                }
                let json = serde_json::to_string(&serde_json::json!({
                    "findings": findings,
                    "count": findings.len(),
                    "by_kind": by_kind,
                }))?;
                Ok(SkillOutput::StructuredData {
                    kind: "pii_findings".into(),
                    json,
                })
            }
            _ => anyhow::bail!("Unknown action: {action}"),
        }
    }

    fn actions(&self) -> Vec<(String, String)> {
        vec![("scan".into(), "Scan for PII".into())]
    }

    fn file_types(&self) -> Vec<String> {
        vec!["md".into(), "txt".into()]
    }
}

/// Run all PII detectors over `text` and return findings sorted by start offset.
/// Public so the Redactor skill (Wave B) can reuse the same rules without
/// duplicating regex maintenance.
pub fn detect(text: &str) -> Vec<PiiFinding> {
    let mut out: Vec<PiiFinding> = Vec::new();

    push_matches(&mut out, "email", email_re(), text);
    push_matches(&mut out, "ssn", ssn_re(), text);
    push_matches(&mut out, "credit_card", credit_card_re(), text);
    push_matches(&mut out, "phone", phone_re(), text);
    push_matches(&mut out, "ipv4", ipv4_re(), text);

    // Sort by start offset, then drop overlaps (earlier wins).
    out.sort_by_key(|f| (f.start, f.end));
    let mut deduped: Vec<PiiFinding> = Vec::with_capacity(out.len());
    for f in out {
        if let Some(last) = deduped.last() {
            if f.start < last.end {
                continue;
            }
        }
        deduped.push(f);
    }
    deduped
}

fn push_matches(out: &mut Vec<PiiFinding>, kind: &'static str, re: &Regex, text: &str) {
    for m in re.find_iter(text) {
        out.push(PiiFinding {
            kind,
            start: m.start(),
            end: m.end(),
            sample: m.as_str().to_string(),
        });
    }
}

fn email_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\b[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}\b").unwrap()
    })
}

fn ssn_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap())
}

fn credit_card_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // 13–19 digits, optionally grouped with single spaces or hyphens.
    // Not Luhn-validated: a positive is "looks like a card number".
    RE.get_or_init(|| {
        Regex::new(r"\b(?:\d{4}[ -]){3,4}\d{1,4}\b|\b\d{13,19}\b").unwrap()
    })
}

fn phone_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // Optional country code, 10-digit body in common US/intl punctuation.
    RE.get_or_init(|| {
        Regex::new(
            r"(?x)
            (?:\+?\d{1,3}[\s\-.])?     # optional country code
            \(?\d{3}\)?[\s\-.]\d{3}[\s\-.]\d{4}
            ",
        )
        .unwrap()
    })
}

fn ipv4_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"\b(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(?:\.(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3}\b",
        )
        .unwrap()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{dummy_ctx, make_doc};

    fn run(body: &str) -> serde_json::Value {
        let skill = PiiDetectorSkill;
        let doc = make_doc(body);
        let result = skill.execute("scan", &doc, "", &dummy_ctx()).unwrap();
        if let SkillOutput::StructuredData { json, .. } = result {
            serde_json::from_str(&json).unwrap()
        } else {
            panic!("expected StructuredData");
        }
    }

    #[test]
    fn detects_emails() {
        let v = run("Contact alice@example.com or bob+test@example.co.uk.");
        assert_eq!(v["count"], 2);
        assert!(v["by_kind"]["email"] == 2);
    }

    #[test]
    fn detects_ssn() {
        let v = run("SSN is 123-45-6789.");
        assert_eq!(v["count"], 1);
        assert_eq!(v["findings"][0]["kind"], "ssn");
        assert_eq!(v["findings"][0]["sample"], "123-45-6789");
    }

    #[test]
    fn detects_phone_us_format() {
        let v = run("Call (555) 123-4567 or 555.123.4567 or 555-123-4567.");
        assert_eq!(v["count"], 3);
        for i in 0..3 {
            assert_eq!(v["findings"][i]["kind"], "phone");
        }
    }

    #[test]
    fn detects_credit_card_grouped() {
        let v = run("Card: 4111 1111 1111 1111");
        assert_eq!(v["count"], 1);
        assert_eq!(v["findings"][0]["kind"], "credit_card");
    }

    #[test]
    fn detects_ipv4() {
        let v = run("Server at 192.168.1.1 was offline.");
        assert_eq!(v["count"], 1);
        assert_eq!(v["findings"][0]["kind"], "ipv4");
    }

    #[test]
    fn rejects_invalid_ipv4() {
        // 999 not a valid octet
        let v = run("Server at 999.1.1.1 was offline.");
        assert_eq!(v["count"], 0);
    }

    #[test]
    fn deduplicates_overlapping_matches() {
        // 16 digits: matches both `\d{13,19}` and the grouped variant — keep one
        let findings = detect("4111111111111111");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, "credit_card");
    }

    #[test]
    fn empty_text_returns_no_findings() {
        let v = run("");
        assert_eq!(v["count"], 0);
    }

    #[test]
    fn detect_returns_findings_in_offset_order() {
        let findings = detect("a@b.com then c@d.com");
        assert_eq!(findings.len(), 2);
        assert!(findings[0].start < findings[1].start);
    }
}
