//! v0.0.12 item 23, **T1** — "One resolution function, beside item 6's
//! (`sovereign-db`), used by every caller above. A scan: no production caller
//! outside that function takes a single document from
//! `search_documents_by_title`." Contract:
//! `doc/plans/V0012-item23-document-titles.md`.
//!
//! `GraphDB::search_documents_by_title` is a search: a blind-index match on
//! the title's tokens. Taking its first result as "the document named X" made
//! "restore Report" restore "Quarterly Report Draft". A search may stay a
//! search (T3: the `search` action, the `search_documents` tool, the app's
//! quick filter list what it finds); what is refused is **picking one** of its
//! results anywhere but in the resolution function, which lives in
//! `sovereign-db` (left out of the scan, as item 6's A1 scan leaves it out).
//!
//! "Picking one" is read on the call's own expression (`….await?.first()`,
//! `.into_iter().next()`, `[0]`) and, when the result is bound by `let`, on
//! that binding in the rest of its block (`docs.first()`, `found.into_iter()
//! .next()`, `docs[0]`, `.get(0)`, `.last()`, `.pop()`, `.find(..)`, …).
//! Behaviour is tested in `sovereign-ai/src/orchestrator_document_titles_tests.rs`.
//!
//! Written before the code, by another author; the implementer does not edit
//! this file.

mod common;

use common::scan::{calls, crate_dirs, production_files, rust_files};

/// The title searches.
const SEARCHES: [&str; 2] = ["search_documents_by_title", "search_documents_by_title_token_hashes"];

/// Methods that take one element of a sequence.
const PICK_ONE: [&str; 12] =
    ["first", "next", "last", "pop", "find", "find_map", "get", "nth", "remove", "swap_remove", "position", "next_back"];

/// Test-only files declared in a form the shared scan does not read (as in
/// `a1_one_thread_resolution.rs`).
fn more_test_only_files(crate_dir: &std::path::Path) -> Vec<std::path::PathBuf> {
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
                    let base = if ["lib", "main", "mod"].contains(&stem.as_str()) { dir.to_path_buf() } else { dir.join(&stem) };
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

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Byte offset of the start of line `line` (1-based) in `text`.
fn line_start(text: &str, line: usize) -> usize {
    text.split_inclusive('\n').take(line - 1).map(str::len).sum()
}

/// From `at` (just after the call's name), the call's expression up to the end
/// of its statement: a `;`, or a `{` opening a block (a `match`/`if let`
/// scrutinee), at depth 0, or the close of the enclosing bracket.
fn statement_tail(text: &str, at: usize) -> (&str, usize) {
    let mut depth: i32 = 0;
    for (k, c) in text[at..].char_indices() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => {
                depth -= 1;
                if depth < 0 {
                    return (&text[at..at + k], at + k);
                }
            }
            '{' if depth == 0 => return (&text[at..at + k], at + k),
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth < 0 {
                    return (&text[at..at + k], at + k);
                }
            }
            ';' if depth == 0 => return (&text[at..at + k], at + k + 1),
            _ => {}
        }
    }
    (&text[at..], text.len())
}

/// Whether `expr` takes one element: `.first(`, `.into_iter().next(`, `[…]`
/// after a call or `?`, … A method name is matched whole, followed by `(`.
fn picks_one(expr: &str) -> bool {
    let b: Vec<char> = expr.chars().collect();
    for (i, c) in b.iter().enumerate() {
        if *c == '.' {
            let mut j = i + 1;
            while j < b.len() && b[j].is_whitespace() {
                j += 1;
            }
            let start = j;
            while j < b.len() && is_ident(b[j]) {
                j += 1;
            }
            let name: String = b[start..j].iter().collect();
            while j < b.len() && b[j].is_whitespace() {
                j += 1;
            }
            let turbofish = j + 1 < b.len() && b[j] == ':' && b[j + 1] == ':';
            if PICK_ONE.contains(&name.as_str()) && j < b.len() && (b[j] == '(' || turbofish) {
                return true;
            }
        }
        if *c == '[' {
            // Indexing: `[` right after an expression (`)`, `?`, an identifier).
            let prev = b[..i].iter().rev().find(|c| !c.is_whitespace());
            if matches!(prev, Some(')') | Some('?')) || prev.is_some_and(|p| is_ident(*p)) {
                return true;
            }
        }
    }
    false
}

/// The name bound by the `let` whose statement contains the call at `call_at`
/// (`let x =`, `let mut x =`, `let x: T =`), if any.
fn bound_name(text: &str, call_at: usize) -> Option<String> {
    let head = &text[..call_at];
    let stmt_start = head.rfind(|c| c == ';' || c == '{' || c == '}').map_or(0, |k| k + 1);
    let stmt = head[stmt_start..].trim_start();
    let rest = stmt.strip_prefix("let")?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let rest = rest.trim_start();
    let rest = rest.strip_prefix("mut ").map(str::trim_start).unwrap_or(rest);
    let name: String = rest.chars().take_while(|c| is_ident(*c)).collect();
    if name.is_empty() || name == "_" {
        None
    } else {
        Some(name)
    }
}

/// The rest of the block from `from`: up to the `}` that closes the block the
/// statement is in.
fn rest_of_block(text: &str, from: usize) -> &str {
    let mut depth: i32 = 0;
    for (k, c) in text[from..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth < 0 {
                    return &text[from..from + k];
                }
            }
            _ => {}
        }
    }
    &text[from..]
}

