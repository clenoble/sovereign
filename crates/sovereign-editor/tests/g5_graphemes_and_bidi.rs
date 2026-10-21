//! Guarantee 5 — "Caret and selection move by grapheme cluster (emoji,
//! combining marks, flags); word and line movement follow parley;
//! bidirectional text moves logically with a correct visual caret."
//! (spec ch. 12, C-TEXT)
//!
//! Decided by Céline 2026-10-10: the arrow keys move in VISUAL order (parley's
//! `move_left` / `move_right`); a selection is still a logical range. The
//! samples outside the bidi tests are left-to-right, where visual and logical
//! order coincide.
//!
//! Grapheme clusters are UAX #29 extended grapheme clusters, taken from the
//! `unicode-segmentation` crate as the reference. Samples avoid the rules where
//! Unicode versions differ (Indic conjuncts).

mod common;
use common::*;

use sovereign_editor::input::{InputEvent, Modifiers, NamedKey, PointerButton, PointerEvent};
use sovereign_editor::Selection;

/// Each sample: a string whose clusters are all multi-code-point.
const SAMPLES: &[(&str, &str)] = &[
    ("zwj family", "👩‍👩‍👧‍👦"),
    ("zwj profession + skin tone", "👩🏽‍🚒"),
    ("flags", "🇫🇷🇩🇪🇯🇵"),
    ("tag flag (Scotland)", "🏴\u{E0067}\u{E0062}\u{E0073}\u{E0063}\u{E0074}\u{E007F}"),
    ("combining marks", "e\u{301}a\u{308}\u{323}o\u{328}\u{301}"),
    ("Hangul conjoining jamo", "\u{1100}\u{1161}\u{11A8}\u{1100}\u{1161}"),
    ("keycap", "1\u{FE0F}\u{20E3}#\u{FE0F}\u{20E3}"),
    ("variation selector", "\u{2764}\u{FE0F}\u{263A}\u{FE0F}"),
    ("CR LF", "a\r\nb\r\n"),
    ("Devanagari with virama-less marks", "\u{915}\u{93F}\u{928}\u{94D}"),
    ("Thai with marks", "\u{E01}\u{E34}\u{E48}\u{E19}"),
];

fn mixed() -> String {
    SAMPLES.iter().map(|(_, s)| *s).collect::<Vec<_>>().join("x")
}

// "Caret … move[s] by grapheme cluster" — Right visits every cluster boundary,
// in order, one cluster per press.
#[test]
fn g5_right_arrow_moves_one_grapheme_cluster() {
    for (name, s) in SAMPLES {
        let text = format!("<{s}>");
        let (mut ed, _f) = open_focused(&doc1(&text));
        caret_at(&mut ed, 1, 0);
        let expected = grapheme_boundaries(&text);
        let mut seen = vec![caret(&ed).offset];
        for _ in 1..expected.len() {
            press(&mut ed, NamedKey::ArrowRight);
            seen.push(caret(&ed).offset);
        }
        assert_eq!(seen, expected, "{name}");
        press(&mut ed, NamedKey::ArrowRight);
        assert_eq!(caret(&ed).offset, text.len(), "{name}: stays at the end");
    }
}

// Same, leftwards from the end.
#[test]
fn g5_left_arrow_moves_one_grapheme_cluster() {
    for (name, s) in SAMPLES {
        let text = format!("<{s}>");
        let (mut ed, _f) = open_focused(&doc1(&text));
        caret_at(&mut ed, 1, text.len());
        let mut expected = grapheme_boundaries(&text);
        expected.reverse();
        let mut seen = vec![caret(&ed).offset];
        for _ in 1..expected.len() {
            press(&mut ed, NamedKey::ArrowLeft);
            seen.push(caret(&ed).offset);
        }
        assert_eq!(seen, expected, "{name}");
    }
}

// "selection move[s] by grapheme cluster" — Shift+Right extends by one cluster,
// the anchor stays.
#[test]
fn g5_shift_arrows_extend_selection_by_cluster() {
    let text = mixed();
    let (mut ed, _f) = open_focused(&doc1(&text));
    caret_at(&mut ed, 1, 0);
    let b = grapheme_boundaries(&text);
    for (i, &want) in b.iter().enumerate().skip(1).take(12) {
        let _ = i;
        sovereign_editor_key(&mut ed, NamedKey::ArrowRight, Modifiers::SHIFT);
        let sel = ed.selection();
        assert_eq!(sel.anchor.offset, 0);
        assert_eq!(sel.focus.offset, want);
    }
    sovereign_editor_key(&mut ed, NamedKey::ArrowLeft, Modifiers::SHIFT);
    assert_eq!(ed.selection().focus.offset, b[11]);
    assert_eq!(ed.selection().anchor.offset, 0);
}

