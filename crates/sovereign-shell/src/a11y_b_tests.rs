//! Spec ch. 12, *The shell's questions and notices, heard and answered from the keyboard*, SQ10,
//! lot B (v0.0.13; Céline, 2026-11-19: « Et on met tous les éléments dès maintenant. »; spec
//! v2.0.135): the whole setup, from the welcome screen to the workspace, by keys alone — its
//! welcome buttons, its bubble-style and theme choices, its other steps (the nickname, the
//! passwords, duress, the canary, the sample-data checkbox, « Skip »), Back and Next in each step's
//! cycle; the recovery wizard while locked (its field and buttons); the pairing offer; the guardian
//! enrolment.
//!
//! Written before the code, by lot B's test author: whoever builds lot B makes these pass
//! **without editing this file**; a test that looks wrong is reported, not worked around.
//!
//! What a test may assert, from the contract (SQ10, with SQ5 and SQ8): - each window, form and
//! control is in the tree, its controls as buttons, fields or choices named as drawn, each taking a
//! screen reader's Click and Focus; - every action a click takes, a key takes too: Tab and
//! Shift+Tab move between a window's controls in the drawn order, top to bottom (and stay inside
//! it), Enter or Space presses the focused one, Escape closes where it closes today; - a group of
//! choices (the bubble styles, the themes) is one stop for Tab: Tab enters it on its checked choice
//! and the next Tab leaves it; the arrows move between the choices inside; Enter or Space picks the
//! focused one, as a click does; - each dialog says its keys in one line, as SQ4 asks of the
//! prompts; - SQ5: an answer by a key or a screen reader does what the click does; - SQ8: what a
//! click does today is unchanged — the `sq8_` tests hold today's clicks and pass before the code; -
//! the locked screens' rules (L1 to L5, N1 to N5, A1 to A5, K1 and K2) stay: they are held by
//! `labels_spoken_tests`, `setup_step_announced_tests` and `first_run_tests`, not here.
//!
//! **Particular cases.** Those of the first version were decided by the Fable maille, by delegation
//! of Céline (2026-11-14), on 2026-11-19, and the tests say so where they hold one. The few the
//! contract still leaves open are marked « proposed, to be confirmed »; if the Fable maille decides
//! otherwise, the test changes with its decision.
//!
//! **The show/hide toggle and the arrows** (the Fable maille, by delegation of Céline (2026-11-14),
//! on 2026-11-19): on a step with two masked fields, each field has its own toggle node right after
//! it, in the tree and in Tab order, each flipping the step's one flag (both fields shown or
//! hidden), each keeping today's name; a step with one masked field keeps one toggle, right after
//! it. In a group of choices the arrows are the WAI-ARIA radio group's: Right and Down to the next
//! choice, Left and Up to the previous, in the tree's order, wrapping at both ends, moving the
//! focus and picking nothing. The nine bubble styles take the same keys as lot C's picker; lot C's
//! tests do not hold that yet, so these do for the setup.
//!
//! **Before this lot, on the setup's steps with fields** (read in `App::locked_key` and
//! `OnboardingWizard::focus_next`): Tab goes between the step's two fields only, never to the
//! show/hide toggle, Back, « Skip » or Next; Enter goes to the next field, then is Next (or « Get
//! started »); Escape is Back; on the sample-data step, Tab does nothing and the checkbox takes a
//! click only. So the walk of each step is this lot's, not a guard.
//!
//! **One key seam.** Every key goes through `App::shell_key`, the one entry point the three
//! accessibility lots share (its signature is lot A's test author's; the release owner,
//! 2026-11-19). It is declared with an `unimplemented!()` body until lot A's code lands, so every
//! test that presses a key is red until then. Only `key_with` below calls it: aligning with
//! lot A's final seam is a change of that one function.
//!
//! A child of `app` (declared in `app.rs`), to drive the wizard, the modals, the clicks, the
//! screen reader's actions and `build_a11y_tree` on a real `App`. The locked screens run on a
//! fresh `App` in a temporary data directory; the pairing offer and the guardian enrolment on an
//! unlocked one (`device_names_tests::unlocked`), whose clipboard is a fake: nothing touches the
//! user's clipboard.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use accesskit::{Action, ActionRequest, Node, NodeId, Role as AkRole, TreeId, TreeUpdate};
use sovereign_core::profile::BubbleStyle;
use sovereign_editor::input::Modifiers;
use winit::keyboard::{Key, NamedKey};

use super::device_names_tests::{unlocked, Unlocked};
use super::editor_v2;
use super::first_run_tests::{
    first_frame, frames_until, launch, profile, setup_settled, shut_runtime, CHILD_VAR, DURESS, H, PRIMARY, W,
};
use super::App;
use crate::onboarding::{
    OnboardingWizard, WizAction, STEP_BUBBLE, STEP_CANARY, STEP_DURESS, STEP_NICKNAME, STEP_PASSWORD, STEP_SAMPLE,
    STEP_THEME, STEP_WELCOME,
};
use crate::panels::{
    guardian_modal_layout, pairing_modal_layout, recovery_wizard_layout, AuthForm, GuardianEnrollModal,
    PairingModal, RecoveryPhase, RecoveryWizard,
};

// ======================================================================= the words drawn

const SET_UP: &str = "Set up this device";
const PAIR: &str = "Pair with an existing device";
const BACK: &str = "Back";
const NEXT: &str = "Next";
const DARK: &str = "Dark";
const LIGHT: &str = "Light";
const SKIP: &str = "Skip";
const GET_STARTED: &str = "Get started";
/// The setup's fields, named by the labels drawn above them (A2), per step.
const NICKNAME: &str = "Nickname";
const DEVICE_NAME: &str = "This device's name";
const PASSWORD: &str = "Password";
const CONFIRM_PASSWORD: &str = "Confirm password";
const DURESS_FIELD: &str = "Duress password";
const CONFIRM_DURESS: &str = "Confirm duress password";
const CANARY_FIELD: &str = "Canary phrase";
const CONFIRM_PHRASE: &str = "Confirm phrase";
/// The setup's show/hide toggle, as L4 names it.
const SHOW_PASSWORDS: &str = "Show passwords";
const SHOW_PHRASE: &str = "Show phrase";
/// The sample-data step's checkbox, as the tree names it today (N5).
const SAMPLE_ON: &str = "Load sample data: Enabled";
const SAMPLE_OFF: &str = "Load sample data: Disabled";
/// Test secrets for the setup's steps.
const SETUP_CANARY: &str = "blue heron";

/// The pairing offer's title and buttons, as `PairingModal::ensure_shaped` draws them. The
/// contract (v2.0.135) names « Copy code »; it draws « Copied! » once pressed, and a control is
/// « named as drawn ».
const PAIR_TITLE: &str = "Pair a device";
const COPY_CODE: &str = "Copy code";
const COPIED: &str = "Copied!";
const DONE: &str = "Done";
const OFFER_CODE: &str = "OFFER-CODE-base64url-a11y-b";
const OFFER_PIN: &str = "48291-30576";

const GUARDIAN_SLOT: usize = 3;
const GUARDIAN_TITLE: &str = "Enroll guardian 3 of 5";

/// The recovery wizard's titles and buttons, as `draw_recovery_wizard` draws them, per phase.
const REC_SET_TITLE: &str = "Recover access — set a new password";
const REC_RESUME_TITLE: &str = "Resume recovery";
const REC_AWAIT_TITLE: &str = "Recover access with guardians";
const REC_READY_TITLE: &str = "Recover now";
const REC_FAILED_TITLE: &str = "Recovery couldn't finish";
const REC_FIELD: &str = "New password";
const START_RECOVERY: &str = "Start recovery";
const CANCEL: &str = "Cancel";
const CHECK_NOW: &str = "Check now";
const CLOSE: &str = "Close";
const RECOVER: &str = "Recover";
const START_OVER: &str = "Start over";
/// What a fresh start says to a password under the policy (`recovery_password_submit`,
/// `recovery_finalize`): an effect a click has today that contacts no guardian.
const TOO_WEAK: &str = "Choose a stronger password (12+ chars, mixed case, a digit, a symbol).";
const WEAK: &str = "weak";

// ======================================================================= the profile

/// The in-process tests' data directory: a fresh temporary one unless `SOVEREIGN_DATA_DIR` is
/// already set (by another test of this binary, or by the run).
fn ensure_test_data_dir() {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        if let Some(d) = std::env::var_os("SOVEREIGN_DATA_DIR") {
            return d.into();
        }
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let d = std::env::temp_dir().join(format!("sov-a11y-b-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::env::set_var("SOVEREIGN_DATA_DIR", &d);
        d
    });
}

/// A locked shell showing the setup at `step`. The theme starts on Light, so that choosing Dark
/// sets the palette the process already has by default (the palette is process-wide).
fn setup_at(step: usize) -> App {
    ensure_test_data_dir();
    let mut app = App::new();
    app.locked = true;
    app.auth_form = AuthForm::login();
    let mut w = OnboardingWizard::new("Designation-7".into(), false);
    w.step = step;
    w.focus = 0;
    app.wizard = Some(w);
    app.test_view = Some((W, H));
    app
}

fn step(app: &App) -> Option<usize> {
    app.wizard.as_ref().map(|w| w.step)
}

/// The setup at `step`, its fields filled as a person who reached it would have them (the
/// password steps valid), the focus on its first field.
fn setup_filled(step: usize) -> App {
    let mut app = setup_at(step);
    let w = app.wizard.as_mut().unwrap();
    w.nickname = "Ada".into();
    w.password = PRIMARY.into();
    w.password_confirm = PRIMARY.into();
    w.duress = DURESS.into();
    w.duress_confirm = DURESS.into();
    w.canary = SETUP_CANARY.into();
    w.canary_confirm = SETUP_CANARY.into();
    app
}

/// The controls of a setup step with fields, in the drawn order, top to bottom: each field, then
/// its own show/hide toggle where the field is masked (drawn at the field's right end; two nodes of
/// the same name, each right after its field: the Fable maille, by delegation of Céline
/// (2026-11-14), on 2026-11-19), then the bottom row, left to right: Back, « Skip », the primary.
fn field_step_controls(step: usize) -> Vec<String> {
    let v: &[&str] = match step {
        STEP_NICKNAME => &[NICKNAME, DEVICE_NAME, BACK, NEXT],
        STEP_PASSWORD => &[PASSWORD, SHOW_PASSWORDS, CONFIRM_PASSWORD, SHOW_PASSWORDS, BACK, NEXT],
        STEP_DURESS => &[DURESS_FIELD, SHOW_PASSWORDS, CONFIRM_DURESS, SHOW_PASSWORDS, BACK, SKIP, NEXT],
        STEP_CANARY => &[CANARY_FIELD, SHOW_PHRASE, CONFIRM_PHRASE, SHOW_PHRASE, BACK, SKIP, GET_STARTED],
        _ => unreachable!("not a step with fields"),
    };
    v.iter().map(|s| s.to_string()).collect()
}

