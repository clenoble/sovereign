//! v0.0.12 item 7 V2b, B1 — one wording, one derivation
//! (`doc/plans/V0012-item7-v2b-app-list.md`). Written before the code, by
//! another author: whoever implements V2b makes these pass without editing
//! this file. **It does not compile until the seams below exist.**
//!
//! Seams named here (the code author creates them, at the crate root of
//! `sovereign-docs`, beside `unlock_migration_undecryptable`):
//!
//! - `pub const UNREADABLE_HEADING: &str` — the list's heading, the shell's
//!   text today: "Documents that could not be read".
//! - `pub const UNREADABLE_MEANING: &str` — the line on what it means, the
//!   shell's text today: "Each one is kept as it was, untouched. Sovereign
//!   will try to read it again after its next update."
//! - `pub fn unreadable_line(u: &UndecryptableDocument) -> String` — one
//!   document's line, its date in the device's local time.
//! - `pub fn unreadable_line_in<Tz: chrono::TimeZone>(u: &UndecryptableDocument,
//!   tz: &Tz) -> String where Tz::Offset: std::fmt::Display` — the same line
//!   with the date in `tz`. **[choice of the test author]** it exists so the
//!   line can be pinned exactly whatever the machine's zone;
//!   `unreadable_line(u)` is `unreadable_line_in(u, &chrono::Local)`.
//!
//! The line is the shell's today (`panels.rs`, `UnreadableLine::text`):
//! "Document <short id>[ (in the trash)] — last modified YYYY-MM-DD HH:MM",
//! the short id being the first 8 characters of the id's key (after the last
//! `:`, without `⟨ ⟩`). The shell's own text does not change (B1); the
//! shell's side of B1 (no copy left in `panels.rs`) is pinned by
//! `sovereign-shell/src/item7_v2b_scan_tests.rs`.

use chrono::{DateTime, FixedOffset, TimeZone, Utc};
use sovereign_docs::{unreadable_line, unreadable_line_in, UndecryptableDocument, UNREADABLE_HEADING, UNREADABLE_MEANING};

/// 2025-03-02 12:34:56 UTC (seconds are not shown).
fn when() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2025, 3, 2, 12, 34, 56).unwrap()
}

fn doc(id: &str, at: DateTime<Utc>, deleted: bool) -> UndecryptableDocument {
    UndecryptableDocument { id: id.into(), modified_at: at, deleted }
}

/// An id as SurrealDB prints a key it quotes: `table:⟨key⟩`, the key longer
/// than 8 characters.
const QUOTED: &str = "document:\u{27e8}k7f3a9c2e1d0\u{27e9}";

// --------------------------------------------------------- the two texts --

/// B1: the heading is the shell's text today, word for word.
#[test]
fn b1_the_heading_is_the_shells_text() {
    assert_eq!(UNREADABLE_HEADING, "Documents that could not be read");
}

/// B1: the meaning line is the shell's text today, word for word.
#[test]
fn b1_the_meaning_is_the_shells_text() {
    assert_eq!(
        UNREADABLE_MEANING,
        "Each one is kept as it was, untouched. Sovereign will try to read it again after its next update."
    );
}

// --------------------------------------------------------------- the line --

/// B1: a document not in the trash — the short id is the key's first 8
/// characters without `⟨ ⟩` after `table:`; the date is `YYYY-MM-DD HH:MM`.
#[test]
fn b1_the_line_of_a_document_not_in_the_trash() {
    assert_eq!(
        unreadable_line_in(&doc(QUOTED, when(), false), &Utc),
        "Document k7f3a9c2 \u{2014} last modified 2025-03-02 12:34"
    );
}

/// B1: a trashed document says so, between the short id and the date.
#[test]
fn b1_the_line_of_a_document_in_the_trash() {
    assert_eq!(
        unreadable_line_in(&doc(QUOTED, when(), true), &Utc),
        "Document k7f3a9c2 (in the trash) \u{2014} last modified 2025-03-02 12:34"
    );
}

/// B1: the same key printed without `⟨ ⟩` gives the same short id.
#[test]
fn b1_an_unquoted_key_gives_the_same_short_id() {
    assert_eq!(
        unreadable_line_in(&doc("document:k7f3a9c2e1d0", when(), false), &Utc),
        "Document k7f3a9c2 \u{2014} last modified 2025-03-02 12:34"
    );
}

/// B1: a key of 8 characters or fewer is shown whole.
#[test]
fn b1_a_short_key_is_shown_whole() {
    assert_eq!(
        unreadable_line_in(&doc("document:\u{27e8}ab12\u{27e9}", when(), false), &Utc),
        "Document ab12 \u{2014} last modified 2025-03-02 12:34"
    );
}

/// B1: "the device's local time" — the date is the instant in the zone the
/// line is given, the day included when the zone crosses midnight.
#[test]
fn b1_the_date_is_in_the_zone_given() {
    let d = doc(QUOTED, when(), false);
    let east = FixedOffset::east_opt(2 * 3600).unwrap();
    assert_eq!(unreadable_line_in(&d, &east), "Document k7f3a9c2 \u{2014} last modified 2025-03-02 14:34");
    let late = doc(QUOTED, Utc.with_ymd_and_hms(2025, 3, 2, 2, 10, 0).unwrap(), false);
    let west = FixedOffset::west_opt(5 * 3600).unwrap();
    assert_eq!(
        unreadable_line_in(&late, &west),
        "Document k7f3a9c2 \u{2014} last modified 2025-03-01 21:10",
        "the previous day, west of UTC"
    );
}

/// B1: `unreadable_line` is the line in the device's local time.
#[test]
fn b1_the_line_is_in_the_devices_local_time() {
    for deleted in [false, true] {
        let d = doc(QUOTED, when(), deleted);
        assert_eq!(unreadable_line(&d), unreadable_line_in(&d, &chrono::Local));
    }
}
