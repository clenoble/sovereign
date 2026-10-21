//! rc2 — the text fields of the auth gate and of the setup wizard are readable
//! in both themes (spec ch. 12, *Onboarding and the auth gate*, v2.0.46):
//! "their text is drawn at **4.5:1 or more** against the field's fill (WCAG
//! 1.4.3), the fill being the palette's input colour." On rc1 both were
//! filled with `on_accent` (near-white) while the text is `text` (light in the
//! dark theme): about 1.2:1.
//!
//! Written before the code, by another author: whoever implements rc2 makes
//! these pass without editing this file.
//!
//! Seam: `crate::theme::field_fill(&Palette) -> Color`, the one place that
//! says what a text field is filled with; the drawing sites
//! (`onboarding.rs` `draw_fields`, `panels.rs` `draw_auth`, and — added with
//! the spec extension of `7506579` — `panels.rs` `draw_search`) fill their
//! field rectangles with it. The fill is checked by contrast against `Palette::text`,
//! the colour both sites draw field text (and password dots) with; drawing
//! itself needs a GPU, so the two sites are checked by reading their source.

use crate::theme::{field_fill, Palette};
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

/// The colour the eye sees for `fg` drawn over `bg` (alpha-composited in sRGB,
/// as the renderer blends); an opaque `fg` is itself.
fn over(fg: Color, bg: Color) -> Color {
    let f = fg.to_rgba8();
    let b = bg.to_rgba8();
    let a = f.a as f64 / 255.0;
    let mix = |x: u8, y: u8| (x as f64 * a + y as f64 * (1.0 - a)).round() as u8;
    Color::from_rgb8(mix(f.r, b.r), mix(f.g, b.g), mix(f.b, b.b))
}

fn contrast(a: Color, b: Color) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

fn field_text_contrast(name: &str, p: &Palette) -> f64 {
    // A translucent fill shows the card under it; both cards are `panel`.
    let fill = over(field_fill(p), p.panel);
    let ratio = contrast(p.text, fill);
    let (t, f) = (p.text.to_rgba8(), fill.to_rgba8());
    assert!(
        ratio >= 4.5,
        "{name} theme: field text {:?} on field fill {:?} is {ratio:.2}:1, below the 4.5:1 of WCAG 1.4.3 (spec ch. 12)",
        (t.r, t.g, t.b),
        (f.r, f.g, f.b),
    );
    ratio
}

#[test]
fn the_metric_is_wcag_contrast() {
    // Black on white is 21:1; a colour on itself is 1:1.
    let r = contrast(Color::from_rgb8(0, 0, 0), Color::from_rgb8(255, 255, 255));
    assert!((r - 21.0).abs() < 0.01, "{r}");
    let one = contrast(Color::from_rgb8(30, 30, 38), Color::from_rgb8(30, 30, 38));
    assert!((one - 1.0).abs() < 1e-9, "{one}");
    // rc1's dark-theme field: `text` on `on_accent` is the ~1.2:1 of the spec.
    let d = Palette::dark();
    let rc1 = contrast(d.text, d.on_accent);
    assert!(rc1 < 1.5, "rc1's dark field measured {rc1:.2}:1");
}

#[test]
fn field_text_reaches_4_5_to_1_in_the_dark_theme() {
    field_text_contrast("dark", &Palette::dark());
}

#[test]
fn field_text_reaches_4_5_to_1_in_the_light_theme() {
    field_text_contrast("light", &Palette::light());
}

#[test]
fn the_field_fill_is_the_palettes_input_colour() {
    // Spec ch. 12: "the fill being the palette's input colour".
    for (name, p) in [("dark", Palette::dark()), ("light", Palette::light())] {
        let (got, want) = (field_fill(&p).to_rgba8(), p.input.to_rgba8());
        assert_eq!(
            (got.r, got.g, got.b, got.a),
            (want.r, want.g, want.b, want.a),
            "{name} theme: a text field must be filled with the palette's `input` colour"
        );
    }
}

// ---------------------------------------------------------------------------
// The drawing sites use the fill (source scan). The helpers are shared with
// `auth_store_write_tests`.
// ---------------------------------------------------------------------------

pub(super) fn read(rel: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// `src` with line comments blanked (same length), so a comment that mentions
/// `on_accent` does not count.
pub(super) fn blank_line_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    for line in src.split_inclusive('\n') {
        match find_comment(line) {
            Some(i) => {
                out.push_str(&line[..i]);
                for ch in line[i..].chars() {
                    out.push(if ch == '\n' { '\n' } else { ' ' });
                }
            }
            None => out.push_str(line),
        }
    }
    out
}

/// Start of a `//` comment on `line` outside a string literal.
fn find_comment(line: &str) -> Option<usize> {
    let b = line.as_bytes();
    let mut in_str = false;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\\' if in_str => i += 1,
            b'"' => in_str = !in_str,
            b'/' if !in_str && b.get(i + 1) == Some(&b'/') => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

/// Body of `fn <name>(` by brace matching (string literals skipped).
pub(super) fn fn_body(code: &str, name: &str, file: &str) -> String {
    let needle = format!("fn {name}(");
    let start = code.find(&needle).unwrap_or_else(|| panic!("{file}: `fn {name}(` not found"));
    let open = start + code[start..].find('{').expect("fn body");
    let b = code.as_bytes();
    let (mut depth, mut i, mut in_str) = (0usize, open, false);
    while i < b.len() {
        match b[i] {
            b'\\' if in_str => i += 1,
            b'"' => in_str = !in_str,
            b'{' if !in_str => depth += 1,
            b'}' if !in_str => {
                depth -= 1;
                if depth == 0 {
                    return code[open..=i].to_string();
                }
            }
            _ => {}
        }
        i += 1;
    }
    panic!("{file}: unbalanced braces in {name}");
}

fn check_site(file: &str, name: &str) -> Vec<String> {
    let code = blank_line_comments(&read(file));
    let body = fn_body(&code, name, file);
    let mut problems = Vec::new();
    let fills_with_field_fill = body
        .match_indices("scene.fill(")
        .any(|(p, _)| body[p..].split(';').next().unwrap_or("").contains("field_fill("));
    if !fills_with_field_fill {
        problems.push(format!(
            "{file} `{name}`: no `scene.fill(…)` uses `field_fill(` — the field rectangles must be filled with the palette's field fill"
        ));
    }
    for (i, line) in body.lines().enumerate() {
        if line.contains("on_accent") {
            problems.push(format!(
                "{file} `{name}`, body line {}: uses `on_accent` (text on a coloured button), not a field fill: `{}`",
                i + 1,
                line.trim()
            ));
        }
    }
    problems
}

#[test]
fn the_wizard_fields_are_filled_with_field_fill() {
    let problems = check_site("src/onboarding.rs", "draw_fields");
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}

#[test]
fn the_auth_gate_fields_are_filled_with_field_fill() {
    let problems = check_site("src/panels.rs", "draw_auth");
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}

/// Spec ch. 12 as extended (`7506579`): "Text fields of the auth gate, the
/// wizard and the search window are readable in both themes … The search box
/// had the same fill (`draw_search`)."
#[test]
fn the_search_field_is_filled_with_field_fill() {
    let problems = check_site("src/panels.rs", "draw_search");
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}