/// The steps with two masked fields: (step, its two fields, its toggle's name with show off and
/// on).
const MASKED_STEPS: [(usize, [&str; 2], &str, &str); 3] = [
    (STEP_PASSWORD, [PASSWORD, CONFIRM_PASSWORD], SHOW_PASSWORDS, "Hide passwords"),
    (STEP_DURESS, [DURESS_FIELD, CONFIRM_DURESS], SHOW_PASSWORDS, "Hide passwords"),
    (STEP_CANARY, [CANARY_FIELD, CONFIRM_PHRASE], SHOW_PHRASE, "Hide phrase"),
];

const FIELD_STEPS: [usize; 4] = [STEP_NICKNAME, STEP_PASSWORD, STEP_DURESS, STEP_CANARY];

/// A locked shell on its login form, the recovery wizard open over it at `phase` (as « Forgot
/// your password? » opens it). No frame is drawn, so the setup never comes back under it.
fn recovery_at(phase: RecoveryPhase) -> App {
    ensure_test_data_dir();
    let mut app = App::new();
    app.locked = true;
    app.wizard = None;
    app.auth_form = AuthForm::login();
    app.recovery_available = true;
    let mut w = RecoveryWizard::fresh();
    w.phase = phase;
    if phase != RecoveryPhase::SetPassword {
        w.shares = 1;
        w.threshold = 3;
        w.released = vec![true, false, false, false, false];
        w.new_password = WEAK.into();
    }
    if phase == RecoveryPhase::Failed {
        w.error = Some("Something went wrong reaching your guardians.".into());
    }
    app.recovery_wizard = Some(w);
    app.test_view = Some((W, H));
    app
}

fn rec_phase(app: &App) -> Option<RecoveryPhase> {
    app.recovery_wizard.as_ref().map(|w| w.phase)
}

/// An unlocked shell showing the pairing offer, its clipboard a fake.
fn pairing(tag: &str) -> Unlocked {
    let mut u = unlocked(tag);
    u.app.pairing_modal = Some(PairingModal::new(OFFER_CODE.into(), OFFER_PIN.into()));
    u.app.test_view = Some((W, H));
    u
}

/// An unlocked shell showing the guardian enrolment.
fn guardian(tag: &str) -> Unlocked {
    let mut u = unlocked(tag);
    u.app.guardian_modal = Some(GuardianEnrollModal::new("GUARDIAN-OFFER".into(), "48291-30576".into(), GUARDIAN_SLOT));
    u.app.test_view = Some((W, H));
    u
}

/// What the (fake) system clipboard holds now.
fn clipboard_text() -> Option<String> {
    editor_v2::clipboard_for_tests().borrow_mut().read().map(|c| c.text)
}

// ======================================================================= the tree, as a screen reader reads it

struct Tree {
    up: TreeUpdate,
    parent: HashMap<NodeId, NodeId>,
}