/// Every use of `name` (whole word) in `block` that takes one element.
fn picks_from_binding(block: &str, name: &str) -> bool {
    let mut from = 0;
    while let Some(rel) = block[from..].find(name) {
        let at = from + rel;
        from = at + name.len();
        let before_ok = block[..at].chars().next_back().map_or(true, |c| !is_ident(c) && c != '.');
        let after = &block[at + name.len()..];
        let after_ok = after.chars().next().map_or(true, |c| !is_ident(c));
        if !(before_ok && after_ok) {
            continue;
        }
        // The chain on the binding, up to the end of its expression.
        let (tail, _) = statement_tail(block, at + name.len());
        let chain: String = tail.chars().take_while(|c| !matches!(c, ',' | '=' | '|')).collect();
        let chain = chain.trim_start();
        if chain.starts_with('[') || (chain.starts_with('.') && picks_one(chain)) {
            return true;
        }
    }
    false
}

/// `(path:line)` of every call of a title search, in `prod`, whose result is
/// picked one element of.
fn single_picks(prod: &str) -> Vec<usize> {
    let mut found = vec![];
    for name in SEARCHES {
        for line in calls(prod, name) {
            let ls = line_start(prod, line);
            let Some(rel) = prod[ls..].find(name) else { continue };
            let call_at = ls + rel;
            let (tail, end) = statement_tail(prod, call_at + name.len());
            // The call's own parentheses are part of the tail: skip them.
            let after_args = {
                let mut depth = 0;
                let mut cut = tail.len();
                for (k, c) in tail.char_indices() {
                    match c {
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth == 0 {
                                cut = k + 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                &tail[cut.min(tail.len())..]
            };
            let picked = picks_one(after_args)
                || bound_name(prod, call_at).is_some_and(|b| picks_from_binding(rest_of_block(prod, end), &b));
            if picked {
                found.push(line);
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

/// The detector itself, on the shapes today's code takes (and on the shapes
/// a search keeps): it catches the first and passes the second.
#[test]
fn t1_scan_detector_catches_the_shapes_of_a_single_pick() {
    let picked = [
        "fn a(){ let found = db.search_documents_by_title(&t).await.map_err(f)?; let Some(doc) = found.into_iter().next() else { return }; }",
        "fn a(){ let found = match target { Some(t) => self.db.search_documents_by_title(t).await?.into_iter().next(), None => None }; }",
        "fn a(){ let docs = self.db.search_documents_by_title(target).await?; match docs.first().and_then(|d| d.id_string()) { _ => {} } }",
        "fn a(){ let docs = db.search_documents_by_title(title).await.unwrap_or_default(); if let Some(doc) = docs.first() { } }",
        "fn a(){ let mut docs = db.search_documents_by_title(title).await?; let d = docs.pop(); }",
        "fn a(){ let docs = db.search_documents_by_title(title).await?; let d = &docs[0]; }",
        "fn a(){ let docs: Vec<Document> = db.search_documents_by_title(title).await?; let d = docs.iter().find(|d| d.title == title); }",
        "fn a(){ let d = db.search_documents_by_title_token_hashes(&h).await?.first().cloned(); }",
    ];
    for src in picked {
        assert_eq!(single_picks(src).len(), 1, "not caught: {src}");
    }
    let searches = [
        "fn a(){ let docs = self.db.search_documents_by_title(search_term).await?; let ids: Vec<String> = docs.iter().filter_map(|d| d.id_string()).collect(); let titles: Vec<String> = docs.iter().take(5).map(|d| d.title.clone()).collect(); }",
        "fn a(){ let docs = db.search_documents_by_title(query).await.unwrap_or_default(); let m: Vec<String> = docs.iter().take(8).map(|d| d.title.clone()).collect(); }",
        "fn a(){ let docs = state.db.search_documents_by_title(&query).await.str_err()?; let r: Vec<Hit> = docs.into_iter().take(50).map(hit).collect(); }",
        "fn a(){ let candidates = db.search_documents_by_title(t).await?; for d in candidates { out.push(d); } let first = other.first(); }",
    ];
    for src in searches {
        assert!(single_picks(src).is_empty(), "a search flagged as a pick: {src}");
    }
}

/// T1: no production code of any crate except `sovereign-db` (where the one
/// resolution function lives) takes a single document from a title search.
/// Searches that list what they find (T3) are allowed.
#[test]
fn t1_no_production_caller_takes_a_single_document_from_a_title_search() {
    let dirs = crate_dirs();
    assert!(dirs.len() > 10, "only {} crates found", dirs.len());
    let mut found = vec![];
    let mut searches = 0;
    for dir in dirs {
        if dir.file_name().is_some_and(|n| n == "sovereign-db") {
            continue;
        }
        let test_only = more_test_only_files(&dir);
        for (file, prod) in production_files(&dir) {
            if test_only.iter().any(|t| *t == file) {
                continue;
            }
            searches += SEARCHES.iter().map(|n| calls(&prod, n).len()).sum::<usize>();
            let rel = file.strip_prefix(dir.parent().unwrap()).unwrap_or(&file).to_string_lossy().replace('\\', "/");
            for line in single_picks(&prod) {
                found.push(format!("{rel}:{line}"));
            }
        }
    }
    assert!(searches >= 2, "the scan sees the searches that stay searches (T3); it saw {searches}");
    assert!(
        found.is_empty(),
        "a single document taken from a title search (T1: one exact-title resolution, in sovereign-db):\n{}",
        found.join("\n")
    );
}
