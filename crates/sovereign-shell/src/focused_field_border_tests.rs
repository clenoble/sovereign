//! Spec ch. 12, *Theme*, TC3 (v0.0.13 rc2): a focused text field's border is seen in both themes. Céline,
//! 2026-11-20, « I - oui, à faire », on the line « Light theme, the focused field's border at 2.65:1. The text
//! colour ».
//!
//! - **The guarantee:** while a text field has the keyboard, its border reaches 3:1 or more (WCAG 1.4.11), in both
//!   themes, against the field's own fill and against each surface a field is drawn on: the palette's `input`,
//!   `surface`, `panel`, `modal`, `popup` and `base`.
//! - **One colour:** one function of the theme gives it, and the eight drawings that stroke a focused field use it,
//!   none of them the accent.
//!
//! Drawing needs a GPU, so the colour is checked by contrast from both palettes, and the eight drawings by reading
//! their source.
//!
//! **Seam, signature by the test author; the body is the implementer's:**
//! - `crate::theme::focused_field_border(p: &Palette) -> Color`.
//!
//! Written before the code by R1's test author; whoever implements makes these pass without editing this file. A
//! test that looks wrong is reported on the channel.

use crate::theme::{focused_field_border, Palette};
use vello::peniko::Color;

/// WCAG 2.x relative luminance of an sRGB colour.
fn luminance(c: Color) -> f64 {
    let p = c.to_rgba8();
    let lin = |v: u8| {
        let s = v as f64 / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(p.r) + 0.7152 * lin(p.g) + 0.0722 * lin(p.b)
}

fn contrast(a: Color, b: Color) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// The surfaces a field is drawn in or on, by their palette names.
fn surfaces(p: &Palette) -> [(&'static str, Color); 6] {
    [
        ("input", p.input),
        ("surface", p.surface),
        ("panel", p.panel),
        ("modal", p.modal),
        ("popup", p.popup),
        ("base", p.base),
    ]
}

#[test]
fn tc3_the_focused_border_reaches_3_to_1_in_both_themes() {
    let mut problems = vec![];
    for (theme, p) in [("dark", Palette::dark()), ("light", Palette::light())] {
        let border = focused_field_border(&p);
        let b = border.to_rgba8();
        assert_eq!(b.a, 255, "{theme} theme: the focused border is opaque, so its contrast is its own");
        for (name, bg) in surfaces(&p) {
            let ratio = contrast(border, bg);
            if ratio < 3.0 {
                let g = bg.to_rgba8();
                problems.push(format!(
                    "{theme} theme: the focused border {:?} against `{name}` {:?} is {ratio:.2}:1, below 3:1",
                    (b.r, b.g, b.b),
                    (g.r, g.g, g.b)
                ));
            }
        }
    }
    assert!(problems.is_empty(), "TC3 (WCAG 1.4.11):\n{}", problems.join("\n"));
}

/// A function's body, from its signature to the brace that closes it at the signature's indentation.
fn body_of<'a>(src: &'a str, signature: &str, file: &str) -> &'a str {
    let start = src.find(signature).unwrap_or_else(|| panic!("no `{signature}` in {file}"));
    let line_start = src[..start].rfind('\n').map_or(0, |i| i + 1);
    let close = format!("\n{}}}\n", " ".repeat(start - line_start));
    let end = src[start..].find(&close).unwrap_or_else(|| panic!("`{signature}` in {file} has no closing brace"));
    &src[start..start + end + close.len()]
}

/// Each `stroke(` call of `body`, whole (to its closing parenthesis), on one line.
fn strokes(body: &str) -> Vec<String> {
    let mut out = vec![];
    let mut from = 0;
    while let Some(at) = body[from..].find("stroke(") {
        let open = from + at + "stroke".len();
        let mut depth = 0;
        let mut end = body.len();
        for (i, ch) in body[open..].char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = open + i + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        out.push(body[from + at..end].split_whitespace().collect::<Vec<_>>().join(" "));
        from = end;
    }
    out
}

