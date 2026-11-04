//! v0.0.12 rc2 — each setup step is announced when it opens (N1–N5).
//!
//! Contract: `doc/plans/V0012-rc2-setup-step-announced.md`. Written before the
//! code, by another author: whoever implements it makes these pass **without
//! editing this file**; a test that looks wrong is reported, not worked around.
//!
//! A child module of `app` (declared in `app.rs`, like `first_run_tests`), so
//! it drives `App`'s own paths on a real, headless `App`:
//! - the tree is always read through `App::build_a11y_tree`, what the adapter
//!   pushes to the screen reader each frame;
//! - a frame (`build_scene`) is laid out before each read, as `render` does
//!   (`build_scene`, then `build_a11y_tree`);
//! - the wizard moves through the functions its controls call:
//!   `handle_wiz_action` (what a click on "Set up this device", Next, Skip,
//!   Back or "Pair with an existing device" resolves to) and `locked_key`
//!   (Enter, Escape: the keyboard);
//! - the login ↔ join switch goes through `handle_click` on the form's hint
//!   strip, the only control that switches them (`App::test_view` gives the
//!   click a window size).
//!
//! Scenarios that need a profile of their own (no `auth.store`, or a real
//! one) run in a **child process** of this test binary, with the launcher of
//! `first_run_tests` (`launch`): the in-process tests share one
//! `SOVEREIGN_DATA_DIR`, and other tests of the binary may write an
//! `auth.store` into it, which changes what the first frame shows.
//!
//! **Readings chosen** (the contract leaves them open; reported with the
//! tests):
//! - A focused node's *name* is what the Windows adapter gives Narrator:
//!   `accesskit_windows` 0.32.1 `Node::name`, over `accesskit_consumer`
//!   0.35.0 `Node::write_label` (the versions in `Cargo.lock`), read there,
//!   not assumed:
//!   - a node of role `Label` is named by its **value**;
//!   - any other node by its own label when it has one (even an empty one);
//!   - else by its `labelled_by` targets, joined by spaces, each by its
//!     **value** when its role is `Label`, by its own label otherwise; with
//!     no explicit `labelled_by`, a button, check box, radio button, link or
//!     menu item is labelled by its `Label`/`Image` descendants (through
//!     `GenericContainer`s), likewise.
//!
//!   So a node named only through `labelled_by` → `Label` nodes that carry
//!   their text as a label (as the shell's do) has no name, and fails. The
//!   role of the focused node is not checked (N1).
//! - Amended for V0012-rc2-labels-spoken (2026-11-01, its test author): the
//!   rule now lives in `crate::a11y_spoken::spoken_name`, shared, and the
//!   names N3 and N5 read (the step's title before the field, the names kept
//!   in drawn order, the masked fields found by name) are the **spoken**
//!   names, so they keep holding once a `Label`'s text moves to its value.
//!   What they assert is unchanged.
//! - "The step's title" is the title `onboarding::step_copy` draws. That
//!   function is private to `onboarding`, so the titles are copied here, as
//!   `first_run_tests::STEP_TITLES` does.
//! - N2's "first display": before the first frame, the screen reader has the
//!   window only (`A11yActivation::request_initial_tree` returns `None`), so
//!   the focus "before" is the root. After the first frame the focus must be
//!   another node.
//! - N3 covers every step with a field: the nickname step too (it has two),
//!   besides the password, duress and canary steps.
//! - N4's switch: in the running app the login form is shown without an
//!   `auth.store` only until the next frame (`ensure_wizard` then puts the
//!   wizard back). The switch the contract names is tested at the form, by
//!   the hint's click, with the tree read right after it; the switch as the
//!   user lives it (join → hint → next frame → the wizard) is tested too.
//! - N5's "every label is unchanged": every name today's tree gives a step
//!   (title, "Step N of 8", widgets, fields, toggle, buttons; the step's body
//!   copy is not listed) is still in the tree, in the same order. New nodes
//!   may be added between them; none may be renamed.

use std::path::PathBuf;
use std::sync::OnceLock;

use accesskit::{NodeId, Role as AkRole, TreeUpdate};
use sovereign_core::profile::BubbleStyle;
use winit::keyboard::{Key, NamedKey};

use super::first_run_tests::{launch, profile, shut_runtime, CHILD_VAR, DURESS, H, PRIMARY, W};
use super::App;
use crate::onboarding::{
    OnboardingWizard, WizAction, STEP_BUBBLE, STEP_CANARY, STEP_DURESS, STEP_NICKNAME, STEP_PASSWORD,
    STEP_SAMPLE, STEP_THEME, STEP_WELCOME, WIZ_STEPS,
};
use crate::panels::AuthForm;

// ======================================================================= shared

