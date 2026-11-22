//! C-AREA **AR8 — No new shared path** (spec ch. 07, *One persona, one area —
//! contract (C-AREA)*). A check in the gate: it counts, per crate, the calls to
//! every function that returns a path under the Sovereign directory without
//! taking a persona, and fails if a count grows, or if a new function of that
//! kind appears without being named here.
//!
//! Written before the code, by another author than whoever moves the shared
//! files (L1c). Whoever implements does not edit the counts upward to pass: a
//! count that must grow is a decision, reported, not worked around. Lowering a
//! count after a shared path is removed is the ratchet tightening: the check
//! prints which counts went down, so they can be lowered here.
//!
//! **Where it lives:** a test of the workspace (`cargo test --workspace`, the
//! gate CI and `_prepublish` run), as the other source scans of
//! `sovereign-core/tests/` are. It reads the sources, it runs nothing.
//!
//! **What it counts, exactly** (precisions of claude-coord `from-laptop/0224`
//! and `from-fable-2nde-position/0011`):
//! - **The scanned code:** every `crates/<crate>/src/**/*.rs`, *outside test
//!   code*. Test code is: a file named `*_tests.rs` or `tests.rs`; a file a
//!   `#[cfg(test)]` (or `#[cfg(all(test, …))]`) module declaration points at
//!   with `#[path = "…"]`; and every item that follows a `#[cfg(test)]` /
//!   `#[cfg(all(test, …))]` attribute inside a scanned file (an inline
//!   `mod tests { … }`, a test-only `fn`). Comments are removed, and string
//!   literals are blanked before calls are matched.
//!   `crates/<crate>/tests/`, `benches/` and `examples/` are not scanned.
//! - **A call** is the function's name as an identifier (not preceded by an
//!   identifier character, `.` or `fn `) followed by `(`, optionally after
//!   whitespace. Its definition (`fn name(`) is not a call. A qualified call
//!   (`sovereign_core::sovereign_dir()`, `crate::crypto::crypto_dir()`) counts.
//!   A name used without being called (passed as a `fn` value) is not counted;
//!   there is none today.
//! - **Per crate:** the count is keyed by the function (its name and the file
//!   that defines it) and the crate whose sources hold the call
//!   (`crates/<crate>`). A function's calls are counted in its own crate and
//!   in the crates whose `Cargo.toml` names its crate as a dependency, so two
//!   functions of the same name in two crates (`crypto_dir` in `sovereign-app`
//!   and in `sovereign-shell`, `access_dir` likewise) are counted apart. A call
//!   of another function that happens to bear the same name, in such a crate,
//!   counts too (`dirs::home_dir()` would count as `home_dir`): the scan reads
//!   names, it does not resolve paths.
//! - **The functions of that kind** are found, not only listed:
//!   1. **Roots:** a function whose body reads `"SOVEREIGN_DATA_DIR"` or joins
//!      `".sovereign"` (today `sovereign_core::sovereign_dir`), and the
//!      functions the agreement names as roots (`sovereign_core::home_dir`,
//!      whose result is the parent of the default Sovereign directory).
//!   2. **Wrappers, to a fixed point:** a function whose return type names
//!      `PathBuf` (or is a `String`/`OsString`/`Path` on a function named
//!      `*_path`, `*_dir` or `*_root`), whose parameters name no persona (no `persona` or
//!      `PersonaKind`, any case), and whose body calls a function already of
//!      that kind. `crypto_dir()` = `sovereign_dir().join("crypto")` is one;
//!      `auth_store_path()` = `crypto_dir().join("auth.store")` is another.
//!   Every function found must be in [`NAMED`], with its definition's file; a
//!   function found and not named fails the check (« a new function of that
//!   kind fails the check until it is named »). A named function no longer
//!   found also fails: the check would otherwise go blind without saying so.
//! - **The agreement named four** (`sovereign_dir`, `home_dir`, `crypto_dir`,
//!   `comms_config_path`). The rule above finds more today: the wrappers of
//!   those four that return a path without a persona. They are named below,
//!   with their counts; their calls are shared paths as much as the four's.
//!   `crypto_dir` is named twice, once per crate that defines one.
//!
//! **The counts are recorded here, by this check, when it was written**
//! (2026-11-13, `main` at `ba1dc18`); the spec carries no number. They are what
//! the scanner above counted on that tree, pasted from its own output
//! (`C_AREA_AR8_PRINT=1`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// A function that returns a persona-blind path under the Sovereign directory.
struct Named {
    name: &'static str,
    /// The file (relative to `crates/`) that defines it.
    defined_in: &'static str,
    /// Calls per crate, today (2026-11-13). A crate absent here has 0.
    counts: &'static [(&'static str, usize)],
}

