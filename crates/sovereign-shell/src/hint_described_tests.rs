//! Spec ch. 12, *Accessibility*, L6 (v0.0.13 rc2): the key is said with the field. Céline, 2026-11-20, « 20. A
//! corriger sans révéler », on the gap « The login form's "Alt+F8 to show" is read in scan mode only, not when
//! the password field takes focus »; the way is WSL's (2026-11-19): the hint as the field's description.
//!
//! - On the login and join forms, each secret field's description in the tree is the form's drawn hint, whole and
//!   as drawn, so a screen reader reads it when the field takes the focus.
//! - The hint stays where it was, a label after the fields, for scan mode.
//! - Nothing drawn changes, and no field's name or value changes.
//!
//! The tree is read as the screen reader receives it (`build_a11y_tree`). The login form is read without a frame,
//! as `labels_spoken_tests` does: without an `auth.store`, a frame puts the wizard back.
//!
//! Written before the code by R1's test author; whoever implements makes these pass without editing this file. A
//! test that looks wrong is reported on the channel.

use accesskit::{Node, NodeId, Role};

use super::first_run_tests::{shut_runtime, H, W};
use super::App;
use crate::panels::AuthForm;

/// The key the hint names (L4).
const SHOW_KEY: &str = "Alt+F8";

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
        let d = std::env::temp_dir().join(format!("sov-hint-described-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::env::set_var("SOVEREIGN_DATA_DIR", &d);
        d
    });
}

fn done(mut app: App) {
    shut_runtime(&mut app);
    std::mem::forget(app);
}

fn locked_with(form: AuthForm) -> App {
    ensure_test_data_dir();
    let mut app = App::new();
    app.locked = true;
    app.test_view = Some((W, H));
    app.wizard = None;
    app.auth_form = form;
    app
}

fn login_screen() -> App {
    let mut f = AuthForm::login();
    f.fields[0] = "Login!Hint-77".into();
    locked_with(f)
}

fn join_screen() -> App {
    let mut f = AuthForm::join();
    f.fields[1] = "482913".into();
    f.fields[2] = "Join!Hint-Pass3".into();
    f.fields[3] = "Join!Hint-Duress4".into();
    let mut app = locked_with(f);
    app.build_scene(W, H);
    app
}

/// The text of the hint as the tree carries it: the label that names the key.
fn hint_label(nodes: &[(NodeId, Node)]) -> Option<String> {
    nodes.iter().find_map(|(_, n)| {
        if n.role() != Role::Label {
            return None;
        }
        [n.value(), n.label()].into_iter().flatten().find(|s| s.contains(SHOW_KEY)).map(str::to_string)
    })
}

fn check(what: &str, app: &mut App, secret_fields: usize) -> Vec<String> {
    let up = app.build_a11y_tree();
    let mut problems = vec![];
    let Some(hint) = hint_label(&up.nodes) else {
        return vec![format!("{what}: the hint's label (naming {SHOW_KEY}) is no longer in the tree")];
    };
    let secrets: Vec<&Node> = up.nodes.iter().map(|(_, n)| n).filter(|n| n.role() == Role::PasswordInput).collect();
    if secrets.len() != secret_fields {
        problems.push(format!("{what}: {} secret fields in the tree, {secret_fields} expected", secrets.len()));
    }
    for n in secrets {
        let name = n.label().unwrap_or("(no name)");
        match n.description() {
            Some(d) if d == hint => {}
            Some(d) => problems.push(format!("{what}: the field « {name} » is described as {d:?}, not by the drawn hint {hint:?}")),
            None => problems.push(format!("{what}: the field « {name} » has no description; the hint {hint:?} is read in scan mode only")),
        }
    }
    problems
}

#[test]
fn l6_the_login_forms_password_is_described_by_the_hint() {
    let mut app = login_screen();
    let problems = check("the login form", &mut app, 1);
    done(app);
    assert!(problems.is_empty(), "L6:\n{}", problems.join("\n"));
}

#[test]
fn l6_each_secret_field_of_the_join_form_is_described_by_the_hint() {
    let mut app = join_screen();
    let problems = check("the join form", &mut app, 2);
    done(app);
    assert!(problems.is_empty(), "L6:\n{}", problems.join("\n"));
}
