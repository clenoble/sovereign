//! Source scans over the workspace's production code (C-DOC adoption step 5:
//! S5-1 "one way to delete", S5-3 "every host purges").
//!
//! A scan reads `crates/*/src/**/*.rs` as text. To look only at calls in
//! production code it first **blanks** comments and the contents of string and
//! character literals (newlines kept, so line numbers stay true), then blanks
//! every item under `#[cfg(test)]` (a `mod tests { … }`, a test-only helper
//! `fn`, a test-only `use`). A file declared as a test-only module
//! (`#[cfg(test)] mod name;`) is skipped whole.
//!
//! A call is `.name(` or `::name(` (whitespace allowed before the parenthesis).
//! A definition (`fn name(`), a string such as `"delete_document"` (an action
//! name) and a mention in a comment are not calls.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// The workspace's `crates/` directory.
pub fn crates_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("crates/").to_path_buf()
}

/// Every `.rs` file under `dir`, sorted.
pub fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = vec![];
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// Comments and the contents of string / char literals replaced by spaces,
/// newlines kept. Handles `//`, nested `/* */`, `"…"` with escapes, raw
/// strings `r"…"` / `r#"…"#` (any number of `#`), byte strings, and char
/// literals (`'x'`, `'\n'`, `'{'`) without mistaking lifetimes (`'a`) for them.
pub fn blank(src: &str) -> String {
    let b: Vec<char> = src.chars().collect();
    let mut out: Vec<char> = b.clone();
    let n = b.len();
    let mut i = 0;
    let sp = |out: &mut Vec<char>, k: usize| {
        if out[k] != '\n' {
            out[k] = ' ';
        }
    };
    while i < n {
        let c = b[i];
        // line comment
        if c == '/' && i + 1 < n && b[i + 1] == '/' {
            while i < n && b[i] != '\n' {
                sp(&mut out, i);
                i += 1;
            }
            continue;
        }
        // block comment (nested)
        if c == '/' && i + 1 < n && b[i + 1] == '*' {
            let mut depth = 0;
            while i < n {
                if b[i] == '/' && i + 1 < n && b[i + 1] == '*' {
                    depth += 1;
                    sp(&mut out, i);
                    sp(&mut out, i + 1);
                    i += 2;
                    continue;
                }
                if b[i] == '*' && i + 1 < n && b[i + 1] == '/' {
                    depth -= 1;
                    sp(&mut out, i);
                    sp(&mut out, i + 1);
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                    continue;
                }
                sp(&mut out, i);
                i += 1;
            }
            continue;
        }
        let prev_ident = i > 0 && (b[i - 1].is_alphanumeric() || b[i - 1] == '_');
        // raw string: r"…", r#"…"#, br"…"
        if (c == 'r' || (c == 'b' && i + 1 < n && b[i + 1] == 'r')) && !prev_ident {
            let mut j = if c == 'b' { i + 2 } else { i + 1 };
            let mut hashes = 0;
            while j < n && b[j] == '#' {
                hashes += 1;
                j += 1;
            }
            if j < n && b[j] == '"' {
                j += 1;
                loop {
                    if j >= n {
                        break;
                    }
                    if b[j] == '"' && (0..hashes).all(|h| j + 1 + h < n && b[j + 1 + h] == '#') {
                        j += 1 + hashes;
                        break;
                    }
                    sp(&mut out, j);
                    j += 1;
                }
                i = j;
                continue;
            }
        }
        // string (or byte string)
        if c == '"' {
            i += 1;
            while i < n && b[i] != '"' {
                if b[i] == '\\' && i + 1 < n {
                    sp(&mut out, i);
                    sp(&mut out, i + 1);
                    i += 2;
                    continue;
                }
                sp(&mut out, i);
                i += 1;
            }
            i += 1;
            continue;
        }
        // char literal (not a lifetime)
        if c == '\'' {
            if i + 2 < n && b[i + 1] == '\\' {
                let mut j = i + 2;
                while j < n && b[j] != '\'' && j < i + 12 {
                    j += 1;
                }
                if j < n && b[j] == '\'' {
                    for k in i + 1..j {
                        sp(&mut out, k);
                    }
                    i = j + 1;
                    continue;
                }
            } else if i + 2 < n && b[i + 2] == '\'' {
                sp(&mut out, i + 1);
                i += 3;
                continue;
            }
        }
        i += 1;
    }
    out.into_iter().collect()
}

