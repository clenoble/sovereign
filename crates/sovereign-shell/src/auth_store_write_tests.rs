//! rc2 — the desktop's first writes of `auth.store` are exclusive too (spec
//! ch. 07, *Auth store*, "Created once, never replaced by account creation",
//! v2.0.46 as extended by Céline, `7506579`): "**Every** first write of
//! `auth.store` refuses an existing file (an exclusive create, not a replace):
//! account creation fresh or paired, on the phone and on the desktop, …".
//!
//! Written before the code, by another author: whoever implements rc2 makes
//! these pass without editing this file.
//!
//! The two desktop sites — `crypto.rs` `create_auth_store` (the wizard's fresh
//! account) and `p2p.rs` `accept_pairing` (the paired account with an imported
//! account key) — touch the real profile, so they are checked by reading their
//! source: `auth.store` is written with `AuthStore::save_new(`, never with
//! `.save(`. No claim is required there: the desktop wizard creates the store
//! on its window's own thread (spec ch. 12, *While the account is being
//! created*). `save_new` itself is tested in
//! `sovereign-crypto/tests/auth_store_save_new.rs`.

use crate::field_contrast_tests::{blank_line_comments, fn_body, read};

/// The text between the `(` at `open_paren` and its matching `)`.
fn call_args(body: &str, open_paren: usize) -> &str {
    let b = body.as_bytes();
    let (mut depth, mut i, mut in_str) = (0usize, open_paren, false);
    while i < b.len() {
        match b[i] {
            b'\\' if in_str => i += 1,
            b'"' => in_str = !in_str,
            b'(' if !in_str => depth += 1,
            b')' if !in_str => {
                depth -= 1;
                if depth == 0 {
                    return &body[open_paren + 1..i];
                }
            }
            _ => {}
        }
        i += 1;
    }
    &body[open_paren + 1..]
}

/// The identifier just before `.method(` at `dot` (a method chain may put the
/// receiver on the line above).
fn receiver_before(body: &str, dot: usize) -> &str {
    let before = body[..dot].trim_end();
    let start = before
        .rfind(|c: char| !(c.is_alphanumeric() || c == '_'))
        .map(|p| p + 1)
        .unwrap_or(0);
    &before[start..]
}

/// The names an `AuthStore` is bound to: every `let <name> = …AuthStore::create…`.
fn store_bindings(body: &str) -> Vec<String> {
    let mut names = Vec::new();
    for (p, _) in body.match_indices("AuthStore::create") {
        let stmt_start = body[..p].rfind(|c| c == ';' || c == '{' || c == '}').map(|q| q + 1).unwrap_or(0);
        if let Some(rest) = body[stmt_start..p].trim().strip_prefix("let ") {
            let name = rest
                .trim_start_matches("mut ")
                .split(|c: char| c == '=' || c == ':' || c.is_whitespace())
                .next()
                .unwrap_or("");
            if !name.is_empty() && name != "_" {
                names.push(name.to_string());
            }
        }
    }
    names
}

fn writes_auth_store(args: &str, recv: &str, stores: &[String]) -> bool {
    args.contains("auth_store_path") || args.contains("auth.store") || stores.iter().any(|s| s == recv)
}

fn check_first_write(file: &str, name: &str) -> Vec<String> {
    let code = blank_line_comments(&read(file));
    let body = fn_body(&code, name, file);
    let stores = store_bindings(&body);
    let mut problems = Vec::new();
    if stores.is_empty() {
        problems.push(format!("{file} `{name}`: no `let … = AuthStore::create…` found — the scan cannot see the store"));
    }
    for (p, _) in body.match_indices(".save(") {
        let args = call_args(&body, p + ".save".len());
        let recv = receiver_before(&body, p);
        if writes_auth_store(args, recv, &stores) {
            problems.push(format!(
                "{file} `{name}`: `{recv}.save({})` replaces auth.store — the first write must be `.save_new(` (exclusive create)",
                args.trim()
            ));
        }
    }
    let exclusive = body.match_indices(".save_new(").any(|(p, _)| {
        let args = call_args(&body, p + ".save_new".len());
        writes_auth_store(args, receiver_before(&body, p), &stores)
    });
    if !exclusive {
        problems.push(format!(
            "{file} `{name}`: auth.store is not written with `.save_new(` (on the AuthStore, or with `auth_store_path()`)"
        ));
    }
    problems
}

#[test]
fn the_desktop_wizard_writes_auth_store_exclusively() {
    let problems = check_first_write("src/crypto.rs", "create_auth_store");
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}

#[test]
fn the_desktop_paired_onboarding_writes_auth_store_exclusively() {
    let problems = check_first_write("src/p2p.rs", "accept_pairing");
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}

/// The scanner on known bodies, so a green result means something.
#[test]
fn the_scanner_tells_a_replace_from_an_exclusive_create() {
    let scan = |src: &str| {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.rs");
        std::fs::write(&path, src).unwrap();
        // CARGO_MANIFEST_DIR.join(absolute) == absolute.
        check_first_write(path.to_str().unwrap(), "f")
    };
    let bad = "fn f() -> R { let store = AuthStore::create(a, b, &s, &d)?;\n    store.save(&auth_store_path())?; Ok(store) }";
    let bad_chain = "fn f() -> R { let auth_store = AuthStore::create_with_imported_account_key(a, b, &s, &d, &k)\n    .map_err(|e| format!(\"x {e}\"))?;\n    auth_store\n        .save(&crate::crypto::auth_store_path())\n        .map_err(|e| e.to_string())?; }";
    let good = "fn f() -> R { let store = AuthStore::create(a, b, &s, &d)?;\n    store.save_new(&auth_store_path())?; Ok(store) }";
    for (label, src) in [("bad", bad), ("bad_chain", bad_chain)] {
        let p = scan(src);
        assert!(p.iter().any(|m| m.contains("replaces auth.store")), "{label}: {p:?}");
        assert!(p.iter().any(|m| m.contains("not written with")), "{label}: {p:?}");
    }
    let p = scan(good);
    assert!(p.is_empty(), "good: {p:?}");
}