fn sovereign_editor_key(ed: &mut sovereign_editor::Editor, k: NamedKey, m: Modifiers) {
    key(ed, k, m);
}

// "Caret and selection move by grapheme cluster" — a selection set from outside
// never ends up inside a cluster.
#[test]
fn g5_set_selection_inside_a_cluster_lands_on_a_boundary() {
    let text = mixed();
    let (mut ed, _f) = open_focused(&doc1(&text));
    let boundaries = grapheme_boundaries(&text);
    for off in (0..=text.len()).filter(|o| text.is_char_boundary(*o)) {
        ed.set_selection(Selection { anchor: pos(1, 0), focus: pos(1, off) });
        let f = ed.selection().focus.offset;
        assert!(boundaries.contains(&f), "focus {off} -> {f} is inside a cluster");
        ed.set_selection(Selection::caret(pos(1, off)));
        let c = ed.selection().focus.offset;
        assert!(boundaries.contains(&c), "caret {off} -> {c} is inside a cluster");
        assert_eq!(ed.selection().anchor, ed.selection().focus);
    }
}

// "Caret … move[s] by grapheme cluster" — Backspace and Delete never leave the
// text or the caret mid-cluster, never remove more than one cluster, and never
// touch text away from the caret.
#[test]
fn g5_backspace_and_delete_remove_at_most_one_cluster() {
    for (name, s) in SAMPLES {
        let text = format!("<{s}>");
        let b = grapheme_boundaries(&text);
        for w in b.windows(2) {
            let (start, end) = (w[0], w[1]);
            // Backspace at the end of the cluster
            let (mut ed, _f) = open_focused(&doc1(&text));
            caret_at(&mut ed, 1, end);
            press(&mut ed, NamedKey::Backspace);
            let after = text_of(&ed, 1);
            assert!(after.len() < text.len(), "{name}: Backspace removed something");
            assert!(after.len() >= text.len() - (end - start), "{name}: at most one cluster");
            assert_eq!(&after[..start], &text[..start], "{name}: text before untouched");
            assert!(after.ends_with(&text[end..]), "{name}: text after untouched");
            assert!(is_grapheme_boundary(&after, caret(&ed).offset), "{name}: caret on a boundary");
            // Delete at the start of the cluster
            let (mut ed, _f) = open_focused(&doc1(&text));
            caret_at(&mut ed, 1, start);
            press(&mut ed, NamedKey::Delete);
            assert_eq!(text_of(&ed, 1), format!("{}{}", &text[..start], &text[end..]), "{name}: Delete removes the cluster after the caret");
        }
    }
}

// "Caret … move[s] by grapheme cluster" — clicking anywhere puts the caret on
// a boundary.
#[test]
fn g5_pointer_clicks_land_on_cluster_boundaries() {
    let text = mixed();
    let (mut ed, _f) = open_focused(&doc1(&text));
    let boundaries = grapheme_boundaries(&text);
    let mut x = 0.0;
    while x < 1500.0 {
        for y in [2.0, 10.0, 40.0] {
            ed.handle_event(InputEvent::Pointer(PointerEvent::Down {
                x,
                y,
                button: PointerButton::Primary,
                click_count: 1,
                modifiers: Modifiers::NONE,
            }));
            ed.handle_event(InputEvent::Pointer(PointerEvent::Up { x, y, button: PointerButton::Primary }));
            let c = caret(&ed).offset;
            assert!(boundaries.contains(&c), "click at ({x},{y}) -> {c}");
        }
        x += 3.7;
    }
}