impl Tree {
    fn of(app: &mut App) -> Tree {
        let up = app.build_a11y_tree();
        let mut parent = HashMap::new();
        for (id, n) in &up.nodes {
            for c in n.children() {
                parent.insert(*c, *id);
            }
        }
        Tree { up, parent }
    }
    fn node(&self, id: NodeId) -> Option<&Node> {
        self.up.nodes.iter().find(|(i, _)| *i == id).map(|(_, n)| n)
    }
    fn spoken(&self, id: NodeId) -> String {
        crate::a11y_spoken::spoken_name(&self.up.nodes, id)
    }
    /// Whether `id` is reachable from the tree's root (a node no parent holds is not read).
    fn reachable(&self, id: NodeId) -> bool {
        let root = self.up.tree.as_ref().map(|t| t.root).unwrap_or(NodeId(0));
        let mut at = id;
        for _ in 0..64 {
            if at == root {
                return true;
            }
            match self.parent.get(&at) {
                Some(p) => at = *p,
                None => return false,
            }
        }
        false
    }
    fn within(&self, ancestor: NodeId, id: NodeId) -> bool {
        let mut at = id;
        for _ in 0..64 {
            if at == ancestor {
                return true;
            }
            match self.parent.get(&at) {
                Some(p) => at = *p,
                None => return false,
            }
        }
        false
    }
    /// The reachable nodes of `roles` whose spoken name is exactly `name`.
    fn named(&self, roles: &[AkRole], name: &str) -> Vec<NodeId> {
        self.up
            .nodes
            .iter()
            .filter(|(id, n)| roles.contains(&n.role()) && self.reachable(*id) && self.spoken(*id) == name)
            .map(|(id, _)| *id)
            .collect()
    }
    fn one(&self, roles: &[AkRole], name: &str) -> NodeId {
        let v = self.named(roles, name);
        assert_eq!(v.len(), 1, "one {roles:?} named « {name} » in the tree, found {}:\n{}", v.len(), self.dump());
        v[0]
    }
    /// The one reachable dialog named `title` (a modal window is a dialog, as the same-name
    /// question's is; an alert dialog counts too).
    fn dialog(&self, title: &str) -> NodeId {
        self.one(&[AkRole::Dialog, AkRole::AlertDialog], title)
    }
    fn focused_name(&self) -> String {
        self.spoken(self.up.focus)
    }
    /// The reachable nodes in document order (depth first, children in order), as a screen reader
    /// walks them.
    fn order(&self) -> Vec<NodeId> {
        let root = self.up.tree.as_ref().map(|t| t.root).unwrap_or(NodeId(0));
        let mut out = Vec::new();
        let mut stack = vec![root];
        let mut seen = std::collections::HashSet::new();
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            if let Some(n) = self.node(id) {
                out.push(id);
                for c in n.children().iter().rev() {
                    stack.push(*c);
                }
            }
        }
        out
    }
    /// The next node after `id` in document order that is not a label (a text read between two
    /// controls does not count).
    fn next_control(&self, id: NodeId) -> Option<NodeId> {
        let order = self.order();
        let at = order.iter().position(|i| *i == id)?;
        order[at + 1..].iter().copied().find(|i| self.node(*i).is_some_and(|n| n.role() != AkRole::Label))
    }
    /// The control before `id` in document order, labels skipped.
    fn prev_control(&self, id: NodeId) -> Option<NodeId> {
        let order = self.order();
        let at = order.iter().position(|i| *i == id)?;
        order[..at].iter().rev().copied().find(|i| self.node(*i).is_some_and(|n| n.role() != AkRole::Label))
    }
    fn every_string(&self) -> Vec<String> {
        let mut v = Vec::new();
        for (_, n) in &self.up.nodes {
            v.extend(n.label().map(str::to_string));
            v.extend(n.value().map(str::to_string));
            v.extend(n.description().map(str::to_string));
        }
        v
    }
    fn dump(&self) -> String {
        self.up
            .nodes
            .iter()
            .filter(|(id, _)| self.reachable(*id))
            .map(|(id, n)| {
                let f = if *id == self.up.focus { " <focus>" } else { "" };
                format!("{:?} {:?} {:?}{f}", id, n.role(), self.spoken(*id))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

const BUTTONS: [AkRole; 2] = [AkRole::Button, AkRole::DefaultButton];
const CHOICES: [AkRole; 1] = [AkRole::RadioButton];
const SECRET_FIELD: [AkRole; 1] = [AkRole::PasswordInput];

/// The control `name` takes a screen reader's Click and Focus (SQ10).
fn assert_clickable_and_focusable(t: &Tree, id: NodeId, what: &str) {
    let n = t.node(id).expect("the node is in the update");
    assert!(n.supports_action(Action::Click), "{what}: « {} » takes a screen reader's Click", t.spoken(id));
    assert!(n.supports_action(Action::Focus), "{what}: « {} » takes a screen reader's Focus", t.spoken(id));
}

/// A field takes Focus (a Click on it is not asked: it is a field).
fn assert_focusable(t: &Tree, id: NodeId, what: &str) {
    let n = t.node(id).expect("the node is in the update");
    assert!(n.supports_action(Action::Focus), "{what}: « {} » takes a screen reader's Focus", t.spoken(id));
}

// ======================================================================= pressing, as she does

fn act(app: &mut App, id: NodeId, action: Action) {
    app.a11y_action(ActionRequest { action, target_tree: TreeId::ROOT, target_node: id, data: None });
}

/// A screen reader's Click on the control of `roles` named `name`.
fn sr_click(app: &mut App, roles: &[AkRole], name: &str) {
    let id = Tree::of(app).one(roles, name);
    act(app, id, Action::Click);
}

/// A screen reader's Focus on the control of `roles` named `name`; the tree's focus after it.
fn sr_focus(app: &mut App, roles: &[AkRole], name: &str) -> String {
    let id = Tree::of(app).one(roles, name);
    act(app, id, Action::Focus);
    Tree::of(app).focused_name()
}

/// A key, as `window_event` receives it: first the shell's one key seam (`App::shell_key`, the
/// three accessibility lots' entry point), then — what it did not take, while locked and with no
/// recovery wizard over the gate — `App::locked_key`, where `window_event` sends a locked screen's
/// keys today. The recovery wizard's, the pairing offer's and the guardian enrolment's keys are
/// handled inline in `window_event` today, which a test cannot reach: so those go through the
/// seam only. Aligning with lot A's final seam is a change of this function only.
fn key_with(app: &mut App, key: Key, text: Option<&str>, shift: bool) {
    let mods = Modifiers { shift, ..Modifiers::NONE };
    if app.shell_key(&key, text, mods) {
        return;
    }
    if app.locked && app.recovery_wizard.is_none() {
        app.locked_key(&key, text);
    }
}

fn tab(app: &mut App) {
    key_with(app, Key::Named(NamedKey::Tab), None, false);
}
fn shift_tab(app: &mut App) {
    key_with(app, Key::Named(NamedKey::Tab), None, true);
}
fn enter(app: &mut App) {
    key_with(app, Key::Named(NamedKey::Enter), Some("\r"), false);
}
fn space(app: &mut App) {
    key_with(app, Key::Named(NamedKey::Space), Some(" "), false);
}
fn escape(app: &mut App) {
    key_with(app, Key::Named(NamedKey::Escape), None, false);
}
fn arrow(app: &mut App, k: NamedKey) {
    key_with(app, Key::Named(k), None, false);
}
fn type_text(app: &mut App, s: &str) {
    for ch in s.chars() {
        let t = ch.to_string();
        key_with(app, Key::Character(t.as_str().into()), Some(&t), false);
    }
}

/// Tab from the window's first focus until it comes back: the names it visits, in order.
/// Panics if it does not come back within `limit` presses.
fn tab_cycle(app: &mut App, limit: usize, backwards: bool) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for _ in 0..limit {
        if backwards {
            shift_tab(app);
        } else {
            tab(app);
        }
        let name = Tree::of(app).focused_name();
        if seen.first() == Some(&name) {
            return seen;
        }
        seen.push(name);
    }
    panic!("Tab does not come back to where it started within {limit} presses: {seen:?}");
}

/// SQ10: Tab and Shift+Tab walk `want`, in this order, the drawn order, top to bottom and left to
/// right on a row, and come back. `fwd` and `back` are the same window, opened twice. The window's
/// first focus is either one of `want` (the walk starts after it) or outside the cycle (a step's
/// group, N4: the first Tab lands on the first control, the first Shift+Tab on the last).
fn assert_tab_order(fwd: &mut App, back: &mut App, want: &[String], what: &str) {
    let n = want.len();
    let start = Tree::of(fwd).focused_name();
    assert_eq!(Tree::of(back).focused_name(), start, "{what}: the two openings start alike");
    let at = want.iter().position(|w| *w == start);
    let forward: Vec<String> = match at {
        Some(i) => (1..=n).map(|k| want[(i + k) % n].clone()).collect(),
        None => want.to_vec(),
    };
    let backward: Vec<String> = match at {
        Some(i) => (1..=n).map(|k| want[(i + 2 * n - k) % n].clone()).collect(),
        None => want.iter().rev().cloned().collect(),
    };
    let mut got = Vec::new();
    for _ in 0..n {
        tab(fwd);
        got.push(Tree::of(fwd).focused_name());
    }
    assert_eq!(got, forward, "{what}: Tab walks the controls in the drawn order and comes back");
    let mut got = Vec::new();
    for _ in 0..n {
        shift_tab(back);
        got.push(Tree::of(back).focused_name());
    }
    assert_eq!(got, backward, "{what}: Shift+Tab walks them the other way");
}

/// Each dialog says its keys in one line (SQ10, as SQ4 asks of the prompts): a label of the
/// dialog that names each of `keys`.
fn assert_keys_line(t: &Tree, dialog: NodeId, keys: &[&str], what: &str) {
    let lines: Vec<String> = t
        .up
        .nodes
        .iter()
        .filter(|(id, n)| n.role() == AkRole::Label && t.within(dialog, *id))
        .map(|(id, _)| t.spoken(*id))
        .collect();
    assert!(
        lines.iter().any(|l| keys.iter().all(|k| l.contains(k))),
        "{what}: one line of the dialog says the keys {keys:?}; its lines: {lines:#?}"
    );
}

/// Tab until the focus is on `name`; panics if it never is.
fn tab_to(app: &mut App, name: &str) {
    for _ in 0..16 {
        if Tree::of(app).focused_name() == name {
            return;
        }
        tab(app);
    }
    let t = Tree::of(app);
    panic!("Tab never reaches « {name} »:\n{}", t.dump());
}

/// The bubble-style choices' names as the tree says them today (N5 keeps them): the style's
/// label, and « , selected » after the chosen one.
fn bubble_name(st: BubbleStyle, chosen: BubbleStyle) -> String {
    if st == chosen {
        format!("{}, selected", st.label())
    } else {
        st.label().to_string()
    }
}

fn theme_name(name: &str, is_dark: bool, wiz_dark: bool) -> String {
    if is_dark == wiz_dark {
        format!("{name}, selected")
    } else {
        name.to_string()
    }
}

/// A point of the setup's card where a mouse click resolves to an action `want` accepts (the
/// wizard's own hit-test), then the click there, through `App::handle_click`.
fn mouse_on_wizard(app: &mut App, want: impl Fn(WizAction) -> bool) {
    let wiz = app.wizard.as_ref().expect("the setup is shown");
    let mut at = None;
    'scan: for y in (0..H as i32).step_by(4) {
        for x in (0..W as i32).step_by(4) {
            if want(wiz.hit_test(vello::kurbo::Point::new(x as f64, y as f64), W, H)) {
                at = Some((x as f64, y as f64));
                break 'scan;
            }
        }
    }
    let (x, y) = at.expect("a drawn control resolves to that action");
    app.handle_click(x, y);
}

fn centre(r: vello::kurbo::Rect) -> (f64, f64) {
    ((r.x0 + r.x1) * 0.5, (r.y0 + r.y1) * 0.5)
}

// ======================================================================= the setup's welcome

/// SQ10: the welcome step's two buttons are in the tree, named as drawn (true today, kept).
#[test]
fn sq10_welcome_buttons_are_named_as_drawn() {
    let mut app = setup_at(STEP_WELCOME);
    let t = Tree::of(&mut app);
    t.one(&BUTTONS, SET_UP);
    t.one(&BUTTONS, PAIR);
}

/// SQ10: each takes a screen reader's Click and Focus.
#[test]
fn sq10_welcome_buttons_take_click_and_focus() {
    let mut app = setup_at(STEP_WELCOME);
    let t = Tree::of(&mut app);
    for name in [SET_UP, PAIR] {
        let id = t.one(&BUTTONS, name);
        assert_clickable_and_focusable(&t, id, "the welcome step");
    }
}

/// SQ10, SQ5: a screen reader's Click on « Set up this device » does what a click does: the
/// next step.
#[test]
fn sq10_welcome_set_up_by_screen_reader_goes_on() {
    let mut app = setup_at(STEP_WELCOME);
    sr_click(&mut app, &BUTTONS, SET_UP);
    assert_eq!(step(&app), Some(STEP_NICKNAME), "the setup goes to its next step, as a click does");
}

/// SQ10, SQ5: a screen reader's Click on « Pair with an existing device » does what a click
/// does: the setup gives way to the join form.
#[test]
fn sq10_welcome_pair_by_screen_reader_opens_the_join() {
    let mut app = setup_at(STEP_WELCOME);
    sr_click(&mut app, &BUTTONS, PAIR);
    assert!(app.wizard.is_none(), "the setup gives way, as a click does");
    assert!(app.auth_form.joining, "the join form is shown, as a click does");
}

/// SQ10: a screen reader's Focus on a welcome button gives it the focus.
#[test]
fn sq10_welcome_buttons_take_the_focus_from_a_screen_reader() {
    let mut app = setup_at(STEP_WELCOME);
    assert_eq!(sr_focus(&mut app, &BUTTONS, PAIR), PAIR);
    assert_eq!(sr_focus(&mut app, &BUTTONS, SET_UP), SET_UP);
    assert_eq!(step(&app), Some(STEP_WELCOME), "a Focus presses nothing");
}

/// SQ10: Tab moves between the welcome step's two buttons in the drawn order, top to bottom, and
/// stays there; Shift+Tab walks the other way. Decided by the Fable maille, by delegation of Céline
/// (2026-11-14), on 2026-11-19: the step opens with its group holding the focus, as N4 has it
/// today, and the first Tab goes to « Set up this device ».
#[test]
fn sq10_welcome_tab_and_shift_tab_walk_its_buttons() {
    let (mut fwd, mut back) = (setup_at(STEP_WELCOME), setup_at(STEP_WELCOME));
    assert_tab_order(&mut fwd, &mut back, &[SET_UP.to_string(), PAIR.to_string()], "the welcome step");
}

/// SQ10: Enter presses the focused button.
#[test]
fn sq10_welcome_enter_presses_the_focused_button() {
    let mut app = setup_at(STEP_WELCOME);
    tab_to(&mut app, PAIR);
    enter(&mut app);
    assert!(app.wizard.is_none() && app.auth_form.joining, "Enter on « {PAIR} » opens the join form");
}

/// SQ10: Space presses the focused button.
#[test]
fn sq10_welcome_space_presses_the_focused_button() {
    let mut app = setup_at(STEP_WELCOME);
    tab_to(&mut app, SET_UP);
    space(&mut app);
    assert_eq!(step(&app), Some(STEP_NICKNAME), "Space on « {SET_UP} » goes on");
    let mut app = setup_at(STEP_WELCOME);
    tab_to(&mut app, PAIR);
    space(&mut app);
    assert!(app.wizard.is_none() && app.auth_form.joining, "Space on « {PAIR} » opens the join form");
}

/// Enter with no button focused keeps today's meaning on the welcome step: the next step (as « Set
/// up this device »). Decided by the Fable maille, by delegation of Céline (2026-11-14), on
/// 2026-11-19 (the focus starts on the step's group, N4).
#[test]
fn sq10_welcome_enter_before_any_tab_keeps_its_meaning() {
    let mut app = setup_at(STEP_WELCOME);
    enter(&mut app);
    assert_eq!(step(&app), Some(STEP_NICKNAME), "Enter goes on, as today");
}

/// SQ10: Escape on the welcome step does what it does today: nothing (there is no step before).
#[test]
fn sq10_welcome_escape_does_what_it_does_today() {
    let mut app = setup_at(STEP_WELCOME);
    tab_to(&mut app, PAIR);
    escape(&mut app);
    assert_eq!(step(&app), Some(STEP_WELCOME), "the setup stays on its welcome");
    assert!(!app.auth_form.joining, "Escape pressed nothing");
}

/// SQ8: a mouse click on each welcome button does what it does today.
#[test]
fn sq8_welcome_clicks_are_unchanged() {
    let mut app = setup_at(STEP_WELCOME);
    mouse_on_wizard(&mut app, |a| a == WizAction::Next);
    assert_eq!(step(&app), Some(STEP_NICKNAME));
    let mut app = setup_at(STEP_WELCOME);
    mouse_on_wizard(&mut app, |a| a == WizAction::ChoosePair);
    assert!(app.wizard.is_none() && app.auth_form.joining);
}

// ======================================================================= the setup's bubble-style choices

/// SQ10: each bubble style is a choice, named as the tree names it today (N5).
#[test]
fn sq10_bubble_choices_are_named_as_today() {
    let mut app = setup_at(STEP_BUBBLE);
    let chosen = app.wizard.as_ref().unwrap().bubble;
    let t = Tree::of(&mut app);
    for st in BubbleStyle::all() {
        t.one(&CHOICES, &bubble_name(*st, chosen));
    }
}

/// SQ10: each choice, and the step's Back and Next, take a screen reader's Click and Focus.
/// Back and Next are in each step's cycle (spec v2.0.135, SQ10's lot B).
#[test]
fn sq10_bubble_controls_take_click_and_focus() {
    let mut app = setup_at(STEP_BUBBLE);
    let chosen = app.wizard.as_ref().unwrap().bubble;
    let t = Tree::of(&mut app);
    for st in BubbleStyle::all() {
        let id = t.one(&CHOICES, &bubble_name(*st, chosen));
        assert_clickable_and_focusable(&t, id, "the bubble-style step");
    }
    for name in [BACK, NEXT] {
        let id = t.one(&BUTTONS, name);
        assert_clickable_and_focusable(&t, id, "the bubble-style step");
    }
}

/// SQ10, SQ5: a screen reader's Click on a style chooses it and stays on the step, as a click.
#[test]
fn sq10_bubble_choice_by_screen_reader_chooses_it() {
    let mut app = setup_at(STEP_BUBBLE);
    let chosen = app.wizard.as_ref().unwrap().bubble;
    let other = *BubbleStyle::all().iter().find(|s| **s != chosen).unwrap();
    sr_click(&mut app, &CHOICES, &bubble_name(other, chosen));
    let w = app.wizard.as_ref().unwrap();
    assert_eq!(w.bubble, other, "the style is chosen, as a click does");
    assert_eq!(w.step, STEP_BUBBLE, "and the step stays, as a click does");
    Tree::of(&mut app).one(&CHOICES, &bubble_name(other, other));
}

/// SQ10, SQ5: a screen reader's Click on Back and on Next does what their click does.
#[test]
fn sq10_bubble_back_and_next_by_screen_reader() {
    let mut app = setup_at(STEP_BUBBLE);
    sr_click(&mut app, &BUTTONS, NEXT);
    assert_eq!(step(&app), Some(STEP_THEME));
    let mut app = setup_at(STEP_BUBBLE);
    sr_click(&mut app, &BUTTONS, BACK);
    assert_eq!(step(&app), Some(STEP_NICKNAME));
}

/// SQ10: a screen reader's Focus on a choice gives it the focus and chooses nothing.
#[test]
fn sq10_bubble_choice_takes_the_focus_from_a_screen_reader() {
    let mut app = setup_at(STEP_BUBBLE);
    let chosen = app.wizard.as_ref().unwrap().bubble;
    let other = *BubbleStyle::all().iter().rev().find(|s| **s != chosen).unwrap();
    let name = bubble_name(other, chosen);
    assert_eq!(sr_focus(&mut app, &CHOICES, &name), name);
    assert_eq!(app.wizard.as_ref().unwrap().bubble, chosen, "a Focus chooses nothing");
}

/// SQ10: the styles are one stop for Tab, entered on the checked style; the next Tab leaves the
/// group, to Back, then Next, and comes back; Shift+Tab walks the other way (the Fable maille, by
/// delegation of Céline (2026-11-14), on 2026-11-19, amended: a group of choices takes the arrows).
#[test]
fn sq10_bubble_the_styles_are_one_tab_stop_entered_on_the_checked_one() {
    let checked = BubbleStyle::all()[4];
    let open = || {
        let mut app = setup_at(STEP_BUBBLE);
        app.wizard.as_mut().unwrap().bubble = checked;
        app
    };
    let want = vec![bubble_name(checked, checked), BACK.to_string(), NEXT.to_string()];
    let (mut fwd, mut back) = (open(), open());
    assert_tab_order(&mut fwd, &mut back, &want, "the bubble-style step");
    assert_eq!(step(&fwd), Some(STEP_BUBBLE), "walking presses nothing");
    assert_eq!(fwd.wizard.as_ref().unwrap().bubble, checked, "and picks nothing");
}

/// SQ10: inside the group, the arrows are a radio group's (WAI-ARIA): Right and Down go to the next
/// style in the tree's order, Left and Up to the previous, wrapping at both ends; they move the
/// focus and pick nothing (the Fable maille, by delegation of Céline (2026-11-14), on 2026-11-19).
/// A Tab leaves the group, and Shift+Tab comes back in on the checked style, not on the one the
/// arrows left.
#[test]
fn sq10_bubble_the_arrows_move_inside_the_group_and_pick_nothing() {
    let all = BubbleStyle::all();
    let n = all.len();
    let checked = all[4];
    let mut app = setup_at(STEP_BUBBLE);
    app.wizard.as_mut().unwrap().bubble = checked;
    let name = |i: usize| bubble_name(all[i], checked);
    let focused = |app: &mut App| Tree::of(app).focused_name();
    tab(&mut app);
    assert_eq!(focused(&mut app), name(4), "Tab enters on the checked style");
    arrow(&mut app, NamedKey::ArrowRight);
    assert_eq!(focused(&mut app), name(5), "Right: the next style");
    arrow(&mut app, NamedKey::ArrowDown);
    assert_eq!(focused(&mut app), name(6), "Down: the next style");
    arrow(&mut app, NamedKey::ArrowLeft);
    assert_eq!(focused(&mut app), name(5), "Left: the previous style");
    arrow(&mut app, NamedKey::ArrowUp);
    assert_eq!(focused(&mut app), name(4), "Up: the previous style");
    // To the last, then past it: the first.
    for _ in 4..n - 1 {
        arrow(&mut app, NamedKey::ArrowRight);
    }
    assert_eq!(focused(&mut app), name(n - 1), "Right walks to the last style");
    arrow(&mut app, NamedKey::ArrowRight);
    assert_eq!(focused(&mut app), name(0), "Right on the last wraps to the first");
    arrow(&mut app, NamedKey::ArrowLeft);
    assert_eq!(focused(&mut app), name(n - 1), "Left on the first wraps to the last");
    arrow(&mut app, NamedKey::ArrowDown);
    assert_eq!(focused(&mut app), name(0), "Down on the last wraps to the first");
    arrow(&mut app, NamedKey::ArrowUp);
    assert_eq!(focused(&mut app), name(n - 1), "Up on the first wraps to the last");
    assert_eq!(app.wizard.as_ref().unwrap().bubble, checked, "the arrows pick nothing");
    assert_eq!(step(&app), Some(STEP_BUBBLE));
    tab(&mut app);
    assert_eq!(focused(&mut app), BACK, "the next Tab leaves the group");
    shift_tab(&mut app);
    assert_eq!(focused(&mut app), name(4), "Shift+Tab comes back in on the checked style");
}

/// SQ10: the themes take the same arrows: Right and Down to the next, Left and Up to the previous,
/// wrapping at both ends (two choices: each arrow goes to the other one), picking nothing.
#[test]
fn sq10_theme_the_arrows_move_inside_the_group_and_pick_nothing() {
    let mut app = setup_at(STEP_THEME);
    let light = theme_name(LIGHT, false, false);
    let focused = |app: &mut App| Tree::of(app).focused_name();
    tab(&mut app);
    assert_eq!(focused(&mut app), light, "Tab enters on the checked theme, Light");
    arrow(&mut app, NamedKey::ArrowRight);
    assert_eq!(focused(&mut app), DARK, "Right on the last wraps to the first, Dark");
    arrow(&mut app, NamedKey::ArrowRight);
    assert_eq!(focused(&mut app), light, "Right: the next, Light");
    arrow(&mut app, NamedKey::ArrowDown);
    assert_eq!(focused(&mut app), DARK, "Down on the last wraps to the first");
    arrow(&mut app, NamedKey::ArrowUp);
    assert_eq!(focused(&mut app), light, "Up on the first wraps to the last");
    arrow(&mut app, NamedKey::ArrowUp);
    assert_eq!(focused(&mut app), DARK, "Up: the previous");
    arrow(&mut app, NamedKey::ArrowLeft);
    assert_eq!(focused(&mut app), light, "Left on the first wraps to the last");
    arrow(&mut app, NamedKey::ArrowLeft);
    assert_eq!(focused(&mut app), DARK, "Left: the previous");
    assert!(!app.wizard.as_ref().unwrap().theme_dark, "the arrows pick nothing");
    assert_eq!(step(&app), Some(STEP_THEME));
}

/// SQ10, SQ5: Space and Enter pick the style the arrows reached, and stay on the step, as a click
/// does (the Fable maille, by delegation of Céline (2026-11-14), on 2026-11-19).
#[test]
fn sq10_bubble_space_and_enter_pick_the_focused_style() {
    let all = BubbleStyle::all();
    let mut app = setup_at(STEP_BUBBLE);
    app.wizard.as_mut().unwrap().bubble = all[0];
    tab(&mut app);
    arrow(&mut app, NamedKey::ArrowRight);
    space(&mut app);
    assert_eq!(app.wizard.as_ref().unwrap().bubble, all[1], "Space picks the focused style");
    assert_eq!(Tree::of(&mut app).focused_name(), bubble_name(all[1], all[1]), "and the focus stays on it");
    arrow(&mut app, NamedKey::ArrowRight);
    enter(&mut app);
    let w = app.wizard.as_ref().unwrap();
    assert_eq!(w.bubble, all[2], "Enter picks the focused style");
    assert_eq!(w.step, STEP_BUBBLE, "and stays on the step, as a click does");
    tab(&mut app);
    shift_tab(&mut app);
    assert_eq!(Tree::of(&mut app).focused_name(), bubble_name(all[2], all[2]), "the group is entered on the newly checked style");
}

/// SQ10: Enter on the focused Next goes on; on the focused Back goes back.
#[test]
fn sq10_bubble_enter_presses_back_and_next() {
    let mut app = setup_at(STEP_BUBBLE);
    tab_to(&mut app, NEXT);
    enter(&mut app);
    assert_eq!(step(&app), Some(STEP_THEME));
    let mut app = setup_at(STEP_BUBBLE);
    tab_to(&mut app, BACK);
    enter(&mut app);
    assert_eq!(step(&app), Some(STEP_NICKNAME));
}

/// Enter before any Tab keeps today's meaning (Next), and Escape keeps its own (Back), even with a
/// choice focused. Decided by the Fable maille, by delegation of Céline (2026-11-14), on 2026-11-19
/// (the focus starts on the step's group, N4).
#[test]
fn sq10_bubble_enter_and_escape_keep_their_meaning() {
    let mut app = setup_at(STEP_BUBBLE);
    enter(&mut app);
    assert_eq!(step(&app), Some(STEP_THEME), "Enter goes on, as today");
    let mut app = setup_at(STEP_BUBBLE);
    tab(&mut app);
    escape(&mut app);
    assert_eq!(step(&app), Some(STEP_NICKNAME), "Escape goes back, as today");
}

/// SQ8: a click on a style chooses it and stays; Back and Next as today.
#[test]
fn sq8_bubble_clicks_are_unchanged() {
    let mut app = setup_at(STEP_BUBBLE);
    let chosen = app.wizard.as_ref().unwrap().bubble;
    let other = *BubbleStyle::all().iter().find(|s| **s != chosen).unwrap();
    mouse_on_wizard(&mut app, |a| a == WizAction::SelectBubble(other));
    assert_eq!(app.wizard.as_ref().unwrap().bubble, other);
    assert_eq!(step(&app), Some(STEP_BUBBLE));
    mouse_on_wizard(&mut app, |a| a == WizAction::Next);
    assert_eq!(step(&app), Some(STEP_THEME));
    mouse_on_wizard(&mut app, |a| a == WizAction::Back);
    assert_eq!(step(&app), Some(STEP_BUBBLE));
}

// ======================================================================= the setup's theme choices

/// SQ10: Dark and Light are choices, named as the tree names them today (N5), each taking a
/// screen reader's Click and Focus; Back and Next too (in each step's cycle).
#[test]
fn sq10_theme_choices_are_named_and_take_click_and_focus() {
    let mut app = setup_at(STEP_THEME);
    let t = Tree::of(&mut app);
    for (name, dark) in [(DARK, true), (LIGHT, false)] {
        let id = t.one(&CHOICES, &theme_name(name, dark, false));
        assert_clickable_and_focusable(&t, id, "the theme step");
    }
    for name in [BACK, NEXT] {
        let id = t.one(&BUTTONS, name);
        assert_clickable_and_focusable(&t, id, "the theme step");
    }
}

/// SQ10, SQ5: a screen reader's Click on Dark chooses it, as a click does: the setup's theme and
/// the shell's.
#[test]
fn sq10_theme_choice_by_screen_reader_chooses_it() {
    let mut app = setup_at(STEP_THEME);
    sr_click(&mut app, &CHOICES, DARK);
    assert!(app.wizard.as_ref().unwrap().theme_dark, "Dark is chosen");
    assert_eq!(app.theme_name, "dark", "and the shell's theme follows, as a click does");
    assert_eq!(step(&app), Some(STEP_THEME), "the step stays");
}

/// SQ10: the two themes are one stop for Tab, entered on the checked one, then Back and Next; the
/// arrows move between Dark and Light without picking; Space picks the focused one and stays on the
/// step (the Fable maille, by delegation of Céline (2026-11-14), on 2026-11-19, amended: a group of
/// choices takes the arrows).
#[test]
fn sq10_theme_one_tab_stop_arrows_inside_space_picks() {
    let want = vec![theme_name(LIGHT, false, false), BACK.to_string(), NEXT.to_string()];
    let (mut fwd, mut back) = (setup_at(STEP_THEME), setup_at(STEP_THEME));
    assert_tab_order(&mut fwd, &mut back, &want, "the theme step");
    let mut app = setup_at(STEP_THEME);
    tab(&mut app);
    assert_eq!(Tree::of(&mut app).focused_name(), theme_name(LIGHT, false, false), "Tab enters on the checked theme");
    arrow(&mut app, NamedKey::ArrowLeft);
    assert_eq!(Tree::of(&mut app).focused_name(), DARK, "Left: Dark, drawn before Light");
    assert!(!app.wizard.as_ref().unwrap().theme_dark, "the arrows pick nothing");
    space(&mut app);
    assert!(app.wizard.as_ref().unwrap().theme_dark, "Space picks the focused theme");
    assert_eq!(app.theme_name, "dark");
    assert_eq!(step(&app), Some(STEP_THEME));
}

/// SQ8: a click on the Dark swatch chooses it, as today.
#[test]
fn sq8_theme_click_is_unchanged() {
    let mut app = setup_at(STEP_THEME);
    mouse_on_wizard(&mut app, |a| a == WizAction::SelectTheme(true));
    assert!(app.wizard.as_ref().unwrap().theme_dark);
    assert_eq!(app.theme_name, "dark");
    assert_eq!(step(&app), Some(STEP_THEME));
}

// ======================================================================= the setup's steps with fields

/// SQ10: on each step with fields, the fields take Focus, and each show/hide toggle, Back, « Skip »
/// and the primary take a screen reader's Click and Focus.
#[test]
fn sq10_setup_field_steps_controls_take_click_and_focus() {
    for st in FIELD_STEPS {
        let mut app = setup_filled(st);
        let t = Tree::of(&mut app);
        let what = format!("step {}", st + 1);
        let mut names = field_step_controls(st);
        names.sort();
        names.dedup();
        for name in names {
            let fields = t.named(&[AkRole::TextInput, AkRole::PasswordInput], &name);
            if let Some(id) = fields.first() {
                assert_focusable(&t, *id, &what);
            } else {
                for id in t.named(&BUTTONS, &name) {
                    assert_clickable_and_focusable(&t, id, &what);
                }
                assert!(!t.named(&BUTTONS, &name).is_empty(), "{what}: « {name} » is in the tree:\n{}", t.dump());
            }
        }
    }
}

/// SQ10: on a step with two masked fields, each field has its own toggle, right after it in the
/// tree (texts read between them aside), each named as the tree names the step's toggle today, two
/// in all; a screen reader's Click on either shows or hides both fields, as its click does, and
/// both then say « Hide ».
#[test]
fn sq10_setup_each_masked_field_has_its_own_toggle_right_after_it() {
    for (st, fields, show, hide) in MASKED_STEPS {
        let mut app = setup_filled(st);
        let what = format!("step {}", st + 1);
        let t = Tree::of(&mut app);
        assert_eq!(t.named(&BUTTONS, show).len(), 2, "{what}: two toggles named « {show} »:\n{}", t.dump());
        let mut toggles = Vec::new();
        for f in fields {
            let field = t.one(&SECRET_FIELD, f);
            let next = t.next_control(field).unwrap_or_else(|| panic!("{what}: a control after « {f} »"));
            assert_eq!(t.spoken(next), show, "{what}: right after « {f} », its toggle:\n{}", t.dump());
            assert_clickable_and_focusable(&t, next, &what);
            toggles.push(next);
        }
        assert_ne!(toggles[0], toggles[1], "{what}: each field its own toggle");
        act(&mut app, toggles[1], Action::Click);
        assert!(app.wizard.as_ref().unwrap().reveal, "{what}: the second toggle shows both fields");
        let t = Tree::of(&mut app);
        assert_eq!(t.named(&BUTTONS, hide).len(), 2, "{what}: both toggles then say « {hide} »:\n{}", t.dump());
        let first = t.next_control(t.one(&SECRET_FIELD, fields[0])).unwrap();
        act(&mut app, first, Action::Click);
        assert!(!app.wizard.as_ref().unwrap().reveal, "{what}: the first toggle hides both again");
    }
}

/// SQ10: the second field's toggle by keys: Tab from the second field reaches it, Space on it shows
/// both fields, and the focus stays on that toggle, now « Hide », right after the second field.
#[test]
fn sq10_setup_the_second_toggle_by_keys() {
    for (st, fields, _show, hide) in MASKED_STEPS {
        let mut app = setup_filled(st);
        let what = format!("step {}", st + 1);
        tab_to(&mut app, fields[1]);
        tab(&mut app);
        space(&mut app);
        assert!(app.wizard.as_ref().unwrap().reveal, "{what}: Space on the second toggle shows both fields");
        let t = Tree::of(&mut app);
        assert_eq!(t.focused_name(), hide, "{what}: the focus stays on the toggle:\n{}", t.dump());
        let before = t.prev_control(t.up.focus).map(|id| t.spoken(id));
        assert_eq!(before.as_deref(), Some(fields[1]), "{what}: the focused toggle is the second field's");
        space(&mut app);
        assert!(!app.wizard.as_ref().unwrap().reveal, "{what}: Space again hides them");
    }
}

/// SQ10: on each step with fields, Tab walks each field and its toggle, then Back, « Skip » and the
/// primary, in the drawn order, and comes back; Shift+Tab walks the other way. The focus starts on
/// the first field, as A5 has it. Today Tab goes between the two fields only.
#[test]
fn sq10_setup_field_steps_tab_walks_fields_toggle_and_buttons_in_the_drawn_order() {
    for st in FIELD_STEPS {
        let (mut fwd, mut back) = (setup_filled(st), setup_filled(st));
        assert_eq!(Tree::of(&mut fwd).focused_name(), field_step_controls(st)[0], "A5: the first field holds the focus");
        assert_tab_order(&mut fwd, &mut back, &field_step_controls(st), &format!("step {}", st + 1));
        assert_eq!(step(&fwd), Some(st), "walking presses nothing");
    }
}

/// SQ10: what is typed goes to the field that holds the focus, and nowhere once a button holds it.
#[test]
fn sq10_setup_typing_goes_to_the_focused_field_only() {
    let mut app = setup_at(STEP_NICKNAME);
    let device = app.wizard.as_ref().unwrap().device_name.clone();
    type_text(&mut app, "Ad");
    assert_eq!(app.wizard.as_ref().unwrap().nickname, "Ad", "typing reaches the first field");
    tab(&mut app);
    type_text(&mut app, "x");
    assert_eq!(app.wizard.as_ref().unwrap().device_name, format!("{device}x"), "then the second");
    tab_to(&mut app, BACK);
    type_text(&mut app, "y");
    let w = app.wizard.as_ref().unwrap();
    assert_eq!((w.nickname.as_str(), w.step), ("Ad", STEP_NICKNAME), "a button holding the focus takes no text");
    assert_eq!(w.device_name, format!("{device}x"));
}

/// Enter in a field keeps today's meaning: the next field, then the primary (proposed, to be
/// confirmed, as lot C's forms keep theirs).
#[test]
fn sq10_setup_enter_in_a_field_keeps_its_meaning() {
    let mut app = setup_filled(STEP_PASSWORD);
    enter(&mut app);
    assert_eq!(Tree::of(&mut app).focused_name(), CONFIRM_PASSWORD, "Enter in the first field: the second");
    enter(&mut app);
    assert_eq!(step(&app), Some(STEP_DURESS), "Enter in the second field: Next");
}

/// SQ10: Enter or Space press the focused button: the toggle shows and hides the passwords,
/// « Skip » skips, Back goes back, the primary goes on.
#[test]
fn sq10_setup_enter_and_space_press_the_focused_button() {
    let mut app = setup_filled(STEP_PASSWORD);
    tab_to(&mut app, SHOW_PASSWORDS);
    enter(&mut app);
    assert!(app.wizard.as_ref().unwrap().reveal, "Enter on the toggle shows the passwords");
    space(&mut app);
    assert!(!app.wizard.as_ref().unwrap().reveal, "Space on it hides them");
    let mut app = setup_filled(STEP_DURESS);
    tab_to(&mut app, SKIP);
    space(&mut app);
    let w = app.wizard.as_ref().unwrap();
    assert_eq!(w.step, STEP_CANARY, "Space on « Skip » skips the step");
    assert!(w.duress.is_empty() && w.duress_confirm.is_empty(), "and clears it, as a click does");
    let mut app = setup_filled(STEP_NICKNAME);
    tab_to(&mut app, BACK);
    enter(&mut app);
    assert_eq!(step(&app), Some(STEP_WELCOME), "Enter on Back goes back");
    let mut app = setup_filled(STEP_NICKNAME);
    tab_to(&mut app, NEXT);
    space(&mut app);
    assert_eq!(step(&app), Some(STEP_BUBBLE), "Space on Next goes on");
}

/// SQ10, SQ5: a screen reader's Click on Back, « Skip » and Next does what their click does; on
/// a Next that a click cannot press (the password step, its password not yet valid), it does
/// nothing either.
#[test]
fn sq10_setup_buttons_by_screen_reader_go_through_the_same_check() {
    let mut app = setup_filled(STEP_DURESS);
    sr_click(&mut app, &BUTTONS, SKIP);
    assert_eq!(step(&app), Some(STEP_CANARY));
    assert!(app.wizard.as_ref().unwrap().duress.is_empty(), "« Skip » clears the step, as a click does");
    let mut app = setup_filled(STEP_NICKNAME);
    sr_click(&mut app, &BUTTONS, NEXT);
    assert_eq!(step(&app), Some(STEP_BUBBLE));
    let mut app = setup_filled(STEP_PASSWORD);
    sr_click(&mut app, &BUTTONS, BACK);
    assert_eq!(step(&app), Some(STEP_SAMPLE));
    let mut app = setup_at(STEP_PASSWORD);
    sr_click(&mut app, &BUTTONS, NEXT);
    assert_eq!(step(&app), Some(STEP_PASSWORD), "a Next a click cannot press, a screen reader cannot either");
}

/// SQ10: Escape on a step with fields does what it does today, wherever the focus is: Back.
#[test]
fn sq10_setup_escape_is_back_as_today() {
    let mut app = setup_filled(STEP_CANARY);
    tab_to(&mut app, SHOW_PHRASE);
    escape(&mut app);
    assert_eq!(step(&app), Some(STEP_DURESS));
}

/// SQ8: on the steps with fields, a click on the second field, the toggle, « Skip » and Back does
/// what it does today.
#[test]
fn sq8_setup_field_step_clicks_are_unchanged() {
    let mut app = setup_filled(STEP_DURESS);
    mouse_on_wizard(&mut app, |a| a == WizAction::FocusField(1));
    assert_eq!(app.wizard.as_ref().unwrap().focus, 1);
    mouse_on_wizard(&mut app, |a| a == WizAction::ToggleReveal);
    assert!(app.wizard.as_ref().unwrap().reveal);
    mouse_on_wizard(&mut app, |a| a == WizAction::Skip);
    assert_eq!(step(&app), Some(STEP_CANARY));
    mouse_on_wizard(&mut app, |a| a == WizAction::Back);
    assert_eq!(step(&app), Some(STEP_DURESS));
}

// ======================================================================= the sample-data step

/// SQ10: the checkbox is in the tree as it is today (N5), taking a screen reader's Click and Focus;
/// its Click turns the sample data off, as a click does, and its name says so.
#[test]
fn sq10_sample_checkbox_takes_click_and_focus() {
    let mut app = setup_at(STEP_SAMPLE);
    let t = Tree::of(&mut app);
    let id = t.one(&[AkRole::CheckBox], SAMPLE_ON);
    assert_clickable_and_focusable(&t, id, "the sample-data step");
    for name in [BACK, NEXT] {
        let id = t.one(&BUTTONS, name);
        assert_clickable_and_focusable(&t, id, "the sample-data step");
    }
    sr_click(&mut app, &[AkRole::CheckBox], SAMPLE_ON);
    assert!(!app.wizard.as_ref().unwrap().seed_sample, "the sample data is off, as a click does");
    Tree::of(&mut app).one(&[AkRole::CheckBox], SAMPLE_OFF);
    assert_eq!(step(&app), Some(STEP_SAMPLE));
}

/// SQ10: Tab walks the checkbox, Back and Next in the drawn order; Space and Enter on the checkbox
/// turn it, and stay on the step; Enter before any Tab keeps today's meaning (Next). Today Tab does
/// nothing on this step.
#[test]
fn sq10_sample_keys() {
    let want = vec![SAMPLE_ON.to_string(), BACK.to_string(), NEXT.to_string()];
    let (mut fwd, mut back) = (setup_at(STEP_SAMPLE), setup_at(STEP_SAMPLE));
    assert_tab_order(&mut fwd, &mut back, &want, "the sample-data step");
    let mut app = setup_at(STEP_SAMPLE);
    tab(&mut app);
    space(&mut app);
    assert!(!app.wizard.as_ref().unwrap().seed_sample, "Space turns the checkbox");
    enter(&mut app);
    assert!(app.wizard.as_ref().unwrap().seed_sample, "Enter turns it back");
    assert_eq!(step(&app), Some(STEP_SAMPLE), "and the step stays");
    let mut app = setup_at(STEP_SAMPLE);
    enter(&mut app);
    assert_eq!(step(&app), Some(STEP_PASSWORD), "Enter before any Tab: Next, as today");
}

/// SQ8: a click on the checkbox turns it, as today.
#[test]
fn sq8_sample_click_is_unchanged() {
    let mut app = setup_at(STEP_SAMPLE);
    mouse_on_wizard(&mut app, |a| a == WizAction::ToggleSample);
    assert!(!app.wizard.as_ref().unwrap().seed_sample);
    assert_eq!(step(&app), Some(STEP_SAMPLE));
}

// ======================================================================= the whole setup, by keys alone

fn module() -> &'static str {
    module_path!()
}

