//! First-device onboarding wizard for the native shell.
//!
//! Replicates the Tauri/Svelte `OnboardingWizard` multi-step flow natively:
//! Welcome → Nickname → Bubble style → Theme → Sample data → Password → Duress →
//! Canary. (Keystroke-enrollment is deferred until login actually verifies
//! cadence.) "Pair with an existing device" on the Welcome step hands off to the
//! shell's existing join flow. The wizard only holds STATE + rendering + pure
//! hit-testing; the App owns the side effects (create auth store, install
//! session, persist profile/canary, seed) in `finish_onboarding`.

use parley::Layout;
use vello::kurbo::{Affine, Line, Point, Rect, RoundedRect, Stroke};
use vello::peniko::{Color, Fill, Mix};
use vello::Scene;

use sovereign_core::profile::BubbleStyle;

use crate::panels::{bubble_style_color, draw_bubble_style};
use crate::text::{draw_text, Brush, TextShaper};
use crate::theme::pal;

/// Number of steps in the first-device flow.
pub(crate) const WIZ_STEPS: usize = 8;

// Step indices (kept as plain usize so the per-step matches read clearly).
pub(crate) const STEP_WELCOME: usize = 0;
pub(crate) const STEP_NICKNAME: usize = 1;
pub(crate) const STEP_BUBBLE: usize = 2;
pub(crate) const STEP_THEME: usize = 3;
pub(crate) const STEP_SAMPLE: usize = 4;
pub(crate) const STEP_PASSWORD: usize = 5;
pub(crate) const STEP_DURESS: usize = 6;
pub(crate) const STEP_CANARY: usize = 7;

/// What a click resolved to — the App acts on it (advance, finish, hand off to
/// join, mutate a field, …). Keeps the App ↔ wizard boundary explicit.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum WizAction {
    None,
    Back,
    Next,
    Skip,
    Finish,
    ChoosePair,
    SelectBubble(BubbleStyle),
    SelectTheme(bool), // true = dark
    ToggleSample,
    FocusField(usize),
    ToggleReveal,
}

pub(crate) struct OnboardingWizard {
    pub(crate) step: usize,
    pub(crate) designation: String,
    pub(crate) nickname: String,
    pub(crate) bubble: BubbleStyle,
    pub(crate) theme_dark: bool,
    pub(crate) seed_sample: bool,
    pub(crate) password: String,
    pub(crate) password_confirm: String,
    pub(crate) duress: String,
    pub(crate) duress_confirm: String,
    pub(crate) canary: String,
    pub(crate) canary_confirm: String,
    pub(crate) reveal: bool,
    pub(crate) focus: usize,
    pub(crate) error: Option<String>,
}

impl OnboardingWizard {
    pub(crate) fn new(designation: String, theme_dark: bool) -> Self {
        Self {
            step: STEP_WELCOME,
            designation,
            nickname: String::new(),
            bubble: BubbleStyle::all()[0],
            theme_dark,
            seed_sample: true,
            password: String::new(),
            password_confirm: String::new(),
            duress: String::new(),
            duress_confirm: String::new(),
            canary: String::new(),
            canary_confirm: String::new(),
            reveal: false,
            focus: 0,
            error: None,
        }
    }

    /// Number of editable text fields on the current step.
    pub(crate) fn field_count(&self) -> usize {
        match self.step {
            STEP_NICKNAME => 1,
            STEP_PASSWORD | STEP_DURESS | STEP_CANARY => 2,
            _ => 0,
        }
    }

    /// Whether field `idx` on this step renders masked (secrets).
    fn field_masked(&self, _idx: usize) -> bool {
        matches!(self.step, STEP_PASSWORD | STEP_DURESS)
    }

    fn field_mut(&mut self, idx: usize) -> Option<&mut String> {
        match (self.step, idx) {
            (STEP_NICKNAME, 0) => Some(&mut self.nickname),
            (STEP_PASSWORD, 0) => Some(&mut self.password),
            (STEP_PASSWORD, 1) => Some(&mut self.password_confirm),
            (STEP_DURESS, 0) => Some(&mut self.duress),
            (STEP_DURESS, 1) => Some(&mut self.duress_confirm),
            (STEP_CANARY, 0) => Some(&mut self.canary),
            (STEP_CANARY, 1) => Some(&mut self.canary_confirm),
            _ => None,
        }
    }

