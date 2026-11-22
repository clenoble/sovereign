//! C-SWEEP **SW3 — the gate check** (spec ch. 09, v2.0.111): « a source scan,
//! as AR8's, fails on any call to the model outside the named ones (the masking
//! readers, the classifier of her own message, the detector). It reads names,
//! not data flow: its limits are written beside it. »
//!
//! **What it counts.** Every method call `.generate(` (the name, then `(`
//! after optional whitespace) in `crates/<crate>/src/**/*.rs`, outside test
//! code, with its enclosing function (the innermost `fn` whose body holds the
//! call; a closure counts for the `fn` that holds it). Test code, comments and
//! string literals are left out exactly as AR8 leaves them out
//! (`c_area_ar8_shared_paths.rs`). Every (file, function) holding a call must be
//! named in [`NAMED`] with its count; a call in a function not named, or a count
//! that grows, fails; a named entry whose calls are gone fails too, so the list
//! never goes blind without saying so (lowering a count is the ratchet: the
//! check says which).
//!
//! **The named ones, and why** (the contract's three kinds, plus the backend
//! itself and the callers the contract names as not covered):
//! - the masking readers: the chat's agent loop (its context and tools are
//!   read masked), the summary, the incoming-change analysis;
//! - the classifier of her own message (the router, and its escalation);
//! - the detector (NER, which ingest and the sweep do not run, SW8);
//! - the model backend's own `generate`, which every caller goes through;
//! - **not covered, said in the contract and the notes:** the reliability
//!   check of a browser page (its two steps), consolidation and
//!   `thread_summary` with the bridge that serves skills (the orchestrator's
//!   `generate` and the app's `LlmBridge`), which run in no shipped build.
//!
//! **Its limits** (it reads names, not data flow):
//! - it cannot see where a prompt's text came from: a named reader that puts
//!   unmasked stored text into its prompt passes. That is what the IBAN tests
//!   hold (`sovereign-ai`, `orchestrator_c_sweep_tests.rs`), on the paths they
//!   drive, and only there;
//! - any method named `generate` counts, whatever its receiver, and a model
//!   reached under another name (a wrapper, a function value, the test seam's
//!   closure) is not seen;
//! - the enclosing function is found by its name: a new function given the name
//!   of a named one in the same file passes;
//! - code compiled only under a feature is scanned like any other; test code is
//!   recognised by the same rules as AR8 (file names, `#[cfg(test)]` items and
//!   the `#[path]` they name).
//!
//! **Where it lives:** a test of the workspace, as AR8 is. It reads the
//! sources and runs nothing. `C_SWEEP_SW3_PRINT=1` prints what it counted.
//!
//! Written before the code by the test author of lot LS(a); whoever implements
//! does not raise a count or add a name to pass: a new call to the model is a
//! decision, reported.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// A function allowed to call the model, with today's count and its reason.
struct Named {
    /// The file, relative to `crates/`.
    file: &'static str,
    /// The enclosing function.
    function: &'static str,
    count: usize,
    why: &'static str,
}

