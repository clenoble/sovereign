//! v0.0.12 rc2 — the desktop's texts reach the screen reader, and "show" is
//! reachable without a mouse (L1–L5).
//!
//! Contract: `doc/plans/V0012-rc2-labels-spoken.md`. Written before the code,
//! by another author: whoever implements it makes these pass **without
//! editing this file**; a test that looks wrong is reported, not worked
//! around.
//!
//! A child module of `app` (declared in `app.rs`, like
//! `setup_step_announced_tests`), so it drives `App`'s own paths on a real,
//! headless `App`:
//! - the tree is read through `App::build_a11y_tree`, what the adapter pushes
//!   to the screen reader each frame, after a frame (`build_scene`) as
//!   `render` does — except the login form in this process: without an
//!   `auth.store` a frame puts the wizard back, so it is read without one here,
//!   and with frames in a child process on a real profile
//!   (`l4_on_a_real_profile_the_login_form_shows_from_the_keyboard_and_the_screen_reader`);
//! - names are what Narrator receives: `crate::a11y_spoken::spoken_name`, the
//!   rule of `accesskit_windows` 0.32.1 over `accesskit_consumer` 0.35.0 (a
//!   node of role `Label` is spoken by its **value**; see that file for the
//!   lines read);
//! - keys go through `App::locked_key`, the shell's key path while locked
//!   (the window's `KeyboardInput` hands it the key, after
//!   `ModifiersChanged` has set `App::mods` and `App::ctrl_down`);
//! - a screen reader's press goes through `App::a11y_action` with
//!   `ActionRequest { action: Click, target_node: <the node> }`, what the
//!   adapter's `A11yAction` delivers.
//!
//! **Readings chosen** (the contract leaves them open; reported with the
//! tests):
//! - **The "show" key is proposed here: Alt+F8** (`SHOW_KEY_NAME`). The
//!   contract makes it the implementer's choice, "written in the form's hint,
//!   and tested"; a test cannot press a key nobody has chosen, so this one is
//!   named, and the implementer builds that one. Why Alt+F8: it is the key
//!   Windows itself uses to reveal a password on its sign-in screen; it types
//!   no character, so a secret field cannot swallow it; Narrator does not use
//!   it (its keys go through Caps Lock or Insert); and the shell binds
//!   nothing to it. If Céline prefers another key, the test author changes
//!   `press_show_key` and `SHOW_KEY_NAME`, not the implementer.
//! - "Written in the form's hint": on the login and join forms, their hint
//!   line (the one ending "Enter to unlock" / "Enter to join") names the key.
//!   The wizard draws no hint line on its secret steps, so there the key must
//!   be spoken by some node of the step (a new hint line, or the toggle's own
//!   name: either is heard). Its exact words are the implementer's.
//! - "Reached and pressed from the keyboard": the key presses the toggle
//!   from any field of the form, without moving the focus to it. Tab is not
//!   required to stop on it.
//! - The show/hide node: a node that is not a `Label`, that offers the Click
//!   action (or Narrator cannot press it), and whose spoken name contains
//!   "show password" or "hide password" (the wizard's password and duress
//!   steps, the login and join forms), "show phrase" or "hide phrase" (the
//!   canary step), case aside: "Show passwords", "Show password (Alt+F8)"
//!   both qualify.
//! - L1's "drawn text": a `Label` node's spoken name must not be empty, and
//!   when the node still carries a label property, the two must agree (one
//!   text, not two). Besides, on each screen, the texts known to be drawn
//!   (copied here from the code that draws them, as `first_run_tests` and
//!   `setup_step_announced_tests` copy the step titles; the panels' own
//!   `a11y_lines` where they exist) must each be the spoken name of a node.
//!   The role of that node is not checked (a title may become a heading).
//! - L5's "texts unchanged" for the login and join hints: the hint must name
//!   the key, so it changes; every one of its other segments (split on the
//!   drawn " · ") is kept as today. "show to reveal" is the segment allowed
//!   to change.
//! - L5's "nothing added when unlocked": with the workspace unlocked, the
//!   tree has as many nodes of each role as today (L1 and L2 move texts, they
//!   add no node), and no node speaks of the show key.

use accesskit::{Action, ActionRequest, NodeId, Role as AkRole, TreeId, TreeUpdate};
use winit::keyboard::{Key, ModifiersState, NamedKey};

use super::first_run_tests::{launch, profile, shut_runtime, CHILD_VAR, DURESS, H, PRIMARY, W};
use super::{App, WindowContent, WindowKind};
use crate::a11y_spoken::spoken_name;
use crate::onboarding::{
    OnboardingWizard, WizAction, STEP_CANARY, STEP_DURESS, STEP_NICKNAME, STEP_PASSWORD, STEP_SAMPLE, STEP_WELCOME,
    WIZ_STEPS,
};
use crate::panels::{AuthForm, ChatMsg, Role as ChatRole};

// ======================================================================= shared

