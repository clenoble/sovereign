//! C-SUP in the native shell — the node's supervision (spec ch. 05, *The
//! node's supervision — contract (C-SUP)*, v2.0.102).
//!
//! Written before the code, by another author: whoever implements lot LQ makes
//! these pass **without editing this file**; a test that looks wrong is
//! reported, not worked around.
//!
//! A child module of `app` (declared in `app.rs`, like `first_run_tests`), so
//! it drives `App`'s own paths on a real, headless `App` (`App::new()` needs no
//! window and no GPU; `build_scene` lays a frame out without rendering it).
//! `SOVEREIGN_DATA_DIR` is set to a fresh temporary folder before any `App` is
//! made, so nothing touches the user's profile.
//!
//! - **SU2** (the words, the Fable maille by delegation of Céline, 2026-11-15):
//!   after the supervisor reports a stop (`App::on_sync_stopped`), the status
//!   line says « Sync stopped »; the notice says « Sync stopped unexpectedly.
//!   Nothing syncs until you restart it. », once per stop, never why; the
//!   gesture « Restart sync » is there while the notice is; a restart that
//!   fails says « Sync could not be restarted: {reason} ».
//! - **SU3, SU4:** the restart is `App::restart_sync`, the user's gesture; in a
//!   duress session it starts no node; nothing in the stop's path restarts it.
//! - **SU6:** the host calls the supervision, held at its call sites in the
//!   source (`p2p.rs` runs the node under `NodeSupervisor::spawn`; `app.rs`
//!   calls `on_sync_stopped` and `restart_sync`), as the AT8/AT6 call-site
//!   tests do, beside the function-level tests in
//!   `crates/sovereign-p2p/tests/csup_supervision.rs`.
//!
//! **Readings chosen** (reported with the tests): the status line is
//! `App::status_notice`; « once per stop » is read as: closed, the notice does
//! not come back while the same stop lasts, frames drawn; the gesture is a
//! button of that name in the accessibility tree while the notice shows; the
//! shell has no runtime lock, switch or logout as built, so SU5 on the shell is
//! the application's exit, where nothing can be said (held at the supervisor).

use std::path::PathBuf;
use std::sync::OnceLock;

use sovereign_core::auth::PersonaKind as CorePersona;
use sovereign_p2p::supervision::NodeEnd;

use super::first_run_tests::{notice_text, read_tree, W};
use super::App;

const STATUS: &str = "Sync stopped";
const TITLE: &str = "Sync stopped";
const TEXT: &str = "Sync stopped unexpectedly. Nothing syncs until you restart it.";
const GESTURE: &str = "Restart sync";
const FAILED: &str = "Sync could not be restarted: ";
const H: f64 = 800.0;

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
        let d = std::env::temp_dir().join(format!("sov-csup-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::env::set_var("SOVEREIGN_DATA_DIR", &d);
        d
    });
}

/// A headless shell with an open session's screen (unlocked).
fn open_shell() -> App {
    ensure_test_data_dir();
    let mut app = App::new();
    app.locked = false;
    app
}

fn frames(app: &mut App, n: usize) {
    for _ in 0..n {
        app.build_scene(W, H);
    }
}

// SU2: the shell says that sync stopped, in its status line, and once as a notice; never why.
#[test]
fn su2_the_shell_says_sync_stopped_in_its_status_line_and_once_as_a_notice() {
    let mut app = open_shell();
    app.on_sync_stopped(NodeEnd::Panicked("swarm poll failed at node.rs:123".into()));
    let status = app.status_notice().map(|(s, _)| s);
    assert_eq!(status.as_deref(), Some(STATUS), "the status line says « Sync stopped »");
    assert_eq!(notice_text(&app).as_deref(), Some(format!("{TITLE}\n{TEXT}").as_str()), "the notice, in the words");
    let said = notice_text(&app).unwrap_or_default();
    assert!(!said.contains("swarm poll failed") && !said.contains("node.rs"), "why it stopped goes to the log, not to the user");

    // Once per stop: closed, it does not come back while the same stop lasts.
    app.notice = None;
    frames(&mut app, 20);
    assert!(notice_text(&app).is_none(), "the notice came back for the same stop");
    assert_eq!(app.status_notice().map(|(s, _)| s).as_deref(), Some(STATUS), "the status line still says it");
}

// SU2 (rule A): the gesture « Restart sync » is there wherever the notice is shown.
#[test]
fn su2_the_gesture_restart_sync_is_there_while_the_notice_shows() {
    let mut app = open_shell();
    app.on_sync_stopped(NodeEnd::Returned);
    frames(&mut app, 1);
    assert!(notice_text(&app).is_some(), "precondition: the notice shows");
    let tree = read_tree(&mut app);
    assert!(
        tree.button_names().iter().any(|n| n == GESTURE),
        "no « Restart sync » button beside the notice; buttons: {:?}",
        tree.button_names()
    );
}