/// `blank`ed text with every `#[cfg(test)]` item blanked too, and the names of
/// the test-only modules this file declares without a body (`mod name;`).
pub fn production(src: &str) -> (String, Vec<String>) {
    let text = blank(src);
    let b: Vec<char> = text.chars().collect();
    let mut out = b.clone();
    let mut test_mods = vec![];
    let marker: Vec<char> = "#[cfg(test)]".chars().collect();
    let mut i = 0;
    while i + marker.len() <= b.len() {
        if b[i..i + marker.len()] != marker[..] {
            i += 1;
            continue;
        }
        // The item: up to its first `;` (no body) or its body's closing brace.
        let mut j = i + marker.len();
        // a declaration `mod name;`
        let rest: String = b[j..b.len().min(j + 200)].iter().collect();
        let trimmed = rest.trim_start();
        if let Some(after) = trimmed.strip_prefix("mod ") {
            let name: String = after.trim_start().chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            let tail = after.trim_start()[name.len()..].trim_start();
            if tail.starts_with(';') {
                test_mods.push(name);
            }
        }
        let mut depth = 0usize;
        while j < b.len() {
            match b[j] {
                ';' if depth == 0 => break,
                '{' => depth += 1,
                '}' => {
                    if depth <= 1 {
                        break;
                    }
                    depth -= 1;
                }
                _ => {}
            }
            j += 1;
        }
        for k in i..=j.min(b.len() - 1) {
            if out[k] != '\n' {
                out[k] = ' ';
            }
        }
        i = j + 1;
    }
    (out.into_iter().collect(), test_mods)
}

/// Lines (1-based) where `name` is called in `prod` (a `production` text).
pub fn calls(prod: &str, name: &str) -> Vec<usize> {
    let mut lines = vec![];
    for (ln, line) in prod.lines().enumerate() {
        let mut from = 0;
        while let Some(rel) = line[from..].find(name) {
            let at = from + rel;
            from = at + name.len();
            let before = line[..at].trim_end();
            let after = line[at + name.len()..].trim_start();
            let is_call_site = before.ends_with('.') || before.ends_with("::");
            if is_call_site && after.starts_with('(') {
                lines.push(ln + 1);
            }
        }
    }
    lines
}

/// The production code of every source file of `crate_dir`'s `src/`, as
/// `(path, production text)`; files declared as test-only modules are left out.
pub fn production_files(crate_dir: &Path) -> Vec<(PathBuf, String)> {
    let src = crate_dir.join("src");
    let files = rust_files(&src);
    let mut test_only: HashSet<PathBuf> = HashSet::new();
    let mut parsed = vec![];
    for f in &files {
        let text = std::fs::read_to_string(f).expect("source");
        let (prod, mods) = production(&text);
        let stem = f.file_stem().unwrap().to_string_lossy().to_string();
        let dir = f.parent().unwrap();
        let base = if ["lib", "main", "mod"].contains(&stem.as_str()) { dir.to_path_buf() } else { dir.join(&stem) };
        for m in mods {
            test_only.insert(base.join(format!("{m}.rs")));
            test_only.insert(base.join(&m));
        }
        parsed.push((f.clone(), prod));
    }
    parsed
        .into_iter()
        .filter(|(f, _)| !test_only.iter().any(|t| f == t || f.starts_with(t)))
        .collect()
}

/// Every crate directory under `crates/`, sorted.
pub fn crate_dirs() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(crates_dir())
        .expect("crates/")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("src").is_dir())
        .collect();
    out.sort();
    out
}
