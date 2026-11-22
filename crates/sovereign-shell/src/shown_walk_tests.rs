//! The walk of what she sees (a trial, observe mode; the Fable maille's proposal
//! of 2026-11-17, agreed by the four mailles; Céline, 2026-11-18: « ok pour
//! lancer l'essai »).
//!
//! A walk on the desktop, end to end: each launch is this test binary in a
//! child process, on a real profile of its own (`SOVEREIGN_DATA_DIR`), with a
//! real setup and a real login, as C-AREA's walk does. Where C-AREA's walk
//! asserts which files a session writes, this one asserts what the window
//! shows or speaks, after a relaunch, so a guarantee that holds only in memory
//! fails here.
//!
//! Each step names the guarantee it holds. Each step is seen red once, on its
//! own tip, before it counts.
//!
//! Its setup is its own, for now: sharing C-AREA's would mean changing that
//! file, whose author is another.

use std::io::Write as _;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

use super::first_run_tests::{
    first_frame, fill_wizard, frames_until, is_button, login_settled, press_finish, read_tree, setup_settled,
    shut_runtime, submit_login, DURESS, H, PRIMARY, W,
};
use super::App;

const CHILD_VAR: &str = "SHOWN_WALK_CHILD";
const OUT_VAR: &str = "SHOWN_WALK_OUT";

const SOURCE: &str = "Walk source";
const TARGET: &str = "Walk target";

/// C-LOG's step: the words of the log's first turn, which still verifies, and the marker of the
/// two after it, which the walk breaks.
const LOG_READABLE: &str = "Walk readable turn";
const LOG_BROKEN: &str = "WALKBROKENMARK";
/// The control below the quarantine notice (the Fable maille, by delegation of Céline,
/// 2026-11-18).
const SHOW_READABLE: &str = "Show what remains readable";

// ============================================================ the launches

fn out(line: &str) {
    let path = std::env::var_os(OUT_VAR).expect("the output file");
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path).expect("open the output file");
    writeln!(f, "{}", line.replace('\n', "\\n")).expect("write the output file");
}

fn end(mut app: App) {
    shut_runtime(&mut app);
    std::mem::forget(app);
}

fn frames(app: &mut App, n: usize) {
    for _ in 0..n {
        app.build_scene(W, H);
    }
}

fn login(app: &mut App) {
    first_frame(app);
    submit_login(app, PRIMARY);
    assert!(frames_until(app, 240, login_settled), "the login ends");
    assert!(!app.locked, "the primary opens: {:?}", app.auth_form.error);
    app.load_workspace_now();
    frames(app, 3);
}

fn create_thread(app: &mut App, name: &str) -> String {
    app.open_new_thread_prompt();
    assert!(app.new_thread_prompt.is_some(), "the name prompt opens");
    app.prompt_step(crate::new_thread::PromptStep::Create(name.into()));
    app.lane_labels
        .iter()
        .find(|l| l.name == name)
        .map(|l| l.thread_id.clone())
        .unwrap_or_else(|| panic!("no lane named {name:?} after Create"))
}

#[test]
#[ignore = "a launch, run by its parent test in a child process on a profile of its own"]
fn child_entry() {
    let Ok(scenario) = std::env::var(CHILD_VAR) else { return };
    match scenario.as_str() {
        "setup" => child_setup(),
        "merge" => child_merge(),
        "look" => child_look(),
        "turns" => child_turns(),
        "look_log" => child_look_log(),
        other => panic!("unknown scenario {other}"),
    }
}

fn child_setup() {
    let mut app = App::new();
    first_frame(&mut app);
    fill_wizard(&mut app, PRIMARY, DURESS);
    press_finish(&mut app);
    assert!(frames_until(&mut app, 240, setup_settled), "the setup ends");
    assert!(!app.locked, "setup unlocks");
    frames(&mut app, 3);
    let _ = app.request_quit();
    end(app);
}

/// Two lanes, the first merged into the second through the document service,
/// as the assistant's merge does.
fn child_merge() {
    let mut app = App::new();
    login(&mut app);
    let source = create_thread(&mut app, SOURCE);
    let target = create_thread(&mut app, TARGET);
    let docs = app.docs.clone().expect("a document service after login");
    app.rt.block_on(docs.thread_merge(&target, &source)).expect("merged");
    app.load_workspace_now();
    frames(&mut app, 3);
    let _ = app.request_quit();
    end(app);
}