/// The key proposed for "show" (see the header): Alt+F8.
const SHOW_KEY_NAME: &str = "Alt+F8";

/// Presses the "show" key through the shell's key path while locked, as the
/// window delivers it: `ModifiersChanged` (Alt down), the key, then the
/// modifiers released.
fn press_show_key(app: &mut App) {
    app.mods = ModifiersState::ALT;
    app.ctrl_down = false;
    app.locked_key(&Key::Named(NamedKey::F8), None);
    app.mods = ModifiersState::empty();
}

/// The titles `onboarding::step_copy` draws, per step (copied: private).
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

/// The copy `onboarding::step_copy` draws under each title (copied: private).
const STEP_BODIES: [&str; WIZ_STEPS] = [
    "Your personal, sovereign computing environment. Everything runs locally — your data, your AI, your rules.",
    "Give your assistant a nickname — how it introduces itself. You can change this later, or leave it blank.",
    "Pick how your AI assistant appears on screen.",
    "Select a visual theme for the interface. You can switch any time in settings.",
    "Load example documents, threads, and contacts so you can explore right away.",
    "This password encrypts your local data. Choose something strong. Lose it and social recovery has your back: enrol 5 trusted guardians after setup — your secrets stay yours as long as fewer than 3 of your 5 guardians collude. Later you can host encrypted fragments for friends, and they for you.",
    "A secondary password that opens a decoy workspace under coercion. Optional — you can skip it.",
    "A personal phrase shown after login. If it ever changes, you'll know the system was tampered with. Optional.",
];

/// The sample step's hint, with sample data on (`draw_sample_step`).
const SAMPLE_HINT_ON: &str = "Sample documents, threads, and contacts will be created.";

/// The drawn separator of the forms' hint segments.
const HINT_SEP: &str = "  \u{00b7}  ";

/// The login form's title, copy and hint segments (`AuthForm::copy`, private).
const LOGIN_TITLE: &str = "Unlock Sovereign";
const LOGIN_COPY: &str = "Enter your password to decrypt your workspace.";
const LOGIN_HINT: [&str; 3] = ["Ctrl+V to paste your password", "show to reveal", "Enter to unlock"];
/// The join form's.
const JOIN_TITLE: &str = "Join from another device";
const JOIN_COPY: &str = "Paste the code from your other device, enter its PIN, and pick a password for THIS device.";
const JOIN_HINT: [&str; 5] =
    ["Ctrl+V paste", "show to reveal", "Tab to switch", "Enter to join", "click below to create a new account instead"];
/// The hint segment allowed to change (it must name the key now).
const REVEAL_SEGMENT: &str = "show to reveal";

/// Secrets typed in these screens; none may appear in the tree.
const WIZ_PASSWORD: &str = "Wz!Spoken-Pass1";
const WIZ_DURESS: &str = "Dz!Spoken-Pass2";
const WIZ_CANARY: &str = "kestrel at noon";
const LOGIN_SECRET: &str = "Login!Spoken-77";
const JOIN_PASSWORD: &str = "Join!Spoken-Pass3";
const JOIN_DURESS: &str = "Join!Spoken-Duress4";
const SECRETS: [&str; 6] = [WIZ_PASSWORD, WIZ_DURESS, WIZ_CANARY, LOGIN_SECRET, JOIN_PASSWORD, JOIN_DURESS];

/// The chat of L2.
const USER_SAYS: &str = "Is the garden plan saved?";
const ASSISTANT_SAYS: &str = "Yes — it is in the Ideas thread, saved a minute ago.";

/// The in-process tests' data directory: a fresh temporary one unless
/// `SOVEREIGN_DATA_DIR` is already set.
fn ensure_test_data_dir() {
    static DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        if let Some(d) = std::env::var_os("SOVEREIGN_DATA_DIR") {
            return d.into();
        }
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let d = std::env::temp_dir().join(format!("sov-labels-spoken-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::env::set_var("SOVEREIGN_DATA_DIR", &d);
        d
    });
}

fn done(mut app: App) {
    shut_runtime(&mut app);
    std::mem::forget(app);
}

// ------------------------------------------------------------- the screens

/// A locked shell showing the setup wizard on `step`, every field filled,
/// reached with the wizard's own Next (as `setup_step_announced_tests`).
fn wizard_at(step: usize) -> App {
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
    app.wizard = Some(w);
    while app.wizard.as_ref().unwrap().step < step {
        let from = app.wizard.as_ref().unwrap().step;
        app.handle_wiz_action(WizAction::Next);
        assert_eq!(app.wizard.as_ref().unwrap().step, from + 1, "setup: Next leaves step {from}");
    }
    app
}

/// A locked shell showing the login form, its password typed (no frame: see
/// the header).
fn login_screen() -> App {
    ensure_test_data_dir();
    let mut app = App::new();
    app.locked = true;
    app.test_view = Some((W, H));
    app.wizard = None;
    let mut f = AuthForm::login();
    f.fields[0] = LOGIN_SECRET.into();
    app.auth_form = f;
    app
}

