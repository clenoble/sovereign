//! Tokenization — pure transform from `(source text, pipeline findings)`
//! to `(canonical body, slot list)`.
//!
//! Canonical body holds reference tokens of the form `[pii:<n>]` instead
//! of the raw values. The DB-side ingest hook (4b) then walks the slot
//! list, writes a `PiiRecord` per slot, and substitutes the integer index
//! placeholder with the real record ID before persisting the canonical
//! body.
//!
//! Confirmed-only: findings with `ReviewState::Unreviewed` are NOT
//! tokenized — per the plan, the source stays fully readable until the
//! user confirms. The ingest hook still writes a `PiiRecord` for them
//! (so the dashboard's review queue can show them) but the body keeps
//! the raw text. Those findings come back via [`Tokenized::deferred`].

use sovereign_db::schema::ReviewState;

use super::pipeline::ScannedFinding;

/// One placeholder slot in the canonical body.
#[derive(Debug, Clone)]
pub struct TokenSlot {
    /// The placeholder string used in the canonical body (e.g. `"[pii:0]"`).
    /// The DB-side ingest hook replaces this with `[pii:<record_id>]`
    /// after writing the corresponding `PiiRecord`.
    pub placeholder: String,
    /// UTF-8 byte offset of the placeholder's start in the canonical body.
    pub canonical_start: usize,
    /// UTF-8 byte offset of the placeholder's end (exclusive) in the
    /// canonical body.
    pub canonical_end: usize,
    /// The pipeline finding that produced this slot, including its
    /// original-text span, encrypted-able sample, entity link, and
    /// review state.
    pub scanned: ScannedFinding,
}

/// Output of [`tokenize`].
#[derive(Debug, Clone, Default)]
pub struct Tokenized {
    /// Source text with all `Confirmed` findings replaced by indexed
    /// placeholders. Same as input when there are no confirmed findings.
    pub canonical: String,
    /// One entry per placeholder, in order of appearance in `canonical`.
    pub slots: Vec<TokenSlot>,
    /// Findings whose `review_state` was not `Confirmed`. The ingest
    /// hook should still write `PiiRecord`s for these so they appear in
    /// the review queue, but it must NOT rewrite the body for them.
    pub deferred: Vec<ScannedFinding>,
}

/// Replace the spans of every `Confirmed` finding in `text` with an
/// indexed placeholder. Returns the new body, a slot per placeholder,
/// and the list of deferred (non-confirmed) findings.
///
/// Assumes `findings` is sorted by `start` and contains no overlapping
/// spans — the pipeline orchestrator guarantees both.
pub fn tokenize(text: &str, findings: &[ScannedFinding]) -> Tokenized {
    let mut canonical = String::with_capacity(text.len());
    let mut slots: Vec<TokenSlot> = Vec::new();
    let mut deferred: Vec<ScannedFinding> = Vec::new();
    let mut cursor = 0;

    for sf in findings {
        if sf.review_state != ReviewState::Confirmed {
            deferred.push(sf.clone());
            continue;
        }
        // Defensive: skip nonsensical or out-of-bounds spans rather than
        // panic. (Pipeline output should never produce these but the
        // type system doesn't enforce it.)
        let (start, end) = (sf.finding.start, sf.finding.end);
        if start < cursor || end > text.len() || start >= end {
            tracing::warn!(
                "tokenize: skipping invalid span ({start}..{end}) cursor={cursor} \
                 text_len={}",
                text.len()
            );
            continue;
        }
        canonical.push_str(&text[cursor..start]);

        let placeholder = format!("[pii:{}]", slots.len());
        let canonical_start = canonical.len();
        canonical.push_str(&placeholder);
        let canonical_end = canonical.len();

        slots.push(TokenSlot {
            placeholder,
            canonical_start,
            canonical_end,
            scanned: sf.clone(),
        });
        cursor = end;
    }
    canonical.push_str(&text[cursor..]);

    Tokenized {
        canonical,
        slots,
        deferred,
    }
}

