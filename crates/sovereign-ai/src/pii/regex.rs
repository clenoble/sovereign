//! Regex stage of the PII pipeline.
//!
//! Thin adapter over [`sovereign_skills::skills::pii_detector::detect_extended`].
//! The skills layer keeps the regex strings (and their tests, and the
//! `RedactorSkill` consumer) in one place; this module just maps the
//! string-tagged [`PiiFinding`] kinds to the typed [`PiiKind`] enum used
//! everywhere else in the pipeline. Unknown kinds are dropped — that's a
//! defensive choice; a fresh string-tag in `sovereign-skills` should always
//! land here with a matching `PiiKind` before it reaches users.

use sovereign_skills::skills::pii_detector::{detect_extended, PiiFinding};

use super::{Finding, Locale, PiiKind};

/// Run the regex stage and return typed findings with `confidence = 1.0`.
///
/// Findings come back sorted by start offset and de-overlapped, matching
/// `detect_extended`'s contract.
pub fn regex_stage(text: &str, locale: Locale) -> Vec<Finding> {
    detect_extended(text, locale)
        .into_iter()
        .filter_map(typed_finding_from)
        .collect()
}

fn typed_finding_from(raw: PiiFinding) -> Option<Finding> {
    let kind = match raw.kind {
        "email" => PiiKind::Email,
        "phone" => PiiKind::Phone,
        "ssn" => PiiKind::Ssn,
        "credit_card" => PiiKind::CreditCard,
        "ipv4" => PiiKind::Ipv4,
        "iban" => PiiKind::Iban,
        "passport" => PiiKind::Passport,
        "dob" => PiiKind::Dob,
        "avs" => PiiKind::Avs,
        "swiss_address" => PiiKind::Address,
        _ => return None,
    };
    Some(Finding {
        kind,
        start: raw.start,
        end: raw.end,
        sample: raw.sample,
        confidence: 1.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_basic_kinds_to_pii_kind() {
        let body = "alice@example.ch and 555-123-4567";
        let f = regex_stage(body, Locale::Generic);
        let kinds: Vec<&PiiKind> = f.iter().map(|x| &x.kind).collect();
        assert!(kinds.contains(&&PiiKind::Email), "kinds={:?}", kinds);
        assert!(kinds.contains(&&PiiKind::Phone), "kinds={:?}", kinds);
    }

    #[test]
    fn regex_stage_findings_have_confidence_one() {
        let f = regex_stage("alice@example.ch", Locale::Generic);
        assert!(!f.is_empty());
        for finding in &f {
            assert_eq!(finding.confidence, 1.0);
        }
    }

    #[test]
    fn swiss_locale_maps_avs_to_pii_kind_avs() {
        let body = "AVS 756.1234.5678.97";
        let f = regex_stage(body, Locale::Swiss);
        let avs = f.iter().find(|x| x.kind == PiiKind::Avs).unwrap();
        assert_eq!(avs.sample, "756.1234.5678.97");
        assert_eq!(avs.confidence, 1.0);
    }

    #[test]
    fn swiss_locale_maps_swiss_address_to_pii_kind_address() {
        // The skills-layer string tag is "swiss_address"; the typed kind is
        // PiiKind::Address (the schema doesn't distinguish locale-tagged
        // address subtypes — the locale is metadata, not structure).
        let body = "Bahnhofstrasse 42, 8001 Zürich";
        let f = regex_stage(body, Locale::Swiss);
        let addr = f.iter().find(|x| x.kind == PiiKind::Address);
        assert!(addr.is_some(), "expected Address finding, got {:?}", f);
    }

    #[test]
    fn generic_locale_skips_swiss_kinds() {
        let body = "AVS 756.1234.5678.97 and Bahnhofstrasse 42, 8001 Zürich";
        let f = regex_stage(body, Locale::Generic);
        assert!(!f.iter().any(|x| x.kind == PiiKind::Avs));
        // Address may still match via no other regex; just confirm that
        // avs specifically is gated.
    }

    #[test]
    fn passport_capture_span_excludes_keyword() {
        let body = "Passport: AB1234567";
        let f = regex_stage(body, Locale::Generic);
        let p = f.iter().find(|x| x.kind == PiiKind::Passport).unwrap();
        // The span must point at "AB1234567", not at "Passport:".
        assert_eq!(&body[p.start..p.end], "AB1234567");
    }

    #[test]
    fn empty_text_yields_no_findings() {
        assert!(regex_stage("", Locale::Generic).is_empty());
        assert!(regex_stage("", Locale::Swiss).is_empty());
    }

    #[test]
    fn findings_offset_ordered() {
        let body = "DOB: 1990-01-01 and IBAN CH93 0076 2011 6238 5295 7 also alice@example.ch";
        let f = regex_stage(body, Locale::Swiss);
        for w in f.windows(2) {
            assert!(w[0].start <= w[1].start, "out of order: {:?}", f);
        }
    }
}