/// The launches these tests run in a child process, on a profile of their own.
#[test]
#[ignore = "a launch, run by its parent test in a child process on a profile of its own"]
fn child_entry() {
    let Ok(scenario) = std::env::var(CHILD_VAR) else { return };
    match scenario.as_str() {
        "whole_setup_by_keys" => whole_setup_by_keys(),
        other => panic!("unknown scenario {other}"),
    }
}

/// SQ10's lot B, above every other test: the whole setup, from the welcome screen to the
/// workspace, by keys alone (spec v2.0.135), on a fresh profile in a child process: no click, no
/// screen reader, only Tab, Shift+Tab, the arrows, Enter, Space and typed text, through the shell's
/// key path (`key_with`).
#[test]
fn sq10_the_whole_setup_by_keys_alone() {
    let p = profile();
    launch(module(), "whole_setup_by_keys", p.path());
}

fn whole_setup_by_keys() {
    let mut app = App::new();
    first_frame(&mut app);
    assert!(app.locked && step(&app) == Some(STEP_WELCOME), "a fresh profile opens on the welcome step");
    let at = |app: &App, want: usize, what: &str| {
        assert_eq!(step(app), Some(want), "{what}: {:?}", app.wizard.as_ref().and_then(|w| w.error.clone()));
    };

    // Welcome: Tab to « Set up this device », Enter.
    tab(&mut app);
    assert_eq!(Tree::of(&mut app).focused_name(), SET_UP);
    enter(&mut app);
    at(&app, STEP_NICKNAME, "the nickname step");

    // The nickname: typed; the device's name left as proposed; Tab to Next, Space.
    type_text(&mut app, "Ada");
    tab_to(&mut app, NEXT);
    space(&mut app);
    at(&app, STEP_BUBBLE, "the bubble-style step");
    assert_eq!(app.wizard.as_ref().unwrap().nickname, "Ada");

    // The bubble style: into the group, Right, Space picks; Tab out to Next, Enter.
    let first = app.wizard.as_ref().unwrap().bubble;
    tab(&mut app);
    arrow(&mut app, NamedKey::ArrowRight);
    space(&mut app);
    let picked = app.wizard.as_ref().unwrap().bubble;
    assert_ne!(picked, first, "a style was picked by keys");
    tab_to(&mut app, NEXT);
    enter(&mut app);
    at(&app, STEP_THEME, "the theme step");

    // The theme: kept as checked; Tab to Next, Enter.
    tab_to(&mut app, NEXT);
    enter(&mut app);
    at(&app, STEP_SAMPLE, "the sample-data step");

    // The sample data: turned off by Space on the checkbox; Next.
    tab(&mut app);
    space(&mut app);
    assert!(!app.wizard.as_ref().unwrap().seed_sample);
    tab_to(&mut app, NEXT);
    enter(&mut app);
    at(&app, STEP_PASSWORD, "the password step");

    // The password: typed twice (the second field after the first one's toggle); shown and hidden
    // by the first toggle; Next.
    type_text(&mut app, PRIMARY);
    tab_to(&mut app, CONFIRM_PASSWORD);
    type_text(&mut app, PRIMARY);
    tab_to(&mut app, SHOW_PASSWORDS);
    space(&mut app);
    space(&mut app);
    tab_to(&mut app, NEXT);
    enter(&mut app);
    at(&app, STEP_DURESS, "the duress step");

    // Duress: typed twice; Next.
    type_text(&mut app, DURESS);
    tab_to(&mut app, CONFIRM_DURESS);
    type_text(&mut app, DURESS);
    tab_to(&mut app, NEXT);
    enter(&mut app);
    at(&app, STEP_CANARY, "the canary step");

    // The canary phrase: typed twice; « Get started ».
    type_text(&mut app, SETUP_CANARY);
    tab_to(&mut app, CONFIRM_PHRASE);
    type_text(&mut app, SETUP_CANARY);
    tab_to(&mut app, GET_STARTED);
    enter(&mut app);

    assert!(frames_until(&mut app, 240, setup_settled), "the setup ends");
    assert!(!app.locked, "the workspace opens: {:?}", app.wizard.as_ref().and_then(|w| w.error.clone()));
    assert_eq!(app.bubble_style, picked, "the style picked by keys is the workspace's");
    frames_until(&mut app, 1, |_| false);
    shut_runtime(&mut app);
    std::mem::forget(app);
}

