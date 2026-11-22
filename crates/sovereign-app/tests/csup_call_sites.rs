//! C-SUP on the app built with P2P — the node's supervision (spec ch. 05, *The
//! node's supervision — contract (C-SUP)*, v2.0.102), held at the app's call
//! sites.
//!
//! "SU6 — Each host calls the supervision, and the tests hold the call, not
//! only the supervising function: a « built » here is read against a caller."
//! The app's `AppState` and Tauri runtime are not built in a test, so the calls
//! are held in the source, as the AT8/AT6 call-site tests do. The supervising
//! function itself is tested in `crates/sovereign-p2p/tests/csup_supervision.rs`;
//! the words and « once per stop » on the app's screens in
//! `frontend/src/lib/stores/syncStopped.svelte.test.ts`.
//!
//! Written before the code, by another author; the implementer does not edit
//! this file. The names that join the backend to its frontend are chosen here:
//! the event `sync-stopped` and the command `restart_sync`.

use std::path::{Path, PathBuf};

fn read(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn flat(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The body of `fn <name>` in `src`, from its signature to the first closing
/// brace at the start of a line.
fn body_of<'a>(src: &'a str, signature: &str) -> &'a str {
    let start = src.find(signature).unwrap_or_else(|| panic!("`{signature}` is in the source"));
    let rest = &src[start..];
    let end = rest.find("\n}\n").map(|e| e + 3).unwrap_or(rest.len());
    &rest[..end]
}

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name().is_some_and(|n| n == "node_modules") {
                continue;
            }
            files_under(&p, out);
        } else if p.extension().is_some_and(|x| x == "ts" || x == "svelte") && !p.to_string_lossy().contains(".test.") {
            out.push(p);
        }
    }
}

// SU6, SU1: the app runs its node under the supervision, not as a task nobody watches.
#[test]
fn su6_the_app_runs_its_node_under_the_supervision() {
    let src = flat(&read("src/sync_startup.rs"));
    assert!(src.contains("NodeSupervisor::spawn("), "sync_startup.rs runs the node under `NodeSupervisor::spawn`");
    assert!(
        !src.contains("spawn(async move { node.run().await"),
        "sync_startup.rs still runs the node as a task nobody watches"
    );
}

// SU2, SU6: a stop the supervisor reports reaches the app's screens as the `sync-stopped` event.
#[test]
fn su2_a_stop_reaches_the_screens_as_the_sync_stopped_event() {
    let mut emitted = false;
    for file in ["src/sync_startup.rs", "src/tauri_events.rs"] {
        if read(file).contains("\"sync-stopped\"") {
            emitted = true;
        }
    }
    assert!(emitted, "nothing emits the `sync-stopped` event: a stop would be silent on the app");
}

// SU5: the app's asked end (logout, switch, lock: `stop_p2p_node`) goes through the supervisor, so it says nothing.
#[test]
fn su5_the_app_s_asked_end_goes_through_the_supervisor() {
    let src = read("src/sync_startup.rs");
    let body = body_of(&src, "pub async fn stop_p2p_node(");
    assert!(body.contains(".stop()"), "stop_p2p_node ends the node without telling its supervisor: the asked end would be said as a stop");
}

// SU3, SU6: the restart is the user's gesture: a `restart_sync` command, registered, and called by the gesture on the screens.
#[test]
fn su3_the_restart_is_a_registered_command_the_gesture_calls() {
    let lib = read("src/lib.rs");
    let start = lib.find("generate_handler![").expect("run_tauri's generate_handler!");
    let end = start + lib[start..].find("])").expect("the end of generate_handler!");
    assert!(
        lib[start..end].lines().any(|l| l.trim().trim_end_matches(',').ends_with("restart_sync")),
        "the `restart_sync` command is not registered"
    );
    let mut files = vec![];
    files_under(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../frontend/src"), &mut files);
    assert!(
        files.iter().any(|f| std::fs::read_to_string(f).unwrap_or_default().contains("'restart_sync'")),
        "no screen invokes `restart_sync`: the gesture « Restart sync » does nothing"
    );
}

/// The text of the call that starts at `at` (the `(` after a name), up to its
/// matching `)`; strings and comments are not parsed, which these call sites
/// do not need.
fn call_at(src: &str, at: usize) -> &str {
    let open = at + src[at..].find('(').expect("an opening parenthesis");
    let mut depth = 0usize;
    for (i, c) in src[open..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return &src[at..open + i + 1];
                }
            }
            _ => {}
        }
    }
    &src[at..]
}

/// The innermost `{ ... }` block of `src` that holds byte `at`.
fn block_around(src: &str, at: usize) -> &str {
    let bytes = src.as_bytes();
    let mut depth = 0i32;
    let mut open = None;
    for i in (0..at).rev() {
        match bytes[i] {
            b'}' => depth += 1,
            b'{' => {
                if depth == 0 {
                    open = Some(i);
                    break;
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    let open = open.expect("an enclosing block");
    let mut depth = 0i32;
    for (i, &b) in bytes[open..].iter().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return &src[open..open + i + 1];
                }
            }
            _ => {}
        }
    }
    &src[open..]
}

/// What starts a node on the app.
const STARTERS: [&str; 5] = ["start_p2p_node", "start_sync_now", "restart_sync", "SovereignNode::new", "NodeSupervisor::spawn"];

/// Every file of the app's sources.
fn app_sources() -> Vec<(PathBuf, String)> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    let mut files = vec![];
    walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
    files.into_iter().map(|p| {
        let s = std::fs::read_to_string(&p).unwrap_or_default();
        (p, s)
    }).collect()
}

