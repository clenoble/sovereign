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

use crate::panels::{bubble_style_color, draw_bubble_style, A11yItem};
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

/// The nickname step's field holding this device's name (v0.0.12 item 19, N2).
pub(crate) const DEVICE_NAME_FIELD: usize = 1;

/// The duress step's refusal of a duress password equal to the main one: the
/// wizard's own check, and the store's (P4) after a Back changed the main one.
pub(crate) const DURESS_EQUALS_MAIN: &str = "Duress password must differ from your password.";

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
    /// This device's name (v0.0.12 item 19, N2): the nickname step's second
    /// field, prefilled with the computer's name; saved with the setup.
    pub(crate) device_name: String,
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
    /// The control the keyboard is on (spec ch. 12, SQ10), and the step it
    /// was set on. `None`, or set on another step: the step's field `focus`
    /// holds it on a step with fields, else the step's group (N4).
    pub(crate) control_focus: Option<(usize, WizTarget)>,
}

/// What the keyboard can be on in a step of the setup (spec ch. 12, SQ10): a
/// field, a choice of a group (the bubble styles, the themes: one Tab stop
/// together, the arrows inside), or a button or checkbox, as what its click
/// does.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum WizTarget {
    Field(usize),
    Choice(WizAction),
    Press(WizAction),
    /// The show/hide toggle drawn in masked field `i`; every toggle of the
    /// step flips its one `reveal` flag, as a click on its face does.
    Reveal(usize),
}

impl OnboardingWizard {
    pub(crate) fn new(designation: String, theme_dark: bool) -> Self {
        Self {
            step: STEP_WELCOME,
            designation,
            nickname: String::new(),
            device_name: crate::device_name::host_name(),
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
            control_focus: None,
        }
    }

    /// The step's targets, in the order of its accessibility items (the drawn
    /// order, top to bottom, left to right on a row): the welcome's two
    /// buttons; the styles row by row, or Dark and Light; the sample-data
    /// checkbox; the fields, then the show/hide toggle; then Back, « Skip »
    /// and the primary.
    pub(crate) fn a11y_targets(&self) -> Vec<WizTarget> {
        use WizTarget::*;
        let mut v = Vec::new();
        match self.step {
            STEP_WELCOME => return vec![Press(WizAction::Next), Press(WizAction::ChoosePair)],
            STEP_BUBBLE => v.extend(BubbleStyle::all().iter().map(|s| Choice(WizAction::SelectBubble(*s)))),
            STEP_THEME => v.extend([Choice(WizAction::SelectTheme(true)), Choice(WizAction::SelectTheme(false))]),
            STEP_SAMPLE => v.push(Press(WizAction::ToggleSample)),
            _ => {
                for (i, toggle_after) in self.field_entries() {
                    v.push(Field(i));
                    if toggle_after {
                        v.push(Reveal(i));
                    }
                }
            }
        }
        v.push(Press(WizAction::Back));
        if matches!(self.step, STEP_DURESS | STEP_CANARY) {
            v.push(Press(WizAction::Skip));
        }
        v.push(Press(if self.step == STEP_CANARY { WizAction::Finish } else { WizAction::Next }));
        v
    }

    /// The step's fields in order, each with whether a show/hide toggle
    /// follows it: each masked field has its own, drawn in it.
    fn field_entries(&self) -> Vec<(usize, bool)> {
        let toggles = self.a11y_show_name().is_some();
        (0..self.field_count()).map(|i| (i, toggles && self.field_masked(i))).collect()
    }

    /// The target the keyboard is on: the one set on this step, else the
    /// focused field, else none (the step's group holds the focus, N4).
    pub(crate) fn focused_target(&self) -> Option<WizTarget> {
        match self.control_focus {
            Some((step, t)) if step == self.step && self.a11y_targets().contains(&t) => Some(t),
            _ if self.field_count() > 0 => Some(WizTarget::Field(self.focus.min(self.field_count() - 1))),
            _ => None,
        }
    }

    /// Put the keyboard on `t`: a field through `focus`, as a click on it.
    pub(crate) fn set_target_focus(&mut self, t: WizTarget) {
        match t {
            WizTarget::Field(i) => {
                self.focus = i;
                self.control_focus = None;
            }
            _ => self.control_focus = Some((self.step, t)),
        }
    }

