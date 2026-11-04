//! v0.0.12 items 18 and 20 in the native shell — the locked screens say what
//! they show (A1–A5), an interrupted or failed setup ends in a usable account
//! (B5, B5b), P4 on the desktop, and the auth form's onboarding mode deleted
//! ("Decided (2026-10-24)").
//!
//! Contract: `doc/plans/V0012-items18-20-first-run.md`. Written before the
//! code, by another author: whoever implements items 18 and 20 makes these
//! pass **without editing this file**; a test that looks wrong is reported,
//! not worked around. B1–B4 (the worker, with a fake derivation that waits)
//! are in `first_run_worker_tests.rs`, which needs a seam named there.
//!
//! A child module of `app` (declared in `app.rs`, like `lot1_tests`), so it
//! drives `App`'s own paths on a real, headless `App` (`App::new()` needs no
//! window and no GPU; `build_scene` lays a frame out without rendering it):
//! - **A1–A5** read `App::build_a11y_tree` — what a screen reader receives —
//!   with the screen locked, for every wizard step, the login form and the
//!   join form. They run in this process.
//! - **B5, B5b, P4** need a profile of their own (an `auth.store`, a marker,
//!   a database), and `SOVEREIGN_DATA_DIR` is read by the whole process. So
//!   each "launch" is a **child process** of this test binary, run on a fresh
//!   temporary profile: the parent runs `child_entry` (ignored in ordinary
//!   runs) with `FIRST_RUN_CHILD=<scenario>` and `SOVEREIGN_DATA_DIR=<profile>`,
//!   and checks that exactly one test passed. A "kill" is a real one: the
//!   child prints a line when it reaches the cut, and the parent kills it
//!   (`Child::kill`, TerminateProcess on Windows). These use the real
//!   `create_auth_store` and the real derivations (several seconds each in a
//!   debug build).
//! - A result that arrives off the UI thread is waited for by drawing frames
//!   (`build_scene`), as the window does; nothing here assumes the work is
//!   synchronous or not.
//!
//! **Readings chosen** (the contract leaves them open; reported with the
//! tests):
//! - A field node is any of AccessKit's text-input roles (`TextInput`,
//!   `PasswordInput`, …); a button is `Button` or `DefaultButton`. A
//!   step's buttons are its drawn nav buttons (Back, Skip, Next / Get
//!   started) and, on Welcome, its two choices; the bubble, theme and sample
//!   widgets are not required here (only A4 applies to them).
//! - A masked field (the wizard's password and duress steps; the masked
//!   fields of the login and join forms) has value "N characters" when it
//!   holds N characters, and no value (or "0 characters") when empty. The
//!   safety phrase is drawn in clear, so its value is not checked.
//! - An empty name is a node whose label is absent or blank; the root counts.
//!   Amended for V0012-rc2-labels-spoken (2026-11-01, its test author): a
//!   node's *name* is now what Narrator receives (`Seen::name`, the spoken
//!   name of `crate::a11y_spoken`: a `Label` is spoken by its value), so these
//!   tests keep holding once a `Label`'s text moves to its value. `has_name`,
//!   used only to say a text is absent, matches the spoken name or the label
//!   property, so it is no weaker. What the tests assert is unchanged.
//! - B5b: "a later failure" after the store is made real by an unreadable
//!   key database (`crypto/keys.db`), which `install_session` reads after the
//!   store is written. The marker's name is the implementer's, so it is
//!   tested by what it does: no notice after a completed setup; the notice
//!   at the first login after an interrupted one, then never again.
//! - B5b as amended (ff959de): the "setup started" record is likewise named
//!   by behaviour only. An interrupted setup is a **real** setup killed once
//!   `auth.store` exists (frames stop at that moment, so nothing the UI thread
//!   does after the store has run); an account "made before this build" is a
//!   store written by `create_auth_store` alone. The join's and the
//!   recovery's ends run with the derivation faked (a real join needs a
//!   second device, a real recovery guardians), on an interrupted profile,
//!   so a missing marker after them would show at the next, real, login.
//! - The notice is shown at a login of either persona (the contract: "Then
//!   the marker is written, at the login of either persona").

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use accesskit::{Node, NodeId, Role as AkRole, TreeUpdate};

use super::App;
use crate::onboarding::{
    OnboardingWizard, WizAction, STEP_CANARY, STEP_DURESS, STEP_NICKNAME, STEP_PASSWORD, WIZ_STEPS,
};
use crate::panels::AuthForm;

// ======================================================================= shared

/// The window the frames are laid out for (logical pixels).
pub(super) const W: f64 = 1280.0;
pub(super) const H: f64 = 800.0;

pub(super) const PRIMARY: &str = "Primary!Pass1234";
pub(super) const DURESS: &str = "Duress!Pass5678";

/// B5b: the desktop's list for lot 1's P2 notice (the contract's exact words).
pub(super) const INTERRUPTED_NOTICE: &str = "Setup was interrupted before it finished. Your account works; \
your profile, safety phrase and sample documents may not have been saved.";

/// The env var naming the scenario a child process runs.
pub(super) const CHILD_VAR: &str = "FIRST_RUN_CHILD";

/// The in-process tests' data directory: a fresh temporary one unless
/// `SOVEREIGN_DATA_DIR` is already set (as `lot1_tests` does).
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
        let d = std::env::temp_dir().join(format!("sov-first-run-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::env::set_var("SOVEREIGN_DATA_DIR", &d);
        d
    });
}

/// The libtest name of `child_entry` in `module` (`module_path!()` of the
/// calling file, without the crate's own segment).
fn child_test_name(module: &str) -> String {
    let rel = module.split_once("::").map(|(_, r)| r).unwrap_or(module);
    format!("{rel}::child_entry")
}

fn child_command(module: &str, scenario: &str, profile: &Path) -> Command {
    let exe = std::env::current_exe().expect("the test binary");
    let mut c = Command::new(exe);
    c.args(["--exact", &child_test_name(module), "--ignored", "--nocapture", "--test-threads=1"])
        .env(CHILD_VAR, scenario)
        .env("SOVEREIGN_DATA_DIR", profile)
        .env_remove("SHELL_NO_AUTH")
        .env_remove("SHELL_AUTH_AUTO")
        .env_remove("SHELL_OPEN_WIZARD")
        .env_remove("SHELL_OPEN_JOIN");
    c
}

fn tail(s: &str) -> &str {
    let n = s.len();
    let mut at = n.saturating_sub(6000);
    while !s.is_char_boundary(at) {
        at += 1;
    }
    &s[at..]
}