// ======================================================================= the recovery wizard, while locked

/// The controls each phase draws (`draw_recovery_wizard`), buttons only.
fn rec_buttons(phase: RecoveryPhase) -> &'static [&'static str] {
    match phase {
        RecoveryPhase::SetPassword => &[CANCEL, START_RECOVERY],
        RecoveryPhase::Awaiting => &[CHECK_NOW, CANCEL, CLOSE],
        RecoveryPhase::Ready => &[CANCEL, RECOVER],
        RecoveryPhase::Failed => &[CLOSE, START_OVER],
    }
}

fn rec_title(phase: RecoveryPhase) -> &'static str {
    match phase {
        RecoveryPhase::SetPassword => REC_SET_TITLE,
        RecoveryPhase::Awaiting => REC_AWAIT_TITLE,
        RecoveryPhase::Ready => REC_READY_TITLE,
        RecoveryPhase::Failed => REC_FAILED_TITLE,
    }
}

const PHASES: [RecoveryPhase; 4] =
    [RecoveryPhase::SetPassword, RecoveryPhase::Awaiting, RecoveryPhase::Ready, RecoveryPhase::Failed];

/// The show/hide toggle of the new password: a button of the wizard's dialog whose name begins
/// « show » or « hide » (either case; the login form's own toggle, under the wizard, is not it).
/// Either name passes: « Show password » / « Hide password », as L4 names the login form's, or
/// « show » / « hide », as drawn (the Fable maille, by delegation of Céline (2026-11-14), on
/// 2026-11-19).
fn reveal_toggle(t: &Tree, shown: bool) -> NodeId {
    let want = if shown { "hide" } else { "show" };
    let dialog = t.dialog(REC_SET_TITLE);
    let v: Vec<NodeId> = t
        .up
        .nodes
        .iter()
        .filter(|(id, n)| {
            BUTTONS.contains(&n.role())
                && t.reachable(*id)
                && t.within(dialog, *id)
                && t.spoken(*id).to_lowercase().starts_with(want)
        })
        .map(|(id, _)| *id)
        .collect();
    assert_eq!(v.len(), 1, "one show/hide toggle saying « {want} »:\n{}", t.dump());
    v[0]
}