    /// The step's Tab stops: each target alone, but a group's choices
    /// together.
    fn tab_stops(&self) -> Vec<Vec<WizTarget>> {
        let mut stops: Vec<Vec<WizTarget>> = Vec::new();
        for t in self.a11y_targets() {
            match (t, stops.last_mut()) {
                (WizTarget::Choice(_), Some(last)) if matches!(last[0], WizTarget::Choice(_)) => last.push(t),
                _ => stops.push(vec![t]),
            }
        }
        stops
    }

    /// The checked choice of a group (where Tab enters it).
    fn checked(&self, group: &[WizTarget]) -> WizTarget {
        group
            .iter()
            .copied()
            .find(|t| match t {
                WizTarget::Choice(WizAction::SelectBubble(st)) => *st == self.bubble,
                WizTarget::Choice(WizAction::SelectTheme(dark)) => *dark == self.theme_dark,
                _ => false,
            })
            .unwrap_or(group[0])
    }

    /// Tab (or Shift+Tab, `back`): the next stop, round; from the step's
    /// group, the first (or the last). A group is entered on its checked
    /// choice.
    pub(crate) fn move_target_focus(&mut self, back: bool) {
        let stops = self.tab_stops();
        let n = stops.len();
        let at = self.focused_target().and_then(|f| stops.iter().position(|s| s.contains(&f)));
        let next = match (at, back) {
            (None, false) => 0,
            (None, true) => n - 1,
            (Some(i), false) => (i + 1) % n,
            (Some(i), true) => (i + n - 1) % n,
        };
        let t = if matches!(stops[next][0], WizTarget::Choice(_)) { self.checked(&stops[next]) } else { stops[next][0] };
        self.set_target_focus(t);
    }

    /// An arrow inside a group: the next choice (`by` 1) or the one before
    /// (`by` -1), in order, round, as a radio group does. Whether it moved
    /// (false off a group).
    pub(crate) fn move_in_group(&mut self, by: isize) -> bool {
        let Some(f @ WizTarget::Choice(_)) = self.focused_target() else {
            return false;
        };
        let Some(group) = self.tab_stops().into_iter().find(|s| s.contains(&f)) else {
            return false;
        };
        let n = group.len() as isize;
        let at = group.iter().position(|t| *t == f).unwrap_or(0) as isize;
        let t = group[(at + by).rem_euclid(n) as usize];
        self.set_target_focus(t);
        true
    }

    /// Number of editable text fields on the current step.
    pub(crate) fn field_count(&self) -> usize {
        match self.step {
            STEP_NICKNAME => 2,
            STEP_PASSWORD | STEP_DURESS | STEP_CANARY => 2,
            _ => 0,
        }
    }

    /// Whether field `idx` on this step renders masked (secrets). The canary
    /// phrase is a secret too, entered like a password (v0.0.12, K1–K2).
    fn field_masked(&self, _idx: usize) -> bool {
        matches!(self.step, STEP_PASSWORD | STEP_DURESS | STEP_CANARY)
    }