    fn field_val(&self, idx: usize) -> &str {
        match (self.step, idx) {
            (STEP_NICKNAME, 0) => &self.nickname,
            (STEP_PASSWORD, 0) => &self.password,
            (STEP_PASSWORD, 1) => &self.password_confirm,
            (STEP_DURESS, 0) => &self.duress,
            (STEP_DURESS, 1) => &self.duress_confirm,
            (STEP_CANARY, 0) => &self.canary,
            (STEP_CANARY, 1) => &self.canary_confirm,
            _ => "",
        }
    }

    pub(crate) fn insert_str(&mut self, s: &str) {
        let focus = self.focus;
        self.error = None;
        if let Some(f) = self.field_mut(focus) {
            f.push_str(s);
        }
    }

    pub(crate) fn insert_char(&mut self, ch: char) {
        if ch.is_control() {
            return;
        }
        let focus = self.focus;
        self.error = None;
        if let Some(f) = self.field_mut(focus) {
            f.push(ch);
        }
    }

    pub(crate) fn backspace(&mut self) {
        let focus = self.focus;
        if let Some(f) = self.field_mut(focus) {
            f.pop();
        }
    }

    /// Tab / Enter between fields on multi-field steps.
    pub(crate) fn focus_next(&mut self) {
        let n = self.field_count();
        if n > 0 {
            self.focus = (self.focus + 1) % n;
        }
    }

    /// Whether the primary button is enabled for this step.
    pub(crate) fn can_advance(&self) -> bool {
        match self.step {
            STEP_PASSWORD => {
                strength_score(&self.password) == 5 && self.password == self.password_confirm
            }
            // Welcome advances via its cards; the rest are always allowed
            // (duress/canary validate their non-empty content on advance).
            _ => true,
        }
    }

    /// Validate the duress step (only when a duress password was entered).
    pub(crate) fn validate_duress(&self) -> Result<(), String> {
        if self.duress.is_empty() {
            return Ok(());
        }
        if self.duress != self.duress_confirm {
            return Err("Duress passwords don't match.".into());
        }
        if self.duress == self.password {
            return Err("Duress password must differ from your password.".into());
        }
        if strength_score(&self.duress) < 5 {
            return Err("Duress password needs 12+ chars with upper, lower, digit, symbol.".into());
        }
        Ok(())
    }

    /// Validate the canary step (only when a phrase was entered).
    pub(crate) fn validate_canary(&self) -> Result<(), String> {
        if self.canary.is_empty() {
            return Ok(());
        }
        if self.canary.chars().count() < 4 {
            return Err("Canary phrase must be at least 4 characters.".into());
        }
        if self.canary != self.canary_confirm {
            return Err("Canary phrases don't match.".into());
        }
        Ok(())
    }

    pub(crate) fn is_skippable(&self) -> bool {
        matches!(self.step, STEP_DURESS | STEP_CANARY)
    }

    /// Resolve a click to an action (pure — the App mutates state).
    pub(crate) fn hit_test(&self, p: Point, w: f64, h: f64) -> WizAction {
        let card = wiz_card(w, h);
        // Welcome: two choice buttons.
        if self.step == STEP_WELCOME {
            let (first, pair) = welcome_buttons(card);
            if first.contains(p) {
                return WizAction::Next;
            }
            if pair.contains(p) {
                return WizAction::ChoosePair;
            }
            return WizAction::None;
        }
        // Per-step widgets.
        match self.step {
            STEP_BUBBLE => {
                for (cell, st) in bubble_cells(card) {
                    if cell.contains(p) {
                        return WizAction::SelectBubble(st);
                    }
                }
            }
            STEP_THEME => {
                let (dark, light) = theme_buttons(card);
                if dark.contains(p) {
                    return WizAction::SelectTheme(true);
                }
                if light.contains(p) {
                    return WizAction::SelectTheme(false);
                }
            }
            STEP_SAMPLE => {
                if sample_toggle(card).contains(p) {
                    return WizAction::ToggleSample;
                }
            }
            _ => {
                // Text-field steps: focus a field or toggle reveal.
                let rects = field_rects(card, self.field_count());
                for (i, fr) in rects.iter().enumerate() {
                    if fr.contains(p) {
                        return WizAction::FocusField(i);
                    }
                    if self.field_masked(i) && reveal_btn(*fr).contains(p) {
                        return WizAction::ToggleReveal;
                    }
                }
            }
        }
        // Nav bar (Back / Skip / Primary).
        let nav = wiz_nav(card, self.step);
        if let Some(b) = nav.back {
            if b.contains(p) {
                return WizAction::Back;
            }
        }
        if let Some(s) = nav.skip {
            if s.contains(p) {
                return WizAction::Skip;
            }
        }
        if nav.primary.contains(p) && self.can_advance() {
            return if self.step == STEP_CANARY {
                WizAction::Finish
            } else {
                WizAction::Next
            };
        }
        WizAction::None
    }
}