/// After a relaunch and a login: every spoken name that names the source lane.
fn child_look() {
    let mut app = App::new();
    login(&mut app);
    let tree = read_tree(&mut app);
    for s in tree.all.iter().filter(|s| s.spoken.contains(SOURCE)) {
        out(&format!("spoken: {}", s.spoken));
    }
    let _ = app.request_quit();
    end(app);
}

/// C-LOG's step, after a login: three turns in the persona's log, under its real log key, as the
/// assistant's log writes them (the walk has no model to talk to). The app is shut first, so its
/// own log is closed.
fn child_turns() {
    let mut app = App::new();
    login(&mut app);
    let key = app.account_key.clone().expect("an account key after login").derive_session_log_key();
    let _ = app.request_quit();
    end(app);
    let paths = crate::crypto::orchestrator_state_paths(Some(sovereign_core::auth::PersonaKind::Primary));
    let mut log = sovereign_ai::session_log::SessionLog::open_encrypted(&paths.dir, key, Some(&paths.beacon))
        .expect("the log opens under its key");
    log.log_user_input("chat", LOG_READABLE, "chat");
    log.log_user_input("chat", &format!("{LOG_BROKEN} second"), "chat");
    log.log_user_input("chat", &format!("{LOG_BROKEN} third"), "chat");
}

/// C-LOG's step, after a relaunch and a login over the broken log: the control below the notice
/// is pressed, as she presses it, and every spoken name is written out.
fn child_look_log() {
    let mut app = App::new();
    login(&mut app);
    frames(&mut app, 5);
    let tree = read_tree(&mut app);
    let control = tree.all.iter().find(|s| is_button(s.role) && s.spoken.starts_with(SHOW_READABLE)).map(|s| s.id);
    out(&format!("control: {}", control.is_some()));
    if let Some(id) = control {
        app.a11y_action(accesskit::ActionRequest {
            action: accesskit::Action::Click,
            target_tree: accesskit::TreeId::ROOT,
            target_node: id,
            data: None,
        });
        frames(&mut app, 5);
        for s in read_tree(&mut app).all {
            out(&format!("spoken: {}", s.spoken));
        }
    }
    let _ = app.request_quit();
    end(app);
}

fn module() -> &'static str {
    module_path!()
}

/// Runs `scenario` in a child process on `profile`; panics unless it passed.
fn launch(scenario: &str, profile: &Path, out_file: &Path) {
    let rel = module().split_once("::").map(|(_, r)| r).unwrap_or(module());
    let exe = std::env::current_exe().expect("the test binary");
    let temp = out_file.with_extension("tmpdir");
    std::fs::create_dir_all(&temp).unwrap();
    let o = Command::new(exe)
        .args(["--exact", &format!("{rel}::child_entry"), "--ignored", "--nocapture", "--test-threads=1"])
        .env(CHILD_VAR, scenario)
        .env(OUT_VAR, out_file)
        .env("SOVEREIGN_DATA_DIR", profile)
        .env("TMP", &temp)
        .env("TEMP", &temp)
        .env("TMPDIR", &temp)
        .env_remove("SHELL_NO_AUTH")
        .env_remove("SHELL_AUTH_AUTO")
        .env_remove("SHELL_OPEN_WIZARD")
        .env_remove("SHELL_OPEN_JOIN")
        .env_remove("SHELL_SEED_PII_TEST")
        .output()
        .expect("spawn the child launch");
    let stdout = String::from_utf8_lossy(&o.stdout);
    let stderr = String::from_utf8_lossy(&o.stderr);
    assert!(
        o.status.success() && stdout.contains("1 passed"),
        "launch `{scenario}` on {} did not pass (status {:?}).\n--- stdout\n{stdout}\n--- stderr\n{stderr}",
        profile.display(),
        o.status,
    );
}

// =============================================================== the steps

