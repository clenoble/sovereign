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

/// Locale hint for the extended detector.
///
/// Country-specific patterns (Swiss AVS, Swiss postal addresses) only fire
/// when `Locale::Swiss` is requested, to keep false-positive rates down on
/// content from other locales. Generic patterns (email, phone, IBAN,
/// passport-with-keyword, DOB-with-keyword) always fire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Locale {
    /// No country-specific patterns. Use this when the source's locale is
    /// unknown or mixed.
    #[default]
    Generic,
    /// Adds AVS and Swiss postal-address patterns.
    Swiss,
}

/// Run the original (Wave-A) PII detectors over `text` and return findings
/// sorted by start offset, with overlaps dropped.
///
/// Preserved as a stable surface for callers that only need the original
/// kinds (`email`, `ssn`, `credit_card`, `phone`, `ipv4`); new code should
/// prefer [`detect_extended`].
pub fn detect(text: &str) -> Vec<PiiFinding> {
    let mut out: Vec<PiiFinding> = Vec::new();

    push_matches(&mut out, "email", email_re(), text);
    push_matches(&mut out, "ssn", ssn_re(), text);
    push_matches(&mut out, "credit_card", credit_card_re(), text);
    push_matches(&mut out, "phone", phone_re(), text);
    push_matches(&mut out, "ipv4", ipv4_re(), text);

    sort_and_dedupe(out)
}

/// Superset of [`detect`] used by the PII pipeline.
///
/// Additions:
///   - `iban`         — ISO 13616, with or without single-space groups.
///   - `passport`     — keyword-anchored (FR/EN/DE/IT). Span is the value only.
///   - `dob`          — ISO date YYYY-MM-DD anchored on a birth keyword
///                      (FR/EN/DE/IT). Span is the date only.
///
/// `Locale::Swiss` adds:
///   - `avs`            — Swiss AHV/AVS social-insurance number
///                        (`756.XXXX.XXXX.XX`).
///   - `swiss_address`  — `<street> <number>, <4-digit postcode> <city>`.
///
/// All findings share `PiiFinding`'s shape; the `kind` field discriminates.
pub fn detect_extended(text: &str, locale: Locale) -> Vec<PiiFinding> {
    let mut out = detect(text);

    // Generic additions (always on).
    push_matches(&mut out, "iban", iban_re(), text);
    push_capture_matches(&mut out, "passport", passport_re(), text, 1);
    push_capture_matches(&mut out, "dob", dob_re(), text, 1);

    // Locale-specific additions.
    if locale == Locale::Swiss {
        push_matches(&mut out, "avs", avs_re(), text);
        push_matches(&mut out, "swiss_address", swiss_address_re(), text);
    }

    sort_and_dedupe(out)
}