/// 5-criteria password strength (mirrors `PasswordPolicy::default_policy`):
/// length≥12, uppercase, lowercase, digit, symbol. 5 = policy-valid.
pub(crate) fn strength_score(pw: &str) -> u8 {
    let mut s = 0u8;
    if pw.chars().count() >= 12 {
        s += 1;
    }
    if pw.chars().any(|c| c.is_ascii_uppercase()) {
        s += 1;
    }
    if pw.chars().any(|c| c.is_ascii_lowercase()) {
        s += 1;
    }
    if pw.chars().any(|c| c.is_ascii_digit()) {
        s += 1;
    }
    if pw.chars().any(|c| !c.is_alphanumeric() && !c.is_whitespace()) {
        s += 1;
    }
    s
}

fn strength_label(s: u8) -> &'static str {
    match s {
        0 => "",
        1 => "Very weak",
        2 => "Weak",
        3 => "Fair",
        4 => "Strong",
        _ => "Very strong",
    }
}

fn strength_color(s: u8) -> Color {
    match s {
        0 | 1 => Color::from_rgb8(0xEF, 0x44, 0x44),
        2 => Color::from_rgb8(0xF5, 0x9E, 0x0B),
        3 => Color::from_rgb8(0x92, 0x61, 0x0a),
        _ => Color::from_rgb8(0x10, 0xB9, 0x81),
    }
}

// ---- Layout --------------------------------------------------------------

fn wiz_card(w: f64, h: f64) -> Rect {
    let cw = 560.0_f64.min(w - 48.0);
    let ch = 600.0_f64.min(h - 48.0);
    let x0 = ((w - cw) * 0.5).round();
    let y0 = ((h - ch) * 0.5).round().max(20.0);
    Rect::new(x0, y0, x0 + cw, y0 + ch)
}

/// Content region between the header (title/progress) and the nav bar.
fn content_rect(card: Rect) -> Rect {
    Rect::new(card.x0 + 32.0, card.y0 + 108.0, card.x1 - 32.0, card.y1 - 70.0)
}

struct WizNav {
    back: Option<Rect>,
    skip: Option<Rect>,
    primary: Rect,
}

fn wiz_nav(card: Rect, step: usize) -> WizNav {
    let by1 = card.y1 - 22.0;
    let by0 = by1 - 34.0;
    let primary = Rect::new(card.x1 - 32.0 - 130.0, by0, card.x1 - 32.0, by1);
    let back = if step > STEP_WELCOME {
        Some(Rect::new(card.x0 + 32.0, by0, card.x0 + 32.0 + 96.0, by1))
    } else {
        None
    };
    let skip = if matches!(step, STEP_DURESS | STEP_CANARY) {
        Some(Rect::new(primary.x0 - 12.0 - 70.0, by0, primary.x0 - 12.0, by1))
    } else {
        None
    };
    WizNav { back, skip, primary }
}

fn welcome_buttons(card: Rect) -> (Rect, Rect) {
    let c = content_rect(card);
    let bw = c.width();
    let first = Rect::new(c.x0, c.y0 + 60.0, c.x0 + bw, c.y0 + 60.0 + 64.0);
    let pair = Rect::new(c.x0, first.y1 + 14.0, c.x0 + bw, first.y1 + 14.0 + 64.0);
    (first, pair)
}

