//! Gate for C-DOC adoption step 1 (`doc/plans/C-DOC-adoption.md`): document
//! content is read through this crate (or the document service), never parsed
//! by hand. Fails on a new direct parse of stored content outside
//! `sovereign-docformat`, `sovereign-docs`, `sovereign-db` and
//! `sovereign-editor`.
//!
//! What it looks for, in non-test code: the v1 tolerant read
//! (`ContentFields::parse`, `parse_with_kind`) and hand-rolled envelope access
//! (`["body"]`, `get("body")`). It is a text scan, so it catches the shapes
//! that existed, not every possible parse; a new shape is added here when it
//! is found.
//!
//! Every remaining use is in `ALLOWED`, each naming the adoption step that
//! removes it. A line that leaves the allow-list must be removed from it.
//!
//! Step 2 (editor save paths) adds the second gate: no document write around
//! the service — no `restore_document`, no auto-commit engine — except the
//! writes of later steps, listed in `WRITES_ALLOWED` with their step. Step 3
//! (creators) adds the creations around the service to it.
//!
//! **Non-test code** (step 3): an item annotated `#[cfg(test)]` — a test
//! module, a test-only helper or `use` — is skipped to its end, and the scan
//! resumes after it ([`non_test_lines`]). Until step 3 the scan stopped at a
//! file's first `#[cfg(test)]`, so a test-only `use` near the top of a file
//! hid the whole file (`sovereign-p2p/src/sync_service.rs`, whose sync apply
//! the write gate did not see).
//!
//! **Test-only files** (2026-10-19, `doc/plans/EDITOR-default.md`, *Settled
//! after the implementer's report*): a whole file declared only under a
//! `cfg` that requires `test` — `#[cfg(test)] mod name;`,
//! `#[cfg(all(test, …))]`, with or without `#[path = "…"]` — is skipped,
//! as are the files it declares in turn ([`test_only_files`]). A file
//! declared under `cfg(test)` somewhere and without it elsewhere is compiled
//! into production: it is scanned, and
//! `a_test_only_file_is_never_declared_without_cfg_test` fails on it. Only a
//! declaration on its own line(s) is read (attributes on the lines above or
//! on the same line); one the reader does not understand leaves its file
//! scanned — the rule can only skip less, never more.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Crates that own the stored format: they read it by construction.
const OWNERS: &[&str] = &["sovereign-docformat", "sovereign-docs", "sovereign-db", "sovereign-editor"];

/// Shapes of a direct parse of stored document content.
const PATTERNS: &[&str] = &["ContentFields::parse", "parse_with_kind(", "[\"body\"]", "get(\"body\")"];

/// (path under `crates/`, pattern, why it may stay and which step removes it).
///
/// Empty since step 6 (M6, `doc/plans/C-DOC-adoption.md`): the deprecated v1
/// tolerant read in `sovereign-core/src/content.rs` (`ContentFields::parse`,
/// `parse_with_kind`) is removed, so its two entries went with it. The entries
/// were removed by the step-6 test author, before the code: until the read is
/// removed, this test fails on it.
const ALLOWED: &[(&str, &str, &str)] = &[];

/// Shapes of a document write around the document service: restores and the
/// auto-commit engine (step 2, no caller left), overwrites and commits (step
/// 2), and creations (step 3). What remains belongs to a later step or is
/// listed with the reason it stays, in `WRITES_ALLOWED`.
const WRITE_PATTERNS: &[&str] = &[
    "restore_document(",
    "AutoCommitEngine",
    "commit_on_close",
    ".update_document(",
    "commit_document(",
    ".create_document(",
    "GraphDB::create_document(",
    "create_document_with_id(",
];

