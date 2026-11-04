//! v0.0.12 item 7 V2b, B1 on the shell's side — one wording, one derivation
//! (`doc/plans/V0012-item7-v2b-app-list.md`). Written before the code, by
//! another author: whoever implements V2b makes these pass without editing
//! this file. Source scans: they compile today and fail until `panels.rs`
//! takes its text from `sovereign-docs`.
//!
//! Seams (created in `sovereign-docs`, pinned by
//! `sovereign-docs/tests/item7_v2b_lines.rs`): `UNREADABLE_HEADING`,
//! `UNREADABLE_MEANING`, `unreadable_line(&UndecryptableDocument) -> String`.
//! `panels.rs` names them, as `sovereign_docs::NAME` or through a
//! `use sovereign_docs::…` line; it may keep a local alias of a constant
//! (`const X: &str = sovereign_docs::UNREADABLE_HEADING;`), never a copy of
//! its text. The shell's text does not change: `item7_tests.rs` keeps
//! pinning what the Alerts window says.
//!
//! What is read is the code: full-line comments (`//`, `///`, `//!`) are left
//! out, since a comment may name the heading. Test files (`*tests.rs`) are
//! left out of the crate-wide scan.

use std::path::{Path, PathBuf};

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// A file's code: its lines without the full-line comments.
fn code_of(path: &Path) -> String {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    text.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n")
}

fn panels() -> String {
    code_of(&src_dir().join("panels.rs"))
}

/// Every non-test `.rs` file of the crate's `src`, recursively.
fn crate_sources() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs")
                && !p.file_name().unwrap().to_string_lossy().ends_with("tests.rs")
            {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(&src_dir(), &mut out);
    assert!(out.iter().any(|p| p.ends_with("panels.rs")), "fixture: the scan sees panels.rs");
    out
}

/// Whether `code` takes `name` from `sovereign-docs`: by its path, or through
/// a `use sovereign_docs` line that lists it (and then uses it).
fn takes_from_docs(code: &str, name: &str) -> bool {
    if code.contains(&format!("sovereign_docs::{name}")) {
        return true;
    }
    let imported = code.lines().any(|l| l.trim_start().starts_with("use sovereign_docs") && l.contains(name));
    imported && code.matches(name).count() >= 2
}

/// B1: `panels.rs` takes the heading, the meaning and the per-document line
/// from `sovereign-docs`.
#[test]
fn b1_panels_uses_the_sovereign_docs_wording() {
    let code = panels();
    for name in ["UNREADABLE_HEADING", "UNREADABLE_MEANING", "unreadable_line"] {
        assert!(takes_from_docs(&code, name), "panels.rs takes `{name}` from sovereign-docs (B1)");
    }
}

/// B1: no literal copy of the heading or the meaning anywhere in the shell's
/// code (not only `panels.rs`: a copy moved to another file is still a copy).
#[test]
fn b1_the_shell_keeps_no_copy_of_the_heading_or_the_meaning() {
    for path in crate_sources() {
        let code = code_of(&path);
        for text in ["\"Documents that could not be read", "kept as it was, untouched", "try to read it again after its next update"] {
            assert!(!code.contains(text), "{} holds a copy of `{text}` (B1: it comes from sovereign-docs)", path.display());
        }
    }
}

/// B1: the per-document line is derived in `sovereign-docs` only — the short
/// id (key without `⟨ ⟩`), the date in local time and the line's words are
/// not computed again in `panels.rs`.
#[test]
fn b1_panels_does_not_derive_the_line_itself() {
    let code = panels();
    for (what, needle) in [
        ("the key's `⟨`, escaped", "\\u{27e8}"),
        ("the key's `⟩`, escaped", "\\u{27e9}"),
        ("the key's `⟨`", "\u{27e8}"),
        ("the date in local time", "modified_at.with_timezone"),
        ("the line's words", "last modified {"),
        ("the line's words", "\" (in the trash)\""),
    ] {
        assert!(!code.contains(needle), "panels.rs derives {what} itself (`{needle}`): B1 puts it in sovereign-docs");
    }
}
