//! Spec ch. 12, *Theme*, TC1, TC2 and TC4 (v0.0.13 rc2): text on the accent and the gravity word are read in both
//! themes. Céline, 2026-11-20: « I - oui, à faire », on the line « Dark theme, the primary buttons' text at
//! 2.06:1. Black on the orange »; and « a+ » on its reach.
//!
//! - **TC1:** text on the accent reaches 4.5:1 (WCAG 1.4.3) in both themes, against the accent as drawn: opaque, or
//!   at 0.82 to 0.92 over the palette's `input`, `surface`, `panel`, `modal`, `popup` and `base`.
//! - **TC2:** the gravity word, « Approve » on its gravity colour (opaque, and at 0.82 over the modal), and the trash's
//!   « Empty » on its red (the same), reach 4.5:1 in both themes.
//! - **One function** gives the text colour for a fill as drawn; each drawing of TC1 and TC2 uses it.
//! - **TC4:** the lanes' labels, the launcher letters, the bubble's styles and the avatars keep the colour they had,
//!   and against each of their fills it reads at least what it read in rc1.
//!
//! Drawing needs a GPU, so the colours are checked by contrast and the drawings by reading their source.
//!
//! **Seam, signature by the test author; the body is the implementer's:**
//! - `crate::theme::text_on_fill(fill: Color) -> Color`: the colour of text drawn on `fill`, `fill` given as it is
//!   seen (opaque, composited over what is under it).
//!
//! Written before the code by R1's test author; whoever implements makes these pass without editing this file. A
//! test that looks wrong is reported on the channel.

use crate::theme::{text_on_fill, Palette};
use sovereign_core::profile::BubbleStyle;
use sovereign_core::security::ActionLevel;
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

/// `fg` at `alpha` over the opaque `bg`, as the renderer blends in sRGB.
fn over(fg: Color, alpha: f64, bg: Color) -> Color {
    let (f, b) = (fg.to_rgba8(), bg.to_rgba8());
    let mix = |x: u8, y: u8| (x as f64 * alpha + y as f64 * (1.0 - alpha)).round() as u8;
    Color::from_rgb8(mix(f.r, b.r), mix(f.g, b.g), mix(f.b, b.b))
}

fn rgb(c: Color) -> (u8, u8, u8) {
    let p = c.to_rgba8();
    (p.r, p.g, p.b)
}

fn surfaces(p: &Palette) -> [(&'static str, Color); 6] {
    [("input", p.input), ("surface", p.surface), ("panel", p.panel), ("modal", p.modal), ("popup", p.popup), ("base", p.base)]
}

const THEMES: [&str; 2] = ["dark", "light"];

fn palette(theme: &str) -> Palette {
    if theme == "light" {
        Palette::light()
    } else {
        Palette::dark()
    }
}

/// Text drawn on `seen` by the one function reaches 4.5:1; otherwise a problem naming `what`.
fn reads(problems: &mut Vec<String>, what: String, seen: Color) {
    let text = text_on_fill(seen);
    let ratio = contrast(text, seen);
    if ratio < 4.5 {
        problems.push(format!("{what}: text {:?} on {:?} is {ratio:.2}:1, below 4.5:1", rgb(text), rgb(seen)));
    }
}

#[test]
fn tc1_text_on_the_accent_reaches_4_5_to_1_in_both_themes_at_every_transparency() {
    let mut problems = vec![];
    for theme in THEMES {
        let p = palette(theme);
        reads(&mut problems, format!("{theme}: the accent, opaque"), p.accent);
        for alpha in [0.92, 0.9, 0.85, 0.82] {
            for (name, under) in surfaces(&p) {
                reads(&mut problems, format!("{theme}: the accent at {alpha} over `{name}`"), over(p.accent, alpha, under));
            }
        }
    }
    assert!(problems.is_empty(), "TC1 (WCAG 1.4.3):\n{}", problems.join("\n"));
}

/// The trash's « Empty » red, read from `trash_ui::draw_modal` (`let danger = Color::from_rgb8(r, g, b)`).
fn trash_red() -> Color {
    let body = body_of(include_str!("trash_ui.rs"), "pub(crate) fn draw_modal(", "trash_ui.rs");
    let line = body.lines().find(|l| l.contains("let danger = Color::from_rgb8(")).expect("`draw_modal`'s `danger` colour");
    let inner = line.split("from_rgb8(").nth(1).and_then(|s| s.split(')').next()).expect("its three values");
    let v: Vec<u8> = inner.split(',').map(|x| x.trim().parse().expect("a byte")).collect();
    Color::from_rgb8(v[0], v[1], v[2])
}

#[test]
fn tc2_the_gravity_word_approve_and_empty_reach_4_5_to_1_in_both_themes() {
    let levels = [
        ("Observe", ActionLevel::Observe),
        ("Annotate", ActionLevel::Annotate),
        ("Modify", ActionLevel::Modify),
        ("Transmit", ActionLevel::Transmit),
        ("Destruct", ActionLevel::Destruct),
    ];
    let mut problems = vec![];
    for theme in THEMES {
        let p = palette(theme);
        let mut fills: Vec<(String, Color)> = levels.iter().map(|(n, l)| (format!("the gravity colour {n}"), crate::app::level_color(*l))).collect();
        fills.push(("the trash's « Empty » red".into(), trash_red()));
        for (name, fill) in fills {
            reads(&mut problems, format!("{theme}: {name}, opaque"), fill);
            reads(&mut problems, format!("{theme}: {name} at 0.82 over `modal`"), over(fill, 0.82, p.modal));
        }
    }
    assert!(problems.is_empty(), "TC2 (WCAG 1.4.3):\n{}", problems.join("\n"));
}

#[test]
fn tc4_the_fills_left_for_the_palettes_design_read_no_less_than_in_rc1() {
    // rc1's colour for text on a coloured badge (`theme.rs`, `on_accent`, at v0.0.13 rc1).
    let rc1 = |theme: &str| if theme == "light" { Color::from_rgb8(255, 255, 255) } else { Color::from_rgb8(250, 250, 248) };
    let styles = [
        BubbleStyle::Icon,
        BubbleStyle::Wave,
        BubbleStyle::Spin,
        BubbleStyle::Pulse,
        BubbleStyle::Blink,
        BubbleStyle::Rings,
        BubbleStyle::Matrix,
        BubbleStyle::Orbit,
        BubbleStyle::Morph,
    ];
    let mut fills: Vec<(String, Color)> =
        (0..crate::canvas::HUE_COUNT).map(|h| (format!("hue {h}"), crate::canvas::hue_color(h))).collect();
    fills.push(("the unfiled grey".into(), crate::canvas::UNFILED_COLOR));
    fills.extend(styles.iter().map(|s| (format!("the bubble's {s:?}"), crate::panels::bubble_style_color(*s))));
    let mut problems = vec![];
    for theme in THEMES {
        let now = palette(theme).on_accent;
        for (name, fill) in &fills {
            let (was, is) = (contrast(rc1(theme), *fill), contrast(now, *fill));
            if is + 1e-9 < was {
                problems.push(format!("{theme}: text on {name} reads {is:.2}:1, less than rc1's {was:.2}:1"));
            }
        }
    }
    assert!(problems.is_empty(), "TC4:\n{}", problems.join("\n"));
}

/// A function's body, from its signature to the brace that closes it at the signature's indentation.
fn body_of<'a>(src: &'a str, signature: &str, file: &str) -> &'a str {
    let start = src.find(signature).unwrap_or_else(|| panic!("no `{signature}` in {file}"));
    let line_start = src[..start].rfind('\n').map_or(0, |i| i + 1);
    let close = format!("\n{}}}\n", " ".repeat(start - line_start));
    let end = src[start..].find(&close).unwrap_or_else(|| panic!("`{signature}` in {file} has no closing brace"));
    &src[start..start + end + close.len()]
}