/// (path under `crates/`, pattern, which adoption step moves it, or why it
/// stays).
const WRITES_ALLOWED: &[(&str, &str, &str)] = &[
    ("sovereign-app/src/pii_sweep.rs", ".update_document(", "dead code, replaced by the C-PII sweep (K10)"),
    // Sync and backup restore no longer write documents (step 4, C-WIRE):
    // both go through the document service (`sovereign_p2p::wire`).
    // Step 3 stopped on the decoy (doc/plans/C-DOC-adoption.md, *Step 3*): it
    // is seeded into the duress persona's raw database before that persona's
    // keys exist on most paths; seeding through the service is C-SESS work.
    ("sovereign-app/src/duress.rs", ".create_document(", "duress decoy seeding: stopped at step 3, C-SESS"),
    // The skills' narrow store access. In a host, `create_document` reaches
    // the service (`sovereign_ai::docs_host::ServiceSkillDb`); the blanket
    // implementation over a store remains for reads and tests.
    ("sovereign-skills/src/db_bridge.rs", "GraphDB::create_document(", "SkillDbAccess over a store; hosts route creates to the service (ServiceSkillDb)"),
    ("sovereign-skills/src/skills/daily_journal.rs", ".create_document(", "SkillDbAccess call; routed to the service by the host"),
    ("sovereign-skills/src/skills/duplicate_document.rs", ".create_document(", "SkillDbAccess call; routed to the service by the host (a copy: docs_host::duplicate, the service's G20 duplicate)"),
    ("sovereign-skills/src/skills/file_import.rs", ".create_document(", "SkillDbAccess call; routed to the service by the host"),
    ("sovereign-skills/src/wasm/host_bridge.rs", ".create_document(", "SkillDbAccess call from a WASM skill; routed to the service by the host"),
    // A development probe of the encrypted layer itself (`run_auth_probe`):
    // it checks that a row is ciphertext at rest, below the service.
    ("sovereign-shell/src/app.rs", ".create_document(", "auth probe harness (dev only): tests the encrypted layer, not a creator"),
];

/// The lines of `text` outside test-only items, with their 0-based numbers.
///
/// An item annotated `#[cfg(test)]` (and any further attributes on it) is
/// skipped to its end: to the brace that closes it, or to its `;` for an item
/// with no body (a `use`, a `mod x;`). Braces inside string and char literals
/// and after `//` are not counted. If an item never closes, the rest of the
/// file is skipped — the old behaviour, never worse.
fn non_test_lines(text: &str) -> Vec<(usize, &str)> {
    const CFG_TEST: &str = "#[cfg(test)]";
    let lines: Vec<&str> = text.lines().collect();
    let mut out = vec![];
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim_start();
        if !trimmed.starts_with(CFG_TEST) {
            out.push((i, lines[i]));
            i += 1;
            continue;
        }
        // The item may start on the attribute's own line.
        let mut j = i;
        let mut first = trimmed[CFG_TEST.len()..].trim();
        if first.is_empty() {
            j += 1;
            while j < lines.len() && lines[j].trim_start().starts_with("#[") {
                j += 1;
            }
            first = lines.get(j).map(|l| l.trim()).unwrap_or("");
        }
        let mut braces = BraceCounter::default();
        let mut line = first;
        loop {
            braces.feed(line);
            if braces.opened && braces.depth <= 0 {
                break;
            }
            if !braces.opened && line.trim_end().ends_with(';') {
                break;
            }
            j += 1;
            match lines.get(j) {
                Some(l) => line = l,
                None => break,
            }
        }
        i = j + 1;
    }
    out
}

/// Brace depth across lines, outside string literals, char literals and line
/// comments.
#[derive(Default)]
struct BraceCounter {
    depth: i64,
    opened: bool,
    in_string: bool,
}

impl BraceCounter {
    fn feed(&mut self, line: &str) {
        let chars: Vec<char> = line.chars().collect();
        let mut k = 0;
        while k < chars.len() {
            let c = chars[k];
            if self.in_string {
                match c {
                    '\\' => k += 1,
                    '"' => self.in_string = false,
                    _ => {}
                }
            } else {
                match c {
                    '"' => self.in_string = true,
                    '/' if chars.get(k + 1) == Some(&'/') => return,
                    '\'' if chars.get(k + 2) == Some(&'\'') => k += 2,
                    '{' => {
                        self.depth += 1;
                        self.opened = true;
                    }
                    '}' => self.depth -= 1,
                    _ => {}
                }
            }
            k += 1;
        }
    }
}