/// Runs one launch (`scenario`) in a child process on `profile` and panics
/// with its output unless it ran and passed.
pub(super) fn launch(module: &str, scenario: &str, profile: &Path) {
    let out: Output = child_command(module, scenario, profile).output().expect("spawn the child launch");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success() && stdout.contains("1 passed"),
        "launch `{scenario}` on {} did not pass (status {:?}).\n--- stdout (tail)\n{}\n--- stderr (tail)\n{}",
        profile.display(),
        out.status,
        tail(&stdout),
        tail(&stderr),
    );
}

/// Runs `scenario` in a child process and **kills** it as soon as it prints
/// a line containing `marker`. Panics if it never does.
fn launch_and_kill_at(module: &str, scenario: &str, profile: &Path, marker: &str) {
    let mut child = child_command(module, scenario, profile)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn the child launch");
    let stderr = child.stderr.take().unwrap();
    let err_reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = std::io::Read::read_to_string(&mut BufReader::new(stderr), &mut s);
        s
    });
    let mut seen = String::new();
    let mut found = false;
    for line in BufReader::new(child.stdout.take().unwrap()).lines() {
        let Ok(line) = line else { break };
        seen.push_str(&line);
        seen.push('\n');
        if line.contains(marker) {
            found = true;
            break;
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    let err = err_reader.join().unwrap_or_default();
    assert!(
        found,
        "launch `{scenario}` never reached `{marker}`.\n--- stdout (tail)\n{}\n--- stderr (tail)\n{}",
        tail(&seen),
        tail(&err)
    );
}

pub(super) fn profile() -> tempfile::TempDir {
    tempfile::Builder::new().prefix("sov-first-run-").tempdir().expect("a temporary profile")
}

/// Stops the App's runtime, waiting at most 5 s, before the test forgets the App.
///
/// A forgotten App kept its runtime alive, so its tasks went on querying RocksDB while `exit()`
/// destroyed RocksDB's statics: "pure virtual method called", status 134, in 1 to 3 of 11
/// child launches on Linux (read from a core by WSL, claude-coord 0028). The App itself is still
/// forgotten, as before. An idle runtime takes the old one's place.
pub(super) fn shut_runtime(app: &mut App) {
    let idle = tokio::runtime::Builder::new_current_thread().build().expect("an idle runtime");
    std::mem::replace(&mut app.rt, idle).shutdown_timeout(Duration::from_secs(5));
}

/// Draws frames until `done` holds or `secs` pass; whether it held.
pub(super) fn frames_until(app: &mut App, secs: u64, mut done: impl FnMut(&App) -> bool) -> bool {
    let end = Instant::now() + Duration::from_secs(secs);
    loop {
        app.build_scene(W, H);
        if done(app) {
            return true;
        }
        if Instant::now() >= end {
            return false;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
}

/// A setup has an outcome: unlocked, or an error on the wizard, or (B5b) the
/// wizard gone and an error on the auth form.
pub(super) fn setup_settled(app: &App) -> bool {
    !app.locked
        || match &app.wizard {
            Some(w) => w.error.is_some(),
            None => app.auth_form.error.is_some(),
        }
}

pub(super) fn login_settled(app: &App) -> bool {
    !app.locked || app.auth_form.error.is_some()
}

/// The first frame of a launch; on a fresh profile it starts the wizard.
pub(super) fn first_frame(app: &mut App) {
    app.build_scene(W, H);
}

/// Fills the wizard as a user who reached the last step would, and sits it
/// on the last step (the safety phrase, left empty).
pub(super) fn fill_wizard(app: &mut App, password: &str, duress: &str) {
    let w = app.wizard.as_mut().expect("the setup wizard is shown on a fresh profile");
    w.nickname = "Ada".into();
    w.password = password.into();
    w.password_confirm = password.into();
    w.duress = duress.into();
    w.duress_confirm = duress.into();
    w.canary.clear();
    w.canary_confirm.clear();
    w.seed_sample = false;
    w.step = STEP_CANARY;
    w.focus = 0;
    w.error = None;
}

/// The last step's primary button ("Get started"), as its click does.
pub(super) fn press_finish(app: &mut App) {
    app.handle_wiz_action(WizAction::Finish);
}

/// Types `password` into the login form and submits it, as Enter does.
pub(super) fn submit_login(app: &mut App, password: &str) {
    assert!(app.wizard.is_none(), "a profile with auth.store shows the login form, not the wizard");
    assert!(!app.auth_form.joining && app.auth_form.fields.len() == 1, "the login form is shown");
    app.auth_form.fields[0] = password.into();
    app.auth_form.focus = 0;
    app.auth_submit();
}

/// What the notice on screen says (title and body), if any.
pub(super) fn notice_text(app: &App) -> Option<String> {
    app.notice.as_ref().map(|n| format!("{}\n{}", n.said.0, n.said.1))
}

// ------------------------------------------------------------- the a11y tree

pub(super) fn is_field(role: AkRole) -> bool {
    matches!(
        role,
        AkRole::TextInput
            | AkRole::MultilineTextInput
            | AkRole::PasswordInput
            | AkRole::SearchInput
            | AkRole::EmailInput
            | AkRole::NumberInput
            | AkRole::PhoneNumberInput
            | AkRole::UrlInput
    )
}

pub(super) fn is_button(role: AkRole) -> bool {
    matches!(role, AkRole::Button | AkRole::DefaultButton)
}

/// A node as read by these tests.
#[derive(Clone, Debug)]
pub(super) struct Seen {
    pub(super) id: NodeId,
    pub(super) role: AkRole,
    pub(super) label: Option<String>,
    pub(super) value: Option<String>,
    pub(super) description: Option<String>,
    /// What Narrator receives for it (`crate::a11y_spoken::spoken_name`).
    pub(super) spoken: String,
}

impl Seen {
    fn of(id: NodeId, n: &Node, nodes: &[(NodeId, Node)]) -> Self {
        Seen {
            id,
            role: n.role(),
            label: n.label().map(str::to_string),
            value: n.value().map(str::to_string),
            description: n.description().map(str::to_string),
            spoken: crate::a11y_spoken::spoken_name(nodes, id),
        }
    }
    /// The node's name as Narrator receives it (amended, see the header).
    pub(super) fn name(&self) -> &str {
        &self.spoken
    }
}

/// The locked screen's tree as a screen reader receives it: every node of
/// the update (`all`), the nodes reachable from the root in document order
/// (`order`), and the focused node.
pub(super) struct Read {
    pub(super) all: Vec<Seen>,
    pub(super) order: Vec<Seen>,
    pub(super) focus: NodeId,
}

impl Read {
    pub(super) fn fields(&self) -> Vec<&Seen> {
        self.order.iter().filter(|s| is_field(s.role)).collect()
    }
    pub(super) fn field_names(&self) -> Vec<String> {
        self.fields().iter().map(|s| s.name().to_string()).collect()
    }
    pub(super) fn button_names(&self) -> Vec<String> {
        self.order.iter().filter(|s| is_button(s.role)).map(|s| s.name().to_string()).collect()
    }
    pub(super) fn names_with_role(&self, role: AkRole) -> Vec<String> {
        self.order.iter().filter(|s| s.role == role).map(|s| s.name().to_string()).collect()
    }
    /// Whether a node is named `name`, spoken or by its label property
    /// (used to say a text is absent: either way counts, see the header).
    pub(super) fn has_name(&self, name: &str) -> bool {
        self.all.iter().any(|s| s.name() == name || s.label.as_deref() == Some(name))
    }
    pub(super) fn has_non_field_named(&self, name: &str) -> bool {
        self.all.iter().any(|s| !is_field(s.role) && s.name() == name)
    }
    pub(super) fn focused(&self) -> Option<&Seen> {
        self.all.iter().find(|s| s.id == self.focus)
    }
    /// Every string the tree carries (labels, values, descriptions).
    pub(super) fn every_string(&self) -> Vec<String> {
        let mut v = Vec::new();
        for s in &self.all {
            v.extend(s.label.clone());
            v.extend(s.value.clone());
            v.extend(s.description.clone());
        }
        v
    }
    pub(super) fn dump(&self) -> String {
        self.order
            .iter()
            .map(|s| format!("{:?} {:?} = {:?}", s.role, s.name(), s.value))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub(super) fn read_tree(app: &mut App) -> Read {
    let up: TreeUpdate = app.build_a11y_tree();
    let all: Vec<Seen> = up.nodes.iter().map(|(id, n)| Seen::of(*id, n, &up.nodes)).collect();
    let root = up.tree.as_ref().map(|t| t.root).unwrap_or(NodeId(0));
    let mut order = Vec::new();
    let mut stack = vec![root];
    let mut visited = std::collections::HashSet::new();
    while let Some(id) = stack.pop() {
        if !visited.insert(id) {
            continue;
        }
        if let Some((_, n)) = up.nodes.iter().find(|(i, _)| *i == id) {
            order.push(Seen::of(id, n, &up.nodes));
            for c in n.children().iter().rev() {
                stack.push(*c);
            }
        }
    }
    Read { all, order, focus: up.focus }
}

// ======================================================= A1–A5 (in-process)

/// Secrets typed in these screens; none may appear in the tree.
const WIZ_PASSWORD: &str = "Wz!Secret-Pass1";
const WIZ_DURESS: &str = "Dz!Secret-Pass2";
const HIDDEN_FORM_SECRET: &str = "Hidden!AuthForm9";
const LOGIN_SECRET: &str = "Login!Secret-77";
const JOIN_PASSWORD: &str = "Join!Secret-Pass3";
const JOIN_DURESS: &str = "Join!Duress-Pass4";

/// The titles `onboarding::step_copy` draws, per step.
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

/// The labels `onboarding::draw_fields` draws above each field, per step.
fn step_labels(step: usize) -> Vec<&'static str> {
    match step {
        // v0.0.12 item 19, N2: a second field, this device's name.
        STEP_NICKNAME => vec!["Nickname", "This device's name"],
        STEP_PASSWORD => vec!["Password", "Confirm password"],
        STEP_DURESS => vec!["Duress password", "Confirm duress password"],
        STEP_CANARY => vec!["Canary phrase", "Confirm phrase"],
        _ => vec![],
    }
}

/// The buttons drawn on each step: Welcome's two choices; then Back, Skip
/// (duress and safety phrase), and the primary ("Get started" on the last).
fn step_buttons(step: usize) -> Vec<&'static str> {
    match step {
        0 => vec!["Set up this device", "Pair with an existing device"],
        STEP_DURESS => vec!["Back", "Skip", "Next"],
        STEP_CANARY => vec!["Back", "Skip", "Get started"],
        _ => vec!["Back", "Next"],
    }
}

/// A locked shell showing the wizard at `step`, with every field filled and
/// the hidden auth form holding a secret of its own.
fn wizard_screen(step: usize, reveal: bool) -> App {
    ensure_test_data_dir();
    let mut app = App::new();
    app.locked = true;
    let mut hidden = AuthForm::login();
    hidden.fields[0] = HIDDEN_FORM_SECRET.into();
    app.auth_form = hidden;
    let mut w = OnboardingWizard::new("Designation-7".into(), false);
    w.step = step;
    w.nickname = "Ada".into();
    w.password = WIZ_PASSWORD.into();
    w.password_confirm = WIZ_PASSWORD.into();
    w.duress = WIZ_DURESS.into();
    w.duress_confirm = WIZ_DURESS.into();
    w.canary = "blue heron".into();
    w.canary_confirm = "blue heron".into();
    w.reveal = reveal;
    w.focus = 0;
    app.wizard = Some(w);
    app
}

fn login_screen(reveal: bool) -> App {
    ensure_test_data_dir();
    let mut app = App::new();
    app.locked = true;
    app.wizard = None;
    let mut f = AuthForm::login();
    f.fields[0] = LOGIN_SECRET.into();
    f.reveal = reveal;
    app.auth_form = f;
    app
}

fn join_screen(reveal: bool) -> App {
    ensure_test_data_dir();
    let mut app = App::new();
    app.locked = true;
    app.wizard = None;
    let mut f = AuthForm::join();
    f.fields[0] = String::new();
    f.fields[1] = "482913".into();
    f.fields[2] = JOIN_PASSWORD.into();
    f.fields[3] = JOIN_DURESS.into();
    f.reveal = reveal;
    app.auth_form = f;
    app
}

const SECRETS: [&str; 6] = [WIZ_PASSWORD, WIZ_DURESS, HIDDEN_FORM_SECRET, LOGIN_SECRET, JOIN_PASSWORD, JOIN_DURESS];

fn assert_no_secret(t: &Read, what: &str) {
    for s in t.every_string() {
        for secret in SECRETS {
            assert!(!s.contains(secret), "{what}: the tree carries a secret's text ({s:?})\n{}", t.dump());
        }
    }
}

fn assert_no_empty_name(t: &Read, what: &str) {
    for s in &t.all {
        assert!(
            !s.name().trim().is_empty(),
            "{what}: node {:?} ({:?}) has an empty name (A4)\n{}",
            s.id,
            s.role,
            t.dump()
        );
    }
}

fn assert_unique_field_names(t: &Read, what: &str) {
    let names = t.field_names();
    let mut seen = std::collections::HashSet::new();
    for n in &names {
        assert!(seen.insert(n.clone()), "{what}: two fields share the name {n:?} (A2): {names:?}");
    }
}

fn assert_masked_value(s: &Seen, len: usize, what: &str) {
    let ok = if len == 0 {
        s.value.is_none() || s.value.as_deref() == Some("0 characters") || s.value.as_deref() == Some("")
    } else {
        s.value.as_deref() == Some(format!("{len} characters").as_str())
    };
    assert!(ok, "{what}: masked field {:?} must expose its length only ({len} characters), got {:?} (A3)", s.name(), s.value);
}

// ---- A1: the tree describes what is drawn

#[test]
fn a1_while_the_wizard_is_shown_the_tree_is_the_wizards_current_step() {
    for step in 0..WIZ_STEPS {
        let mut app = wizard_screen(step, false);
        let t = read_tree(&mut app);
        let what = format!("wizard step {step}");
        assert!(
            t.has_non_field_named(STEP_TITLES[step]),
            "{what}: the step's title {:?} is in the tree (A1)\n{}",
            STEP_TITLES[step],
            t.dump()
        );
        let expected: Vec<String> = step_labels(step).iter().map(|s| s.to_string()).collect();
        assert_eq!(t.field_names(), expected, "{what}: the tree's fields are the step's, in drawn order (A1, A2)\n{}", t.dump());
        let buttons = t.button_names();
        for b in step_buttons(step) {
            assert!(buttons.iter().any(|n| n == b), "{what}: the drawn button {b:?} is in the tree (A1): {buttons:?}\n{}", t.dump());
        }
    }
}

#[test]
fn a1_while_the_wizard_is_shown_the_hidden_auth_form_is_not_in_the_tree() {
    for step in 0..WIZ_STEPS {
        let mut app = wizard_screen(step, false);
        let t = read_tree(&mut app);
        let what = format!("wizard step {step}");
        for title in ["Unlock Sovereign", "Join from another device"] {
            assert!(!t.has_name(title), "{what}: the hidden auth form's title {title:?} is in the tree (A1)\n{}", t.dump());
        }
        assert_no_secret(&t, &what);
    }
    // The wizard over a hidden *join* form: none of the join form's fields.
    let mut app = wizard_screen(STEP_PASSWORD, false);
    app.auth_form = AuthForm::join();
    let t = read_tree(&mut app);
    for label in AuthForm::join().labels {
        assert!(!t.has_name(label), "wizard over a hidden join form: its field {label:?} is in the tree (A1)\n{}", t.dump());
    }
}

#[test]
fn a1_a_wizard_error_that_is_drawn_is_in_the_tree() {
    let mut app = wizard_screen(STEP_DURESS, false);
    app.wizard.as_mut().unwrap().error = Some("Duress passwords don't match.".into());
    let t = read_tree(&mut app);
    assert!(
        t.every_string().iter().any(|s| s.contains("Duress passwords don't match.")),
        "the wizard's drawn error is in the tree (A1)\n{}",
        t.dump()
    );
}

#[test]
fn a1_the_login_form_is_described_as_drawn() {
    let mut app = login_screen(false);
    let t = read_tree(&mut app);
    assert!(t.has_non_field_named("Unlock Sovereign"), "the login form's title (A1)\n{}", t.dump());
    assert_eq!(t.field_names(), vec!["Password".to_string()], "the login form's one field, by its label (A1, A2)\n{}", t.dump());
    for title in &STEP_TITLES[1..] {
        assert!(!t.has_name(title), "the login form's tree carries the wizard's {title:?} (A1)\n{}", t.dump());
    }
    let mut app = login_screen(false);
    app.auth_form.error = Some("Invalid password".into());
    let t = read_tree(&mut app);
    assert!(t.every_string().iter().any(|s| s.contains("Invalid password")), "the drawn error is in the tree (A1)\n{}", t.dump());
}

#[test]
fn a1_the_join_form_is_described_as_drawn() {
    let mut app = join_screen(false);
    let t = read_tree(&mut app);
    assert!(t.has_non_field_named("Join from another device"), "the join form's title (A1)\n{}", t.dump());
    let expected: Vec<String> = AuthForm::join().labels.iter().map(|s| s.to_string()).collect();
    assert_eq!(t.field_names(), expected, "the join form's fields, by their drawn labels (A1, A2)\n{}", t.dump());
    assert!(!t.has_name("Unlock Sovereign"), "the join form's tree is not the login form's (A1)\n{}", t.dump());
}

// ---- A2: no two fields share a name

#[test]
fn a2_no_two_fields_share_a_name_on_any_locked_screen() {
    for step in 0..WIZ_STEPS {
        let mut app = wizard_screen(step, false);
        assert_unique_field_names(&read_tree(&mut app), &format!("wizard step {step}"));
    }
    assert_unique_field_names(&read_tree(&mut login_screen(false)), "login form");
    assert_unique_field_names(&read_tree(&mut join_screen(false)), "join form");
}

// ---- A3: secrets stay secret, "show" on or off

#[test]
fn a3_masked_wizard_fields_expose_their_length_only_even_with_show_on() {
    for reveal in [false, true] {
        for step in [STEP_PASSWORD, STEP_DURESS] {
            let mut app = wizard_screen(step, reveal);
            let t = read_tree(&mut app);
            let what = format!("wizard step {step}, show {reveal}");
            let secret = if step == STEP_PASSWORD { WIZ_PASSWORD } else { WIZ_DURESS };
            let fields = t.fields();
            assert_eq!(fields.len(), 2, "{what}: two fields\n{}", t.dump());
            for f in fields {
                assert_masked_value(f, secret.chars().count(), &what);
            }
            assert_no_secret(&t, &what);
        }
    }
    // An empty masked field announces no length but its own.
    let mut app = wizard_screen(STEP_PASSWORD, true);
    app.wizard.as_mut().unwrap().password_confirm.clear();
    let t = read_tree(&mut app);
    let fields = t.fields();
    assert_eq!(fields.len(), 2, "two fields\n{}", t.dump());
    assert_masked_value(fields[1], 0, "empty confirmation");
}

// ---- K1, K2: the canary phrase is a secret (Céline, 2026-10-28), entered like a password

/// The phrase `wizard_screen` types in both canary fields.
const WIZ_CANARY: &str = "blue heron";

#[test]
fn k2_the_canary_fields_expose_their_length_only_even_with_show_on() {
    for reveal in [false, true] {
        let mut app = wizard_screen(STEP_CANARY, reveal);
        let t = read_tree(&mut app);
        let what = format!("canary step, show {reveal}");
        let fields = t.fields();
        assert_eq!(fields.len(), 2, "{what}: two fields\n{}", t.dump());
        for f in &fields {
            assert_eq!(f.role, AkRole::PasswordInput, "{what}: {:?} is a password input (K2)\n{}", f.name(), t.dump());
            assert_masked_value(f, WIZ_CANARY.chars().count(), &what);
        }
        for s in t.every_string() {
            assert!(!s.contains(WIZ_CANARY), "{what}: the tree carries the phrase ({s:?}) (K2)\n{}", t.dump());
        }
        assert_no_secret(&t, &what);
    }
}

#[test]
fn k1_k2_the_canary_step_has_the_wizards_show_toggle_named_for_the_phrase() {
    for (reveal, name) in [(false, "Show phrase"), (true, "Hide phrase")] {
        let mut app = wizard_screen(STEP_CANARY, reveal);
        let t = read_tree(&mut app);
        let buttons = t.button_names();
        assert!(
            buttons.iter().any(|b| b == name),
            "canary step, show {reveal}: the toggle {name:?} is in the tree (K1, K2): {buttons:?}\n{}",
            t.dump()
        );
        assert!(
            !buttons.iter().any(|b| b.contains("passwords")),
            "canary step, show {reveal}: the toggle is named for the phrase, not the passwords (K2): {buttons:?}"
        );
    }
    // The password steps keep their own names.
    for step in [STEP_PASSWORD, STEP_DURESS] {
        let t = read_tree(&mut wizard_screen(step, false));
        assert!(t.button_names().iter().any(|b| b == "Show passwords"), "wizard step {step}: \"Show passwords\" unchanged\n{}", t.dump());
    }
}

#[test]
fn a3_masked_login_and_join_fields_expose_their_length_only_even_with_show_on() {
    for reveal in [false, true] {
        let mut app = login_screen(reveal);
        let t = read_tree(&mut app);
        let fields = t.fields();
        assert_eq!(fields.len(), 1, "login, show {reveal}\n{}", t.dump());
        assert_masked_value(fields[0], LOGIN_SECRET.chars().count(), &format!("login, show {reveal}"));
        assert_no_secret(&t, &format!("login, show {reveal}"));

        let mut app = join_screen(reveal);
        let t = read_tree(&mut app);
        let fields = t.fields();
        // v0.0.12 item 19, N2: a fifth field, this device's name, not masked.
        assert_eq!(fields.len(), 5, "join, show {reveal}\n{}", t.dump());
        assert_masked_value(fields[2], JOIN_PASSWORD.chars().count(), &format!("join, show {reveal}"));
        assert_masked_value(fields[3], JOIN_DURESS.chars().count(), &format!("join, show {reveal}"));
        assert_eq!(fields[4].role, AkRole::TextInput, "join, show {reveal}: the device's name is not masked (N2)\n{}", t.dump());
        assert_no_secret(&t, &format!("join, show {reveal}"));
    }
}

// ---- A4: no empty names

#[test]
fn a4_no_node_of_any_locked_screen_has_an_empty_name() {
    for reveal in [false, true] {
        for step in 0..WIZ_STEPS {
            let mut app = wizard_screen(step, reveal);
            assert_no_empty_name(&read_tree(&mut app), &format!("wizard step {step}, show {reveal}"));
            let mut app = wizard_screen(step, reveal);
            app.wizard.as_mut().unwrap().error = Some("Something to say.".into());
            assert_no_empty_name(&read_tree(&mut app), &format!("wizard step {step} with an error"));
        }
        assert_no_empty_name(&read_tree(&mut login_screen(reveal)), "login form");
        assert_no_empty_name(&read_tree(&mut join_screen(reveal)), "join form");
    }
    // Empty fields still have names.
    let mut app = login_screen(false);
    app.auth_form.fields[0].clear();
    assert_no_empty_name(&read_tree(&mut app), "empty login form");
    let mut app = join_screen(false);
    for f in app.auth_form.fields.iter_mut() {
        f.clear();
    }
    assert_no_empty_name(&read_tree(&mut app), "empty join form");
}

// ---- A5: the focused field is the focused node

#[test]
fn a5_the_focused_field_is_the_focused_node() {
    for step in [STEP_NICKNAME, STEP_PASSWORD, STEP_DURESS, STEP_CANARY] {
        for focus in 0..step_labels(step).len() {
            let mut app = wizard_screen(step, false);
            app.wizard.as_mut().unwrap().focus = focus;
            let t = read_tree(&mut app);
            let f = t.focused();
            assert!(
                f.is_some_and(|f| is_field(f.role) && f.name() == step_labels(step)[focus]),
                "wizard step {step}, field {focus} focused: the focused node is {:?} (A5)\n{}",
                f.map(|f| (f.role, f.name().to_string())),
                t.dump()
            );
        }
    }
    let mut app = login_screen(false);
    let t = read_tree(&mut app);
    let f = t.focused();
    assert!(f.is_some_and(|f| is_field(f.role) && f.name() == "Password"), "login: focused {:?} (A5)", f.map(|f| f.name().to_string()));
    for focus in 0..4 {
        let mut app = join_screen(false);
        app.auth_form.focus = focus;
        let t = read_tree(&mut app);
        let f = t.focused();
        let want = AuthForm::join().labels[focus];
        assert!(
            f.is_some_and(|f| is_field(f.role) && f.name() == want),
            "join, field {focus} focused: the focused node is {:?}, not {want:?} (A5)",
            f.map(|f| f.name().to_string())
        );
    }
}

// ============================================== "Decided": source scans

/// `code` without its `#[cfg(test)] mod … { … }` blocks.
fn strip_test_mods(code: &str) -> String {
    let mut out = code.to_string();
    let mut from = 0;
    while let Some(at) = out[from..].find("#[cfg(test)]").map(|a| a + from) {
        let mut rest = at + "#[cfg(test)]".len();
        // Skip whitespace and further attributes.
        loop {
            let t = out[rest..].trim_start();
            rest = out.len() - t.len();
            if t.starts_with("#[") {
                rest += t.find(']').map(|e| e + 1).unwrap_or(t.len());
            } else {
                break;
            }
        }
        let t = &out[rest..];
        let is_mod = t.starts_with("mod ") || t.starts_with("pub mod ") || t.starts_with("pub(crate) mod ");
        let brace = t.find('{');
        let semi = t.find(';');
        match (is_mod, brace) {
            (true, Some(b)) if semi.map_or(true, |s| b < s) => {
                let open = rest + b;
                let close = matching_brace(&out, open);
                let blank: String = out[at..=close].chars().map(|c| if c == '\n' { '\n' } else { ' ' }).collect();
                out.replace_range(at..=close, &blank);
                from = at + blank.len();
            }
            _ => from = rest,
        }
    }
    out
}

fn matching_brace(code: &str, open: usize) -> usize {
    let b = code.as_bytes();
    let (mut depth, mut i, mut in_str) = (0usize, open, false);
    while i < b.len() {
        match b[i] {
            b'\\' if in_str => i += 1,
            b'"' => in_str = !in_str,
            // A char literal: 'x' or '\x'.
            b'\'' if !in_str && b.get(i + 2) == Some(&b'\'') => i += 2,
            b'\'' if !in_str && b.get(i + 1) == Some(&b'\\') && b.get(i + 3) == Some(&b'\'') => i += 3,
            b'{' if !in_str => depth += 1,
            b'}' if !in_str => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
        i += 1;
    }
    panic!("unbalanced braces from byte {open}");
}

/// Every production `.rs` file of the shell (no `*_tests.rs`, no test
/// modules, comments blanked), with its path relative to `src/`.
fn production_files() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = Vec::new();
    let mut dirs = vec![root.clone()];
    while let Some(d) = dirs.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                dirs.push(p);
                continue;
            }
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            if !name.ends_with(".rs") || name.ends_with("_tests.rs") {
                continue;
            }
            let rel = p.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
            let src = std::fs::read_to_string(&p).unwrap();
            out.push((rel, strip_test_mods(&crate::field_contrast_tests::blank_line_comments(&src))));
        }
    }
    assert!(out.len() >= 20, "scanned only {} files", out.len());
    out
}

