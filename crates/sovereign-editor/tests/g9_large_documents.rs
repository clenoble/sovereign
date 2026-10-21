//! Guarantee 9 — "Large documents stay usable: up to 1 MB of text in one
//! document, typing stays within one frame and opening within a short delay;
//! beyond the cap the document opens read-only rather than freezing."
//! (spec ch. 12, C-TEXT)
//!
//! "1 MB", decided by Céline 2026-10-10: 1,000,000 bytes of UTF-8 text in
//! `text` blocks (`MAX_EDITABLE_TEXT_BYTES`). Exactly that much is editable;
//! one byte more opens read-only. "Just under" (999,000) is used for the
//! editing and timing tests.
//!
//! Timing tests are `#[ignore]`d so the normal run is not flaky. Run them with
//!   cargo test -p sovereign-editor --release -- --ignored g9_timing
//! Bounds (proposed in the first suite, accepted by Céline 2026-10-10):
//! - one frame = 16 ms (60 Hz): the 95th percentile of keystroke-to-painted
//!   over 200 keystrokes must be ≤ 16 ms;
//! - "a short delay" = 1 s from `Editor::open` to the first `paint` done;
//! - "rather than freezing": opening a 5 MB document returns and paints within
//!   1 s too.

mod common;
use common::*;

use std::time::{Duration, Instant};

use sovereign_editor::input::{InputEvent, NamedKey};
use sovereign_editor::{NoticeKind, ReadOnlyReason};

const UNDER: usize = 999_000;
const CAP: usize = sovereign_editor::MAX_EDITABLE_TEXT_BYTES;
const OVER: usize = CAP + 1;

/// `n` bytes of ASCII prose with line breaks every ~80 bytes.
fn prose(n: usize) -> String {
    let line = "The quick brown fox jumps over the lazy dog; pack my box with five dozen jugs.\n";
    let mut s = line.repeat(n / line.len() + 1);
    s.truncate(n);
    s
}

/// The same amount of text spread over many blocks.
fn many_blocks(total: usize, blocks: usize) -> String {
    let per = total / blocks;
    let mut v = vec![];
    for i in 0..blocks {
        v.push(text_block(i as u32 + 1, &prose(per)));
    }
    v2(&v)
}

// "up to 1 MB of text in one document" — just under the cap: editable, and
// typing goes in.
#[test]
fn g9_just_under_the_cap_is_editable() {
    let (mut ed, f) = open_focused(&doc1(&prose(UNDER)));
    assert!(!ed.is_read_only(), "{:?}", ed.read_only_reason());
    caret_at(&mut ed, 1, UNDER / 2);
    type_text(&mut ed, "INSERTED");
    assert_eq!(&text_of(&ed, 1)[UNDER / 2..UNDER / 2 + 8], "INSERTED");
    ctrl(&mut ed, "s");
    assert!(last_save(&f).content.contains("INSERTED"));
}

// Same, with the text spread over a thousand blocks.
#[test]
fn g9_just_under_the_cap_in_many_blocks_is_editable() {
    let (mut ed, _f) = open_focused(&many_blocks(UNDER, 1000));
    assert!(!ed.is_read_only(), "{:?}", ed.read_only_reason());
    caret_at(&mut ed, 500, 10);
    type_text(&mut ed, "Z");
    assert!(text_of(&ed, 500).contains('Z'));
}

// The decided number (2026-10-10): 1 MB = 1,000,000 bytes.
#[test]
fn g9_the_cap_is_one_million_bytes() {
    assert_eq!(CAP, 1_000_000);
}

// "up to 1 MB of text" — exactly at the cap is still editable.
#[test]
fn g9_exactly_at_the_cap_is_editable() {
    let (ed, _f) = open_focused(&doc1(&prose(CAP)));
    assert!(!ed.is_read_only(), "{:?}", ed.read_only_reason());
}

// The cap counts UTF-8 bytes, not characters: 500,000 two-byte characters are
// at the cap, one more is over it.
#[test]
fn g9_the_cap_counts_utf8_bytes() {
    let (ed, _f) = open_focused(&doc1(&"é".repeat(CAP / 2)));
    assert!(!ed.is_read_only(), "{:?}", ed.read_only_reason());
    let (ed, _f) = open_focused(&doc1(&"é".repeat(CAP / 2 + 1)));
    assert!(matches!(ed.read_only_reason(), Some(ReadOnlyReason::TooLarge { .. })), "{:?}", ed.read_only_reason());
}

// "beyond the cap the document opens read-only rather than freezing" — one
// byte over.
#[test]
fn g9_just_over_the_cap_opens_read_only() {
    let content = doc1(&prose(OVER));
    let (mut ed, f) = open_focused(&content);
    assert!(ed.is_read_only());
    assert!(matches!(ed.read_only_reason(), Some(ReadOnlyReason::TooLarge { .. })), "{:?}", ed.read_only_reason());
    caret_at(&mut ed, 1, 10);
    type_text(&mut ed, "no");
    press(&mut ed, NamedKey::Backspace);
    ctrl(&mut ed, "v");
    ctrl(&mut ed, "s");
    advance(&f, Duration::from_secs(3600));
    ed.poll();
    assert_eq!(model_json(&ed), content, "a read-only document is not changed");
    assert_eq!(save_count(&f), 0, "and not rewritten");
    paint(&mut ed);
    // still readable: the accessible text exposes it, read-only
    let ft = focused_text(&a11y_tree(&mut ed));
    assert!(ft.read_only);
}