/// Every non-test line of every crate but the format owners that matches one
/// of `patterns`, with which `allowed` entries were used.
fn scan(patterns: &[&str], allowed: &[(&str, &str, &str)]) -> (Vec<String>, Vec<bool>) {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
    let mut found = vec![];
    let mut used = vec![false; allowed.len()];
    for entry in fs::read_dir(&crates).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if OWNERS.contains(&name.as_str()) {
            continue;
        }
        let mut files = vec![];
        rust_files(&entry.path().join("src"), &mut files);
        let test_only = test_only_files(&files);
        for f in files {
            if test_only.contains(&f) {
                continue;
            }
            let text = fs::read_to_string(&f).unwrap();
            let rel = f.strip_prefix(&crates).unwrap().to_string_lossy().replace('\\', "/");
            for (n, line) in non_test_lines(&text) {
                if line.trim_start().starts_with("//") {
                    continue;
                }
                for pat in patterns {
                    if !line.contains(pat) {
                        continue;
                    }
                    match allowed.iter().position(|(p, q, _)| *p == rel && q == pat) {
                        Some(i) => used[i] = true,
                        None => found.push(format!("{rel}:{}: `{pat}` — {}", n + 1, line.trim())),
                    }
                }
            }
        }
    }
    (found, used)
}

#[test]
fn no_document_write_around_the_service_except_the_later_steps() {
    let (found, used) = scan(WRITE_PATTERNS, WRITES_ALLOWED);
    assert!(
        found.is_empty(),
        "a document write around the document service (go through sovereign_docs, or list it \
         with the adoption step that moves it):\n{}",
        found.join("\n")
    );
    let stale: Vec<_> = WRITES_ALLOWED
        .iter()
        .zip(&used)
        .filter(|(_, u)| !**u)
        .map(|((p, q, _), _)| format!("{p} `{q}`"))
        .collect();
    assert!(stale.is_empty(), "write allow-list entries no longer needed — remove them:\n{}", stale.join("\n"));
}

#[test]
fn no_direct_parsing_of_document_content_outside_the_format_owners() {
    let (found, used) = scan(PATTERNS, ALLOWED);
    assert!(
        found.is_empty(),
        "direct parsing of document content (read it with sovereign_docformat or the document \
         service instead):\n{}",
        found.join("\n")
    );
    let stale: Vec<_> = ALLOWED
        .iter()
        .zip(&used)
        .filter(|(_, used)| !**used)
        .map(|((p, q, _), _)| format!("{p} `{q}`"))
        .collect();
    assert!(stale.is_empty(), "allow-list entries no longer needed — remove them:\n{}", stale.join("\n"));
}

/// The scan skips test-only items, and only them: code after a test-only
/// `use` or helper is scanned (the blind spot step 2 reported), a test module
/// is skipped whole, braces in strings and comments do not end it early.
#[test]
fn non_test_lines_skip_test_items_and_nothing_else() {
    let src = r#"use a::b;
#[cfg(test)]
use c::d;
fn real_one() { x.update_document(); }
#[cfg(test)]
fn helper() {
    let s = "}"; // }
    let c = '}';
}
fn real_two() {}
#[cfg(test)] mod inline_tests;
#[cfg(test)]
#[allow(dead_code)]
mod tests {
    fn t() { "{"; }
}
fn real_three() {}
"#;
    let kept: Vec<&str> = non_test_lines(src).into_iter().map(|(_, l)| l).collect();
    assert_eq!(
        kept,
        vec!["use a::b;", "fn real_one() { x.update_document(); }", "fn real_two() {}", "fn real_three() {}"]
    );
}

/// A body-less module declaration (`mod name;`) read from a source file: the
/// file it loads, and whether its `cfg` requires `test`.
#[derive(Debug, PartialEq)]
struct ModDecl {
    target: PathBuf,
    test_only: bool,
    /// `file:line`, for messages.
    at: String,
}

/// Whether a `cfg` predicate can hold only in a test build: `test`, or an
/// `all(…)` with such a predicate among its arguments. Anything else
/// (`any(test, …)`, `not(…)`, a feature) is not.
fn requires_test(pred: &str) -> bool {
    let p = pred.trim();
    if p == "test" {
        return true;
    }
    let Some(inner) = p.strip_prefix("all(").and_then(|r| r.strip_suffix(')')) else { return false };
    let mut depth = 0i32;
    let mut start = 0;
    let mut args = vec![];
    for (k, c) in inner.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                args.push(&inner[start..k]);
                start = k + 1;
            }
            _ => {}
        }
    }
    args.push(&inner[start..]);
    args.into_iter().any(requires_test)
}