/// Counted on `main` at 557a9638 (2026-11-17) with `C_SWEEP_SW3_PRINT=1`.
const NAMED: &[Named] = &[
    Named { file: "sovereign-ai/src/consolidation.rs", function: "score_pairs", count: 1, why: "not covered: consolidation runs in no shipped build" },
    Named { file: "sovereign-ai/src/intent/classifier.rs", function: "classify", count: 1, why: "the classifier of her own message" },
    Named { file: "sovereign-ai/src/intent/classifier.rs", function: "escalate_to_reasoning", count: 1, why: "the classifier of her own message (its escalation)" },
    Named { file: "sovereign-ai/src/llm/async_backend.rs", function: "generate", count: 1, why: "the model backend itself" },
    Named { file: "sovereign-ai/src/orchestrator.rs", function: "generate", count: 1, why: "not covered: the bridge that serves skills (thread_summary), in no shipped build" },
    Named { file: "sovereign-ai/src/orchestrator.rs", function: "generate_summary", count: 1, why: "a masking reader: the summary" },
    Named { file: "sovereign-ai/src/orchestrator.rs", function: "run_chat_agent_loop", count: 1, why: "a masking reader: the chat's context and tools" },
    Named { file: "sovereign-ai/src/peer_audit.rs", function: "llm_assess", count: 1, why: "a masking reader: the incoming-change analysis" },
    Named { file: "sovereign-ai/src/pii/ner.rs", function: "ner_stage", count: 1, why: "the detector" },
    Named { file: "sovereign-ai/src/reliability.rs", function: "classify_content", count: 1, why: "not covered: the browser page sent to the reliability check" },
    Named { file: "sovereign-ai/src/reliability.rs", function: "score_rubric", count: 1, why: "not covered: the browser page sent to the reliability check" },
    Named { file: "sovereign-app/src/llm_bridge.rs", function: "generate", count: 1, why: "not covered: the bridge that serves skills, in no shipped build" },
    Named { file: "sovereign-skills/src/skills/thread_summary.rs", function: "execute", count: 1, why: "not covered: thread_summary, in no shipped build" },
];

// ============================================================== the scan

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap().to_path_buf()
}

/// `crates/<crate>/src/**/*.rs`, as (path relative to `crates/`, text).
fn sources() -> Vec<(String, String)> {
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
        let mut files = Vec::new();
        walk(&src, &mut files);
        for f in files {
            let rel = f.strip_prefix(&crates).unwrap().to_string_lossy().replace('\\', "/");
            out.push((rel, std::fs::read_to_string(&f).unwrap_or_default()));
        }
    }
    out
}