/// The titles `onboarding::step_copy` draws, per step (copied: `step_copy` is
/// private to `onboarding`).
const STEP_TITLES: [&str; WIZ_STEPS] = [
    "Welcome to Sovereign",
    "Name your AI",
    "Choose a bubble style",
    "Choose your theme",
    "Sample data",
    "Set your password",
    "Duress password",
    "Canary phrase",
];

/// The steps without a text field (N1).
const NO_FIELD_STEPS: [usize; 4] = [STEP_WELCOME, STEP_BUBBLE, STEP_THEME, STEP_SAMPLE];
/// The steps with a field (N3).
const FIELD_STEPS: [usize; 4] = [STEP_NICKNAME, STEP_PASSWORD, STEP_DURESS, STEP_CANARY];

/// Secrets typed in these screens; none may appear in the tree (N5).
const WIZ_PASSWORD: &str = "Wz!Announce-Pass1";
const WIZ_DURESS: &str = "Dz!Announce-Pass2";
const WIZ_CANARY: &str = "heron at dusk";
const LOGIN_SECRET: &str = "Login!Announce-77";
const JOIN_PASSWORD: &str = "Join!Announce-Pass3";
const JOIN_DURESS: &str = "Join!Announce-Duress4";

fn step_number(step: usize) -> String {
    format!("Step {} of {}", step + 1, WIZ_STEPS)
}

/// The labels drawn above a step's fields (`onboarding::field_labels`).
fn field_labels(step: usize) -> Vec<&'static str> {
    match step {
        STEP_NICKNAME => vec!["Nickname", crate::device_name::FIELD_LABEL],
        STEP_PASSWORD => vec!["Password", "Confirm password"],
        STEP_DURESS => vec!["Duress password", "Confirm duress password"],
        STEP_CANARY => vec!["Canary phrase", "Confirm phrase"],
        _ => vec![],
    }
}

/// The in-process tests' data directory: a fresh temporary one unless
/// `SOVEREIGN_DATA_DIR` is already set.
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
        let d = std::env::temp_dir().join(format!("sov-step-announced-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::env::set_var("SOVEREIGN_DATA_DIR", &d);
        d
    });
}

/// A locked shell showing the setup wizard on its welcome step, every field
/// filled so each step can be left forward (a valid password, a valid duress
/// password, a valid safety phrase). Reached through `OnboardingWizard::new`,
/// as `ensure_wizard` does.
fn setup_app() -> App {
    ensure_test_data_dir();
    let mut app = App::new();
    app.locked = true;
    app.test_view = Some((W, H));
    let mut w = OnboardingWizard::new("Designation-7".into(), false);
    w.nickname = "Ada".into();
    w.password = WIZ_PASSWORD.into();
    w.password_confirm = WIZ_PASSWORD.into();
    w.duress = WIZ_DURESS.into();
    w.duress_confirm = WIZ_DURESS.into();
    w.canary = WIZ_CANARY.into();
    w.canary_confirm = WIZ_CANARY.into();
    assert_eq!(w.step, STEP_WELCOME, "the wizard opens on its welcome step");
    app.wizard = Some(w);
    app
}

fn step_of(app: &App) -> usize {
    app.wizard.as_ref().expect("the wizard is shown").step
}

/// Moves the wizard forward to `step` with its Next action (the welcome
/// step's "Set up this device" resolves to Next too), one step at a time.
fn walk_to(app: &mut App, step: usize) {
    while step_of(app) < step {
        let from = step_of(app);
        app.handle_wiz_action(WizAction::Next);
        assert_eq!(step_of(app), from + 1, "Next leaves step {from} (error: {:?})", app.wizard.as_ref().unwrap().error);
    }
}

// ------------------------------------------------------------- the a11y tree

/// The tree as the screen reader receives it.
struct Snap {
    up: TreeUpdate,
    root: NodeId,
}