/// SQ10: in every phase, the wizard is a dialog named by its drawn title, holding its buttons
/// named as drawn, each taking a screen reader's Click and Focus; while it is open, the focus is
/// in it (the login form under it takes no click).
#[test]
fn sq10_recovery_wizard_is_a_dialog_with_its_buttons_in_every_phase() {
    for phase in PHASES {
        let mut app = recovery_at(phase);
        let t = Tree::of(&mut app);
        let dialog = t.dialog(rec_title(phase));
        for name in rec_buttons(phase) {
            let id = t.one(&BUTTONS, name);
            assert!(t.within(dialog, id), "« {name} » is in the wizard's dialog:\n{}", t.dump());
            assert_clickable_and_focusable(&t, id, rec_title(phase));
        }
        assert!(t.within(dialog, t.up.focus), "the focus is in the wizard ({}):\n{}", rec_title(phase), t.dump());
    }
}

/// SQ10: a resumed recovery's dialog is named by its own drawn title.
#[test]
fn sq10_recovery_wizard_resumed_is_named_as_drawn() {
    let mut app = recovery_at(RecoveryPhase::SetPassword);
    app.recovery_wizard.as_mut().unwrap().resuming = true;
    Tree::of(&mut app).dialog(REC_RESUME_TITLE);
}