// "word … movement follow[s] parley" — whatever parley's rule, Ctrl+Right
// stops on a UAX #29 word boundary that is also a cluster boundary, moves
// forward, and reaches the end.
#[test]
fn g5_word_movement_lands_on_word_boundaries() {
    let text = "Hello, wörld! e\u{301}tude 👩‍👩‍👧‍👦 l'arbre — fin";
    let (mut ed, _f) = open_focused(&doc1(text));
    caret_at(&mut ed, 1, 0);
    use unicode_segmentation::UnicodeSegmentation;
    let mut words: Vec<usize> = text.split_word_bound_indices().map(|(i, _)| i).collect();
    words.push(text.len());
    let mut last = 0;
    for _ in 0..64 {
        key(&mut ed, NamedKey::ArrowRight, Modifiers::CTRL);
        let c = caret(&ed).offset;
        assert!(words.contains(&c), "Ctrl+Right stopped at {c}, not a word boundary");
        assert!(is_grapheme_boundary(text, c));
        assert!(c > last || c == text.len(), "Ctrl+Right moved forward");
        last = c;
        if c == text.len() {
            break;
        }
    }
    assert_eq!(last, text.len(), "Ctrl+Right reaches the end");
    for _ in 0..64 {
        key(&mut ed, NamedKey::ArrowLeft, Modifiers::CTRL);
        let c = caret(&ed).offset;
        assert!(words.contains(&c), "Ctrl+Left stopped at {c}");
        if c == 0 {
            break;
        }
    }
    assert_eq!(caret(&ed).offset, 0, "Ctrl+Left reaches the start");
}

// "line movement follow[s] parley" — Home/End stay on the caret's line; Down
// and Up change line (explicit line breaks, wide viewport: no wrapping).
#[test]
fn g5_home_end_up_down_respect_lines() {
    let text = "first line\nsecond 👩‍👩‍👧‍👦 line\nthird";
    let (mut ed, _f) = open_focused(&doc1(text));
    let l2 = text.find("second").unwrap();
    let l2_end = text[l2..].find('\n').unwrap() + l2;
    caret_at(&mut ed, 1, l2 + 3);
    press(&mut ed, NamedKey::Home);
    assert_eq!(caret(&ed).offset, l2);
    press(&mut ed, NamedKey::End);
    assert_eq!(caret(&ed).offset, l2_end);
    press(&mut ed, NamedKey::ArrowUp);
    assert!(caret(&ed).offset < l2, "Up goes to line 1");
    press(&mut ed, NamedKey::ArrowDown);
    press(&mut ed, NamedKey::ArrowDown);
    assert!(caret(&ed).offset > l2_end, "Down goes to line 3");
    assert!(is_grapheme_boundary(text, caret(&ed).offset));
    key(&mut ed, NamedKey::Home, Modifiers::CTRL);
    assert_eq!(caret(&ed).offset, 0, "Ctrl+Home: document start");
    key(&mut ed, NamedKey::End, Modifiers::CTRL);
    assert_eq!(caret(&ed).offset, text.len(), "Ctrl+End: document end");
}

// Decided by Céline 2026-10-10: Left/Right move to the next cluster in VISUAL
// order (parley's `move_left` / `move_right`). Inside a right-to-left run,
// Right therefore moves toward logically EARLIER text.
#[test]
fn g5_bidi_right_arrow_in_an_rtl_run_moves_to_logically_earlier_text() {
    let text = "abc אבגד efg";
    let after = |c: char| text.find(c).unwrap() + c.len_utf8();
    let (mut ed, _f) = open_focused(&doc1(text));
    caret_at(&mut ed, 1, after('ב'));
    press(&mut ed, NamedKey::ArrowRight);
    assert_eq!(caret(&ed).offset, after('א'), "Right inside the RTL run goes to logically earlier text");
    press(&mut ed, NamedKey::ArrowLeft);
    assert_eq!(caret(&ed).offset, after('ב'), "Left goes back");
    press(&mut ed, NamedKey::ArrowLeft);
    assert_eq!(caret(&ed).offset, after('ג'), "Left inside the RTL run goes to logically later text");
}

/// Presses `k` from the caret's current place until it stops moving, and
/// returns the caret x after each press (the start included).
fn walk(ed: &mut sovereign_editor::Editor, text: &str, k: NamedKey) -> Vec<f64> {
    let mut xs = vec![ed.caret_rect().expect("caret").x0];
    for _ in 0..(4 * text.len()) {
        let before = (caret(ed), xs[xs.len() - 1]);
        press(ed, k);
        assert!(is_grapheme_boundary(text, caret(ed).offset), "caret inside a cluster at {}", caret(ed).offset);
        let x = ed.caret_rect().expect("caret").x0;
        if caret(ed) == before.0 && (x - before.1).abs() < 0.01 {
            break;
        }
        xs.push(x);
    }
    xs
}