    fn field_mut(&mut self, idx: usize) -> Option<&mut String> {
        match (self.step, idx) {
            (STEP_NICKNAME, 0) => Some(&mut self.nickname),
            (STEP_NICKNAME, 1) => Some(&mut self.device_name),
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
            (STEP_NICKNAME, 1) => &self.device_name,
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
            return Err(DURESS_EQUALS_MAIN.into());
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
                    // The "show" button is drawn inside its field: test it
                    // first, or the field swallows the click (K1).
                    if self.field_masked(i) && reveal_btn(*fr).contains(p) {
                        return WizAction::ToggleReveal;
                    }
                    if fr.contains(p) {
                        return WizAction::FocusField(i);
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

    /// The accessibility items of the step on screen, in drawn order (v0.0.12
    /// item 20, A1–A5): its title and copy, its widgets, each field named by
    /// the label drawn above it, its error, its buttons. A masked field says
    /// its length only, whether "show" is on or not (A3).
    pub(crate) fn a11y_items(&self) -> Vec<A11yItem> {
        use accesskit::Role;
        let item = |role: Role, label: &str, value: String, focused: bool| (role, label.to_string(), value, focused);
        let (title, body) = step_copy(self.step);
        let mut v = vec![item(Role::Label, title, String::new(), false)];
        if self.step > STEP_WELCOME {
            v.push(item(Role::Label, &format!("Step {} of {}", self.step + 1, WIZ_STEPS), String::new(), false));
        }
        v.push(item(Role::Label, body, String::new(), false));
        match self.step {
            STEP_WELCOME => {
                if !self.designation.trim().is_empty() {
                    v.push(item(Role::Label, &format!("Your designation: {}", self.designation), String::new(), false));
                }
                v.push(item(Role::Button, "Set up this device", String::new(), false));
                v.push(item(Role::Button, "Pair with an existing device", String::new(), false));
            }
            STEP_BUBBLE => {
                for st in BubbleStyle::all() {
                    let name = if *st == self.bubble { format!("{}, selected", st.label()) } else { st.label().into() };
                    v.push(item(Role::RadioButton, &name, String::new(), false));
                }
            }
            STEP_THEME => {
                for (name, dark) in [("Dark", true), ("Light", false)] {
                    let name = if dark == self.theme_dark { format!("{name}, selected") } else { name.into() };
                    v.push(item(Role::RadioButton, &name, String::new(), false));
                }
            }
            STEP_SAMPLE => {
                let (state, hint) = if self.seed_sample {
                    ("Enabled", "Sample documents, threads, and contacts will be created.")
                } else {
                    ("Disabled", "You'll start with a clean, empty workspace.")
                };
                v.push(item(Role::CheckBox, &format!("Load sample data: {state}"), String::new(), false));
                v.push(item(Role::Label, hint, String::new(), false));
            }
            _ => {
                for (i, label) in field_labels(self.step).iter().enumerate() {
                    let val = self.field_val(i);
                    let focused = i == self.focus;
                    if self.field_masked(i) {
                        // Never the text, even with "show" on: its length only.
                        let n = val.chars().count();
                        let len = if n == 0 { String::new() } else { format!("{n} characters") };
                        v.push(item(Role::PasswordInput, label, len, focused));
                    } else {
                        v.push(item(Role::TextInput, label, val.to_string(), focused));
                    }
                    // N2: under this device's name, in plain words.
                    if self.step == STEP_NICKNAME && i == DEVICE_NAME_FIELD {
                        v.push(item(Role::Label, crate::device_name::SEEN_BY, String::new(), false));
                    }
                    // The strength line, under the password it measures.
                    if self.step == STEP_PASSWORD && i == 0 && !self.password.is_empty() {
                        let s = strength_label(strength_score(&self.password));
                        v.push(item(Role::Label, &format!("Password strength: {s}"), String::new(), false));
                    }
                    // Each masked field's own show/hide toggle, right after
                    // it (SQ10), named for what it reveals on this step (K2:
                    // "Show phrase" on the canary step); all flip one flag.
                    if let Some(name) = self.a11y_show_name().filter(|_| self.field_masked(i)) {
                        v.push(item(Role::Button, name, String::new(), false));
                    }
                }
                if self.a11y_show_name().is_some() {
                    // v0.0.12 rc2 (V0012-rc2-labels-spoken, L4): the key, heard
                    // after the toggles and never drawn. Not in the toggles'
                    // own name: K1/K2 (first_run) and N5
                    // (setup_step_announced) pin that name as it is.
                    let what = if self.step == STEP_CANARY { "the phrase" } else { "the passwords" };
                    v.push(item(Role::Label, &format!("{} shows or hides {what}", crate::panels::SHOW_KEY), String::new(), false));
                }
            }
        }
        if let Some(e) = self.error.as_deref().filter(|e| !e.trim().is_empty()) {
            v.push(item(Role::Alert, e, String::new(), false));
        }
        if self.step > STEP_WELCOME {
            v.push(item(Role::Button, "Back", String::new(), false));
            if matches!(self.step, STEP_DURESS | STEP_CANARY) {
                v.push(item(Role::Button, "Skip", String::new(), false));
            }
            let primary = if self.step == STEP_CANARY { "Get started" } else { "Next" };
            v.push(item(Role::DefaultButton, primary, String::new(), false));
        }
        // SQ10: the target the keyboard is on holds the focus. The items of
        // target roles are the step's targets, in order.
        let targets = self.a11y_targets();
        let focused = self.focused_target();
        for (k, it) in v.iter_mut().filter(|it| is_target_role(it.0)).enumerate() {
            it.3 = focused.is_some() && targets.get(k).copied() == focused;
        }
        v
    }

    /// The name of the step's show/hide toggle, on the steps that draw one
    /// (K2; v0.0.12 rc2, V0012-rc2-labels-spoken, L4: Alt+F8 and a screen
    /// reader's Click press it).
    pub(crate) fn a11y_show_name(&self) -> Option<&'static str> {
        match (self.step, self.reveal) {
            (STEP_PASSWORD | STEP_DURESS, false) => Some("Show passwords"),
            (STEP_PASSWORD | STEP_DURESS, true) => Some("Hide passwords"),
            (STEP_CANARY, false) => Some("Show phrase"),
            (STEP_CANARY, true) => Some("Hide phrase"),
            _ => None,
        }
    }

    /// The name of the step's group in the accessibility tree: its title and,
    /// past the welcome step, "Step N of 8" — what a screen reader says when
    /// a step without a field opens (v0.0.12 rc2, V0012-rc2-setup-step-announced, N1).
    pub(crate) fn a11y_name(&self) -> String {
        let (title, _) = step_copy(self.step);
        if self.step > STEP_WELCOME {
            format!("{title}, Step {} of {}", self.step + 1, WIZ_STEPS)
        } else {
            title.to_string()
        }
    }
}

/// Whether an accessibility item of a step is one of its targets (a field, a
/// button, a choice or the checkbox), as `OnboardingWizard::a11y_targets`
/// lists them.
pub(crate) fn is_target_role(role: accesskit::Role) -> bool {
    use accesskit::Role::*;
    matches!(role, TextInput | PasswordInput | Button | DefaultButton | RadioButton | CheckBox)
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

/// `status`: the line of a derivation in flight (v0.0.12 item 18, B2), drawn
/// in the error's place; the buttons are inert meanwhile, and drawn so.
pub(crate) fn draw_onboarding(
    scene: &mut Scene,
    shaper: &mut TextShaper,
    wiz: &OnboardingWizard,
    status: Option<&str>,
    w: f64,
    h: f64,
) {
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

    // Error line just above the nav bar; a derivation's status line takes it.
    if let Some(s) = status {
        let l = shape(shaper, s, (card.width() - 64.0) as f32, 12.0);
        draw_text(scene, &l, Affine::translate((card.x0 + 32.0, card.y1 - 92.0)), accent);
    } else if let Some(e) = &wiz.error {
        let l = shape(shaper, e, (card.width() - 64.0) as f32, 12.0);
        draw_text(scene, &l, Affine::translate((card.x0 + 32.0, card.y1 - 92.0)), Color::from_rgb8(220, 110, 110));
    }

    // Nav bar (not on Welcome — its cards are the nav).
    if wiz.step > STEP_WELCOME {
        let idle = status.is_none();
        let nav = wiz_nav(card, wiz.step);
        if let Some(b) = nav.back {
            button(scene, shaper, b, "Back", text, pal().surface_alt, idle);
        }
        if let Some(s) = nav.skip {
            button(scene, shaper, s, "Skip", dim, pal().surface, idle);
        }
        let primary_label = if wiz.step == STEP_CANARY { "Get started" } else { "Next" };
        button(scene, shaper, nav.primary, primary_label, crate::theme::text_on_fill(accent), accent, idle && wiz.can_advance());
    }

    // The keyboard's control has a visible focus outline (WCAG 2.4.7); a
    // focused field has its focused-field border instead (`draw_fields`, TC3).
    for r in focus_rects(wiz, card) {
        scene.stroke(&Stroke::new(2.0), Affine::IDENTITY, pal().text, None, &RoundedRect::from_rect(r.inflate(3.0, 3.0), 9.0));
    }

    scene.pop_layer();
}

/// Where the control the keyboard is on is drawn (its outline goes around
/// it): none on a field or on the step's group.
fn focus_rects(wiz: &OnboardingWizard, card: Rect) -> Vec<Rect> {
    let Some(t) = wiz.focused_target() else {
        return Vec::new();
    };
    let nav = wiz_nav(card, wiz.step);
    let a = match t {
        WizTarget::Field(_) => return Vec::new(),
        // Only the focused toggle's own face.
        WizTarget::Reveal(i) => return field_rects(card, wiz.field_count()).get(i).map(|fr| reveal_btn(*fr)).into_iter().collect(),
        WizTarget::Choice(a) | WizTarget::Press(a) => a,
    };
    match (wiz.step, a) {
        (STEP_WELCOME, WizAction::Next) => vec![welcome_buttons(card).0],
        (STEP_WELCOME, WizAction::ChoosePair) => vec![welcome_buttons(card).1],
        (_, WizAction::SelectBubble(st)) => bubble_cells(card)
            .into_iter()
            .filter(|(_, s)| *s == st)
            .map(|(cell, _)| Rect::new(cell.x0 + 6.0, cell.y0 + 2.0, cell.x1 - 6.0, cell.y1 - 2.0))
            .collect(),
        (_, WizAction::SelectTheme(dark)) => {
            let (d, l) = theme_buttons(card);
            vec![if dark { d } else { l }]
        }
        (_, WizAction::ToggleSample) => vec![sample_toggle(card)],
        (_, WizAction::Back) => nav.back.into_iter().collect(),
        (_, WizAction::Skip) => nav.skip.into_iter().collect(),
        (_, WizAction::Next | WizAction::Finish) => vec![nav.primary],
        _ => Vec::new(),
    }
}

fn step_copy(step: usize) -> (&'static str, &'static str) {
    match step {
        STEP_WELCOME => ("Welcome to Sovereign", "Your personal, sovereign computing environment. Everything runs locally — your data, your AI, your rules."),
        STEP_NICKNAME => ("Name your AI", "Give your assistant a nickname — how it introduces itself. You can change this later, or leave it blank."),
        STEP_BUBBLE => ("Choose a bubble style", "Pick how your AI assistant appears on screen."),
        STEP_THEME => ("Choose your theme", "Select a visual theme for the interface. You can switch any time in settings."),
        STEP_SAMPLE => ("Sample data", "Load example documents, threads, and contacts so you can explore right away."),
        STEP_PASSWORD => ("Set your password", "This password encrypts your local data. Choose something strong. Lose it and social recovery has your back: enrol 5 trusted guardians after setup — your secrets stay yours as long as fewer than 3 of your 5 guardians collude. Later you can host encrypted fragments for friends, and they for you."),
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
    scene.fill(Fill::NonZero, Affine::IDENTITY, if on { crate::theme::text_on_fill(pal().accent) } else { pal().text_dim }, None, &vello::kurbo::Circle::new(Point::new(cx, t.y0 + t.height() * 0.5), r));
    text_at(scene, shaper, if on { "Enabled" } else { "Disabled" }, t.x1 + 14.0, t.y0 + 5.0, 13.0, pal().text);
    let hint = if on {
        "Sample documents, threads, and contacts will be created."
    } else {
        "You'll start with a clean, empty workspace."
    };
    text_at(scene, shaper, hint, t.x0, t.y1 + 18.0, 11.5, pal().text_dim);
}

/// The labels drawn above a step's fields, in order: what they are named in
/// the accessibility tree too (A2).
fn field_labels(step: usize) -> &'static [&'static str] {
    match step {
        STEP_NICKNAME => &["Nickname", crate::device_name::FIELD_LABEL],
        STEP_PASSWORD => &["Password", "Confirm password"],
        STEP_DURESS => &["Duress password", "Confirm duress password"],
        STEP_CANARY => &["Canary phrase", "Confirm phrase"],
        _ => &[],
    }
}

fn draw_fields(scene: &mut Scene, shaper: &mut TextShaper, wiz: &OnboardingWizard, card: Rect) {
    let labels = field_labels(wiz.step);
    let rects = field_rects(card, wiz.field_count());
    for (i, fr) in rects.iter().enumerate() {
        if let Some(lbl) = labels.get(i) {
            text_at(scene, shaper, lbl, fr.x0, fr.y0 - 18.0, 11.5, pal().text_dim);
        }
        // Not while a button holds the keyboard (SQ10).
        let focused = wiz.focused_target() == Some(WizTarget::Field(i));
        let frr = RoundedRect::from_rect(*fr, 8.0);
        scene.fill(Fill::NonZero, Affine::IDENTITY, crate::theme::field_fill(&pal()), None, &frr);
        scene.stroke(
            &Stroke::new(if focused { 1.6 } else { 1.0 }),
            Affine::IDENTITY,
            if focused { crate::theme::focused_field_border(&pal()) } else { pal().input_border },
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
        // N2: under this device's name, in plain words.
        if wiz.step == STEP_NICKNAME && i == DEVICE_NAME_FIELD {
            text_at(scene, shaper, crate::device_name::SEEN_BY, fr.x0, fr.y1 + 6.0, 11.0, pal().text_faint);
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

#[cfg(test)]
#[path = "onboarding_reveal_tests.rs"]
mod reveal_tests;