// SU2: a restart that fails says « Sync could not be restarted: {reason} ».
#[test]
fn su2_a_restart_that_fails_says_so_with_its_reason() {
    let mut app = open_shell();
    app.on_sync_stopped(NodeEnd::Failed("listener closed".into()));
    // No open session's keys in this shell: the restart cannot build a node.
    app.restart_sync();
    let said = notice_text(&app).unwrap_or_default();
    let at = said.find(FAILED).unwrap_or_else(|| panic!("no « {FAILED}{{reason}} » in the notice: {said:?}"));
    assert!(!said[at + FAILED.len()..].trim().is_empty(), "the reason is said");
    assert!(app.p2p.is_none(), "no node runs after a failed restart");
}

// SU4: in a duress session, the gesture starts no node (C-AREA AR4, « Aucun moteur »).
#[test]
fn su4_a_restart_in_a_duress_session_starts_no_node() {
    let mut app = open_shell();
    app.persona = Some(CorePersona::Duress);
    app.restart_sync();
    frames(&mut app, 5);
    assert!(app.p2p.is_none(), "a node was started in a duress session");
}

/// The body of `fn <name>` in `src`, from its signature to the closing brace
/// at its own indentation.
fn body_of<'a>(src: &'a str, signature: &str) -> &'a str {
    let start = src.find(signature).unwrap_or_else(|| panic!("`{signature}` is in the source"));
    let rest = &src[start..];
    let end = rest.find("\n    }\n").map(|e| e + 6).unwrap_or(rest.len());
    &rest[..end]
}

/// Every occurrence of `call` in `src` that is not the function's own
/// definition line.
fn calls(src: &str, call: &str, definition: &str) -> usize {
    src.match_indices(call).filter(|(i, _)| !src[..*i].ends_with("fn ") && !src[*i..].starts_with(definition)).count()
}

// SU3: nothing restarts the node by itself: the stop's path never starts one.
#[test]
fn su3_the_stop_s_path_starts_no_node() {
    let src = include_str!("app.rs");
    let body = body_of(src, "pub(crate) fn on_sync_stopped(");
    for starter in ["start_p2p", "restart_sync", "start_p2p_node", "NodeSupervisor::spawn"] {
        assert!(!body.contains(starter), "on_sync_stopped calls `{starter}`: a restart that is not the user's");
    }
}

// SU6: the shell runs its node under the supervision, and the report and the gesture are wired to it.
#[test]
fn su6_the_shell_calls_the_supervision() {
    let p2p = include_str!("p2p.rs");
    let flat: String = p2p.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(flat.contains("NodeSupervisor::spawn("), "p2p.rs runs the node under `NodeSupervisor::spawn`");
    assert!(
        !flat.contains("tokio::spawn(async move { node.run().await"),
        "p2p.rs still runs the node as a task nobody watches"
    );
    let app = include_str!("app.rs");
    assert!(
        calls(app, "self.on_sync_stopped(", "self.on_sync_stopped(&mut self") >= 1,
        "nothing in app.rs calls on_sync_stopped: the supervisor's report reaches no screen"
    );
    assert!(
        calls(app, "self.restart_sync(", "self.restart_sync(&mut self") >= 1,
        "nothing in app.rs calls restart_sync: the gesture « Restart sync » does nothing"
    );
}

// ------------------------------------------------------ v3: DEL-0019 and more --

/// The old sentence of a gesture that needs the node when sync was never
/// started.
const NOT_RUNNING: &str = "Sync isn't running. Log in to start the P2P node.";

fn has_gesture(app: &mut App) -> bool {
    frames(app, 1);
    read_tree(app).button_names().iter().any(|n| n == GESTURE)
}

/// A shell whose session is open (a KEK, so the Recovery gesture reaches its
/// node check) and whose sync stopped; the stop's notice has been closed.
fn stopped_shell() -> App {
    let mut app = open_shell();
    app.kek = Some(std::sync::Arc::new(sovereign_crypto::kek::Kek::generate()));
    app.on_sync_stopped(NodeEnd::Returned);
    app.notice = None;
    app
}

/// After `gesture`, the stop's notice shows again with its gesture, and never
/// the old sentence (DEL-0019).
fn assert_the_stop_s_notice_again(mut app: App, what: &str) {
    let said = notice_text(&app).unwrap_or_default();
    assert_eq!(said, format!("{TITLE}\n{TEXT}"), "{what}: the stop's notice is not shown again");
    assert!(!said.contains(NOT_RUNNING), "{what}: « Log in to start the P2P node » while logged in");
    assert!(has_gesture(&mut app), "{what}: no « Restart sync » beside the notice");
    assert_eq!(app.status_notice().map(|(s, _)| s).as_deref(), Some(STATUS), "{what}: the status line still says it");
}