#[test]
fn tc1_tc2_each_drawing_takes_its_text_from_the_one_function_and_tc4s_keep_theirs() {
    let app = include_str!("app.rs");
    let panels = include_str!("panels.rs");
    let onboarding = include_str!("onboarding.rs");
    // TC1 and TC2's drawings.
    let fixed: [(&str, &str, &str); 14] = [
        ("app.rs", app, "pub(crate) fn build_scene("),
        ("app.rs", app, "fn draw_plus("),
        ("app.rs", app, "fn draw_unsaved_prompt("),
        ("app.rs", app, "fn draw_action_prompt("),
        ("log_window.rs", include_str!("log_window.rs"), "fn draw(scene: &mut Scene, shaper: &mut TextShaper, win: &mut LogWindow"),
        ("new_thread.rs", include_str!("new_thread.rs"), "pub(crate) fn draw("),
        ("onboarding.rs", onboarding, "pub(crate) fn draw_onboarding("),
        ("onboarding.rs", onboarding, "fn draw_sample_step("),
        ("panels.rs", panels, "pub(crate) fn draw_comms_form("),
        ("panels.rs", panels, "pub(crate) fn draw_compose_form("),
        ("same_name.rs", include_str!("same_name.rs"), "pub(crate) fn draw("),
        ("trash_ui.rs", include_str!("trash_ui.rs"), "pub(crate) fn draw_modal("),
        ("trash_ui.rs", include_str!("trash_ui.rs"), "pub(crate) fn draw_toast("),
        ("onboarding.rs", onboarding, "fn draw_bubble_step("),
    ];
    let mut problems = vec![];
    for (file, src, signature) in fixed.iter().take(13) {
        let body = body_of(src, signature, file);
        if !body.contains("text_on_fill(") {
            problems.push(format!("{file} `{signature}…)`: its text is not taken from `text_on_fill`"));
        }
        let left: Vec<&str> = body.lines().filter(|l| l.contains("on_accent")).map(str::trim).collect();
        // build_scene keeps one: the lanes' labels (TC4).
        let allowed = if *signature == "pub(crate) fn build_scene(" { 1 } else { 0 };
        if left.len() != allowed || (allowed == 1 && !left[0].contains("&label")) {
            problems.push(format!("{file} `{signature}…)`: `on_accent` left on TC1's or TC2's fills: {left:?}"));
        }
    }
    // TC4's drawings keep the colour they had.
    let kept: [(&str, &str, &str); 5] = [
        ("app.rs", app, "fn draw_taskbar("),
        ("app.rs", app, "fn draw_bubble("),
        ("panels.rs", panels, "pub(crate) fn draw_bubble_picker("),
        ("panels.rs", panels, "pub(crate) fn draw_inbox("),
        fixed[13],
    ];
    for (file, src, signature) in kept {
        if !body_of(src, signature, file).contains("on_accent") {
            problems.push(format!("{file} `{signature}…)`: TC4 leaves its text as it was (`on_accent`), for the palette's design"));
        }
    }
    let lane_label_kept = body_of(app, "pub(crate) fn build_scene(", "app.rs").lines().any(|l| l.contains("&label") && l.contains("on_accent"));
    if !lane_label_kept {
        problems.push("app.rs `build_scene`: the lanes' labels keep `on_accent` (TC4)".into());
    }
    assert!(problems.is_empty(), "TC1, TC2, TC4, in the source:\n{}", problems.join("\n"));
}