fn production(rel: &str) -> String {
    production_files().into_iter().find(|(r, _)| r == rel).map(|(_, c)| c).unwrap_or_else(|| panic!("src/{rel} not found"))
}

#[test]
fn decided_try_onboard_and_the_auth_forms_onboarding_mode_no_longer_exist() {
    let mut problems = Vec::new();
    for (rel, code) in production_files() {
        for needle in ["try_onboard(", "AuthForm::onboarding("] {
            if code.contains(needle) {
                problems.push(format!("src/{rel}: `{needle}` outside tests"));
            }
        }
    }
    if production("panels.rs").contains("fn onboarding(") {
        problems.push("src/panels.rs: `fn onboarding(` (AuthForm's onboarding mode) still exists".into());
    }
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}

#[test]
fn decided_shell_auth_auto_fills_the_wizard_and_goes_through_finish_onboarding() {
    let code = production("app.rs");
    let at = code.find("dev::flag(\"SHELL_AUTH_AUTO\")").expect("the SHELL_AUTH_AUTO helper in app.rs");
    let open = at + code[at..].find('{').expect("its block");
    let block = &code[open..=matching_brace(&code, open)];
    assert!(block.contains("finish_onboarding("), "SHELL_AUTH_AUTO's setup goes through finish_onboarding:\n{block}");
    assert!(block.contains("wizard"), "SHELL_AUTH_AUTO fills the wizard:\n{block}");
    assert!(!block.contains("try_onboard"), "SHELL_AUTH_AUTO no longer calls try_onboard:\n{block}");
}