/// C-MERGE, MG2 (spec ch. 03): an ended lane « shows its mark, « merged into
/// '{target}' on {YYYY-MM-DD} », which is also its accessibility label, after
/// its name ». Held here after a relaunch and a login, on a real profile.
#[test]
fn mg2_an_ended_lane_says_its_mark_after_a_relaunch() {
    let base = tempfile::Builder::new().prefix("sov-shown-walk-").tempdir().expect("a temporary base").keep();
    let profile = base.join("p");
    std::fs::create_dir_all(&profile).unwrap();
    let look = base.join("look.out");

    let mut timings = vec![];
    for (scenario, out_file) in [("setup", base.join("setup.out")), ("merge", base.join("merge.out")), ("look", look.clone())] {
        let t = Instant::now();
        launch(scenario, &profile, &out_file);
        timings.push(format!("{scenario} {:.1} s", t.elapsed().as_secs_f64()));
    }
    println!("shown walk, the launches: {}", timings.join(", "));

    let spoken: Vec<String> = std::fs::read_to_string(&look)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.strip_prefix("spoken: ").map(str::to_string))
        .collect();
    let mark = format!("merged into '{TARGET}' on ");
    let with_mark: Vec<&String> = spoken
        .iter()
        .filter(|s| {
            let (Some(n), Some(m)) = (s.find(SOURCE), s.find(&mark)) else { return false };
            let date = &s[m + mark.len()..];
            n < m
                && date.len() >= 10
                && date.as_bytes()[..10].iter().enumerate().all(|(i, b)| if i == 4 || i == 7 { *b == b'-' } else { b.is_ascii_digit() })
        })
        .collect();
    assert_eq!(with_mark.len(), 1, "one spoken name: {SOURCE:?}, then « {mark}{{YYYY-MM-DD}} »; read: {spoken:?}");
    let _ = std::fs::remove_dir_all(&base);
}

/// C-LOG, DG3 (spec ch. 08; the window's place decided by the Fable maille, by delegation of
/// Céline, 2026-11-18): after a relaunch over a log broken between launches, the control below
/// the quarantine notice opens the window. What is spoken shows the first turn, which still
/// verifies, counts the two from the break on, and speaks the broken part's marker nowhere. Held
/// on a real profile, with a real setup, a real login and the log under its real key.
#[test]
fn dg3_the_window_after_a_relaunch_shows_what_still_verifies() {
    let base = tempfile::Builder::new().prefix("sov-shown-walk-log-").tempdir().expect("a temporary base").keep();
    let profile = base.join("p");
    std::fs::create_dir_all(&profile).unwrap();
    let look = base.join("look_log.out");

    let mut timings = vec![];
    for (scenario, out_file) in [("setup", base.join("setup.out")), ("turns", base.join("turns.out"))] {
        let t = Instant::now();
        launch(scenario, &profile, &out_file);
        timings.push(format!("{scenario} {:.1} s", t.elapsed().as_secs_f64()));
    }
    // Between launches: the second and third lines swapped, so the chain breaks at entry 2 of 3.
    let log = sovereign_ai::orchestrator::OrchestratorStatePaths::for_persona(&profile, sovereign_core::auth::PersonaKind::Primary)
        .dir
        .join("session_log.jsonl");
    let mut lines: Vec<String> =
        std::fs::read_to_string(&log).expect("the log").lines().filter(|l| !l.trim().is_empty()).map(String::from).collect();
    assert_eq!(lines.len(), 3, "fixture: three turns in the log");
    lines.swap(1, 2);
    std::fs::write(&log, format!("{}\n", lines.join("\n"))).unwrap();

    let t = Instant::now();
    launch("look_log", &profile, &look);
    timings.push(format!("look_log {:.1} s", t.elapsed().as_secs_f64()));
    println!("shown walk, C-LOG's launches: {}", timings.join(", "));

    let read = std::fs::read_to_string(&look).unwrap_or_default();
    assert!(read.lines().any(|l| l == "control: true"), "« {SHOW_READABLE} » is offered below the notice; read:\n{read}");
    let spoken: Vec<&str> = read.lines().filter_map(|l| l.strip_prefix("spoken: ")).collect();
    assert!(spoken.iter().any(|s| s.contains(LOG_READABLE)), "the first turn is shown; spoken: {spoken:#?}");
    assert!(
        spoken.iter().any(|s| !s.contains(LOG_READABLE) && s.split(|c: char| !c.is_alphanumeric()).any(|w| w == "2")),
        "the two entries from the break on are counted; spoken: {spoken:#?}"
    );
    let count = "2 entries are not shown: from the break on, they no longer verify.";
    assert!(spoken.iter().any(|s| s.contains(count)), "the count is said « {count} »; spoken: {spoken:#?}");
    assert!(!read.contains(LOG_BROKEN), "the broken part is spoken nowhere; read:\n{read}");
    let _ = std::fs::remove_dir_all(&base);
}