/// A locked shell showing the join form, its secrets typed.
fn join_screen() -> App {
    ensure_test_data_dir();
    let mut app = App::new();
    app.locked = true;
    app.test_view = Some((W, H));
    app.wizard = None;
    let mut f = AuthForm::join();
    f.fields[1] = "482913".into();
    f.fields[2] = JOIN_PASSWORD.into();
    f.fields[3] = JOIN_DURESS.into();
    app.auth_form = f;
    app
}

/// An unlocked shell with no store and no window (the canvas only).
fn unlocked() -> App {
    ensure_test_data_dir();
    let mut app = App::new();
    app.test_view = Some((W, H));
    app.wizard = None;
    app.locked = false;
    app.windows.clear();
    app
}

/// An unlocked shell with the chat open: a user message, an assistant
/// message, and (if `pending`) a reply awaited.
fn chat_open(pending: bool) -> App {
    let mut app = unlocked();
    app.focus_or_toggle(WindowKind::Chat);
    match &mut app.windows.last_mut().expect("the chat opens").content {
        WindowContent::Chat(c) => {
            c.msgs = vec![
                ChatMsg { role: ChatRole::User, text: USER_SAYS.into() },
                ChatMsg { role: ChatRole::Assistant, text: ASSISTANT_SAYS.into() },
            ];
            c.pending = pending;
        }
        _ => panic!("setup: the top window is the chat"),
    }
    app
}

/// The secret screens of L4: (what, the shell, the words its toggle carries).
fn secret_screens() -> Vec<(String, App, &'static str)> {
    let mut v = Vec::new();
    for step in [STEP_PASSWORD, STEP_DURESS, STEP_CANARY] {
        let words = if step == STEP_CANARY { "phrase" } else { "password" };
        v.push((format!("wizard step {step} ({:?})", STEP_TITLES[step]), wizard_at(step), words));
    }
    v.push(("login".to_string(), login_screen(), "password"));
    v.push(("join".to_string(), join_screen(), "password"));
    v
}

/// Whether the screen is the login form (read without a frame, see the header).
fn is_login(app: &App) -> bool {
    app.locked && app.wizard.is_none() && !app.auth_form.joining
}

/// The reveal state of the screen on show.
fn reveal_of(app: &App) -> bool {
    match &app.wizard {
        Some(w) => w.reveal,
        None => app.auth_form.reveal,
    }
}