// SU4: the `restart_sync` command starts no node in a duress session. It checks the session's persona itself, or goes through `start_p2p_node`, whose first step is that check (C-AREA AR4, « Aucun moteur »).
#[test]
fn su4_restart_sync_goes_through_the_duress_guard() {
    // The guard the node's start already keeps: its first step.
    let startup = read("src/sync_startup.rs");
    let start = body_of(&startup, "pub async fn start_p2p_node(");
    let first = start.find('{').map(|i| start[i + 1..].trim_start()).unwrap_or_default();
    assert!(
        first.starts_with("if state.is_duress_session()"),
        "start_p2p_node no longer begins with the duress check: the guard a restart relies on moved"
    );
    // The command.
    let sources = app_sources();
    let (file, src) = sources
        .iter()
        .find(|(_, s)| s.contains("fn restart_sync("))
        .expect("no `restart_sync` function in the app: the gesture « Restart sync » has no command");
    let at = src.find("fn restart_sync(").unwrap();
    let before = &src[..at];
    let attrs = &before[before.rfind("\n\n").unwrap_or(0)..];
    assert!(attrs.contains("#[tauri::command]"), "{}: `restart_sync` is not a Tauri command", file.display());
    let body = block_around(src, at + src[at..].find('{').unwrap() + 1);
    let checks_itself = body.contains("is_duress_session()");
    let goes_through_the_guard = body.contains("start_p2p_node(") || body.contains("start_sync_now(");
    assert!(
        checks_itself || goes_through_the_guard,
        "{}: restart_sync neither checks the session's persona nor goes through start_p2p_node",
        file.display()
    );
    if body.contains("SovereignNode::new") || body.contains("NodeSupervisor::spawn") {
        let guard = body.find("is_duress_session()").expect("restart_sync builds a node without the duress check");
        let build = body.find("SovereignNode::new").or_else(|| body.find("NodeSupervisor::spawn")).unwrap();
        assert!(guard < build, "restart_sync builds a node before it checks the persona");
    }
}

// SU3: nothing on the app's stop path starts a node: neither the supervisor's on_stop nor the code that emits `sync-stopped` calls a starter.
#[test]
fn su3_the_app_s_stop_path_starts_no_node() {
    let startup = read("src/sync_startup.rs");
    let at = startup.find("NodeSupervisor::spawn(").expect("sync_startup.rs runs the node under `NodeSupervisor::spawn`");
    let call = call_at(&startup, at);
    let after_node = &call["NodeSupervisor::spawn(".len()..];
    for starter in STARTERS {
        assert!(
            !after_node.contains(starter),
            "the supervision of the app's node calls `{starter}`: a restart that is not the user's"
        );
    }
    let mut emitters = 0;
    for (file, src) in app_sources() {
        for (i, _) in src.match_indices("\"sync-stopped\"") {
            emitters += 1;
            let block = block_around(&src, i);
            for starter in STARTERS {
                assert!(
                    !block.contains(starter),
                    "{}: the code that emits `sync-stopped` calls `{starter}`: a restart that is not the user's",
                    file.display()
                );
            }
        }
    }
    assert!(emitters > 0, "nothing emits the `sync-stopped` event");
}

// DEL-0019 (the Fable maille, by delegation of Céline): the three commands that need the node answer, when no node runs, through `AppState::node_refusal`: the stop's notice while sync is stopped, their own sentence (still passed to it) when sync was never started.
#[test]
fn del0019_the_app_s_node_refusals_go_through_the_stop_s_notice() {
    let places = [
        ("src/tauri_commands/pairing.rs", "pairing requires the P2P node"),
        ("src/tauri_commands/guardian.rs", "guardian enrollment requires the P2P node"),
        ("src/tauri_commands/backup.rs", "backup requires the P2P node"),
    ];
    for (file, sentence) in places {
        let src = read(file);
        let at = src.find(sentence).unwrap_or_else(|| panic!("{file}: its own sentence « {sentence} … » is kept for a sync never started"));
        // The refusal: the `ok_or_else(` around that sentence.
        let open = src[..at].rfind("ok_or_else(").unwrap_or_else(|| panic!("{file}: the refusal is an ok_or_else"));
        let refusal = call_at(&src, open);
        assert!(refusal.contains(sentence), "{file}: the sentence sits in the refusal's own ok_or_else");
        assert!(
            refusal.contains("node_refusal("),
            "{file}: the refusal does not go through `node_refusal`: while sync is stopped it would say « {sentence} … » instead of the stop's notice"
        );
    }
}

// SU1 (Céline, 2026-11-16): a start that fails is said as a stop: start_sync_now's failed start takes the stop's path (`on_sync_stopped`), not only the log.
#[test]
fn su1_the_app_s_failed_start_takes_the_stop_s_path() {
    let src = read("src/tauri_commands/auth.rs");
    let body = body_of(&src, "pub(crate) async fn start_sync_now(");
    assert!(
        body.contains("on_sync_stopped("),
        "start_sync_now's failed start is written to the log only: it is not said as a stop"
    );
}

// SU1 (Céline, 2026-11-16): the stop reaches the screen after the login has answered: the login asks `sync_is_stopped` after its reset, so the command is registered.
#[test]
fn su1_the_app_registers_sync_is_stopped() {
    let lib = read("src/lib.rs");
    let start = lib.find("generate_handler![").expect("run_tauri's generate_handler!");
    let end = start + lib[start..].find("])").expect("the end of generate_handler!");
    assert!(
        lib[start..end].lines().any(|l| l.trim().trim_end_matches(',').ends_with("sync_is_stopped")),
        "the `sync_is_stopped` command is not registered: the login cannot ask whether its session's sync stopped"
    );
}
