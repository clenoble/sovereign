//! C-DOC adoption step 6 — the source scans (`doc/plans/C-DOC-adoption.md`,
//! *Step 6 … contract*, M4, M6, M7). Written before the code, by another
//! author: whoever implements step 6 makes these pass without editing this
//! file.
//!
//! - **M4 — Both hosts.** The shell's and the phone's production code start
//!   the migration through the one seam (`sovereign_docs::start_unlock_migration`),
//!   and nothing outside `sovereign-docs` runs `migrate_all` or
//!   `run_unlock_migration` itself — so both hosts run it the same way, with
//!   the same record. *When* they call it (at every unlock, after the
//!   persona's document service is built) and how the notice is shown are
//!   wiring inside the running hosts, checked at review (as S5-3 was).
//! - **M6 — The tolerant read ends.** `ContentFields::parse` and
//!   `parse_with_kind` do not exist, and nothing names them again, in any
//!   crate, test code included. (The readers gate,
//!   `sovereign-docformat/tests/readers_gate.rs`, loses its two step-6
//!   allow-list entries in the same commit as these tests.)
//! - **M7 — Nothing writes an old form.** Production code outside the service
//!   (`sovereign-docs`), the store (`sovereign-db`) and the format
//!   (`sovereign-docformat`, the reader of every form, G2) does not assign a
//!   document's `content`, serialise the v1 envelope (`ContentFields`), or
//!   build one by hand (a `"body"` key). The exceptions that remain are the
//!   ones the writers gate already lists, each with its reason; an exception
//!   that is no longer needed fails the test.
//!
//! Text scans, like the gates before them (`tests/common/scan.rs`): comments
//! and literals are blanked before looking for code, `#[cfg(test)]` items are
//! left out of "production".

mod common;

use std::path::{Path, PathBuf};

use common::scan::{blank, crate_dirs, crates_dir, production_files, rust_files};

// ----------------------------------------------------------------- helpers --

/// Lines (1-based) where `name` is called in `prod`, in any form: `name(`,
/// `.name(`, `path::name(`; not its definition (`fn name(`).
fn called(prod: &str, name: &str) -> Vec<usize> {
    let mut out = vec![];
    for (ln, line) in prod.lines().enumerate() {
        let mut from = 0;
        while let Some(rel) = line[from..].find(name) {
            let at = from + rel;
            from = at + name.len();
            let before_char = line[..at].chars().next_back();
            if before_char.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                continue;
            }
            if !line[at + name.len()..].trim_start().starts_with('(') {
                continue;
            }
            if line[..at].trim_end().ends_with("fn") {
                continue;
            }
            out.push(ln + 1);
        }
    }
    out
}

/// Files of `crate_dir/src` declared test-only in a form the shared scan does
/// not read (`#[cfg(test)]` / `#[cfg(all(test, …))]` followed by `#[path]`
/// and/or `mod name;`). Same rule as `ts1_one_thread_write_path.rs`.
fn more_test_only_files(crate_dir: &Path) -> Vec<PathBuf> {
    let mut out = vec![];
    for f in rust_files(&crate_dir.join("src")) {
        let text = std::fs::read_to_string(&f).unwrap_or_default();
        let lines: Vec<&str> = text.lines().map(str::trim).collect();
        for (i, l) in lines.iter().enumerate() {
            if !(l.starts_with("#[cfg(") && l.contains("test")) {
                continue;
            }
            let mut path_attr: Option<String> = None;
            for next in lines.iter().skip(i + 1).take(3) {
                if let Some(p) = next.strip_prefix("#[path = \"").and_then(|r| r.strip_suffix("\"]")) {
                    path_attr = Some(p.to_string());
                    continue;
                }
                if let Some(m) = next.strip_prefix("mod ").and_then(|r| r.strip_suffix(';')) {
                    let dir = f.parent().unwrap();
                    let stem = f.file_stem().unwrap().to_string_lossy().to_string();
                    let base =
                        if ["lib", "main", "mod"].contains(&stem.as_str()) { dir.to_path_buf() } else { dir.join(&stem) };
                    out.push(match &path_attr {
                        Some(p) => dir.join(p),
                        None => base.join(format!("{}.rs", m.trim())),
                    });
                }
                break;
            }
        }
    }
    out
}

/// `(crate name, path relative to crates/ with '/', production text, raw
/// production text)` for every production file of every crate. The raw text
/// keeps literals (and comments) but blanks the same `#[cfg(test)]` items.
fn production_sources() -> Vec<(String, String, String, String)> {
    let mut out = vec![];
    for dir in crate_dirs() {
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        let test_only = more_test_only_files(&dir);
        for (file, prod) in production_files(&dir) {
            if test_only.iter().any(|t| *t == file) {
                continue;
            }
            let rel = file.strip_prefix(crates_dir()).unwrap_or(&file).to_string_lossy().replace('\\', "/");
            let raw = std::fs::read_to_string(&file).expect("source");
            let raw_prod = keep_where_code_kept(&raw, &prod);
            out.push((name.clone(), rel, prod, raw_prod));
        }
    }
    out
}

