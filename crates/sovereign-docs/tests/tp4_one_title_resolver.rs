//! v0.0.13, contract **C-TPII**, **TP4** (spec ch. 08, *A title that holds
//! personal data*; Céline, 2026-11-17): « The three forms are compared in the
//! one resolver of documents by title, so every caller named above (the
//! orchestrator, the chat tools, the import, the skills' bridge, the browser's
//! save) has them. »
//!
//! Two scans of the production code of every crate:
//!
//! 1. **No caller compares the stored form alone.** Outside the store layer
//!    (`sovereign-db/src`, where the first form's comparison and the blind
//!    index live) and the one resolver (`sovereign-ai/src/document_titles.rs`),
//!    nothing calls `resolve_document_title` (the stored form only) or the
//!    title lookup `document_title_candidates`. A caller that did would miss
//!    the title as typed and as shown.
//! 2. **Every caller gives the resolver the account key.** The second form
//!    needs the records of a value, which only the key opens. Each call of
//!    `resolve_document_or_explain` or `resolve_document_title_forms` outside
//!    the resolver passes, as its second argument, something other than a bare
//!    `None`.
//!
//! What they read: names and argument text, not data flow. A key held in a
//! variable that is always `None` passes the second scan; the behaviour tests
//! (`sovereign-ai/src/orchestrator_tpii_tests.rs`, the chat tools end to end)
//! are what catch that.
//!
//! Written before the code by another author; the implementer does not edit
//! this file.

mod common;

use common::scan::{crate_dirs, production_files};

/// The comparison of the stored form alone, and the store's title lookup.
const STORED_FORM_ONLY: [&str; 3] =
    ["resolve_document_title", "document_title_candidates", "document_title_candidates_by_token_hashes"];

/// The one resolver's entry points: each takes `(db, account_key, title)`.
const RESOLVERS: [&str; 2] = ["resolve_document_or_explain", "resolve_document_title_forms"];

/// Where the stored form's comparison and the title lookup are defined and
/// delegated: the store layer.
fn in_store_layer(path: &std::path::Path) -> bool {
    let p = path.to_string_lossy().replace('\\', "/");
    p.contains("/sovereign-db/src/")
}

/// The one resolver.
fn is_the_resolver(path: &std::path::Path) -> bool {
    path.to_string_lossy().replace('\\', "/").ends_with("/sovereign-ai/src/document_titles.rs")
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Byte offset of every call of `name` in `prod`: the whole identifier,
/// qualified (`a::name(`, `x.name(`) or bare (`name(`, after a `use`), followed
/// by its opening parenthesis; never its definition (`fn name(`).
fn call_sites(prod: &str, name: &str) -> Vec<usize> {
    let mut out = vec![];
    let mut from = 0;
    while let Some(rel) = prod[from..].find(name) {
        let at = from + rel;
        from = at + name.len();
        let before_ok = prod[..at].chars().next_back().map_or(true, |c| !is_ident(c));
        let after = &prod[at + name.len()..];
        let after_ok = after.chars().next().map_or(false, |c| !is_ident(c));
        let opens = after.trim_start().starts_with('(');
        let is_definition = prod[..at].trim_end().ends_with("fn");
        if before_ok && after_ok && opens && !is_definition {
            out.push(at);
        }
    }
    out
}

/// The 1-based line of byte offset `at` in `text`.
fn line_of(text: &str, at: usize) -> usize {
    text[..at].matches('\n').count() + 1
}

/// The arguments, split at the top-level commas, of the call of `name` at
/// byte offset `at` of `prod`.
fn call_args(prod: &str, at: usize, name: &str) -> Vec<String> {
    let after = at + name.len();
    let Some(open_rel) = prod[after..].find('(') else { return vec![] };
    let mut depth = 0;
    let mut args = vec![];
    let mut cur = String::new();
    for c in prod[after + open_rel..].chars() {
        match c {
            '(' | '[' | '{' => {
                depth += 1;
                if depth > 1 {
                    cur.push(c);
                }
            }
            ')' | ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    args.push(cur.trim().to_string());
                    break;
                }
                cur.push(c);
            }
            ',' if depth == 1 => {
                args.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    args.retain(|a| !a.is_empty());
    args
}

/// Every call of a resolver in `prod` whose key argument is missing or a bare
/// `None`, with its line.
fn keyless_calls(prod: &str) -> Vec<String> {
    let mut out = vec![];
    for name in RESOLVERS {
        for at in call_sites(prod, name) {
            let args = call_args(prod, at, name);
            if args.len() < 3 || args[1] == "None" {
                out.push(format!("l. {}: {name}({})", line_of(prod, at), args.join(", ")));
            }
        }
    }
    out
}

/// The detector itself, on the shapes it must catch and those it must pass.
#[test]
fn tp4_scan_detector_reads_the_key_argument() {
    let keyless = [
        "fn a(){ let d = crate::document_titles::resolve_document_or_explain(db, None, title).await; }",
        "fn a(){ let d = resolve_document_title_forms(\n    db,\n    None,\n    title,\n).await; }",
        "fn a(){ let d = crate::document_titles::resolve_document_or_explain(db, title).await; }",
    ];
    for src in keyless {
        assert_eq!(keyless_calls(src).len(), 1, "not caught: {src}");
    }
    let keyed = [
        "fn a(){ let d = crate::document_titles::resolve_document_or_explain(self.db.as_ref(), key.as_deref(), title).await; }",
        "fn a(){ let d = crate::document_titles::resolve_document_title_forms(db, Some(&k), &f(x, y)).await; }",
        "pub(crate) async fn resolve_document_or_explain(db: &dyn GraphDB, title: &str) -> R { }",
        "use crate::document_titles::resolve_document_or_explain;",
    ];
    for src in keyed {
        assert!(keyless_calls(src).is_empty(), "wrongly caught: {src}");
    }
}

/// TP4, scan 1: outside the store layer and the one resolver, no production
/// code compares a title by its stored form alone.
///
/// Green against today's code (only the resolver calls them). The mutation
/// that turns it red: the chat tool `get_document` resolving with
/// `sovereign_db::resolve_document_title(db, title)` instead of the resolver.
#[test]
fn tp4_no_caller_compares_the_stored_form_alone() {
    let mut found = vec![];
    for dir in crate_dirs() {
        for (path, prod) in production_files(&dir) {
            if in_store_layer(&path) || is_the_resolver(&path) {
                continue;
            }
            for name in STORED_FORM_ONLY {
                for at in call_sites(&prod, name) {
                    found.push(format!("{}:{}: {name}", path.display(), line_of(&prod, at)));
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "a document named by title is resolved by the one resolver, with its three forms (TP4); these compare \
         the stored form alone:\n{}",
        found.join("\n")
    );
}

/// TP4, scan 2: every caller of the resolver gives it the account key, so the
/// title as typed (the second form) is compared for every caller.
#[test]
fn tp4_every_caller_gives_the_resolver_the_key() {
    let mut found = vec![];
    for dir in crate_dirs() {
        for (path, prod) in production_files(&dir) {
            if is_the_resolver(&path) {
                continue;
            }
            for c in keyless_calls(&prod) {
                found.push(format!("{}: {c}", path.display()));
            }
        }
    }
    assert!(
        found.is_empty(),
        "these callers resolve a title without the account key, so the title as she typed it, with its \
         values, is never compared for them (TP1's second form, TP4):\n{}",
        found.join("\n")
    );
}