fn bubble_cells(card: Rect) -> Vec<(Rect, BubbleStyle)> {
    let c = content_rect(card);
    let (cols, rows) = (3usize, 3usize);
    let cellw = c.width() / cols as f64;
    let cellh = (c.height() - 10.0) / rows as f64;
    let mut out = Vec::new();
    for (i, &st) in BubbleStyle::all().iter().enumerate() {
        let (gx, gy) = (i % cols, i / cols);
        let x0 = c.x0 + gx as f64 * cellw;
        let y0 = c.y0 + gy as f64 * cellh;
        out.push((Rect::new(x0, y0, x0 + cellw, y0 + cellh), st));
    }
    out
}

fn theme_buttons(card: Rect) -> (Rect, Rect) {
    let c = content_rect(card);
    let bw = (c.width() - 20.0) * 0.5;
    let bh = 120.0_f64.min(c.height() - 20.0);
    let y0 = c.y0 + 30.0;
    let dark = Rect::new(c.x0, y0, c.x0 + bw, y0 + bh);
    let light = Rect::new(c.x1 - bw, y0, c.x1, y0 + bh);
    (dark, light)
}

fn sample_toggle(card: Rect) -> Rect {
    let c = content_rect(card);
    Rect::new(c.x0, c.y0 + 40.0, c.x0 + 52.0, c.y0 + 40.0 + 28.0)
}

fn field_rects(card: Rect, n: usize) -> Vec<Rect> {
    let c = content_rect(card);
    let fh = 42.0;
    let gap = 34.0;
    let mut out = Vec::new();
    let mut y = c.y0 + 34.0;
    for _ in 0..n {
        out.push(Rect::new(c.x0, y, c.x1, y + fh));
        y += fh + gap;
    }
    out
}

fn reveal_btn(field: Rect) -> Rect {
    let bw = 44.0;
    Rect::new(field.x1 - bw - 6.0, field.y0 + 6.0, field.x1 - 6.0, field.y1 - 6.0)
}

// ---- Render --------------------------------------------------------------

fn shape<'a>(shaper: &mut TextShaper, text: &str, max_w: f32, px: f32) -> Layout<Brush> {
    shaper.shape(text, max_w, px)
}

fn text_at(scene: &mut Scene, shaper: &mut TextShaper, s: &str, x: f64, y: f64, px: f32, color: Color) {
    let l = shape(shaper, s, 4000.0, px);
    draw_text(scene, &l, Affine::translate((x, y)), color);
}

fn button(scene: &mut Scene, shaper: &mut TextShaper, r: Rect, label: &str, fg: Color, bg: Color, enabled: bool) {
    let a = if enabled { 1.0 } else { 0.45 };
    scene.fill(Fill::NonZero, Affine::IDENTITY, bg.with_alpha(a), None, &RoundedRect::from_rect(r, 7.0));
    let l = shape(shaper, label, r.width() as f32, 13.0);
    let tx = r.x0 + (r.width() - l.width() as f64) * 0.5;
    let ty = r.y0 + (r.height() - l.height() as f64) * 0.5;
    draw_text(scene, &l, Affine::translate((tx, ty)), fg.with_alpha(a));
}