// SU2 (v3): after a failed restart, sync is still stopped: the status line says it, and the gesture is still there.
#[test]
fn su2_after_a_failed_restart_sync_is_still_stopped_and_the_gesture_still_there() {
    let mut app = open_shell();
    app.on_sync_stopped(NodeEnd::Failed("listener closed".into()));
    app.restart_sync(); // no open session's keys here: it fails
    assert!(notice_text(&app).unwrap_or_default().contains(FAILED), "precondition: the restart failed and said so");
    assert_eq!(app.status_notice().map(|(s, _)| s).as_deref(), Some(STATUS), "a failed restart cleared « Sync stopped »");
    assert!(has_gesture(&mut app), "after a failed restart, « Restart sync » is gone");
}

// DEL-0019: while sync is stopped, « Sync now » shows the stop's notice again, with its gesture.
#[test]
fn del0019_sync_now_while_stopped_shows_the_stop_s_notice() {
    let mut app = stopped_shell();
    app.p2p_sync_now();
    assert_the_stop_s_notice_again(app, "Sync now");
}

// DEL-0019: while sync is stopped, pairing a device shows the stop's notice again, with its gesture.
#[test]
fn del0019_pairing_while_stopped_shows_the_stop_s_notice() {
    let mut app = stopped_shell();
    app.open_pairing();
    assert_the_stop_s_notice_again(app, "Pairing");
}

// DEL-0019: while sync is stopped, renaming a paired device shows the stop's notice again, with its gesture.
#[test]
fn del0019_renaming_a_paired_device_while_stopped_shows_the_stop_s_notice() {
    let mut app = stopped_shell();
    app.rename_paired_device("12D3KooWPeer", "Laptop");
    assert_the_stop_s_notice_again(app, "Device not renamed");
}

// DEL-0019: while sync is stopped, enrolling a guardian (Recovery) shows the stop's notice again, with its gesture.
#[test]
fn del0019_enrolling_a_guardian_while_stopped_shows_the_stop_s_notice() {
    let mut app = stopped_shell();
    app.open_guardian_enroll();
    assert_the_stop_s_notice_again(app, "Recovery");
}

// DEL-0019: when sync was never started (no stop), each gesture that needs the node keeps its old sentence.
#[test]
fn del0019_never_started_each_gesture_keeps_its_old_sentence() {
    let gestures: [(&str, fn(&mut App)); 4] = [
        ("Sync", |a| a.p2p_sync_now()),
        ("Pairing", |a| a.open_pairing()),
        ("Device not renamed", |a| a.rename_paired_device("12D3KooWPeer", "Laptop")),
        ("Recovery", |a| a.open_guardian_enroll()),
    ];
    for (title, gesture) in gestures {
        let mut app = open_shell();
        app.kek = Some(std::sync::Arc::new(sovereign_crypto::kek::Kek::generate()));
        gesture(&mut app);
        assert_eq!(
            notice_text(&app).as_deref(),
            Some(format!("{title}\n{NOT_RUNNING}").as_str()),
            "{title}: sync was never started, so the old sentence"
        );
        assert_ne!(app.status_notice().map(|(s, _)| s).as_deref(), Some(STATUS), "{title}: no stop, so no « Sync stopped »");
    }
}

// ------------------------------------------------------- v6: spec v2.0.105 --

// SU1 (Céline, 2026-11-16): a start that fails is said as a stop. A failing start cannot be driven here without a real node listening on this machine, so the failed start's branch in `start_p2p` is held at its source: it takes the stop's path (`on_sync_stopped`), never a notice of its own with the reason.
#[test]
fn su1_a_start_that_fails_takes_the_stop_s_path() {
    let src = include_str!("app.rs");
    let body = body_of(src, "pub(crate) fn start_p2p(&mut self)");
    let at = body.rfind("Err(").expect("start_p2p has a failed start's branch");
    let branch = &body[at..];
    assert!(
        branch.contains("on_sync_stopped("),
        "start_p2p's failed start does not take the stop's path: it is not said as a stop"
    );
}

// SU2 (the Fable maille, by delegation of Céline, 2026-11-16): the status line says « Sync stopped » first, then what it would say without it; an alert still open is still shown.
#[test]
fn su2_the_status_line_says_sync_stopped_first_then_the_rest() {
    let mut app = open_shell();
    app.unreadable_count = 1;
    let rest = app.status_notice().map(|(s, _)| s).expect("precondition: the line says something without the stop");
    app.on_sync_stopped(NodeEnd::Returned);
    let line = app.status_notice().map(|(s, _)| s);
    assert_eq!(line.as_deref(), Some(format!("{STATUS}; {rest}").as_str()), "« Sync stopped; {{the rest}} »");
}