// ======================================================= B1: source reading

/// app.rs's production code without the development probes (which drive the
/// crypto directly, never from the window's event handler).
fn app_ui_code() -> String {
    let mut code = production("app.rs");
    for probe in ["run_auth_probe", "run_chat_probe"] {
        if code.contains(&format!("fn {probe}(")) {
            let body = crate::field_contrast_tests::fn_body(&code, probe, "src/app.rs");
            code = code.replacen(&body, "{}", 1);
        }
    }
    code
}

#[test]
fn b1_app_blocks_on_no_key_derivation() {
    let squeezed: String = app_ui_code().chars().filter(|c| !c.is_whitespace()).collect();
    let mut problems = Vec::new();
    for needle in [
        "block_on(install_session(",
        "block_on(crate::crypto::install_session(",
        "block_on(recover_and_install(",
        "block_on(crate::crypto::recover_and_install(",
    ] {
        let n = squeezed.matches(needle).count();
        if n > 0 {
            problems.push(format!("app.rs: {n} × `{needle}` — the derivation blocks the UI thread (B1)"));
        }
    }
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}

#[test]
fn b1_the_ui_paths_call_no_derivation_themselves() {
    // The real calls live behind the worker seam (`crate::auth_work`, see
    // `first_run_worker_tests.rs`); `App`'s methods on the UI path name none.
    let code = app_ui_code();
    let mut problems = Vec::new();
    for f in ["finish_onboarding", "try_login", "auth_submit", "handle_wiz_action", "wiz_enter", "build_scene", "recovery_finalize"] {
        if !code.contains(&format!("fn {f}(")) {
            assert!(f == "recovery_finalize", "app.rs: `fn {f}(` not found");
            continue;
        }
        let body = crate::field_contrast_tests::fn_body(&code, f, "src/app.rs");
        for needle in ["install_session(", "create_auth_store(", "recover_and_install(", "AuthStore::create("] {
            if body.contains(needle) {
                problems.push(format!("app.rs `{f}`: calls `{needle}` on the UI thread (B1)"));
            }
        }
    }
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}