pub(crate) fn draw_onboarding(scene: &mut Scene, shaper: &mut TextShaper, wiz: &OnboardingWizard, w: f64, h: f64) {
    // Dark full-window backdrop + centered card (matches the auth screen).
    scene.fill(Fill::NonZero, Affine::IDENTITY, Color::from_rgb8(15, 16, 21), None, &Rect::new(0.0, 0.0, w, h));
    let card = wiz_card(w, h);
    let card_rr = RoundedRect::from_rect(card, 12.0);
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().panel, None, &card_rr);
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().border, None, &card_rr);
    scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &card_rr);

    let text = pal().text;
    let dim = pal().text_dim;
    let faint = pal().text_faint;
    let accent = pal().accent;

    // Progress (step X of N), top-right; the Welcome step shows no counter.
    if wiz.step > STEP_WELCOME {
        let prog = format!("Step {} of {}", wiz.step + 1, WIZ_STEPS);
        let l = shape(shaper, &prog, 200.0, 11.5);
        draw_text(scene, &l, Affine::translate((card.x1 - 32.0 - l.width() as f64, card.y0 + 30.0)), faint);
    }

    let (title, body) = step_copy(wiz.step);
    text_at(scene, shaper, title, card.x0 + 32.0, card.y0 + 34.0, 19.0, text);
    {
        let l = shape(shaper, body, (card.width() - 64.0) as f32, 12.5);
        draw_text(scene, &l, Affine::translate((card.x0 + 32.0, card.y0 + 66.0)), dim);
    }

    match wiz.step {
        STEP_WELCOME => draw_welcome(scene, shaper, wiz, card),
        STEP_BUBBLE => draw_bubble_step(scene, shaper, wiz, card),
        STEP_THEME => draw_theme_step(scene, shaper, wiz, card),
        STEP_SAMPLE => draw_sample_step(scene, shaper, wiz, card),
        _ => draw_fields(scene, shaper, wiz, card),
    }

    // Error line just above the nav bar.
    if let Some(e) = &wiz.error {
        let l = shape(shaper, e, (card.width() - 64.0) as f32, 12.0);
        draw_text(scene, &l, Affine::translate((card.x0 + 32.0, card.y1 - 92.0)), Color::from_rgb8(220, 110, 110));
    }

    // Nav bar (not on Welcome — its cards are the nav).
    if wiz.step > STEP_WELCOME {
        let nav = wiz_nav(card, wiz.step);
        if let Some(b) = nav.back {
            button(scene, shaper, b, "Back", text, pal().surface_alt, true);
        }
        if let Some(s) = nav.skip {
            button(scene, shaper, s, "Skip", dim, pal().surface, true);
        }
        let primary_label = if wiz.step == STEP_CANARY { "Get started" } else { "Next" };
        button(scene, shaper, nav.primary, primary_label, pal().on_accent, accent, wiz.can_advance());
    }

    scene.pop_layer();
}

fn step_copy(step: usize) -> (&'static str, &'static str) {
    match step {
        STEP_WELCOME => ("Welcome to Sovereign", "Your personal, sovereign computing environment. Everything runs locally — your data, your AI, your rules."),
        STEP_NICKNAME => ("Name your AI", "Give your assistant a nickname — how it introduces itself. You can change this later, or leave it blank."),
        STEP_BUBBLE => ("Choose a bubble style", "Pick how your AI assistant appears on screen."),
        STEP_THEME => ("Choose your theme", "Select a visual theme for the interface. You can switch any time in settings."),
        STEP_SAMPLE => ("Sample data", "Load example documents, threads, and contacts so you can explore right away."),
        STEP_PASSWORD => ("Set your password", "This password encrypts your local data. Choose something strong — there is no recovery."),
        STEP_DURESS => ("Duress password", "A secondary password that opens a decoy workspace under coercion. Optional — you can skip it."),
        STEP_CANARY => ("Canary phrase", "A personal phrase shown after login. If it ever changes, you'll know the system was tampered with. Optional."),
        _ => ("", ""),
    }
}

fn draw_welcome(scene: &mut Scene, shaper: &mut TextShaper, wiz: &OnboardingWizard, card: Rect) {
    let c = content_rect(card);
    // Designation chip.
    text_at(scene, shaper, "YOUR DESIGNATION", c.x0, c.y0, 10.5, pal().text_faint);
    text_at(scene, shaper, &wiz.designation, c.x0, c.y0 + 16.0, 17.0, pal().accent);

    let (first, pair) = welcome_buttons(card);
    draw_choice(scene, shaper, first, "Set up this device", "Start a fresh workspace from scratch.", true);
    draw_choice(scene, shaper, pair, "Pair with an existing device", "Link to a device you already use and sync.", false);
}

fn draw_choice(scene: &mut Scene, shaper: &mut TextShaper, r: Rect, title: &str, sub: &str, primary: bool) {
    let rr = RoundedRect::from_rect(r, 9.0);
    let bg = if primary { pal().accent.with_alpha(0.16) } else { pal().surface };
    scene.fill(Fill::NonZero, Affine::IDENTITY, bg, None, &rr);
    scene.stroke(&Stroke::new(1.2), Affine::IDENTITY, if primary { pal().accent } else { pal().border }, None, &rr);
    text_at(scene, shaper, title, r.x0 + 16.0, r.y0 + 13.0, 14.0, pal().text);
    text_at(scene, shaper, sub, r.x0 + 16.0, r.y0 + 36.0, 11.5, pal().text_dim);
}