impl Snap {
    fn node(&self, id: NodeId) -> Option<&accesskit::Node> {
        self.up.nodes.iter().find(|(i, _)| *i == id).map(|(_, n)| n)
    }
    fn focus(&self) -> NodeId {
        self.up.focus
    }
    /// A node's label property (what N5 keeps unchanged).
    fn name_of(&self, id: NodeId) -> String {
        self.node(id).and_then(|n| n.label()).unwrap_or_default().to_string()
    }
    /// The name Narrator receives for a node: `accesskit_windows` 0.32.1
    /// `Node::name` over `accesskit_consumer` 0.35.0 `Node::write_label`
    /// (see the header), now `crate::a11y_spoken::spoken_name`.
    fn spoken_name(&self, id: NodeId) -> String {
        crate::a11y_spoken::spoken_name(&self.up.nodes, id)
    }
    fn role_of(&self, id: NodeId) -> Option<AkRole> {
        self.node(id).map(|n| n.role())
    }
    /// What Narrator says for the focused node.
    fn focus_name(&self) -> String {
        self.spoken_name(self.focus())
    }
    /// The nodes reachable from the root, in document order.
    fn order(&self) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = vec![self.root];
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
    /// The spoken names in document order (amended, see the header).
    fn names_in_order(&self) -> Vec<String> {
        self.order().into_iter().map(|id| self.spoken_name(id)).collect()
    }
    /// Every string the tree carries (labels, values, descriptions).
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
        self.order()
            .into_iter()
            .map(|id| {
                let n = self.node(id).unwrap();
                let mark = if id == self.focus() {
                    format!("  <- focus, spoken as {:?}", self.spoken_name(id))
                } else {
                    String::new()
                };
                format!("{:?} {:?} {:?} = {:?} (spoken {:?}){mark}", id, n.role(), self.name_of(id), n.value(), self.spoken_name(id))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
    /// The node of the given role and spoken name, if any (amended, see the
    /// header).
    fn find(&self, role: AkRole, name: &str) -> Option<NodeId> {
        self.order().into_iter().find(|id| self.role_of(*id) == Some(role) && self.spoken_name(*id) == name)
    }
}

/// The tree as pushed by `render`: a frame, then `build_a11y_tree`.
fn frame_and_read(app: &mut App) -> Snap {
    app.build_scene(W, H);
    read(app)
}

/// The tree without a frame before it (for a form a frame would replace).
fn read(app: &mut App) -> Snap {
    let up = app.build_a11y_tree();
    let root = up.tree.as_ref().map(|t| t.root).expect("the update carries its tree");
    Snap { up, root }
}

/// N1 on the step on screen (no field): the focus is not the root, and its
/// name holds the step's title and, past the welcome step, "Step N of 8".
fn n1_problem(step: usize, t: &Snap, how: &str) -> Option<String> {
    let name = t.focus_name();
    let mut wrong = Vec::new();
    if t.focus() == t.root {
        wrong.push("the focus is the root (the window)".to_string());
    }
    if !name.contains(STEP_TITLES[step]) {
        wrong.push(format!("its name lacks the title {:?}", STEP_TITLES[step]));
    }
    if step > STEP_WELCOME && !name.contains(&step_number(step)) {
        wrong.push(format!("its name lacks {:?}", step_number(step)));
    }
    if wrong.is_empty() {
        None
    } else {
        Some(format!(
            "step {step} ({:?}) {how}: focus {:?} named {name:?}: {}\n{}",
            STEP_TITLES[step],
            t.focus(),
            wrong.join("; "),
            t.dump()
        ))
    }
}

fn assert_none(problems: Vec<String>, what: &str) {
    assert!(problems.is_empty(), "{what}: {} problem(s)\n\n{}", problems.len(), problems.join("\n\n"));
}

// ======================================================= N1 (in-process)

/// N1, each field-less step reached forward (Next from the step before).
#[test]
fn n1_a_step_without_a_field_announces_itself_when_reached_forward() {
    let mut problems = Vec::new();
    for step in NO_FIELD_STEPS {
        let mut app = setup_app();
        walk_to(&mut app, step);
        let t = frame_and_read(&mut app);
        problems.extend(n1_problem(step, &t, "reached by Next"));
        shut_runtime(&mut app);
        std::mem::forget(app);
    }
    assert_none(problems, "N1 (forward)");
}

/// N1, each field-less step reached back (the Back button from the step
/// after, and Escape from the step after).
#[test]
fn n1_a_step_without_a_field_announces_itself_when_reached_back() {
    let mut problems = Vec::new();
    for step in NO_FIELD_STEPS {
        for how in ["Back", "Escape"] {
            let mut app = setup_app();
            walk_to(&mut app, step + 1);
            frame_and_read(&mut app);
            match how {
                "Back" => app.handle_wiz_action(WizAction::Back),
                _ => app.locked_key(&Key::Named(NamedKey::Escape), None),
            }
            assert_eq!(step_of(&app), step, "{how} from step {} goes back one step", step + 1);
            let t = frame_and_read(&mut app);
            problems.extend(n1_problem(step, &t, &format!("reached by {how}")));
            shut_runtime(&mut app);
            std::mem::forget(app);
        }
    }
    assert_none(problems, "N1 (back)");
}

// ======================================================= N2 (in-process)

/// N2, every forward move: Next (the button's action; "Set up this device"
/// on welcome) from each step, and Skip on the duress step.
#[test]
fn n2_every_forward_move_moves_the_focus() {
    let mut problems = Vec::new();
    for from in 0..STEP_CANARY {
        let mut app = setup_app();
        walk_to(&mut app, from);
        let before = frame_and_read(&mut app);
        app.handle_wiz_action(WizAction::Next);
        assert_eq!(step_of(&app), from + 1, "Next from step {from}");
        let after = frame_and_read(&mut app);
        if after.focus() == before.focus() {
            problems.push(format!(
                "Next {from} -> {}: the focus stays on {:?} ({:?} -> {:?})\n{}",
                from + 1,
                after.focus(),
                before.focus_name(),
                after.focus_name(),
                after.dump()
            ));
        }
        shut_runtime(&mut app);
        std::mem::forget(app);
    }
    // Skip on the duress step (the canary's Skip finishes the setup).
    let mut app = setup_app();
    walk_to(&mut app, STEP_DURESS);
    let before = frame_and_read(&mut app);
    app.handle_wiz_action(WizAction::Skip);
    assert_eq!(step_of(&app), STEP_CANARY, "Skip from the duress step");
    let after = frame_and_read(&mut app);
    if after.focus() == before.focus() {
        problems.push(format!(
            "Skip {STEP_DURESS} -> {STEP_CANARY}: the focus stays on {:?} ({:?} -> {:?})\n{}",
            after.focus(),
            before.focus_name(),
            after.focus_name(),
            after.dump()
        ));
    }
    shut_runtime(&mut app);
    std::mem::forget(app);
    assert_none(problems, "N2 (forward)");
}

/// N2, every forward move by the keyboard: Enter on the step's last field
/// (or on a step without a field), which `wiz_enter` turns into Next.
#[test]
fn n2_every_forward_move_by_enter_moves_the_focus() {
    let mut problems = Vec::new();
    for from in 0..STEP_CANARY {
        let mut app = setup_app();
        walk_to(&mut app, from);
        {
            let w = app.wizard.as_mut().unwrap();
            w.focus = w.field_count().saturating_sub(1);
        }
        let before = frame_and_read(&mut app);
        app.locked_key(&Key::Named(NamedKey::Enter), None);
        assert_eq!(step_of(&app), from + 1, "Enter on step {from}'s last field moves on");
        let after = frame_and_read(&mut app);
        if after.focus() == before.focus() {
            problems.push(format!(
                "Enter {from} -> {}: the focus stays on {:?} ({:?} -> {:?})\n{}",
                from + 1,
                after.focus(),
                before.focus_name(),
                after.focus_name(),
                after.dump()
            ));
        }
        shut_runtime(&mut app);
        std::mem::forget(app);
    }
    assert_none(problems, "N2 (Enter)");
}

/// N2, every back move: the Back button's action and Escape, from each step
/// past the welcome step. (Céline: "when I go back one step in the
/// onboarding flow, the narrator doesn't start".)
#[test]
fn n2_every_back_move_moves_the_focus() {
    let mut problems = Vec::new();
    for from in 1..WIZ_STEPS {
        for how in ["Back", "Escape"] {
            let mut app = setup_app();
            walk_to(&mut app, from);
            let before = frame_and_read(&mut app);
            match how {
                "Back" => app.handle_wiz_action(WizAction::Back),
                _ => app.locked_key(&Key::Named(NamedKey::Escape), None),
            }
            assert_eq!(step_of(&app), from - 1, "{how} from step {from}");
            let after = frame_and_read(&mut app);
            if after.focus() == before.focus() {
                problems.push(format!(
                    "{how} {from} -> {}: the focus stays on {:?} ({:?} -> {:?})\n{}",
                    from - 1,
                    after.focus(),
                    before.focus_name(),
                    after.focus_name(),
                    after.dump()
                ));
            }
            shut_runtime(&mut app);
            std::mem::forget(app);
        }
    }
    assert_none(problems, "N2 (back)");
}

/// N2, the first display: on a fresh profile the first frame shows the
/// wizard's welcome step, and the focus leaves the root (the window, all the
/// screen reader has before). In a child process: a profile without
/// `auth.store`.
#[test]
fn n2_the_first_display_moves_the_focus_off_the_window() {
    let p = profile();
    launch(module(), "first_display", p.path());
}

fn first_display() {
    assert!(!crate::crypto::auth_store_exists(), "precondition: a fresh profile");
    let mut app = App::new();
    let t = frame_and_read(&mut app);
    assert!(
        app.locked && app.wizard.as_ref().is_some_and(|w| w.step == STEP_WELCOME),
        "a fresh profile's first frame shows the wizard's welcome step"
    );
    assert_ne!(t.focus(), t.root, "N2, first display: the focus is still the root (the window)\n{}", t.dump());
    if let Some(p) = n1_problem(STEP_WELCOME, &t, "on the first display") {
        panic!("N1 on the first display: {p}");
    }
    shut_runtime(&mut app);
    std::mem::forget(app);
}

// ======================================================= N3 (in-process)

/// N3, today's behaviour pinned: on a step with a field, the focus is on its
/// first field, named by its drawn label (a password input on the secret
/// steps); the step's title is in the tree, before the field. Reached
/// forward, and back.
#[test]
fn n3_a_step_with_a_field_focuses_the_field_named_by_its_label() {
    let mut problems = Vec::new();
    for step in FIELD_STEPS {
        for how in ["Next", "Back"] {
            if how == "Back" && step == STEP_CANARY {
                continue; // the last step: nothing to come back from
            }
            let mut app = setup_app();
            if how == "Next" {
                walk_to(&mut app, step);
            } else {
                walk_to(&mut app, step + 1);
                frame_and_read(&mut app);
                app.handle_wiz_action(WizAction::Back);
                assert_eq!(step_of(&app), step);
            }
            let t = frame_and_read(&mut app);
            let want = field_labels(step)[0];
            let role = t.role_of(t.focus());
            let want_role = if step == STEP_NICKNAME { AkRole::TextInput } else { AkRole::PasswordInput };
            if role != Some(want_role) || t.focus_name() != want {
                problems.push(format!(
                    "step {step} reached by {how}: focus {:?} is {role:?} {:?}, not the {want_role:?} {want:?}\n{}",
                    t.focus(),
                    t.focus_name(),
                    t.dump()
                ));
            }
            let order = t.order();
            let title_at = order.iter().position(|id| t.spoken_name(*id) == STEP_TITLES[step]);
            let field_at = order.iter().position(|id| *id == t.focus());
            if !matches!((title_at, field_at), (Some(a), Some(b)) if a < b) {
                problems.push(format!(
                    "step {step} reached by {how}: the title {:?} is not in the tree before the field (title at {title_at:?}, field at {field_at:?})\n{}",
                    STEP_TITLES[step],
                    t.dump()
                ));
            }
            shut_runtime(&mut app);
            std::mem::forget(app);
        }
    }
    assert_none(problems, "N3");
}

/// N3 with N2: arriving on a step with a field, forward or back, the focused
/// field is a different node from the one focused before the change.
#[test]
fn n3_arriving_on_a_step_with_a_field_moves_the_focus() {
    let mut problems = Vec::new();
    for step in FIELD_STEPS {
        // Forward, from the step before.
        let mut app = setup_app();
        walk_to(&mut app, step - 1);
        let before = frame_and_read(&mut app);
        app.handle_wiz_action(WizAction::Next);
        let after = frame_and_read(&mut app);
        if after.focus() == before.focus() {
            problems.push(format!(
                "Next {} -> {step}: the focus stays on {:?} ({:?} -> {:?})",
                step - 1,
                after.focus(),
                before.focus_name(),
                after.focus_name()
            ));
        }
        shut_runtime(&mut app);
        std::mem::forget(app);
        // Back, from the step after.
        if step < STEP_CANARY {
            let mut app = setup_app();
            walk_to(&mut app, step + 1);
            let before = frame_and_read(&mut app);
            app.handle_wiz_action(WizAction::Back);
            let after = frame_and_read(&mut app);
            if after.focus() == before.focus() {
                problems.push(format!(
                    "Back {} -> {step}: the focus stays on {:?} ({:?} -> {:?})",
                    step + 1,
                    after.focus(),
                    before.focus_name(),
                    after.focus_name()
                ));
            }
            shut_runtime(&mut app);
            std::mem::forget(app);
        }
    }
    assert_none(problems, "N3 with N2");
}

// ======================================================= N4

/// N4, the join form shown from the welcome step ("Pair with an existing
/// device"): the focus is on its first field, named by its drawn label, and
/// it moved.
#[test]
fn n4_the_join_form_shown_from_welcome_is_focused_named_and_moved() {
    let mut app = setup_app();
    let before = frame_and_read(&mut app);
    app.handle_wiz_action(WizAction::ChoosePair);
    assert!(app.wizard.is_none() && app.auth_form.joining, "Pair with an existing device shows the join form");
    let t = frame_and_read(&mut app);
    let want = AuthForm::join().labels[0];
    assert_ne!(t.focus(), t.root, "N4: the join form's focus is the root (the window)\n{}", t.dump());
    assert_eq!(t.focus_name(), want, "N4: the join form's focus is named by its drawn label\n{}", t.dump());
    assert_ne!(
        t.focus(),
        before.focus(),
        "N4 (N2): welcome -> join, the focus stays on {:?} ({:?} -> {:?})\n{}",
        t.focus(),
        before.focus_name(),
        t.focus_name(),
        t.dump()
    );
    shut_runtime(&mut app);
    std::mem::forget(app);
}

/// N4, the login form on a profile with an account: its first frame's focus
/// is its password field, named by its label, not the window. In a child
/// process with a real `auth.store` (several seconds in a debug build).
#[test]
fn n4_the_login_form_is_focused_and_named_when_shown() {
    let p = profile();
    launch(module(), "login_first_display", p.path());
}

fn login_first_display() {
    crate::crypto::create_auth_store(PRIMARY.as_bytes(), DURESS.as_bytes()).expect("create auth.store");
    let mut app = App::new();
    let t = frame_and_read(&mut app);
    assert!(app.locked && app.wizard.is_none() && !app.auth_form.joining, "a profile with auth.store shows the login form");
    assert_ne!(t.focus(), t.root, "N4: the login form's focus is the root (the window)\n{}", t.dump());
    assert_eq!(t.focus_name(), "Password", "N4: the login form's focus is named by its drawn label\n{}", t.dump());
    assert_eq!(t.role_of(t.focus()), Some(AkRole::PasswordInput), "N4: the login form's focus is its password field");
    shut_runtime(&mut app);
    std::mem::forget(app);
}

/// N4, the switch between the forms, by the hint strip's click (the only
/// control that switches them): login -> join -> login, the tree read right
/// after each click. Each time, the focus is on the new form's first field,
/// named, and it moved. In a child process: the click acts only without an
/// `auth.store`.
#[test]
fn n4_switching_between_login_and_join_moves_the_focus() {
    let p = profile();
    launch(module(), "form_switch", p.path());
}

/// Clicks the auth form's hint strip, as a user does.
fn click_hint(app: &mut App) {
    let (card, _) = crate::panels::auth_layout(W, H, app.auth_form.fields.len());
    app.handle_click((card.x0 + card.x1) * 0.5, card.y1 - 17.0);
}

fn form_switch() {
    assert!(!crate::crypto::auth_store_exists(), "precondition: a fresh profile");
    let mut app = App::new();
    app.test_view = Some((W, H));
    app.locked = true;
    app.wizard = None;
    app.auth_form = AuthForm::login();
    let mut problems = Vec::new();
    let mut before = read(&mut app);
    for (want_join, label) in [(true, "login -> join"), (false, "join -> login")] {
        click_hint(&mut app);
        assert_eq!(app.auth_form.joining, want_join, "{label}: the hint's click switches the form");
        let t = read(&mut app);
        let want = app.auth_form.labels[0];
        if t.focus() == t.root || t.focus_name() != want {
            problems.push(format!("{label}: focus {:?} named {:?}, not the field {want:?}\n{}", t.focus(), t.focus_name(), t.dump()));
        }
        if t.focus() == before.focus() {
            problems.push(format!(
                "{label}: the focus stays on {:?} ({:?} -> {:?})\n{}",
                t.focus(),
                before.focus_name(),
                t.focus_name(),
                t.dump()
            ));
        }
        before = t;
    }
    shut_runtime(&mut app);
    std::mem::forget(app);
    assert_none(problems, "N4 (switch)");
}

/// N4, the switch as the user lives it before any account exists: welcome
/// -> "Pair with an existing device" -> the join form -> the hint ("create a
/// new account instead") -> the next frame shows the wizard's welcome step
/// again. Each change moves the focus, and the welcome step announces itself
/// (N1). In a child process: a profile without `auth.store`.
#[test]
fn n4_leaving_the_join_form_for_the_setup_moves_the_focus() {
    let p = profile();
    launch(module(), "join_and_back", p.path());
}

fn join_and_back() {
    assert!(!crate::crypto::auth_store_exists(), "precondition: a fresh profile");
    let mut app = App::new();
    app.test_view = Some((W, H));
    let welcome = frame_and_read(&mut app);
    assert!(app.wizard.as_ref().is_some_and(|w| w.step == STEP_WELCOME), "a fresh profile shows the welcome step");
    app.handle_wiz_action(WizAction::ChoosePair);
    let join = frame_and_read(&mut app);
    assert!(app.auth_form.joining, "the join form is shown");
    click_hint(&mut app);
    let back = frame_and_read(&mut app);
    assert!(
        app.wizard.as_ref().is_some_and(|w| w.step == STEP_WELCOME),
        "leaving the join form puts the wizard back on the next frame"
    );
    let mut problems = Vec::new();
    if join.focus() == welcome.focus() {
        problems.push(format!("welcome -> join: the focus stays on {:?}\n{}", join.focus(), join.dump()));
    }
    if back.focus() == join.focus() {
        problems.push(format!("join -> welcome: the focus stays on {:?}\n{}", back.focus(), back.dump()));
    }
    problems.extend(n1_problem(STEP_WELCOME, &back, "after leaving the join form"));
    shut_runtime(&mut app);
    std::mem::forget(app);
    assert_none(problems, "N4 (join and back)");
}

// ======================================================= N5 (in-process)

/// N5: a masked field of the wizard (password, duress, safety phrase) still
/// exposes its length only, never its text, "show" on or off.
#[test]
fn n5_masked_wizard_fields_expose_their_length_only_show_on_and_off() {
    let mut problems = Vec::new();
    for step in [STEP_PASSWORD, STEP_DURESS, STEP_CANARY] {
        for reveal in [false, true] {
            let mut app = setup_app();
            walk_to(&mut app, step);
            app.wizard.as_mut().unwrap().reveal = reveal;
            let t = frame_and_read(&mut app);
            let secret = match step {
                STEP_PASSWORD => WIZ_PASSWORD,
                STEP_DURESS => WIZ_DURESS,
                _ => WIZ_CANARY,
            };
            for s in t.every_string() {
                for x in [WIZ_PASSWORD, WIZ_DURESS, WIZ_CANARY] {
                    if s.contains(x) {
                        problems.push(format!("step {step}, show {reveal}: the tree carries a secret's text ({s:?})"));
                    }
                }
            }
            for label in field_labels(step) {
                match t.find(AkRole::PasswordInput, label) {
                    None => problems.push(format!("step {step}, show {reveal}: no password input named {label:?}\n{}", t.dump())),
                    Some(id) => {
                        let v = t.node(id).and_then(|n| n.value()).map(str::to_string);
                        let want = format!("{} characters", secret.chars().count());
                        if v.as_deref() != Some(want.as_str()) {
                            problems.push(format!("step {step}, show {reveal}: {label:?} exposes {v:?}, not {want:?}"));
                        }
                    }
                }
            }
            shut_runtime(&mut app);
            std::mem::forget(app);
        }
    }
    assert_none(problems, "N5 (wizard)");
}

/// N5: the login and join forms' masked fields, likewise.
#[test]
fn n5_masked_login_and_join_fields_expose_their_length_only_show_on_and_off() {
    ensure_test_data_dir();
    let mut problems = Vec::new();
    for reveal in [false, true] {
        // Login (no frame: without an auth.store, a frame would put the wizard back).
        let mut app = App::new();
        app.locked = true;
        app.wizard = None;
        let mut f = AuthForm::login();
        f.fields[0] = LOGIN_SECRET.into();
        f.reveal = reveal;
        app.auth_form = f;
        let t = read(&mut app);
        check_masked(&t, &[("Password", LOGIN_SECRET)], &format!("login, show {reveal}"), &mut problems);
        shut_runtime(&mut app);
        std::mem::forget(app);
        // Join (a frame keeps it: `ensure_wizard` leaves a join alone).
        let mut app = App::new();
        app.locked = true;
        app.wizard = None;
        let mut f = AuthForm::join();
        f.fields[1] = "482913".into();
        f.fields[2] = JOIN_PASSWORD.into();
        f.fields[3] = JOIN_DURESS.into();
        f.reveal = reveal;
        app.auth_form = f;
        let t = frame_and_read(&mut app);
        let labels = AuthForm::join().labels;
        check_masked(&t, &[(labels[2], JOIN_PASSWORD), (labels[3], JOIN_DURESS)], &format!("join, show {reveal}"), &mut problems);
        shut_runtime(&mut app);
        std::mem::forget(app);
    }
    assert_none(problems, "N5 (login, join)");
}

fn check_masked(t: &Snap, fields: &[(&str, &str)], what: &str, problems: &mut Vec<String>) {
    for s in t.every_string() {
        for (_, secret) in fields {
            if s.contains(secret) {
                problems.push(format!("{what}: the tree carries a secret's text ({s:?})"));
            }
        }
    }
    for (label, secret) in fields {
        match t.find(AkRole::PasswordInput, label) {
            None => problems.push(format!("{what}: no password input named {label:?}\n{}", t.dump())),
            Some(id) => {
                let v = t.node(id).and_then(|n| n.value()).map(str::to_string);
                let want = format!("{} characters", secret.chars().count());
                if v.as_deref() != Some(want.as_str()) {
                    problems.push(format!("{what}: {label:?} exposes {v:?}, not {want:?}"));
                }
            }
        }
    }
}

/// The names today's tree gives a step, in drawn order (the body copy left
/// out): what N5 keeps unchanged.
fn todays_names(w: &OnboardingWizard) -> Vec<String> {
    let step = w.step;
    let mut v: Vec<String> = vec![STEP_TITLES[step].into()];
    if step > STEP_WELCOME {
        v.push(step_number(step));
    }
    match step {
        STEP_WELCOME => {
            v.push(format!("Your designation: {}", w.designation));
            v.push("Set up this device".into());
            v.push("Pair with an existing device".into());
        }
        STEP_BUBBLE => {
            for st in BubbleStyle::all() {
                v.push(if *st == w.bubble { format!("{}, selected", st.label()) } else { st.label().into() });
            }
        }
        STEP_THEME => {
            for (name, dark) in [("Dark", true), ("Light", false)] {
                v.push(if dark == w.theme_dark { format!("{name}, selected") } else { name.into() });
            }
        }
        STEP_SAMPLE => {
            v.push(format!("Load sample data: {}", if w.seed_sample { "Enabled" } else { "Disabled" }));
        }
        _ => {
            for (i, l) in field_labels(step).into_iter().enumerate() {
                v.push(l.into());
                if step == STEP_NICKNAME && i == crate::onboarding::DEVICE_NAME_FIELD {
                    v.push(crate::device_name::SEEN_BY.into());
                }
            }
            match step {
                STEP_PASSWORD | STEP_DURESS => v.push("Show passwords".into()),
                STEP_CANARY => v.push("Show phrase".into()),
                _ => {}
            }
        }
    }
    if step > STEP_WELCOME {
        v.push("Back".into());
        if matches!(step, STEP_DURESS | STEP_CANARY) {
            v.push("Skip".into());
        }
        v.push(if step == STEP_CANARY { "Get started" } else { "Next" }.into());
    }
    v
}

/// N5: every label is unchanged — each name today's tree gives a step is
/// still there, in the same order (nodes may be added between them).
#[test]
fn n5_every_step_keeps_its_labels_in_drawn_order() {
    let mut problems = Vec::new();
    for step in 0..WIZ_STEPS {
        let mut app = setup_app();
        walk_to(&mut app, step);
        let t = frame_and_read(&mut app);
        let want = todays_names(app.wizard.as_ref().unwrap());
        let have = t.names_in_order();
        let mut it = have.iter();
        for name in &want {
            if !it.any(|h| h == name) {
                problems.push(format!("step {step}: {name:?} is missing or out of order\nwant (in order): {want:?}\n{}", t.dump()));
                break;
            }
        }
        shut_runtime(&mut app);
        std::mem::forget(app);
    }
    for (label, form) in [("login", AuthForm::login()), ("join", AuthForm::join())] {
        ensure_test_data_dir();
        let mut app = App::new();
        app.locked = true;
        app.wizard = None;
        let title = if form.joining { "Join from another device" } else { "Unlock Sovereign" };
        let mut want: Vec<String> = vec![title.into()];
        want.extend(form.labels.iter().map(|l| l.to_string()));
        app.auth_form = form;
        let t = read(&mut app);
        let have = t.names_in_order();
        let mut it = have.iter();
        for name in &want {
            if !it.any(|h| h == name) {
                problems.push(format!("{label}: {name:?} is missing or out of order\nwant (in order): {want:?}\n{}", t.dump()));
                break;
            }
        }
        shut_runtime(&mut app);
        std::mem::forget(app);
    }
    assert_none(problems, "N5 (labels)");
}

/// N5: nothing is added to the tree when the workspace is unlocked — not
/// the wizard's step (even with a wizard left in the App), not a form; the
/// focus stays where it is today with no window open (the root).
#[test]
fn n5_nothing_is_added_when_unlocked() {
    let mut app = setup_app();
    walk_to(&mut app, STEP_BUBBLE);
    app.locked = false;
    app.windows.clear();
    let t = read(&mut app);
    let names = t.names_in_order();
    for s in t.every_string() {
        for title in STEP_TITLES.iter().chain(["Unlock Sovereign", "Join from another device"].iter()) {
            assert!(!s.contains(title), "unlocked: the tree carries {s:?} (a locked screen's)\n{}", t.dump());
        }
        assert!(!s.contains("Step ") || !s.contains(" of 8"), "unlocked: the tree carries {s:?}\n{}", t.dump());
    }
    assert_eq!(t.focus(), t.root, "unlocked, no window open: the focus is the root, as today\n{}", t.dump());
    assert!(
        names.iter().any(|n| n.starts_with("Spatial canvas")),
        "unlocked, no window open: the canvas summary, as today\n{}",
        t.dump()
    );
    shut_runtime(&mut app);
    std::mem::forget(app);
}

// ================================================ child launches

fn module() -> &'static str {
    module_path!()
}

/// One launch of the app on the profile of `SOVEREIGN_DATA_DIR` (set by the
/// parent, `first_run_tests::launch`). Ignored in ordinary runs.
#[test]
#[ignore = "a launch, run by its parent test in a child process on a profile of its own"]
fn child_entry() {
    let Ok(scenario) = std::env::var(CHILD_VAR) else { return };
    match scenario.as_str() {
        "first_display" => first_display(),
        "login_first_display" => login_first_display(),
        "form_switch" => form_switch(),
        "join_and_back" => join_and_back(),
        other => panic!("unknown scenario {other}"),
    }
}