fn distinct(xs: &[f64]) -> usize {
    let mut v: Vec<f64> = xs.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v.dedup_by(|a, b| (*a - *b).abs() < 0.5);
    v.len()
}

// "at a run boundary the caret moves to the visually adjacent cluster": on one
// line of N clusters there are N+1 visual caret places. Walking with Right
// from the visual left end, the caret x never goes back and all N+1 places are
// visited — none is skipped at a run boundary. Same with Left from the right
// end. (Font-independent: every cluster, even .notdef, has a positive advance.)
#[test]
fn g5_bidi_visual_walk_visits_every_visual_place_in_order() {
    for text in ["abc אבגד efg", "مرحبا بالعالم abc שלום 42", "ab אב cd גד ef", "שלום abc עולם"] {
        let clusters = grapheme_boundaries(text).len() - 1;
        let (mut ed, _f) = open_focused(&doc1(text));
        // the visual left end: the boundary whose caret is leftmost
        let mut left_end = 0;
        let mut min_x = f64::INFINITY;
        for b in grapheme_boundaries(text) {
            let x = x_of(&mut ed, 1, b);
            if x < min_x {
                min_x = x;
                left_end = b;
            }
        }
        caret_at(&mut ed, 1, left_end);
        let xs = walk(&mut ed, text, NamedKey::ArrowRight);
        assert!(xs.windows(2).all(|w| w[1] >= w[0] - 0.01), "{text:?}: Right went back ({xs:?})");
        assert_eq!(distinct(&xs), clusters + 1, "{text:?}: Right skipped or invented a visual place");
        let xs = walk(&mut ed, text, NamedKey::ArrowLeft);
        assert!(xs.windows(2).all(|w| w[1] <= w[0] + 0.01), "{text:?}: Left went back ({xs:?})");
        assert_eq!(distinct(&xs), clusters + 1, "{text:?}: Left skipped or invented a visual place");
    }
}

// "selection still covers a logical range": however Shift+arrows wander
// visually across run boundaries, the selection is a logical range — what is
// copied is the text between its two ends in storage order.
#[test]
fn g5_bidi_selection_is_a_logical_range() {
    let text = "abc אבגד efg";
    for start in grapheme_boundaries(text) {
        for (k, presses) in [(NamedKey::ArrowRight, 3), (NamedKey::ArrowRight, 7), (NamedKey::ArrowLeft, 4)] {
            let (mut ed, f) = open_focused(&doc1(text));
            caret_at(&mut ed, 1, start);
            for _ in 0..presses {
                key(&mut ed, k, Modifiers::SHIFT);
            }
            let s = ed.selection();
            assert_eq!(s.anchor.offset, start, "the anchor stays");
            let (a, b) = (s.anchor.offset.min(s.focus.offset), s.anchor.offset.max(s.focus.offset));
            assert!(is_grapheme_boundary(text, a) && is_grapheme_boundary(text, b));
            if a < b {
                ctrl(&mut ed, "c");
                assert_eq!(clip_text(&f).as_deref(), Some(&text[a..b]), "copied text is the logical range {a}..{b}");
            }
        }
    }
}

// "with a correct visual caret" — inside a right-to-left run, a caret further
// along in logical order is drawn further LEFT; inside a left-to-right run,
// further RIGHT. (Font-independent: any glyph, even .notdef, has a positive
// advance.)
#[test]
fn g5_bidi_visual_caret_position() {
    let text = "abc אבגד efg";
    let (mut ed, _f) = open_focused(&doc1(text));
    let alef = text.find('א').unwrap();
    let after = |c: char| text.find(c).unwrap() + c.len_utf8();
    let x_a1 = x_of(&mut ed, 1, 1);
    let x_a2 = x_of(&mut ed, 1, 2);
    assert!(x_a2 > x_a1, "LTR run: logical forward is visually right");
    let x_h1 = x_of(&mut ed, 1, after('א'));
    let x_h2 = x_of(&mut ed, 1, after('ב'));
    let x_h3 = x_of(&mut ed, 1, after('ג'));
    assert!(x_h1 > x_h2 && x_h2 > x_h3, "RTL run: logical forward is visually left ({x_h1}, {x_h2}, {x_h3})");
    let x_start = x_of(&mut ed, 1, 0);
    let x_e = x_of(&mut ed, 1, text.find('e').unwrap() + 1);
    assert!(x_e > x_h1 && x_h1 > x_start, "the RTL run sits between the LTR runs");
    let _ = alef;
}