/// `raw` with every character blanked that `production` blanked **as part of
/// a test item**. A character the production text blanked only because it is
/// inside a literal or a comment is kept: it is told apart by `blank(raw)`,
/// which blanks literals and comments but no item.
fn keep_where_code_kept(raw: &str, prod: &str) -> String {
    let r: Vec<char> = raw.chars().collect();
    let b: Vec<char> = blank(raw).chars().collect();
    let p: Vec<char> = prod.chars().collect();
    assert_eq!(r.len(), p.len(), "blanking keeps every character's position");
    // A test item is a run that `production` blanked though `blank` kept code
    // in it; its literal contents (blank in both) are blanked with it. A run
    // is cut at the end of the line it ends on, so a test item's last line is
    // covered whole.
    let mut out = r.clone();
    let mut in_item = false;
    for k in 0..r.len() {
        let code_dropped = b[k] != ' ' && b[k] != '\n' && p[k] == ' ';
        let code_kept = p[k] != ' ' && p[k] != '\n';
        if code_dropped {
            in_item = true;
        } else if code_kept {
            in_item = false;
        }
        if in_item && r[k] != '\n' {
            out[k] = ' ';
        }
    }
    out.into_iter().collect()
}

// ---------------------------------------------------------------------- M4 --

/// Both hosts start the migration through the one seam.
#[test]
fn m4_both_hosts_start_the_unlock_migration_through_the_seam() {
    let sources = production_sources();
    for host in ["sovereign-shell", "sovereign-app"] {
        let sites: Vec<String> = sources
            .iter()
            .filter(|(c, ..)| c == host)
            .flat_map(|(_, rel, prod, _)| {
                called(prod, "start_unlock_migration").into_iter().map(move |l| format!("{rel}:{l}"))
            })
            .collect();
        assert!(
            !sites.is_empty(),
            "{host}: its production code never calls `start_unlock_migration` (M4: each host runs the \
             migration at its own unlock, through the one seam)"
        );
    }
}

/// Nothing outside the service runs the migration itself: no host calls
/// `migrate_all` or `run_unlock_migration` directly (they would skip the
/// record, or run it twice).
#[test]
fn m4_only_the_seam_runs_the_migration() {
    let mut found = vec![];
    for (krate, rel, prod, _) in production_sources() {
        if krate == "sovereign-docs" {
            continue;
        }
        for name in ["migrate_all", "run_unlock_migration"] {
            for l in called(&prod, name) {
                found.push(format!("{rel}:{l}: {name}"));
            }
        }
    }
    assert!(found.is_empty(), "the migration run around the seam (M4):\n{}", found.join("\n"));
}

// ---------------------------------------------------------------------- M6 --

/// `ContentFields::parse` and `parse_with_kind` are removed, and nothing — in
/// any crate, production or test code — names them again.
#[test]
fn m6_the_tolerant_read_does_not_exist() {
    let content = crates_dir().join("sovereign-core/src/content.rs");
    let text = blank(&std::fs::read_to_string(&content).expect("sovereign-core/src/content.rs"));
    for def in ["fn parse(", "fn parse_with_kind("] {
        assert!(!text.contains(def), "sovereign-core/src/content.rs still defines `{def}` (M6)");
    }

    let mut found = vec![];
    let mut scanned = 0;
    for dir in crate_dirs() {
        for sub in ["src", "tests", "benches", "examples"] {
            for f in rust_files(&dir.join(sub)) {
                scanned += 1;
                let code = blank(&std::fs::read_to_string(&f).unwrap_or_default());
                let rel = f.strip_prefix(crates_dir()).unwrap_or(&f).to_string_lossy().replace('\\', "/");
                for (ln, line) in code.lines().enumerate() {
                    let mut hit = |what: &str| found.push(format!("{rel}:{}: {what}", ln + 1));
                    if line.split(|c: char| !(c.is_alphanumeric() || c == '_')).any(|w| w == "parse_with_kind") {
                        hit("parse_with_kind");
                    }
                    let mut from = 0;
                    while let Some(rel_at) = line[from..].find("ContentFields::parse") {
                        let end = from + rel_at + "ContentFields::parse".len();
                        from = end;
                        if !line[end..].chars().next().is_some_and(|c| c.is_alphanumeric() || c == '_') {
                            hit("ContentFields::parse");
                        }
                    }
                }
            }
        }
    }
    assert!(scanned > 200, "only {scanned} files scanned");
    assert!(found.is_empty(), "the v1 tolerant read is named again (M6):\n{}", found.join("\n"));
}

// ---------------------------------------------------------------------- M7 --

/// Crates allowed to write document content: the service, the store, and the
/// format (which reads every form and writes v2 only — its own suite, G1).
const CONTENT_OWNERS: &[&str] = &["sovereign-docs", "sovereign-db", "sovereign-docformat"];

/// What the M7 scan looks for, by tag:
/// - `assign`: an assignment to a `content` field (`x.content = …`), other
///   than a window's content (`WindowContent`);
/// - `envelope`: `.serialize()` in a file that uses `ContentFields` (the v1
///   envelope's serialiser);
/// - `body-key`: a `"body"` key in a literal (an envelope built by hand, in a
///   string, `format!` or `json!`).
const ASSIGN: &str = "assign";
const ENVELOPE: &str = "envelope";
const BODY_KEY: &str = "body-key";

