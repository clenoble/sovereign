//! Central UI palette. Every structural color (backgrounds, borders, text)
//! flows from here so a theme can one day be swapped from settings. Only the
//! Dark palette ships today; `set_palette` is the (future) settings hook.
//!
//! Semantic, meaning-bearing colors are deliberately NOT in the palette — lane
//! hues (`canvas::hue_color`), action-gravity colors (`app::level_color`),
//! avatar hues, and the per-window category accents encode information, not
//! chrome, so they stay constant across themes.

use std::sync::{OnceLock, RwLock};

use vello::peniko::Color;

/// The set of theme-able chrome + text colors. `Copy` so `pal()` hands back a
/// cheap value; callers read fields directly (`pal().text`).
#[derive(Clone, Copy)]
pub(crate) struct Palette {
    // Backgrounds, lightest-sits-on-top order.
    pub(crate) base: Color,          // window/render base behind the canvas
    pub(crate) lane_even: Color,     // alternating lane band
    pub(crate) lane_odd: Color,
    pub(crate) card: Color,          // owned card fill
    pub(crate) card_ext: Color,      // external card fill
    pub(crate) minimap_bg: Color,    // minimap panel
    pub(crate) taskbar: Color,       // bottom dock
    pub(crate) input: Color,         // text-input / status-bar bg
    pub(crate) panel: Color,         // floating-window body
    pub(crate) modal: Color,         // action-prompt modal body
    pub(crate) header: Color,        // window title bar
    pub(crate) popup: Color,         // context menu / notice bg
    pub(crate) surface: Color,       // rows / buttons
    pub(crate) surface_alt: Color,   // close button / lighter button
    pub(crate) surface_hover: Color, // hovered row / button
    // Borders + separators.
    pub(crate) border: Color,        // panel / popup outline
    pub(crate) border_soft: Color,   // minimap outline
    pub(crate) input_border: Color,  // text-input outline without the keyboard (TC5)
    pub(crate) divider: Color,       // row separators
    pub(crate) divider_strong: Color,// chrome separators (taskbar/bar tops)
    pub(crate) scrollbar: Color,
    // Text.
    pub(crate) text: Color,          // primary
    pub(crate) text_body: Color,     // body / secondary
    pub(crate) text_dim: Color,      // metadata
    pub(crate) text_faint: Color,    // hints
    pub(crate) on_accent: Color,     // text on a colored (accent/avatar) badge/button
    // Brand + provenance signals.
    pub(crate) accent: Color,        // BRAND highlight (orange) — bubble, buttons, focus
    pub(crate) owned: Color,         // owned provenance (blue)
    pub(crate) accent_warm: Color,   // external provenance (warm red)
    pub(crate) caret: Color,         // text caret
    // Document text (the document editor and, through `editor_palette`,
    // the new-thread prompt's field; both in every build). Contrast
    // checked in editor_v2's tests.
    pub(crate) link: Color,          // link text in documents
    pub(crate) selection: Color,     // text selection highlight (opaque)
    pub(crate) selection_text: Color,// text drawn on the selection
    pub(crate) pin: Color,           // pinned-card marker
    pub(crate) now_line: Color,      // "now" time line
    pub(crate) scrim: Color,         // modal dim (used with .with_alpha)
}

impl Palette {
    /// Dark theme — structural shades + the canonical brand/provenance colors
    /// from the Tauri `theme/colors.ts` (accent orange, owned blue, external warm,
    /// tinted card fills).
    pub(crate) fn dark() -> Self {
        Self {
            base: Color::from_rgb8(26, 26, 32),       // --bg-primary
            lane_even: Color::from_rgb8(34, 34, 42),  // --bg-secondary
            lane_odd: Color::from_rgb8(30, 30, 37),
            card: Color::from_rgb8(27, 42, 58),       // --prov-owned-bg
            card_ext: Color::from_rgb8(58, 32, 32),   // --prov-external-bg
            minimap_bg: Color::from_rgb8(20, 20, 25),
            taskbar: Color::from_rgb8(34, 34, 42),    // --bg-secondary
            input: Color::from_rgb8(30, 30, 38),      // --bg-input
            panel: Color::from_rgb8(37, 37, 48),      // --bg-panel
            modal: Color::from_rgb8(42, 42, 53),      // --bg-tertiary
            header: Color::from_rgb8(34, 34, 42),
            popup: Color::from_rgb8(42, 42, 53),
            surface: Color::from_rgb8(42, 42, 53),    // --bg-tertiary
            surface_alt: Color::from_rgb8(48, 48, 61),// --bg-hover
            surface_hover: Color::from_rgb8(54, 54, 68),
            border: Color::from_rgb8(51, 51, 64),     // --border
            border_soft: Color::from_rgb8(44, 44, 56),
            input_border: Color::from_rgb8(132, 132, 148), // TC5: 3:1 or more on every fill
            divider: Color::from_rgb8(44, 44, 56),
            divider_strong: Color::from_rgb8(51, 51, 64),
            scrollbar: Color::from_rgb8(96, 96, 110),
            text: Color::from_rgb8(224, 224, 224),    // --text-primary
            text_body: Color::from_rgb8(179, 179, 179),// --text-secondary
            text_dim: Color::from_rgb8(154, 154, 154),// --text-muted
            text_faint: Color::from_rgb8(130, 130, 134),
            on_accent: Color::from_rgb8(250, 250, 248),
            accent: Color::from_rgb8(245, 158, 11),   // --accent  #F59E0B
            owned: Color::from_rgb8(90, 159, 212),    // --prov-owned #5a9fd4
            accent_warm: Color::from_rgb8(224, 124, 106), // --prov-external #e07c6a
            caret: Color::from_rgb8(245, 158, 11),
            link: Color::from_rgb8(122, 178, 230),
            selection: Color::from_rgb8(90, 159, 212),  // owned blue
            selection_text: Color::from_rgb8(20, 20, 26),
            pin: Color::from_rgb8(222, 184, 96),
            now_line: Color::from_rgb8(245, 158, 11),
            scrim: Color::from_rgb8(0, 0, 0),
        }
    }