/// SQ10: the new password is a secret field named by its drawn label, taking Focus, with its
/// show/hide toggle; the password is never in the tree, « show » on or off: its value says its
/// length only, as the setup's secret fields do (A3, L5; the « N characters » form: the Fable
/// maille, by delegation of Céline (2026-11-14), on 2026-11-19).
#[test]
fn sq10_recovery_field_is_a_secret_field_named_as_drawn() {
    let secret = "Rc!Secret-Pass9";
    for reveal in [false, true] {
        let mut app = recovery_at(RecoveryPhase::SetPassword);
        {
            let w = app.recovery_wizard.as_mut().unwrap();
            w.new_password = secret.into();
            w.reveal = reveal;
        }
        let t = Tree::of(&mut app);
        let dialog = t.dialog(REC_SET_TITLE);
        let field = t.one(&SECRET_FIELD, REC_FIELD);
        assert!(t.within(dialog, field), "the field is in the wizard");
        assert_focusable(&t, field, "the recovery wizard");
        let toggle = reveal_toggle(&t, reveal);
        assert!(t.within(dialog, toggle), "the toggle is in the wizard");
        assert_clickable_and_focusable(&t, toggle, "the recovery wizard");
        let value = t.node(field).and_then(|n| n.value()).map(str::to_string);
        assert_eq!(value.as_deref(), Some(format!("{} characters", secret.chars().count()).as_str()));
        assert!(
            !t.every_string().iter().any(|s| s.contains(secret)),
            "the new password is never in the tree (reveal {reveal})"
        );
    }
}

/// Where the focus starts in each phase. Decided by the Fable maille, by delegation of Céline
/// (2026-11-14), on 2026-11-19: where Enter lands today, so that Enter keeps its meaning (SQ3's
/// rule for the prompts): the field (Enter starts the recovery); « Check now » (Enter checks); «
/// Recover » (Enter recovers). On the failed phase Enter does nothing today: « Close », which loses
/// nothing.
#[test]
fn sq10_recovery_focus_starts_where_enter_lands_today() {
    let want = [
        (RecoveryPhase::SetPassword, REC_FIELD),
        (RecoveryPhase::Awaiting, CHECK_NOW),
        (RecoveryPhase::Ready, RECOVER),
        (RecoveryPhase::Failed, CLOSE),
    ];
    for (phase, name) in want {
        let mut app = recovery_at(phase);
        assert_eq!(Tree::of(&mut app).focused_name(), name, "{}", rec_title(phase));
    }
}

/// SQ10: Tab walks each phase's controls in the drawn order, top to bottom and left to right on a
/// row, inside the wizard, and comes back; Shift+Tab walks the other way (the Fable maille, by
/// delegation of Céline (2026-11-14), on 2026-11-19). The first phase: the field, its show/hide
/// toggle (drawn at the field's right end), « Cancel », « Start recovery »; the waiting phase: «
/// Check now », above « Cancel » and « Close ».
#[test]
fn sq10_recovery_tab_walks_its_controls_in_the_drawn_order() {
    for phase in PHASES {
        let mut want: Vec<String> = Vec::new();
        if phase == RecoveryPhase::SetPassword {
            let mut app = recovery_at(phase);
            let t = Tree::of(&mut app);
            want.push(REC_FIELD.into());
            want.push(t.spoken(reveal_toggle(&t, false)));
        }
        want.extend(rec_buttons(phase).iter().map(|s| s.to_string()));
        let (mut fwd, mut back) = (recovery_at(phase), recovery_at(phase));
        assert_tab_order(&mut fwd, &mut back, &want, rec_title(phase));
        assert!(rec_phase(&fwd) == Some(phase), "walking presses nothing");
    }
}

/// SQ10: in every phase, one line of the wizard says its keys: Tab, Enter, Space, Escape.
#[test]
fn sq10_recovery_wizard_says_its_keys() {
    for phase in PHASES {
        let mut app = recovery_at(phase);
        let t = Tree::of(&mut app);
        let dialog = t.dialog(rec_title(phase));
        assert_keys_line(&t, dialog, &["Tab", "Enter", "Space", "Escape"], rec_title(phase));
    }
}

/// SQ10, SQ5: on the first phase, a screen reader's Click on each control does what its click
/// does: the toggle shows the password, « Start recovery » with a weak password says so (and
/// contacts no one), « Cancel » closes the wizard.
#[test]
fn sq10_recovery_set_password_controls_by_screen_reader() {
    let mut app = recovery_at(RecoveryPhase::SetPassword);
    let toggle = {
        let t = Tree::of(&mut app);
        reveal_toggle(&t, false)
    };
    act(&mut app, toggle, Action::Click);
    assert!(app.recovery_wizard.as_ref().unwrap().reveal, "the toggle shows the password");

    app.recovery_wizard.as_mut().unwrap().new_password = WEAK.into();
    sr_click(&mut app, &BUTTONS, START_RECOVERY);
    let w = app.recovery_wizard.as_ref().unwrap();
    assert_eq!(w.error.as_deref(), Some(TOO_WEAK), "a weak password is refused, as a click does");
    assert!(w.phase == RecoveryPhase::SetPassword);

    sr_click(&mut app, &BUTTONS, CANCEL);
    assert!(app.recovery_wizard.is_none(), "Cancel closes the wizard, as a click does");
    assert!(app.locked, "the gate stays locked");
}

/// SQ10, SQ5: on the waiting phase: « Check now » starts a check, « Close » closes the wizard.
#[test]
fn sq10_recovery_awaiting_controls_by_screen_reader() {
    let mut app = recovery_at(RecoveryPhase::Awaiting);
    sr_click(&mut app, &BUTTONS, CHECK_NOW);
    assert!(app.recovery_wizard.as_ref().unwrap().polling, "a check starts, as a click does");
    let mut app = recovery_at(RecoveryPhase::Awaiting);
    sr_click(&mut app, &BUTTONS, CLOSE);
    assert!(app.recovery_wizard.is_none(), "Close closes the wizard");
}

/// SQ10, SQ5: on the ready phase, « Recover » goes through the same check as a click (a weak
/// password is refused there too); on the failed phase, « Start over » starts again and
/// « Close » closes.
#[test]
fn sq10_recovery_ready_and_failed_controls_by_screen_reader() {
    let mut app = recovery_at(RecoveryPhase::Ready);
    sr_click(&mut app, &BUTTONS, RECOVER);
    assert_eq!(app.recovery_wizard.as_ref().unwrap().error.as_deref(), Some(TOO_WEAK));
    assert!(app.auth_job.is_none(), "nothing was derived");
    let mut app = recovery_at(RecoveryPhase::Failed);
    sr_click(&mut app, &BUTTONS, START_OVER);
    assert!(rec_phase(&app) == Some(RecoveryPhase::SetPassword), "Start over opens the first phase again");
    let mut app = recovery_at(RecoveryPhase::Failed);
    sr_click(&mut app, &BUTTONS, CLOSE);
    assert!(app.recovery_wizard.is_none());
}

/// SQ10: a screen reader's Focus moves the focus to the field and to a button, pressing nothing.
#[test]
fn sq10_recovery_controls_take_the_focus_from_a_screen_reader() {
    let mut app = recovery_at(RecoveryPhase::SetPassword);
    assert_eq!(sr_focus(&mut app, &BUTTONS, CANCEL), CANCEL);
    assert_eq!(sr_focus(&mut app, &SECRET_FIELD, REC_FIELD), REC_FIELD);
    assert!(app.recovery_wizard.is_some(), "a Focus presses nothing");
}

/// SQ10: typing goes to the field while it holds the focus, and not once a button holds it; Space
/// on the focused « Cancel » presses it.
#[test]
fn sq10_recovery_keys_type_into_the_field_and_press_the_focused_button() {
    let mut app = recovery_at(RecoveryPhase::SetPassword);
    type_text(&mut app, "ab");
    assert_eq!(app.recovery_wizard.as_ref().unwrap().new_password, "ab", "typing reaches the field");
    tab_to(&mut app, CANCEL);
    type_text(&mut app, "c");
    assert_eq!(app.recovery_wizard.as_ref().unwrap().new_password, "ab", "not once a button holds the focus");
    space(&mut app);
    assert!(app.recovery_wizard.is_none(), "Space presses the focused « Cancel »");
}

/// SQ10: Enter presses the focused button (here « Close » on the waiting phase).
#[test]
fn sq10_recovery_enter_presses_the_focused_button() {
    let mut app = recovery_at(RecoveryPhase::Awaiting);
    tab_to(&mut app, CLOSE);
    enter(&mut app);
    assert!(app.recovery_wizard.is_none(), "Enter on « Close » closes the wizard");
}

/// Enter where the focus starts keeps today's meaning: in the field it starts the recovery (a
/// weak password is refused); on the waiting phase it checks now.
#[test]
fn sq10_recovery_enter_keeps_its_meaning_where_the_focus_starts() {
    let mut app = recovery_at(RecoveryPhase::SetPassword);
    type_text(&mut app, WEAK);
    enter(&mut app);
    assert_eq!(app.recovery_wizard.as_ref().unwrap().error.as_deref(), Some(TOO_WEAK));
    let mut app = recovery_at(RecoveryPhase::Awaiting);
    enter(&mut app);
    assert!(app.recovery_wizard.as_ref().unwrap().polling, "Enter checks now, as today");
}

/// SQ10: Escape closes the wizard in every phase, as it does today, whatever holds the focus; the
/// gate stays locked.
#[test]
fn sq10_recovery_escape_closes_as_today() {
    for phase in PHASES {
        let mut app = recovery_at(phase);
        tab(&mut app);
        escape(&mut app);
        assert!(app.recovery_wizard.is_none(), "Escape closes « {} »", rec_title(phase));
        assert!(app.locked);
    }
}