/// (path under `crates/`, tag, why it stays). The same exceptions as the
/// writers gate (`sovereign-docformat/tests/readers_gate.rs`, `WRITES_ALLOWED`),
/// for the same reasons; none is new at step 6.
const EXCEPTIONS: &[(&str, &str, &str)] = &[
    (
        "sovereign-app/src/duress.rs",
        ASSIGN,
        "duress decoy seeding into the raw store (writers gate: stopped at step 3, C-SESS; BACKLOG §11, the \
         decoy store's redesign after v0.0.11). M5 checks that its rows convert like a real persona's.",
    ),
    (
        "sovereign-skills/src/db_bridge.rs",
        ASSIGN,
        "SkillDbAccess over a bare store (writers gate); hosts route creates to the service (ServiceSkillDb)",
    ),
    (
        "sovereign-shell/src/app.rs",
        ASSIGN,
        "the auth probe (dev only, writers gate): tests the encrypted layer below the service",
    ),
    ("sovereign-app/src/pii_sweep.rs", BODY_KEY, "dead code, replaced by the C-PII sweep (writers gate, K10)"),
];

#[test]
fn m7_nothing_writes_an_old_form_of_document_content() {
    let mut found = vec![];
    let mut used = vec![false; EXCEPTIONS.len()];
    let mut scanned = 0;
    for (krate, rel, prod, raw_prod) in production_sources() {
        if CONTENT_OWNERS.contains(&krate.as_str()) {
            continue;
        }
        scanned += 1;
        let mut hits: Vec<(usize, &str, String)> = vec![];

        for (ln, line) in prod.lines().enumerate() {
            let mut from = 0;
            while let Some(at) = line[from..].find(".content") {
                let start = from + at;
                let after = &line[start + ".content".len()..];
                from = start + ".content".len();
                if after.starts_with(|c: char| c.is_alphanumeric() || c == '_') {
                    continue; // `.content_kind`, …
                }
                let rest = after.trim_start();
                if rest.starts_with('=') && !rest.starts_with("==") && !rest[1..].trim_start().starts_with("WindowContent") {
                    hits.push((ln + 1, ASSIGN, line.trim().to_string()));
                }
            }
        }
        if prod.contains("ContentFields") {
            for (ln, line) in prod.lines().enumerate() {
                if line.contains(".serialize()") {
                    hits.push((ln + 1, ENVELOPE, line.trim().to_string()));
                }
            }
        }
        for (ln, line) in raw_prod.lines().enumerate() {
            let t = line.trim_start();
            if t.starts_with("//") {
                continue;
            }
            if line.contains("\"body\"") || line.contains("\\\"body\\\"") {
                hits.push((ln + 1, BODY_KEY, t.to_string()));
            }
        }

        for (ln, tag, line) in hits {
            match EXCEPTIONS.iter().position(|(p, t, _)| *p == rel && *t == tag) {
                Some(i) => used[i] = true,
                None => found.push(format!("{rel}:{ln}: [{tag}] {line}")),
            }
        }
    }
    assert!(scanned > 100, "only {scanned} production files scanned");
    assert!(
        found.is_empty(),
        "document content written in an old form, or around the service (M7: write through \
         sovereign_docs, which writes v2):\n{}",
        found.join("\n")
    );
    let stale: Vec<String> = EXCEPTIONS
        .iter()
        .zip(&used)
        .filter(|(_, u)| !**u)
        .map(|((p, t, _), _)| format!("{p} [{t}]"))
        .collect();
    assert!(stale.is_empty(), "M7 exceptions no longer needed — remove them:\n{}", stale.join("\n"));
}

/// The scan's own sanity: the matcher sees a bare call, a method call and a
/// path call, and not a definition; the raw production text drops a test
/// item, literals and all, and keeps a production literal.
#[test]
fn step6_scan_helpers_see_what_they_should() {
    let prod = "fn start_unlock_migration(x: u8) {}\nlet a = start_unlock_migration(d);\nx.migrate_all ().await;\nsovereign_docs::run_unlock_migration(&d);\nrestart_unlock_migration(d);\n";
    assert_eq!(called(prod, "start_unlock_migration"), vec![2]);
    assert_eq!(called(prod, "migrate_all"), vec![3]);
    assert_eq!(called(prod, "run_unlock_migration"), vec![4]);

    let raw = "fn real() { let s = \"{\\\"body\\\": 1}\"; }\n#[cfg(test)]\nmod tests {\n    const T: &str = r#\"{\"body\":\"x\"}\"#;\n}\nfn after() {}\n";
    let (prod, _) = common::scan::production(raw);
    let kept = keep_where_code_kept(raw, &prod);
    let lines: Vec<&str> = kept.lines().collect();
    assert!(lines[0].contains("\\\"body\\\""), "a production literal is kept");
    assert!(!lines[3].contains("body"), "a test item's literal is dropped");
    assert!(lines[5].contains("fn after()"), "code after the test item is kept");
}