/// Whether `text` names the accent as a word (`accent`, `pal().accent`, `p.accent`; not `accent_warm`).
fn names_the_accent(text: &str) -> bool {
    text.match_indices("accent").any(|(i, _)| {
        let before = text[..i].chars().next_back();
        let after = text[i + "accent".len()..].chars().next();
        !before.is_some_and(|c| c.is_alphanumeric() || c == '_') && !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}

#[test]
fn tc3_each_focused_field_draws_its_border_in_the_one_colour() {
    let sites: [(&str, &str, &str); 8] = [
        ("onboarding.rs", include_str!("onboarding.rs"), "fn draw_fields("),
        ("panels.rs", include_str!("panels.rs"), "pub(crate) fn draw_auth("),
        ("panels.rs", include_str!("panels.rs"), "pub(crate) fn draw_recovery_wizard("),
        ("panels.rs", include_str!("panels.rs"), "pub(crate) fn draw_comms_form("),
        ("panels.rs", include_str!("panels.rs"), "pub(crate) fn draw_compose_form("),
        ("new_thread.rs", include_str!("new_thread.rs"), "pub(crate) fn draw("),
        ("device_rename.rs", include_str!("device_rename.rs"), "pub(crate) fn draw("),
        ("lane_rename.rs", include_str!("lane_rename.rs"), "pub(crate) fn draw("),
    ];
    let mut problems = vec![];
    for (file, src, signature) in sites {
        let body = body_of(src, signature, file);
        if !body.contains("focused_field_border(") {
            problems.push(format!("{file} `{signature}…)`: its focused border is not drawn in `focused_field_border`'s colour"));
        }
        for s in strokes(body).iter().filter(|s| names_the_accent(s)) {
            problems.push(format!("{file} `{signature}…)`: a stroke in the accent: {s}"));
        }
        for line in body.lines().map(str::trim).filter(|l| l.starts_with("let ") && l.contains(" if ")) {
            if names_the_accent(line) {
                problems.push(format!("{file} `{signature}…)`: a border chosen from the accent: {line}"));
            }
        }
    }
    assert!(problems.is_empty(), "TC3, in the source:\n{}", problems.join("\n"));
}

// ── TC5 (Céline, 2026-11-20): every text field's border is seen, with or without the keyboard ────────────────────

/// Every fill of the palette, by its name.
fn every_fill(p: &Palette) -> [(&'static str, Color); 15] {
    [
        ("base", p.base),
        ("lane_even", p.lane_even),
        ("lane_odd", p.lane_odd),
        ("card", p.card),
        ("card_ext", p.card_ext),
        ("minimap_bg", p.minimap_bg),
        ("taskbar", p.taskbar),
        ("input", p.input),
        ("panel", p.panel),
        ("modal", p.modal),
        ("header", p.header),
        ("popup", p.popup),
        ("surface", p.surface),
        ("surface_alt", p.surface_alt),
        ("surface_hover", p.surface_hover),
    ]
}

#[test]
fn tc5_the_text_field_border_reaches_3_to_1_against_every_fill_in_both_themes() {
    let mut problems = vec![];
    for (theme, p) in [("dark", Palette::dark()), ("light", Palette::light())] {
        let border = p.input_border;
        let b = border.to_rgba8();
        assert_eq!(b.a, 255, "{theme} theme: `input_border` is opaque, so its contrast is its own");
        for (name, fill) in every_fill(&p) {
            let ratio = contrast(border, fill);
            if ratio < 3.0 {
                let f = fill.to_rgba8();
                problems.push(format!(
                    "{theme} theme: `input_border` {:?} against `{name}` {:?} is {ratio:.2}:1, below 3:1",
                    (b.r, b.g, b.b),
                    (f.r, f.g, f.b)
                ));
            }
        }
        // Reported, not required: the step from the unfocused border to the focused one.
        eprintln!(
            "TC5, {theme} theme: the step from `input_border` to the focused border is {:.2}:1",
            contrast(border, focused_field_border(&p))
        );
    }
    assert!(problems.is_empty(), "TC5 (WCAG 1.4.11):\n{}", problems.join("\n"));
}

#[test]
fn tc5_the_search_box_draws_the_text_field_border() {
    let body = body_of(include_str!("panels.rs"), "pub(crate) fn draw_search(", "panels.rs");
    let field_strokes: Vec<String> = strokes(body).into_iter().filter(|s| s.contains("input_rr")).collect();
    assert!(!field_strokes.is_empty(), "`draw_search` strokes its box (`input_rr`)");
    for s in &field_strokes {
        assert!(
            s.contains("input_border") && !s.contains("from_rgb8"),
            "the search box's border is the palette's `input_border`, not a fixed colour: {s}"
        );
    }
}