// Same over the cap with many blocks and an unknown block: read-only, bytes kept.
#[test]
fn g9_over_the_cap_in_many_blocks_opens_read_only() {
    let mut blocks = vec![unknown_block(5000)];
    for i in 0..1100 {
        blocks.push(text_block(i + 1, &prose(1000)));
    }
    let content = v2(&blocks);
    let (ed, _f) = open_focused(&content);
    assert!(matches!(ed.read_only_reason(), Some(ReadOnlyReason::TooLarge { .. })));
    assert_eq!(model_json(&ed), content);
}

// The cap counts text, not media references or unknown blocks: a small text
// with a large unknown block is still editable.
#[test]
fn g9_large_unknown_block_does_not_count_as_text() {
    let big_unknown =
        format!(r#"{{"type":"x-blob","id":"{}","data":"{}"}}"#, id(9), "A".repeat(2_000_000));
    let content = v2(&[text_block(1, "small"), big_unknown]);
    let (ed, _f) = open_focused(&content);
    assert!(!ed.is_read_only(), "{:?}", ed.read_only_reason());
}

// "Nothing typed is lost" at the cap: typing across the cap either goes in or
// is refused with a notice — never silently dropped.
#[test]
fn g9_typing_across_the_cap_is_not_silently_dropped() {
    let (mut ed, _f) = open_focused(&doc1(&prose(UNDER)));
    ed.take_notices();
    caret_at(&mut ed, 1, UNDER);
    // 60 text-input events of 1,000 characters each (an input method can
    // commit long strings): crosses both readings of "1 MB".
    let typed = "x".repeat(60_000);
    for _ in 0..60 {
        ed.handle_event(InputEvent::Text("x".repeat(1000)));
    }
    let got = text_of(&ed, 1).len() - UNDER;
    if got < typed.len() {
        let n = ed.take_notices();
        assert!(n.iter().any(|n| n.kind == NoticeKind::InputRefused), "{got} of {} typed bytes kept, no notice", typed.len());
    }
}

// A paste that would cross the cap goes in whole or not at all (with a notice).
#[test]
fn g9_paste_across_the_cap_is_whole_or_refused() {
    let (mut ed, f) = open_focused(&doc1(&prose(UNDER)));
    ed.take_notices();
    let pasted = "y".repeat(2_000_000);
    set_clip_text(&f, pasted.clone());
    caret_at(&mut ed, 1, 0);
    ctrl(&mut ed, "v");
    let t = text_of(&ed, 1);
    if t.len() == UNDER {
        let n = ed.take_notices();
        assert!(n.iter().any(|n| n.kind == NoticeKind::InputRefused), "refused without a notice");
    } else {
        assert_eq!(t.len(), UNDER + pasted.len(), "a paste is not truncated");
        assert!(t.starts_with(&pasted));
    }
}

fn percentile(mut v: Vec<Duration>, p: f64) -> Duration {
    v.sort();
    v[((v.len() as f64 - 1.0) * p).round() as usize]
}

// "typing stays within one frame"
#[test]
#[ignore = "timing: run with --release -- --ignored g9_timing"]
fn g9_timing_typing_in_a_1mb_document_stays_within_one_frame() {
    for content in [doc1(&prose(UNDER)), many_blocks(UNDER, 1000)] {
        let (mut ed, _f) = open_focused(&content);
        paint(&mut ed);
        let first = ed.document().blocks()[ed.document().blocks().len() / 2].id();
        ed.set_selection(sovereign_editor::Selection::caret(sovereign_editor::Position { block: first, offset: 40 }));
        let mut times = vec![];
        for i in 0..200 {
            let c = ((b'a' + (i % 26) as u8) as char).to_string();
            let t0 = Instant::now();
            ed.handle_event(InputEvent::Text(c));
            paint(&mut ed);
            times.push(t0.elapsed());
        }
        let p95 = percentile(times, 0.95);
        assert!(p95 <= Duration::from_millis(16), "p95 keystroke-to-painted = {p95:?}");
    }
}

// "opening within a short delay"
#[test]
#[ignore = "timing: run with --release -- --ignored g9_timing"]
fn g9_timing_opening_a_1mb_document_is_short() {
    for content in [doc1(&prose(UNDER)), many_blocks(UNDER, 1000)] {
        let t0 = Instant::now();
        let (mut ed, _f) = open_focused(&content);
        paint(&mut ed);
        let dt = t0.elapsed();
        assert!(dt <= Duration::from_secs(1), "open + first paint = {dt:?}");
    }
}

// "beyond the cap the document opens read-only rather than freezing"
#[test]
#[ignore = "timing: run with --release -- --ignored g9_timing"]
fn g9_timing_opening_far_beyond_the_cap_does_not_freeze() {
    let t0 = Instant::now();
    let (mut ed, _f) = open_focused(&doc1(&prose(5_000_000)));
    paint(&mut ed);
    let _ = ed.accessibility_tree();
    let dt = t0.elapsed();
    assert!(ed.is_read_only());
    assert!(dt <= Duration::from_secs(1), "open + first paint of 5 MB = {dt:?}");
}

// Scrolling-like movement through a large document stays responsive:
// Ctrl+End then Ctrl+Home on 1 MB.
#[test]
#[ignore = "timing: run with --release -- --ignored g9_timing"]
fn g9_timing_document_end_and_start_on_1mb() {
    let (mut ed, _f) = open_focused(&doc1(&prose(UNDER)));
    paint(&mut ed);
    let t0 = Instant::now();
    key(&mut ed, NamedKey::End, sovereign_editor::input::Modifiers::CTRL);
    paint(&mut ed);
    key(&mut ed, NamedKey::Home, sovereign_editor::input::Modifiers::CTRL);
    paint(&mut ed);
    let dt = t0.elapsed();
    assert!(dt <= Duration::from_millis(32), "two jumps + paints = {dt:?}");
}
