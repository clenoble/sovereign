//! C-SWEEP **SW1** (spec ch. 09, v2.0.111): « content shared into Sovereign
//! from another app on the phone (which takes the import's origin, not the
//! editor's) » is suspected. What an import's origin gives is held over the
//! real hook in `sovereign-ai` (`pii/c_sweep_tests.rs`,
//! `sw1_an_import_is_suspected`); here, read in the source, that the phone's
//! share command gives it that origin. (A page saved from the browser,
//! v2.0.112, is held over `new_web_document` in `sovereign-ai`.)
//!
//! **Its limits:** it reads the command's body by name: an origin built in a
//! helper the body calls is not followed.
//!
//! Written before the code by the test author of lot LS(a).

use std::path::Path;

fn body_of<'a>(src: &'a str, name: &str) -> &'a str {
    let at = src.find(&format!("fn {name}(")).unwrap_or_else(|| panic!("fn {name} not found"));
    let open = at + src[at..].find('{').unwrap();
    let mut depth = 0usize;
    for (i, c) in src[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
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

#[test]
fn sw1_the_phones_share_takes_the_imports_origin() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sovereign-app/src/tauri_commands/mobile.rs");
    let src = std::fs::read_to_string(&path).unwrap();
    let body = body_of(&src, "receive_shared_content");
    let code: String = body.lines().map(|l| l.split("//").next().unwrap_or("")).collect::<Vec<_>>().join("\n");
    assert!(!code.contains("Origin::Editor"), "content shared from another app is not written through the editor");
    assert!(code.contains("Origin::Import"), "content shared from another app takes the import's origin");
}

/// Every non-test function of the phone app, as (file, name, body without
/// line comments).
fn app_functions() -> Vec<(String, String, String)> {
    fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                let n = p.file_name().unwrap().to_string_lossy().into_owned();
                if !n.ends_with("_tests.rs") && n != "tests.rs" {
                    out.push(p);
                }
            }
        }
    }
    let mut files = Vec::new();
    walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../sovereign-app/src"), &mut files);
    let mut out = Vec::new();
    for f in files {
        let src = std::fs::read_to_string(&f).unwrap_or_default();
        let src: String = src.lines().map(|l| l.split("//").next().unwrap_or("")).collect::<Vec<_>>().join("\n");
        let mut from = 0;
        while let Some(at) = src[from..].find("fn ") {
            let start = from + at;
            from = start + 3;
            let name: String = src[start + 3..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            if name.is_empty() || !src[start..].starts_with(&format!("fn {name}(")) || !src[start..].contains('{') {
                continue;
            }
            let body = body_of(&src[start..], &name).to_string();
            out.push((f.file_name().unwrap().to_string_lossy().into_owned(), name, body));
        }
    }
    out
}

/// SW1, on the phone: « only while the app is open » (v2.0.111). Read in the
/// source: the app runs the sweep after a login and after a sync round, and
/// from no timer of its own (no function that calls the routine waits on an
/// interval or a sleep), so nothing sweeps in the background while the app is
/// closed. **Its limits:** it reads names in function bodies; a timer in a
/// helper that the calling function reaches is not seen, and a call never
/// reached passes.
#[test]
fn sw1_the_phone_sweeps_after_login_and_sync_and_on_no_timer() {
    let fns = app_functions();
    let callers: Vec<&(String, String, String)> = fns.iter().filter(|(_, _, b)| b.contains("run_sweep_pass(")).collect();
    assert!(!callers.is_empty(), "the phone app calls the sweep nowhere");
    for (file, name, body) in &callers {
        assert!(
            !body.contains("interval(") && !body.contains("sleep("),
            "{file}: `{name}` calls the sweep and waits on a timer: the phone sweeps only while open, after a login or a sync round"
        );
    }
    let mentions_sweep = |b: &str| b.contains("run_sweep_pass(") || b.contains("sweep");
    assert!(
        fns.iter().any(|(f, n, b)| f == "auth.rs" && n == "validate_password" && mentions_sweep(b)),
        "the login (validate_password) starts the sweep"
    );
    assert!(
        fns.iter().any(|(_, _, b)| b.contains("SyncCompleted") && mentions_sweep(b)),
        "the end of a sync round (SyncCompleted) starts the sweep"
    );
}