/// Sort findings by start, then drop overlaps (earlier wins).
fn sort_and_dedupe(mut findings: Vec<PiiFinding>) -> Vec<PiiFinding> {
    findings.sort_by_key(|f| (f.start, f.end));
    let mut deduped: Vec<PiiFinding> = Vec::with_capacity(findings.len());
    for f in findings {
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

/// Like `push_matches` but reports the span of capture group `group_idx`
/// instead of the full match. Used for keyword-anchored patterns where the
/// keyword must match for context but isn't part of the PII value.
fn push_capture_matches(
    out: &mut Vec<PiiFinding>,
    kind: &'static str,
    re: &Regex,
    text: &str,
    group_idx: usize,
) {
    for caps in re.captures_iter(text) {
        if let Some(g) = caps.get(group_idx) {
            out.push(PiiFinding {
                kind,
                start: g.start(),
                end: g.end(),
                sample: g.as_str().to_string(),
            });
        }
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
            (?:\+\d{1,3}[\s\-.])?     # optional country code (must start with +)
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

fn avs_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // Swiss AHV/AVS: country code 756 + 10 digits, formatted in 4 dotted
    // groups (3.4.4.2). Always written with dots in human-facing material.
    RE.get_or_init(|| Regex::new(r"\b756\.\d{4}\.\d{4}\.\d{2}\b").unwrap())
}

fn iban_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // ISO 13616: 2-letter country, 2-digit check, 11–30 alphanumerics. We
    // accept optional single spaces between groups. Uppercase only — IBAN
    // convention. Length range chosen to cover real IBAN spec (15–34 BBAN
    // chars total) while being robust to space placement.
    RE.get_or_init(|| Regex::new(r"\b[A-Z]{2}\d{2}(?:[ ]?[A-Z0-9]){11,30}\b").unwrap())
}

fn passport_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // Keyword-anchored: a passport-style identifier (1–2 letters then 6–8
    // alphanumerics, OR 6–9 digits) following a passport keyword in EN/FR/
    // DE/IT. Capture group 1 is the value only.
    RE.get_or_init(|| {
        Regex::new(
            r"(?ix)
            \b
            (?: passport | passeport | reisepass | passaporto )
            \s* (?: no\.? | number | n[°º] | nr\.? | \# )?
            \s* :? \s*
            ( [A-Z][A-Z0-9]{6,8} | \d{6,9} )
            \b
            ",
        )
        .unwrap()
    })
}

fn dob_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // Keyword-anchored ISO date. Multi-language: EN/FR/DE/IT keywords.
    // Capture group 1 is the date only.
    RE.get_or_init(|| {
        Regex::new(
            r"(?ix)
            \b
            (?:
                  DOB
                | date \s of \s birth
                | born \s on
                | birthday
                | n[ée]e? \s+ le
                | date \s de \s naissance
                | geboren \s am
                | geburtsdatum
                | nato \s il
                | data \s di \s nascita
            )
            \s* :? \s*
            ( \d{4}-\d{2}-\d{2} )
            \b
            ",
        )
        .unwrap()
    })
}

fn swiss_address_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // Swiss postal address heuristic:
    //   <Street name (1–5 capitalized words, allowing diacritics, hyphens,
    //                  apostrophes, periods for abbreviations like "Rte.")>
    //   <house number (1–4 digits, optional letter suffix), optional comma>
    //   <4-digit postcode>
    //   <city name (1–4 capitalized words)>
    //
    // Tightly anchored on the 4-digit postcode to keep false positives down.
    // Free-form / multi-line addresses are out of regex scope — those are
    // handled by the LLM-NER stage.
    RE.get_or_init(|| {
        Regex::new(
            r"(?x)
            \b
            (?:
                \p{Lu}                          # Capitalized word start.
                [\p{L}\.'\-]+                   # Word body: letters, period, apostrophe, hyphen.
                \s+
            ){1,5}
            \d{1,4} [a-z]?                      # House number, optional letter suffix.
            ,? \s+
            \d{4}                               # 4-digit postcode.
            \s+
            \p{Lu} [\p{L}'\-]+                  # City name, first word.
            (?: \s+ \p{Lu} [\p{L}'\-]+ ){0,3}   # City name, optional extra words.
            \b
            ",
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

    // === detect_extended: Swiss/EU additions ===

    fn kinds(findings: &[PiiFinding]) -> Vec<&str> {
        findings.iter().map(|f| f.kind).collect()
    }

    #[test]
    fn detect_extended_is_superset_of_detect() {
        let body = "Email me at alice@example.com or call 555-123-4567.";
        let base = detect(body);
        let extended = detect_extended(body, Locale::Generic);
        assert_eq!(base.len(), extended.len());
        for (b, e) in base.iter().zip(extended.iter()) {
            assert_eq!(b.kind, e.kind);
            assert_eq!((b.start, b.end), (e.start, e.end));
        }
    }

    #[test]
    fn iban_swiss_with_spaces() {
        let f = detect_extended("IBAN: CH93 0076 2011 6238 5295 7", Locale::Generic);
        assert!(kinds(&f).contains(&"iban"), "kinds={:?}", kinds(&f));
        let iban = f.iter().find(|x| x.kind == "iban").unwrap();
        assert_eq!(iban.sample, "CH93 0076 2011 6238 5295 7");
    }

    #[test]
    fn iban_german_no_spaces() {
        let f = detect_extended("Account DE89370400440532013000.", Locale::Generic);
        let iban = f.iter().find(|x| x.kind == "iban").unwrap();
        assert_eq!(iban.sample, "DE89370400440532013000");
    }

    #[test]
    fn iban_does_not_match_short_alphanumeric() {
        // 14 chars total — below the 15-char IBAN minimum.
        let f = detect_extended("XX12ABCDEFGHIJ", Locale::Generic);
        assert!(!kinds(&f).contains(&"iban"), "should not match: {:?}", f);
    }

    #[test]
    fn passport_keyword_anchored() {
        let f = detect_extended("Passport No.: AB1234567", Locale::Generic);
        let p = f.iter().find(|x| x.kind == "passport").unwrap();
        assert_eq!(p.sample, "AB1234567");
        // The captured span should cover only the value, not "Passport No.:".
        assert!(p.start > 10, "span should not include keyword: start={}", p.start);
    }

    #[test]
    fn passport_french_keyword() {
        let f = detect_extended("Numéro de passeport: F1234567", Locale::Generic);
        // "passeport: F1234567" — F1234567 is 8 chars (1 letter + 7 digits) → matches.
        let p = f.iter().find(|x| x.kind == "passport");
        assert!(p.is_some(), "expected passport finding, got {:?}", f);
        assert_eq!(p.unwrap().sample, "F1234567");
    }

    #[test]
    fn passport_without_keyword_is_not_matched() {
        // Bare 9-digit number with no passport keyword nearby — not flagged
        // as passport (would over-trigger on phone numbers, IDs, etc.).
        let f = detect_extended("Random ID: 123456789", Locale::Generic);
        assert!(!kinds(&f).contains(&"passport"), "kinds={:?}", kinds(&f));
    }

    #[test]
    fn dob_iso_with_french_keyword() {
        let f = detect_extended("Née le 1985-03-12 à Lausanne.", Locale::Generic);
        let dob = f.iter().find(|x| x.kind == "dob").unwrap();
        assert_eq!(dob.sample, "1985-03-12");
    }

    #[test]
    fn dob_iso_with_english_keyword() {
        let f = detect_extended("DOB: 1990-07-22", Locale::Generic);
        let dob = f.iter().find(|x| x.kind == "dob").unwrap();
        assert_eq!(dob.sample, "1990-07-22");
    }

    #[test]
    fn dob_without_keyword_is_not_matched() {
        // Bare ISO date with no birth context — should not flag (would
        // over-trigger on every dated note).
        let f = detect_extended("Meeting on 2025-04-29 at 10am.", Locale::Generic);
        assert!(!kinds(&f).contains(&"dob"));
    }

    #[test]
    fn avs_swiss_locale_matches() {
        let f = detect_extended("AVS 756.1234.5678.97", Locale::Swiss);
        let avs = f.iter().find(|x| x.kind == "avs").unwrap();
        assert_eq!(avs.sample, "756.1234.5678.97");
    }

    #[test]
    fn avs_generic_locale_does_not_match() {
        // Even when the AVS is present, Generic locale must skip it to avoid
        // false-positive on non-Swiss content.
        let f = detect_extended("Number 756.1234.5678.97", Locale::Generic);
        assert!(!kinds(&f).contains(&"avs"));
    }

    #[test]
    fn avs_invalid_country_prefix_rejected() {
        // 200.XXXX.XXXX.XX — wrong prefix (must be 756).
        let f = detect_extended("ID 200.1234.5678.97", Locale::Swiss);
        assert!(!kinds(&f).contains(&"avs"));
    }

    #[test]
    fn swiss_address_simple() {
        let body = "Address: Rue de l'Hôpital 12, 1700 Fribourg";
        let f = detect_extended(body, Locale::Swiss);
        let addr = f.iter().find(|x| x.kind == "swiss_address");
        assert!(addr.is_some(), "expected swiss_address finding, got {:?}", f);
        let s = addr.unwrap().sample.as_str();
        assert!(s.contains("1700"), "sample missing postcode: {s:?}");
        assert!(s.contains("Fribourg"), "sample missing city: {s:?}");
    }

    #[test]
    fn swiss_address_german_form() {
        let body = "Bahnhofstrasse 42, 8001 Zürich";
        let f = detect_extended(body, Locale::Swiss);
        assert!(
            kinds(&f).contains(&"swiss_address"),
            "expected swiss_address, got {:?}",
            f
        );
    }

    #[test]
    fn swiss_address_generic_locale_does_not_match() {
        let body = "Rue de l'Hôpital 12, 1700 Fribourg";
        let f = detect_extended(body, Locale::Generic);
        assert!(!kinds(&f).contains(&"swiss_address"));
    }

    #[test]
    fn detect_extended_dedupe_does_not_drop_disjoint_kinds() {
        // Email and phone in the same string must both survive de-dup since
        // they don't overlap.
        let body = "alice@example.ch and 555-123-4567";
        let f = detect_extended(body, Locale::Generic);
        assert!(kinds(&f).contains(&"email"));
        assert!(kinds(&f).contains(&"phone"));
    }

    #[test]
    fn detect_extended_findings_are_offset_ordered() {
        let body = "DOB: 1990-01-01 and IBAN CH93 0076 2011 6238 5295 7 also alice@example.ch";
        let f = detect_extended(body, Locale::Swiss);
        for w in f.windows(2) {
            assert!(w[0].start <= w[1].start, "out of order: {:?}", f);
        }
    }
}