/// SQ8: each phase's buttons do on a click what they do today; a click away does nothing (a
/// mis-click must not abandon a multi-day recovery).
#[test]
fn sq8_recovery_clicks_are_unchanged() {
    let layout = |phase| recovery_wizard_layout(W, H, phase);

    let mut app = recovery_at(RecoveryPhase::SetPassword);
    let (_, _, primary, secondary, _, reveal) = layout(RecoveryPhase::SetPassword);
    let (x, y) = centre(reveal);
    app.handle_click(x, y);
    assert!(app.recovery_wizard.as_ref().unwrap().reveal);
    app.recovery_wizard.as_mut().unwrap().new_password = WEAK.into();
    let (x, y) = centre(primary);
    app.handle_click(x, y);
    assert_eq!(app.recovery_wizard.as_ref().unwrap().error.as_deref(), Some(TOO_WEAK));
    app.handle_click(4.0, 4.0);
    assert!(app.recovery_wizard.is_some(), "a click away does nothing");
    let (x, y) = centre(secondary);
    app.handle_click(x, y);
    assert!(app.recovery_wizard.is_none());

    let mut app = recovery_at(RecoveryPhase::Awaiting);
    let (_, _, primary, _, check_now, _) = layout(RecoveryPhase::Awaiting);
    let (x, y) = centre(check_now);
    app.handle_click(x, y);
    assert!(app.recovery_wizard.as_ref().unwrap().polling);
    let (x, y) = centre(primary);
    app.handle_click(x, y);
    assert!(app.recovery_wizard.is_none());

    let mut app = recovery_at(RecoveryPhase::Failed);
    let (_, _, primary, _, _, _) = layout(RecoveryPhase::Failed);
    let (x, y) = centre(primary);
    app.handle_click(x, y);
    assert!(rec_phase(&app) == Some(RecoveryPhase::SetPassword));
}

// ======================================================================= the pairing offer

/// SQ10: the offer is a dialog named by its drawn title, holding the pairing code it draws and its
/// two buttons named as drawn, each taking a screen reader's Click and Focus; it holds the focus.
#[test]
fn sq10_pairing_offer_is_a_dialog_with_its_buttons() {
    let mut u = pairing("a11y-b-pair-tree");
    let t = Tree::of(&mut u.app);
    let dialog = t.dialog(PAIR_TITLE);
    for name in [COPY_CODE, DONE] {
        let id = t.one(&BUTTONS, name);
        assert!(t.within(dialog, id), "« {name} » is in the offer:\n{}", t.dump());
        assert_clickable_and_focusable(&t, id, "the pairing offer");
    }
    assert!(
        t.every_string().iter().any(|s| s.contains(OFFER_PIN)),
        "the pairing code it draws is heard:\n{}",
        t.dump()
    );
    assert!(t.within(dialog, t.up.focus), "the offer holds the focus:\n{}", t.dump());
}

/// Where the focus starts: « Done » (spec v2.0.135, SQ10's lot B): Enter closes the offer today,
/// so Enter keeps its meaning and a stray key copies nothing.
#[test]
fn sq10_pairing_focus_starts_on_done() {
    let mut u = pairing("a11y-b-pair-focus");
    assert_eq!(Tree::of(&mut u.app).focused_name(), DONE);
}

/// SQ10, SQ5: a screen reader's Click on « Copy code » copies the offer's code, as a click does,
/// and the button then says what it draws, « Copied! »; on « Done » it closes the offer.
#[test]
fn sq10_pairing_buttons_by_screen_reader() {
    let mut u = pairing("a11y-b-pair-sr");
    sr_click(&mut u.app, &BUTTONS, COPY_CODE);
    assert_eq!(clipboard_text().as_deref(), Some(OFFER_CODE), "the code is copied, as a click does");
    assert!(u.app.pairing_modal.as_ref().unwrap().copied);
    Tree::of(&mut u.app).one(&BUTTONS, COPIED);
    sr_click(&mut u.app, &BUTTONS, DONE);
    assert!(u.app.pairing_modal.is_none(), "Done closes the offer");
}

/// SQ10: a screen reader's Focus moves the focus between the two buttons, pressing nothing.
#[test]
fn sq10_pairing_buttons_take_the_focus_from_a_screen_reader() {
    let mut u = pairing("a11y-b-pair-srf");
    assert_eq!(sr_focus(&mut u.app, &BUTTONS, COPY_CODE), COPY_CODE);
    assert_eq!(sr_focus(&mut u.app, &BUTTONS, DONE), DONE);
    assert!(u.app.pairing_modal.as_ref().is_some_and(|m| !m.copied), "a Focus presses nothing");
}

/// SQ10: Tab and Shift+Tab move between « Copy code » and « Done », left to right as drawn, and
/// never to what is under the offer (the canvas's lane labels are in the tree).
#[test]
fn sq10_pairing_tab_walks_its_buttons_in_the_drawn_order() {
    let (mut fwd, mut back) = (pairing("a11y-b-pair-tab-f"), pairing("a11y-b-pair-tab-b"));
    assert_tab_order(&mut fwd.app, &mut back.app, &[COPY_CODE.to_string(), DONE.to_string()], "the pairing offer");
    assert!(fwd.app.pairing_modal.as_ref().is_some_and(|m| !m.copied), "walking presses nothing");
}

/// SQ10: one line of the offer says its keys: Tab, Enter, Space, Escape.
#[test]
fn sq10_pairing_offer_says_its_keys() {
    let mut u = pairing("a11y-b-pair-keys-line");
    let t = Tree::of(&mut u.app);
    let dialog = t.dialog(PAIR_TITLE);
    assert_keys_line(&t, dialog, &["Tab", "Enter", "Space", "Escape"], "the pairing offer");
}

/// SQ10, SQ5: Enter on the focused « Copy code » copies the same code a click copies, and leaves
/// the offer open; Space on « Done » closes it. Decided by the Fable maille, by delegation of
/// Céline (2026-11-14), on 2026-11-19: Enter presses the focused button (SQ10's rule), so it no
/// longer closes from « Copy code », as it does today from anywhere.
#[test]
fn sq10_pairing_enter_and_space_press_the_focused_button() {
    let mut u = pairing("a11y-b-pair-keys");
    tab_to(&mut u.app, COPY_CODE);
    enter(&mut u.app);
    assert_eq!(clipboard_text().as_deref(), Some(OFFER_CODE), "Enter copies the code");
    assert!(u.app.pairing_modal.as_ref().is_some_and(|m| m.copied), "and the offer stays");
    tab_to(&mut u.app, DONE);
    space(&mut u.app);
    assert!(u.app.pairing_modal.is_none(), "Space on Done closes the offer");
}

/// SQ10, SQ8: Escape closes the offer, as it does today, whatever holds the focus, and does
/// nothing more than today (the Fable maille, by delegation of Céline (2026-11-14), on 2026-11-19).
#[test]
fn sq10_pairing_escape_closes_as_today() {
    let mut u = pairing("a11y-b-pair-esc");
    tab_to(&mut u.app, COPY_CODE);
    escape(&mut u.app);
    assert!(u.app.pairing_modal.is_none());
}

/// SQ8: a click on « Copy code » copies, on « Done » closes, and a click away closes, as today.
#[test]
fn sq8_pairing_clicks_are_unchanged() {
    let (_, _, copy, done) = pairing_modal_layout(W, H);
    let mut u = pairing("a11y-b-pair-click");
    let (x, y) = centre(copy);
    u.app.handle_click(x, y);
    assert_eq!(clipboard_text().as_deref(), Some(OFFER_CODE));
    assert!(u.app.pairing_modal.as_ref().is_some_and(|m| m.copied));
    let (x, y) = centre(done);
    u.app.handle_click(x, y);
    assert!(u.app.pairing_modal.is_none());
    let mut u = pairing("a11y-b-pair-away");
    u.app.handle_click(4.0, 4.0);
    assert!(u.app.pairing_modal.is_none(), "a click away closes the offer");
}

// ======================================================================= the guardian enrolment

/// SQ10: the enrolment is a dialog named by its drawn title, holding the spoken code it draws and
/// « Done », which takes a screen reader's Click and Focus; it holds the focus, on « Done » (its
/// only control).
#[test]
fn sq10_guardian_enrolment_is_a_dialog_with_done() {
    let mut u = guardian("a11y-b-guardian-tree");
    let t = Tree::of(&mut u.app);
    let dialog = t.dialog(GUARDIAN_TITLE);
    let done = t.one(&BUTTONS, DONE);
    assert!(t.within(dialog, done), "Done is in the enrolment");
    assert_clickable_and_focusable(&t, done, "the guardian enrolment");
    assert!(t.every_string().iter().any(|s| s.contains("48291-30576")), "the spoken code is heard");
    assert_eq!(t.up.focus, done, "the focus is on Done:\n{}", t.dump());
}

/// SQ10, SQ5: a screen reader's Click on « Done » closes it, as a click does.
#[test]
fn sq10_guardian_done_by_screen_reader() {
    let mut u = guardian("a11y-b-guardian-sr");
    sr_click(&mut u.app, &BUTTONS, DONE);
    assert!(u.app.guardian_modal.is_none());
}

/// SQ10: Tab and Shift+Tab stay on « Done »; Enter and Space press it.
#[test]
fn sq10_guardian_keys() {
    let mut u = guardian("a11y-b-guardian-keys");
    assert_eq!(tab_cycle(&mut u.app, 4, false), vec![DONE.to_string()], "Tab stays in the enrolment");
    assert_eq!(tab_cycle(&mut u.app, 4, true), vec![DONE.to_string()]);
    assert!(u.app.guardian_modal.is_some(), "walking presses nothing");
    enter(&mut u.app);
    assert!(u.app.guardian_modal.is_none(), "Enter presses Done");
    let mut u = guardian("a11y-b-guardian-space");
    space(&mut u.app);
    assert!(u.app.guardian_modal.is_none(), "Space presses Done");
}

/// SQ10: one line of the enrolment says its keys: Enter, Space, Escape (Tab has nowhere to go
/// with « Done » alone, so the line need not name it).
#[test]
fn sq10_guardian_enrolment_says_its_keys() {
    let mut u = guardian("a11y-b-guardian-keys-line");
    let t = Tree::of(&mut u.app);
    let dialog = t.dialog(GUARDIAN_TITLE);
    assert_keys_line(&t, dialog, &["Enter", "Space", "Escape"], "the guardian enrolment");
}

/// SQ10: Escape closes the enrolment, as it does today (the Fable maille, by delegation of Céline
/// (2026-11-14), on 2026-11-19).
#[test]
fn sq10_guardian_escape_closes_as_today() {
    let mut u = guardian("a11y-b-guardian-esc");
    escape(&mut u.app);
    assert!(u.app.guardian_modal.is_none());
}

/// SQ8: a click on « Done », or away, closes it, as today.
#[test]
fn sq8_guardian_clicks_are_unchanged() {
    let (_, _, done) = guardian_modal_layout(W, H);
    let mut u = guardian("a11y-b-guardian-click");
    let (x, y) = centre(done);
    u.app.handle_click(x, y);
    assert!(u.app.guardian_modal.is_none());
    let mut u = guardian("a11y-b-guardian-away");
    u.app.handle_click(4.0, 4.0);
    assert!(u.app.guardian_modal.is_none());
}