/// Splits leading `#[…]` attributes off a trimmed line: (attributes, rest).
fn leading_attrs(mut s: &str) -> (Vec<String>, &str) {
    let mut attrs = vec![];
    while s.starts_with("#[") {
        let mut depth = 0i32;
        let mut end = None;
        for (k, c) in s.char_indices() {
            match c {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(k);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(e) = end else { break };
        attrs.push(s[..=e].to_string());
        s = s[e + 1..].trim_start();
    }
    (attrs, s)
}

/// The `mod name;` declarations of `file` (text `text`). `exists` says
/// whether a path is a file (for `name.rs` against `name/mod.rs`).
fn mod_declarations(file: &Path, text: &str, exists: &dyn Fn(&Path) -> bool) -> Vec<ModDecl> {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let dir = file.parent().unwrap_or(Path::new(""));
    let stem = file.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let base = if ["lib", "main", "mod"].contains(&stem.as_str()) { dir.to_path_buf() } else { dir.join(&stem) };
    let mut out = vec![];
    for (i, line) in lines.iter().enumerate() {
        let (mut attrs, rest) = leading_attrs(line);
        let rest = ["pub(crate) ", "pub(super) ", "pub "].iter().fold(rest, |r, p| r.strip_prefix(p).unwrap_or(r));
        let Some(name) = rest.strip_prefix("mod ").and_then(|r| r.trim().strip_suffix(';')).map(str::trim) else {
            continue;
        };
        if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        // The attributes above it (comments and blank lines between them).
        for above in lines[..i].iter().rev() {
            if above.is_empty() || above.starts_with("//") {
                continue;
            }
            if above.starts_with("#[") && above.ends_with(']') {
                attrs.extend(leading_attrs(above).0);
                continue;
            }
            break;
        }
        let path_attr = attrs.iter().find_map(|a| {
            let v = a.strip_prefix("#[path")?.trim_start().strip_prefix('=')?.trim();
            v.strip_suffix(']')?.trim().strip_prefix('"')?.strip_suffix('"').map(str::to_string)
        });
        let target = match path_attr {
            Some(p) => dir.join(p),
            None => {
                let flat = base.join(format!("{name}.rs"));
                let nested = base.join(name).join("mod.rs");
                if !exists(&flat) && exists(&nested) { nested } else { flat }
            }
        };
        let test_only = attrs.iter().any(|a| {
            a.strip_prefix("#[cfg(").and_then(|r| r.strip_suffix(")]")).is_some_and(requires_test)
        });
        out.push(ModDecl { target, test_only, at: format!("{}:{}", file.display(), i + 1) });
    }
    out
}

/// Every declaration of every file in `files`, by target file.
fn declarations(files: &[PathBuf]) -> HashMap<PathBuf, Vec<(PathBuf, ModDecl)>> {
    let mut by_target: HashMap<PathBuf, Vec<(PathBuf, ModDecl)>> = HashMap::new();
    for f in files {
        let text = fs::read_to_string(f).unwrap_or_default();
        for d in mod_declarations(f, &text, &|p: &Path| p.is_file()) {
            by_target.entry(normal(&d.target)).or_default().push((normal(f), d));
        }
    }
    by_target
}

/// `p` without `.` / `..` components (a `#[path = "../x.rs"]` names the same
/// file as the walk finds).
fn normal(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// The files of `files` compiled only into test builds: declared, and every
/// declaration of them either under a `cfg` that requires `test` or inside a
/// file that is itself test-only.
fn test_only_files(files: &[PathBuf]) -> HashSet<PathBuf> {
    let by_target = declarations(files);
    let mut test_only: HashSet<PathBuf> = HashSet::new();
    loop {
        let mut grew = false;
        for (target, decls) in &by_target {
            if test_only.contains(target) {
                continue;
            }
            if decls.iter().all(|(from, d)| d.test_only || test_only.contains(from)) {
                test_only.insert(target.clone());
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    files.iter().filter(|f| test_only.contains(&normal(f))).cloned().collect()
}

/// A file declared under `cfg(test)` is never also declared without it: such
/// a file is compiled into production, and the gates must read it. (The scan
/// already reads it, because `test_only_files` requires *every* declaration
/// to be test-only; this check makes the mistake visible.)
#[test]
fn a_test_only_file_is_never_declared_without_cfg_test() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
    let mut mixed = vec![];
    let mut skipped = 0;
    for entry in fs::read_dir(&crates).unwrap().flatten() {
        let mut files = vec![];
        rust_files(&entry.path().join("src"), &mut files);
        skipped += test_only_files(&files).len();
        for (target, decls) in declarations(&files) {
            let (test, prod): (Vec<_>, Vec<_>) = decls.iter().partition(|(_, d)| d.test_only);
            if !test.is_empty() && !prod.is_empty() {
                let at: Vec<&str> = decls.iter().map(|(_, d)| d.at.as_str()).collect();
                mixed.push(format!("{} — declared at {}", target.display(), at.join(", ")));
            }
        }
    }
    assert!(mixed.is_empty(), "a test-only file is also declared without cfg(test):\n{}", mixed.join("\n"));
    // The rule sees the shell's editor tests (`#[cfg(test)] #[path] mod`) and
    // the others; a reader that stopped seeing them would skip nothing.
    assert!(skipped >= 15, "only {skipped} test-only files found: the declaration reader is broken");
}

/// The declaration reader on a sample: `cfg(test)` above, on the same line,
/// with `#[path]`, `cfg(all(test, …))`; not `cfg(any(test, …))`, not a feature;
/// a plain declaration; a declaration after a comment line.
#[test]
fn the_declaration_reader_sees_what_it_should() {
    let src = r#"mod plain;
#[cfg(test)]
mod tests_a;
#[cfg(test)] mod tests_b;
// a comment
#[cfg(test)]
#[path = "../elsewhere_tests.rs"]
mod tests_c;
#[cfg(all(test, any(debug_assertions, feature = "x")))]
pub(crate) mod tests_d;
#[cfg(any(test, feature = "y"))]
mod not_test_any;
#[cfg(feature = "z")]
mod not_test_feature;
mod inline { fn f() {} }
"#;
    let file = Path::new("/c/src/app.rs");
    let decls = mod_declarations(file, src, &|_| false);
    let got: Vec<(String, bool)> =
        decls.iter().map(|d| (d.target.to_string_lossy().replace('\\', "/"), d.test_only)).collect();
    assert_eq!(
        got,
        vec![
            ("/c/src/app/plain.rs".to_string(), false),
            ("/c/src/app/tests_a.rs".to_string(), true),
            ("/c/src/app/tests_b.rs".to_string(), true),
            ("/c/src/../elsewhere_tests.rs".to_string(), true),
            ("/c/src/app/tests_d.rs".to_string(), true),
            ("/c/src/app/not_test_any.rs".to_string(), false),
            ("/c/src/app/not_test_feature.rs".to_string(), false),
        ]
    );
    // From lib.rs / main.rs / mod.rs, a declaration is beside the file.
    let decls = mod_declarations(Path::new("/c/src/main.rs"), "#[cfg(test)]\nmod t;\n", &|_| false);
    assert_eq!(decls[0].target.to_string_lossy().replace('\\', "/"), "/c/src/t.rs");
}

/// The test-only set: a file declared under `cfg(test)`, and what it declares,
/// are skipped; a file declared both ways is not.
#[test]
fn only_files_declared_under_cfg_test_alone_are_skipped() {
    let dir = std::env::temp_dir().join(format!("readers-gate-{}", std::process::id()));
    let src = dir.join("src");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(src.join("helpers")).unwrap();
    fs::write(src.join("real.rs"), "#[cfg(test)]\n#[path = \"real_tests.rs\"]\nmod t;\n").unwrap();
    fs::write(src.join("real_tests.rs"), "").unwrap();
    fs::write(src.join("helpers").join("mod.rs"), "mod deeper;\n").unwrap();
    fs::write(src.join("helpers").join("deeper.rs"), "").unwrap();
    fs::write(src.join("both.rs"), "").unwrap();
    // `both.rs` declared again without cfg(test), through a path.
    fs::write(src.join("lib.rs"), "mod real;\n#[cfg(test)]\nmod helpers;\n#[cfg(test)]\nmod both;\n#[path = \"both.rs\"]\nmod both_again;\n").unwrap();
    let mut files = vec![];
    rust_files(&src, &mut files);
    let skipped: HashSet<String> = test_only_files(&files)
        .into_iter()
        .map(|p| p.strip_prefix(&src).unwrap().to_string_lossy().replace('\\', "/"))
        .collect();
    let _ = fs::remove_dir_all(&dir);
    let want: HashSet<String> =
        ["real_tests.rs", "helpers/mod.rs", "helpers/deeper.rs"].iter().map(|s| s.to_string()).collect();
    assert_eq!(skipped, want, "lib.rs, real.rs and both.rs (declared both ways) are scanned");
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}
