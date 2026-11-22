//! v0.0.13 rc2 (Céline, 2026-11-20; the way decided by the Fable maille, by delegation of Céline,
//! the same day): nothing reaches the phone's system log by default. On Android, the process's
//! stdout and stderr go to the system log, which USB debugging, a bug report or a forensic tool
//! can read. So the phone's entry, `run()`, installs no log subscriber, and sets a panic hook
//! that writes nothing (a panic's default message goes to stderr, and can carry a file's path).
//!
//! No cargo test runs on a phone: these tests read the source of `run()`, the function compiled
//! for Android and iOS, as SW1's host tests read theirs. What holds it on a candidate is the
//! measure on its APK (the release checklist). Written before the code, by another author than
//! the implementer; the implementer does not edit this file.

const LIB: &str = include_str!("lib.rs");

/// The text of the phone's `run()`: the function that follows the Android-and-iOS `cfg` and the
/// mobile entry point's attribute, from its `{` to the matching `}`.
fn phone_run_body() -> &'static str {
    let marker = "#[cfg_attr(mobile, tauri::mobile_entry_point)]";
    let at = LIB.find(marker).expect("the phone's entry point is in lib.rs, under tauri::mobile_entry_point");
    let after = &LIB[at..];
    let sig = after.find("pub fn run()").expect("the phone's entry is `pub fn run()`");
    let open = sig + after[sig..].find('{').expect("run() has a body");
    let mut depth = 0usize;
    for (i, c) in after[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &after[open..=open + i];
                }
            }
            _ => {}
        }
    }
    panic!("run()'s body is not closed");
}

/// Code only: each line cut at `//`, so a comment that names a subscriber does not count.
fn code(text: &str) -> String {
    text.lines().map(|l| l.split("//").next().unwrap_or("")).collect::<Vec<_>>().join("\n")
}

/// The text passed to `std::panic::set_hook(...)` in `body`, from its `(` to the matching `)`.
fn set_hook_argument(body: &str) -> Option<String> {
    let at = body.find("set_hook(")?;
    let open = at + "set_hook".len();
    let mut depth = 0usize;
    for (i, c) in body[open..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(body[open..=open + i].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

/// The phone's entry installs no log subscriber: no call to the desktop's `init_tracing`, and no
/// subscriber of its own.
#[test]
fn the_phones_entry_installs_no_log_subscriber() {
    let body = code(phone_run_body());
    for name in ["init_tracing", "set_global_default", "tracing_subscriber", "fmt()", "set_logger", "android_logger"] {
        assert!(!body.contains(name), "the phone's run() installs a log writer: it names `{name}`:\n{body}");
    }
}

/// The phone's entry sets a panic hook, in its own body, and the hook writes nothing.
#[test]
fn the_phones_entry_sets_a_panic_hook_that_writes_nothing() {
    let body = code(phone_run_body());
    assert!(
        body.contains("panic::set_hook("),
        "the phone's run() sets a panic hook (std::panic::set_hook), in its own body:\n{body}"
    );
    let hook = set_hook_argument(&body).expect("the panic hook's argument is readable");
    for writer in [
        "print!", "println!", "eprint!", "eprintln!", "dbg!", "write", "tracing", "log::", "stderr", "stdout",
        "take_hook", "default_hook",
    ] {
        assert!(!hook.contains(writer), "the phone's panic hook writes, or calls the default hook: it names `{writer}`:\n{hook}");
    }
}

/// The desktop is untouched: `run_cli()` still installs the desktop's subscriber.
#[test]
fn the_desktops_command_line_still_installs_its_subscriber() {
    let at = LIB.find("pub fn run_cli()").expect("the desktop's entry is `pub fn run_cli()`");
    let body = &LIB[at..at + LIB[at..].find("\n}\n").expect("run_cli() ends")];
    assert!(code(body).contains("init_tracing"), "run_cli() installs the desktop's subscriber, as before");
}