/// The scanner on known text, so a green result means something.
#[test]
fn the_scanners_find_what_they_look_for() {
    let src = "fn a() { x(); }\n#[cfg(test)]\nmod t {\n    fn b() { try_onboard(); }\n}\nfn c() { y(); }\n";
    let s = strip_test_mods(src);
    assert!(!s.contains("try_onboard"), "{s}");
    assert!(s.contains("fn c()"), "{s}");
    let decl = "#[cfg(test)]\n#[path = \"x_tests.rs\"]\nmod x_tests;\nfn d() { try_onboard(); }\n";
    assert!(strip_test_mods(decl).contains("try_onboard"), "a declaration is not a test body");
    let squeezed: String = "self.rt.block_on(\n  install_session(&s, p))".chars().filter(|c| !c.is_whitespace()).collect();
    assert!(squeezed.contains("block_on(install_session("));
}

// ================================================ B5, B5b, P4 (child launches)

fn module() -> &'static str {
    module_path!()
}

/// One launch of the app on the profile of `SOVEREIGN_DATA_DIR` (set by the
/// parent). Ignored in ordinary runs.
#[test]
#[ignore = "a launch, run by its parent test in a child process on a profile of its own"]
fn child_entry() {
    let Ok(scenario) = std::env::var(CHILD_VAR) else { return };
    match scenario.as_str() {
        "store_then_kill" => store_then_kill(),
        "database_then_kill" => database_then_kill(),
        "store_only" => store_only(),
        "fresh_profile_shows_setup" => fresh_profile_shows_setup(),
        "login_primary_opens" => login_opens(PRIMARY, sovereign_core::auth::PersonaKind::Primary),
        "login_duress_opens" => login_opens(DURESS, sovereign_core::auth::PersonaKind::Duress),
        "setup_completes" => setup_completes(),
        "failure_after_store" => failure_after_store(),
        "login_primary_notice" => login_and_notice(PRIMARY, true),
        "login_primary_no_notice" => login_and_notice(PRIMARY, false),
        "login_duress_notice" => login_and_notice(DURESS, true),
        "login_duress_no_notice" => login_and_notice(DURESS, false),
        "duress_equals_main" => duress_equals_main(),
        "setup_killed_after_store" => setup_killed_after_store(),
        "join_completes" => {
            let mut app = App::new();
            first_frame(&mut app);
            super::first_run_worker_tests::complete_a_join_faked(&mut app);
            shut_runtime(&mut app);
            std::mem::forget(app);
        }
        "recovery_completes" => {
            let mut app = App::new();
            first_frame(&mut app);
            super::first_run_worker_tests::complete_a_recovery_faked(&mut app);
            shut_runtime(&mut app);
            std::mem::forget(app);
        }
        other => panic!("unknown scenario {other}"),
    }
}

