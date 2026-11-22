//! C-SWEEP **SW9 — One routine, in the crate both hosts link** (spec ch. 09,
//! v2.0.111): « It is built on `sovereign-ai`'s ingest and run by the shell and
//! the phone app (Céline, 2026-11-17). The app's `pii_sweep.rs` is retired. »
//!
//! Held as the node's rule asks (NODE.md, 2026-11-14: « built » is written after
//! reading the host): a source read that each host's non-test code calls
//! `sovereign_ai::pii::sweep::run_sweep_pass`, and that the app's own routine is
//! gone. When the hosts run it (after a login, after a sync round that brought
//! content not yet analysed; on the phone only while the app is open) is held
//! by the hosts' own tests.
//!
//! **Its limits:** it reads names: a call that is never reached passes, and a
//! routine reached under another name is not seen.
//!
//! Written before the code by the test author of lot LS(a).

use std::path::{Path, PathBuf};

fn crates() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

/// The text of every `.rs` file under `dir`, test files left out (`*_tests.rs`,
/// `tests.rs`), with line comments removed.
fn non_test_sources(dir: &Path) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else { return out };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(non_test_sources(&p));
        } else if p.extension().is_some_and(|x| x == "rs") {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            if name.ends_with("_tests.rs") || name == "tests.rs" {
                continue;
            }
            let text = std::fs::read_to_string(&p).unwrap_or_default();
            let code: String = text
                .lines()
                .map(|l| match l.find("//") {
                    Some(i) => &l[..i],
                    None => l,
                })
                .collect::<Vec<_>>()
                .join("\n");
            out.push((p, code));
        }
    }
    out
}

/// Whether `code` calls the routine: `run_sweep_pass(`, as a call, not its
/// definition.
fn calls_the_routine(code: &str) -> bool {
    code.match_indices("run_sweep_pass").any(|(i, m)| {
        let before = code[..i].trim_end();
        let after = code[i + m.len()..].trim_start();
        after.starts_with('(') && !before.ends_with("fn")
    })
}

#[test]
fn sw9_the_shell_runs_the_shared_routine() {
    let hits: Vec<_> = non_test_sources(&crates().join("sovereign-shell/src"))
        .into_iter()
        .filter(|(_, c)| calls_the_routine(c))
        .map(|(p, _)| p)
        .collect();
    assert!(!hits.is_empty(), "the shell calls sovereign_ai::pii::sweep::run_sweep_pass nowhere");
}

#[test]
fn sw9_the_phone_app_runs_the_shared_routine() {
    let hits: Vec<_> = non_test_sources(&crates().join("sovereign-app/src"))
        .into_iter()
        .filter(|(_, c)| calls_the_routine(c))
        .map(|(p, _)| p)
        .collect();
    assert!(!hits.is_empty(), "the phone app calls sovereign_ai::pii::sweep::run_sweep_pass nowhere");
}

#[test]
fn sw9_the_routine_is_defined_once_in_sovereign_ai() {
    let mut defs = Vec::new();
    for krate in std::fs::read_dir(crates()).unwrap().flatten() {
        for (p, c) in non_test_sources(&krate.path().join("src")) {
            if c.contains("fn run_sweep_pass") || c.contains("fn run_sweep_cycle") {
                defs.push(p.strip_prefix(crates()).unwrap().to_string_lossy().replace('\\', "/"));
            }
        }
    }
    defs.sort();
    assert_eq!(defs, vec!["sovereign-ai/src/pii/sweep.rs".to_string()], "one routine, in sovereign-ai");
}

#[test]
fn sw9_the_apps_own_sweep_is_retired() {
    assert!(
        !crates().join("sovereign-app/src/pii_sweep.rs").exists(),
        "crates/sovereign-app/src/pii_sweep.rs is retired (SW9)"
    );
}

#[test]
fn sw9_the_reader_finds_what_it_looks_for() {
    assert!(calls_the_routine("let r = sovereign_ai::pii::sweep::run_sweep_pass (&docs, 10).await;"));
    assert!(!calls_the_routine("pub async fn run_sweep_pass(docs: &DocumentService) {}"));
    assert!(!calls_the_routine("let f = run_sweep_passes(1);"));
}