fn draw_bubble_step(scene: &mut Scene, shaper: &mut TextShaper, wiz: &OnboardingWizard, card: Rect) {
    let on = pal().on_accent;
    for (cell, st) in bubble_cells(card) {
        let cc = Point::new((cell.x0 + cell.x1) * 0.5, cell.y0 + 40.0);
        let sr = 28.0;
        if st == wiz.bubble {
            let hl = RoundedRect::from_rect(Rect::new(cell.x0 + 6.0, cell.y0 + 2.0, cell.x1 - 6.0, cell.y1 - 2.0), 10.0);
            scene.fill(Fill::NonZero, Affine::IDENTITY, pal().accent.with_alpha(0.16), None, &hl);
            scene.stroke(&Stroke::new(1.4), Affine::IDENTITY, pal().accent, None, &hl);
        }
        draw_bubble_style(scene, st, cc, sr, bubble_style_color(st), on);
        let l = shape(shaper, st.label(), (cell.width() - 8.0) as f32, 11.5);
        let tx = (cell.x0 + cell.x1) * 0.5 - l.width() as f64 * 0.5;
        draw_text(scene, &l, Affine::translate((tx, cc.y + sr + 8.0)), pal().text_dim);
    }
}

fn draw_theme_step(scene: &mut Scene, shaper: &mut TextShaper, wiz: &OnboardingWizard, card: Rect) {
    let (dark, light) = theme_buttons(card);
    let draw_swatch = |scene: &mut Scene, shaper: &mut TextShaper, r: Rect, is_dark: bool, selected: bool, label: &str| {
        let rr = RoundedRect::from_rect(r, 9.0);
        let (bg, hdr, line) = if is_dark {
            (Color::from_rgb8(20, 22, 28), Color::from_rgb8(38, 42, 52), Color::from_rgb8(70, 76, 90))
        } else {
            (Color::from_rgb8(245, 244, 240), Color::from_rgb8(255, 255, 255), Color::from_rgb8(205, 205, 205))
        };
        scene.fill(Fill::NonZero, Affine::IDENTITY, bg, None, &rr);
        scene.stroke(&Stroke::new(if selected { 1.8 } else { 1.0 }), Affine::IDENTITY, if selected { pal().accent } else { pal().border }, None, &rr);
        // Mock header + two body lines.
        scene.fill(Fill::NonZero, Affine::IDENTITY, hdr, None, &Rect::new(r.x0 + 10.0, r.y0 + 10.0, r.x1 - 10.0, r.y0 + 26.0));
        scene.fill(Fill::NonZero, Affine::IDENTITY, line, None, &Rect::new(r.x0 + 10.0, r.y0 + 38.0, r.x1 - 30.0, r.y0 + 44.0));
        scene.fill(Fill::NonZero, Affine::IDENTITY, line, None, &Rect::new(r.x0 + 10.0, r.y0 + 52.0, r.x1 - 50.0, r.y0 + 58.0));
        let l = shape(shaper, label, r.width() as f32, 12.5);
        draw_text(scene, &l, Affine::translate((r.x0 + (r.width() - l.width() as f64) * 0.5, r.y1 + 8.0)), pal().text_dim);
    };
    draw_swatch(scene, shaper, dark, true, wiz.theme_dark, "Dark");
    draw_swatch(scene, shaper, light, false, !wiz.theme_dark, "Light");
}

