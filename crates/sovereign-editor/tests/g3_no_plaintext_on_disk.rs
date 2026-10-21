//! Guarantee 3 — "No plaintext leaves memory except through the host's save.
//! No temp file, no swap file, no log line containing document text, no crash
//! report written by Sovereign containing it." (spec ch. 12, C-TEXT)
//!
//! This file holds ONE test on purpose: it repoints the process's temp, home,
//! cache and app-data directories, which are process-wide.

mod common;
use common::*;

use std::path::{Path, PathBuf};

fn list(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            out.push(p.clone());
            if p.is_dir() {
                list(&p, out);
            }
        }
    }
}

// "No temp file, no swap file" — every place a library would put a temp, swap,
// cache or state file stays empty, and the working directory gains nothing.
#[test]
fn g3_a_full_session_writes_no_file() {
    let base = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("g3-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let vars = [
        "TMP",
        "TEMP",
        "TMPDIR",
        "HOME",
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "XDG_CACHE_HOME",
        "XDG_DATA_HOME",
        "XDG_STATE_HOME",
        "XDG_CONFIG_HOME",
        "XDG_RUNTIME_DIR",
    ];
    let mut dirs = vec![];
    for v in vars {
        let d = base.join(v);
        std::fs::create_dir_all(&d).unwrap();
        std::env::set_var(v, &d);
        dirs.push(d);
    }
    let cwd = std::env::current_dir().unwrap();
    let mut cwd_before = vec![];
    list(&cwd, &mut cwd_before);
    cwd_before.sort();

    let marker = "G3DISKMARKER";
    let _ = session::exercise_everything(marker);

    for d in &dirs {
        let mut found = vec![];
        list(d, &mut found);
        assert!(found.is_empty(), "the editor wrote under {}: {found:?}", d.display());
    }
    let mut cwd_after = vec![];
    list(&cwd, &mut cwd_after);
    cwd_after.sort();
    assert_eq!(cwd_before, cwd_after, "the editor wrote in the working directory");
    let _ = std::fs::remove_dir_all(&base);
}