fn data_dir() -> PathBuf {
    sovereign_core::sovereign_dir()
}

/// Writes auth.store as first-device setup does, then waits to be killed.
fn store_then_kill() {
    crate::crypto::create_auth_store(PRIMARY.as_bytes(), DURESS.as_bytes()).expect("create auth.store");
    assert!(crate::crypto::auth_store_exists());
    println!("\nFIRST_RUN_STORE_WRITTEN");
    std::thread::sleep(Duration::from_secs(600));
}

/// Writes auth.store, then runs `install_session`'s steps up to the persona
/// database (authenticate, open the raw database) and waits to be killed
/// there — before the key databases and the index key are written.
fn database_then_kill() {
    let store = crate::crypto::create_auth_store(PRIMARY.as_bytes(), DURESS.as_bytes()).expect("create auth.store");
    let auth = store.authenticate(PRIMARY.as_bytes()).expect("authenticate");
    let persona = crate::crypto::map_persona(auth.persona);
    let rt = tokio::runtime::Runtime::new().unwrap();
    let db = rt.block_on(crate::canvas::open_db_at(&crate::crypto::persona_raw_db_path(persona)));
    assert!(db.is_some(), "the persona database opens");
    println!("\nFIRST_RUN_DATABASE_OPENED");
    std::thread::sleep(Duration::from_secs(600));
}