/// The secrets the screen holds, as typed (they must not change with "show").
fn typed(app: &App) -> Vec<String> {
    match &app.wizard {
        Some(w) => vec![
            w.password.clone(),
            w.password_confirm.clone(),
            w.duress.clone(),
            w.duress_confirm.clone(),
            w.canary.clone(),
            w.canary_confirm.clone(),
        ],
        None => app.auth_form.fields.clone(),
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
    fn spoken(&self, id: NodeId) -> String {
        spoken_name(&self.up.nodes, id)
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
    /// What Narrator reads walking the tree (scan mode), in order.
    fn heard(&self) -> Vec<String> {
        self.order().into_iter().map(|id| self.spoken(id)).collect()
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
    /// The show/hide node (see the header): not a `Label`, spoken as
    /// "show <words>" or "hide <words>".
    fn toggle(&self, words: &str) -> Option<NodeId> {
        self.order().into_iter().find(|id| {
            let s = self.spoken(*id).to_lowercase();
            self.node(*id).is_some_and(|n| n.role() != AkRole::Label)
                && (s.contains(&format!("show {words}")) || s.contains(&format!("hide {words}")))
        })
    }
    fn count_role(&self, role: AkRole) -> usize {
        self.up.nodes.iter().filter(|(_, n)| n.role() == role).count()
    }
    fn dump(&self) -> String {
        self.order()
            .into_iter()
            .map(|id| {
                let n = self.node(id).unwrap();
                let mark = if id == self.up.focus { "  <- focus" } else { "" };
                format!(
                    "{:?} {:?} label={:?} value={:?} spoken={:?}{mark}",
                    id,
                    n.role(),
                    n.label(),
                    n.value(),
                    self.spoken(id)
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// The tree as pushed by `render`: a frame, then `build_a11y_tree`. The
/// login form in this process is read without the frame (see the header).
fn frame_and_read(app: &mut App) -> Snap {
    if !is_login(app) {
        app.build_scene(W, H);
    }
    read(app)
}

fn read(app: &mut App) -> Snap {
    let up = app.build_a11y_tree();
    let root = up.tree.as_ref().map(|t| t.root).expect("the update carries its tree");
    Snap { up, root }
}

fn click(app: &mut App, id: NodeId) {
    app.a11y_action(ActionRequest { action: Action::Click, target_tree: TreeId::ROOT, target_node: id, data: None });
}

fn assert_none(problems: Vec<String>, what: &str) {
    assert!(problems.is_empty(), "{what}: {} problem(s)\n\n{}", problems.len(), problems.join("\n\n"));
}

/// L1 on one screen: every `Label` node is spoken, and as the one text it
/// carries; and each text known to be drawn is spoken by a node.
fn l1_problems(t: &Snap, drawn: &[String], what: &str) -> Vec<String> {
    let mut p = Vec::new();
    for (id, n) in &t.up.nodes {
        if n.role() != AkRole::Label {
            continue;
        }
        let s = t.spoken(*id);
        if s.trim().is_empty() {
            p.push(format!("{what}: Label {id:?} (label {:?}) is spoken as nothing", n.label()));
        } else if let Some(l) = n.label().filter(|l| !l.is_empty()) {
            if l != s {
                p.push(format!("{what}: Label {id:?} carries {l:?} but is spoken as {s:?}"));
            }
        }
    }
    let heard = t.heard();
    for d in drawn {
        if !heard.iter().any(|h| h == d) {
            p.push(format!("{what}: the drawn text {d:?} is not the spoken name of any node"));
        }
    }
    if !p.is_empty() {
        p.push(format!("{what}, the tree:\n{}", t.dump()));
    }
    p
}

/// The texts the wizard draws on `step` that are `Label`s today (title,
/// "Step N of 8", copy, and the step's own lines).
fn wizard_drawn(w: &OnboardingWizard) -> Vec<String> {
    let step = w.step;
    let mut v = vec![STEP_TITLES[step].to_string()];
    if step > STEP_WELCOME {
        v.push(format!("Step {} of {}", step + 1, WIZ_STEPS));
    }
    v.push(STEP_BODIES[step].to_string());
    match step {
        STEP_WELCOME => v.push(format!("Your designation: {}", w.designation)),
        STEP_NICKNAME => v.push(crate::device_name::SEEN_BY.to_string()),
        STEP_SAMPLE if w.seed_sample => v.push(SAMPLE_HINT_ON.to_string()),
        _ => {}
    }
    v
}

/// The spoken name, among `heard`, of the form's hint line (the one holding
/// `anchor`, a segment no key choice touches).
fn hint_heard<'a>(heard: &'a [String], anchor: &str) -> Option<&'a String> {
    heard.iter().find(|h| h.contains(anchor))
}

// ======================================================= L1

/// L1 on every wizard step: every `Label` spoken, as drawn.
#[test]
fn l1_every_label_of_every_wizard_step_is_spoken_as_drawn() {
    let mut problems = Vec::new();
    for step in 0..WIZ_STEPS {
        let mut app = wizard_at(step);
        let t = frame_and_read(&mut app);
        let drawn = wizard_drawn(app.wizard.as_ref().unwrap());
        problems.extend(l1_problems(&t, &drawn, &format!("wizard step {step}")));
        if step == STEP_PASSWORD {
            // The strength line, a Label drawn under the password.
            if !t.heard().iter().any(|h| h.starts_with("Password strength: ")) {
                problems.push(format!("wizard step {step}: the strength line is not spoken\n{}", t.dump()));
            }
        }
        done(app);
    }
    assert_none(problems, "L1 (wizard)");
}

/// L1 on the login and join forms.
#[test]
fn l1_every_label_of_the_login_and_join_forms_is_spoken_as_drawn() {
    let mut problems = Vec::new();
    let mut app = login_screen();
    let t = frame_and_read(&mut app);
    problems.extend(l1_problems(&t, &[LOGIN_TITLE.into(), LOGIN_COPY.into()], "login"));
    done(app);
    let mut app = join_screen();
    let t = frame_and_read(&mut app);
    problems.extend(l1_problems(
        &t,
        &[JOIN_TITLE.into(), JOIN_COPY.into(), crate::device_name::SEEN_BY.into()],
        "join",
    ));
    done(app);
    assert_none(problems, "L1 (login, join)");
}

/// L1 unlocked: the canvas summary, the chat (with a reply awaited), Settings,
/// the Inbox, and the count of other windows open.
#[test]
fn l1_every_label_of_the_unlocked_windows_is_spoken_as_drawn() {
    let mut problems = Vec::new();

    let mut app = unlocked();
    let t = frame_and_read(&mut app);
    let summary = format!("Spatial canvas — {} documents across {} thread lanes", app.cards.len(), app.lanes.thread_count());
    problems.extend(l1_problems(&t, &[summary], "the canvas, no window"));
    done(app);

    let mut app = chat_open(true);
    let t = frame_and_read(&mut app);
    problems.extend(l1_problems(
        &t,
        &["Chat".into(), format!("You: {USER_SAYS}"), format!("Assistant: {ASSISTANT_SAYS}"), "Assistant is thinking".into()],
        "the chat",
    ));
    done(app);

    let mut app = unlocked();
    app.focus_or_toggle(WindowKind::Settings);
    let lines = match &app.windows.last().expect("Settings opens").content {
        WindowContent::Settings(s) => s.a11y_lines(),
        _ => panic!("setup: the top window is Settings"),
    };
    let t = frame_and_read(&mut app);
    let mut drawn = vec!["Settings".to_string()];
    drawn.extend(lines);
    problems.extend(l1_problems(&t, &drawn, "Settings"));
    done(app);

    let mut app = unlocked();
    app.focus_or_toggle(WindowKind::Inbox);
    let lines = match &app.windows.last().expect("the Inbox opens (synthetic, no store)").content {
        WindowContent::Inbox(ib) => ib.a11y_lines(),
        _ => panic!("setup: the top window is the Inbox"),
    };
    let t = frame_and_read(&mut app);
    let mut drawn = vec!["Inbox".to_string()];
    drawn.extend(lines);
    problems.extend(l1_problems(&t, &drawn, "the Inbox"));
    done(app);

    let mut app = chat_open(false);
    app.focus_or_toggle(WindowKind::Settings);
    let t = frame_and_read(&mut app);
    problems.extend(l1_problems(&t, &["1 more window(s) open".into()], "Settings over the chat"));
    done(app);

    assert_none(problems, "L1 (unlocked)");
}

/// L1 in the Trash dialogs (Restore into…, with and without a thread; Empty
/// trash) and the Orphans dialog (Move into…, with and without a thread):
/// their hint `Label` is spoken, and says what the dialog draws.
#[test]
fn l1_the_trash_and_orphans_dialogs_hints_are_spoken() {
    use crate::orphans_ui::{OrphanTarget, ThreadChooser};
    use crate::trash_ui::{RestoreTarget, TrashModal};
    let threads = vec![("thread:a".to_string(), "Ideas".to_string()), ("thread:b".to_string(), "Notes".to_string())];
    let mut problems = Vec::new();

    let cases: Vec<(&str, TrashModal, &str)> = vec![
        (
            "Trash, Restore into… (threads)",
            TrashModal::ChooseThread {
                target: RestoreTarget::One("doc:x".into()),
                title: "Restore \u{201c}Plan\u{201d} into\u{2026}".into(),
                threads: threads.clone(),
                selected: 0,
                top: 0,
            },
            "Choose the thread it goes back to. To bring a deleted thread back, create a new thread first.",
        ),
        (
            "Trash, Restore into… (no thread)",
            TrashModal::ChooseThread {
                target: RestoreTarget::One("doc:x".into()),
                title: "Restore \u{201c}Plan\u{201d} into\u{2026}".into(),
                threads: Vec::new(),
                selected: 0,
                top: 0,
            },
            "There is no thread to restore into. Create a thread first (\u{201c}New lane\u{201d}), then restore.",
        ),
        (
            "Trash, Empty trash",
            TrashModal::ConfirmEmpty { count: 3 },
            "3 documents will be deleted for good, with their versions. This cannot be undone.",
        ),
    ];
    for (what, modal, body) in cases {
        let mut app = unlocked();
        app.trash_modal = Some(modal);
        let t = frame_and_read(&mut app);
        problems.extend(l1_problems(&t, &[], what));
        if !t.heard().iter().any(|h| h.contains(body)) {
            problems.push(format!("{what}: the drawn body {body:?} is not spoken\n{}", t.dump()));
        }
        done(app);
    }

    for (what, threads, hint) in [
        ("Orphans, Move into… (threads)", threads.clone(), "Up and Down choose a thread, Enter moves into it, Escape cancels."),
        (
            "Orphans, Move into… (no thread)",
            Vec::new(),
            "There is no thread to move into. Escape cancels; choose New thread instead.",
        ),
    ] {
        let mut app = unlocked();
        app.orphan_modal = Some(ThreadChooser::new(OrphanTarget::One("doc:y".into()), "Move \u{201c}Plan\u{201d} into\u{2026}".into(), threads));
        let t = frame_and_read(&mut app);
        problems.extend(l1_problems(&t, &[hint.to_string()], what));
        done(app);
    }
    assert_none(problems, "L1 (dialogs)");
}

/// L1 in the new-thread dialog: whatever `Label`s it holds are spoken (it
/// holds none today; the check stands if one is added).
#[test]
fn l1_the_new_thread_dialog_has_no_silent_label() {
    let mut app = unlocked();
    app.open_new_thread_prompt();
    assert!(app.new_thread_prompt.is_some(), "setup: the new-thread dialog is open");
    let t = frame_and_read(&mut app);
    let problems = l1_problems(&t, &[], "the new-thread dialog");
    done(app);
    assert_none(problems, "L1 (new thread)");
}

// ======================================================= L2

/// L2: with the chat open, each message is heard, in drawn order, "You: …"
/// then "Assistant: …"; the window's name before them.
#[test]
fn l2_the_chats_messages_are_heard_in_order() {
    let mut app = chat_open(false);
    let t = frame_and_read(&mut app);
    let heard = t.heard();
    let want = ["Chat".to_string(), format!("You: {USER_SAYS}"), format!("Assistant: {ASSISTANT_SAYS}")];
    let mut it = heard.iter();
    for w in &want {
        assert!(
            it.any(|h| h == w),
            "L2: {w:?} is not heard, or not in drawn order (want {want:?})\n{}",
            t.dump()
        );
    }
    done(app);
}

// ======================================================= L3

/// L3 on every wizard step: its title, "Step N of 8" past welcome, its copy,
/// and (sample data) its hint each reach Narrator, in that order.
#[test]
fn l3_every_wizard_step_is_heard() {
    let mut problems = Vec::new();
    for step in 0..WIZ_STEPS {
        let mut app = wizard_at(step);
        let t = frame_and_read(&mut app);
        let heard = t.heard();
        let mut want = vec![STEP_TITLES[step].to_string()];
        if step > STEP_WELCOME {
            want.push(format!("Step {} of {}", step + 1, WIZ_STEPS));
        }
        want.push(STEP_BODIES[step].to_string());
        if step == STEP_SAMPLE {
            want.push(SAMPLE_HINT_ON.to_string());
        }
        let mut it = heard.iter();
        for w in &want {
            if !it.any(|h| h == w) {
                problems.push(format!("step {step}: {w:?} is not heard, or out of order (want {want:?})\n{}", t.dump()));
                break;
            }
        }
        done(app);
    }
    assert_none(problems, "L3 (wizard)");
}

/// L3 on the login and join forms: title ("Unlock Sovereign" for login),
/// copy and hint each reach Narrator, in that order.
#[test]
fn l3_the_login_and_join_forms_are_heard() {
    let mut problems = Vec::new();
    for (what, mut app, title, copy, anchor) in [
        ("login", login_screen(), LOGIN_TITLE, LOGIN_COPY, "Enter to unlock"),
        ("join", join_screen(), JOIN_TITLE, JOIN_COPY, "Enter to join"),
    ] {
        let t = frame_and_read(&mut app);
        let heard = t.heard();
        let at = |s: &str| heard.iter().position(|h| h == s);
        let hint_at = heard.iter().position(|h| h.contains(anchor));
        match (at(title), at(copy), hint_at) {
            (Some(a), Some(b), Some(c)) if a < b && b < c => {}
            got => problems.push(format!(
                "{what}: title {title:?}, copy and hint (holding {anchor:?}) not all heard in order: {got:?}\n{}",
                t.dump()
            )),
        }
        done(app);
    }
    assert_none(problems, "L3 (login, join)");
}

// ======================================================= L4

/// L4: wherever a secret field is drawn, a show/hide node exists, named for
/// what it reveals, and pressable by a screen reader (Click).
#[test]
fn l4_every_secret_screen_has_a_show_node_named_for_what_it_reveals() {
    let mut problems = Vec::new();
    for (what, mut app, words) in secret_screens() {
        let t = frame_and_read(&mut app);
        match t.toggle(words) {
            None => problems.push(format!("{what}: no node spoken as \"show {words}…\" or \"hide {words}…\"\n{}", t.dump())),
            Some(id) => {
                let s = t.spoken(id).to_lowercase();
                if !s.contains(&format!("show {words}")) {
                    problems.push(format!("{what}: show is off, the node says {:?}, not \"show {words}…\"", t.spoken(id)));
                }
                if !t.node(id).is_some_and(|n| n.supports_action(Action::Click)) {
                    problems.push(format!("{what}: the node {:?} offers no Click: Narrator cannot press it", t.spoken(id)));
                }
            }
        }
        done(app);
    }
    assert_none(problems, "L4 (the node)");
}

/// L4: the key is written where it is heard — the login and join hints name
/// it; on the wizard's secret steps, some node of the step speaks it.
#[test]
fn l4_the_show_key_is_written_in_the_hint() {
    let mut problems = Vec::new();
    for (what, mut app, _) in secret_screens() {
        let t = frame_and_read(&mut app);
        let heard = t.heard();
        let ok = match (app.wizard.is_some(), app.auth_form.joining) {
            (true, _) => heard.iter().any(|h| h.contains(SHOW_KEY_NAME)),
            (false, false) => hint_heard(&heard, "Enter to unlock").is_some_and(|h| h.contains(SHOW_KEY_NAME)),
            (false, true) => hint_heard(&heard, "Enter to join").is_some_and(|h| h.contains(SHOW_KEY_NAME)),
        };
        if !ok {
            problems.push(format!("{what}: {SHOW_KEY_NAME:?} is not heard in the hint\n{}", t.dump()));
        }
        done(app);
    }
    assert_none(problems, "L4 (the key in the hint)");
}

/// L4: the key, pressed through the shell's key path, toggles what is drawn
/// (on, then off again), the node's name follows, and no field changes.
#[test]
fn l4_the_show_key_toggles_the_reveal() {
    let mut problems = Vec::new();
    for (what, mut app, words) in secret_screens() {
        frame_and_read(&mut app);
        let before = typed(&app);
        assert!(!reveal_of(&app), "setup: {what} opens with show off");
        press_show_key(&mut app);
        let t = frame_and_read(&mut app);
        if !reveal_of(&app) {
            problems.push(format!("{what}: {SHOW_KEY_NAME} did not turn show on"));
        } else if !t.toggle(words).is_some_and(|id| t.spoken(id).to_lowercase().contains(&format!("hide {words}"))) {
            problems.push(format!("{what}: show is on, no node says \"hide {words}…\"\n{}", t.dump()));
        }
        if typed(&app) != before {
            problems.push(format!("{what}: {SHOW_KEY_NAME} changed a field"));
        }
        press_show_key(&mut app);
        frame_and_read(&mut app);
        if reveal_of(&app) {
            problems.push(format!("{what}: {SHOW_KEY_NAME} pressed again did not turn show off"));
        }
        done(app);
    }
    assert_none(problems, "L4 (the key)");
}

/// L4: a screen reader's Click on the node toggles the reveal (on, then off
/// on the renamed node), through `App::a11y_action`.
#[test]
fn l4_a_screen_reader_click_toggles_the_reveal() {
    let mut problems = Vec::new();
    for (what, mut app, words) in secret_screens() {
        let t = frame_and_read(&mut app);
        let Some(id) = t.toggle(words) else {
            problems.push(format!("{what}: no show node to press\n{}", t.dump()));
            done(app);
            continue;
        };
        let before = typed(&app);
        click(&mut app, id);
        let t = frame_and_read(&mut app);
        if !reveal_of(&app) {
            problems.push(format!("{what}: Click on {:?} did not turn show on", t.spoken(id)));
            done(app);
            continue;
        }
        let Some(id) = t.toggle(words).filter(|id| t.spoken(*id).to_lowercase().contains(&format!("hide {words}"))) else {
            problems.push(format!("{what}: show is on, no node says \"hide {words}…\"\n{}", t.dump()));
            done(app);
            continue;
        };
        click(&mut app, id);
        frame_and_read(&mut app);
        if reveal_of(&app) {
            problems.push(format!("{what}: Click on \"hide {words}\" did not turn show off"));
        }
        if typed(&app) != before {
            problems.push(format!("{what}: the Click changed a field"));
        }
        done(app);
    }
    assert_none(problems, "L4 (Click)");
}

/// L4 on a real profile, with frames: the login form a user meets (an
/// `auth.store`, the password typed key by key) has its show node, its key
/// in the hint, and both the key and the screen reader's Click toggle it. In
/// a child process (several seconds in a debug build).
#[test]
fn l4_on_a_real_profile_the_login_form_shows_from_the_keyboard_and_the_screen_reader() {
    let p = profile();
    launch(module(), "login_on_a_real_profile", p.path());
}

fn login_on_a_real_profile() {
    crate::crypto::create_auth_store(PRIMARY.as_bytes(), DURESS.as_bytes()).expect("create auth.store");
    let mut app = App::new();
    app.test_view = Some((W, H));
    app.build_scene(W, H);
    assert!(app.locked && app.wizard.is_none() && !app.auth_form.joining, "a profile with auth.store shows the login form");
    for c in LOGIN_SECRET.chars() {
        let s = c.to_string();
        app.locked_key(&Key::Character(s.as_str().into()), Some(&s));
    }
    assert_eq!(app.auth_form.fields[0], LOGIN_SECRET, "setup: the password typed");
    let mut problems = Vec::new();
    app.build_scene(W, H);
    let t = read(&mut app);
    let heard = t.heard();
    // L3 after a frame, on the real screen.
    for want in [LOGIN_TITLE, LOGIN_COPY] {
        if !heard.iter().any(|h| h == want) {
            problems.push(format!("L3: {want:?} is not heard\n{}", t.dump()));
        }
    }
    if !hint_heard(&heard, "Enter to unlock").is_some_and(|h| h.contains(SHOW_KEY_NAME)) {
        problems.push(format!("L4: the hint does not name {SHOW_KEY_NAME}\n{}", t.dump()));
    }
    match t.toggle("password") {
        None => problems.push(format!("L4: no show node\n{}", t.dump())),
        Some(id) => {
            press_show_key(&mut app);
            app.build_scene(W, H);
            if !app.auth_form.reveal {
                problems.push(format!("L4: {SHOW_KEY_NAME} did not turn show on"));
            }
            press_show_key(&mut app);
            app.build_scene(W, H);
            if app.auth_form.reveal {
                problems.push(format!("L4: {SHOW_KEY_NAME} again did not turn show off"));
            }
            click(&mut app, id);
            app.build_scene(W, H);
            if !app.auth_form.reveal {
                problems.push("L4: the screen reader's Click did not turn show on".to_string());
            }
            let t = read(&mut app);
            for s in t.every_string() {
                if s.contains(LOGIN_SECRET) {
                    problems.push(format!("L5: show on, the tree carries the password ({s:?})"));
                }
            }
        }
    }
    assert_eq!(app.auth_form.fields[0], LOGIN_SECRET, "the password is untouched by show");
    shut_runtime(&mut app);
    std::mem::forget(app);
    assert_none(problems, "L4 (real profile)");
}

// ======================================================= L5

/// L5: a secret field says its length only, show off and on, whichever way
/// show was turned (the key, the screen reader, the mouse's own action).
#[test]
fn l5_secrets_stay_length_only_whichever_way_show_is_turned() {
    let mut problems = Vec::new();
    for (what, mut app, words) in secret_screens() {
        for how in ["off", "the key", "Click", "the mouse's action"] {
            match how {
                "the key" => press_show_key(&mut app),
                "Click" => {
                    let t = frame_and_read(&mut app);
                    if let Some(id) = t.toggle(words) {
                        click(&mut app, id);
                    }
                }
                "the mouse's action" => {
                    if app.wizard.is_some() {
                        // What a click on the step's "show" resolves to.
                        app.handle_wiz_action(WizAction::ToggleReveal);
                    } else {
                        // A click on the first secret field's "show".
                        let (_, rects) = crate::panels::auth_layout(W, H, app.auth_form.fields.len());
                        let i = (0..rects.len()).find(|i| app.auth_form.is_masked(*i)).expect("a secret field");
                        let b = crate::panels::auth_reveal_btn(rects[i]);
                        app.handle_click((b.x0 + b.x1) * 0.5, (b.y0 + b.y1) * 0.5);
                    }
                }
                _ => {}
            }
            let t = frame_and_read(&mut app);
            for s in t.every_string() {
                for secret in SECRETS {
                    if s.contains(secret) {
                        problems.push(format!("{what}, after {how} (show {}): the tree carries a secret ({s:?})", reveal_of(&app)));
                    }
                }
            }
            for (id, n) in &t.up.nodes {
                if n.role() != AkRole::PasswordInput {
                    continue;
                }
                let v = n.value().unwrap_or_default();
                let ok = v.is_empty() || (v.ends_with(" characters") && v.trim_end_matches(" characters").parse::<usize>().is_ok());
                if !ok {
                    problems.push(format!("{what}, after {how}: password input {id:?} ({:?}) says {v:?}", t.spoken(*id)));
                }
            }
        }
        done(app);
    }
    assert_none(problems, "L5 (secrets)");
}

/// L5: the texts are unchanged — the login and join hints keep every segment
/// but the one about "show"; the chat keeps its "You: " and "Assistant: "
/// (L2); the titles and copy are exact (L3).
#[test]
fn l5_the_forms_hints_keep_their_other_segments() {
    let mut problems = Vec::new();
    for (what, mut app, segments, anchor) in [
        ("login", login_screen(), &LOGIN_HINT[..], "Enter to unlock"),
        ("join", join_screen(), &JOIN_HINT[..], "Enter to join"),
    ] {
        let t = frame_and_read(&mut app);
        let heard = t.heard();
        match hint_heard(&heard, anchor) {
            None => problems.push(format!("{what}: no hint heard\n{}", t.dump())),
            Some(h) => {
                let have: Vec<&str> = h.split(HINT_SEP).collect();
                for s in segments.iter().filter(|s| **s != REVEAL_SEGMENT) {
                    if !have.contains(s) {
                        problems.push(format!("{what}: the hint lost {s:?}: {h:?}"));
                    }
                }
            }
        }
        done(app);
    }
    assert_none(problems, "L5 (hints)");
}

/// L5: nothing is added when the workspace is unlocked — as many nodes of
/// each role as today (the canvas alone; the chat with a reply awaited), and
/// none speaks of the show key or of show/hide.
#[test]
fn l5_nothing_is_added_when_unlocked() {
    let mut problems = Vec::new();
    // Today: the canvas alone is the window and its summary.
    let mut app = unlocked();
    let t = frame_and_read(&mut app);
    let lanes = t.count_role(AkRole::Button);
    if t.up.nodes.len() != 2 + lanes || t.count_role(AkRole::Label) != 1 {
        problems.push(format!("the canvas alone: {} nodes, {} Labels (today: 2 + {lanes} lane buttons, 1 Label)\n{}", t.up.nodes.len(), t.count_role(AkRole::Label), t.dump()));
    }
    done(app);
    // Today: the chat is its name, one Label per message, the reply awaited.
    let mut app = chat_open(true);
    let t = frame_and_read(&mut app);
    if t.count_role(AkRole::Label) != 4 {
        problems.push(format!("the chat: {} Labels (today: Chat, 2 messages, thinking)\n{}", t.count_role(AkRole::Label), t.dump()));
    }
    for h in t.heard() {
        let l = h.to_lowercase();
        if h.contains(SHOW_KEY_NAME) || l.contains("show password") || l.contains("show phrase") {
            problems.push(format!("the chat: a locked screen's {h:?} is heard"));
        }
    }
    done(app);
    assert_none(problems, "L5 (unlocked)");
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
        "login_on_a_real_profile" => login_on_a_real_profile(),
        other => panic!("unknown scenario {other}"),
    }
}
