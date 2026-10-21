//! Target parsers split at the real separator of the user's text, whatever the text holds
//! (cloud review of v0.0.10, finding 15). A byte index found in `to_lowercase()` of a string
//! must never be used to slice the original: lowercasing changes byte lengths ("İ" grows from
//! 2 to 3 bytes, the Kelvin sign shrinks from 3 to 1), so the slice panics or cuts the wrong
//! place.
//!
//! Oracle: the separators (" to ", " on ", " for ") are ASCII, and an ASCII byte never occurs
//! inside a multi-byte UTF-8 sequence, so an ASCII-case-insensitive byte search on the
//! *original* string finds the separator at a char boundary. The expected split is the
//! parsers' documented behaviour, computed on that index.
//!
//! `parse_milestone_target` does not exist yet: the "title on/for thread" parse is inline in
//! `handle_query` ("create_milestone"); it is to be extracted with exactly the behaviour below.

use super::{parse_milestone_target, parse_move_target, parse_rename_target};

fn find_ci(hay: &str, needle: &str) -> Option<usize> {
    let n = needle.len();
    hay.as_bytes()
        .windows(n)
        .position(|w| w.eq_ignore_ascii_case(needle.as_bytes()))
}

fn split_at(hay: &str, idx: usize, sep_len: usize) -> (String, String) {
    (hay[..idx].trim().to_string(), hay[idx + sep_len..].trim().to_string())
}

fn expected_rename(t: &str) -> (String, String) {
    match find_ci(t, " to ") {
        Some(i) => split_at(t, i, 4),
        None => (t.to_string(), t.to_string()),
    }
}

fn expected_move(t: &str) -> (String, String) {
    match find_ci(t, " to ") {
        Some(i) => split_at(t, i, 4),
        None => (t.to_string(), String::new()),
    }
}

fn expected_milestone(t: &str) -> (String, String) {
    if let Some(i) = find_ci(t, " on ") {
        split_at(t, i, 4)
    } else if let Some(i) = find_ci(t, " for ") {
        split_at(t, i, 5)
    } else {
        (t.to_string(), String::new())
    }
}

#[test]
fn rename_with_a_dotted_capital_i_splits_at_the_real_to() {
    // v0.0.10 panicked here: the index from the lowercased string lands inside "Ü".
    assert_eq!(parse_rename_target("İ to Ünal"), ("İ".into(), "Ünal".into()));
}

#[test]
fn rename_with_turkish_text_keeps_the_whole_new_name() {
    // v0.0.10 renamed the thread to "rşiv".
    assert_eq!(
        parse_rename_target("İş Planı to Arşiv"),
        ("İş Planı".into(), "Arşiv".into())
    );
}

#[test]
fn move_with_text_whose_lowercase_is_longer_splits_correctly() {
    assert_eq!(parse_move_target("Ⱥnnual İİ to Ünal"), ("Ⱥnnual İİ".into(), "Ünal".into()));
}

#[test]
fn milestone_with_text_whose_lowercase_is_shorter_splits_correctly() {
    // The Kelvin sign (3 bytes) lowercases to 'k' (1 byte): an index from the lowercased
    // string points too early in the original.
    assert_eq!(
        parse_milestone_target("\u{212A}\u{212A}ick-off on Ünal"),
        ("\u{212A}\u{212A}ick-off".into(), "Ünal".into())
    );
    assert_eq!(
        parse_milestone_target("Release for İş Planı"),
        ("Release".into(), "İş Planı".into())
    );
}

#[test]
fn milestone_prefers_on_over_for_and_falls_back_to_the_title() {
    assert_eq!(parse_milestone_target("Demo for A on B"), ("Demo for A".into(), "B".into()));
    assert_eq!(parse_milestone_target("Just a title"), ("Just a title".into(), String::new()));
}

/// Every combination of three pieces drawn from text whose lowercase changes byte length,
/// separators in several cases, combining marks and wide characters: no parser panics, and
/// each splits where the oracle does.
#[test]
fn no_parser_panics_or_misplaces_the_split_on_any_generated_text() {
    const PIECES: &[&str] = &[
        "", "a", "İ", "İş Planı", "Ⱥ", "ẞ", "\u{212A}", "Ünal", "Arşiv", "ǅ", "ﬀ", "Σ", "日本",
        "e\u{301}", " to ", " TO ", " To ", " on ", " On ", " for ", " FOR ", " ",
    ];
    let mut checked = 0usize;
    for a in PIECES {
        for b in PIECES {
            for c in PIECES {
                let t = format!("{a}{b}{c}");
                let got = std::panic::catch_unwind(|| parse_rename_target(&t));
                assert_eq!(got.ok(), Some(expected_rename(&t)), "rename {t:?}");
                let got = std::panic::catch_unwind(|| parse_move_target(&t));
                assert_eq!(got.ok(), Some(expected_move(&t)), "move {t:?}");
                let got = std::panic::catch_unwind(|| parse_milestone_target(&t));
                assert_eq!(got.ok(), Some(expected_milestone(&t)), "milestone {t:?}");
                checked += 1;
            }
        }
    }
    assert_eq!(checked, PIECES.len().pow(3));
}