/// A real first-device setup (the wizard's last button), killed once
/// auth.store exists. Frames stop as soon as the store is seen, so nothing
/// the UI thread does after the store (the profile, the safety phrase, the
/// setup-complete marker) runs before the kill; whatever `finish_onboarding`
/// records before the store (the amended B5b's "setup started") is there.
fn setup_killed_after_store() {
    assert!(!crate::crypto::auth_store_exists(), "precondition: a fresh profile");
    let mut app = App::new();
    first_frame(&mut app);
    fill_wizard(&mut app, PRIMARY, DURESS);
    press_finish(&mut app);
    let end = Instant::now() + Duration::from_secs(120);
    while !crate::crypto::auth_store_exists() {
        assert!(Instant::now() < end, "the setup never wrote auth.store");
        assert!(app.locked, "the setup ended before the store could be seen");
        app.build_scene(W, H);
        std::thread::sleep(Duration::from_millis(2));
    }
    println!("\nFIRST_RUN_SETUP_STORE_WRITTEN");
    std::thread::sleep(Duration::from_secs(600));
}

/// auth.store written the old way (`create_auth_store` alone), and nothing
/// else: an account made before this build — no "setup started" record, no
/// setup-complete marker. (Until ff959de this stood for an interrupted setup.)
fn store_only() {
    crate::crypto::create_auth_store(PRIMARY.as_bytes(), DURESS.as_bytes()).expect("create auth.store");
}

fn fresh_profile_shows_setup() {
    assert!(!crate::crypto::auth_store_exists(), "precondition: a fresh profile");
    let mut app = App::new();
    first_frame(&mut app);
    assert!(app.locked && app.wizard.is_some(), "a profile without auth.store shows the setup again (B5)");
    shut_runtime(&mut app);
    std::mem::forget(app);
}

fn login_opens(password: &str, persona: sovereign_core::auth::PersonaKind) {
    assert!(crate::crypto::auth_store_exists(), "precondition: auth.store was written before the kill");
    let mut app = App::new();
    first_frame(&mut app);
    assert!(app.wizard.is_none(), "a profile with auth.store shows the login form (B5)");
    submit_login(&mut app, password);
    assert!(frames_until(&mut app, 240, login_settled), "the login ends");
    assert!(!app.locked, "the account opens with its password (B5): {:?}", app.auth_form.error);
    assert_eq!(app.persona, Some(persona), "the persona of that password");
    shut_runtime(&mut app);
    std::mem::forget(app);
}

fn setup_completes() {
    let mut app = App::new();
    first_frame(&mut app);
    fill_wizard(&mut app, PRIMARY, DURESS);
    press_finish(&mut app);
    assert!(frames_until(&mut app, 240, setup_settled), "the setup ends");
    assert!(!app.locked, "setup unlocks: {:?}", app.wizard.as_ref().and_then(|w| w.error.clone()));
    frames_until(&mut app, 1, |_| false);
    shut_runtime(&mut app);
    std::mem::forget(app);
}

fn failure_after_store() {
    // A key database the session cannot read: install_session fails after
    // create_auth_store has written the store.
    let crypto = data_dir().join("crypto");
    std::fs::create_dir_all(&crypto).unwrap();
    std::fs::write(crypto.join("keys.db"), b"not a key database").unwrap();

    let mut app = App::new();
    first_frame(&mut app);
    fill_wizard(&mut app, PRIMARY, DURESS);
    press_finish(&mut app);
    assert!(frames_until(&mut app, 240, setup_settled), "the setup ends");
    assert!(crate::crypto::auth_store_exists(), "precondition: the store was written before the failure");
    assert!(app.locked, "a failed installation leaves the screen locked (B4)");
    assert!(
        app.wizard.is_none(),
        "once the store is written, a later failure drops the wizard (B5b); the wizard shows {:?} at step {:?}",
        app.wizard.as_ref().and_then(|w| w.error.clone()),
        app.wizard.as_ref().map(|w| w.step)
    );
    assert!(
        !app.auth_form.joining && app.auth_form.labels == vec!["Password"],
        "the login form is shown (B5b): {:?}",
        app.auth_form.labels
    );
    assert!(app.auth_form.error.as_deref().is_some_and(|e| !e.trim().is_empty()), "with the error (B5b)");
    // The next frames keep the login form: the store exists.
    frames_until(&mut app, 1, |_| false);
    assert!(app.wizard.is_none(), "the wizard does not come back once the store exists (B5b)");
    shut_runtime(&mut app);
    std::mem::forget(app);
}