/// Roots named by the agreement, whatever their body reads.
const NAMED_ROOTS: &[&str] = &["home_dir"];

/// Every function of that kind, with today's baseline. Filled from the
/// scanner's own output (`C_AREA_AR8_PRINT=1 cargo test -p sovereign-core
/// --test c_area_ar8_shared_paths -- --nocapture`).
const NAMED: &[Named] = &[
    // BASELINE-START (2026-11-13, main ba1dc18; lowered 2026-11-14 on L1c's code, v0013/l1c-code a9729af
    // merged with main: comms_config_path 3 -> 1, device_name.rs path 2 -> 1, sovereign_dir in the app 12 -> 11)
    // wrapper found: sovereign_dir()/recovery (access recovery).
    Named { name: "access_dir", defined_in: "sovereign-app/src/tauri_commands/access_recovery.rs", counts: &[("sovereign-app", 6)] },
    // wrapper found: sovereign_dir()/recovery (access recovery).
    Named { name: "access_dir", defined_in: "sovereign-shell/src/recovery.rs", counts: &[("sovereign-shell", 8)] },
    // wrapper found: crypto/auth.store.
    Named { name: "auth_store_path", defined_in: "sovereign-shell/src/crypto.rs", counts: &[("sovereign-shell", 4)] },
    // wrapper found: crypto/kdf.json.
    Named { name: "cli_kdf_path", defined_in: "sovereign-app/src/setup.rs", counts: &[("sovereign-app", 4)] },
    // named by the agreement: comms.toml.
    Named { name: "comms_config_path", defined_in: "sovereign-shell/src/comms.rs", counts: &[("sovereign-shell", 1)] },
    // named by the agreement: sovereign_dir()/crypto.
    Named { name: "crypto_dir", defined_in: "sovereign-app/src/setup.rs", counts: &[("sovereign-app", 20)] },
    // named by the agreement: sovereign_dir()/crypto.
    Named { name: "crypto_dir", defined_in: "sovereign-shell/src/crypto.rs", counts: &[("sovereign-shell", 11)] },
    // wrapper found: sovereign_dir()/signal, as a String.
    Named { name: "default_signal_store_path", defined_in: "sovereign-comms/src/config.rs", counts: &[] },
    // the OS user's Downloads folder, outside the Sovereign directory, where an approved export of a
    // quarantined log is written; one folder for both personas, so that no name or place says there
    // are two (decided 2026-11-18 and 2026-11-19; added by C-LOG's host lot's test author).
    Named { name: "downloads_folder", defined_in: "sovereign-shell/src/log_window.rs", counts: &[("sovereign-shell", 1)] },
    // root named by the agreement: the parent of the default Sovereign directory. In the shell: the
    // Downloads folder above (C-LOG's host lot).
    Named { name: "home_dir", defined_in: "sovereign-core/src/lib.rs", counts: &[("sovereign-app", 1), ("sovereign-core", 1), ("sovereign-shell", 1)] },
    // the app's login: the data folder and the persona's profile folder, handed to the login error's
    // scrub so that no path of either is shown; it opens nothing (ch. 07, the login errors; added by
    // the login-errors lot's test author, 2026-11-19).
    Named { name: "login_data_dirs", defined_in: "sovereign-app/src/tauri_commands/auth.rs", counts: &[("sovereign-app", 1)] },
    // wrapper found: the device-name file, shared by both personas.
    Named { name: "path", defined_in: "sovereign-shell/src/device_name.rs", counts: &[("sovereign-shell", 1)] },
    // wrapper found.
    Named { name: "recovery_dir", defined_in: "sovereign-app/src/tauri_commands/recovery.rs", counts: &[("sovereign-app", 5)] },
    // wrapper found: onboarding_done.
    Named { name: "setup_marker_path", defined_in: "sovereign-shell/src/crypto.rs", counts: &[("sovereign-shell", 2)] },
    // wrapper found: onboarding_started.
    Named { name: "setup_started_path", defined_in: "sovereign-shell/src/crypto.rs", counts: &[("sovereign-shell", 3)] },
    // root: reads SOVEREIGN_DATA_DIR, else ~/.sovereign (named by the agreement). Raised by the
    // login-errors lot (app 11 -> 15, shell 19 -> 21): each new call hands the data folder to the
    // login error's scrub, so that its paths are cleared from the message shown; none opens, reads or
    // writes a file (ch. 07, the login errors; decided 2026-11-19).
    Named { name: "sovereign_dir", defined_in: "sovereign-core/src/lib.rs", counts: &[("sovereign-app", 15), ("sovereign-comms", 1), ("sovereign-shell", 21)] },
    // BASELINE-END
];