    /// Light theme — reproduced from the Tauri `theme/colors.ts` light values
    /// (the reference design): off-white surfaces, dark text, orange brand accent.
    pub(crate) fn light() -> Self {
        Self {
            base: Color::from_rgb8(245, 245, 240),    // --bg-primary #f5f5f0
            lane_even: Color::from_rgb8(235, 235, 224),// --bg-secondary
            lane_odd: Color::from_rgb8(240, 240, 232),
            card: Color::from_rgb8(232, 240, 248),    // --prov-owned-bg #e8f0f8
            card_ext: Color::from_rgb8(252, 232, 232),// --prov-external-bg #fce8e8
            minimap_bg: Color::from_rgb8(222, 222, 210), // --bg-tertiary
            taskbar: Color::from_rgb8(235, 235, 224), // --bg-secondary
            input: Color::from_rgb8(240, 240, 234),   // --bg-input #f0f0ea
            panel: Color::from_rgb8(255, 255, 255),   // --bg-panel #ffffff
            modal: Color::from_rgb8(255, 255, 255),
            header: Color::from_rgb8(240, 240, 234),
            popup: Color::from_rgb8(255, 255, 255),
            surface: Color::from_rgb8(235, 235, 224), // --bg-secondary
            surface_alt: Color::from_rgb8(224, 224, 213), // --bg-hover #e0e0d5
            surface_hover: Color::from_rgb8(224, 224, 213),
            border: Color::from_rgb8(208, 208, 192),  // --border #d0d0c0
            border_soft: Color::from_rgb8(216, 216, 202),
            input_border: Color::from_rgb8(122, 122, 106), // TC5: 3:1 or more on every fill
            divider: Color::from_rgb8(224, 224, 213),
            divider_strong: Color::from_rgb8(208, 208, 192),
            scrollbar: Color::from_rgb8(168, 168, 156),
            text: Color::from_rgb8(26, 26, 32),       // --text-primary #1a1a20
            text_body: Color::from_rgb8(85, 85, 85),  // --text-secondary #555
            text_dim: Color::from_rgb8(110, 110, 110),// --text-muted #6e6e6e
            text_faint: Color::from_rgb8(150, 150, 142),
            on_accent: Color::from_rgb8(255, 255, 255),
            accent: Color::from_rgb8(217, 119, 6),    // --accent #D97706
            owned: Color::from_rgb8(58, 127, 196),    // --prov-owned #3a7fc4
            accent_warm: Color::from_rgb8(192, 85, 69), // --prov-external #c05545
            caret: Color::from_rgb8(217, 119, 6),
            link: Color::from_rgb8(31, 95, 170),
            selection: Color::from_rgb8(95, 145, 205),
            selection_text: Color::from_rgb8(26, 26, 32),
            pin: Color::from_rgb8(199, 145, 0),
            now_line: Color::from_rgb8(217, 119, 6),
            scrim: Color::from_rgb8(0, 0, 0),
        }
    }
}

/// Resolve a profile theme name ("light" / "dark") to a palette.
pub(crate) fn palette_for(name: &str) -> Palette {
    match name {
        "light" => Palette::light(),
        _ => Palette::dark(),
    }
}

fn cell() -> &'static RwLock<Palette> {
    static PALETTE: OnceLock<RwLock<Palette>> = OnceLock::new();
    PALETTE.get_or_init(|| RwLock::new(Palette::dark()))
}