fn login_and_notice(password: &str, expect_notice: bool) {
    let mut app = App::new();
    first_frame(&mut app);
    submit_login(&mut app, password);
    assert!(frames_until(&mut app, 240, login_settled), "the login ends");
    assert!(!app.locked, "the account opens: {:?}", app.auth_form.error);
    // The notice comes after the login; give it a few frames.
    let end = Instant::now() + Duration::from_millis(500);
    while Instant::now() < end && notice_text(&app).is_none() {
        app.build_scene(W, H);
        std::thread::sleep(Duration::from_millis(20));
    }
    let said = notice_text(&app).unwrap_or_default();
    if expect_notice {
        assert!(
            said.contains(INTERRUPTED_NOTICE),
            "a login with auth.store and no setup-complete marker shows the notice (B5b); the notice says {said:?}"
        );
    } else {
        assert!(
            !said.contains("Setup was interrupted"),
            "no interrupted-setup notice here (B5b: after a completed setup, or once it was shown); it says {said:?}"
        );
    }
    shut_runtime(&mut app);
    std::mem::forget(app);
}

fn duress_equals_main() {
    let mut app = App::new();
    first_frame(&mut app);
    // The main password changed after the duress step (Back) to the duress one.
    fill_wizard(&mut app, PRIMARY, PRIMARY);
    press_finish(&mut app);
    assert!(frames_until(&mut app, 120, setup_settled), "the setup ends");
    assert!(app.locked, "nothing opens");
    let w = app.wizard.as_ref().expect("the wizard stays (P4)");
    let e = w.error.clone().unwrap_or_default();
    assert!(
        w.step == STEP_DURESS,
        "the store's refusal takes the wizard back to the duress step (P4); it is at step {} with {e:?}",
        w.step
    );
    assert!(e.contains("Duress password must differ from your password"), "with the desktop's words (P4): {e:?}");
    assert!(!e.contains("Could not create account"), "not \"Could not create account\" (P4): {e:?}");
    assert!(!crate::crypto::auth_store_exists(), "no auth.store is written");
    shut_runtime(&mut app);
    std::mem::forget(app);
}

// ---- B5: closing the window while it runs

#[test]
fn b5_killed_right_after_the_store_write_the_account_opens_with_its_password() {
    let p = profile();
    launch_and_kill_at(module(), "store_then_kill", p.path(), "FIRST_RUN_STORE_WRITTEN");
    launch(module(), "login_primary_opens", p.path());
    launch(module(), "login_duress_opens", p.path());
}

#[test]
fn b5_killed_inside_the_first_installation_the_account_opens_with_its_password() {
    let p = profile();
    launch_and_kill_at(module(), "database_then_kill", p.path(), "FIRST_RUN_DATABASE_OPENED");
    launch(module(), "login_primary_opens", p.path());
    launch(module(), "login_duress_opens", p.path());
}

#[test]
fn b5_without_a_store_the_next_launch_shows_the_setup_again() {
    let p = profile();
    launch(module(), "fresh_profile_shows_setup", p.path());
}

// ---- B5b: after the store

#[test]
fn b5b_a_failure_after_the_store_drops_the_wizard_for_the_login_form() {
    let p = profile();
    launch(module(), "failure_after_store", p.path());
}

#[test]
fn b5b_a_completed_setup_shows_no_interrupted_notice_at_the_next_login() {
    let p = profile();
    launch(module(), "setup_completes", p.path());
    launch(module(), "login_primary_no_notice", p.path());
}

/// A real setup killed after its store write (the amended B5b: the notice
/// needs the "setup started" record, so the interrupted state is built
/// through the real setup, not with `create_auth_store` alone).
fn interrupted_setup(p: &Path) {
    launch_and_kill_at(module(), "setup_killed_after_store", p, "FIRST_RUN_SETUP_STORE_WRITTEN");
}

#[test]
fn b5b_an_interrupted_setup_shows_the_notice_once_at_the_next_login() {
    let p = profile();
    interrupted_setup(p.path());
    launch(module(), "login_primary_notice", p.path());
    launch(module(), "login_primary_no_notice", p.path());
}

#[test]
fn b5b_a_duress_login_shows_the_notice_and_writes_the_marker_too() {
    let p = profile();
    interrupted_setup(p.path());
    launch(module(), "login_duress_notice", p.path());
    launch(module(), "login_primary_no_notice", p.path());
}

// ---- B5b amended (ff959de): an account made before this build

/// A store and neither record (made the old way): no notice at its login,
/// nor at the next. That the marker is then written is not observable by
/// behaviour for such an account (no notice either way), so it is not
/// asserted beyond the second login.
#[test]
fn b5b_amended_an_account_made_before_this_build_logs_in_with_no_notice() {
    let p = profile();
    launch(module(), "store_only", p.path());
    launch(module(), "login_primary_no_notice", p.path());
    launch(module(), "login_primary_no_notice", p.path());
}

#[test]
fn b5b_amended_a_duress_login_on_an_account_made_before_this_build_shows_no_notice() {
    let p = profile();
    launch(module(), "store_only", p.path());
    launch(module(), "login_duress_no_notice", p.path());
    launch(module(), "login_primary_no_notice", p.path());
}

/// A join completes setup: after an interrupted setup (its "setup started"
/// record and no marker, the state in which a login shows the notice), a
/// join's end leaves no notice for the next login. The join's end is run on
/// the interrupted profile's own store with the derivation faked
/// (`first_run_worker_tests::complete_a_join_faked`): a real join needs a
/// second device; what is tested is `App`'s side.
#[test]
fn b5b_amended_a_completed_join_leaves_no_false_notice() {
    let p = profile();
    interrupted_setup(p.path());
    launch(module(), "join_completes", p.path());
    launch(module(), "login_primary_no_notice", p.path());
}

/// A recovery completes setup, likewise (`complete_a_recovery_faked`).
#[test]
fn b5b_amended_a_completed_recovery_leaves_no_false_notice() {
    let p = profile();
    interrupted_setup(p.path());
    launch(module(), "recovery_completes", p.path());
    launch(module(), "login_primary_no_notice", p.path());
}

// ---- P4 on the desktop

#[test]
fn p4_duress_equal_to_main_takes_the_wizard_back_to_the_duress_step() {
    let p = profile();
    launch(module(), "duress_equals_main", p.path());
}