/// Replace each slot's index placeholder (e.g. `"[pii:0]"`) with the
/// final reference token built from the corresponding entry in
/// `record_ids` (e.g. `"[pii:pii_record:abc]"`).
///
/// `record_ids[i]` must correspond to `slots[i]` — same ordering. The
/// canonical body is rewritten in-place; the returned slot list has
/// updated `placeholder` strings and `canonical_start`/`canonical_end`
/// to reflect the new lengths.
///
/// Used by the DB-side ingest hook (4b) once it has assigned IDs.
pub fn substitute_record_ids(
    canonical: &str,
    slots: &[TokenSlot],
    record_ids: &[String],
) -> (String, Vec<TokenSlot>) {
    assert_eq!(
        slots.len(),
        record_ids.len(),
        "substitute_record_ids: slots and record_ids must align 1:1"
    );

    let mut out = String::with_capacity(canonical.len());
    let mut new_slots: Vec<TokenSlot> = Vec::with_capacity(slots.len());
    let mut cursor = 0;

    for (i, slot) in slots.iter().enumerate() {
        // Carry over text up to this slot's placeholder.
        out.push_str(&canonical[cursor..slot.canonical_start]);

        let new_placeholder = format!("[pii:{}]", record_ids[i]);
        let new_start = out.len();
        out.push_str(&new_placeholder);
        let new_end = out.len();

        new_slots.push(TokenSlot {
            placeholder: new_placeholder,
            canonical_start: new_start,
            canonical_end: new_end,
            scanned: slot.scanned.clone(),
        });
        cursor = slot.canonical_end;
    }
    out.push_str(&canonical[cursor..]);

    (out, new_slots)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pii::Finding;
    use sovereign_db::schema::PiiKind;

    fn confirmed(kind: PiiKind, sample: &str, start: usize) -> ScannedFinding {
        ScannedFinding {
            finding: Finding {
                kind,
                start,
                end: start + sample.len(),
                sample: sample.into(),
                confidence: 1.0,
            },
            entity_id: None,
            review_state: ReviewState::Confirmed,
        }
    }

    fn unreviewed(kind: PiiKind, sample: &str, start: usize) -> ScannedFinding {
        ScannedFinding {
            finding: Finding {
                kind,
                start,
                end: start + sample.len(),
                sample: sample.into(),
                confidence: 0.5,
            },
            entity_id: None,
            review_state: ReviewState::Unreviewed,
        }
    }

    // --- tokenize ---

    #[test]
    fn tokenize_no_findings_returns_text_unchanged() {
        let t = tokenize("hello world", &[]);
        assert_eq!(t.canonical, "hello world");
        assert!(t.slots.is_empty());
        assert!(t.deferred.is_empty());
    }

    #[test]
    fn tokenize_single_finding() {
        let body = "Email me at alice@example.com please.";
        // alice@example.com starts at byte 12, length 17.
        let f = vec![confirmed(PiiKind::Email, "alice@example.com", 12)];
        let t = tokenize(body, &f);
        assert_eq!(t.canonical, "Email me at [pii:0] please.");
        assert_eq!(t.slots.len(), 1);
        assert_eq!(t.slots[0].placeholder, "[pii:0]");
        assert_eq!(
            &t.canonical[t.slots[0].canonical_start..t.slots[0].canonical_end],
            "[pii:0]"
        );
        assert!(t.deferred.is_empty());
    }

    #[test]
    fn tokenize_multiple_findings() {
        let body = "Call 555-123-4567 or email alice@example.com.";
        let f = vec![
            confirmed(PiiKind::Phone, "555-123-4567", 5),
            confirmed(PiiKind::Email, "alice@example.com", 27),
        ];
        let t = tokenize(body, &f);
        assert_eq!(t.canonical, "Call [pii:0] or email [pii:1].");
        assert_eq!(t.slots.len(), 2);
        // Spans into canonical resolve back to the placeholder strings.
        for (i, slot) in t.slots.iter().enumerate() {
            assert_eq!(
                &t.canonical[slot.canonical_start..slot.canonical_end],
                &format!("[pii:{i}]")
            );
        }
    }

    #[test]
    fn tokenize_deferred_findings_left_in_text() {
        let body = "alice@example.com and Charlie Newcomer met.";
        // Confirmed email at byte 0; Unreviewed person_name later.
        let f = vec![
            confirmed(PiiKind::Email, "alice@example.com", 0),
            unreviewed(PiiKind::PersonName, "Charlie Newcomer", 22),
        ];
        let t = tokenize(body, &f);
        // Email was replaced; person_name was NOT.
        assert!(t.canonical.starts_with("[pii:0]"));
        assert!(t.canonical.contains("Charlie Newcomer"));
        assert_eq!(t.slots.len(), 1);
        assert_eq!(t.deferred.len(), 1);
        assert_eq!(t.deferred[0].finding.kind, PiiKind::PersonName);
    }

    #[test]
    fn tokenize_unicode_safe() {
        let body = "Bonjour, contactez alice@example.ch — merci.";
        // alice@example.ch: must use byte offsets, not char offsets.
        let start = body.find("alice@example.ch").unwrap();
        let f = vec![confirmed(PiiKind::Email, "alice@example.ch", start)];
        let t = tokenize(body, &f);
        assert!(t.canonical.contains("[pii:0]"));
        assert!(!t.canonical.contains("alice@example.ch"));
        // Surrounding em-dash and other non-ASCII bytes survive.
        assert!(t.canonical.contains("Bonjour"));
        assert!(t.canonical.contains("merci"));
        assert!(t.canonical.contains("—"));
    }

    #[test]
    fn tokenize_finding_at_start_and_end_of_text() {
        let body = "alice@example.com end";
        let f = vec![confirmed(PiiKind::Email, "alice@example.com", 0)];
        let t = tokenize(body, &f);
        assert_eq!(t.canonical, "[pii:0] end");

        let body2 = "start alice@example.com";
        let f2 = vec![confirmed(
            PiiKind::Email,
            "alice@example.com",
            body2.find("alice@example.com").unwrap(),
        )];
        let t2 = tokenize(body2, &f2);
        assert_eq!(t2.canonical, "start [pii:0]");
    }

    #[test]
    fn tokenize_invalid_span_is_skipped_not_panicked() {
        // Defensive: out-of-bounds end. tokenize should drop and continue
        // rather than panic.
        let body = "short";
        let f = vec![ScannedFinding {
            finding: Finding {
                kind: PiiKind::Email,
                start: 0,
                end: 999,
                sample: "junk".into(),
                confidence: 1.0,
            },
            entity_id: None,
            review_state: ReviewState::Confirmed,
        }];
        let t = tokenize(body, &f);
        // No tokenization happened; canonical == input.
        assert_eq!(t.canonical, body);
        assert!(t.slots.is_empty());
    }

    // --- substitute_record_ids ---

    #[test]
    fn substitute_record_ids_replaces_indexed_with_real() {
        let body = "Call 555-123-4567 or email alice@example.com.";
        let f = vec![
            confirmed(PiiKind::Phone, "555-123-4567", 5),
            confirmed(PiiKind::Email, "alice@example.com", 27),
        ];
        let t = tokenize(body, &f);
        let ids = vec!["pii_record:abc".to_string(), "pii_record:def".to_string()];
        let (canonical, new_slots) = substitute_record_ids(&t.canonical, &t.slots, &ids);

        assert_eq!(
            canonical,
            "Call [pii:pii_record:abc] or email [pii:pii_record:def]."
        );
        assert_eq!(new_slots[0].placeholder, "[pii:pii_record:abc]");
        assert_eq!(new_slots[1].placeholder, "[pii:pii_record:def]");
        // Updated spans in the new canonical body.
        for slot in &new_slots {
            assert_eq!(
                &canonical[slot.canonical_start..slot.canonical_end],
                slot.placeholder
            );
        }
    }

    #[test]
    fn substitute_record_ids_no_slots_is_identity() {
        let (canonical, slots) = substitute_record_ids("hello", &[], &[]);
        assert_eq!(canonical, "hello");
        assert!(slots.is_empty());
    }

    #[test]
    #[should_panic(expected = "must align 1:1")]
    fn substitute_record_ids_misaligned_lengths_panics() {
        let body = "alice@example.com";
        let f = vec![confirmed(PiiKind::Email, "alice@example.com", 0)];
        let t = tokenize(body, &f);
        let _ = substitute_record_ids(&t.canonical, &t.slots, &[]);
    }
}