// ============================================================== the scan

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap().to_path_buf()
}

/// `crates/<crate>/src/**/*.rs`, as (crate, path relative to `crates/`, text).
fn sources() -> Vec<(String, String, String)> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|e| e == "rs") {
                out.push(p);
            }
        }
    }
    let crates = root().join("crates");
    let mut out = Vec::new();
    let mut names: Vec<_> = std::fs::read_dir(&crates).unwrap().flatten().map(|e| e.path()).collect();
    names.sort();
    for c in names {
        let src = c.join("src");
        if !src.is_dir() {
            continue;
        }
        let krate = c.file_name().unwrap().to_string_lossy().into_owned();
        let mut files = Vec::new();
        walk(&src, &mut files);
        for f in files {
            let rel = f.strip_prefix(&crates).unwrap().to_string_lossy().replace('\\', "/");
            let text = std::fs::read_to_string(&f).unwrap_or_default();
            out.push((krate.clone(), rel, text));
        }
    }
    out
}

/// Removes comments; with `blank_strings`, also the contents of string and
/// char literals (the quotes stay). Line structure is kept.
fn strip(code: &str, blank_strings: bool) -> String {
    let b = code.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    let push_blank = |out: &mut Vec<u8>, c: u8| out.push(if c == b'\n' { b'\n' } else { b' ' });
    while i < b.len() {
        let c = b[i];
        // Line comment.
        if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        // Block comment (nested).
        if c == b'/' && b.get(i + 1) == Some(&b'*') {
            let mut depth = 0;
            while i < b.len() {
                if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
                    depth += 1;
                    i += 2;
                } else if b[i] == b'*' && b.get(i + 1) == Some(&b'/') {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    if b[i] == b'\n' {
                        out.push(b'\n');
                    }
                    i += 1;
                }
            }
            continue;
        }
        // Raw string: r"..", r#".."#, br#".."#.
        let raw_at = if c == b'r' && matches!(b.get(i + 1), Some(b'"') | Some(b'#')) {
            Some(i + 1)
        } else if c == b'b' && b.get(i + 1) == Some(&b'r') && matches!(b.get(i + 2), Some(b'"') | Some(b'#')) {
            Some(i + 2)
        } else {
            None
        };
        if let Some(mut j) = raw_at {
            let prev_ident = i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_');
            if !prev_ident {
                let mut hashes = 0;
                while b.get(j) == Some(&b'#') {
                    hashes += 1;
                    j += 1;
                }
                if b.get(j) == Some(&b'"') {
                    out.extend_from_slice(&b[i..=j]);
                    j += 1;
                    loop {
                        if j >= b.len() {
                            break;
                        }
                        if b[j] == b'"' && (0..hashes).all(|k| b.get(j + 1 + k) == Some(&b'#')) {
                            out.push(b'"');
                            out.extend(std::iter::repeat(b'#').take(hashes));
                            j += 1 + hashes;
                            break;
                        }
                        if blank_strings {
                            push_blank(&mut out, b[j]);
                        } else {
                            out.push(b[j]);
                        }
                        j += 1;
                    }
                    i = j;
                    continue;
                }
            }
        }
        // String literal.
        if c == b'"' {
            out.push(b'"');
            i += 1;
            while i < b.len() && b[i] != b'"' {
                if b[i] == b'\\' && i + 1 < b.len() {
                    if blank_strings {
                        push_blank(&mut out, b[i]);
                        push_blank(&mut out, b[i + 1]);
                    } else {
                        out.push(b[i]);
                        out.push(b[i + 1]);
                    }
                    i += 2;
                    continue;
                }
                if blank_strings {
                    push_blank(&mut out, b[i]);
                } else {
                    out.push(b[i]);
                }
                i += 1;
            }
            out.push(b'"');
            i += 1;
            continue;
        }
        // Char literal ('x', '\x', '\u{..}'); a lifetime ('a) is left as is.
        if c == b'\'' {
            if b.get(i + 1) == Some(&b'\\') {
                let mut j = i + 2;
                while j < b.len() && b[j] != b'\'' && j - i < 12 {
                    j += 1;
                }
                if b.get(j) == Some(&b'\'') {
                    out.extend_from_slice(b"' '");
                    i = j + 1;
                    continue;
                }
            } else if b.get(i + 2) == Some(&b'\'') {
                out.extend_from_slice(b"' '");
                i += 3;
                continue;
            } else if i + 1 < b.len() && b[i + 1] >= 0x80 {
                // A non-ASCII char literal: skip to the closing quote.
                let mut j = i + 1;
                while j < b.len() && b[j] != b'\'' && j - i < 6 {
                    j += 1;
                }
                if b.get(j) == Some(&b'\'') {
                    out.extend_from_slice(b"' '");
                    i = j + 1;
                    continue;
                }
            }
        }
        out.push(c);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The index of the brace that closes the one at `open` (strings and
/// comments already removed).
fn matching(b: &[u8], open: usize, o: u8, c: u8) -> usize {
    let mut depth = 0usize;
    let mut i = open;
    while i < b.len() {
        if b[i] == o {
            depth += 1;
        } else if b[i] == c {
            depth -= 1;
            if depth == 0 {
                return i;
            }
        }
        i += 1;
    }
    b.len().saturating_sub(1)
}

fn is_test_attr(attr: &str) -> bool {
    let a: String = attr.chars().filter(|c| !c.is_whitespace()).collect();
    a.starts_with("#[cfg(test)]") || a.starts_with("#[cfg(all(test,") || a.starts_with("#[cfg(all(test)")
}

/// The text with every item that follows a test-only `cfg` attribute removed
/// (to its matching `}` or its `;`), plus the `#[path]` targets of the test
/// modules so removed. Expects comments and strings already handled.
fn remove_test_items(code: &str) -> (String, Vec<String>) {
    let b = code.as_bytes();
    let mut keep = vec![true; b.len()];
    let mut paths = Vec::new();
    let mut i = 0;
    while let Some(off) = code[i..].find("#[cfg(") {
        let at = i + off;
        let close = matching(b, at + 1, b'[', b']');
        let attr = &code[at..=close];
        if !is_test_attr(attr) {
            i = close + 1;
            continue;
        }
        // The item: up to its first `;` or matching `{…}` at depth 0, after
        // any further attributes.
        let mut j = close + 1;
        let mut end = b.len() - 1;
        while j < b.len() {
            match b[j] {
                b'#' if b.get(j + 1) == Some(&b'[') => {
                    let c = matching(b, j + 1, b'[', b']');
                    let a = &code[j..=c];
                    if let Some(p) = a.find("path") {
                        let rest = &a[p..];
                        if let (Some(q1), Some(q2)) = (rest.find('"'), rest.rfind('"')) {
                            if q2 > q1 {
                                paths.push(rest[q1 + 1..q2].to_string());
                            }
                        }
                    }
                    j = c + 1;
                }
                b';' => {
                    end = j;
                    break;
                }
                b'{' => {
                    end = matching(b, j, b'{', b'}');
                    break;
                }
                _ => j += 1,
            }
        }
        for k in at..=end.min(b.len() - 1) {
            if b[k] != b'\n' {
                keep[k] = false;
            }
        }
        i = end + 1;
        if i >= b.len() {
            break;
        }
    }
    let out: Vec<u8> = b.iter().zip(&keep).map(|(c, k)| if *k { *c } else { b' ' }).collect();
    (String::from_utf8_lossy(&out).into_owned(), paths)
}

fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// The calls to `name` in `code` (strings blanked, comments removed).
fn calls(code: &str, name: &str) -> usize {
    let b = code.as_bytes();
    let mut n = 0;
    let mut from = 0;
    while let Some(off) = code[from..].find(name) {
        let at = from + off;
        from = at + name.len();
        if at > 0 && (is_ident(b[at - 1]) || b[at - 1] == b'.') {
            continue;
        }
        let after = at + name.len();
        if after < b.len() && is_ident(b[after]) {
            continue;
        }
        let mut k = after;
        while k < b.len() && (b[k] == b' ' || b[k] == b'\t' || b[k] == b'\n' || b[k] == b'\r') {
            k += 1;
        }
        // A turbofish is not used by these functions; `(` must follow.
        if b.get(k) != Some(&b'(') {
            continue;
        }
        // Not a definition: `fn name(`.
        let before = code[..at].trim_end();
        if before.ends_with("fn") && before[..before.len() - 2].ends_with(|c: char| !c.is_alphanumeric() && c != '_') {
            continue;
        }
        n += 1;
    }
    n
}

/// A function definition in non-test code.
#[derive(Clone, Debug)]
struct FnDef {
    krate: String,
    file: String,
    name: String,
    params: String,
    ret: String,
    /// Strings blanked.
    body: String,
    /// Strings kept (for the root markers).
    body_with_strings: String,
}

fn fn_defs(krate: &str, file: &str, blanked: &str, kept: &str) -> Vec<FnDef> {
    let b = blanked.as_bytes();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(off) = blanked[from..].find("fn ") {
        let at = from + off;
        from = at + 3;
        if at > 0 && is_ident(b[at - 1]) {
            continue;
        }
        let mut k = at + 3;
        while k < b.len() && b[k] == b' ' {
            k += 1;
        }
        let s = k;
        while k < b.len() && is_ident(b[k]) {
            k += 1;
        }
        if k == s {
            continue;
        }
        let name = blanked[s..k].to_string();
        // Generics.
        while k < b.len() && b[k] == b' ' {
            k += 1;
        }
        if b.get(k) == Some(&b'<') {
            k = matching(b, k, b'<', b'>') + 1;
        }
        while k < b.len() && b[k] != b'(' {
            k += 1;
        }
        if k >= b.len() {
            break;
        }
        let pclose = matching(b, k, b'(', b')');
        let params = blanked[k + 1..pclose].to_string();
        // Return type up to `{`, `;` or `where`.
        let mut r = pclose + 1;
        while r < b.len() && b[r] != b'{' && b[r] != b';' {
            r += 1;
        }
        if r >= b.len() || b[r] == b';' {
            continue;
        }
        let sig_tail = &blanked[pclose + 1..r];
        let ret = sig_tail.split("where").next().unwrap_or("").trim().trim_start_matches("->").trim().to_string();
        let bclose = matching(b, r, b'{', b'}');
        out.push(FnDef {
            krate: krate.to_string(),
            file: file.to_string(),
            name,
            params,
            ret,
            body: blanked[r..=bclose].to_string(),
            body_with_strings: kept.get(r..=bclose).unwrap_or("").to_string(),
        });
    }
    out
}

struct Scan {
    /// (crate, file, code with strings blanked) for non-test code.
    code: Vec<(String, String, String)>,
    defs: Vec<FnDef>,
}

fn scan() -> Scan {
    let all = sources();
    // First pass: the files test modules point at.
    let mut test_files: BTreeSet<String> = BTreeSet::new();
    for (_, rel, text) in &all {
        let kept = strip(text, false);
        let (_, paths) = remove_test_items(&kept);
        let dir = Path::new(rel).parent().unwrap_or(Path::new(""));
        for p in paths {
            // A `#[path]` in `foo.rs` (not mod.rs/lib.rs/main.rs) of a
            // non-inline module resolves next to the file; good enough here:
            // test files are found by name too.
            let joined = dir.join(&p).to_string_lossy().replace('\\', "/");
            test_files.insert(normalize(&joined));
        }
    }
    let mut code = Vec::new();
    let mut defs = Vec::new();
    for (krate, rel, text) in &all {
        let fname = rel.rsplit('/').next().unwrap_or("");
        if fname.ends_with("_tests.rs") || fname == "tests.rs" || test_files.contains(rel) {
            continue;
        }
        let kept = strip(text, false);
        let (kept, _) = remove_test_items(&kept);
        let blanked = strip(&kept, true);
        defs.extend(fn_defs(krate, rel, &blanked, &kept));
        code.push((krate.clone(), rel.clone(), blanked));
    }
    Scan { code, defs }
}

fn normalize(p: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "." | "" => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

fn names_persona(params: &str) -> bool {
    params.to_lowercase().contains("persona")
}

/// A return type that is a path: one naming `PathBuf`, or a string-like one
/// (`String`, `OsString`) on a function named `*_path`, `*_dir` or `*_root`
/// (`default_signal_store_path() -> String` in sovereign-comms).
fn returns_a_path(d: &FnDef) -> bool {
    d.ret.contains("PathBuf")
        || ((d.name.ends_with("_path") || d.name.ends_with("_dir") || d.name.ends_with("_root"))
            && (d.ret.contains("String") || d.ret.contains("Path")))
}

/// Every function of that kind: (name, file) → its definition.
fn functions_of_that_kind(s: &Scan) -> BTreeMap<(String, String), FnDef> {
    let mut found: BTreeMap<(String, String), FnDef> = BTreeMap::new();
    for d in &s.defs {
        let root_marker = d.body_with_strings.contains("\"SOVEREIGN_DATA_DIR\"")
            || d.body_with_strings.contains("\".sovereign\"");
        if (root_marker && returns_a_path(d) && !names_persona(&d.params))
            || (NAMED_ROOTS.contains(&d.name.as_str()) && d.file == "sovereign-core/src/lib.rs")
        {
            found.insert((d.name.clone(), d.file.clone()), d.clone());
        }
    }
    loop {
        let names: BTreeSet<String> = found.keys().map(|(n, _)| n.clone()).collect();
        let mut added = false;
        for d in &s.defs {
            let key = (d.name.clone(), d.file.clone());
            if found.contains_key(&key) || !returns_a_path(d) || names_persona(&d.params) {
                continue;
            }
            if names.iter().any(|n| calls(&d.body, n) > 0) {
                found.insert(key, d.clone());
                added = true;
            }
        }
        if !added {
            return found;
        }
    }
}

/// The crate of a file relative to `crates/`.
fn crate_of(file: &str) -> &str {
    file.split('/').next().unwrap_or("")
}

/// Whether crate `user` can call a function defined in crate `def`: the same
/// crate, or a crate whose `Cargo.toml` names `def` as a dependency.
fn can_call(user: &str, def: &str) -> bool {
    if user == def {
        return true;
    }
    let toml = std::fs::read_to_string(root().join("crates").join(user).join("Cargo.toml")).unwrap_or_default();
    toml.lines().any(|l| {
        let l = l.trim_start();
        l.starts_with(&format!("{def} ")) || l.starts_with(&format!("{def}=")) || l.starts_with(&format!("{def}."))
    })
}

/// Calls per (name, defining file) and crate, outside test code, definitions
/// excluded: in the defining crate and in the crates that depend on it.
fn counts(s: &Scan, fns: &BTreeSet<(String, String)>) -> BTreeMap<(String, String), BTreeMap<String, usize>> {
    let mut out: BTreeMap<(String, String), BTreeMap<String, usize>> = BTreeMap::new();
    for (name, file) in fns {
        let per = out.entry((name.clone(), file.clone())).or_default();
        for (krate, _, code) in &s.code {
            if !can_call(krate, crate_of(file)) {
                continue;
            }
            let n = calls(code, name);
            if n > 0 {
                *per.entry(krate.clone()).or_default() += n;
            }
        }
    }
    out
}

#[test]
fn ar8_no_new_shared_path() {
    let s = scan();
    let found = functions_of_that_kind(&s);
    let fns: BTreeSet<(String, String)> = found
        .keys()
        .cloned()
        .chain(NAMED.iter().map(|n| (n.name.to_string(), n.defined_in.to_string())))
        .collect();
    let now = counts(&s, &fns);

    if std::env::var_os("C_AREA_AR8_PRINT").is_some() {
        println!("    // BASELINE-START");
        for ((name, file), d) in &found {
            let per = now.get(&(name.clone(), file.clone())).cloned().unwrap_or_default();
            let list = per.iter().map(|(k, v)| format!("(\"{k}\", {v})")).collect::<Vec<_>>().join(", ");
            println!(
                "    Named {{ name: \"{name}\", defined_in: \"{file}\", counts: &[{list}] }}, // -> {}",
                d.ret
            );
        }
        println!("    // BASELINE-END");
    }

    let mut problems = Vec::new();
    // A named function must still be there.
    for n in NAMED {
        if !found.contains_key(&(n.name.to_string(), n.defined_in.to_string())) {
            problems.push(format!(
                "`{}` ({}) is named here but no longer found as a function of that kind: the check \
                 would go blind for it. If it was removed or renamed, update NAMED.",
                n.name, n.defined_in
            ));
        }
    }
    // A function found must be named.
    for ((name, file), d) in &found {
        if !NAMED.iter().any(|n| n.name == name && n.defined_in == file) {
            problems.push(format!(
                "new function of that kind, not named: `fn {name}({}) -> {}` in crates/{file}. It returns a \
                 path under the Sovereign directory without taking a persona (C-AREA AR8). Give it a persona, \
                 or name it in NAMED with its counts.",
                d.params.trim(),
                d.ret
            ));
        }
    }
    // No count grows.
    let mut lowered = Vec::new();
    for (name, file) in &fns {
        let base: BTreeMap<&str, usize> = NAMED
            .iter()
            .filter(|n| n.name == name && n.defined_in == file)
            .flat_map(|n| n.counts.iter().copied())
            .collect();
        let cur = now.get(&(name.clone(), file.clone())).cloned().unwrap_or_default();
        let crates: BTreeSet<&str> = base.keys().copied().chain(cur.keys().map(String::as_str)).collect();
        for k in crates {
            let (b, c) = (base.get(k).copied().unwrap_or(0), cur.get(k).copied().unwrap_or(0));
            if c > b {
                problems.push(format!(
                    "`{name}()` (crates/{file}) is called {c} times in {k}, {b} recorded: the count grew by {}",
                    c - b
                ));
            } else if c < b {
                lowered.push(format!("`{name}()` (crates/{file}) in {k}: {c} now, {b} recorded"));
            }
        }
    }
    if !lowered.is_empty() {
        println!("C-AREA AR8: counts that went down (lower them in NAMED to tighten the ratchet):\n  {}", lowered.join("\n  "));
    }
    assert!(problems.is_empty(), "C-AREA AR8 — new shared paths:\n  {}", problems.join("\n  "));
}

/// The scanner finds what it looks for, on a sample with every case it must
/// handle: comments, strings, a test module, a test-only fn, a wrapper, a
/// method of the same name, a definition.
#[test]
fn ar8_scanner_finds_what_it_looks_for() {
    let src = r##"
// sovereign_dir() in a comment
/* and sovereign_dir() in a block */
fn root() -> std::path::PathBuf { std::env::var("SOVEREIGN_DATA_DIR").map(Into::into).unwrap() }
fn wrap() -> PathBuf { root().join("crypto") }
fn wrap2() -> anyhow::Result<PathBuf> { Ok(wrap().join("x")) }
fn per(persona: PersonaKind) -> PathBuf { wrap().join("y") }
fn user() { let _ = wrap(); let s = "wrap()"; let _ = x.wrap(); let r = r#"wrap()"#; }
#[cfg(test)]
fn only_test() { wrap(); }
#[cfg(test)]
mod t { fn a() { wrap(); wrap(); } }
#[cfg(test)]
#[path = "x_tests.rs"]
mod x_tests;
"##;
    let kept = strip(src, false);
    let (kept, paths) = remove_test_items(&kept);
    assert_eq!(paths, vec!["x_tests.rs".to_string()]);
    let blanked = strip(&kept, true);
    assert_eq!(calls(&blanked, "sovereign_dir"), 0, "comments are not calls");
    // root() in wrap; wrap() in wrap2, per, user (string, method and test calls excluded).
    assert_eq!(calls(&blanked, "root"), 1, "{blanked}");
    assert_eq!(calls(&blanked, "wrap"), 3, "{blanked}");
    let s = Scan {
        code: vec![("c".into(), "c/src/lib.rs".into(), blanked.clone())],
        defs: fn_defs("c", "c/src/lib.rs", &blanked, &kept),
    };
    let found: BTreeSet<String> = functions_of_that_kind(&s).keys().map(|(n, _)| n.clone()).collect();
    let want: BTreeSet<String> = ["root", "wrap", "wrap2"].iter().map(|s| s.to_string()).collect();
    assert_eq!(found, want, "roots and wrappers, not the persona-taking fn nor a caller returning no path");
}
