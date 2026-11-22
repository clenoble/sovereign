//! Spec ch. 12, *Theme*, TC6 (v0.0.13 rc2): a chosen row's mark is seen in both themes. Céline, 2026-11-20: « ok
//! pour a », on the line « the chosen row's outline in the text colour, as TC3 and the buttons' focus outline
//! already are ».
//!
//! - **The guarantee:** in the six lists a person moves through by the keyboard, the 2-pixel outline that marks the
//!   chosen row reaches 3:1 (WCAG 1.4.11) in both themes, against the row (`surface`, `surface_hover`) and the
//!   window under it (`modal`, `panel`).
//! - **One colour:** one function gives it, and the six drawings stroke the chosen row in it, none in the accent.
//!   The dialogs' own 1-pixel frames, and a button's focus ring (drawn around the button), are not row marks and
//!   are not concerned.
//!
//! Drawing needs a GPU, so the colour is checked by contrast and the drawings by reading their source.
//!
//! **Seam, signature by the test author; the body is the implementer's:**
//! - `crate::theme::chosen_row_outline(p: &Palette) -> Color`.
//!
//! Written before the code by its test author; whoever implements makes these pass without editing this file. A
//! test that looks wrong is reported on the channel.

use crate::theme::{chosen_row_outline, Palette};
use vello::peniko::Color;

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

#[test]
fn tc6_the_chosen_rows_outline_reaches_3_to_1_in_both_themes() {
    let mut problems = vec![];
    for (theme, p) in [("dark", Palette::dark()), ("light", Palette::light())] {
        let outline = chosen_row_outline(&p);
        let o = outline.to_rgba8();
        assert_eq!(o.a, 255, "{theme} theme: the outline is opaque, so its contrast is its own");
        for (name, fill) in [("surface", p.surface), ("surface_hover", p.surface_hover), ("modal", p.modal), ("panel", p.panel)] {
            let ratio = contrast(outline, fill);
            if ratio < 3.0 {
                let f = fill.to_rgba8();
                problems.push(format!(
                    "{theme} theme: the chosen row's outline {:?} against `{name}` {:?} is {ratio:.2}:1, below 3:1",
                    (o.r, o.g, o.b),
                    (f.r, f.g, f.b)
                ));
            }
        }
    }
    assert!(problems.is_empty(), "TC6 (WCAG 1.4.11):\n{}", problems.join("\n"));
}

/// A function's body, from its signature to the brace that closes it at the signature's indentation.
fn body_of<'a>(src: &'a str, signature: &str, file: &str) -> &'a str {
    let start = src.find(signature).unwrap_or_else(|| panic!("no `{signature}` in {file}"));
    let line_start = src[..start].rfind('\n').map_or(0, |i| i + 1);
    let close = format!("\n{}}}\n", " ".repeat(start - line_start));
    let end = src[start..].find(&close).unwrap_or_else(|| panic!("`{signature}` in {file} has no closing brace"));
    &src[start..start + end + close.len()]
}

/// Each `stroke(` call of `body`, whole, on one line.
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

#[test]
fn tc6_each_list_strokes_its_chosen_row_in_the_one_colour() {
    let panels = include_str!("panels.rs");
    let sites: [(&str, &str, &str); 6] = [
        ("same_name.rs", include_str!("same_name.rs"), "pub(crate) fn draw("),
        ("trash_ui.rs", include_str!("trash_ui.rs"), "pub(crate) fn draw_modal("),
        ("orphans_ui.rs", include_str!("orphans_ui.rs"), "pub(crate) fn draw("),
        ("panels.rs", panels, "pub(crate) fn draw_alerts("),
        ("panels.rs", panels, "fn draw_orphans_section("),
        ("panels.rs", panels, "pub(crate) fn draw_previous_values("),
    ];
    let mut problems = vec![];
    for (file, src, signature) in sites {
        let body = body_of(src, signature, file);
        // The chosen row's mark is the 2-pixel stroke on the row; the dialogs' frames are 1-pixel, and a button's
        // focus ring (2 pixels, drawn around the button, `inflate(`) is not a row's mark.
        let marks: Vec<String> =
            strokes(body).into_iter().filter(|s| s.contains("Stroke::new(2.0)") && !s.contains("inflate(")).collect();
        if marks.is_empty() {
            problems.push(format!("{file} `{signature}…)`: no 2-pixel outline marks the chosen row"));
        }
        for s in &marks {
            if !s.contains("chosen_row_outline(") || s.contains("accent") {
                problems.push(format!("{file} `{signature}…)`: the chosen row is not stroked in `chosen_row_outline`'s colour: {s}"));
            }
        }
    }
    assert!(problems.is_empty(), "TC6, in the source:\n{}", problems.join("\n"));
}