/// Removes comments; with `blank_strings`, also the contents of string and
/// char literals (the quotes stay). Line structure is kept. (AR8's.)
fn strip(code: &str, blank_strings: bool) -> String {
    let b = code.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    let push_blank = |out: &mut Vec<u8>, c: u8| out.push(if c == b'\n' { b'\n' } else { b' ' });
    while i < b.len() {
        let c = b[i];
        if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
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

/// AR8's: every item after a test-only `cfg` blanked, and the `#[path]`
/// targets of the test modules so removed.
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

fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// The functions of `code` (strings blanked): (name, body start, body end).
fn fn_bodies(code: &str) -> Vec<(String, usize, usize)> {
    let b = code.as_bytes();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(off) = code[from..].find("fn ") {
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
        let name = code[s..k].to_string();
        while k < b.len() && b[k] != b'(' && b[k] != b'{' && b[k] != b';' {
            if b[k] == b'<' {
                k = matching(b, k, b'<', b'>');
            }
            k += 1;
        }
        if k >= b.len() || b[k] != b'(' {
            continue;
        }
        let pclose = matching(b, k, b'(', b')');
        let mut r = pclose + 1;
        while r < b.len() && b[r] != b'{' && b[r] != b';' {
            r += 1;
        }
        if r >= b.len() || b[r] == b';' {
            continue;
        }
        out.push((name, r, matching(b, r, b'{', b'}')));
    }
    out
}

/// The offsets of the method calls `.generate(` in `code`.
fn generate_calls(code: &str) -> Vec<usize> {
    const NAME: &str = ".generate";
    let b = code.as_bytes();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(off) = code[from..].find(NAME) {
        let at = from + off;
        from = at + NAME.len();
        let after = at + NAME.len();
        if after < b.len() && is_ident(b[after]) {
            continue;
        }
        let mut k = after;
        while k < b.len() && b[k].is_ascii_whitespace() {
            k += 1;
        }
        if b.get(k) == Some(&b'(') {
            out.push(at);
        }
    }
    out
}

/// (file, enclosing function) → calls, over the workspace's non-test code.
fn scan_files(files: &[(String, String)]) -> BTreeMap<(String, String), usize> {
    let mut test_files: BTreeSet<String> = BTreeSet::new();
    for (rel, text) in files {
        let (_, paths) = remove_test_items(&strip(text, false));
        let dir = Path::new(rel).parent().unwrap_or(Path::new(""));
        for p in paths {
            test_files.insert(normalize(&dir.join(&p).to_string_lossy().replace('\\', "/")));
        }
    }
    let mut out = BTreeMap::new();
    for (rel, text) in files {
        let fname = rel.rsplit('/').next().unwrap_or("");
        if fname.ends_with("_tests.rs") || fname == "tests.rs" || test_files.contains(rel) {
            continue;
        }
        let (kept, _) = remove_test_items(&strip(text, false));
        let code = strip(&kept, true);
        let bodies = fn_bodies(&code);
        for at in generate_calls(&code) {
            let enclosing = bodies
                .iter()
                .filter(|(_, s, e)| *s <= at && at <= *e)
                .min_by_key(|(_, s, e)| e - s)
                .map(|(n, _, _)| n.clone())
                .unwrap_or_else(|| "<outside any fn>".into());
            *out.entry((rel.clone(), enclosing)).or_insert(0) += 1;
        }
    }
    out
}

#[test]
fn sw3_no_call_to_the_model_outside_the_named_ones() {
    let found = scan_files(&sources());
    if std::env::var("C_SWEEP_SW3_PRINT").is_ok() {
        for ((file, function), n) in &found {
            println!("{file}\t{function}\t{n}");
        }
    }
    let named: BTreeMap<(String, String), usize> =
        NAMED.iter().map(|n| ((n.file.to_string(), n.function.to_string()), n.count)).collect();
    let mut failures = Vec::new();
    for ((file, function), n) in &found {
        match named.get(&(file.clone(), function.clone())) {
            None => failures.push(format!(
                "{file}: {n} call(s) to the model in `{function}`, which is not named: content reaching the model there is \
                 not masked where it is read (C-SWEEP SW3). A new call to the model is a decision, reported."
            )),
            Some(want) if n > want => {
                failures.push(format!("{file}: `{function}` now calls the model {n} time(s), {want} named"))
            }
            Some(want) if n < want => {
                failures.push(format!("{file}: `{function}` calls the model {n} time(s), {want} named: lower the count"))
            }
            _ => {}
        }
    }
    for n in NAMED {
        if !found.contains_key(&(n.file.to_string(), n.function.to_string())) {
            failures.push(format!(
                "{}: `{}` is named ({}) but calls the model no more: remove it from the list",
                n.file, n.function, n.why
            ));
        }
    }
    assert!(failures.is_empty(), "C-SWEEP SW3, the gate check:\n{}", failures.join("\n"));
}

/// The scanner finds what it looks for: a call in a function, a call in a
/// closure (counted for its function), not a definition, not a comment, not a
/// string, not test code, not another method whose name starts the same.
#[test]
fn sw3_the_scanner_finds_what_it_looks_for() {
    let src = r##"
fn reader(m: &M) { let _ = m.generate(&p, 3); }
fn holder(m: &M) { let f = || m
    .generate (&p, 3); f(); }
fn generate(&self) {}
fn noise(m: &M) { let s = "m.generate(x)"; m.generated(1); m.generate_summary(1); }
// fn commented(m: &M) { m.generate(&p, 1); }
#[cfg(test)]
mod tests { fn t(m: &M) { m.generate(&p, 1); } }
"##;
    let found = scan_files(&[("demo/src/lib.rs".to_string(), src.to_string())]);
    let want: BTreeMap<(String, String), usize> = [
        (("demo/src/lib.rs".to_string(), "holder".to_string()), 1),
        (("demo/src/lib.rs".to_string(), "reader".to_string()), 1),
    ]
    .into_iter()
    .collect();
    assert_eq!(found, want);
}