fn draw_sample_step(scene: &mut Scene, shaper: &mut TextShaper, wiz: &OnboardingWizard, card: Rect) {
    let t = sample_toggle(card);
    let on = wiz.seed_sample;
    let track = RoundedRect::from_rect(t, t.height() * 0.5);
    scene.fill(Fill::NonZero, Affine::IDENTITY, if on { pal().accent } else { pal().surface_alt }, None, &track);
    let r = t.height() * 0.5 - 3.0;
    let cx = if on { t.x1 - r - 3.0 } else { t.x0 + r + 3.0 };
    scene.fill(Fill::NonZero, Affine::IDENTITY, if on { pal().on_accent } else { pal().text_dim }, None, &vello::kurbo::Circle::new(Point::new(cx, t.y0 + t.height() * 0.5), r));
    text_at(scene, shaper, if on { "Enabled" } else { "Disabled" }, t.x1 + 14.0, t.y0 + 5.0, 13.0, pal().text);
    let hint = if on {
        "Sample documents, threads, and contacts will be created."
    } else {
        "You'll start with a clean, empty workspace."
    };
    text_at(scene, shaper, hint, t.x0, t.y1 + 18.0, 11.5, pal().text_dim);
}

fn draw_fields(scene: &mut Scene, shaper: &mut TextShaper, wiz: &OnboardingWizard, card: Rect) {
    let labels: &[&str] = match wiz.step {
        STEP_NICKNAME => &["Nickname"],
        STEP_PASSWORD => &["Password", "Confirm password"],
        STEP_DURESS => &["Duress password", "Confirm duress password"],
        STEP_CANARY => &["Canary phrase", "Confirm phrase"],
        _ => &[],
    };
    let rects = field_rects(card, wiz.field_count());
    for (i, fr) in rects.iter().enumerate() {
        if let Some(lbl) = labels.get(i) {
            text_at(scene, shaper, lbl, fr.x0, fr.y0 - 18.0, 11.5, pal().text_dim);
        }
        let focused = i == wiz.focus;
        let frr = RoundedRect::from_rect(*fr, 8.0);
        scene.fill(Fill::NonZero, Affine::IDENTITY, pal().on_accent, None, &frr);
        scene.stroke(
            &Stroke::new(if focused { 1.6 } else { 1.0 }),
            Affine::IDENTITY,
            if focused { pal().accent } else { pal().input_border },
            None,
            &frr,
        );
        let masked = wiz.field_masked(i) && !wiz.reveal;
        let shown: String = if masked {
            "\u{2022}".repeat(wiz.field_val(i).chars().count())
        } else {
            wiz.field_val(i).to_string()
        };
        let tx = fr.x0 + 12.0;
        let ty = fr.y0 + (fr.height() - 16.0) * 0.5;
        let l = shape(shaper, &shown, (fr.width() - 70.0) as f32, 14.0);
        draw_text(scene, &l, Affine::translate((tx, ty)), pal().text);
        if focused {
            let cx = tx + l.width() as f64 + 1.0;
            scene.stroke(&Stroke::new(1.5), Affine::IDENTITY, pal().caret, None, &Line::new(Point::new(cx, fr.y0 + 8.0), Point::new(cx, fr.y1 - 8.0)));
        }
        if wiz.field_masked(i) {
            let btn = reveal_btn(*fr);
            scene.fill(Fill::NonZero, Affine::IDENTITY, pal().surface, None, &RoundedRect::from_rect(btn, 5.0));
            let l = shape(shaper, if wiz.reveal { "hide" } else { "show" }, btn.width() as f32, 11.0);
            draw_text(scene, &l, Affine::translate((btn.x0 + (btn.width() - l.width() as f64) * 0.5, btn.y0 + (btn.height() - l.height() as f64) * 0.5)), pal().text_dim);
        }
    }

    // Password strength meter below the fields.
    if wiz.step == STEP_PASSWORD && !wiz.password.is_empty() {
        let s = strength_score(&wiz.password);
        if let Some(first) = rects.first() {
            let y = first.y1 + 8.0;
            let seg_w = (first.width() - 4.0 * 6.0) / 5.0;
            for i in 0..5u8 {
                let x = first.x0 + i as f64 * (seg_w + 6.0);
                let on = i < s;
                let col = if on { strength_color(s) } else { pal().surface_alt };
                scene.fill(Fill::NonZero, Affine::IDENTITY, col, None, &RoundedRect::from_rect(Rect::new(x, y, x + seg_w, y + 5.0), 2.5));
            }
            let l = shape(shaper, strength_label(s), 200.0, 11.0);
            draw_text(scene, &l, Affine::translate((first.x0, y + 9.0)), strength_color(s));
        }
    }
}