/// The active palette (a cheap `Copy`). Read fields directly: `pal().text`.
pub(crate) fn pal() -> Palette {
    *cell().read().unwrap()
}

/// What a text field is filled with: the palette's input colour (spec ch. 12,
/// *Onboarding and the auth gate*, "Text fields … are readable in both
/// themes"). Field text is drawn in `text` on it, at 4.5:1 or more in both
/// themes (WCAG 1.4.3). Never `on_accent`, which is text on a coloured button:
/// on rc1 that fill left the dark theme's field text at about 1.2:1.
pub(crate) fn field_fill(p: &Palette) -> Color {
    p.input
}

/// A focused text field's border (spec ch. 12, *Theme*, TC3): the text colour,
/// which reaches 3:1 or more in both themes against `input`, `surface`,
/// `panel`, `modal`, `popup` and `base` (WCAG 1.4.11). Not the accent: on rc1
/// the light theme's accent border read 2.65:1 against `surface`. The
/// unfocused border stays `input_border`.
pub(crate) fn focused_field_border(p: &Palette) -> Color {
    p.text
}

/// The 2-pixel outline that alone marks the chosen row of a list moved
/// through by the keyboard (spec ch. 12, *Theme*, TC6): the text colour, as a
/// focused field's border and a button's focus ring. It reaches 3:1 or more in
/// both themes against the row (`surface`, `surface_hover`) and the window
/// under it (`modal`, `panel`) (WCAG 1.4.11). Not the accent: in the light
/// theme the accent read 2.65:1 against `surface`, 2.40 under the pointer.
pub(crate) fn chosen_row_outline(p: &Palette) -> Color {
    p.text
}

/// The near-black that `text_on_fill` offers against a light fill.
const TEXT_ON_LIGHT_FILL: Color = Color::from_rgb8(20, 20, 26);
/// The white that `text_on_fill` offers against a dark fill.
const TEXT_ON_DARK_FILL: Color = Color::from_rgb8(255, 255, 255);

/// WCAG 2.x relative luminance of an sRGB colour (its alpha ignored).
fn relative_luminance(c: Color) -> f64 {
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

fn contrast_ratio(a: Color, b: Color) -> f64 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// The colour of text (or a glyph) drawn on a coloured fill (spec ch. 12,
/// *Theme*, TC1 and TC2): of near-black and white, the one that reads better
/// against it. `fill` is given as it is seen: opaque, or composited over what
/// is under it (`seen_over`). The same fill can thus take near-black opaque
/// and white at a transparency. `on_accent` stays for TC4's drawings only.
pub(crate) fn text_on_fill(fill: Color) -> Color {
    if contrast_ratio(TEXT_ON_LIGHT_FILL, fill) >= contrast_ratio(TEXT_ON_DARK_FILL, fill) {
        TEXT_ON_LIGHT_FILL
    } else {
        TEXT_ON_DARK_FILL
    }
}

/// `fill`, with its own alpha, as it is seen over the opaque `under` (the
/// renderer blends in sRGB): the colour to give `text_on_fill`.
pub(crate) fn seen_over(fill: Color, under: Color) -> Color {
    let alpha = fill.components[3].clamp(0.0, 1.0) as f64;
    let (f, u) = (fill.to_rgba8(), under.to_rgba8());
    let mix = |x: u8, y: u8| (x as f64 * alpha + y as f64 * (1.0 - alpha)).round() as u8;
    Color::from_rgb8(mix(f.r, u.r), mix(f.g, u.g), mix(f.b, u.b))
}

/// Swap the active palette — the future settings theme hook. Unused today
/// (only Dark ships), but wired so theming is a settings toggle, not a refactor.
#[allow(dead_code)]
pub(crate) fn set_palette(p: Palette) {
    *cell().write().unwrap() = p;
}

/// The editor's colours from the shell's theme (spec ch. 12, *Theme*): the
/// document sits on the window body (`panel`), with the theme's text levels,
/// caret and provenance pair, and its document colours (link, selection).
/// Contrast (WCAG 2.1 AA) is checked for both themes in the editor's tests
/// (`app/editor_v2.rs`). Shared by the document editor and the new-thread
/// prompt's name field.
pub(crate) fn editor_palette(p: &Palette) -> sovereign_editor::Palette {
    let mut e = sovereign_editor::Palette::DARK;
    e.background = p.panel;
    e.text = p.text;
    e.muted = p.text_dim;
    e.link = p.link;
    e.selection = p.selection;
    e.selection_text = p.selection_text;
    e.caret = p.caret;
    e.placeholder_background = p.panel;
    e.placeholder_border = p.input_border;
    e.owned = p.owned;
    e.external = p.accent_warm;
    e.undetermined = p.text_dim; // text_faint is under 3:1 on white
    e
}
